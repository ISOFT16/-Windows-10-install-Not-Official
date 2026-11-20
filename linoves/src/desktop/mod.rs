//! Рабочий стол. Внешний вид — рисунок владельца проекта `theme/desktop.png`
//! (панель задач, меню, значок «Корзина»), встроенный пиксель в пиксель
//! через `tools/gen_theme.py`. Здесь только механика: где что нажимается
//! и что при этом происходит.
//!
//! Нижние элементы (панель задач и меню) привязаны к нижнему краю экрана,
//! «Корзина» — к верхнему левому углу.

mod theme_data;

use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::gfx::{self, rgb, Canvas, Color, FbInfo, Rect};
use crate::input::{self, Key};
use crate::mouse::{self, MouseEvent};
use crate::{cpu, power, rtc, time};
use theme_data as t;

const WHITE: Color = rgb(255, 255, 255);
const BLACK: Color = rgb(0, 0, 0);

static RUNNING: AtomicBool = AtomicBool::new(false);

pub fn is_running() -> bool {
    RUNNING.load(Ordering::Relaxed)
}

/// Значок из рисунка (оттенки серого).
#[derive(Clone, Copy)]
struct Sprite {
    data: &'static [u8],
    w: usize,
    h: usize,
}

const ICON_WELCOME: Sprite = Sprite { data: &t::ICON_WELCOME, w: t::ICON_WELCOME_W, h: t::ICON_WELCOME_H };
const ICON_FILES: Sprite = Sprite { data: &t::ICON_FILES, w: t::ICON_FILES_W, h: t::ICON_FILES_H };
const ICON_MAIL: Sprite = Sprite { data: &t::ICON_MAIL, w: t::ICON_MAIL_W, h: t::ICON_MAIL_H };
const ICON_BROWSER: Sprite = Sprite { data: &t::ICON_BROWSER, w: t::ICON_BROWSER_W, h: t::ICON_BROWSER_H };

fn draw_gray(c: &mut Canvas, x: i32, y: i32, data: &[u8], w: usize, h: usize, alpha: Option<&[u8]>) {
    for row in 0..h {
        for col in 0..w {
            let i = row * w + col;
            if let Some(a) = alpha {
                if a[i] == 0 {
                    continue;
                }
            }
            let v = data[i];
            c.pixel(x + col as i32, y + row as i32, rgb(v, v, v));
        }
    }
}

/// Что открывается щелчком: пункты меню и «Корзина».
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
    UserName,
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
            Item::User | Item::UserName => "Пользователь 1",
            Item::Settings => "Настройки",
            Item::Documents => "Документы",
            Item::Images => "Изображение",
            Item::Sleep => "Спящий режим",
            Item::Reboot => "Перезагрузка",
            Item::Shutdown => "Выключить",
            Item::Bin => "Корзина",
        }
    }

    fn icon(self) -> Option<Sprite> {
        match self {
            Item::Welcome => Some(ICON_WELCOME),
            Item::Files => Some(ICON_FILES),
            Item::Mail => Some(ICON_MAIL),
            Item::Browser => Some(ICON_BROWSER),
            _ => None,
        }
    }
}

/// Области пунктов меню на рисунке: (x0, y0, x1, y1) в координатах рисунка.
const MENU_ITEMS: [(i32, i32, i32, i32, Item); 13] = [
    (0, 279, 112, 308, Item::Welcome),
    (0, 308, 112, 334, Item::Files),
    (0, 334, 112, 359, Item::Mail),
    (0, 359, 112, 385, Item::Browser),
    (112, 279, 133, 510, Item::Apps),
    (133, 313, 207, 382, Item::User),
    (133, 382, 244, 404, Item::Settings),
    (133, 404, 244, 427, Item::Documents),
    (133, 427, 244, 449, Item::Images),
    (133, 449, 232, 466, Item::Sleep),
    (133, 466, 232, 481, Item::Reboot),
    (133, 481, 232, 496, Item::Shutdown),
    (133, 496, 232, 510, Item::UserName),
];

const WIN_TITLE_H: i32 = 24;
const WIN_BORDER: i32 = 2;

/// Окно. Содержимое окон ещё не нарисовано: окно пустое, с заголовком и
/// кнопкой закрытия в том же стиле линий, что и рисунок.
struct Window {
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
        Rect::new(self.x + self.w - WIN_BORDER - 22, self.y + WIN_BORDER + 2, 18, 18)
    }
}

struct Desktop {
    fb: FbInfo,
    w: i32,
    h: i32,
    /// Сдвиг нижних элементов рисунка: экран может быть выше рисунка.
    dy: i32,
    back: Vec<u32>,
    windows: Vec<Window>, // последнее — верхнее
    slots: Vec<Item>,     // кнопки на панели задач в порядке открытия
    menu_open: bool,
    mx: i32,
    my: i32,
    buttons: u8,
    drag: Option<(usize, i32, i32)>,
    last_click: (bool, u64),
    cursor_at: (i32, i32),
    dirty: bool,
    minute: u8,
    last_tick: u64,
}

/// Запустить рабочий стол (не возвращается).
pub fn run() -> ! {
    let fb = gfx::info().expect("graphics not initialised");
    RUNNING.store(true, Ordering::SeqCst);
    crate::console::set_gui(true);
    let (w, h) = (fb.width as i32, fb.height as i32);
    let mut d = Desktop {
        fb,
        w,
        h,
        dy: h - t::DESIGN_H,
        back: vec![WHITE; fb.width * fb.height],
        windows: Vec::new(),
        slots: Vec::new(),
        menu_open: false,
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
                    self.dirty = true; // часы на панели
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
    // Окна
    // ------------------------------------------------------------------

    fn open(&mut self, item: Item) {
        match item {
            Item::Reboot => power::reboot(),
            Item::Shutdown => power::shutdown(),
            Item::Sleep => {
                self.sleep();
                return;
            }
            _ => {}
        }
        if let Some(i) = self.windows.iter().position(|w| w.item == item) {
            self.windows[i].minimized = false;
            self.raise(i);
            return;
        }
        if self.slots.len() >= t::SLOTS.len() {
            return; // все кнопки на панели заняты
        }
        let n = self.windows.len() as i32;
        let (ww, wh) = (420, 280);
        let x = (300 + (n % 8) * 30).min((self.w - ww).max(0));
        let y = (60 + (n % 8) * 30).min((self.h - t::TASKBAR_H as i32 - wh).max(0));
        self.windows.push(Window { item, x, y, w: ww, h: wh, minimized: false });
        self.slots.push(item);
        self.dirty = true;
    }

    fn close(&mut self, i: usize) {
        let item = self.windows.remove(i).item;
        self.slots.retain(|&s| s != item);
        self.dirty = true;
    }

    fn raise(&mut self, i: usize) {
        let w = self.windows.remove(i);
        self.windows.push(w);
        self.dirty = true;
    }

    fn top_visible(&self) -> Option<usize> {
        self.windows.iter().rposition(|w| !w.minimized)
    }

    /// Спящий режим: экран гаснет до нажатия клавиши или кнопки мыши.
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
            if let Some((i, ox, oy)) = self.drag {
                if i < self.windows.len() {
                    let max_y = self.h - t::TASKBAR_H as i32 - WIN_TITLE_H;
                    self.windows[i].x = (self.mx - ox).clamp(-self.windows[i].w + 60, self.w - 60);
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

    fn taskbar_y(&self) -> i32 {
        self.h - t::TASKBAR_H as i32
    }

    /// Пункт меню под точкой (экранные координаты).
    fn menu_item_at(&self, x: i32, y: i32) -> Option<Item> {
        let dy = y - self.dy;
        MENU_ITEMS
            .iter()
            .find(|&&(x0, y0, x1, y1, _)| x >= x0 && x < x1 && dy >= y0 && dy < y1)
            .map(|&(_, _, _, _, item)| item)
    }

    fn menu_rect(&self) -> Rect {
        Rect::new(t::MENU_X, t::MENU_Y + self.dy, t::MENU_W as i32, t::MENU_H as i32)
    }

    fn left_down(&mut self) {
        let (mx, my) = (self.mx, self.my);
        self.dirty = true;

        if self.menu_open {
            if self.menu_rect().contains(mx, my) {
                if let Some(item) = self.menu_item_at(mx, my) {
                    self.menu_open = false;
                    self.open(item);
                }
                return;
            }
            self.menu_open = false;
        }

        if my >= self.taskbar_y() {
            if mx >= t::START.0 && mx < t::START.1 {
                self.menu_open = !self.menu_open;
                return;
            }
            for (slot, &(x0, x1)) in t::SLOTS.iter().enumerate() {
                if mx >= x0 && mx <= x1 {
                    if let Some(&item) = self.slots.get(slot) {
                        if let Some(i) = self.windows.iter().position(|w| w.item == item) {
                            if self.top_visible() == Some(i) {
                                self.windows[i].minimized = true;
                            } else {
                                self.windows[i].minimized = false;
                                self.raise(i);
                            }
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
                self.close(top);
            } else if my < win.y + WIN_TITLE_H {
                self.drag = Some((top, mx - win.x, my - win.y));
            }
            return;
        }

        // «Корзина»: двойной щелчок открывает
        let bin = Rect::new(t::BIN_X, t::BIN_Y, t::BIN_W as i32, t::BIN_H as i32);
        let now = time::ticks();
        let hit = bin.contains(mx, my);
        if hit && self.last_click.0 && now - self.last_click.1 <= time::HZ / 2 {
            self.open(Item::Bin);
            self.last_click = (false, 0);
        } else {
            self.last_click = (hit, now);
        }
    }

    // ------------------------------------------------------------------
    // Рисование
    // ------------------------------------------------------------------

    fn render(&mut self) {
        let (w, h, dy) = (self.w, self.h, self.dy);
        let tb_y = self.taskbar_y();
        let mut c = Canvas::new(&mut self.back, w as usize, h as usize);
        c.fill(WHITE);

        draw_gray(&mut c, t::BIN_X, t::BIN_Y, &t::BIN, t::BIN_W, t::BIN_H, None);

        for win in self.windows.iter().filter(|w| !w.minimized) {
            draw_window(&mut c, win);
        }

        // панель задач: левая часть рисунка, растянутое пустое поле, правая часть
        let split = (t::FIELD_X + 50) as usize;
        let right_w = t::TASKBAR_W - split;
        let right_x = (w - right_w as i32).max(split as i32);
        for row in 0..t::TASKBAR_H {
            let line = &t::TASKBAR[row * t::TASKBAR_W..(row + 1) * t::TASKBAR_W];
            let y = tb_y + row as i32;
            for (x, &v) in line[..split].iter().enumerate() {
                c.pixel(x as i32, y, rgb(v, v, v));
            }
            let fill = line[split - 1];
            c.fill_rect(split as i32, y, right_x - split as i32, 1, rgb(fill, fill, fill));
            for (x, &v) in line[split..].iter().enumerate() {
                c.pixel(right_x + x as i32, y, rgb(v, v, v));
            }
        }

        // кнопки открытых окон: значок из рисунка или первая буква названия
        for (slot, item) in self.slots.iter().enumerate() {
            let (x0, x1) = t::SLOTS[slot];
            let area = Rect::new(x0, tb_y + 5, x1 - x0 + 1, t::TASKBAR_H as i32 - 8);
            let mut s = c.sub(area);
            match item.icon() {
                Some(sp) => {
                    let ix = (area.w - sp.w as i32) / 2;
                    let iy = (area.h - sp.h as i32) / 2;
                    draw_gray(&mut s, ix, iy, sp.data, sp.w, sp.h, None);
                }
                None => {
                    let letter: alloc::string::String = item.title().chars().take(1).collect();
                    s.text_centered(0, (area.h - 16) / 2, area.w, &letter, BLACK);
                }
            }
        }

        // значки и часы справа
        let now = rtc::now();
        self.minute = now.minute;
        let mut clock = alloc::string::String::new();
        let _ = write!(clock, "{:02}:{:02}", now.hour, now.minute);
        let clock_x = w - gfx::text_width(&clock) - 6;
        draw_gray(&mut c, clock_x - 4 - t::TRAY_W as i32, t::TRAY_Y + dy, &t::TRAY, t::TRAY_W, t::TRAY_H, None);
        c.text(clock_x, tb_y + (t::TASKBAR_H as i32 - 16) / 2 + 1, &clock, BLACK);

        if self.menu_open {
            draw_gray(&mut c, t::MENU_X, t::MENU_Y + dy, &t::MENU, t::MENU_W, t::MENU_H, Some(&t::MENU_ALPHA));
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

fn draw_window(c: &mut Canvas, win: &Window) {
    let r = win.rect();
    c.fill_rect(r.x, r.y, r.w, r.h, WHITE);
    for i in 0..WIN_BORDER {
        c.rect(r.x + i, r.y + i, r.w - 2 * i, r.h - 2 * i, BLACK);
    }
    c.fill_rect(r.x, r.y + WIN_TITLE_H, r.w, WIN_BORDER, BLACK);
    c.text(r.x + 8, r.y + (WIN_TITLE_H - 16) / 2 + 1, win.item.title(), BLACK);
    let cr = win.close_rect();
    c.rect(cr.x, cr.y, cr.w, cr.h, BLACK);
    c.draw_char(cr.x + (cr.w - 8) / 2, cr.y + 1, '×', BLACK);
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
