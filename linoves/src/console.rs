//! Консоль: вывод одновременно на экран VGA (в текстовом режиме), в COM1 и
//! в текстовую сетку 80x25
//! (её показывает аварийный экран в графическом режиме).

use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::serial;
use crate::sync::IrqMutex;
use crate::vga::{Color, Writer};

pub const GRID_COLS: usize = 80;
pub const GRID_ROWS: usize = 25;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub ch: char,
    /// Цвета как в VGA: младшие 4 бита — текст, старшие — фон.
    pub attr: u8,
}

const BLANK: Cell = Cell { ch: ' ', attr: 0x07 };

/// Текстовая сетка с той же логикой, что и экран VGA.
pub struct TextGrid {
    pub cells: [Cell; GRID_COLS * GRID_ROWS],
    pub col: usize,
    pub row: usize,
    attr: u8,
}

impl TextGrid {
    const fn new() -> Self {
        TextGrid { cells: [BLANK; GRID_COLS * GRID_ROWS], col: 0, row: 0, attr: 0x07 }
    }

    fn clear(&mut self) {
        let blank = Cell { ch: ' ', attr: self.attr };
        self.cells.fill(blank);
        self.col = 0;
        self.row = 0;
    }

    fn newline(&mut self) {
        self.col = 0;
        if self.row + 1 < GRID_ROWS {
            self.row += 1;
        } else {
            self.cells.copy_within(GRID_COLS.., 0);
            let blank = Cell { ch: ' ', attr: self.attr };
            self.cells[(GRID_ROWS - 1) * GRID_COLS..].fill(blank);
        }
    }

    pub fn put_char(&mut self, c: char) {
        match c {
            '\n' => self.newline(),
            '\r' => self.col = 0,
            '\u{8}' => {
                if self.col > 0 {
                    self.col -= 1;
                } else if self.row > 0 {
                    self.row -= 1;
                    self.col = GRID_COLS - 1;
                }
            }
            '\t' => {
                let n = 4 - (self.col % 4);
                for _ in 0..n {
                    self.put_char(' ');
                }
            }
            c if (c as u32) < 0x20 => {}
            c => {
                if self.col >= GRID_COLS {
                    self.newline();
                }
                self.cells[self.row * GRID_COLS + self.col] = Cell { ch: c, attr: self.attr };
                self.col += 1;
            }
        }
    }
}

pub struct Console {
    vga: Writer,
    pub grid: TextGrid,
    fg: Color,
    bg: Color,
    /// Графический режим: экран VGA не используется.
    gui: bool,
}

pub static CONSOLE: IrqMutex<Console> = IrqMutex::new(Console {
    vga: Writer::new(),
    grid: TextGrid::new(),
    fg: Color::LightGray,
    bg: Color::Black,
    gui: false,
});

/// Увеличивается при каждом изменении сетки (окно «Терминал» знает, когда перерисоваться).
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Цвет ANSI для терминала на последовательном порту.
const ANSI: [u8; 16] = [30, 34, 32, 36, 31, 35, 33, 37, 90, 94, 92, 96, 91, 95, 93, 97];

impl Console {
    pub fn set_color(&mut self, fg: Color, bg: Color) {
        self.fg = fg;
        self.bg = bg;
        self.vga.set_color(fg, bg);
        self.grid.attr = ((bg as u8) << 4) | fg as u8;
        let mut buf = [0u8; 16];
        let s = ansi_sequence(&mut buf, ANSI[fg as usize], ANSI[bg as usize] + 10);
        serial::write_str(s);
    }

    pub fn colors(&self) -> (Color, Color) {
        (self.fg, self.bg)
    }

    pub fn clear(&mut self) {
        if !self.gui {
            self.vga.clear();
        }
        self.grid.clear();
        GENERATION.fetch_add(1, Ordering::Relaxed);
        serial::write_str("\x1b[2J\x1b[H");
    }

    pub fn is_gui(&self) -> bool {
        self.gui
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
            self.grid.put_char(c);
            if !self.gui {
                self.vga.put_char(c);
            }
        }
        if !self.gui {
            self.vga.update_cursor();
        }
        GENERATION.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

pub fn init() {
    let mut c = CONSOLE.lock();
    c.vga.enable_cursor();
    c.set_color(Color::LightGray, Color::Black);
    c.clear();
}

/// Переключить консоль в графический режим (экран VGA больше не трогаем).
pub fn set_gui(on: bool) {
    CONSOLE.lock().gui = on;
}

pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

/// Скопировать сетку и позицию курсора (для отрисовки без удержания блокировки).
pub fn copy_grid(out: &mut [Cell]) -> (usize, usize) {
    let c = CONSOLE.lock();
    let n = out.len().min(c.grid.cells.len());
    out[..n].copy_from_slice(&c.grid.cells[..n]);
    (c.grid.col.min(GRID_COLS - 1), c.grid.row)
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
