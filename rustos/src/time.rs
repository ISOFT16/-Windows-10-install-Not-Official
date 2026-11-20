//! Системный таймер PIT 8253/8254: 100 прерываний в секунду.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::cpu::{self, outb};
use crate::input::{self, Key};

pub const HZ: u64 = 100;
const PIT_FREQUENCY: u64 = 1_193_182;

static TICKS: AtomicU64 = AtomicU64::new(0);

pub fn init() {
    let divisor = (PIT_FREQUENCY / HZ) as u16;
    unsafe {
        outb(0x43, 0x36); // канал 0, младший+старший байт, режим 3
        outb(0x40, (divisor & 0xFF) as u8);
        outb(0x40, (divisor >> 8) as u8);
    }
}

pub fn tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

pub fn uptime_ms() -> u64 {
    ticks() * 1000 / HZ
}

/// Подождать `ms` миллисекунд. Возвращает `false`, если ожидание прервано
/// клавишами Ctrl+C. Если прерывания выключены, таймер не идёт, поэтому
/// функция сразу возвращается вместо вечного ожидания.
pub fn sleep_ms(ms: u64, interruptible: bool) -> bool {
    if !cpu::interrupts_enabled() {
        return true;
    }
    let wait = ms.saturating_mul(HZ).saturating_add(999) / 1000;
    let target = ticks().saturating_add(wait.max(1));
    while ticks() < target {
        if interruptible {
            if let Some(Key::CtrlC) = input::try_pop() {
                return false;
            }
        }
        cpu::halt();
    }
    true
}
