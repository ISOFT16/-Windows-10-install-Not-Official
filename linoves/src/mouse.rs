//! Мышь PS/2 (IRQ 12): пакеты по 3 байта -> события перемещения и кнопок.

use crate::cpu::{inb, outb};
use crate::keyboard;
use crate::sync::IrqMutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct MouseEvent {
    pub dx: i32,
    /// Положительное значение — движение вниз (в экранных координатах).
    pub dy: i32,
    /// Бит 0 — левая, бит 1 — правая, бит 2 — средняя кнопка.
    pub buttons: u8,
}

const CAPACITY: usize = 64;

struct Queue {
    buf: [MouseEvent; CAPACITY],
    head: usize,
    len: usize,
}

struct Packet {
    bytes: [u8; 3],
    idx: usize,
}

static QUEUE: IrqMutex<Queue> =
    IrqMutex::new(Queue { buf: [MouseEvent { dx: 0, dy: 0, buttons: 0 }; CAPACITY], head: 0, len: 0 });
static PACKET: IrqMutex<Packet> = IrqMutex::new(Packet { bytes: [0; 3], idx: 0 });

fn wait_write() -> bool {
    for _ in 0..100_000 {
        if unsafe { inb(0x64) } & 0x02 == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

fn wait_read() -> bool {
    for _ in 0..100_000 {
        if unsafe { inb(0x64) } & 0x01 != 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

fn write_cmd(cmd: u8) -> bool {
    wait_write() && {
        unsafe { outb(0x64, cmd) };
        true
    }
}

fn write_data(b: u8) -> bool {
    wait_write() && {
        unsafe { outb(0x60, b) };
        true
    }
}

fn mouse_command(b: u8) -> bool {
    if !(write_cmd(0xD4) && write_data(b)) {
        return false;
    }
    wait_read() && unsafe { inb(0x60) } == 0xFA
}

/// Включить мышь. Вызывать при выключенных прерываниях.
pub fn init() -> bool {
    // очистить буфер контроллера
    for _ in 0..64 {
        if unsafe { inb(0x64) } & 1 == 0 {
            break;
        }
        unsafe { inb(0x60) };
    }
    if !write_cmd(0xA8) {
        return false; // включить второй порт (мышь)
    }
    if !write_cmd(0x20) || !wait_read() {
        return false;
    }
    let mut config = unsafe { inb(0x60) };
    config |= 0x02; // прерывание IRQ12
    config &= !0x20; // включить тактирование мыши
    if !(write_cmd(0x60) && write_data(config)) {
        return false;
    }
    mouse_command(0xF6) && mouse_command(0xF4)
}

/// Разобрать 3-байтовый пакет. `None` — пакет с переполнением (отбрасывается).
pub fn decode(p: [u8; 3]) -> Option<MouseEvent> {
    let flags = p[0];
    if flags & 0xC0 != 0 {
        return None;
    }
    let mut dx = p[1] as i32;
    let mut dy = p[2] as i32;
    if flags & 0x10 != 0 {
        dx -= 256;
    }
    if flags & 0x20 != 0 {
        dy -= 256;
    }
    Some(MouseEvent { dx, dy: -dy, buttons: flags & 0x07 })
}

fn push(ev: MouseEvent) {
    let mut q = QUEUE.lock();
    if q.len == CAPACITY {
        // очередь полна: складываем движение с последним событием, кнопки не теряются
        let last = (q.head + q.len - 1) % CAPACITY;
        if q.buf[last].buttons == ev.buttons {
            q.buf[last].dx = q.buf[last].dx.saturating_add(ev.dx);
            q.buf[last].dy = q.buf[last].dy.saturating_add(ev.dy);
        }
        return;
    }
    let idx = (q.head + q.len) % CAPACITY;
    q.buf[idx] = ev;
    q.len += 1;
}

pub fn try_pop() -> Option<MouseEvent> {
    let mut q = QUEUE.lock();
    if q.len == 0 {
        return None;
    }
    let ev = q.buf[q.head];
    q.head = (q.head + 1) % CAPACITY;
    q.len -= 1;
    Some(ev)
}

pub fn handle_byte(b: u8) {
    let mut p = PACKET.lock();
    if p.idx == 0 && b & 0x08 == 0 {
        return; // первый байт пакета всегда имеет бит 3: ждём синхронизации
    }
    let i = p.idx;
    p.bytes[i] = b;
    p.idx += 1;
    if p.idx == 3 {
        p.idx = 0;
        let bytes = p.bytes;
        drop(p);
        if let Some(ev) = decode(bytes) {
            push(ev);
        }
    }
}

/// Обработчик IRQ12.
pub fn irq() {
    let status = unsafe { inb(0x64) };
    if status & 1 == 0 {
        return;
    }
    let b = unsafe { inb(0x60) };
    if status & 0x20 != 0 {
        handle_byte(b);
    } else {
        keyboard::handle_scancode(b);
    }
}
