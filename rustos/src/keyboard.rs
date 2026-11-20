//! Клавиатура PS/2 (набор скан-кодов 1, раскладка US).

use crate::cpu::inb;
use crate::input::{self, Key};
use crate::power;
use crate::sync::IrqMutex;

const DATA: u16 = 0x60;
const STATUS: u16 = 0x64;

const NORMAL: [u8; 58] =
    *b"\0\x1b1234567890-=\x08\tqwertyuiop[]\n\0asdfghjkl;'`\0\\zxcvbnm,./\0*\0 ";
const SHIFTED: [u8; 58] =
    *b"\0\x1b!@#$%^&*()_+\x08\tQWERTYUIOP{}\n\0ASDFGHJKL:\"~\0|ZXCVBNM<>?\0*\0 ";

struct State {
    extended: bool,
    skip: u8,
    lshift: bool,
    rshift: bool,
    ctrl: bool,
    alt: bool,
    caps: bool,
}

static STATE: IrqMutex<State> = IrqMutex::new(State {
    extended: false,
    skip: 0,
    lshift: false,
    rshift: false,
    ctrl: false,
    alt: false,
    caps: false,
});

/// Очистить буфер контроллера от старых данных.
pub fn init() {
    for _ in 0..64 {
        if unsafe { inb(STATUS) } & 1 == 0 {
            break;
        }
        unsafe { inb(DATA) };
    }
}

/// Обработчик IRQ1.
pub fn irq() {
    if unsafe { inb(STATUS) } & 1 == 0 {
        return;
    }
    let sc = unsafe { inb(DATA) };
    let mut st = STATE.lock();

    if st.skip > 0 {
        st.skip -= 1;
        return;
    }
    match sc {
        0xE0 => {
            st.extended = true;
            return;
        }
        0xE1 => {
            st.skip = 5; // клавиша Pause: E1 1D 45 E1 9D C5
            return;
        }
        _ => {}
    }

    let released = sc & 0x80 != 0;
    let code = sc & 0x7F;
    let extended = core::mem::replace(&mut st.extended, false);

    if extended {
        let key = match code {
            0x1D => {
                st.ctrl = !released;
                None
            }
            0x38 => {
                st.alt = !released;
                None
            }
            _ if released => None,
            0x48 => Some(Key::Up),
            0x50 => Some(Key::Down),
            0x4B => Some(Key::Left),
            0x4D => Some(Key::Right),
            0x47 => Some(Key::Home),
            0x4F => Some(Key::End),
            0x49 => Some(Key::PageUp),
            0x51 => Some(Key::PageDown),
            0x1C => Some(Key::Enter),
            0x35 => Some(Key::Char('/')),
            0x53 => {
                if st.ctrl && st.alt {
                    power::reboot();
                }
                Some(Key::Delete)
            }
            _ => None,
        };
        drop(st);
        if let Some(k) = key {
            input::push(k);
        }
        return;
    }

    let key = match code {
        0x2A => {
            st.lshift = !released;
            None
        }
        0x36 => {
            st.rshift = !released;
            None
        }
        0x1D => {
            st.ctrl = !released;
            None
        }
        0x38 => {
            st.alt = !released;
            None
        }
        0x3A => {
            if !released {
                st.caps = !st.caps;
            }
            None
        }
        _ if released => None,
        0x01 => Some(Key::Escape),
        0x0E => Some(Key::Backspace),
        0x0F => Some(Key::Tab),
        0x1C => Some(Key::Enter),
        0x53 if st.ctrl && st.alt => power::reboot(),
        c if (c as usize) < NORMAL.len() => {
            let base = NORMAL[c as usize];
            if base == 0 {
                None
            } else {
                let letter = base.is_ascii_lowercase();
                let shift = st.lshift || st.rshift;
                if st.ctrl && letter {
                    match base {
                        b'c' => Some(Key::CtrlC),
                        b'l' => Some(Key::CtrlL),
                        _ => None,
                    }
                } else {
                    let upper = if letter { shift ^ st.caps } else { shift };
                    let ch = if upper { SHIFTED[c as usize] } else { base };
                    Some(Key::Char(ch as char))
                }
            }
        }
        _ => None,
    };
    drop(st);
    if let Some(k) = key {
        input::push(k);
    }
}

/// Для аварийного экрана: ждём нажатия опросом, с выключенными прерываниями.
pub fn poll_key_pressed() -> bool {
    if unsafe { inb(STATUS) } & 1 != 0 {
        let sc = unsafe { inb(DATA) };
        return sc != 0xE0 && sc & 0x80 == 0;
    }
    false
}
