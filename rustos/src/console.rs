//! Консоль: всё выводится одновременно на экран (VGA) и в COM1.

use core::fmt;

use crate::serial;
use crate::sync::IrqMutex;
use crate::vga::{Color, Writer};

pub struct Console {
    vga: Writer,
    fg: Color,
    bg: Color,
}

pub static CONSOLE: IrqMutex<Console> = IrqMutex::new(Console {
    vga: Writer::new(),
    fg: Color::LightGray,
    bg: Color::Black,
});

/// Цвет ANSI для терминала на последовательном порту.
const ANSI: [u8; 16] = [30, 34, 32, 36, 31, 35, 33, 37, 90, 94, 92, 96, 91, 95, 93, 97];

impl Console {
    pub fn set_color(&mut self, fg: Color, bg: Color) {
        self.fg = fg;
        self.bg = bg;
        self.vga.set_color(fg, bg);
        let mut buf = [0u8; 16];
        let s = ansi_sequence(&mut buf, ANSI[fg as usize], ANSI[bg as usize] + 10);
        serial::write_str(s);
    }

    pub fn colors(&self) -> (Color, Color) {
        (self.fg, self.bg)
    }

    pub fn clear(&mut self) {
        self.vga.clear();
        serial::write_str("\x1b[2J\x1b[H");
    }
}

fn ansi_sequence(buf: &mut [u8; 16], fg: u8, bg: u8) -> &str {
    let mut n = 0;
    let mut push = |b: u8, n: &mut usize| {
        buf[*n] = b;
        *n += 1;
    };
    push(0x1B, &mut n);
    push(b'[', &mut n);
    for (i, v) in [fg, bg].iter().enumerate() {
        if i > 0 {
            push(b';', &mut n);
        }
        if *v >= 100 {
            push(b'0' + v / 100, &mut n);
        }
        push(b'0' + (v / 10) % 10, &mut n);
        push(b'0' + v % 10, &mut n);
    }
    push(b'm', &mut n);
    core::str::from_utf8(&buf[..n]).unwrap_or("")
}

impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            serial::write_char(c);
            self.vga.put_char(c);
        }
        self.vga.update_cursor();
        Ok(())
    }
}

pub fn init() {
    let mut c = CONSOLE.lock();
    c.vga.enable_cursor();
    c.set_color(Color::LightGray, Color::Black);
    c.clear();
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    let _ = CONSOLE.lock().write_fmt(args);
}

pub fn set_color(fg: Color) {
    let mut c = CONSOLE.lock();
    let bg = c.bg;
    c.set_color(fg, bg);
}

pub fn clear() {
    CONSOLE.lock().clear();
}

/// Напечатать текст указанным цветом и вернуть прежний цвет.
pub fn print_colored(fg: Color, args: fmt::Arguments) {
    use core::fmt::Write;
    let mut c = CONSOLE.lock();
    let (old_fg, old_bg) = c.colors();
    c.set_color(fg, old_bg);
    let _ = c.write_fmt(args);
    c.set_color(old_fg, old_bg);
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::console::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[macro_export]
macro_rules! cprint {
    ($color:expr, $($arg:tt)*) => ($crate::console::print_colored($color, format_args!($($arg)*)));
}

#[macro_export]
macro_rules! cprintln {
    ($color:expr, $($arg:tt)*) => ($crate::console::print_colored($color, format_args!("{}\n", format_args!($($arg)*))));
}
