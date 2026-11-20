#!/usr/bin/env python3
"""Превращает рисунок рабочего стола theme/desktop.png в данные для ядра
(src/desktop/theme_data.rs). Внешний вид берётся из рисунка пиксель в пиксель:
панель задач, меню «Пуск», значок «Корзина» и значки программ.

Использование: python3 tools/gen_theme.py
"""
from PIL import Image

SRC = "theme/desktop.png"
OUT = "src/desktop/theme_data.rs"

img = Image.open(SRC).convert("RGBA")
bg = Image.new("RGBA", img.size, (255, 255, 255, 255))
bg.alpha_composite(img)
g = bg.convert("L")
W, H = g.size
px = g.load()

# --- области на рисунке (координаты в пикселях рисунка) ---
TASKBAR_TOP = 510            # первая строка панели задач
MENU = (0, 277, 244, 510)    # x0, y0, x1, y1 (не включая x1, y1)
BIN = (4, 6, 60, 72)         # значок «Корзина» с подписью
TRAY = (872, 512, 899, 535)  # значки в правом углу панели (без часов)
CLOCK = (870, 512, W - 1, 535)  # здесь на рисунке часы и значки — стираем, рисуем живые
ICONS = {                    # значки пунктов меню (для кнопок на панели задач)
    "welcome": (2, 282, 24, 304),
    "files": (2, 310, 28, 332),
    "mail": (2, 336, 28, 358),
    "browser": (2, 361, 28, 383),
}

def crop(box):
    x0, y0, x1, y1 = box
    return [px[x, y] for y in range(y0, y1) for x in range(x0, x1)], x1 - x0, y1 - y0

def is_dark(x, y):
    return px[x, y] < 128

# панель задач: стираем нарисованные часы и значки (вместо них — настоящее время)
for y in range(CLOCK[1], CLOCK[3]):
    for x in range(CLOCK[0], CLOCK[2]):
        px[x, y] = 255
taskbar, tb_w, tb_h = crop((0, TASKBAR_TOP, W, H))
# значки трея берём из исходного рисунка (до стирания)
g2 = bg.convert("L")
tray = [g2.getpixel((x, y)) for y in range(TRAY[1], TRAY[3]) for x in range(TRAY[0], TRAY[2])]

# кнопки на панели: ищем вертикальные разделители по средней строке панели
mid = TASKBAR_TOP + 15
runs, cur = [], None
for x in range(W):
    d = is_dark(x, mid)
    if d and cur is None:
        cur = x
    if not d and cur is not None:
        runs.append((cur, x - 1))
        cur = None
START = (0, runs[2][1] + 1)  # кнопка «A»: до третьего тёмного участка
slots = []
seps = [r for r in runs if r[0] > START[1] and r[1] < 660]
prev_end = START[1]
for a, b in seps:
    if a - prev_end > 12:
        slots.append((prev_end + 1, a - 1))
    prev_end = b
FIELD_X = seps[-1][1] + 1  # начало пустого поля справа

# меню: прозрачным становится белый фон снаружи рамок (заливка от краёв)
mx0, my0, mx1, my1 = MENU
mw, mh = mx1 - mx0, my1 - my0
alpha = [[255] * mw for _ in range(mh)]
stack = [(x, y) for x in range(mw) for y in (0, mh - 1)] + [(x, y) for y in range(mh) for x in (0, mw - 1)]
seen = set()
while stack:
    x, y = stack.pop()
    if (x, y) in seen or not (0 <= x < mw and 0 <= y < mh):
        continue
    seen.add((x, y))
    if px[mx0 + x, my0 + y] < 200:
        continue
    alpha[y][x] = 0
    stack += [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]
menu = [px[mx0 + x, my0 + y] for y in range(mh) for x in range(mw)]
menu_alpha = [alpha[y][x] for y in range(mh) for x in range(mw)]

def arr(name, data):
    lines = [f"pub static {name}: [u8; {len(data)}] = ["]
    for i in range(0, len(data), 64):
        lines.append("    " + ",".join(str(v) for v in data[i:i + 64]) + ",")
    lines.append("];")
    return "\n".join(lines)

out = ["// Сгенерировано tools/gen_theme.py из theme/desktop.png. Не редактировать вручную.",
       "#![allow(dead_code)]",
       f"pub const DESIGN_W: i32 = {W};",
       f"pub const DESIGN_H: i32 = {H};",
       f"pub const TASKBAR_TOP: i32 = {TASKBAR_TOP};",
       f"pub const TASKBAR_W: usize = {tb_w};",
       f"pub const TASKBAR_H: usize = {tb_h};",
       f"pub const FIELD_X: i32 = {FIELD_X};",
       f"pub const START: (i32, i32) = {START};",
       "pub const SLOTS: [(i32, i32); %d] = [%s];" % (len(slots), ", ".join(f"({a}, {b})" for a, b in slots)),
       arr("TASKBAR", taskbar),
       f"pub const MENU_X: i32 = {mx0};",
       f"pub const MENU_Y: i32 = {my0};",
       f"pub const MENU_W: usize = {mw};",
       f"pub const MENU_H: usize = {mh};",
       arr("MENU", menu),
       arr("MENU_ALPHA", menu_alpha),
       f"pub const BIN_X: i32 = {BIN[0]};",
       f"pub const BIN_Y: i32 = {BIN[1]};",
       f"pub const BIN_W: usize = {BIN[2] - BIN[0]};",
       f"pub const BIN_H: usize = {BIN[3] - BIN[1]};",
       arr("BIN", crop(BIN)[0]),
       f"pub const TRAY_W: usize = {TRAY[2] - TRAY[0]};",
       f"pub const TRAY_H: usize = {TRAY[3] - TRAY[1]};",
       f"pub const TRAY_Y: i32 = {TRAY[1]};",
       arr("TRAY", tray)]
for name, box in ICONS.items():
    data, w, h = crop(box)
    n = name.upper()
    out += [f"pub const ICON_{n}_W: usize = {w};", f"pub const ICON_{n}_H: usize = {h};", arr(f"ICON_{n}", data)]
open(OUT, "w").write("\n".join(out) + "\n")
print("slots:", len(slots), slots[:3], "...", "start:", START, "field:", FIELD_X)
