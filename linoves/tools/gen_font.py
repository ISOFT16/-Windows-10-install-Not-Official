#!/usr/bin/env python3
"""Генерирует src/gfx/font_data.rs: сглаженный моноширинный шрифт 8x16
(латиница, кириллица, псевдографика) из DejaVu Sans Mono.

Использование: python3 tools/gen_font.py [путь к DejaVuSansMono.ttf]
Лицензия шрифта DejaVu (Bitstream Vera) разрешает встраивание и распространение.
"""
import sys
from PIL import Image, ImageDraw, ImageFont

TTF = sys.argv[1] if len(sys.argv) > 1 else "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
W, H, BASELINE, SIZE = 8, 16, 12, 13

chars = [chr(c) for c in range(0x20, 0x7F)]
chars += [chr(c) for c in range(0x410, 0x450)] + ["Ё", "ё"]
chars += list("─│┌┐└┘├┤┬┴┼═║╔╗╚╝█▓▒░■•·°←→↑↓×—«»№▲▼◆✓")

# Линии рамок рисуются программно, чтобы соседние символы соединялись.
# (вверх, вниз, влево, вправо): 0 - нет, 1 - одинарная, 2 - двойная
BOX = {
    "─": (0, 0, 1, 1), "│": (1, 1, 0, 0), "┌": (0, 1, 0, 1), "┐": (0, 1, 1, 0),
    "└": (1, 0, 0, 1), "┘": (1, 0, 1, 0), "├": (1, 1, 0, 1), "┤": (1, 1, 1, 0),
    "┬": (0, 1, 1, 1), "┴": (1, 0, 1, 1), "┼": (1, 1, 1, 1),
    "═": (0, 0, 2, 2), "║": (2, 2, 0, 0), "╔": (0, 2, 0, 2), "╗": (0, 2, 2, 0),
    "╚": (2, 0, 0, 2), "╝": (2, 0, 2, 0),
}
SHADE = {"█": 255, "▓": 190, "▒": 128, "░": 64}

def box_glyph(ch):
    img = Image.new("L", (W, H), 0)
    px = img.load()
    up, down, left, right = BOX[ch]
    cx, cy = 3, 7
    def vline(x, y0, y1):
        for y in range(y0, y1 + 1): px[x, y] = 255
    def hline(y, x0, x1):
        for x in range(x0, x1 + 1): px[x, y] = 255
    for kind, (a, b) in ((up, (0, cy)), (down, (cy, H - 1))):
        if kind == 1: vline(cx, a, b)
        if kind == 2: vline(cx - 1, a, b); vline(cx + 2, a, b)
    for kind, (a, b) in ((left, (0, cx)), (right, (cx, W - 1))):
        if kind == 1: hline(cy, a, b)
        if kind == 2: hline(cy - 1, a, b); hline(cy + 2, a, b)
    return bytes(img.tobytes())

def shade_glyph(ch):
    out = []
    for y in range(H):
        for x in range(W):
            light = (x % 2 == 0) and (y % 2 == 0)
            checker = (x + y) % 2 == 0
            on = {"█": True, "▓": not light, "▒": checker, "░": light}[ch]
            out.append(255 if on else 0)
    return bytes(out)

font = ImageFont.truetype(TTF, SIZE)
ascent, _ = font.getmetrics()
glyphs = []
for ch in chars:
    if ch in BOX:
        glyphs.append((ch, box_glyph(ch)))
        continue
    if ch in SHADE:
        glyphs.append((ch, shade_glyph(ch)))
        continue
    img = Image.new("L", (W, H), 0)
    d = ImageDraw.Draw(img)
    adv = font.getlength(ch)
    x = (W - adv) / 2
    d.text((x, BASELINE - ascent), ch, fill=255, font=font)
    if ch != " " and img.getbbox() is None:
        continue
    glyphs.append((ch, img.tobytes()))

out = []
out.append("// Сгенерировано tools/gen_font.py из DejaVu Sans Mono. Не редактировать вручную.")
out.append("// DejaVu fonts: Bitstream Vera Fonts license (free to embed and redistribute).")
out.append(f"pub const WIDTH: usize = {W};")
out.append(f"pub const HEIGHT: usize = {H};")
out.append(f"pub const COUNT: usize = {len(glyphs)};")
out.append("/// Коды символов по возрастанию (для двоичного поиска).")
glyphs.sort(key=lambda g: ord(g[0]))
out.append("pub static CODES: [u32; COUNT] = [" + ", ".join(str(ord(c)) for c, _ in glyphs) + "];")
out.append("/// Яркость (0..255) каждого пикселя, 8x16 на символ.")
out.append(f"pub static ALPHA: [u8; COUNT * {W*H}] = [")
for ch, data in glyphs:
    out.append("    " + ",".join(str(b) for b in data) + ",")
out.append("];")
open("src/gfx/font_data.rs", "w").write("\n".join(out) + "\n")

# предпросмотр
prev = Image.new("L", (W * 32, H * ((len(glyphs) + 31) // 32)), 0)
for i, (ch, data) in enumerate(glyphs):
    g = Image.frombytes("L", (W, H), data)
    prev.paste(g, ((i % 32) * W, (i // 32) * H))
if len(sys.argv) > 2:
    prev.resize((prev.width * 3, prev.height * 3), Image.NEAREST).save(sys.argv[2])
print(f"{len(glyphs)} glyphs")
