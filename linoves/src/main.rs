//! Linoves — небольшое 64-битное ядро на Rust.
//!
//! Порядок загрузки:
//! 1. `boot.rs`: загрузчик (QEMU/GRUB, Multiboot) передаёт управление в
//!    32-битном режиме; заглушка включает страничную память и long mode.
//! 2. `kernel_main`: консоль, GDT/TSS, прерывания, таймер, клавиатура, куча.
//! 3. Командная оболочка `shell`.

#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
mod console;

mod boot;
mod calc;
mod cpu;
mod desktop;
mod fatal;
mod fs;
mod gdt;
mod gfx;
mod heap;
mod input;
mod interrupts;
mod keyboard;
mod mouse;
mod multiboot;
mod pci;
mod pic;
mod power;
mod rtc;
mod selftest;
mod serial;
mod shell;
mod sync;
mod time;
mod vga;

use core::panic::PanicInfo;

use vga::Color;

/// Версия системы (берётся из Cargo.toml): «1.0», «1.1», ...
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION_MAJOR"), ".", env!("CARGO_PKG_VERSION_MINOR"));
/// Кодовое имя выпуска: C1, C2, C3, ... (увеличивается с каждым обновлением).
pub const CODENAME: &str = "C4";

fn ok(msg: core::fmt::Arguments) {
    cprint!(Color::LightGreen, "  [ OK ] ");
    println!("{}", msg);
}

fn warn(msg: core::fmt::Arguments) {
    cprint!(Color::Yellow, "  [WARN] ");
    println!("{}", msg);
}

struct Buf<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl core::fmt::Write for Buf<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let n = s.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

fn banner() {
    cprintln!(Color::LightCyan, "  ╔════════════════════════════════════════════════════════╗");
    let mut title = [0u8; 56];
    let mut w = Buf { buf: &mut title, len: 0 };
    let _ = core::fmt::Write::write_fmt(&mut w, format_args!("   Linoves {} ({})  -  64-bit kernel written in Rust", VERSION, CODENAME));
    let len = w.len;
    let text = core::str::from_utf8(&title[..len]).unwrap_or("Linoves");
    cprintln!(Color::LightCyan, "  ║{:<56}║", text);
    cprintln!(Color::LightCyan, "  ╚════════════════════════════════════════════════════════╝");
    println!();
}

#[no_mangle]
pub extern "C" fn kernel_main(magic: u32, mbi: u32) -> ! {
    serial::init();
    console::init();
    banner();

    if multiboot::parse(magic, mbi) {
        ok(format_args!("Multiboot information received"));
    } else {
        warn(format_args!("No valid Multiboot information (magic {:#x})", magic));
    }

    gdt::init();
    ok(format_args!("GDT + TSS loaded (separate stacks for double fault and NMI)"));

    interrupts::init();
    ok(format_args!("IDT loaded: all 256 vectors have handlers"));

    let has_mouse = mouse::init();
    keyboard::init();
    let mut irqs = [0u8; 4];
    let mut n = 0;
    for (irq, on) in [
        (pic::IRQ_TIMER, true),
        (pic::IRQ_KEYBOARD, true),
        (pic::IRQ_COM1, serial::is_present()),
        (pic::IRQ_MOUSE, has_mouse),
    ] {
        if on {
            irqs[n] = irq;
            n += 1;
        }
    }
    pic::init(&irqs[..n]);
    time::init();
    serial::enable_rx_interrupt();
    unsafe { cpu::enable_interrupts() };
    ok(format_args!(
        "Interrupts on: timer {} Hz, PS/2 keyboard{}{}",
        time::HZ,
        if has_mouse { ", PS/2 mouse" } else { "" },
        if serial::is_present() { ", serial COM1" } else { "" }
    ));

    match heap::init_from_boot_info() {
        Ok(s) => ok(format_args!(
            "Kernel heap: {} MiB at {:#x}",
            s.size / (1024 * 1024),
            s.start
        )),
        Err(e) => panic!("cannot initialise memory: {}", e),
    }

    fs::init();
    ok(format_args!("RAM disk mounted"));

    if multiboot::cmdline_has("selftest") {
        println!();
        selftest::run_and_exit();
    }

    if multiboot::cmdline_has("text") {
        ok(format_args!("Text mode requested ('text' boot option)"));
    } else {
        match gfx::init() {
            Some(fb) => {
                ok(format_args!(
                    "Graphics {}x{}x{} ({}, {}), starting the desktop",
                    fb.width, fb.height, fb.bpp, fb.source, fb.layout()
                ));
                desktop::run();
            }
            None => warn(format_args!("No graphics adapter found, staying in text mode")),
        }
    }

    shell::run();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    fatal::report("KERNEL PANIC", |w| {
        use core::fmt::Write;
        let _ = writeln!(w, "  {}", info);
    })
}
