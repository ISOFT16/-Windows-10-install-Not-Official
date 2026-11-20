//! Приложения рабочего стола Linoves.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::{
    Action, App, AppKind, ACCENT, ACCENT_SOFT, BORDER, DANGER, MUTED, OPEN_WINDOWS, SETTINGS,
    TEXT, WALLPAPERS,
};
use crate::console::{self, Cell, GRID_COLS, GRID_ROWS};
use crate::gfx::ui::{self, Font, Icon};
use crate::gfx::{self, rgb, Canvas, Color, Rect, PALETTE};
use crate::input::Key;
use crate::shell::Shell;
use crate::{calc, cpu, fs, heap, keyboard, rtc, time};

const WHITE: Color = rgb(255, 255, 255);
const SURFACE: Color = rgb(241, 245, 249);
const LINE: Color = rgb(226, 232, 240);

// ---------------------------------------------------------------------------
// Общие элементы
// ---------------------------------------------------------------------------

fn button(c: &mut Canvas, r: Rect, icon: Option<Icon>, label: &str, primary: bool) {
    let (bg, fg) = if primary { (ACCENT, WHITE) } else { (WHITE, TEXT) };
    if !primary {
        c.round_rect(r.x, r.y, r.w, r.h, 6, BORDER, 255);
        c.round_rect(r.x + 1, r.y + 1, r.w - 2, r.h - 2, 5, bg, 255);
    } else {
        c.round_rect(r.x, r.y, r.w, r.h, 6, bg, 255);
    }
    let tw = ui::text_width(Font::Regular, label);
    let iw = if icon.is_some() { 22 } else { 0 };
    let x0 = r.x + (r.w - tw - iw) / 2;
    if let Some(i) = icon {
        ui::icon(c, i, 16, x0, r.y + (r.h - 16) / 2, if primary { WHITE } else { ACCENT });
    }
    ui::text(c, Font::Regular, x0 + iw, r.y + (r.h - 15) / 2, label, fg);
}

fn toolbar(c: &mut Canvas, w: i32, h: i32) {
    c.fill_rect(0, 0, w, h, SURFACE);
    c.fill_rect(0, h - 1, w, 1, LINE);
}

fn status_bar(c: &mut Canvas, w: i32, h: i32, left: &str, right: &str) {
    c.fill_rect(0, h - 26, w, 26, SURFACE);
    c.fill_rect(0, h - 26, w, 1, LINE);
    ui::text(c, Font::Regular, 12, h - 20, left, MUTED);
    ui::text(c, Font::Regular, w - 12 - ui::text_width(Font::Regular, right), h - 20, right, MUTED);
}

fn double_click(last: &mut (Option<usize>, u64), hit: Option<usize>) -> bool {
    let now = time::ticks();
    let d = hit.is_some() && last.0 == hit && now - last.1 <= time::HZ / 2;
    *last = if d { (None, 0) } else { (hit, now) };
    d
}

pub fn empty_state(c: &mut Canvas, w: i32, h: i32, icon: Icon, icon_color: Color, lines: &[(Font, &str, Color)]) {
    let text_h: i32 = lines.iter().map(|(f, _, _)| ui::line_height(*f) + 6).sum();
    let mut y = (h - 64 - text_h) / 2;
    ui::icon(c, icon, 48, (w - 48) / 2, y, icon_color);
    y += 64;
    for (f, s, color) in lines {
        ui::text_centered(c, *f, 0, y, w, s, *color);
        y += ui::line_height(*f) + 6;
    }
}

struct Rng(u64);

impl Rng {
    fn new() -> Self {
        Rng(cpu::rdtsc() | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

// ---------------------------------------------------------------------------
// Простые окна-заглушки (Добро пожаловать, Корзина, Почта, Браузер, Пользователь)
// ---------------------------------------------------------------------------

pub struct Info {
    kind: AppKind,
}

impl Info {
    pub fn new(kind: AppKind) -> Self {
        Info { kind }
    }
}

impl App for Info {
    fn kind(&self) -> AppKind {
        self.kind
    }
    fn size(&self) -> (i32, i32) {
        (460, 300)
    }
    fn draw(&mut self, c: &mut Canvas, w: i32, h: i32, _focused: bool) {
        match self.kind {
            AppKind::Welcome => {
                let v = format!("Версия {} ({})", crate::VERSION, crate::CODENAME);
                empty_state(c, w, h, Icon::Sparkles, ACCENT, &[
                    (Font::Large, "Добро пожаловать в Linoves!", TEXT),
                    (Font::Regular, &v, MUTED),
                    (Font::Regular, "", MUTED),
                    (Font::Regular, "Все программы — в меню и в разделе «Приложения».", TEXT),
                    (Font::Regular, "Раскладка клавиатуры: Alt+Shift.", TEXT),
                ]);
            }
            AppKind::Bin => empty_state(c, w, h, Icon::Trash2, MUTED, &[(Font::Large, "Корзина пуста", TEXT)]),
            AppKind::User => empty_state(c, w, h, Icon::CircleUser, ACCENT, &[
                (Font::Large, "Пользователь 1", TEXT),
                (Font::Regular, "Учётная запись этого компьютера", MUTED),
            ]),
            AppKind::Mail | AppKind::Browser => empty_state(c, w, h, Icon::WifiOff, MUTED, &[
                (Font::Large, self.kind.title(), TEXT),
                (Font::Regular, "Нет подключения к сети.", MUTED),
                (Font::Regular, "Поддержка сети появится в следующих версиях.", MUTED),
            ]),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Приложения (сетка всех программ)
// ---------------------------------------------------------------------------

const LAUNCHER: [AppKind; 10] = [
    AppKind::Terminal,
    AppKind::Notepad,
    AppKind::Calculator,
    AppKind::Paint,
    AppKind::Files,
    AppKind::Monitor,
    AppKind::Settings,
    AppKind::Snake,
    AppKind::Browser,
    AppKind::Mail,
];

pub struct Launcher;

impl Launcher {
    fn tile(i: usize) -> Rect {
        let (col, row) = ((i % 5) as i32, (i / 5) as i32);
        Rect::new(16 + col * 104, 52 + row * 108, 96, 100)
    }
}

impl App for Launcher {
    fn kind(&self) -> AppKind {
        AppKind::Apps
    }
    fn size(&self) -> (i32, i32) {
        (552, 280)
    }
    fn draw(&mut self, c: &mut Canvas, _w: i32, _h: i32, _focused: bool) {
        ui::text(c, Font::Bold, 18, 16, "Все приложения", TEXT);
        for (i, kind) in LAUNCHER.iter().enumerate() {
            let r = Self::tile(i);
            c.round_rect(r.x + 24, r.y + 8, 48, 48, 12, ACCENT_SOFT, 255);
            ui::icon(c, kind.icon(), 24, r.x + 36, r.y + 20, ACCENT);
            let label = ui::fit(Font::Regular, kind.title(), r.w);
            ui::text_centered(c, Font::Regular, r.x, r.y + 64, r.w, &label, TEXT);
        }
    }
    fn mouse_down(&mut self, x: i32, y: i32) -> Action {
        match (0..LAUNCHER.len()).find(|&i| Self::tile(i).contains(x, y)) {
            Some(i) => Action::Open(LAUNCHER[i]),
            None => Action::None,
        }
    }
}

// ---------------------------------------------------------------------------
// Терминал: та же оболочка, что и в текстовом режиме
// ---------------------------------------------------------------------------

pub struct Terminal {
    shell: Shell,
    cells: Vec<Cell>,
    seen: u64,
    blink: bool,
}

const TERM_PAD: i32 = 8;

impl Terminal {
    pub fn new() -> Self {
        let mut shell = Shell::new();
        console::clear();
        crate::println!("Linoves {} ({}) terminal. Type 'help' for a list of commands.", crate::VERSION, crate::CODENAME);
        shell.prompt();
        Terminal { shell, cells: vec![Cell { ch: ' ', attr: 7 }; GRID_COLS * GRID_ROWS], seen: 0, blink: true }
    }
}

impl App for Terminal {
    fn kind(&self) -> AppKind {
        AppKind::Terminal
    }
    fn size(&self) -> (i32, i32) {
        (GRID_COLS as i32 * 8 + 2 * TERM_PAD, GRID_ROWS as i32 * 16 + 2 * TERM_PAD)
    }
    fn draw(&mut self, c: &mut Canvas, w: i32, h: i32, focused: bool) {
        self.seen = console::generation();
        let (ccol, crow) = console::copy_grid(&mut self.cells);
        c.fill_rect(0, 0, w, h, PALETTE[0]);
        for row in 0..GRID_ROWS {
            for col in 0..GRID_COLS {
                let cell = self.cells[row * GRID_COLS + col];
                let (x, y) = (TERM_PAD + col as i32 * 8, TERM_PAD + row as i32 * 16);
                let bg = (cell.attr >> 4) & 0x0F;
                if bg != 0 {
                    c.fill_rect(x, y, 8, 16, PALETTE[bg as usize]);
                }
                if cell.ch != ' ' {
                    c.draw_char(x, y, cell.ch, PALETTE[(cell.attr & 0x0F) as usize]);
                }
            }
        }
        if focused && self.blink {
            c.fill_rect(TERM_PAD + ccol as i32 * 8, TERM_PAD + crow as i32 * 16 + 13, 8, 2, rgb(230, 230, 230));
        }
    }
    fn key(&mut self, k: Key) -> Action {
        self.shell.handle_key(k);
        self.blink = true;
        Action::None
    }
    fn tick(&mut self, now: u64, focused: bool) -> bool {
        let blink = (now / (time::HZ / 2)) % 2 == 0;
        let changed = console::generation() != self.seen || (focused && blink != self.blink);
        self.blink = blink;
        changed
    }
}

// ---------------------------------------------------------------------------
// Файлы
// ---------------------------------------------------------------------------

pub struct Files {
    selected: Option<usize>,
    scroll: usize,
    last_click: (Option<usize>, u64),
    message: String,
}

const FILES_BAR: i32 = 48;
const FILES_ROW: i32 = 32;

impl Files {
    pub fn new() -> Self {
        Files { selected: None, scroll: 0, last_click: (None, 0), message: String::new() }
    }

    fn buttons() -> [(Rect, Icon, &'static str); 3] {
        [
            (Rect::new(12, 10, 108, 28), Icon::FolderOpen, "Открыть"),
            (Rect::new(128, 10, 108, 28), Icon::FilePlus, "Создать"),
            (Rect::new(244, 10, 108, 28), Icon::Trash2, "Удалить"),
        ]
    }

    fn visible_rows(h: i32) -> usize {
        ((h - FILES_BAR - 34 - 26) / FILES_ROW).max(1) as usize
    }

    fn selected_name(&self) -> Option<String> {
        let files = fs::list();
        self.selected.and_then(|i| files.get(i).map(|(n, _)| n.clone()))
    }

    fn create(&mut self) -> Action {
        for n in 1..1000 {
            let name = format!("new{}.txt", n);
            if fs::read(&name).is_err() {
                return match fs::write(&name, "") {
                    Ok(()) => Action::OpenFile(name),
                    Err(e) => {
                        self.message = format!("Ошибка: {}", e);
                        Action::None
                    }
                };
            }
        }
        Action::None
    }

    fn delete(&mut self) {
        if let Some(n) = self.selected_name() {
            let _ = fs::remove(&n);
            self.message = format!("Файл {} удалён", n);
            self.selected = None;
        } else {
            self.message = String::from("Сначала выберите файл");
        }
    }
}

impl App for Files {
    fn kind(&self) -> AppKind {
        AppKind::Files
    }
    fn size(&self) -> (i32, i32) {
        (560, 420)
    }
    fn draw(&mut self, c: &mut Canvas, w: i32, h: i32, _focused: bool) {
        toolbar(c, w, FILES_BAR);
        for (i, (r, icon, label)) in Self::buttons().iter().enumerate() {
            button(c, *r, Some(*icon), label, i == 0);
        }
        let files = fs::list();
        if self.selected.map_or(false, |s| s >= files.len()) {
            self.selected = None;
        }
        let top = FILES_BAR + 8;
        ui::text(c, Font::Bold, 48, top + 2, "Имя", MUTED);
        ui::text(c, Font::Bold, w - 110, top + 2, "Размер", MUTED);
        c.fill_rect(12, top + 24, w - 24, 1, LINE);
        let rows = Self::visible_rows(h);
        for (vi, (i, (name, size))) in files.iter().enumerate().skip(self.scroll).take(rows).enumerate() {
            let y = top + 30 + vi as i32 * FILES_ROW;
            if self.selected == Some(i) {
                c.round_rect(8, y, w - 16, FILES_ROW - 2, 6, ACCENT_SOFT, 255);
            }
            let icon = if name.ends_with(".rs") { Icon::SquareTerminal } else { Icon::FileText };
            ui::icon(c, icon, 20, 18, y + 5, ACCENT);
            ui::text(c, Font::Regular, 48, y + 8, name, TEXT);
            ui::text(c, Font::Regular, w - 110, y + 8, &format!("{} байт", size), MUTED);
        }
        let (count, bytes) = fs::usage();
        let left = if self.message.is_empty() {
            String::from("Двойной щелчок — открыть в Блокноте")
        } else {
            self.message.clone()
        };
        status_bar(c, w, h, &left, &format!("{} файлов, {} байт", count, bytes));
    }
    fn mouse_down(&mut self, x: i32, y: i32) -> Action {
        self.message.clear();
        for (i, (r, _, _)) in Self::buttons().iter().enumerate() {
            if r.contains(x, y) {
                return match i {
                    0 => self.selected_name().map_or_else(
                        || {
                            self.message = String::from("Сначала выберите файл");
                            Action::None
                        },
                        Action::OpenFile,
                    ),
                    1 => self.create(),
                    _ => {
                        self.delete();
                        Action::None
                    }
                };
            }
        }
        let list_top = FILES_BAR + 38;
        let n = fs::list().len();
        let hit = if y >= list_top {
            let i = ((y - list_top) / FILES_ROW) as usize + self.scroll;
            if i < n {
                Some(i)
            } else {
                None
            }
        } else {
            None
        };
        self.selected = hit;
        if double_click(&mut self.last_click, hit) {
            if let Some(name) = self.selected_name() {
                return Action::OpenFile(name);
            }
        }
        Action::None
    }
    fn key(&mut self, k: Key) -> Action {
        let n = fs::list().len();
        if n == 0 {
            return Action::None;
        }
        let rows = Self::visible_rows(self.size().1);
        match k {
            Key::Down => self.selected = Some(self.selected.map_or(0, |s| (s + 1).min(n - 1))),
            Key::Up => self.selected = Some(self.selected.map_or(0, |s| s.saturating_sub(1))),
            Key::Enter => {
                if let Some(name) = self.selected_name() {
                    return Action::OpenFile(name);
                }
            }
            Key::Delete => self.delete(),
            _ => {}
        }
        if let Some(s) = self.selected {
            if s < self.scroll {
                self.scroll = s;
            } else if s >= self.scroll + rows {
                self.scroll = s + 1 - rows;
            }
        }
        Action::None
    }
}

// ---------------------------------------------------------------------------
// Блокнот
// ---------------------------------------------------------------------------

pub struct Notepad {
    name: Option<String>,
    lines: Vec<Vec<char>>,
    row: usize,
    col: usize,
    scroll: usize,
    hscroll: usize,
    modified: bool,
    status: String,
    blink: bool,
}

const NOTE_W: i32 = 640;
const NOTE_H: i32 = 440;
const NOTE_BAR: i32 = 48;
const NOTE_PAD: i32 = 10;

impl Notepad {
    pub fn new(name: Option<String>) -> Self {
        let text = name.as_deref().and_then(|n| fs::read(n).ok()).unwrap_or_default();
        let mut lines: Vec<Vec<char>> = text.split('\n').map(|l| l.chars().collect()).collect();
        if lines.len() > 1 && lines.last().map_or(false, |l| l.is_empty()) {
            lines.pop();
        }
        if lines.is_empty() {
            lines.push(Vec::new());
        }
        Notepad {
            name,
            lines,
            row: 0,
            col: 0,
            scroll: 0,
            hscroll: 0,
            modified: false,
            status: String::from("Ctrl+S — сохранить"),
            blink: true,
        }
    }

    fn rows() -> usize {
        ((NOTE_H - NOTE_BAR - 26 - 2 * NOTE_PAD) / 16) as usize
    }

    fn cols() -> usize {
        ((NOTE_W - 2 * NOTE_PAD) / 8) as usize
    }

    fn total_len(&self) -> usize {
        self.lines.iter().map(|l| l.len() + 1).sum()
    }

    fn keep_visible(&mut self) {
        let (rows, cols) = (Self::rows(), Self::cols());
        if self.row < self.scroll {
            self.scroll = self.row;
        } else if self.row >= self.scroll + rows {
            self.scroll = self.row + 1 - rows;
        }
        if self.col < self.hscroll {
            self.hscroll = self.col;
        } else if self.col >= self.hscroll + cols {
            self.hscroll = self.col + 1 - cols;
        }
    }

    fn save(&mut self) {
        let name = match &self.name {
            Some(n) => n.clone(),
            None => (1..1000).map(|n| format!("note{}.txt", n)).find(|n| fs::read(n).is_err()).unwrap_or_default(),
        };
        let mut text = String::new();
        for (i, l) in self.lines.iter().enumerate() {
            if i > 0 {
                text.push('\n');
            }
            text.extend(l.iter());
        }
        text.push('\n');
        match fs::write(&name, &text) {
            Ok(()) => {
                self.status = format!("Сохранено в {}", name);
                self.name = Some(name);
                self.modified = false;
            }
            Err(e) => self.status = format!("Не удалось сохранить: {}", e),
        }
    }

    fn save_button() -> Rect {
        Rect::new(12, 10, 124, 28)
    }
}

impl App for Notepad {
    fn kind(&self) -> AppKind {
        AppKind::Notepad
    }
    fn title(&self) -> String {
        let name = self.name.as_deref().unwrap_or("без имени");
        format!("{}{} — Блокнот", if self.modified { "*" } else { "" }, name)
    }
    fn size(&self) -> (i32, i32) {
        (NOTE_W, NOTE_H)
    }
    fn draw(&mut self, c: &mut Canvas, w: i32, h: i32, focused: bool) {
        toolbar(c, w, NOTE_BAR);
        button(c, Self::save_button(), Some(Icon::Save), "Сохранить", true);
        let lang = if keyboard::is_russian() { "RU" } else { "EN" };
        ui::text(c, Font::Regular, 150, 17, self.name.as_deref().unwrap_or("(новый файл)"), MUTED);
        ui::text(c, Font::Bold, w - 36, 17, lang, MUTED);
        {
            let area = Rect::new(0, NOTE_BAR, w, h - NOTE_BAR - 26);
            let mut t = c.sub(area);
            for (i, line) in self.lines.iter().enumerate().skip(self.scroll).take(Self::rows()) {
                let y = NOTE_PAD + (i - self.scroll) as i32 * 16;
                for (j, ch) in line.iter().enumerate().skip(self.hscroll).take(Self::cols()) {
                    t.draw_char(NOTE_PAD + (j - self.hscroll) as i32 * 8, y, *ch, TEXT);
                }
            }
            if focused && self.blink && self.row >= self.scroll {
                let x = NOTE_PAD + (self.col.saturating_sub(self.hscroll)) as i32 * 8;
                let y = NOTE_PAD + (self.row - self.scroll) as i32 * 16;
                t.fill_rect(x, y, 2, 16, ACCENT);
            }
        }
        let pos = format!("Стр {}, кол {}", self.row + 1, self.col + 1);
        let status = self.status.clone();
        status_bar(c, w, h, &status, &pos);
    }
    fn key(&mut self, k: Key) -> Action {
        let len = self.lines[self.row].len();
        match k {
            Key::Char(ch) => {
                if self.total_len() < fs::MAX_FILE_SIZE / 2 {
                    self.lines[self.row].insert(self.col, ch);
                    self.col += 1;
                    self.modified = true;
                } else {
                    self.status = String::from("Файл слишком большой");
                }
            }
            Key::Tab => {
                for _ in 0..4 {
                    self.lines[self.row].insert(self.col, ' ');
                    self.col += 1;
                }
                self.modified = true;
            }
            Key::Enter => {
                let rest = self.lines[self.row].split_off(self.col);
                self.lines.insert(self.row + 1, rest);
                self.row += 1;
                self.col = 0;
                self.modified = true;
            }
            Key::Backspace => {
                if self.col > 0 {
                    self.col -= 1;
                    self.lines[self.row].remove(self.col);
                    self.modified = true;
                } else if self.row > 0 {
                    let line = self.lines.remove(self.row);
                    self.row -= 1;
                    self.col = self.lines[self.row].len();
                    self.lines[self.row].extend(line);
                    self.modified = true;
                }
            }
            Key::Delete => {
                if self.col < len {
                    self.lines[self.row].remove(self.col);
                    self.modified = true;
                } else if self.row + 1 < self.lines.len() {
                    let next = self.lines.remove(self.row + 1);
                    self.lines[self.row].extend(next);
                    self.modified = true;
                }
            }
            Key::Left => {
                if self.col > 0 {
                    self.col -= 1;
                } else if self.row > 0 {
                    self.row -= 1;
                    self.col = self.lines[self.row].len();
                }
            }
            Key::Right => {
                if self.col < len {
                    self.col += 1;
                } else if self.row + 1 < self.lines.len() {
                    self.row += 1;
                    self.col = 0;
                }
            }
            Key::Up if self.row > 0 => {
                self.row -= 1;
                self.col = self.col.min(self.lines[self.row].len());
            }
            Key::Down if self.row + 1 < self.lines.len() => {
                self.row += 1;
                self.col = self.col.min(self.lines[self.row].len());
            }
            Key::Home => self.col = 0,
            Key::End => self.col = len,
            Key::PageUp => {
                self.row = self.row.saturating_sub(Self::rows());
                self.col = self.col.min(self.lines[self.row].len());
            }
            Key::PageDown => {
                self.row = (self.row + Self::rows()).min(self.lines.len() - 1);
                self.col = self.col.min(self.lines[self.row].len());
            }
            Key::CtrlS => self.save(),
            _ => {}
        }
        self.blink = true;
        self.keep_visible();
        Action::None
    }
    fn mouse_down(&mut self, x: i32, y: i32) -> Action {
        if Self::save_button().contains(x, y) {
            self.save();
        } else if y > NOTE_BAR && y < NOTE_H - 26 {
            let r = ((y - NOTE_BAR - NOTE_PAD).max(0) / 16) as usize + self.scroll;
            let col = ((x - NOTE_PAD).max(0) / 8) as usize + self.hscroll;
            self.row = r.min(self.lines.len() - 1);
            self.col = col.min(self.lines[self.row].len());
        }
        Action::None
    }
    fn tick(&mut self, now: u64, focused: bool) -> bool {
        let blink = (now / (time::HZ / 2)) % 2 == 0;
        let changed = focused && blink != self.blink;
        self.blink = blink;
        changed
    }
}

// ---------------------------------------------------------------------------
// Калькулятор
// ---------------------------------------------------------------------------

pub struct Calculator {
    expr: String,
    result: String,
    pressed: Option<usize>,
}

const CALC_KEYS: [&str; 20] = [
    "C", "(", ")", "/", "7", "8", "9", "*", "4", "5", "6", "-", "1", "2", "3", "+", "<", "0", "%", "=",
];
const CALC_DISPLAY: i32 = 96;

impl Calculator {
    pub fn new() -> Self {
        Calculator { expr: String::new(), result: String::from("0"), pressed: None }
    }

    fn key_rect(i: usize) -> Rect {
        let (col, row) = ((i % 4) as i32, (i / 4) as i32);
        Rect::new(12 + col * 68, CALC_DISPLAY + 10 + row * 56, 62, 50)
    }

    fn press(&mut self, k: &str) {
        match k {
            "C" => {
                self.expr.clear();
                self.result = String::from("0");
            }
            "<" => {
                self.expr.pop();
            }
            "=" => {
                if !self.expr.is_empty() {
                    self.result = match calc::eval(&self.expr) {
                        Ok(v) => format!("{}", v),
                        Err("division by zero") => String::from("деление на 0"),
                        Err("overflow") | Err("number too large") => String::from("переполнение"),
                        Err(_) => String::from("ошибка"),
                    };
                }
            }
            s => {
                if self.expr.len() < 28 {
                    self.expr.push_str(s);
                }
            }
        }
    }
}

impl App for Calculator {
    fn kind(&self) -> AppKind {
        AppKind::Calculator
    }
    fn size(&self) -> (i32, i32) {
        (12 + 4 * 68 + 6, CALC_DISPLAY + 10 + 5 * 56 + 6)
    }
    fn draw(&mut self, c: &mut Canvas, w: i32, _h: i32, _focused: bool) {
        c.fill_rect(0, 0, w, CALC_DISPLAY, SURFACE);
        let expr = if self.expr.is_empty() { " " } else { self.expr.as_str() };
        ui::text(c, Font::Regular, w - 16 - ui::text_width(Font::Regular, expr), 18, expr, MUTED);
        let res = ui::fit(Font::Large, &self.result, w - 32);
        ui::text(c, Font::Large, w - 16 - ui::text_width(Font::Large, &res), 50, &res, TEXT);
        for (i, k) in CALC_KEYS.iter().enumerate() {
            let r = Self::key_rect(i);
            let (bg, fg) = match *k {
                "=" => (ACCENT, WHITE),
                "C" => (rgb(254, 226, 226), DANGER),
                "/" | "*" | "-" | "+" | "(" | ")" | "%" | "<" => (SURFACE, ACCENT),
                _ => (WHITE, TEXT),
            };
            let bg = if self.pressed == Some(i) { gfx::blend(bg, rgb(0, 0, 0), 30) } else { bg };
            c.round_rect(r.x, r.y, r.w, r.h, 10, BORDER, 255);
            c.round_rect(r.x + 1, r.y + 1, r.w - 2, r.h - 2, 9, bg, 255);
            let label = match *k {
                "<" => "←",
                "*" => "×",
                s => s,
            };
            ui::text_centered(c, Font::Large, r.x, r.y + (r.h - 23) / 2, r.w, label, fg);
        }
    }
    fn mouse_down(&mut self, x: i32, y: i32) -> Action {
        if let Some(i) = (0..CALC_KEYS.len()).find(|&i| Self::key_rect(i).contains(x, y)) {
            self.pressed = Some(i);
            self.press(CALC_KEYS[i]);
        }
        Action::None
    }
    fn mouse_up(&mut self, _x: i32, _y: i32) -> Action {
        self.pressed = None;
        Action::None
    }
    fn key(&mut self, k: Key) -> Action {
        match k {
            Key::Char(ch) if "0123456789+-*/%()".contains(ch) => {
                let mut b = [0u8; 4];
                self.press(ch.encode_utf8(&mut b));
            }
            Key::Char('=') | Key::Enter => self.press("="),
            Key::Backspace => self.press("<"),
            Key::Escape | Key::Delete => self.press("C"),
            _ => {}
        }
        Action::None
    }
}

// ---------------------------------------------------------------------------
// Рисование
// ---------------------------------------------------------------------------

pub struct Paint {
    pixels: Vec<u32>,
    color: Color,
    brush: i32,
    last: Option<(i32, i32)>,
}

const PAINT_W: i32 = 600;
const PAINT_H: i32 = 360;
const PAINT_BAR: i32 = 48;
const PAINT_COLORS: [Color; 10] = [
    rgb(15, 23, 42),
    rgb(255, 255, 255),
    rgb(220, 38, 38),
    rgb(249, 115, 22),
    rgb(234, 179, 8),
    rgb(22, 163, 74),
    rgb(6, 182, 212),
    rgb(37, 99, 235),
    rgb(147, 51, 234),
    rgb(120, 72, 40),
];
const BRUSHES: [i32; 3] = [1, 3, 7];

impl Paint {
    pub fn new() -> Self {
        Paint { pixels: vec![WHITE; (PAINT_W * PAINT_H) as usize], color: PAINT_COLORS[0], brush: 3, last: None }
    }
    fn swatch(i: usize) -> Rect {
        Rect::new(12 + i as i32 * 30, 12, 24, 24)
    }
    fn brush_rect(i: usize) -> Rect {
        Rect::new(330 + i as i32 * 34, 10, 28, 28)
    }
    fn clear_rect() -> Rect {
        Rect::new(PAINT_W - 120, 10, 108, 28)
    }
    fn stroke(&mut self, x: i32, y: i32) {
        let (cx, cy) = (x, y - PAINT_BAR);
        let from = self.last.unwrap_or((cx, cy));
        let (color, brush) = (self.color, self.brush);
        let mut canvas = Canvas::new(&mut self.pixels, PAINT_W as usize, PAINT_H as usize);
        canvas.thick_line(from.0, from.1, cx, cy, brush, color);
        self.last = Some((cx, cy));
    }
}

impl App for Paint {
    fn kind(&self) -> AppKind {
        AppKind::Paint
    }
    fn size(&self) -> (i32, i32) {
        (PAINT_W, PAINT_H + PAINT_BAR)
    }
    fn draw(&mut self, c: &mut Canvas, w: i32, _h: i32, _focused: bool) {
        toolbar(c, w, PAINT_BAR);
        for (i, &col) in PAINT_COLORS.iter().enumerate() {
            let r = Self::swatch(i);
            if col == self.color {
                c.round_rect(r.x - 3, r.y - 3, r.w + 6, r.h + 6, 8, ACCENT, 255);
            }
            c.round_rect(r.x, r.y, r.w, r.h, 6, BORDER, 255);
            c.round_rect(r.x + 1, r.y + 1, r.w - 2, r.h - 2, 5, col, 255);
        }
        for (i, &b) in BRUSHES.iter().enumerate() {
            let r = Self::brush_rect(i);
            c.round_rect(r.x, r.y, r.w, r.h, 6, if b == self.brush { ACCENT_SOFT } else { WHITE }, 255);
            c.fill_circle(r.x + r.w / 2, r.y + r.h / 2, b, TEXT);
        }
        button(c, Self::clear_rect(), Some(Icon::Trash2), "Очистить", false);
        // холст
        for row in 0..PAINT_H {
            let line = &self.pixels[(row * PAINT_W) as usize..((row + 1) * PAINT_W) as usize];
            for (x, &p) in line.iter().enumerate() {
                c.pixel(x as i32, PAINT_BAR + row, p);
            }
        }
    }
    fn mouse_down(&mut self, x: i32, y: i32) -> Action {
        if y < PAINT_BAR {
            if let Some(i) = (0..PAINT_COLORS.len()).find(|&i| Self::swatch(i).contains(x, y)) {
                self.color = PAINT_COLORS[i];
            } else if let Some(i) = (0..BRUSHES.len()).find(|&i| Self::brush_rect(i).contains(x, y)) {
                self.brush = BRUSHES[i];
            } else if Self::clear_rect().contains(x, y) {
                self.pixels.fill(WHITE);
            }
            return Action::None;
        }
        self.last = None;
        self.stroke(x, y);
        Action::None
    }
    fn mouse_drag(&mut self, x: i32, y: i32) -> Action {
        if self.last.is_some() {
            self.stroke(x, y);
        }
        Action::None
    }
    fn mouse_up(&mut self, _x: i32, _y: i32) -> Action {
        self.last = None;
        Action::None
    }
}

// ---------------------------------------------------------------------------
// Монитор системы
// ---------------------------------------------------------------------------

pub struct Monitor {
    cpu: String,
    last: u64,
}

impl Monitor {
    pub fn new() -> Self {
        Monitor { cpu: cpu::brand_string(), last: 0 }
    }
    fn close_rect(i: usize) -> Rect {
        Rect::new(420, 222 + i as i32 * 30, 96, 24)
    }
}

impl App for Monitor {
    fn kind(&self) -> AppKind {
        AppKind::Monitor
    }
    fn size(&self) -> (i32, i32) {
        (540, 420)
    }
    fn draw(&mut self, c: &mut Canvas, w: i32, _h: i32, _focused: bool) {
        let s = heap::stats();
        let secs = time::uptime_ms() / 1000;
        // карточки
        let cards = [
            (Icon::Cpu, "Процессор", ui::fit(Font::Bold, &self.cpu, 186)),
            (Icon::Clock, "Время работы", format!("{} ч {:02} мин {:02} с", secs / 3600, (secs / 60) % 60, secs % 60)),
        ];
        for (i, (icon, title, value)) in cards.iter().enumerate() {
            let x = 16 + i as i32 * 258;
            c.round_rect(x, 16, 250, 64, 10, SURFACE, 255);
            ui::icon(c, *icon, 24, x + 14, 36, ACCENT);
            ui::text(c, Font::Regular, x + 50, 26, title, MUTED);
            ui::text(c, Font::Bold, x + 50, 46, value, TEXT);
        }
        // память
        c.round_rect(16, 92, w - 32, 84, 10, SURFACE, 255);
        ui::icon(c, Icon::MemoryStick, 24, 30, 106, ACCENT);
        ui::text(c, Font::Regular, 66, 102, "Память ядра", MUTED);
        let used_kb = s.used / 1024;
        let total_kb = (s.size / 1024).max(1);
        ui::text(c, Font::Bold, 66, 122, &format!("{} КиБ из {} МиБ", used_kb, total_kb / 1024), TEXT);
        let bar_w = w - 64;
        let fill = ((used_kb as u64 * bar_w as u64) / total_kb as u64).max(2) as i32;
        c.round_rect(32, 150, bar_w, 10, 5, LINE, 255);
        c.round_rect(32, 150, fill.min(bar_w), 10, 5, ACCENT, 255);
        // окна
        ui::text(c, Font::Bold, 16, 194, "Открытые окна", TEXT);
        let list = OPEN_WINDOWS.lock().clone();
        for (i, (_, kind, title)) in list.iter().enumerate().take(6) {
            let y = 222 + i as i32 * 30;
            ui::icon(c, kind.icon(), 16, 20, y + 4, MUTED);
            ui::text(c, Font::Regular, 46, y + 4, &ui::fit(Font::Regular, title, 360), TEXT);
            if *kind != AppKind::Monitor {
                button(c, Self::close_rect(i), None, "Закрыть", false);
            }
        }
    }
    fn mouse_down(&mut self, x: i32, y: i32) -> Action {
        let list = OPEN_WINDOWS.lock().clone();
        for (i, (id, kind, _)) in list.iter().enumerate().take(6) {
            if *kind != AppKind::Monitor && Self::close_rect(i).contains(x, y) {
                return Action::CloseWindow(*id);
            }
        }
        Action::None
    }
    fn tick(&mut self, now: u64, _focused: bool) -> bool {
        if now >= self.last + time::HZ / 2 {
            self.last = now;
            true
        } else {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Настройки
// ---------------------------------------------------------------------------

pub struct Settings {
    cpu: String,
}

impl Settings {
    pub fn new() -> Self {
        Settings { cpu: cpu::brand_string() }
    }
    fn wallpaper_rect(i: usize) -> Rect {
        Rect::new(16 + i as i32 * 118, 44, 108, 64)
    }
    fn anim_rect() -> Rect {
        Rect::new(440, 148, 48, 26)
    }
    fn lang_rect() -> Rect {
        Rect::new(400, 196, 88, 28)
    }
}

impl App for Settings {
    fn kind(&self) -> AppKind {
        AppKind::Settings
    }
    fn size(&self) -> (i32, i32) {
        (504, 400)
    }
    fn draw(&mut self, c: &mut Canvas, w: i32, _h: i32, _focused: bool) {
        let (wallpaper, animations) = {
            let s = SETTINGS.lock();
            (s.wallpaper, s.animations)
        };
        ui::text(c, Font::Bold, 16, 16, "Обои", TEXT);
        for (i, (name, top, bottom)) in WALLPAPERS.iter().enumerate() {
            let r = Self::wallpaper_rect(i);
            if i == wallpaper {
                c.round_rect(r.x - 3, r.y - 3, r.w + 6, r.h + 6, 10, ACCENT, 255);
            }
            let mut s = c.sub(r);
            s.gradient(0, 0, r.w, r.h, *top, *bottom);
            ui::text_centered(c, Font::Regular, r.x, r.y + r.h + 6, r.w, name, if i == wallpaper { ACCENT } else { MUTED });
            if i == wallpaper {
                ui::icon(c, Icon::Check, 20, r.x + r.w - 26, r.y + 6, WHITE);
            }
        }
        c.fill_rect(16, 136, w - 32, 1, LINE);
        // анимации
        ui::icon(c, Icon::Sparkles, 20, 16, 151, ACCENT);
        ui::text(c, Font::Regular, 46, 153, "Анимации окон и меню", TEXT);
        let r = Self::anim_rect();
        c.round_rect(r.x, r.y, r.w, r.h, 13, if animations { ACCENT } else { BORDER }, 255);
        let knob = if animations { r.x + r.w - 24 } else { r.x + 2 };
        c.round_rect(knob, r.y + 2, 22, 22, 11, WHITE, 255);
        // раскладка
        ui::icon(c, Icon::Keyboard, 20, 16, 199, ACCENT);
        ui::text(c, Font::Regular, 46, 201, "Раскладка (Alt+Shift)", TEXT);
        let lang = if keyboard::is_russian() { "Русская" } else { "English" };
        button(c, Self::lang_rect(), None, lang, false);
        c.fill_rect(16, 240, w - 32, 1, LINE);
        // о системе
        ui::text(c, Font::Bold, 16, 254, "О системе", TEXT);
        let s = heap::stats();
        let fb = gfx::info();
        let rows = [
            ("Версия", format!("Linoves {} ({})", crate::VERSION, crate::CODENAME)),
            ("Процессор", ui::fit(Font::Regular, &self.cpu, 330)),
            ("Память ядра", format!("{} МиБ", s.size / (1024 * 1024))),
            ("Экран", fb.map_or(String::from("—"), |f| format!("{} × {}", f.width, f.height))),
        ];
        for (i, (k, v)) in rows.iter().enumerate() {
            let y = 280 + i as i32 * 24;
            ui::text(c, Font::Regular, 16, y, k, MUTED);
            ui::text(c, Font::Regular, 140, y, v, TEXT);
        }
    }
    fn mouse_down(&mut self, x: i32, y: i32) -> Action {
        for i in 0..WALLPAPERS.len() {
            if Self::wallpaper_rect(i).contains(x, y) {
                SETTINGS.lock().wallpaper = i;
                return Action::Redraw;
            }
        }
        if Self::anim_rect().contains(x, y) {
            let mut s = SETTINGS.lock();
            s.animations = !s.animations;
            return Action::Redraw;
        }
        if Self::lang_rect().contains(x, y) {
            keyboard::toggle_layout();
            return Action::Redraw;
        }
        Action::None
    }
}

// ---------------------------------------------------------------------------
// Змейка
// ---------------------------------------------------------------------------

const SN_COLS: i32 = 24;
const SN_ROWS: i32 = 16;
const SN_CELL: i32 = 20;
const SN_BAR: i32 = 40;
const SN_STEP: u64 = 12; // тиков таймера на шаг

#[derive(PartialEq, Eq, Clone, Copy)]
enum SnakeState {
    Ready,
    Playing,
    Over,
}

pub struct Snake {
    body: Vec<(i32, i32)>,
    dir: (i32, i32),
    next_dir: (i32, i32),
    food: (i32, i32),
    score: u32,
    best: u32,
    state: SnakeState,
    last: u64,
    rng: Rng,
}

impl Snake {
    pub fn new() -> Self {
        let mut s = Snake {
            body: Vec::new(),
            dir: (1, 0),
            next_dir: (1, 0),
            food: (0, 0),
            score: 0,
            best: 0,
            state: SnakeState::Ready,
            last: 0,
            rng: Rng::new(),
        };
        s.reset();
        s
    }

    fn reset(&mut self) {
        self.body = vec![(6, 8), (5, 8), (4, 8)];
        self.dir = (1, 0);
        self.next_dir = (1, 0);
        self.score = 0;
        self.place_food();
    }

    fn place_food(&mut self) {
        for _ in 0..1000 {
            let f = ((self.rng.next() % SN_COLS as u64) as i32, (self.rng.next() % SN_ROWS as u64) as i32);
            if !self.body.contains(&f) {
                self.food = f;
                return;
            }
        }
    }

    fn step(&mut self) {
        self.dir = self.next_dir;
        let (hx, hy) = self.body[0];
        let head = (hx + self.dir.0, hy + self.dir.1);
        let hit_wall = head.0 < 0 || head.1 < 0 || head.0 >= SN_COLS || head.1 >= SN_ROWS;
        if hit_wall || self.body[..self.body.len() - 1].contains(&head) {
            self.state = SnakeState::Over;
            self.best = self.best.max(self.score);
            return;
        }
        self.body.insert(0, head);
        if head == self.food {
            self.score += 1;
            self.place_food();
        } else {
            self.body.pop();
        }
    }

    fn start(&mut self) {
        if self.state == SnakeState::Over {
            self.reset();
        }
        self.state = SnakeState::Playing;
        self.last = time::ticks();
    }
}

impl App for Snake {
    fn kind(&self) -> AppKind {
        AppKind::Snake
    }
    fn size(&self) -> (i32, i32) {
        (SN_COLS * SN_CELL, SN_ROWS * SN_CELL + SN_BAR)
    }
    fn draw(&mut self, c: &mut Canvas, w: i32, h: i32, _focused: bool) {
        toolbar(c, w, SN_BAR);
        ui::text(c, Font::Bold, 14, 12, &format!("Счёт: {}", self.score), TEXT);
        let best = format!("Рекорд: {}", self.best);
        ui::text(c, Font::Regular, w - 14 - ui::text_width(Font::Regular, &best), 12, &best, MUTED);
        let field = rgb(240, 253, 244);
        c.fill_rect(0, SN_BAR, w, h - SN_BAR, field);
        for y in 0..SN_ROWS {
            for x in 0..SN_COLS {
                if (x + y) % 2 == 0 {
                    c.fill_rect(x * SN_CELL, SN_BAR + y * SN_CELL, SN_CELL, SN_CELL, rgb(220, 252, 231));
                }
            }
        }
        let (fx, fy) = self.food;
        c.fill_circle(fx * SN_CELL + SN_CELL / 2, SN_BAR + fy * SN_CELL + SN_CELL / 2, 7, rgb(220, 38, 38));
        for (i, &(x, y)) in self.body.iter().enumerate() {
            let color = if i == 0 { rgb(21, 128, 61) } else { rgb(34, 197, 94) };
            c.round_rect(x * SN_CELL + 1, SN_BAR + y * SN_CELL + 1, SN_CELL - 2, SN_CELL - 2, 5, color, 255);
        }
        if self.state != SnakeState::Playing {
            c.fill_rect_alpha(0, SN_BAR, w, h - SN_BAR, WHITE, 170);
            let (title, hint) = match self.state {
                SnakeState::Over => ("Игра окончена", "Enter или щелчок — новая игра"),
                _ => ("Змейка", "Нажмите стрелку или щёлкните, чтобы начать"),
            };
            ui::icon(c, Icon::Gamepad2, 48, (w - 48) / 2, SN_BAR + 70, ACCENT);
            ui::text_centered(c, Font::Large, 0, SN_BAR + 130, w, title, TEXT);
            ui::text_centered(c, Font::Regular, 0, SN_BAR + 162, w, hint, MUTED);
        }
    }
    fn key(&mut self, k: Key) -> Action {
        let d = match k {
            Key::Up => Some((0, -1)),
            Key::Down => Some((0, 1)),
            Key::Left => Some((-1, 0)),
            Key::Right => Some((1, 0)),
            Key::Enter | Key::Char(' ') => {
                if self.state != SnakeState::Playing {
                    self.start();
                }
                None
            }
            _ => None,
        };
        if let Some(d) = d {
            if self.state != SnakeState::Playing {
                self.start();
            }
            if d.0 != -self.dir.0 || d.1 != -self.dir.1 {
                self.next_dir = d;
            }
        }
        Action::None
    }
    fn mouse_down(&mut self, _x: i32, _y: i32) -> Action {
        if self.state != SnakeState::Playing {
            self.start();
        }
        Action::None
    }
    fn tick(&mut self, now: u64, focused: bool) -> bool {
        if self.state != SnakeState::Playing || !focused {
            if self.state == SnakeState::Playing {
                self.last = now; // пауза, пока окно не активно
            }
            return false;
        }
        if now >= self.last + SN_STEP {
            self.last = now;
            self.step();
            return true;
        }
        false
    }
}

/// Текущее время для панели задач.
pub fn clock_strings() -> (String, String) {
    let now = rtc::now();
    (
        format!("{:02}:{:02}", now.hour, now.minute),
        format!("{:02}.{:02}.{}", now.day, now.month, now.year),
    )
}
