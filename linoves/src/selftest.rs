//! Встроенные самотесты ядра. Запускаются командой `selftest` или
//! автоматически при загрузке с параметром `selftest` (тогда QEMU
//! завершается с кодом результата — так проверка работает в CI).

use alloc::alloc::{alloc, dealloc};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::vga::Color;
use crate::{calc, cpu, fs, heap, interrupts, power, time};

static TEST_MODE: AtomicBool = AtomicBool::new(false);

pub fn is_test_mode() -> bool {
    TEST_MODE.load(Ordering::Relaxed)
}

type TestResult = Result<(), &'static str>;

fn ensure(cond: bool, msg: &'static str) -> TestResult {
    if cond {
        Ok(())
    } else {
        Err(msg)
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

fn test_heap_basic() -> TestResult {
    let b = Box::new(0xDEAD_BEEFu64);
    ensure(*b == 0xDEAD_BEEF, "Box value mismatch")?;
    let v: Vec<u64> = (0..1000).collect();
    ensure(v.iter().sum::<u64>() == 999 * 1000 / 2, "Vec sum mismatch")?;
    let s = format!("{}-{}", "rust", 42);
    ensure(s == "rust-42", "format! mismatch")
}

fn test_heap_alignment() -> TestResult {
    for shift in 0..=12 {
        let align = 1usize << shift;
        let layout = Layout::from_size_align(24, align).map_err(|_| "bad layout")?;
        let p = unsafe { alloc(layout) };
        ensure(!p.is_null(), "aligned allocation failed")?;
        let ok = p as usize % align == 0;
        unsafe {
            p.write_bytes(0xAB, 24);
            dealloc(p, layout);
        }
        ensure(ok, "allocation is not aligned")?;
    }
    Ok(())
}

fn test_heap_stress() -> TestResult {
    let before = heap::stats().used;
    {
        let mut rng = Rng(cpu::rdtsc() | 1);
        let mut blocks: Vec<Vec<u8>> = Vec::with_capacity(2000);
        for i in 0..2000 {
            let size = (rng.next() % 2048) as usize + 1;
            let mut v = Vec::with_capacity(size);
            v.resize(size, (i % 251) as u8);
            blocks.push(v);
        }
        for i in 0..blocks.len() {
            let j = (rng.next() as usize) % blocks.len();
            blocks.swap(i, j);
        }
        for b in blocks.iter() {
            let first = b[0];
            if b.iter().any(|&x| x != first) {
                return Err("heap block corrupted (contents overwritten)");
            }
        }
        while let Some(b) = blocks.pop() {
            drop(b);
        }
    }
    heap::check_integrity()?;
    ensure(heap::stats().used == before, "memory leak: used bytes changed")
}

fn test_heap_large() -> TestResult {
    let mut v: Vec<u8> = Vec::new();
    v.try_reserve_exact(4 * 1024 * 1024).map_err(|_| "4 MiB allocation failed")?;
    v.resize(4 * 1024 * 1024, 0);
    for (i, b) in v.iter_mut().enumerate().step_by(4096) {
        *b = (i / 4096) as u8;
    }
    let ok = v.iter().enumerate().step_by(4096).all(|(i, &b)| b == (i / 4096) as u8);
    ensure(ok, "large buffer readback failed")
}

fn test_heap_oom_is_handled() -> TestResult {
    let mut v: Vec<u8> = Vec::new();
    // Явно невыполнимый запрос должен вернуть ошибку, а не уронить ядро.
    ensure(v.try_reserve_exact(1 << 40).is_err(), "1 TiB allocation unexpectedly succeeded")
}

fn test_collections() -> TestResult {
    let mut m = BTreeMap::new();
    for i in 0..500u32 {
        m.insert(i.wrapping_mul(2654435761) % 10007, i);
    }
    ensure(m.len() == 500, "BTreeMap size mismatch")?;
    let keys: Vec<_> = m.keys().copied().collect();
    ensure(keys.windows(2).all(|w| w[0] < w[1]), "BTreeMap order broken")?;
    let mut s = String::new();
    for _ in 0..100 {
        s.push_str("abc");
    }
    ensure(s.len() == 300, "String length mismatch")
}

fn test_timer() -> TestResult {
    ensure(cpu::interrupts_enabled(), "interrupts are disabled")?;
    let t0 = time::ticks();
    time::sleep_ms(50, false);
    ensure(time::ticks() >= t0 + 5, "timer interrupt is not advancing")
}

fn test_breakpoint() -> TestResult {
    let before = interrupts::breakpoint_count();
    cpu::breakpoint();
    ensure(interrupts::breakpoint_count() == before + 1, "int3 handler did not run")
}

fn test_fs() -> TestResult {
    let name = "selftest.tmp";
    fs::write(name, "hello").map_err(|_| "fs write failed")?;
    fs::append(name, " world").map_err(|_| "fs append failed")?;
    ensure(fs::read(name).map_err(|_| "fs read failed")? == "hello world", "fs content mismatch")?;
    fs::remove(name).map_err(|_| "fs remove failed")?;
    ensure(fs::read(name) == Err(fs::FsError::NotFound), "file still exists after rm")?;
    ensure(fs::write("bad/name", "x") == Err(fs::FsError::InvalidName), "invalid name accepted")?;
    let big = "x".repeat(fs::MAX_FILE_SIZE + 1);
    ensure(fs::write("big.tmp", &big) == Err(fs::FsError::FileTooLarge), "size limit not enforced")
}

fn test_calc() -> TestResult {
    ensure(calc::eval("2+3*4") == Ok(14), "precedence")?;
    ensure(calc::eval("(2+3)*4") == Ok(20), "parentheses")?;
    ensure(calc::eval("-7/2") == Ok(-3), "negative division")?;
    ensure(calc::eval("1/0").is_err(), "division by zero not caught")?;
    ensure(calc::eval("9223372036854775807+1").is_err(), "overflow not caught")?;
    let deep = format!("{}1{}", "(".repeat(500), ")".repeat(500));
    ensure(calc::eval(&deep).is_err(), "deep nesting not limited")
}

fn test_gfx() -> TestResult {
    use crate::gfx::{font, rgb, Canvas, Rect};
    let mut buf = alloc::vec![0u32; 64 * 32];
    {
        let mut c = Canvas::new(&mut buf, 64, 32);
        // рисование за границами не должно ничего ломать
        c.fill_rect(-100, -100, 1000, 1000, rgb(1, 2, 3));
        c.thick_line(-500, -500, 500, 500, 5, rgb(9, 9, 9));
        c.fill_circle(1000, 1000, 50, 0);
        let mut sub = c.sub(Rect::new(10, 10, 20, 10));
        sub.fill(rgb(255, 0, 0));
        sub.text(0, 0, "Привет, Linoves", rgb(0, 255, 0));
    }
    ensure(buf[63] == rgb(1, 2, 3), "fill_rect with clipping failed")?;
    ensure(buf[10 * 64 + 30] != rgb(255, 0, 0), "sub-canvas drew outside its area")?;
    ensure(buf[10 * 64 + 9] != rgb(255, 0, 0), "sub-canvas drew outside its area")?;
    ensure(font::has_glyph('Ж') && font::has_glyph('A'), "font is missing glyphs")?;
    ensure(font::glyph('\u{1F600}').len() == font::WIDTH * font::HEIGHT, "fallback glyph broken")
}

fn test_mouse_decoder() -> TestResult {
    use crate::mouse::{decode, MouseEvent};
    ensure(decode([0x09, 5, 3]) == Some(MouseEvent { dx: 5, dy: -3, buttons: 1 }), "plain packet")?;
    ensure(decode([0x38, 0xFB, 0xFE]) == Some(MouseEvent { dx: -5, dy: 2, buttons: 0 }), "negative packet")?;
    ensure(decode([0x48, 1, 1]).is_none(), "overflow packet must be dropped")
}

const TESTS: &[(&str, fn() -> TestResult)] = &[
    ("heap: Box, Vec, String", test_heap_basic),
    ("heap: alignment 1..4096", test_heap_alignment),
    ("heap: 2000 random alloc/free, no leaks", test_heap_stress),
    ("heap: 4 MiB buffer", test_heap_large),
    ("heap: impossible request fails safely", test_heap_oom_is_handled),
    ("heap: free-list integrity", heap::check_integrity),
    ("collections: BTreeMap, String", test_collections),
    ("timer: PIT interrupts", test_timer),
    ("exceptions: int3 recovers", test_breakpoint),
    ("ramfs: write/append/read/rm, limits", test_fs),
    ("calc: precedence, overflow, depth", test_calc),
    ("gfx: clipping, sub-canvas, Cyrillic font", test_gfx),
    ("mouse: PS/2 packet decoder", test_mouse_decoder),
];

/// Запустить все тесты. Возвращает `true`, если все прошли.
pub fn run() -> bool {
    cprintln!(Color::Yellow, "Running {} kernel self-tests...", TESTS.len());
    let mut failed = 0;
    for (name, test) in TESTS {
        match test() {
            Ok(()) => {
                cprint!(Color::LightGreen, "  [ OK ] ");
                println!("{}", name);
            }
            Err(e) => {
                failed += 1;
                cprint!(Color::LightRed, "  [FAIL] ");
                println!("{}: {}", name, e);
            }
        }
    }
    if failed == 0 {
        cprintln!(Color::LightGreen, "All {} tests passed.", TESTS.len());
    } else {
        cprintln!(Color::LightRed, "{} of {} tests FAILED.", failed, TESTS.len());
    }
    failed == 0
}

/// Режим автоматической проверки (параметр загрузки `selftest`).
pub fn run_and_exit() {
    TEST_MODE.store(true, Ordering::Relaxed);
    let ok = run();
    println!("LINOVES_SELFTEST: {}", if ok { "PASS" } else { "FAIL" });
    power::qemu_exit(ok);
    // Без устройства isa-debug-exit просто продолжаем работу в оболочке.
    TEST_MODE.store(false, Ordering::Relaxed);
}
