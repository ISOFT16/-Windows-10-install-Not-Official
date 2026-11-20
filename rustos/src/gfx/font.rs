//! Растровый шрифт 8x16 со сглаживанием: латиница, кириллица, псевдографика.

use super::font_data::{ALPHA, CODES};

pub const WIDTH: usize = super::font_data::WIDTH;
pub const HEIGHT: usize = super::font_data::HEIGHT;
const GLYPH_SIZE: usize = WIDTH * HEIGHT;

fn index(c: char) -> Option<usize> {
    CODES.binary_search(&(c as u32)).ok()
}

pub fn has_glyph(c: char) -> bool {
    index(c).is_some()
}

/// Яркость пикселей символа. Для неизвестного символа — знак «?».
pub fn glyph(c: char) -> &'static [u8] {
    let i = index(c).or_else(|| index('?')).unwrap_or(0);
    &ALPHA[i * GLYPH_SIZE..(i + 1) * GLYPH_SIZE]
}
