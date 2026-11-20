#!/usr/bin/env python3
"""Генерирует src/gfx/ui_data.rs: значки Lucide (theme/lucide/*.svg) и шрифт
интерфейса DejaVu Sans (со сглаживанием и кириллицей).

Нужно: pip install pillow cairosvg
Использование: python3 tools/gen_ui.py [путь к папке со шрифтами DejaVu]
"""
import io, os, sys
import cairosvg
from PIL import Image, ImageDraw, ImageFont

FONT_DIR = sys.argv[1] if len(sys.argv) > 1 else "/usr/share/fonts/truetype/dejavu"
ICON_DIR = "theme/lucide"
OUT = "src/gfx/ui_data.rs"
ICON_SIZES = [16, 20, 24, 40, 48]
FONTS = [  # имя, файл, размер в пикселях
    ("REGULAR", "DejaVuSans.ttf", 13),
    ("BOLD", "DejaVuSans-Bold.ttf", 13),
    ("LARGE", "DejaVuSans-Bold.ttf", 20),
]
CHARS = [chr(c) for c in range(0x20, 0x7F)] + [chr(c) for c in range(0x410, 0x450)] + list("Ёё·—«»№°…×←→")

out = ["// Сгенерировано tools/gen_ui.py. Не редактировать вручную.",
       "// Значки: Lucide (ISC License, theme/lucide/LICENSE).",
       "// Шрифт: DejaVu Sans (Bitstream Vera License).",
       "#![allow(dead_code)]"]

def arr(name, data, ty="u8"):
    lines = [f"pub static {name}: [{ty}; {len(data)}] = ["]
    for i in range(0, len(data), 48):
        lines.append("    " + ",".join(str(v) for v in data[i:i + 48]) + ",")
    lines.append("];")
    return "\n".join(lines)

# ---------------- значки ----------------
names = sorted(f[:-4] for f in os.listdir(ICON_DIR) if f.endswith(".svg"))
out.append("#[derive(Clone, Copy, PartialEq, Eq, Debug)]")
out.append("pub enum Icon {")
for n in names:
    out.append("    " + "".join(p.capitalize() for p in n.split("-")) + ",")
out.append("}")
out.append(f"pub const ICON_SIZES: [usize; {len(ICON_SIZES)}] = {ICON_SIZES};")
alpha, offsets = [], []
for n in names:
    svg = open(os.path.join(ICON_DIR, n + ".svg"), "rb").read()
    for s in ICON_SIZES:
        png = cairosvg.svg2png(bytestring=svg, output_width=s, output_height=s)
        im = Image.open(io.BytesIO(png)).convert("RGBA")
        offsets.append(len(alpha))
        alpha += list(im.getchannel("A").tobytes())
out.append(arr("ICON_OFFSETS", offsets, "u32"))
out.append(arr("ICON_ALPHA", alpha))

# ---------------- шрифты ----------------
for fname, ffile, size in FONTS:
    font = ImageFont.truetype(os.path.join(FONT_DIR, ffile), size)
    ascent, descent = font.getmetrics()
    height = ascent + descent
    codes, advances, widths, offs, data = [], [], [], [], []
    for ch in sorted(set(CHARS), key=ord):
        adv = font.getlength(ch)
        w = int(adv + 0.999) + 2
        img = Image.new("L", (w, height), 0)
        ImageDraw.Draw(img).text((1, 0), ch, fill=255, font=font)
        if ch != " " and img.getbbox() is None:
            continue
        codes.append(ord(ch)); advances.append(max(1, round(adv))); widths.append(w)
        offs.append(len(data)); data += list(img.tobytes())
    out.append(f"pub const {fname}_HEIGHT: usize = {height};")
    out.append(f"pub const {fname}_ASCENT: usize = {ascent};")
    out.append(arr(f"{fname}_CODES", codes, "u32"))
    out.append(arr(f"{fname}_ADVANCES", advances))
    out.append(arr(f"{fname}_WIDTHS", widths))
    out.append(arr(f"{fname}_OFFSETS", offs, "u32"))
    out.append(arr(f"{fname}_ALPHA", data))

open(OUT, "w").write("\n".join(out) + "\n")
print(f"{len(names)} icons x {len(ICON_SIZES)} sizes, {len(FONTS)} fonts -> {OUT} ({os.path.getsize(OUT)//1024} KiB)")
