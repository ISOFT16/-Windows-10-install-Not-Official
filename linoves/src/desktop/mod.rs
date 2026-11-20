//! Рабочий стол Linoves: окна с анимациями, меню, панель задач.
//! Всё рисуется кодом, значки — Lucide. Расположение — по наброску владельца
//! проекта (`docs/sketch.png`).

mod apps;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::gfx::ui::{self, Font, Icon};
use crate::gfx::{self, rgb, Canvas, Color, FbInfo, Rect};
use crate::input::{self, Key};
use crate::mouse::{self, MouseEvent};
use crate::sync::IrqMutex;
use crate::{cpu, keyboard, power, rtc, time};

// ---------------------------------------------------------------------------
// Цвета и размеры
// ---------------------------------------------------------------------------

const WHITE: Color = rgb(255, 255, 255);
const BLACK: Color = rgb(0, 0, 0);
pub const ACCENT: Color = rgb(37, 99, 235);
pub const ACCENT_SOFT: Color = rgb(224, 234, 255);
pub const TEXT: Color = rgb(15, 23, 42);
pub const MUTED: Color = rgb(100, 116, 139);
pub const BORDER: Color = rgb(203, 213, 225);
pub const DANGER: Color = rgb(232, 17, 35);
const PANEL: Color = rgb(248, 250, 252);
const STRIP: Color = rgb(236, 241, 247);
const TASKBAR_BG: Color = rgb(15, 23, 42);

const TASKBAR_H: i32 = 48;
const TASK_BTN: i32 = 40;
const TITLE_H: i32 = 36;
const MENU_W: i32 = 520;
const MENU_H: i32 = 404;
const ROW_H: i32 = 38;

// длительность анимаций в тиках таймера (100 в секунду)
const ANIM_OPEN: u64 = 18;
const ANIM_CLOSE: u64 = 14;
const ANIM_MINIMIZE: u64 = 20;
const ANIM_RESTORE: u64 = 18;
const ANIM_MENU: u64 = 14;

static RUNNING: AtomicBool = AtomicBool::new(false);

pub fn is_running() -> bool {
    RUNNING.load(Ordering::Relaxed)
}

/// Обои: название, цвет сверху, цвет снизу.
pub const WALLPAPERS: [(&str, Color, Color); 4] = [
    ("Синие", rgb(12, 30, 74), rgb(48, 112, 208)),
    ("Закат", rgb(49, 27, 86), rgb(234, 120, 82)),
    ("Лес", rgb(6, 46, 38), rgb(52, 140, 96)),
    ("Ночь", rgb(8, 10, 20), rgb(52, 58, 84)),
];

pub struct Settings {
    pub wallpaper: usize,
    pub animations: bool,
}

pub static SETTINGS: IrqMutex<Settings> = IrqMutex::new(Settings { wallpaper: 0, animations: true });

/// Список открытых окон (для «Монитора системы»).
pub static OPEN_WINDOWS: IrqMutex<Vec<(u32, AppKind, String)>> = IrqMutex::new(Vec::new());

// ---------------------------------------------------------------------------
// Приложения
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppKind {
    Welcome,
    Files,
    Mail,
    Browser,
    Apps,
    User,
    Settings,
    Notepad,
    Paint,
    Calculator,
    Terminal,
    Monitor,
    Snake,
    Bin,
}

impl AppKind {
    pub fn title(self) -> &'static str {
        match self {
            AppKind::Welcome => "Добро пожаловать!",
            AppKind::Files => "Файлы",
            AppKind::Mail => "Почта",
            AppKind::Browser => "Браузер",
            AppKind::Apps => "Приложения",
            AppKind::User => "Пользователь 1",
            AppKind::Settings => "Настройки",
            AppKind::Notepad => "Блокнот",
            AppKind::Paint => "Рисование",
            AppKind::Calculator => "Калькулятор",
            AppKind::Terminal => "Терминал",
            AppKind::Monitor => "Монитор системы",
            AppKind::Snake => "Змейка",
            AppKind::Bin => "Корзина",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            AppKind::Welcome => Icon::Sparkles,
            AppKind::Files => Icon::Folder,
            AppKind::Mail => Icon::Mail,
            AppKind::Browser => Icon::Globe,
            AppKind::Apps => Icon::LayoutGrid,
            AppKind::User => Icon::CircleUser,
            AppKind::Settings => Icon::Settings,
            AppKind::Notepad => Icon::NotebookPen,
            AppKind::Paint => Icon::Palette,
            AppKind::Calculator => Icon::Calculator,
            AppKind::Terminal => Icon::SquareTerminal,
            AppKind::Monitor => Icon::Activity,
            AppKind::Snake => Icon::Gamepad2,
            AppKind::Bin => Icon::Trash2,
        }
    }

    /// Можно ли открыть несколько окон этой программы.
    fn multi(self) -> bool {
        matches!(self, AppKind::Notepad | AppKind::Paint | AppKind::Calculator)
    }

    fn create(self) -> Box<dyn App> {
        match self {
            AppKind::Files => Box::new(apps::Files::new()),
            AppKind::Apps => Box::new(apps::Launcher),
            AppKind::Settings => Box::new(apps::Settings::new()),
            AppKind::Notepad => Box::new(apps::Notepad::new(None)),
            AppKind::Paint => Box::new(apps::Paint::new()),
            AppKind::Calculator => Box::new(apps::Calculator::new()),
            AppKind::Terminal => Box::new(apps::Terminal::new()),
            AppKind::Monitor => Box::new(apps::Monitor::new()),
            AppKind::Snake => Box::new(apps::Snake::new()),
            other => Box::new(apps::Info::new(other)),
        }
    }
}

/// Что программа просит сделать рабочий стол.
pub enum Action {
    None,
    Redraw,
    Open(AppKind),
    OpenFile(String),
    CloseWindow(u32),
}

pub trait App {
    fn kind(&self) -> AppKind;
    fn title(&self) -> String {
        String::from(self.kind().title())
    }
    /// Размер рабочей области окна (без заголовка).
    fn size(&self) -> (i32, i32);
    fn draw(&mut self, c: &mut Canvas, w: i32, h: i32, focused: bool);
    fn key(&mut self, _k: Key) -> Action {
        Action::None
    }
    fn mouse_down(&mut self, _x: i32, _y: i32) -> Action {
        Action::None
    }
    fn mouse_drag(&mut self, _x: i32, _y: i32) -> Action {
        Action::None
    }
    fn mouse_up(&mut self, _x: i32, _y: i32) -> Action {
        Action::None
    }
    /// Вызывается каждый тик таймера; `true` — нужна перерисовка.
    fn tick(&mut self, _now: u64, _focused: bool) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Меню
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MenuItem {
    Open(AppKind),
    Sleep,
    Reboot,
    Shutdown,
}

impl MenuItem {
    fn title(self) -> &'static str {
        match self {
            MenuItem::Open(AppKind::Files) => "Файлы",
            MenuItem::Open(AppKind::Notepad) => "Документы",
            MenuItem::Open(AppKind::Paint) => "Изображение",
            MenuItem::Open(k) => k.title(),
            MenuItem::Sleep => "Спящий режим",
            MenuItem::Reboot => "Перезагрузка",
            MenuItem::Shutdown => "Выключить",
        }
    }
    fn icon(self) -> Icon {
        match self {
            MenuItem::Open(AppKind::Notepad) => Icon::FileText,
            MenuItem::Open(AppKind::Paint) => Icon::Image,
            MenuItem::Open(k) => k.icon(),
            MenuItem::Sleep => Icon::Moon,
            MenuItem::Reboot => Icon::RotateCcw,
            MenuItem::Shutdown => Icon::Power,
        }
    }
}

fn menu_layout() -> Vec<(MenuItem, Rect)> {
    let mut v = Vec::new();
    v.push((MenuItem::Open(AppKind::Welcome), Rect::new(12, 12, 210, 64)));
    for (i, k) in [AppKind::Files, AppKind::Mail, AppKind::Browser, AppKind::Terminal, AppKind::Calculator, AppKind::Snake]
        .iter()
        .enumerate()
    {
        v.push((MenuItem::Open(*k), Rect::new(12, 88 + i as i32 * (ROW_H + 2), 210, ROW_H)));
    }
    v.push((MenuItem::Open(AppKind::Apps), Rect::new(230, 12, 40, MENU_H - 24)));
    v.push((MenuItem::Open(AppKind::User), Rect::new(278, 12, 230, 104)));
    for (i, k) in [AppKind::Settings, AppKind::Notepad, AppKind::Paint].iter().enumerate() {
        v.push((MenuItem::Open(*k), Rect::new(278, 124 + i as i32 * (ROW_H + 2), 230, ROW_H)));
    }
    for (i, m) in [MenuItem::Sleep, MenuItem::Reboot, MenuItem::Shutdown].iter().enumerate() {
        v.push((*m, Rect::new(278, 258 + i as i32 * (ROW_H + 2), 230, ROW_H)));
    }
    v
}

// ---------------------------------------------------------------------------
// Окна
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Anim {
    Open,
    Close,
    Minimize,
    Restore,
}

impl Anim {
    fn duration(self) -> u64 {
        match self {
            Anim::Open => ANIM_OPEN,
            Anim::Close => ANIM_CLOSE,
            Anim::Minimize => ANIM_MINIMIZE,
            Anim::Restore => ANIM_RESTORE,
        }
    }
}

struct Window {
    id: u32,
    x: i32,
    y: i32,
    minimized: bool,
    anim: Option<(Anim, u64)>,
    app: Box<dyn App>,
}

impl Window {
    fn size(&self) -> (i32, i32) {
        let (w, h) = self.app.size();
        (w + 2, h + TITLE_H + 2)
    }
    fn rect(&self) -> Rect {
        let (w, h) = self.size();
        Rect::new(self.x, self.y, w, h)
    }
    fn close_rect(&self) -> Rect {
        let r = self.rect();
        Rect::new(r.x + r.w - 46, r.y, 46, TITLE_H)
    }
    fn min_rect(&self) -> Rect {
        let r = self.rect();
        Rect::new(r.x + r.w - 92, r.y, 46, TITLE_H)
    }
    fn client(&self) -> Rect {
        let (w, h) = self.app.size();
        Rect::new(self.x + 1, self.y + TITLE_H + 1, w, h)
    }
    /// Видно ли окно и можно ли с ним работать (не свёрнуто и не закрывается).
    fn interactive(&self) -> bool {
        !self.minimized && !matches!(self.anim, Some((Anim::Close, _)) | Some((Anim::Minimize, _)))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Hover {
    None,
    Start,
    Task(u32),
    Menu(MenuItem),
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
    scratch: Vec<u32>,
    wallpaper_index: usize,
    windows: Vec<Window>,
    next_id: u32,
    menu_open: bool,
    menu_anim: Option<(bool, u64)>,
    hover: Hover,
    bin_selected: bool,
    mx: i32,
    my: i32,
    buttons: u8,
    drag: Option<(u32, i32, i32)>,
    capture: Option<u32>,
    last_click: (bool, u64),
    cursor_at: (i32, i32),
    dirty: bool,
    minute: u8,
    russian: bool,
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
        scratch: Vec::new(),
        wallpaper_index: usize::MAX,
        windows: Vec::new(),
        next_id: 1,
        menu_open: false,
        menu_anim: None,
        hover: Hover::None,
        bin_selected: false,
        mx: w / 2,
        my: h / 2,
        buttons: 0,
        drag: None,
        capture: None,
        last_click: (false, 0),
        cursor_at: (0, 0),
        dirty: true,
        minute: 255,
        russian: false,
        last_tick: 0,
    };
    d.open(AppKind::Welcome);
    d.main_loop()
}

fn ease_out(t: f32) -> f32 {
    let u = 1.0 - t;
    1.0 - u * u * u
}

fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let u = -2.0 * t + 2.0;
        1.0 - u * u * u / 2.0
    }
}

fn lerp(a: i32, b: i32, t: f32) -> i32 {
    a + ((b - a) as f32 * t) as i32
}

impl Desktop {
    fn animations(&self) -> bool {
        SETTINGS.lock().animations
    }

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
            let now = time::ticks();
            if now != self.last_tick {
                self.last_tick = now;
                self.on_tick(now);
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

    fn on_tick(&mut self, now: u64) {
        // анимации окон
        let mut finished = Vec::new();
        for w in self.windows.iter_mut() {
            if let Some((a, start)) = w.anim {
                self.dirty = true;
                if now >= start + a.duration() {
                    finished.push((w.id, a));
                }
            }
        }
        for (id, a) in finished {
            if let Some(i) = self.index_of(id) {
                match a {
                    Anim::Close => {
                        self.windows.remove(i);
                    }
                    Anim::Minimize => {
                        self.windows[i].minimized = true;
                        self.windows[i].anim = None;
                    }
                    _ => self.windows[i].anim = None,
                }
            }
        }
        // анимация меню
        if let Some((opening, start)) = self.menu_anim {
            self.dirty = true;
            if now >= start + ANIM_MENU {
                self.menu_anim = None;
                self.menu_open = opening;
            }
        }
        // программы
        let top = self.top_visible();
        for (i, w) in self.windows.iter_mut().enumerate() {
            if !w.minimized && w.app.tick(now, Some(i) == top) {
                self.dirty = true;
            }
        }
        // часы и раскладка
        if now % time::HZ == 0 && rtc::now().minute != self.minute {
            self.dirty = true;
        }
        if keyboard::is_russian() != self.russian {
            self.russian = keyboard::is_russian();
            self.dirty = true;
        }
    }

    // ------------------------------------------------------------------
    // Окна
    // ------------------------------------------------------------------

    fn open(&mut self, kind: AppKind) {
        if !kind.multi() {
            if let Some(i) = self.windows.iter().position(|w| w.app.kind() == kind && !matches!(w.anim, Some((Anim::Close, _)))) {
                self.restore(i);
                return;
            }
        }
        self.add_window(kind.create());
    }

    fn add_window(&mut self, app: Box<dyn App>) {
        let (aw, ah) = app.size();
        let (ww, wh) = (aw + 2, ah + TITLE_H + 2);
        let n = self.windows.len() as i32;
        let x = (200 + (n % 8) * 32).min((self.w - ww).max(0));
        let y = (40 + (n % 8) * 28).min((self.h - TASKBAR_H - wh).max(0));
        let id = self.next_id;
        self.next_id += 1;
        let anim = if self.animations() { Some((Anim::Open, time::ticks())) } else { None };
        self.windows.push(Window { id, x, y, minimized: false, anim, app });
        self.dirty = true;
    }

    fn close(&mut self, i: usize) {
        if self.animations() {
            self.windows[i].anim = Some((Anim::Close, time::ticks()));
        } else {
            self.windows.remove(i);
        }
        self.dirty = true;
    }

    fn minimize(&mut self, i: usize) {
        if self.animations() {
            self.windows[i].anim = Some((Anim::Minimize, time::ticks()));
        } else {
            self.windows[i].minimized = true;
        }
        self.dirty = true;
    }

    fn restore(&mut self, i: usize) {
        let was_minimized = self.windows[i].minimized;
        self.windows[i].minimized = false;
        if was_minimized && self.animations() {
            self.windows[i].anim = Some((Anim::Restore, time::ticks()));
        } else if matches!(self.windows[i].anim, Some((Anim::Minimize, _))) {
            self.windows[i].anim = None;
        }
        self.raise(i);
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
        self.windows.iter().rposition(|w| w.interactive())
    }

    fn handle(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::Redraw => {}
            Action::Open(kind) => self.open(kind),
            Action::OpenFile(name) => self.add_window(Box::new(apps::Notepad::new(Some(name)))),
            Action::CloseWindow(id) => {
                if let Some(i) = self.index_of(id) {
                    self.close(i);
                }
            }
        }
        self.dirty = true;
    }

    fn activate(&mut self, item: MenuItem) {
        match item {
            MenuItem::Open(kind) => self.open(kind),
            MenuItem::Sleep => self.sleep(),
            MenuItem::Reboot => power::reboot(),
            MenuItem::Shutdown => power::shutdown(),
        }
    }

    fn set_menu(&mut self, open: bool) {
        if self.animations() {
            if self.menu_open != open || self.menu_anim.is_some() {
                self.menu_anim = Some((open, time::ticks()));
                self.menu_open = true;
            }
        } else {
            self.menu_open = open;
            self.menu_anim = None;
        }
        self.dirty = true;
    }

    /// Меню открыто и не закрывается (с ним можно работать).
    fn menu_active(&self) -> bool {
        self.menu_open && !matches!(self.menu_anim, Some((false, _)))
    }

    fn sleep(&mut self) {
        self.menu_open = false;
        self.menu_anim = None;
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

    fn menu_rect(&self) -> Rect {
        let (ox, oy) = self.menu_origin();
        Rect::new(ox, oy, MENU_W, MENU_H)
    }

    fn start_rect(&self) -> Rect {
        Rect::new(6, self.h - TASKBAR_H + 4, TASK_BTN, TASK_BTN)
    }

    fn task_buttons(&self) -> Vec<(u32, Rect)> {
        let mut ids: Vec<u32> = self
            .windows
            .iter()
            .filter(|w| !matches!(w.anim, Some((Anim::Close, _))))
            .map(|w| w.id)
            .collect();
        ids.sort_unstable();
        ids.iter()
            .enumerate()
            .map(|(i, &id)| (id, Rect::new(54 + i as i32 * (TASK_BTN + 4), self.h - TASKBAR_H + 4, TASK_BTN, TASK_BTN)))
            .collect()
    }

    fn lang_rect(&self) -> Rect {
        Rect::new(self.w - 196, self.h - TASKBAR_H + 10, 30, 28)
    }

    fn bin_rect(&self) -> Rect {
        Rect::new(12, 12, 84, 82)
    }

    fn hit_hover(&self) -> Hover {
        let (x, y) = (self.mx, self.my);
        if self.menu_active() {
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
        if self.menu_active() && self.menu_rect().contains(x, y) {
            return Hover::None;
        }
        for w in self.windows.iter().rev().filter(|w| w.interactive()) {
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

    // ------------------------------------------------------------------
    // Ввод
    // ------------------------------------------------------------------

    fn on_key(&mut self, k: Key) {
        if self.menu_active() {
            if k == Key::Escape {
                self.set_menu(false);
            }
            return;
        }
        if let Some(i) = self.top_visible() {
            let action = self.windows[i].app.key(k);
            self.handle(action);
        }
    }

    fn on_mouse(&mut self, ev: MouseEvent, moved: &mut bool) {
        if ev.dx != 0 || ev.dy != 0 {
            self.mx = (self.mx + ev.dx).clamp(0, self.w - 1);
            self.my = (self.my + ev.dy).clamp(0, self.h - 1);
            if let Some((id, ox, oy)) = self.drag {
                if let Some(i) = self.index_of(id) {
                    let max_y = self.h - TASKBAR_H - TITLE_H;
                    let ww = self.windows[i].size().0;
                    self.windows[i].x = (self.mx - ox).clamp(-ww + 80, self.w - 80);
                    self.windows[i].y = (self.my - oy).clamp(0, max_y);
                    self.dirty = true;
                }
            } else if let Some(id) = self.capture {
                if let Some(i) = self.index_of(id) {
                    let c = self.windows[i].client();
                    let action = self.windows[i].app.mouse_drag(self.mx - c.x, self.my - c.y);
                    self.handle(action);
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
            self.left_up();
        }
    }

    fn left_down(&mut self) {
        let (mx, my) = (self.mx, self.my);
        self.dirty = true;

        if self.menu_active() {
            if let Hover::Menu(item) = self.hit_hover() {
                self.set_menu(false);
                self.activate(item);
                return;
            }
            if self.menu_rect().contains(mx, my) {
                return;
            }
            self.set_menu(false);
            if self.start_rect().contains(mx, my) {
                return;
            }
        }

        if my >= self.h - TASKBAR_H {
            if self.start_rect().contains(mx, my) {
                self.set_menu(true);
                return;
            }
            if self.lang_rect().contains(mx, my) {
                keyboard::toggle_layout();
                return;
            }
            for (id, r) in self.task_buttons() {
                if r.contains(mx, my) {
                    if let Some(i) = self.index_of(id) {
                        if self.top_visible() == Some(i) {
                            self.minimize(i);
                        } else {
                            self.restore(i);
                        }
                    }
                    return;
                }
            }
            return;
        }

        for i in (0..self.windows.len()).rev() {
            if !self.windows[i].interactive() || !self.windows[i].rect().contains(mx, my) {
                continue;
            }
            self.raise(i);
            let top = self.windows.len() - 1;
            let (id, wx, wy) = (self.windows[top].id, self.windows[top].x, self.windows[top].y);
            if self.windows[top].close_rect().contains(mx, my) {
                self.close(top);
            } else if self.windows[top].min_rect().contains(mx, my) {
                self.minimize(top);
            } else if my < wy + TITLE_H {
                self.drag = Some((id, mx - wx, my - wy));
            } else {
                let c = self.windows[top].client();
                if c.contains(mx, my) {
                    self.capture = Some(id);
                    let action = self.windows[top].app.mouse_down(mx - c.x, my - c.y);
                    self.handle(action);
                }
            }
            return;
        }

        let hit = self.bin_rect().contains(mx, my);
        self.bin_selected = hit;
        let now = time::ticks();
        if hit && self.last_click.0 && now - self.last_click.1 <= time::HZ / 2 {
            self.open(AppKind::Bin);
            self.last_click = (false, 0);
        } else {
            self.last_click = (hit, now);
        }
    }

    fn left_up(&mut self) {
        self.drag = None;
        if let Some(id) = self.capture.take() {
            if let Some(i) = self.index_of(id) {
                let c = self.windows[i].client();
                let action = self.windows[i].app.mouse_up(self.mx - c.x, self.my - c.y);
                self.handle(action);
            }
        }
    }

    // ------------------------------------------------------------------
    // Рисование
    // ------------------------------------------------------------------

    fn paint_wallpaper(&mut self, index: usize) {
        let (w, h) = (self.w, self.h);
        let (_, top, bottom) = WALLPAPERS[index % WALLPAPERS.len()];
        let mut c = Canvas::new(&mut self.wallpaper, w as usize, h as usize);
        c.gradient(0, 0, w, h, top, bottom);
        let (cx, cy, r) = (w * 3 / 4, h * 3 / 5, h * 2 / 3);
        let mut ring = r;
        while ring > 0 {
            for dy in -ring..=ring {
                let dx = gfx::sqrt((ring * ring - dy * dy) as f32) as i32;
                c.fill_rect_alpha(cx - dx, cy + dy, 2 * dx + 1, 1, gfx::blend(bottom, WHITE, 120), 3);
            }
            ring -= r / 12;
        }
        self.wallpaper_index = index;
    }

    fn render(&mut self) {
        let wp = SETTINGS.lock().wallpaper;
        if wp != self.wallpaper_index {
            self.paint_wallpaper(wp);
        }
        // список окон для «Монитора системы»
        {
            let mut list = OPEN_WINDOWS.lock();
            list.clear();
            for w in self.windows.iter().filter(|w| !matches!(w.anim, Some((Anim::Close, _)))) {
                list.push((w.id, w.app.kind(), w.app.title()));
            }
        }
        self.back.copy_from_slice(&self.wallpaper);
        let (w, h) = (self.w, self.h);
        let now = time::ticks();
        let hover = self.hover;
        let focused = self.top_visible().map(|i| self.windows[i].id);
        let tasks = self.task_buttons();
        let start = self.start_rect();
        let bin = self.bin_rect();
        let lang = self.lang_rect();
        let menu_origin = self.menu_origin();
        let bin_selected = self.bin_selected;

        {
            let mut c = Canvas::new(&mut self.back, w as usize, h as usize);
            if bin_selected || hover == Hover::Bin {
                c.round_rect(bin.x, bin.y, bin.w, bin.h, 8, WHITE, if bin_selected { 60 } else { 35 });
            }
            ui::icon(&mut c, Icon::Trash2, 40, bin.x + (bin.w - 40) / 2 + 1, bin.y + 11, BLACK);
            ui::icon(&mut c, Icon::Trash2, 40, bin.x + (bin.w - 40) / 2, bin.y + 10, WHITE);
            ui::text_centered(&mut c, Font::Regular, bin.x + 1, bin.y + 58, bin.w, "Корзина", BLACK);
            ui::text_centered(&mut c, Font::Regular, bin.x, bin.y + 57, bin.w, "Корзина", WHITE);
        }

        // окна (снизу вверх)
        for i in 0..self.windows.len() {
            if self.windows[i].minimized {
                continue;
            }
            let is_focused = Some(self.windows[i].id) == focused;
            match self.windows[i].anim {
                None => {
                    let mut c = Canvas::new(&mut self.back, w as usize, h as usize);
                    let win = &mut self.windows[i];
                    let r = win.rect();
                    draw_shadow(&mut c, r, is_focused);
                    draw_window(&mut c, win, r.x, r.y, is_focused, hover);
                }
                Some((anim, started)) => {
                    let id = self.windows[i].id;
                    let target = tasks.iter().find(|(tid, _)| *tid == id).map(|(_, r)| *r).unwrap_or(start);
                    let p = ((now.saturating_sub(started)) as f32 / anim.duration() as f32).min(1.0);
                    let r = self.windows[i].rect();
                    let (dst, alpha) = match anim {
                        Anim::Open => {
                            let t = ease_out(p);
                            (scale_rect(r, 0.86 + 0.14 * t), (255.0 * t) as u32)
                        }
                        Anim::Close => (scale_rect(r, 1.0 - 0.1 * p), (255.0 * (1.0 - p)) as u32),
                        Anim::Minimize => (lerp_rect(r, target, ease_in_out(p)), (255.0 * (1.0 - 0.8 * p)) as u32),
                        Anim::Restore => (lerp_rect(target, r, ease_out(p)), (255.0 * (0.2 + 0.8 * p)) as u32),
                    };
                    // окно рисуется в отдельный буфер и выводится уменьшенным и полупрозрачным
                    let (ww, wh) = (r.w as usize, r.h as usize);
                    self.scratch.clear();
                    self.scratch.resize(ww * wh, WHITE);
                    {
                        let mut sc = Canvas::new(&mut self.scratch, ww, wh);
                        draw_window(&mut sc, &mut self.windows[i], 0, 0, is_focused, Hover::None);
                    }
                    let mut c = Canvas::new(&mut self.back, w as usize, h as usize);
                    blit_scaled(&mut c, dst, &self.scratch, ww, wh, alpha);
                }
            }
        }

        let mut c = Canvas::new(&mut self.back, w as usize, h as usize);

        // панель задач
        c.fill_rect_alpha(0, h - TASKBAR_H, w, TASKBAR_H, TASKBAR_BG, 225);
        c.fill_rect_alpha(0, h - TASKBAR_H, w, 1, WHITE, 30);
        let sa = if self.menu_open { 60 } else if hover == Hover::Start { 35 } else { 0 };
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
            ui::icon(&mut c, win.app.kind().icon(), 24, r.x + 8, r.y + 7, WHITE);
            let (bw, color) = if active { (16, rgb(96, 165, 250)) } else { (6, rgb(148, 163, 184)) };
            c.round_rect(r.x + (r.w - bw) / 2, r.y + r.h - 4, bw, 3, 1, color, 255);
        }

        let (time_s, date_s) = apps::clock_strings();
        self.minute = rtc::now().minute;
        let cw = ui::text_width(Font::Regular, &time_s).max(ui::text_width(Font::Regular, &date_s));
        let cx = w - 12 - cw;
        let ty = h - TASKBAR_H + 7;
        ui::text(&mut c, Font::Regular, cx + cw - ui::text_width(Font::Regular, &time_s), ty, &time_s, WHITE);
        ui::text(&mut c, Font::Regular, cx + cw - ui::text_width(Font::Regular, &date_s), ty + 17, &date_s, rgb(203, 213, 225));
        ui::icon(&mut c, Icon::Signal, 16, cx - 28, h - TASKBAR_H + 16, WHITE);
        ui::icon(&mut c, Icon::PlugZap, 16, cx - 50, h - TASKBAR_H + 16, WHITE);
        let lang_s = if keyboard::is_russian() { "RU" } else { "EN" };
        ui::text_centered(&mut c, Font::Bold, lang.x, lang.y + 6, lang.w, lang_s, WHITE);

        // меню (с анимацией появления)
        if self.menu_open {
            match self.menu_anim {
                None => draw_menu(&mut c, menu_origin, hover),
                Some((opening, started)) => {
                    let p = ((now.saturating_sub(started)) as f32 / ANIM_MENU as f32).min(1.0);
                    let t = if opening { ease_out(p) } else { 1.0 - p };
                    let offset = ((1.0 - t) * 24.0) as i32;
                    let alpha = (255.0 * t) as u32;
                    let area = Rect::new(0, menu_origin.1 - 16, MENU_W + 32, MENU_H + 48);
                    let (aw, ah) = (area.w as usize, area.h as usize);
                    // слой: копия фона под меню + меню со сдвигом, затем смешивание
                    self.scratch.clear();
                    self.scratch.resize(aw * ah, 0);
                    for row in 0..ah {
                        let sy = (area.y as usize + row).min(h as usize - 1);
                        let src = &self.back[sy * w as usize..sy * w as usize + aw];
                        self.scratch[row * aw..row * aw + aw].copy_from_slice(src);
                    }
                    {
                        let mut layer = Canvas::new(&mut self.scratch, aw, ah);
                        draw_menu(&mut layer, (menu_origin.0, 16 + offset), Hover::None);
                    }
                    let mut c = Canvas::new(&mut self.back, w as usize, h as usize);
                    blit_scaled(&mut c, area, &self.scratch, aw, ah, alpha);
                }
            }
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

fn scale_rect(r: Rect, s: f32) -> Rect {
    let w = (r.w as f32 * s) as i32;
    let h = (r.h as f32 * s) as i32;
    Rect::new(r.x + (r.w - w) / 2, r.y + (r.h - h) / 2, w.max(1), h.max(1))
}

fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::new(lerp(a.x, b.x, t), lerp(a.y, b.y, t), lerp(a.w, b.w, t).max(1), lerp(a.h, b.h, t).max(1))
}

/// Вывести картинку `src` (sw × sh) в прямоугольник `dst` с масштабированием и прозрачностью.
fn blit_scaled(c: &mut Canvas, dst: Rect, src: &[u32], sw: usize, sh: usize, alpha: u32) {
    if dst.w <= 0 || dst.h <= 0 || alpha == 0 {
        return;
    }
    for dy in 0..dst.h {
        let sy = (dy as usize * sh) / dst.h as usize;
        for dx in 0..dst.w {
            let sx = (dx as usize * sw) / dst.w as usize;
            if let Some(&p) = src.get(sy * sw + sx) {
                c.blend_pixel(dst.x + dx, dst.y + dy, p, alpha);
            }
        }
    }
}

fn draw_shadow(c: &mut Canvas, r: Rect, focused: bool) {
    for i in 1..=10 {
        c.fill_rect_alpha(r.x - 2 + i / 2, r.y + i, r.w + 4 - i, r.h, BLACK, if focused { 6 } else { 4 });
    }
}

fn draw_window(c: &mut Canvas, win: &mut Window, x: i32, y: i32, focused: bool, hover: Hover) {
    let (w, h) = win.size();
    c.fill_rect(x, y, w, h, if focused { rgb(148, 163, 184) } else { BORDER });
    c.fill_rect(x + 1, y + 1, w - 2, h - 2, WHITE);
    let title_bg = if focused { WHITE } else { rgb(241, 245, 249) };
    c.fill_rect(x + 1, y + 1, w - 2, TITLE_H - 1, title_bg);
    c.fill_rect(x + 1, y + TITLE_H, w - 2, 1, rgb(226, 232, 240));
    ui::icon(c, win.app.kind().icon(), 16, x + 12, y + 10, if focused { ACCENT } else { MUTED });
    let title = ui::fit(Font::Bold, &win.app.title(), w - 140);
    ui::text(c, Font::Bold, x + 36, y + 10, &title, if focused { TEXT } else { MUTED });

    let (mx, cx) = (x + w - 92, x + w - 46);
    if hover == Hover::Min(win.id) {
        c.fill_rect(mx, y + 1, 46, TITLE_H - 1, rgb(226, 232, 240));
    }
    ui::icon(c, Icon::Minus, 16, mx + 15, y + 10, TEXT);
    let close_hover = hover == Hover::Close(win.id);
    if close_hover {
        c.fill_rect(cx, y + 1, 45, TITLE_H - 1, DANGER);
    }
    ui::icon(c, Icon::X, 16, cx + 15, y + 10, if close_hover { WHITE } else { TEXT });

    let (cw, ch) = win.app.size();
    let mut s = c.sub(Rect::new(x + 1, y + TITLE_H + 1, cw, ch));
    win.app.draw(&mut s, cw, ch, focused);
}

fn draw_menu(c: &mut Canvas, (ox, oy): (i32, i32), hover: Hover) {
    for i in 1..=8 {
        c.round_rect(ox - 1 + i / 2, oy + i, MENU_W, MENU_H, 12, BLACK, 7);
    }
    c.round_rect(ox, oy, MENU_W, MENU_H, 12, BORDER, 255);
    c.round_rect(ox + 1, oy + 1, MENU_W - 2, MENU_H - 2, 11, PANEL, 255);
    let mut m = c.sub(Rect::new(ox, oy, MENU_W, MENU_H));
    for (item, r) in menu_layout() {
        let hovered = hover == Hover::Menu(item);
        match item {
            MenuItem::Open(AppKind::Welcome) => {
                m.round_rect(r.x, r.y, r.w, r.h, 8, if hovered { rgb(29, 78, 216) } else { ACCENT }, 255);
                ui::icon(&mut m, Icon::Sparkles, 24, r.x + 12, r.y + 20, WHITE);
                ui::text(&mut m, Font::Bold, r.x + 46, r.y + 14, "Добро пожаловать!", WHITE);
                let v = alloc::format!("Linoves {} ({})", crate::VERSION, crate::CODENAME);
                ui::text(&mut m, Font::Regular, r.x + 46, r.y + 34, &v, rgb(219, 234, 254));
            }
            MenuItem::Open(AppKind::Apps) => {
                m.round_rect(r.x, r.y, r.w, r.h, 8, if hovered { ACCENT_SOFT } else { STRIP }, 255);
                let color = if hovered { ACCENT } else { MUTED };
                ui::icon(&mut m, Icon::LayoutGrid, 20, r.x + 10, r.y + 12, color);
                let lh = ui::line_height(Font::Bold);
                ui::text_vertical(&mut m, Font::Bold, r.x + (r.w - lh) / 2, r.y + r.h - 14, "Приложения", color);
            }
            MenuItem::Open(AppKind::User) => {
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
                    let bg = if item == MenuItem::Shutdown { rgb(254, 226, 226) } else { ACCENT_SOFT };
                    m.round_rect(r.x, r.y, r.w, r.h, 8, bg, 255);
                }
                let color = match item {
                    MenuItem::Shutdown if hovered => DANGER,
                    _ if hovered => ACCENT,
                    _ => MUTED,
                };
                ui::icon(&mut m, item.icon(), 20, r.x + 12, r.y + (r.h - 20) / 2, color);
                ui::text(&mut m, Font::Regular, r.x + 44, r.y + (r.h - 15) / 2, item.title(), TEXT);
            }
        }
    }
    m.fill_rect(290, 249, 206, 1, BORDER);
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
