//! Командная оболочка с редактированием строки, историей и Tab-дополнением.

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;

use crate::input::{self, Key};
use crate::vga::Color;
use crate::{
    boot, calc, console, cpu, fs, gdt, heap, interrupts, multiboot, power, selftest, time,
};

const MAX_LINE: usize = 200;
const MAX_HISTORY: usize = 50;
const PROMPT: &str = "rustos> ";

const COMMANDS: &[(&str, &str)] = &[
    ("help", "show this list"),
    ("clear", "clear the screen (also Ctrl+L)"),
    ("echo", "echo <text>: print text"),
    ("about", "information about the system"),
    ("version", "show the system version and codename"),
    ("uptime", "time since boot"),
    ("mem", "kernel heap usage"),
    ("memmap", "physical memory map from the bootloader"),
    ("cpu", "processor information"),
    ("ls", "list files on the RAM disk"),
    ("cat", "cat <file>: show a file"),
    ("write", "write <file> <text>: create/replace a file"),
    ("append", "append <file> <text>: add a line to a file"),
    ("rm", "rm <file>: delete a file"),
    ("calc", "calc <expr>: integer calculator, e.g. calc (2+3)*4"),
    ("color", "color <name>: change text color"),
    ("sleep", "sleep <seconds>: wait (Ctrl+C stops)"),
    ("history", "show command history"),
    ("selftest", "run the kernel self-tests"),
    ("int3", "raise a breakpoint exception and recover from it"),
    ("crash", "crash <panic|null|stack|div0>: test the fatal error screen"),
    ("reboot", "restart the computer"),
    ("shutdown", "power off the computer"),
];

pub struct Shell {
    history: VecDeque<String>,
    color: Color,
}

pub fn run() -> ! {
    let mut shell = Shell { history: VecDeque::new(), color: Color::LightGray };
    println!();
    println!("Type 'help' for a list of commands.");
    loop {
        shell.prompt();
        let line = shell.read_line();
        shell.execute(line.trim());
    }
}

fn split_word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(' ') {
        Some(i) => (&s[..i], s[i + 1..].trim_start()),
        None => (s, ""),
    }
}

fn backspaces(n: usize) {
    for _ in 0..n {
        print!("\x08");
    }
}

fn print_chars(chars: &[char]) {
    let s: String = chars.iter().collect();
    print!("{}", s);
}

impl Shell {
    fn prompt(&self) {
        cprint!(Color::LightGreen, "{}", PROMPT);
        console::set_color(self.color);
    }

    /// Перерисовать хвост строки после изменения в середине.
    fn redraw_tail(tail: &[char], erased: usize) {
        print_chars(tail);
        for _ in 0..erased {
            print!(" ");
        }
        backspaces(tail.len() + erased);
    }

    fn replace_line(buf: &mut Vec<char>, cur: &mut usize, new: &str) {
        backspaces(*cur);
        let old_len = buf.len();
        buf.clear();
        buf.extend(new.chars().take(MAX_LINE));
        print_chars(buf);
        let extra = old_len.saturating_sub(buf.len());
        for _ in 0..extra {
            print!(" ");
        }
        backspaces(extra);
        *cur = buf.len();
    }

    fn complete(buf: &mut Vec<char>, cur: &mut usize) {
        if *cur != buf.len() || buf.contains(&' ') {
            return;
        }
        let prefix: String = buf.iter().collect();
        let matches: Vec<&str> =
            COMMANDS.iter().map(|(n, _)| *n).filter(|n| n.starts_with(prefix.as_str())).collect();
        match matches.len() {
            0 => {}
            1 => {
                let rest = &matches[0][prefix.len()..];
                for c in rest.chars().chain(core::iter::once(' ')) {
                    buf.push(c);
                    print!("{}", c);
                }
                *cur = buf.len();
            }
            _ => {
                println!();
                for m in &matches {
                    print!("{}  ", m);
                }
                println!();
                cprint!(Color::LightGreen, "{}", PROMPT);
                print_chars(buf);
            }
        }
    }

    fn read_line(&mut self) -> String {
        let mut buf: Vec<char> = Vec::new();
        let mut cur = 0usize;
        let mut hist_pos = self.history.len();
        let mut draft = String::new();

        loop {
            match input::read_key() {
                Key::Char(c) => {
                    if buf.len() < MAX_LINE {
                        buf.insert(cur, c);
                        print!("{}", c);
                        cur += 1;
                        if cur < buf.len() {
                            Self::redraw_tail(&buf[cur..], 0);
                        }
                    }
                }
                Key::Backspace => {
                    if cur > 0 {
                        cur -= 1;
                        buf.remove(cur);
                        print!("\x08");
                        Self::redraw_tail(&buf[cur..], 1);
                    }
                }
                Key::Delete => {
                    if cur < buf.len() {
                        buf.remove(cur);
                        Self::redraw_tail(&buf[cur..], 1);
                    }
                }
                Key::Left => {
                    if cur > 0 {
                        cur -= 1;
                        print!("\x08");
                    }
                }
                Key::Right => {
                    if cur < buf.len() {
                        print!("{}", buf[cur]);
                        cur += 1;
                    }
                }
                Key::Home => {
                    backspaces(cur);
                    cur = 0;
                }
                Key::End => {
                    print_chars(&buf[cur..]);
                    cur = buf.len();
                }
                Key::Up => {
                    if hist_pos > 0 {
                        if hist_pos == self.history.len() {
                            draft = buf.iter().collect();
                        }
                        hist_pos -= 1;
                        let entry = self.history[hist_pos].clone();
                        Self::replace_line(&mut buf, &mut cur, &entry);
                    }
                }
                Key::Down => {
                    if hist_pos < self.history.len() {
                        hist_pos += 1;
                        let entry = if hist_pos == self.history.len() {
                            draft.clone()
                        } else {
                            self.history[hist_pos].clone()
                        };
                        Self::replace_line(&mut buf, &mut cur, &entry);
                    }
                }
                Key::Tab => Self::complete(&mut buf, &mut cur),
                Key::Enter => {
                    println!();
                    return buf.iter().collect();
                }
                Key::CtrlC => {
                    println!("^C");
                    return String::new();
                }
                Key::CtrlL => {
                    console::clear();
                    self.prompt();
                    print_chars(&buf);
                    cur = buf.len();
                }
                Key::Escape | Key::PageUp | Key::PageDown => {}
            }
        }
    }

    fn execute(&mut self, line: &str) {
        if line.is_empty() {
            return;
        }
        if self.history.back().map(|s| s.as_str()) != Some(line) {
            if self.history.len() >= MAX_HISTORY {
                self.history.pop_front();
            }
            self.history.push_back(String::from(line));
        }

        let (cmd, args) = split_word(line);
        match cmd {
            "help" => cmd_help(),
            "clear" | "cls" => console::clear(),
            "echo" => println!("{}", args),
            "about" => cmd_about(),
            "version" | "ver" => println!("RustOS {} ({})", crate::VERSION, crate::CODENAME),
            "uptime" => cmd_uptime(),
            "mem" | "free" => cmd_mem(),
            "memmap" => cmd_memmap(),
            "cpu" => cmd_cpu(),
            "ls" | "dir" => cmd_ls(),
            "cat" | "type" => cmd_cat(args),
            "write" => cmd_write(args, false),
            "append" => cmd_write(args, true),
            "rm" | "del" => cmd_rm(args),
            "calc" => cmd_calc(args),
            "color" => self.cmd_color(args),
            "sleep" => cmd_sleep(args),
            "history" => {
                for (i, h) in self.history.iter().enumerate() {
                    println!("{:4}  {}", i + 1, h);
                }
            }
            "selftest" => {
                selftest::run();
            }
            "int3" => {
                let before = interrupts::breakpoint_count();
                cpu::breakpoint();
                let after = interrupts::breakpoint_count();
                cprintln!(
                    Color::LightGreen,
                    "Breakpoint exception handled ({} -> {}), the kernel keeps running.",
                    before,
                    after
                );
            }
            "crash" => cmd_crash(args),
            "reboot" | "restart" => {
                println!("Rebooting...");
                power::reboot();
            }
            "shutdown" | "poweroff" | "halt" => {
                println!("Shutting down...");
                power::shutdown();
            }
            _ => {
                cprintln!(Color::LightRed, "Unknown command: {}", cmd);
                println!("Type 'help' for a list of commands.");
            }
        }
    }

    fn cmd_color(&mut self, args: &str) {
        match Color::from_name(args.trim()) {
            Some(Color::Black) => cprintln!(Color::LightRed, "Black text on black would be invisible."),
            Some(c) => {
                self.color = c;
                console::set_color(c);
            }
            None => {
                print!("Colors:");
                for (name, _) in Color::ALL.iter().skip(1) {
                    print!(" {}", name);
                }
                println!();
            }
        }
    }
}

fn cmd_help() {
    cprintln!(Color::Yellow, "Available commands:");
    for (name, desc) in COMMANDS {
        cprint!(Color::LightCyan, "  {:<10}", name);
        println!("{}", desc);
    }
    println!("Keys: arrows/Home/End edit the line, Up/Down history, Tab completes.");
}

fn fmt_kib(bytes: u64) -> (u64, &'static str) {
    if bytes >= 10 * 1024 * 1024 {
        (bytes / (1024 * 1024), "MiB")
    } else {
        (bytes / 1024, "KiB")
    }
}

fn cmd_about() {
    cprintln!(Color::LightCyan, "RustOS {} ({}) x86_64", crate::VERSION, crate::CODENAME);
    println!("  Build      : {}", if cfg!(debug_assertions) { "debug" } else { "release" });
    multiboot::with(|i| {
        let loader = if i.loader_name().is_empty() { "unknown" } else { i.loader_name() };
        println!("  Bootloader : {}", loader);
        println!("  Cmdline    : {}", i.cmdline());
        let (v, u) = fmt_kib(i.usable_bytes());
        println!("  Usable RAM : {} {}", v, u);
    });
    let (ks, ke) = boot::kernel_bounds();
    println!("  Kernel     : {:#x} - {:#x} ({} KiB)", ks, ke, (ke - ks) / 1024);
    let (sb, st) = boot::stack_bounds();
    println!("  Stack      : {:#x} - {:#x}, guard page at {:#x}", sb, st, boot::stack_guard_page());
    let ist = gdt::ist_stack_ranges();
    println!("  Fault stack: {:#x} (double fault), {:#x} (NMI/MCE)", ist[0].1, ist[1].1);
    println!("  Protection : NULL page unmapped, stack guard, IST stacks, 256 IDT vectors,");
    println!("               checked heap frees, overflow checks, deadlock detection");
}

fn cmd_uptime() {
    let ms = time::uptime_ms();
    let s = ms / 1000;
    println!(
        "Up {}h {:02}m {:02}.{:02}s  (timer ticks: {}, spurious IRQs: {}, dropped keys: {})",
        s / 3600,
        (s / 60) % 60,
        s % 60,
        (ms % 1000) / 10,
        time::ticks(),
        interrupts::spurious_count(),
        input::dropped()
    );
}

fn cmd_mem() {
    let s = heap::stats();
    let pct = if s.size > 0 { s.used as u64 * 100 / s.size as u64 } else { 0 };
    let (sz, su) = fmt_kib(s.size as u64);
    println!("Kernel heap at {:#x}, size {} {}", s.start, sz, su);
    println!("  used    : {} KiB ({}%)", s.used / 1024, pct);
    println!("  free    : {} KiB in {} block(s), largest {} KiB", s.free / 1024, s.free_blocks, s.largest_free / 1024);
    println!("  allocs  : {}, frees: {}, failed: {}", s.allocations, s.frees, s.failures);
    let (files, bytes) = fs::usage();
    println!("RAM disk : {} file(s), {} bytes", files, bytes);
}

fn cmd_memmap() {
    multiboot::with(|i| {
        if i.regions().is_empty() {
            println!("The bootloader did not provide a memory map.");
            if let (Some(lo), Some(hi)) = (i.mem_lower_kb, i.mem_upper_kb) {
                println!("Lower memory: {} KiB, upper memory: {} KiB", lo, hi);
            }
            return;
        }
        println!("{:<20} {:<20} {}", "Start", "End", "Type");
        for r in i.regions() {
            println!(
                "{:#018x}   {:#018x}   {} ({} KiB)",
                r.base,
                r.base.saturating_add(r.len),
                r.kind_name(),
                r.len / 1024
            );
        }
    });
}

fn cmd_cpu() {
    let r = cpu::cpuid(0, 0);
    let mut vendor = [0u8; 12];
    vendor[0..4].copy_from_slice(&r.ebx.to_le_bytes());
    vendor[4..8].copy_from_slice(&r.edx.to_le_bytes());
    vendor[8..12].copy_from_slice(&r.ecx.to_le_bytes());
    println!("Vendor   : {}", core::str::from_utf8(&vendor).unwrap_or("?"));

    let max_ext = cpu::cpuid(0x8000_0000, 0).eax;
    if max_ext >= 0x8000_0004 {
        let mut brand = [0u8; 48];
        for (i, leaf) in (0x8000_0002u32..=0x8000_0004).enumerate() {
            let r = cpu::cpuid(leaf, 0);
            for (j, v) in [r.eax, r.ebx, r.ecx, r.edx].iter().enumerate() {
                let o = i * 16 + j * 4;
                brand[o..o + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        let s = core::str::from_utf8(&brand).unwrap_or("?");
        println!("Model    : {}", s.trim_matches(|c: char| c == '\0' || c == ' '));
    }

    let f = cpu::cpuid(1, 0);
    let ext = cpu::cpuid(0x8000_0001, 0);
    let features: [(&str, bool); 12] = [
        ("fpu", f.edx & 1 != 0),
        ("tsc", f.edx & (1 << 4) != 0),
        ("pae", f.edx & (1 << 6) != 0),
        ("apic", f.edx & (1 << 9) != 0),
        ("sse", f.edx & (1 << 25) != 0),
        ("sse2", f.edx & (1 << 26) != 0),
        ("sse3", f.ecx & 1 != 0),
        ("sse4.2", f.ecx & (1 << 20) != 0),
        ("avx", f.ecx & (1 << 28) != 0),
        ("nx", ext.edx & (1 << 20) != 0),
        ("lm", ext.edx & (1 << 29) != 0),
        ("hypervisor", f.ecx & (1 << 31) != 0),
    ];
    print!("Features :");
    for (name, on) in features.iter() {
        if *on {
            print!(" {}", name);
        }
    }
    println!();

    let t0 = cpu::rdtsc();
    time::sleep_ms(100, false);
    let mhz = (cpu::rdtsc().wrapping_sub(t0)) / 100_000;
    println!("Speed    : ~{} MHz (measured with the timer)", mhz);
    println!("CR0={:#x} CR3={:#x} CR4={:#x}", cpu::read_cr0(), cpu::read_cr3(), cpu::read_cr4());
}

fn cmd_ls() {
    let files = fs::list();
    if files.is_empty() {
        println!("(no files)");
        return;
    }
    for (name, size) in &files {
        println!("  {:<32} {:>6} bytes", name, size);
    }
    let (count, bytes) = fs::usage();
    println!("{} file(s), {} bytes", count, bytes);
}

fn cmd_cat(args: &str) {
    let name = args.trim();
    if name.is_empty() {
        println!("usage: cat <file>");
        return;
    }
    match fs::read(name) {
        Ok(text) => {
            print!("{}", text);
            if !text.ends_with('\n') && !text.is_empty() {
                println!();
            }
        }
        Err(e) => cprintln!(Color::LightRed, "cat: {}: {}", name, e),
    }
}

fn cmd_write(args: &str, append: bool) {
    let (name, text) = split_word(args);
    if name.is_empty() {
        println!("usage: {} <file> <text>", if append { "append" } else { "write" });
        return;
    }
    let mut line = String::new();
    if line.try_reserve(text.len() + 1).is_err() {
        cprintln!(Color::LightRed, "out of memory");
        return;
    }
    line.push_str(text);
    line.push('\n');
    let r = if append { fs::append(name, &line) } else { fs::write(name, &line) };
    match r {
        Ok(()) => {}
        Err(e) => cprintln!(Color::LightRed, "{}: {}", name, e),
    }
}

fn cmd_rm(args: &str) {
    let name = args.trim();
    if name.is_empty() {
        println!("usage: rm <file>");
        return;
    }
    if let Err(e) = fs::remove(name) {
        cprintln!(Color::LightRed, "rm: {}: {}", name, e);
    }
}

fn cmd_calc(args: &str) {
    if args.trim().is_empty() {
        println!("usage: calc <expression>   e.g. calc (2+3)*4 - 10/3");
        return;
    }
    match calc::eval(args) {
        Ok(v) => println!("= {}", v),
        Err(e) => cprintln!(Color::LightRed, "calc: {}", e),
    }
}

fn cmd_sleep(args: &str) {
    let secs: u64 = match args.trim().parse() {
        Ok(s) if s <= 3600 => s,
        _ => {
            println!("usage: sleep <seconds 0-3600>");
            return;
        }
    };
    if !time::sleep_ms(secs * 1000, true) {
        println!("^C");
    }
}

#[inline(never)]
#[allow(unconditional_recursion)]
fn recurse(depth: u64) -> u64 {
    let buf = core::hint::black_box([depth; 64]);
    recurse(depth + 1).wrapping_add(buf[7])
}

fn cmd_crash(args: &str) {
    match args.trim() {
        "panic" => panic!("panic requested by the user from the shell"),
        "null" => {
            let p = core::hint::black_box(0usize) as *const u64;
            let v = unsafe { p.read_volatile() };
            println!("read {}", v);
        }
        "stack" => {
            println!("result {}", recurse(0));
        }
        "div0" => unsafe {
            core::arch::asm!("xor edx, edx", "xor eax, eax", "div ecx", in("ecx") 0u32, out("eax") _, out("edx") _);
        },
        _ => {
            println!("Deliberately stop the kernel to see how errors are reported:");
            println!("  crash panic   Rust panic");
            println!("  crash null    NULL pointer read (page fault)");
            println!("  crash stack   endless recursion (stack guard page + double fault)");
            println!("  crash div0    CPU divide error");
        }
    }
}
