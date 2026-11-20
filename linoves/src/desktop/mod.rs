//! Рабочий стол Linoves. Всё рисуется кодом, значки — Lucide.
//! Расположение элементов — по наброску владельца проекта (`docs/sketch.png`):
//! «Корзина» слева вверху, панель задач внизу, меню с колонкой программ,
//! вертикальной полосой «Приложения» и колонкой пользователя.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::gfx::ui::{self, Font, Icon};
use crate::gfx::{self, rgb, Canvas, Color, FbInfo, Rect};
use crate::input::{self, Key};
use crate::mouse::{self, MouseEvent};
use crate::{cpu, fs, power, rtc, time};

// ---------------------------------------------------------------------------
// Цвета и размеры
// ---------------------------------------------------------------------------

const WHITE: Color = rgb(255, 255, 255);
const BLACK: Color = rgb(0, 0, 0);
const ACCENT: Color = rgb(37, 99, 235);
const ACCENT_SOFT: Color = rgb(224, 234, 255);
const TEXT: Color = rgb(15, 23, 42);
const MUTED: Color = rgb(100, 116, 139);
const BORDER: Color = rgb(203, 213, 225);
const PANEL: Color = rgb(248, 250, 252);
const STRIP: Color = rgb(236, 241, 247);
const TASKBAR_BG: Color = rgb(15, 23, 42);
const DANGER: Color = rgb(232, 17, 35);

const TASKBAR_H: i32 = 48;
const TASK_BTN: i32 = 40;
const TITLE_H: i32 = 36;
const MENU_W: i32 = 520;
const MENU_H: i32 = 404;
const ROW_H: i32 = 38;

static RUNNING: AtomicBool = AtomicBool::new(false);

pub fn is_running() -> bool {
    RUNNING.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Пункты
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Item {
    Welcome,
    Files,
    Mail,
    Browser,
    Apps,
    User,
    Settings,
    Documents,
    Images,
    Sleep,
    Reboot,
    Shutdown,
    Bin,
}

impl Item {
    fn title(self) -> &'static str {
        match self {
            Item::Welcome => "Добро пожаловать!",
            Item::Files => "Файлы",
            Item::Mail => "Почта",
            Item::Browser => "Браузер",
            Item::Apps => "Приложения",
            Item::User => "Пользователь 1",
            Item::Settings => "Настройки",
            Item::Documents => "Документы",
            Item::Images => "Изображение",
            Item::Sleep => "Спящий режим",
            Item::Reboot => "Перезагрузка",
            Item::Shutdown => "Выключить",
            Item::Bin => "Корзина",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Item::Welcome => Icon::Sparkles,
            Item::Files => Icon::Folder,
            Item::Mail => Icon::Mail,
            Item::Browser => Icon::Globe,
            Item::Apps => Icon::LayoutGrid,
            Item::User => Icon::CircleUser,
            Item::Settings => Icon::Settings,
            Item::Documents => Icon::FileText,
            Item::Images => Icon::Image,
            Item::Sleep => Icon::Moon,
            Item::Reboot => Icon::RotateCcw,
            Item::Shutdown => Icon::Power,
            Item::Bin => Icon::Trash2,
        }
    }

    fn window_size(self) -> (i32, i32) {
        match self {
            Item::Files => (520, 360),
            _ => (460, 300),
        }
    }
}

/// Раскладка меню: области пунктов относительно левого верхнего угла меню.
fn menu_layout() -> Vec<(Item, Rect)> {
    let mut v = Vec::new();
    // левая колонка
    v.push((Item::Welcome, Rect::new(12, 12, 210, 64)));
    for (i, item) in [Item::Files, Item::Mail, Item::Browser].iter().enumerate() {
        v.push((*item, Rect::new(12, 88 + i as i32 * (ROW_H + 2), 210, ROW_H)));
    }
    // вертикальная полоса «Приложения»
    v.push((Item::Apps, Rect::new(230, 12, 40, MENU_H - 24)));
    // правая колонка
    v.push((Item::User, Rect::new(278, 12, 230, 104)));
    for (i, item) in [Item::Settings, Item::Documents, Item::Images].iter().enumerate() {
        v.push((*item, Rect::new(278, 124 + i as i32 * (ROW_H + 2), 230, ROW_H)));
    }
    for (i, item) in [Item::Sleep, Item::Reboot, Item::Shutdown].iter().enumerate() {
        v.push((*item, Rect::new(278, 258 + i as i32 * (ROW_H + 2), 230, ROW_H)));
    }
    v
}

// ---------------------------------------------------------------------------
// Окна
// ---------------------------------------------------------------------------

struct Window {
    id: u32,
    item: Item,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    minimized: bool,
}

impl Window {
    fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.w, self.h)
    }
    fn close_rect(&self) -> Rect {
        Rect::new(self.x + self.w - 46, self.y, 46, TITLE_H)
    }
    fn min_rect(&self) -> Rect {
        Rect::new(self.x + self.w - 92, self.y, 46, TITLE_H)
    }
}

/// Что находится под курсором (для подсветки).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Hover {
    None,
    Start,
    Task(u32),
    Menu(Item),
    Close(u32),
    Min(u32),
    Bin,
}

struct Desktop {
    fb: FbInfo,
    w: i32,
    h: i32,
    back: Vec<u32>,
    wallpaper: Vec<u32>,
    windows: Vec<Window>, // последнее — верхнее
    next_id: u32,
    menu_open: bool,
    hover: Hover,
    bin_selected: bool,
    mx: i32,
    my: i32,
    buttons: u8,
    drag: Option<(u32, i32, i32)>,
    last_click: (bool, u64),
    cursor_at: (i32, i32),
    dirty: bool,
    minute: u8,
    last_tick: u64,
}

pub fn run() -> ! {
    let fb = gfx::info().expect("graphics not initialised");
    RUNNING.store(true, Ordering::SeqCst);
    crate::console::set_gui(true);
    let (w, h) = (fb.width as i32, fb.height as i32);
    let mut d = Desktop {
        fb,
        w,
        h,
        back: vec![0; fb.width * fb.height],
        wallpaper: vec![0; fb.width * fb.height],
        windows: Vec::new(),
        next_id: 1,
        menu_open: false,
        hover: Hover::None,
        bin_selected: false,
        mx: w / 2,
        my: h / 2,
        buttons: 0,
        drag: None,
        last_click: (false, 0),
        cursor_at: (0, 0),
        dirty: true,
        minute: 255,
        last_tick: 0,
    };
    d.paint_wallpaper();
    d.main_loop()
}

impl Desktop {
    fn main_loop(&mut self) -> ! {
        loop {
            let mut moved = false;
            let mut busy = false;
            while let Some(k) = input::try_pop() {
                busy = true;
                self.on_key(k);
            }
            while let Some(ev) = mouse::try_pop() {
                busy = true;
                self.on_mouse(ev, &mut moved);
            }
            let ticks = time::ticks();
            if ticks >= self.last_tick + time::HZ {
                self.last_tick = ticks;
                if rtc::now().minute != self.minute {
                    self.dirty = true;
                }
            }
            if moved {
                let hover = self.hit_hover();
                if hover != self.hover {
                    self.hover = hover;
                    self.dirty = true;
                }
            }
            if self.dirty {
                self.dirty = false;
                self.render();
                self.present();
            } else if moved {
                self.move_cursor();
            } else if !busy {
                cpu::halt();
            }
        }
    }

    // ------------------------------------------------------------------
    // Окна и действия
    // ------------------------------------------------------------------

    fn activate(&mut self, item: Item) {
        match item {
            Item::Reboot => power::reboot(),
            Item::Shutdown => power::shutdown(),
            Item::Sleep => return self.sleep(),
            _ => {}
        }
        if let Some(i) = self.windows.iter().position(|w| w.item == item) {
            self.windows[i].minimized = false;
            self.raise(i);
            return;
        }
        let (ww, wh) = item.window_size();
        let n = self.windows.len() as i32;
        let x = (240 + (n % 8) * 32).min((self.w - ww).max(0));
        let y = (60 + (n % 8) * 32).min((self.h - TASKBAR_H - wh).max(0));
        let id = self.next_id;
        self.next_id += 1;
        self.windows.push(Window { id, item, x, y, w: ww, h: wh, minimized: false });
        self.dirty = true;
    }

    fn raise(&mut self, i: usize) {
        let w = self.windows.remove(i);
        self.windows.push(w);
        self.dirty = true;
    }

    fn index_of(&self, id: u32) -> Option<usize> {
        self.windows.iter().position(|w| w.id == id)
    }

    fn top_visible(&self) -> Option<usize> {
        self.windows.iter().rposition(|w| !w.minimized)
    }

    fn sleep(&mut self) {
        self.menu_open = false;
        self.back.fill(0);
        let (w, h) = (self.w as usize, self.h as usize);
        gfx::present_rect(&self.fb, &self.back, w, 0, 0, w, h);
        while input::try_pop().is_some() {}
        while mouse::try_pop().is_some() {}
        loop {
            if input::try_pop().is_some() {
                break;
            }
            if let Some(ev) = mouse::try_pop() {
                if ev.buttons != 0 {
                    self.buttons = ev.buttons;
                    break;
                }
            }
            cpu::halt();
        }
        self.dirty = true;
    }

    // ------------------------------------------------------------------
    // Геометрия
    // ------------------------------------------------------------------

    fn menu_origin(&self) -> (i32, i32) {
        (8, self.h - TASKBAR_H - 8 - MENU_H)
    }

    fn start_rect(&self) -> Rect {
        Rect::new(6, self.h - TASKBAR_H + 4, TASK_BTN, TASK_BTN)
    }

    /// Кнопки окон на панели задач (в порядке открытия).
    fn task_buttons(&self) -> Vec<(u32, Rect)> {
        let mut ids: Vec<u32> = self.windows.iter().map(|w| w.id).collect();
        ids.sort_unstable();
        ids.iter()
            .enumerate()
            .map(|(i, &id)| {
                (id, Rect::new(54 + i as i32 * (TASK_BTN + 4), self.h - TASKBAR_H + 4, TASK_BTN, TASK_BTN))
            })
            .collect()
    }

    fn bin_rect(&self) -> Rect {
        Rect::new(12, 12, 84, 82)
    }

    fn hit_hover(&self) -> Hover {
        let (x, y) = (self.mx, self.my);
        if self.menu_open {
            let (ox, oy) = self.menu_origin();
            for (item, r) in menu_layout() {
                if Rect::new(r.x + ox, r.y + oy, r.w, r.h).contains(x, y) {
                    return Hover::Menu(item);
                }
            }
        }
        if y >= self.h - TASKBAR_H {
            if self.start_rect().contains(x, y) {
                return Hover::Start;
            }
            for (id, r) in self.task_buttons() {
                if r.contains(x, y) {
                    return Hover::Task(id);
                }
            }
            return Hover::None;
        }
        if self.menu_open && self.menu_rect().contains(x, y) {
            return Hover::None;
        }
        for w in self.windows.iter().rev().filter(|w| !w.minimized) {
            if w.rect().contains(x, y) {
                if w.close_rect().contains(x, y) {
                    return Hover::Close(w.id);
                }
                if w.min_rect().contains(x, y) {
                    return Hover::Min(w.id);
                }
                return Hover::None;
            }
        }
        if self.bin_rect().contains(x, y) {
            return Hover::Bin;
        }
        Hover::None
    }

    fn menu_rect(&self) -> Rect {
        let (ox, oy) = self.menu_origin();
        Rect::new(ox, oy, MENU_W, MENU_H)
    }

    // ------------------------------------------------------------------
    // Ввод
    // ------------------------------------------------------------------

    fn on_key(&mut self, k: Key) {
        if k == Key::Escape && self.menu_open {
            self.menu_open = false;
            self.dirty = true;
        }
    }

    fn on_mouse(&mut self, ev: MouseEvent, moved: &mut bool) {
        if ev.dx != 0 || ev.dy != 0 {
            self.mx = (self.mx + ev.dx).clamp(0, self.w - 1);
            self.my = (self.my + ev.dy).clamp(0, self.h - 1);
            if let Some((id, ox, oy)) = self.drag {
                if let Some(i) = self.index_of(id) {
                    let max_y = self.h - TASKBAR_H - TITLE_H;
                    let ww = self.windows[i].w;
                    self.windows[i].x = (self.mx - ox).clamp(-ww + 80, self.w - 80);
                    self.windows[i].y = (self.my - oy).clamp(0, max_y);
                    self.dirty = true;
                }
            } else {
                *moved = true;
            }
        }
        let was = self.buttons;
        self.buttons = ev.buttons;
        if ev.buttons & 1 != 0 && was & 1 == 0 {
            self.left_down();
        } else if ev.buttons & 1 == 0 && was & 1 != 0 {
            self.drag = None;
        }
    }

    fn left_down(&mut self) {
        let (mx, my) = (self.mx, self.my);
        self.dirty = true;

        if self.menu_open {
            if let Hover::Menu(item) = self.hit_hover() {
                self.menu_open = false;
                self.activate(item);
                return;
            }
            if self.menu_rect().contains(mx, my) {
                return;
            }
            self.menu_open = false;
            if self.start_rect().contains(mx, my) {
                return;
            }
        }

        if my >= self.h - TASKBAR_H {
            if self.start_rect().contains(mx, my) {
                self.menu_open = true;
                return;
            }
            for (id, r) in self.task_buttons() {
                if r.contains(mx, my) {
                    if let Some(i) = self.index_of(id) {
                        if self.top_visible() == Some(i) {
                            self.windows[i].minimized = true;
                        } else {
                            self.windows[i].minimized = false;
                            self.raise(i);
                        }
                    }
                    return;
                }
            }
            return;
        }

        for i in (0..self.windows.len()).rev() {
            if self.windows[i].minimized || !self.windows[i].rect().contains(mx, my) {
                continue;
            }
            self.raise(i);
            let top = self.windows.len() - 1;
            let win = &self.windows[top];
            if win.close_rect().contains(mx, my) {
                self.windows.remove(top);
            } else if win.min_rect().contains(mx, my) {
                self.windows[top].minimized = true;
            } else if my < win.y + TITLE_H {
                self.drag = Some((win.id, mx - win.x, my - win.y));
            }
            return;
        }

        let hit = self.bin_rect().contains(mx, my);
        self.bin_selected = hit;
        let now = time::ticks();
        if hit && self.last_click.0 && now - self.last_click.1 <= time::HZ / 2 {
            self.activate(Item::Bin);
            self.last_click = (false, 0);
        } else {
            self.last_click = (hit, now);
        }
    }

    // ------------------------------------------------------------------
    // Рисование
    // ------------------------------------------------------------------

    fn paint_wallpaper(&mut self) {
        let (w, h) = (self.w, self.h);
        let mut c = Canvas::new(&mut self.wallpaper, w as usize, h as usize);
        c.gradient(0, 0, w, h, rgb(12, 30, 74), rgb(48, 112, 208));
        // мягкое свечение
        let (cx, cy, r) = (w * 3 / 4, h * 3 / 5, h * 2 / 3);
        let mut ring = r;
        while ring > 0 {
            for dy in -ring..=ring {
                let dx = gfx::sqrt((ring * ring - dy * dy) as f32) as i32;
                c.fill_rect_alpha(cx - dx, cy + dy, 2 * dx + 1, 1, rgb(140, 190, 255), 3);
            }
            ring -= r / 12;
        }
    }

    fn render(&mut self) {
        self.back.copy_from_slice(&self.wallpaper);
        let (w, h) = (self.w, self.h);
        let hover = self.hover;
        let focused = self.top_visible().map(|i| self.windows[i].id);
        let tasks = self.task_buttons();
        let start = self.start_rect();
        let bin = self.bin_rect();
        let menu_origin = self.menu_origin();
        let bin_selected = self.bin_selected;
        let menu_open = self.menu_open;

        let mut c = Canvas::new(&mut self.back, w as usize, h as usize);

        // «Корзина»
        if bin_selected || hover == Hover::Bin {
            c.round_rect(bin.x, bin.y, bin.w, bin.h, 8, WHITE, if bin_selected { 60 } else { 35 });
        }
        ui::icon(&mut c, Icon::Trash2, 40, bin.x + (bin.w - 40) / 2 + 1, bin.y + 11, BLACK);
        ui::icon(&mut c, Icon::Trash2, 40, bin.x + (bin.w - 40) / 2, bin.y + 10, WHITE);
        ui::text_centered(&mut c, Font::Regular, bin.x + 1, bin.y + 58, bin.w, "Корзина", BLACK);
        ui::text_centered(&mut c, Font::Regular, bin.x, bin.y + 57, bin.w, "Корзина", WHITE);

        // окна
        for win in self.windows.iter().filter(|w| !w.minimized) {
            draw_window(&mut c, win, Some(win.id) == focused, hover);
        }

        // панель задач
        c.fill_rect_alpha(0, h - TASKBAR_H, w, TASKBAR_H, TASKBAR_BG, 225);
        c.fill_rect_alpha(0, h - TASKBAR_H, w, 1, WHITE, 30);
        let sa = if menu_open { 60 } else if hover == Hover::Start { 35 } else { 0 };
        if sa > 0 {
            c.round_rect(start.x, start.y, start.w, start.h, 6, WHITE, sa);
        }
        ui::icon(&mut c, Icon::AppWindow, 24, start.x + 8, start.y + 8, WHITE);

        for (id, r) in tasks.iter() {
            let Some(win) = self.windows.iter().find(|w| w.id == *id) else { continue };
            let active = focused == Some(*id);
            let a = if active { 45 } else if hover == Hover::Task(*id) { 30 } else { 0 };
            if a > 0 {
                c.round_rect(r.x, r.y, r.w, r.h, 6, WHITE, a);
            }
            ui::icon(&mut c, win.item.icon(), 24, r.x + 8, r.y + 7, WHITE);
            let (bw, color) = if active { (16, rgb(96, 165, 250)) } else { (6, rgb(148, 163, 184)) };
            c.round_rect(r.x + (r.w - bw) / 2, r.y + r.h - 4, bw, 3, 1, color, 255);
        }

        // часы и значки справа
        let now = rtc::now();
        self.minute = now.minute;
        let time_s = format!("{:02}:{:02}", now.hour, now.minute);
        let date_s = format!("{:02}.{:02}.{}", now.day, now.month, now.year);
        let cw = ui::text_width(Font::Regular, &time_s).max(ui::text_width(Font::Regular, &date_s));
        let cx = w - 12 - cw;
        let ty = h - TASKBAR_H + 7;
        ui::text(&mut c, Font::Regular, cx + cw - ui::text_width(Font::Regular, &time_s), ty, &time_s, WHITE);
        ui::text(&mut c, Font::Regular, cx + cw - ui::text_width(Font::Regular, &date_s), ty + 17, &date_s, rgb(203, 213, 225));
        ui::icon(&mut c, Icon::Signal, 16, cx - 28, h - TASKBAR_H + 16, WHITE);
        ui::icon(&mut c, Icon::PlugZap, 16, cx - 50, h - TASKBAR_H + 16, WHITE);

        if menu_open {
            draw_menu(&mut c, menu_origin, hover);
        }
    }

    fn present(&mut self) {
        let (w, h) = (self.w as usize, self.h as usize);
        gfx::present_rect(&self.fb, &self.back, w, 0, 0, w, h);
        self.draw_cursor();
    }

    fn move_cursor(&mut self) {
        let (ox, oy) = self.cursor_at;
        gfx::present_rect(&self.fb, &self.back, self.w as usize, ox as usize, oy as usize, CURSOR[0].len(), CURSOR.len());
        self.draw_cursor();
    }

    fn draw_cursor(&mut self) {
        let (x, y) = (self.mx, self.my);
        for (row, line) in CURSOR.iter().enumerate() {
            for (col, b) in line.bytes().enumerate() {
                let color = match b {
                    b'X' => BLACK,
                    b'.' => WHITE,
                    _ => continue,
                };
                gfx::put_pixel_direct(&self.fb, x + col as i32, y + row as i32, color);
            }
        }
        self.cursor_at = (x, y);
    }
}

fn draw_menu(c: &mut Canvas, (ox, oy): (i32, i32), hover: Hover) {
    // тень и панель
    for i in 1..=8 {
        c.round_rect(ox - 1 + i / 2, oy + i, MENU_W, MENU_H, 12, BLACK, 7);
    }
    c.round_rect(ox, oy, MENU_W, MENU_H, 12, BORDER, 255);
    c.round_rect(ox + 1, oy + 1, MENU_W - 2, MENU_H - 2, 11, PANEL, 255);
    let mut m = c.sub(Rect::new(ox, oy, MENU_W, MENU_H));

    for (item, r) in menu_layout() {
        let hovered = hover == Hover::Menu(item);
        match item {
            Item::Welcome => {
                m.round_rect(r.x, r.y, r.w, r.h, 8, if hovered { rgb(29, 78, 216) } else { ACCENT }, 255);
                ui::icon(&mut m, Icon::Sparkles, 24, r.x + 12, r.y + 20, WHITE);
                ui::text(&mut m, Font::Bold, r.x + 46, r.y + 14, "Добро пожаловать!", WHITE);
                let v = format!("Linoves {} ({})", crate::VERSION, crate::CODENAME);
                ui::text(&mut m, Font::Regular, r.x + 46, r.y + 34, &v, rgb(219, 234, 254));
            }
            Item::Apps => {
                m.round_rect(r.x, r.y, r.w, r.h, 8, if hovered { ACCENT_SOFT } else { STRIP }, 255);
                ui::icon(&mut m, Icon::LayoutGrid, 20, r.x + 10, r.y + 12, if hovered { ACCENT } else { MUTED });
                let lh = ui::line_height(Font::Bold);
                ui::text_vertical(&mut m, Font::Bold, r.x + (r.w - lh) / 2, r.y + r.h - 14, "Приложения", if hovered { ACCENT } else { MUTED });
            }
            Item::User => {
                if hovered {
                    m.round_rect(r.x, r.y, r.w, r.h, 8, ACCENT_SOFT, 255);
                }
                let (cx, cy) = (r.x + r.w / 2, r.y + 38);
                m.fill_circle(cx, cy, 28, rgb(226, 232, 240));
                ui::icon(&mut m, Icon::User, 40, cx - 20, cy - 20, MUTED);
                ui::text_centered(&mut m, Font::Bold, r.x, r.y + 76, r.w, "Пользователь 1", TEXT);
            }
            _ => {
                if hovered {
                    let bg = if item == Item::Shutdown { rgb(254, 226, 226) } else { ACCENT_SOFT };
                    m.round_rect(r.x, r.y, r.w, r.h, 8, bg, 255);
                }
                let color = match item {
                    Item::Shutdown if hovered => DANGER,
                    _ if hovered => ACCENT,
                    _ => MUTED,
                };
                ui::icon(&mut m, item.icon(), 20, r.x + 12, r.y + (r.h - 20) / 2, color);
                ui::text(&mut m, Font::Regular, r.x + 44, r.y + (r.h - 15) / 2, item.title(), TEXT);
            }
        }
    }
    // разделитель перед кнопками питания
    m.fill_rect(290, 249, 206, 1, BORDER);
}

fn draw_window(c: &mut Canvas, win: &Window, focused: bool, hover: Hover) {
    let r = win.rect();
    for i in 1..=10 {
        c.fill_rect_alpha(r.x - 2 + i / 2, r.y + i, r.w + 4 - i, r.h, BLACK, if focused { 6 } else { 4 });
    }
    c.fill_rect(r.x, r.y, r.w, r.h, if focused { rgb(148, 163, 184) } else { BORDER });
    c.fill_rect(r.x + 1, r.y + 1, r.w - 2, r.h - 2, WHITE);
    let title_bg = if focused { WHITE } else { rgb(241, 245, 249) };
    c.fill_rect(r.x + 1, r.y + 1, r.w - 2, TITLE_H - 1, title_bg);
    c.fill_rect(r.x + 1, r.y + TITLE_H, r.w - 2, 1, rgb(226, 232, 240));
    ui::icon(c, win.item.icon(), 16, r.x + 12, r.y + 10, if focused { ACCENT } else { MUTED });
    let title = ui::fit(Font::Bold, win.item.title(), r.w - 140);
    ui::text(c, Font::Bold, r.x + 36, r.y + 10, &title, if focused { TEXT } else { MUTED });

    let mr = win.min_rect();
    if hover == Hover::Min(win.id) {
        c.fill_rect(mr.x, mr.y + 1, mr.w, mr.h - 1, rgb(226, 232, 240));
    }
    ui::icon(c, Icon::Minus, 16, mr.x + 15, mr.y + 10, TEXT);
    let cr = win.close_rect();
    let close_hover = hover == Hover::Close(win.id);
    if close_hover {
        c.fill_rect(cr.x, cr.y + 1, cr.w - 1, cr.h - 1, DANGER);
    }
    ui::icon(c, Icon::X, 16, cr.x + 15, cr.y + 10, if close_hover { WHITE } else { TEXT });

    let client = Rect::new(r.x + 1, r.y + TITLE_H + 1, r.w - 2, r.h - TITLE_H - 2);
    let mut s = c.sub(client);
    draw_content(&mut s, win.item, client.w, client.h);
}

/// Содержимое окна.
fn draw_content(c: &mut Canvas, item: Item, w: i32, h: i32) {
    match item {
        Item::Files => {
            let files = fs::list();
            ui::text(c, Font::Bold, 16, 14, "RAM-диск", TEXT);
            let info = format!("файлов: {}", files.len());
            ui::text(c, Font::Regular, 16 + ui::text_width(Font::Bold, "RAM-диск") + 10, 15, &info, MUTED);
            c.fill_rect(16, 40, w - 32, 1, rgb(226, 232, 240));
            for (i, (name, size)) in files.iter().enumerate() {
                let y = 48 + i as i32 * 34;
                if y + 34 > h {
                    break;
                }
                ui::icon(c, Icon::FileText, 20, 20, y + 7, ACCENT);
                ui::text(c, Font::Regular, 52, y + 10, name, TEXT);
                let sz = format!("{} байт", size);
                ui::text(c, Font::Regular, w - 20 - ui::text_width(Font::Regular, &sz), y + 10, &sz, MUTED);
            }
        }
        Item::Welcome => {
            let lines: [(Font, String, Color); 5] = [
                (Font::Large, String::from("Добро пожаловать в Linoves!"), TEXT),
                (Font::Regular, format!("Версия {} ({})", crate::VERSION, crate::CODENAME), MUTED),
                (Font::Regular, String::new(), MUTED),
                (Font::Regular, String::from("Меню открывается кнопкой в левом нижнем углу."), TEXT),
                (Font::Regular, String::from("Окна можно перетаскивать за заголовок."), TEXT),
            ];
            empty_state(c, w, h, Icon::Sparkles, ACCENT, &lines);
        }
        Item::Bin => {
            let lines = [(Font::Large, String::from("Корзина пуста"), TEXT)];
            empty_state(c, w, h, Icon::Trash2, MUTED, &lines);
        }
        Item::User => {
            let lines = [
                (Font::Large, String::from("Пользователь 1"), TEXT),
                (Font::Regular, String::from("Учётная запись этого компьютера"), MUTED),
            ];
            empty_state(c, w, h, Icon::CircleUser, ACCENT, &lines);
        }
        other => {
            let lines = [
                (Font::Large, String::from(other.title()), TEXT),
                (Font::Regular, String::from("Здесь пока ничего нет"), MUTED),
            ];
            empty_state(c, w, h, other.icon(), MUTED, &lines);
        }
    }
}

fn empty_state(c: &mut Canvas, w: i32, h: i32, icon: Icon, icon_color: Color, lines: &[(Font, String, Color)]) {
    let text_h: i32 = lines.iter().map(|(f, _, _)| ui::line_height(*f) + 6).sum();
    let total = 48 + 16 + text_h;
    let mut y = (h - total) / 2;
    ui::icon(c, icon, 48, (w - 48) / 2, y, icon_color);
    y += 64;
    for (f, s, color) in lines {
        ui::text_centered(c, *f, 0, y, w, s, *color);
        y += ui::line_height(*f) + 6;
    }
}

/// Курсор-стрелка: X — контур, . — заливка.
const CURSOR: [&str; 19] = [
    "X          ",
    "XX         ",
    "X.X        ",
    "X..X       ",
    "X...X      ",
    "X....X     ",
    "X.....X    ",
    "X......X   ",
    "X.......X  ",
    "X........X ",
    "X.....XXXXX",
    "X..X..X    ",
    "X.X X..X   ",
    "XX  X..X   ",
    "X    X..X  ",
    "     X..X  ",
    "      X..X ",
    "      X..X ",
    "       XX  ",
];
