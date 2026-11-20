//! Очередь нажатий клавиш: её наполняют обработчики прерываний клавиатуры и
//! последовательного порта, а читает оболочка.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::cpu;
use crate::sync::IrqMutex;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Char(char),
    Enter,
    Backspace,
    Delete,
    Tab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Escape,
    CtrlC,
    CtrlL,
}

const CAPACITY: usize = 128;

struct Queue {
    buf: [Key; CAPACITY],
    head: usize,
    len: usize,
}

static QUEUE: IrqMutex<Queue> =
    IrqMutex::new(Queue { buf: [Key::Escape; CAPACITY], head: 0, len: 0 });
static DROPPED: AtomicU64 = AtomicU64::new(0);

/// Добавить клавишу. При переполнении клавиша отбрасывается (и считается),
/// а не затирает чужую память.
pub fn push(key: Key) {
    let mut q = QUEUE.lock();
    if q.len == CAPACITY {
        DROPPED.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let idx = (q.head + q.len) % CAPACITY;
    q.buf[idx] = key;
    q.len += 1;
}

pub fn try_pop() -> Option<Key> {
    let mut q = QUEUE.lock();
    if q.len == 0 {
        return None;
    }
    let key = q.buf[q.head];
    q.head = (q.head + 1) % CAPACITY;
    q.len -= 1;
    Some(key)
}

pub fn dropped() -> u64 {
    DROPPED.load(Ordering::Relaxed)
}

/// Дождаться клавиши, усыпляя процессор (`hlt`) между прерываниями.
pub fn read_key() -> Key {
    loop {
        cpu::disable_interrupts();
        if let Some(k) = try_pop() {
            unsafe { cpu::enable_interrupts() };
            return k;
        }
        unsafe { cpu::enable_interrupts_and_halt() };
    }
}
