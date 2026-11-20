//! Текстовый режим VGA 80x25 (видеопамять по адресу 0xB8000).

use crate::cpu::{inb, outb};

pub const WIDTH: usize = 80;
pub const HEIGHT: usize = 25;
const BUFFER: *mut u16 = 0xB8000 as *mut u16;

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

impl Color {
    pub const ALL: [(&'static str, Color); 16] = [
        ("black", Color::Black),
        ("blue", Color::Blue),
        ("green", Color::Green),
        ("cyan", Color::Cyan),
        ("red", Color::Red),
        ("magenta", Color::Magenta),
        ("brown", Color::Brown),
        ("lightgray", Color::LightGray),
        ("darkgray", Color::DarkGray),
        ("lightblue", Color::LightBlue),
        ("lightgreen", Color::LightGreen),
        ("lightcyan", Color::LightCyan),
        ("lightred", Color::LightRed),
        ("pink", Color::Pink),
        ("yellow", Color::Yellow),
        ("white", Color::White),
    ];

    pub fn from_name(name: &str) -> Option<Color> {
        Color::ALL.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|&(_, c)| c)
    }
}

pub struct Writer {
    col: usize,
    row: usize,
    attr: u8,
}

impl Writer {
    pub const fn new() -> Self {
        Writer { col: 0, row: 0, attr: 0x07 }
    }

    pub fn set_color(&mut self, fg: Color, bg: Color) {
        self.attr = ((bg as u8) << 4) | (fg as u8);
    }

    fn cell(&self, ch: u8) -> u16 {
        ((self.attr as u16) << 8) | ch as u16
    }

    fn write_cell(&mut self, row: usize, col: usize, value: u16) {
        if row < HEIGHT && col < WIDTH {
            unsafe { BUFFER.add(row * WIDTH + col).write_volatile(value) };
        }
    }

    pub fn clear(&mut self) {
        let blank = self.cell(b' ');
        for row in 0..HEIGHT {
            for col in 0..WIDTH {
                self.write_cell(row, col, blank);
            }
        }
        self.row = 0;
        self.col = 0;
        self.update_cursor();
    }

    fn scroll(&mut self) {
        for i in 0..(HEIGHT - 1) * WIDTH {
            unsafe {
                let v = BUFFER.add(i + WIDTH).read_volatile();
                BUFFER.add(i).write_volatile(v);
            }
        }
        let blank = self.cell(b' ');
        for col in 0..WIDTH {
            self.write_cell(HEIGHT - 1, col, blank);
        }
    }

    fn newline(&mut self) {
        self.col = 0;
        if self.row + 1 < HEIGHT {
            self.row += 1;
        } else {
            self.scroll();
        }
    }

    pub fn put_byte(&mut self, b: u8) {
        match b {
            b'\n' => self.newline(),
            b'\r' => self.col = 0,
            0x08 => {
                // курсор влево без стирания (как в терминалах)
                if self.col > 0 {
                    self.col -= 1;
                } else if self.row > 0 {
                    self.row -= 1;
                    self.col = WIDTH - 1;
                }
            }
            b'\t' => {
                let n = 4 - (self.col % 4);
                for _ in 0..n {
                    self.put_byte(b' ');
                }
            }
            _ => {
                if self.col >= WIDTH {
                    self.newline();
                }
                let v = self.cell(b);
                self.write_cell(self.row, self.col, v);
                self.col += 1;
            }
        }
    }

    pub fn put_char(&mut self, c: char) {
        self.put_byte(to_cp437(c));
    }

    pub fn update_cursor(&self) {
        let col = if self.col >= WIDTH { WIDTH - 1 } else { self.col };
        let pos = (self.row * WIDTH + col) as u16;
        unsafe {
            outb(0x3D4, 0x0F);
            outb(0x3D5, (pos & 0xFF) as u8);
            outb(0x3D4, 0x0E);
            outb(0x3D5, (pos >> 8) as u8);
        }
    }

    pub fn enable_cursor(&self) {
        unsafe {
            outb(0x3D4, 0x0A);
            let start = inb(0x3D5) & 0xC0;
            outb(0x3D5, start | 13);
            outb(0x3D4, 0x0B);
            let end = inb(0x3D5) & 0xE0;
            outb(0x3D5, end | 15);
        }
    }
}

/// Перевод символа Unicode в кодовую страницу видеокарты (CP437).
fn to_cp437(c: char) -> u8 {
    match c {
        '\n' | '\r' | '\t' | '\u{8}' => c as u8,
        ' '..='~' => c as u8,
        '█' => 0xDB,
        '▓' => 0xB2,
        '▒' => 0xB1,
        '░' => 0xB0,
        '─' => 0xC4,
        '│' => 0xB3,
        '┌' => 0xDA,
        '┐' => 0xBF,
        '└' => 0xC0,
        '┘' => 0xD9,
        '═' => 0xCD,
        '║' => 0xBA,
        '╔' => 0xC9,
        '╗' => 0xBB,
        '╚' => 0xC8,
        '╝' => 0xBC,
        '•' => 0x07,
        '·' => 0xFA,
        '°' => 0xF8,
        '■' => 0xFE,
        _ => b'?',
    }
}
