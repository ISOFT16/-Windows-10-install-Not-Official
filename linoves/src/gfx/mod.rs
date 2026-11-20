//! Графика: включение видеорежима, холст для рисования и вывод на экран.
//!
//! Видеорежим берётся из двух источников:
//! 1. графический буфер, который включил загрузчик GRUB (ISO на реальном ПК);
//! 2. адаптер Bochs/QEMU/VirtualBox (BGA, «Bochs VBE»), который ядро
//!    программирует само — так работает `cargo run` и запуск ISO в QEMU.
//!
//! Всё рисуется во внутренний буфер (0x00RRGGBB), а затем копируется на
//! экран целиком — без мерцания.

pub mod font;
mod font_data;
pub mod ui;
mod ui_data;

use crate::console::{Cell, TextGrid, GRID_COLS, GRID_ROWS};
use crate::cpu::{inw, outw};
use crate::sync::IrqMutex;
use crate::{multiboot, pci};

pub type Color = u32;

pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}

/// Смешать цвет `src` поверх `dst` с непрозрачностью `a` (0..255).
#[inline]
pub fn blend(dst: Color, src: Color, a: u32) -> Color {
    if a >= 255 {
        return src;
    }
    if a == 0 {
        return dst;
    }
    let inv = 255 - a;
    let r = (((src >> 16) & 0xFF) * a + ((dst >> 16) & 0xFF) * inv) / 255;
    let g = (((src >> 8) & 0xFF) * a + ((dst >> 8) & 0xFF) * inv) / 255;
    let b = ((src & 0xFF) * a + (dst & 0xFF) * inv) / 255;
    (r << 16) | (g << 8) | b
}

/// Палитра 16 цветов терминала (индексы как у VGA).
pub const PALETTE: [Color; 16] = [
    rgb(12, 12, 12),
    rgb(0, 55, 218),
    rgb(19, 161, 14),
    rgb(58, 150, 221),
    rgb(197, 15, 31),
    rgb(136, 23, 152),
    rgb(193, 156, 0),
    rgb(204, 204, 204),
    rgb(118, 118, 118),
    rgb(59, 120, 255),
    rgb(22, 198, 12),
    rgb(97, 214, 214),
    rgb(231, 72, 86),
    rgb(180, 0, 158),
    rgb(249, 241, 165),
    rgb(242, 242, 242),
];

// ---------------------------------------------------------------------------
// Видеорежим
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct FbInfo {
    pub addr: u64,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub bpp: u8,
    red_pos: u8,
    green_pos: u8,
    blue_pos: u8,
    pub source: &'static str,
}

impl FbInfo {
    /// Порядок цветовых каналов, например «R16 G8 B0».
    pub fn layout(&self) -> alloc::string::String {
        alloc::format!("R{} G{} B{}", self.red_pos, self.green_pos, self.blue_pos)
    }

    fn native(&self) -> bool {
        self.bpp == 32 && self.red_pos == 16 && self.green_pos == 8 && self.blue_pos == 0
    }

    #[inline]
    unsafe fn write_pixel(&self, x: usize, y: usize, c: Color) {
        let off = self.addr as usize + y * self.pitch + x * (self.bpp as usize / 8);
        if self.native() {
            (off as *mut u32).write_volatile(c);
            return;
        }
        let r = (c >> 16) & 0xFF;
        let g = (c >> 8) & 0xFF;
        let b = c & 0xFF;
        let v = (r << self.red_pos) | (g << self.green_pos) | (b << self.blue_pos);
        let p = off as *mut u8;
        p.write_volatile(v as u8);
        p.add(1).write_volatile((v >> 8) as u8);
        p.add(2).write_volatile((v >> 16) as u8);
        if self.bpp == 32 {
            p.add(3).write_volatile((v >> 24) as u8);
        }
    }
}

static FB: IrqMutex<Option<FbInfo>> = IrqMutex::new(None);

const MAX_W: usize = 2560;
const MAX_H: usize = 1600;
pub const DEFAULT_W: usize = 1024;
pub const DEFAULT_H: usize = 768;

fn from_multiboot() -> Option<FbInfo> {
    let f = multiboot::with(|i| i.framebuffer)?;
    let (w, h) = (f.width as usize, f.height as usize);
    let ok = f.kind == 1
        && (f.bpp == 32 || f.bpp == 24)
        && (640..=MAX_W).contains(&w)
        && (480..=MAX_H).contains(&h)
        && f.pitch as usize >= w * (f.bpp as usize / 8)
        && f.addr.checked_add(f.pitch as u64 * h as u64).map_or(false, |e| e <= 1 << 32);
    if !ok {
        return None;
    }
    Some(FbInfo {
        addr: f.addr,
        width: w,
        height: h,
        pitch: f.pitch as usize,
        bpp: f.bpp,
        red_pos: f.red_pos,
        green_pos: f.green_pos,
        blue_pos: f.blue_pos,
        source: "GRUB framebuffer",
    })
}

const BGA_INDEX: u16 = 0x01CE;
const BGA_DATA: u16 = 0x01CF;

fn bga_write(index: u16, value: u16) {
    unsafe {
        outw(BGA_INDEX, index);
        outw(BGA_DATA, value);
    }
}

fn bga_read(index: u16) -> u16 {
    unsafe {
        outw(BGA_INDEX, index);
        inw(BGA_DATA)
    }
}

/// Включить режим через адаптер Bochs VBE (QEMU, Bochs, VirtualBox VBoxVGA).
fn from_bga(width: usize, height: usize) -> Option<FbInfo> {
    let id = bga_read(0);
    if !(0xB0C0..=0xB0CF).contains(&id) {
        return None;
    }
    let dev = pci::find(|d| {
        d.class == 0x03
            && matches!((d.vendor, d.device), (0x1234, 0x1111) | (0x80EE, 0xBEEF))
    })?;
    let lfb = (dev.bar0 & 0xFFFF_FFF0) as u64;
    if lfb == 0 || lfb + (width * height * 4) as u64 > 1 << 32 {
        return None;
    }
    bga_write(4, 0); // выключить
    bga_write(1, width as u16);
    bga_write(2, height as u16);
    bga_write(3, 32);
    bga_write(4, 0x01 | 0x40); // включить + линейный буфер
    if bga_read(1) as usize != width || bga_read(2) as usize != height || bga_read(3) != 32 {
        bga_write(4, 0);
        return None;
    }
    Some(FbInfo {
        addr: lfb,
        width,
        height,
        pitch: width * 4,
        bpp: 32,
        red_pos: 16,
        green_pos: 8,
        blue_pos: 0,
        source: "Bochs VBE",
    })
}

/// Включить графику. `None` — графика недоступна, остаёмся в текстовом режиме.
pub fn init() -> Option<FbInfo> {
    if let Some(fb) = *FB.lock() {
        return Some(fb);
    }
    let fb = from_multiboot().or_else(|| from_bga(DEFAULT_W, DEFAULT_H))?;
    *FB.lock() = Some(fb);
    Some(fb)
}

pub fn info() -> Option<FbInfo> {
    match FB.try_lock() {
        Some(g) => *g,
        None => None,
    }
}

/// Скопировать прямоугольник из буфера `buf` (ширина строки `stride`) на экран.
pub fn present_rect(fb: &FbInfo, buf: &[u32], stride: usize, x: usize, y: usize, w: usize, h: usize) {
    let x_end = (x + w).min(fb.width).min(stride);
    let y_end = (y + h).min(fb.height);
    if x >= x_end || y >= y_end {
        return;
    }
    let native = fb.native();
    for row in y..y_end {
        let start = row * stride + x;
        let end = row * stride + x_end;
        if end > buf.len() {
            break;
        }
        let src = &buf[start..end];
        unsafe {
            if native {
                let dst = (fb.addr as usize + row * fb.pitch + x * 4) as *mut u32;
                core::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len());
            } else {
                for (i, &c) in src.iter().enumerate() {
                    fb.write_pixel(x + i, row, c);
                }
            }
        }
    }
}

/// Нарисовать точку прямо на экране (курсор мыши, аварийный экран).
pub fn put_pixel_direct(fb: &FbInfo, x: i32, y: i32, c: Color) {
    if x >= 0 && y >= 0 && (x as usize) < fb.width && (y as usize) < fb.height {
        unsafe { fb.write_pixel(x as usize, y as usize, c) };
    }
}

/// Аварийный экран в графическом режиме: текстовая сетка во весь экран,
/// без выделения памяти и без промежуточного буфера.
pub fn draw_grid_direct(fb: &FbInfo, grid: &TextGrid) {
    let bg = PALETTE[((grid.cells[0].attr >> 4) & 0x0F) as usize];
    for y in 0..fb.height {
        for x in 0..fb.width {
            unsafe { fb.write_pixel(x, y, bg) };
        }
    }
    let ox = fb.width.saturating_sub(GRID_COLS * font::WIDTH) / 2;
    let oy = fb.height.saturating_sub(GRID_ROWS * font::HEIGHT) / 2;
    for row in 0..GRID_ROWS {
        for col in 0..GRID_COLS {
            let Cell { ch, attr } = grid.cells[row * GRID_COLS + col];
            let fg = PALETTE[(attr & 0x0F) as usize];
            let cbg = PALETTE[((attr >> 4) & 0x0F) as usize];
            let glyph = font::glyph(ch);
            for gy in 0..font::HEIGHT {
                for gx in 0..font::WIDTH {
                    let a = glyph[gy * font::WIDTH + gx] as u32;
                    let px = ox + col * font::WIDTH + gx;
                    let py = oy + row * font::HEIGHT + gy;
                    if px < fb.width && py < fb.height {
                        unsafe { fb.write_pixel(px, py, blend(cbg, fg, a)) };
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Холст
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }

    pub fn intersect(&self, o: &Rect) -> Rect {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = (self.x + self.w).min(o.x + o.w);
        let y1 = (self.y + self.h).min(o.y + o.h);
        Rect { x: x0, y: y0, w: (x1 - x0).max(0), h: (y1 - y0).max(0) }
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }
}

/// Холст поверх буфера пикселей. Все координаты относительные (от `origin`),
/// всё, что выходит за область отсечения, просто не рисуется — выйти за
/// границы буфера невозможно.
pub struct Canvas<'a> {
    buf: &'a mut [u32],
    stride: usize,
    ox: i32,
    oy: i32,
    w: i32,
    h: i32,
    clip: Rect,
}

impl<'a> Canvas<'a> {
    pub fn new(buf: &'a mut [u32], width: usize, height: usize) -> Self {
        let height = height.min(buf.len() / width.max(1));
        Canvas {
            buf,
            stride: width,
            ox: 0,
            oy: 0,
            w: width as i32,
            h: height as i32,
            clip: Rect::new(0, 0, width as i32, height as i32),
        }
    }

    /// Вложенный холст для области `r` (в координатах текущего холста).
    pub fn sub(&mut self, r: Rect) -> Canvas<'_> {
        let abs = Rect::new(r.x + self.ox, r.y + self.oy, r.w, r.h);
        Canvas {
            buf: &mut *self.buf,
            stride: self.stride,
            ox: abs.x,
            oy: abs.y,
            w: r.w,
            h: r.h,
            clip: self.clip.intersect(&abs),
        }
    }

    #[inline]
    fn abs_rect(&self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect::new(x + self.ox, y + self.oy, w, h).intersect(&self.clip)
    }

    #[inline]
    pub fn pixel(&mut self, x: i32, y: i32, c: Color) {
        let (ax, ay) = (x + self.ox, y + self.oy);
        if self.clip.contains(ax, ay) {
            self.buf[ay as usize * self.stride + ax as usize] = c;
        }
    }

    #[inline]
    pub fn blend_pixel(&mut self, x: i32, y: i32, c: Color, a: u32) {
        let (ax, ay) = (x + self.ox, y + self.oy);
        if a > 0 && self.clip.contains(ax, ay) {
            let i = ay as usize * self.stride + ax as usize;
            self.buf[i] = blend(self.buf[i], c, a);
        }
    }

    pub fn fill(&mut self, c: Color) {
        let (w, h) = (self.w, self.h);
        self.fill_rect(0, 0, w, h, c);
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color) {
        let r = self.abs_rect(x, y, w, h);
        if r.is_empty() {
            return;
        }
        for yy in r.y..r.y + r.h {
            let s = yy as usize * self.stride + r.x as usize;
            self.buf[s..s + r.w as usize].fill(c);
        }
    }

    pub fn fill_rect_alpha(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color, a: u32) {
        let r = self.abs_rect(x, y, w, h);
        if r.is_empty() {
            return;
        }
        for yy in r.y..r.y + r.h {
            let s = yy as usize * self.stride + r.x as usize;
            for p in &mut self.buf[s..s + r.w as usize] {
                *p = blend(*p, c, a);
            }
        }
    }

    /// Вертикальный градиент.
    pub fn gradient(&mut self, x: i32, y: i32, w: i32, h: i32, top: Color, bottom: Color) {
        let span = (h - 1).max(1) as u32;
        for i in 0..h {
            let a = (i as u32 * 255) / span;
            self.fill_rect(x, y + i, w, 1, blend(top, bottom, a));
        }
    }

    /// Прямоугольник со скруглёнными углами (со сглаживанием), непрозрачность `a`.
    pub fn round_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, c: Color, a: u32) {
        let r = r.min(w / 2).min(h / 2).max(0);
        self.fill_rect_alpha(x, y + r, w, h - 2 * r, c, a);
        for i in 0..r {
            let dy = r as f32 - i as f32 - 0.5;
            let dx = sqrt((r * r) as f32 - dy * dy);
            let inset = r as f32 - dx;
            let full = ceil(inset);
            let frac = ((full as f32 - inset) * a as f32) as u32;
            for &yy in &[y + i, y + h - 1 - i] {
                self.fill_rect_alpha(x + full, yy, w - 2 * full, 1, c, a);
                if full > 0 {
                    self.blend_pixel(x + full - 1, yy, c, frac);
                    self.blend_pixel(x + w - full, yy, c, frac);
                }
            }
        }
    }

    pub fn fill_circle(&mut self, cx: i32, cy: i32, r: i32, c: Color) {
        if r <= 0 {
            self.pixel(cx, cy, c);
            return;
        }
        for dy in -r..=r {
            let dx = sqrt((r * r - dy * dy) as f32) as i32;
            self.fill_rect(cx - dx, cy + dy, 2 * dx + 1, 1, c);
        }
    }

    /// Линия толщиной `radius*2+1` (для рисования кистью).
    pub fn thick_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, radius: i32, c: Color) {
        let (mut x, mut y) = (x0, y0);
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut guard = 0;
        loop {
            self.fill_circle(x, y, radius, c);
            if (x == x1 && y == y1) || guard > 10_000 {
                break;
            }
            guard += 1;
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    pub fn draw_char(&mut self, x: i32, y: i32, ch: char, c: Color) {
        let glyph = font::glyph(ch);
        for gy in 0..font::HEIGHT as i32 {
            for gx in 0..font::WIDTH as i32 {
                let a = glyph[(gy * font::WIDTH as i32 + gx) as usize] as u32;
                self.blend_pixel(x + gx, y + gy, c, a);
            }
        }
    }

    /// Текст; возвращает ширину в пикселях.
    pub fn text(&mut self, x: i32, y: i32, s: &str, c: Color) -> i32 {
        let mut cx = x;
        for ch in s.chars() {
            if cx >= self.w + 8 {
                break;
            }
            if ch != ' ' {
                self.draw_char(cx, y, ch, c);
            }
            cx += font::WIDTH as i32;
        }
        cx - x
    }
}

fn ceil(v: f32) -> i32 {
    let t = v as i32;
    if (t as f32) < v {
        t + 1
    } else {
        t
    }
}

/// Квадратный корень без SSE (метод Ньютона), достаточно точный для графики.
pub fn sqrt(v: f32) -> f32 {
    if v <= 0.0 {
        return 0.0;
    }
    let mut x = if v > 1.0 { v / 2.0 } else { 1.0 };
    for _ in 0..12 {
        x = 0.5 * (x + v / x);
    }
    x
}
