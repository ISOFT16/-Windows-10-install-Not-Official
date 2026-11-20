//! Значки Lucide и шрифт интерфейса (DejaVu Sans, пропорциональный, со сглаживанием).
//! Данные генерирует `tools/gen_ui.py` в `ui_data.rs`.

use alloc::string::String;

use super::ui_data as d;
use super::{Canvas, Color};
pub use d::Icon;

/// Нарисовать значок размером `size` (16, 20, 24, 40 или 48) цветом `color`.
pub fn icon(c: &mut Canvas, icon: Icon, size: usize, x: i32, y: i32, color: Color) {
    let si = d::ICON_SIZES
        .iter()
        .position(|&s| s >= size)
        .unwrap_or(d::ICON_SIZES.len() - 1);
    let s = d::ICON_SIZES[si];
    let off = d::ICON_OFFSETS[icon as usize * d::ICON_SIZES.len() + si] as usize;
    for yy in 0..s {
        for xx in 0..s {
            let a = d::ICON_ALPHA[off + yy * s + xx] as u32;
            c.blend_pixel(x + xx as i32, y + yy as i32, color, a);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Font {
    Regular,
    Bold,
    Large,
}

struct FontData {
    height: usize,
    codes: &'static [u32],
    advances: &'static [u8],
    widths: &'static [u8],
    offsets: &'static [u32],
    alpha: &'static [u8],
}

fn data(f: Font) -> FontData {
    match f {
        Font::Regular => FontData {
            height: d::REGULAR_HEIGHT,
            codes: &d::REGULAR_CODES,
            advances: &d::REGULAR_ADVANCES,
            widths: &d::REGULAR_WIDTHS,
            offsets: &d::REGULAR_OFFSETS,
            alpha: &d::REGULAR_ALPHA,
        },
        Font::Bold => FontData {
            height: d::BOLD_HEIGHT,
            codes: &d::BOLD_CODES,
            advances: &d::BOLD_ADVANCES,
            widths: &d::BOLD_WIDTHS,
            offsets: &d::BOLD_OFFSETS,
            alpha: &d::BOLD_ALPHA,
        },
        Font::Large => FontData {
            height: d::LARGE_HEIGHT,
            codes: &d::LARGE_CODES,
            advances: &d::LARGE_ADVANCES,
            widths: &d::LARGE_WIDTHS,
            offsets: &d::LARGE_OFFSETS,
            alpha: &d::LARGE_ALPHA,
        },
    }
}

impl FontData {
    fn index(&self, ch: char) -> Option<usize> {
        self.codes
            .binary_search(&(ch as u32))
            .ok()
            .or_else(|| self.codes.binary_search(&('?' as u32)).ok())
    }
}

pub fn line_height(f: Font) -> i32 {
    data(f).height as i32
}

pub fn text_width(f: Font, s: &str) -> i32 {
    let fd = data(f);
    s.chars().filter_map(|ch| fd.index(ch)).map(|i| fd.advances[i] as i32).sum()
}

/// Текст; (x, y) — левый верхний угол строки. Возвращает ширину.
pub fn text(c: &mut Canvas, f: Font, x: i32, y: i32, s: &str, color: Color) -> i32 {
    let fd = data(f);
    let mut pen = x;
    for ch in s.chars() {
        let Some(i) = fd.index(ch) else { continue };
        let w = fd.widths[i] as usize;
        let off = fd.offsets[i] as usize;
        for gy in 0..fd.height {
            for gx in 0..w {
                let a = fd.alpha[off + gy * w + gx] as u32;
                if a > 0 {
                    c.blend_pixel(pen - 1 + gx as i32, y + gy as i32, color, a);
                }
            }
        }
        pen += fd.advances[i] as i32;
    }
    pen - x
}

pub fn text_centered(c: &mut Canvas, f: Font, x: i32, y: i32, w: i32, s: &str, color: Color) {
    let tw = text_width(f, s);
    text(c, f, x + (w - tw) / 2, y, s, color);
}

/// Текст, повёрнутый на 90° (читается снизу вверх); (x, y) — нижний левый угол.
pub fn text_vertical(c: &mut Canvas, f: Font, x: i32, y: i32, s: &str, color: Color) {
    let fd = data(f);
    let mut pen = 0;
    for ch in s.chars() {
        let Some(i) = fd.index(ch) else { continue };
        let w = fd.widths[i] as usize;
        let off = fd.offsets[i] as usize;
        for gy in 0..fd.height {
            for gx in 0..w {
                let a = fd.alpha[off + gy * w + gx] as u32;
                if a > 0 {
                    c.blend_pixel(x + gy as i32, y - (pen - 1 + gx as i32), color, a);
                }
            }
        }
        pen += fd.advances[i] as i32;
    }
}

/// Обрезать строку с многоточием, чтобы она влезла в `max_w` пикселей.
pub fn fit(f: Font, s: &str, max_w: i32) -> String {
    if text_width(f, s) <= max_w {
        return String::from(s);
    }
    let mut out = String::new();
    for ch in s.chars() {
        out.push(ch);
        out.push('…');
        if text_width(f, &out) > max_w {
            out.pop();
            out.pop();
            out.push('…');
            return out;
        }
        out.pop();
    }
    out
}
