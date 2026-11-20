//! Последовательный порт COM1 (UART 16550): журнал ядра и ввод с терминала.
//! В QEMU с `-serial stdio` всё, что печатает система, видно в терминале,
//! а набранные там символы приходят в оболочку.

use core::sync::atomic::{AtomicBool, Ordering};

use crate::cpu::{inb, outb};
use crate::input::{self, Key};
use crate::sync::IrqMutex;

const COM1: u16 = 0x3F8;
static PRESENT: AtomicBool = AtomicBool::new(false);

pub fn init() {
    unsafe {
        outb(COM1 + 1, 0x00); // прерывания выкл.
        outb(COM1 + 3, 0x80); // DLAB
        outb(COM1, 0x03); // делитель 3 -> 38400 бод
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03); // 8 бит, без чётности, 1 стоп-бит
        outb(COM1 + 2, 0xC7); // FIFO
        outb(COM1 + 4, 0x1E); // режим петли для проверки
        outb(COM1, 0xAE);
        if inb(COM1) != 0xAE {
            PRESENT.store(false, Ordering::SeqCst);
            return;
        }
        outb(COM1 + 4, 0x0B); // обычный режим: DTR, RTS, OUT2
    }
    PRESENT.store(true, Ordering::SeqCst);
}

pub fn is_present() -> bool {
    PRESENT.load(Ordering::Relaxed)
}

pub fn enable_rx_interrupt() {
    if is_present() {
        unsafe { outb(COM1 + 1, 0x01) };
    }
}

pub fn write_byte(b: u8) {
    if !is_present() {
        return;
    }
    // Ограниченное ожидание: неисправный порт не должен вешать ядро.
    let mut spins = 0u32;
    while unsafe { inb(COM1 + 5) } & 0x20 == 0 {
        spins += 1;
        if spins > 200_000 {
            return;
        }
        core::hint::spin_loop();
    }
    unsafe { outb(COM1, b) };
}

pub fn write_char(c: char) {
    if c == '\n' {
        write_byte(b'\r');
    }
    let mut buf = [0u8; 4];
    for &b in c.encode_utf8(&mut buf).as_bytes() {
        write_byte(b);
    }
}

pub fn write_str(s: &str) {
    for c in s.chars() {
        write_char(c);
    }
}

pub fn try_read_byte() -> Option<u8> {
    if is_present() && unsafe { inb(COM1 + 5) } & 1 != 0 {
        Some(unsafe { inb(COM1) })
    } else {
        None
    }
}

/// Разбор входящих байтов, включая escape-последовательности стрелок.
struct Decoder {
    state: u8,
    param: u8,
    last_cr: bool,
}

static DECODER: IrqMutex<Decoder> = IrqMutex::new(Decoder { state: 0, param: 0, last_cr: false });

impl Decoder {
    fn feed(&mut self, b: u8) {
        match self.state {
            1 => {
                if b == b'[' || b == b'O' {
                    self.state = 2;
                    self.param = 0;
                } else {
                    self.state = 0;
                    input::push(Key::Escape);
                }
                return;
            }
            2 => {
                let key = match b {
                    b'0'..=b'9' => {
                        self.param = self.param.saturating_mul(10).saturating_add(b - b'0');
                        return;
                    }
                    b';' => return,
                    b'A' => Some(Key::Up),
                    b'B' => Some(Key::Down),
                    b'C' => Some(Key::Right),
                    b'D' => Some(Key::Left),
                    b'H' => Some(Key::Home),
                    b'F' => Some(Key::End),
                    b'~' => match self.param {
                        1 | 7 => Some(Key::Home),
                        3 => Some(Key::Delete),
                        4 | 8 => Some(Key::End),
                        5 => Some(Key::PageUp),
                        6 => Some(Key::PageDown),
                        _ => None,
                    },
                    _ => None,
                };
                self.state = 0;
                if let Some(k) = key {
                    input::push(k);
                }
                return;
            }
            _ => {}
        }

        let was_cr = core::mem::replace(&mut self.last_cr, false);
        match b {
            0x1B => self.state = 1,
            b'\r' => {
                self.last_cr = true;
                input::push(Key::Enter);
            }
            b'\n' => {
                if !was_cr {
                    input::push(Key::Enter);
                }
            }
            0x7F | 0x08 => input::push(Key::Backspace),
            b'\t' => input::push(Key::Tab),
            0x03 => input::push(Key::CtrlC),
            0x0C => input::push(Key::CtrlL),
            0x20..=0x7E => input::push(Key::Char(b as char)),
            _ => {}
        }
    }
}

/// Обработчик IRQ4.
pub fn irq() {
    let mut dec = DECODER.lock();
    let mut n = 0;
    while n < 64 {
        match try_read_byte() {
            Some(b) => dec.feed(b),
            None => break,
        }
        n += 1;
    }
}
