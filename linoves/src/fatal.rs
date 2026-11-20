//! Аварийный экран. Если в ядре случилось что-то непоправимое (паника Rust
//! или исключение процессора), система не перезагружается молча и не
//! продолжает работать с испорченной памятью: она останавливается, выводит
//! подробный отчёт на экран и в COM1 и ждёт нажатия клавиши для перезапуска.

use core::fmt::Write;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::console::{Console, CONSOLE};
use crate::interrupts::{InterruptFrame, EXCEPTION_NAMES};
use crate::vga::Color;
use crate::{boot, cpu, gfx, keyboard, power, selftest, serial};

static IN_FATAL: AtomicBool = AtomicBool::new(false);

pub fn exception(frame: &InterruptFrame) -> ! {
    let vector = frame.vector as usize;
    let name = EXCEPTION_NAMES.get(vector).copied().unwrap_or("Unknown");
    let cr2 = cpu::read_cr2();
    let guard = boot::stack_guard_page();
    let stack_overflow = (vector == 14 || vector == 8) && cr2 >= guard && cr2 < guard + 4096;
    let title = if stack_overflow { "KERNEL STACK OVERFLOW" } else { "CPU EXCEPTION" };

    report(title, |w| {
        let _ = writeln!(w, "  Exception : {} (vector {})", name, vector);
        let _ = writeln!(w, "  Error code: {:#x}", frame.error_code);
        if vector == 14 {
            let e = frame.error_code;
            let _ = writeln!(
                w,
                "  Address   : {:#018x} ({}, {} access{})",
                cr2,
                if e & 1 != 0 { "protection violation" } else { "page not present" },
                if e & 2 != 0 { "write" } else if e & 16 != 0 { "execute" } else { "read" },
                if cr2 < 0x1000 { ", NULL pointer" } else { "" }
            );
        }
        if stack_overflow {
            let _ = writeln!(w, "  The kernel stack ran into its guard page (too deep recursion).");
        }
        let _ = writeln!(w);
        let _ = writeln!(w, "  RIP={:016x} CS={:04x}   RFLAGS={:016x}", frame.rip, frame.cs, frame.rflags);
        let _ = writeln!(w, "  RSP={:016x} SS={:04x}", frame.rsp, frame.ss);
        let _ = writeln!(w, "  RAX={:016x} RBX={:016x} RCX={:016x}", frame.rax, frame.rbx, frame.rcx);
        let _ = writeln!(w, "  RDX={:016x} RSI={:016x} RDI={:016x}", frame.rdx, frame.rsi, frame.rdi);
        let _ = writeln!(w, "  RBP={:016x} R8 ={:016x} R9 ={:016x}", frame.rbp, frame.r8, frame.r9);
        let _ = writeln!(w, "  R10={:016x} R11={:016x} R12={:016x}", frame.r10, frame.r11, frame.r12);
        let _ = writeln!(w, "  R13={:016x} R14={:016x} R15={:016x}", frame.r13, frame.r14, frame.r15);
        let _ = writeln!(
            w,
            "  CR0={:016x} CR2={:016x} CR3={:016x}",
            cpu::read_cr0(),
            cr2,
            cpu::read_cr3()
        );
    })
}

pub fn report(title: &str, body: impl FnOnce(&mut Console)) -> ! {
    cpu::disable_interrupts();
    if IN_FATAL.swap(true, Ordering::SeqCst) {
        serial::write_str("\n*** Linoves: nested fatal error, system halted ***\n");
        cpu::halt_forever();
    }

    // Система остановлена, других владельцев консоли больше нет.
    unsafe { CONSOLE.force_unlock() };
    {
        let mut c = CONSOLE.lock();
        c.set_color(Color::White, Color::Red);
        c.clear();
        let _ = writeln!(c);
        let _ = writeln!(c, "  Linoves has stopped: {}", title);
        let _ = writeln!(c, "  ----------------------------------------------------------------------");
        body(&mut c);
        let _ = writeln!(c);
        let _ = writeln!(c, "  The kernel halted itself to protect your data from corruption.");
        let _ = writeln!(c, "  Press any key to restart the computer.");
        c.set_color(Color::White, Color::Red);
        if c.is_gui() {
            if let Some(fb) = gfx::info() {
                gfx::draw_grid_direct(&fb, &c.grid);
            }
        }
    }

    if selftest::is_test_mode() {
        power::qemu_exit(false);
    }

    // Сбросить уже нажатые клавиши, затем ждать новую.
    for _ in 0..10_000 {
        let _ = keyboard::poll_key_pressed();
        let _ = serial::try_read_byte();
    }
    loop {
        if keyboard::poll_key_pressed() || serial::try_read_byte().is_some() {
            break;
        }
        core::hint::spin_loop();
    }
    power::reboot();
}
