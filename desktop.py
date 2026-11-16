"""Рабочий стол MicroOS: обои, иконки, панель задач, меню «Пуск» и окна.

Все картинки берутся из папки assets/desktop/. Если какой-то картинки нет,
вместо неё рисуется заглушка-цвет, поэтому стол работает и без текстур.
Список картинок и их размеры — в словаре SKIN ниже.
"""

import os
import time

from kivy.clock import Clock
from kivy.graphics import Color, Rectangle, RoundedRectangle
from kivy.metrics import dp, sp
from kivy.uix.behaviors import ButtonBehavior
from kivy.uix.boxlayout import BoxLayout
from kivy.uix.button import Button
from kivy.uix.floatlayout import FloatLayout
from kivy.uix.image import Image
from kivy.uix.label import Label
from kivy.uix.scrollview import ScrollView
from kivy.uix.textinput import TextInput
from kivy.uix.widget import Widget

SKIN_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "assets", "desktop")

# Имя файла -> (назначение, рекомендуемый размер). Это и есть список текстур.
SKIN = {
    "wallpaper.png":     ("обои рабочего стола", "1080x1920 (растягивается на весь экран)"),
    "taskbar.png":       ("фон панели задач", "1080x120 (растягивается по ширине)"),
    "start.png":         ("кнопка «Пуск»", "128x128, прозрачный фон"),
    "startmenu.png":     ("фон меню «Пуск»", "600x900"),
    "window_title.png":  ("заголовок окна", "600x64 (растягивается по ширине)"),
    "window_body.png":   ("фон окна", "600x600"),
    "btn_close.png":     ("кнопка «закрыть» окна", "64x64, прозрачный фон"),
    "btn_min.png":       ("кнопка «свернуть» окна", "64x64, прозрачный фон"),
    "btn_max.png":       ("кнопка «развернуть» окна", "64x64, прозрачный фон"),
    "icon_terminal.png": ("иконка «Терминал»", "128x128, прозрачный фон"),
    "icon_notes.png":    ("иконка «Заметки»", "128x128, прозрачный фон"),
    "icon_files.png":    ("иконка «Файлы»", "128x128, прозрачный фон"),
    "icon_tasks.png":    ("иконка «Процессы»", "128x128, прозрачный фон"),
    "icon_about.png":    ("иконка «О системе»", "128x128, прозрачный фон"),
}

# Цвета-заглушки, пока нет картинок (и цвет текста)
COLORS = {
    "wallpaper": (0.10, 0.25, 0.45, 1),
    "taskbar": (0.08, 0.10, 0.16, 0.95),
    "startmenu": (0.12, 0.14, 0.22, 0.97),
    "window_title": (0.18, 0.32, 0.55, 1),
    "window_body": (0.95, 0.95, 0.97, 1),
    "text_light": (1, 1, 1, 1),
    "text_dark": (0.1, 0.1, 0.12, 1),
}
TASKBAR_H = dp(48)


def skin(name):
    """Путь к картинке, если она есть, иначе None."""
    path = os.path.join(SKIN_DIR, name)
    return path if os.path.isfile(path) else None


def paint(widget, image_name, color):
    """Фон виджета: картинка из скина или цвет-заглушка."""
    src = skin(image_name)
    with widget.canvas.before:
        if src:
            Color(1, 1, 1, 1)
            rect = Rectangle(source=src, pos=widget.pos, size=widget.size)
        else:
            Color(*color)
            rect = Rectangle(pos=widget.pos, size=widget.size)
    widget.bind(pos=lambda w, v: setattr(rect, "pos", v),
                size=lambda w, v: setattr(rect, "size", v))


class SkinButton(ButtonBehavior, Widget):
    """Кнопка-картинка; без картинки рисуется кружок с символом."""

    def __init__(self, image_name, fallback_text, fallback_color, **kw):
        super().__init__(**kw)
        src = skin(image_name)
        if src:
            self.img = Image(source=src, fit_mode="contain")
            self.add_widget(self.img)
            self.bind(pos=self._sync, size=self._sync)
        else:
            with self.canvas.before:
                Color(*fallback_color)
                self._bg = RoundedRectangle(radius=[dp(6)])
            self.lbl = Label(text=fallback_text, bold=True, font_size=sp(16),
                             font_name="DejaVuSans")
            self.add_widget(self.lbl)
            self.bind(pos=self._sync_fallback, size=self._sync_fallback)

    def _sync(self, *_):
        self.img.pos, self.img.size = self.pos, self.size

    def _sync_fallback(self, *_):
        self._bg.pos, self._bg.size = self.pos, self.size
        self.lbl.pos, self.lbl.size = self.pos, self.size


class FallbackIcon(Widget):
    """Заглушка иконки: цветной квадрат с буквой (пока нет картинки)."""

    def __init__(self, letter, color, **kw):
        super().__init__(**kw)
        with self.canvas.before:
            Color(*color)
            self._bg = RoundedRectangle(radius=[dp(12)])
        self.lbl = Label(text=letter, bold=True, font_size=sp(26), font_name="DejaVuSans")
        self.add_widget(self.lbl)
        self.bind(pos=self._sync, size=self._sync)

    def _sync(self, *_):
        side = min(self.width, self.height)
        self._bg.size = (side, side)
        self._bg.pos = (self.center_x - side / 2, self.y)
        self.lbl.size = (side, side)
        self.lbl.pos = self._bg.pos


class DesktopIcon(ButtonBehavior, BoxLayout):
    """Иконка на рабочем столе: картинка + подпись."""

    def __init__(self, title, image_name, letter, color, **kw):
        super().__init__(orientation="vertical", size_hint=(None, None),
                         size=(dp(84), dp(96)), spacing=dp(2), **kw)
        src = skin(image_name)
        if src:
            pic = Image(source=src, fit_mode="contain", size_hint_y=0.72)
        else:
            pic = FallbackIcon(letter, color, size_hint_y=0.72)
        self.add_widget(pic)
        self.add_widget(Label(text=title, font_size=sp(12), color=COLORS["text_light"],
                              size_hint_y=0.28, halign="center", outline_width=1,
                              outline_color=(0, 0, 0, 1)))


# ======================== ОКНА ========================

class TitleBar(BoxLayout):
    """Заголовок окна: за него окно перетаскивается."""

    def __init__(self, window, title, **kw):
        super().__init__(size_hint_y=None, height=dp(36), padding=(dp(8), dp(3)),
                         spacing=dp(4), **kw)
        self.window = window
        paint(self, "window_title.png", COLORS["window_title"])
        self.add_widget(Label(text=title, bold=True, color=COLORS["text_light"],
                              halign="left", valign="middle", shorten=True,
                              text_size=(None, None)))
        for img, sym, col, cb in (("btn_min.png", "_", (0.3, 0.4, 0.6, 1), window.minimize),
                                  ("btn_max.png", "▢", (0.3, 0.4, 0.6, 1), window.toggle_max),
                                  ("btn_close.png", "×", (0.8, 0.2, 0.2, 1), window.close)):
            b = SkinButton(img, sym, col, size_hint=(None, None), size=(dp(30), dp(30)))
            b.bind(on_release=lambda _b, f=cb: f())
            self.add_widget(b)

    def on_touch_down(self, touch):
        if not self.collide_point(*touch.pos):
            return False
        if super().on_touch_down(touch):  # нажали кнопку в заголовке
            return True
        touch.grab(self)
        self._drag = (touch.x - self.window.x, touch.y - self.window.y)
        return True

    def on_touch_move(self, touch):
        if touch.grab_current is self and not self.window.maximized:
            ws = self.window.parent
            x = touch.x - self._drag[0]
            y = touch.y - self._drag[1]
            if ws:  # не даём утащить окно за пределы стола
                x = max(ws.x - self.window.width + dp(60), min(x, ws.right - dp(60)))
                y = max(ws.y, min(y, ws.top - self.window.height))
            self.window.pos = (x, y)
            return True
        return super().on_touch_move(touch)

    def on_touch_up(self, touch):
        if touch.grab_current is self:
            touch.ungrab(self)
            return True
        return super().on_touch_up(touch)


class AppWindow(BoxLayout):
    """Окно программы."""

    def __init__(self, desktop, app_id, title, content, **kw):
        super().__init__(orientation="vertical", size_hint=(None, None), **kw)
        self.desktop = desktop
        self.app_id = app_id
        self.title = title
        self.maximized = False
        self._saved = None
        paint(self, "window_body.png", COLORS["window_body"])
        self.add_widget(TitleBar(self, title))
        self.content = content
        self.add_widget(content)

    def on_touch_down(self, touch):
        if not self.collide_point(*touch.pos):
            return False
        Clock.schedule_once(lambda dt: self.desktop.raise_window(self), 0)
        super().on_touch_down(touch)
        return True  # окна под этим не получают нажатие

    def minimize(self):
        self.desktop.minimize(self)

    def toggle_max(self):
        ws = self.desktop.workspace
        if self.maximized:
            self.pos, self.size = self._saved
        else:
            self._saved = (tuple(self.pos), tuple(self.size))
            self.pos, self.size = ws.pos, ws.size
        self.maximized = not self.maximized

    def close(self):
        self.desktop.close(self)


# ======================== ПРОГРАММЫ ========================

def make_notes(desktop):
    sh = desktop.app.shell
    path = "/home/user/notes.txt"
    text = sh.fs.read(path) if sh and sh.fs.isfile(path) else ""
    box = BoxLayout(orientation="vertical", padding=dp(4), spacing=dp(4))
    ti = TextInput(text=text, font_size=sp(14))
    status = Label(text=path, color=COLORS["text_dark"], size_hint_x=0.6, font_size=sp(12))

    def save(*_):
        if desktop.app.shell:
            desktop.app.shell.fs.write(path, ti.text, "user")
            desktop.app.shell.fs.sync()
            status.text = "сохранено " + time.strftime("%H:%M:%S")
    row = BoxLayout(size_hint_y=None, height=dp(36))
    b = Button(text="Сохранить", size_hint_x=0.4)
    b.bind(on_release=save)
    row.add_widget(status)
    row.add_widget(b)
    box.add_widget(ti)
    box.add_widget(row)
    return box


def make_files(desktop):
    box = BoxLayout(orientation="vertical", padding=dp(4), spacing=dp(2))
    path_lbl = Label(size_hint_y=None, height=dp(24), color=COLORS["text_dark"], bold=True)
    sv = ScrollView()
    lst = BoxLayout(orientation="vertical", size_hint_y=None, spacing=dp(2))
    lst.bind(minimum_height=lst.setter("height"))
    sv.add_widget(lst)
    viewer = Label(text="", color=COLORS["text_dark"], size_hint_y=None, height=0,
                   halign="left", valign="top", font_size=sp(12))
    viewer.bind(width=lambda l, w: setattr(l, "text_size", (w, None)))
    state = {"cwd": "/home/user"}

    def show(path):
        fs = desktop.app.shell.fs if desktop.app.shell else None
        if not fs:
            return
        state["cwd"] = path
        path_lbl.text = path
        lst.clear_widgets()
        viewer.text, viewer.height = "", 0
        entries = ([".."] if path != "/" else []) + fs.listdir(path)
        for name in entries:
            full = os.path.normpath(os.path.join(path, name)).replace("\\", "/")
            is_dir = fs.isdir(full)
            b = Button(text=("[папка] " if is_dir else "") + name, size_hint_y=None,
                       height=dp(34), halign="left")
            b.bind(on_release=lambda _b, f=full, d=is_dir: show(f) if d else open_file(f))
            lst.add_widget(b)

    def open_file(path):
        text = desktop.app.shell.fs.read(path)
        viewer.text = f"--- {path} ---\n{text[:3000]}"
        viewer.height = dp(160)

    box.add_widget(path_lbl)
    box.add_widget(sv)
    box.add_widget(viewer)
    Clock.schedule_once(lambda dt: show(state["cwd"]), 0)
    return box


def make_text_app(desktop, command, refresh=None):
    """Окно, показывающее вывод команды оболочки (ps, neofetch и т.п.)."""
    sv = ScrollView()
    lbl = Label(color=COLORS["text_dark"], font_name="RobotoMono-Regular", font_size=sp(11),
                halign="left", valign="top", size_hint_y=None, padding=(dp(6), dp(6)))
    lbl.bind(width=lambda l, w: setattr(l, "text_size", (w, None)),
             texture_size=lambda l, s: setattr(l, "height", s[1]))
    sv.add_widget(lbl)

    def update(*_):
        sh = desktop.app.shell
        if sh is None:
            return False
        lbl.text = sh.execute(command)
    Clock.schedule_once(update, 0)
    if refresh:
        ev = Clock.schedule_interval(update, refresh)
        sv.bind(parent=lambda w, p: ev.cancel() if p is None else None)
    return sv


# Программы рабочего стола: id -> (название, иконка, буква-заглушка, цвет, создать содержимое)
APPS = {
    "terminal": ("Терминал", "icon_terminal.png", ">_", (0.15, 0.15, 0.18, 1),
                 lambda d: d.app.term),
    "notes":    ("Заметки", "icon_notes.png", "З", (0.85, 0.65, 0.15, 1), make_notes),
    "files":    ("Файлы", "icon_files.png", "Ф", (0.25, 0.55, 0.85, 1), make_files),
    "tasks":    ("Процессы", "icon_tasks.png", "П", (0.35, 0.65, 0.35, 1),
                 lambda d: make_text_app(d, "top", refresh=1.0)),
    "about":    ("О системе", "icon_about.png", "i", (0.55, 0.35, 0.75, 1),
                 lambda d: make_text_app(d, "neofetch")),
}


# ======================== СТОЛ ========================

class StartMenu(BoxLayout):
    def __init__(self, desktop, **kw):
        super().__init__(orientation="vertical", size_hint=(None, None),
                         padding=dp(8), spacing=dp(4), **kw)
        self.desktop = desktop
        paint(self, "startmenu.png", COLORS["startmenu"])
        self.add_widget(Label(text="MicroOS", bold=True, font_size=sp(18),
                              color=COLORS["text_light"], size_hint_y=None, height=dp(36)))
        for app_id, (title, *_rest) in APPS.items():
            b = Button(text=title, size_hint_y=None, height=dp(40))
            b.bind(on_release=lambda _b, a=app_id: self._run(a))
            self.add_widget(b)
        self.add_widget(Widget())
        for text, mode in (("Перезагрузка", "reboot"), ("Выключение", "shutdown")):
            b = Button(text=text, size_hint_y=None, height=dp(40),
                       background_color=(0.8, 0.3, 0.3, 1))
            b.bind(on_release=lambda _b, m=mode: self._power(m))
            self.add_widget(b)
        self.size = (dp(220), dp(56) + (len(APPS) + 2) * dp(44) + dp(30))

    def _run(self, app_id):
        self.desktop.toggle_start(False)
        self.desktop.open_app(app_id)

    def _power(self, mode):
        self.desktop.toggle_start(False)
        self.desktop.app.power(mode)

    def on_touch_down(self, touch):
        super().on_touch_down(touch)
        return self.collide_point(*touch.pos)


class Taskbar(BoxLayout):
    def __init__(self, desktop, **kw):
        super().__init__(size_hint=(1, None), height=TASKBAR_H, padding=dp(4),
                         spacing=dp(4), pos=(0, 0), **kw)
        self.desktop = desktop
        paint(self, "taskbar.png", COLORS["taskbar"])
        start = SkinButton("start.png", "Пуск", (0.2, 0.55, 0.3, 1),
                           size_hint=(None, 1), width=dp(64))
        start.bind(on_release=lambda *_: desktop.toggle_start())
        self.add_widget(start)
        self.tasks = BoxLayout(spacing=dp(4))
        self.add_widget(self.tasks)
        self.clock = Label(size_hint_x=None, width=dp(56), color=COLORS["text_light"],
                           font_size=sp(13))
        self.add_widget(self.clock)
        Clock.schedule_interval(self._tick, 1)
        self._tick()

    def _tick(self, *_):
        self.clock.text = time.strftime("%H:%M")

    def refresh(self):
        self.tasks.clear_widgets()
        for w in self.desktop.windows:
            hidden = w.parent is None
            b = Button(text=w.title, shorten=True,
                       background_color=(0.4, 0.4, 0.5, 1) if hidden else (0.3, 0.5, 0.8, 1))
            b.bind(on_release=lambda _b, win=w: self.desktop.toggle_window(win))
            self.tasks.add_widget(b)


class Desktop(FloatLayout):
    def __init__(self, app, **kw):
        super().__init__(**kw)
        self.app = app
        self.windows = []
        paint(self, "wallpaper.png", COLORS["wallpaper"])

        self.workspace = FloatLayout(size_hint=(1, None), pos=(0, TASKBAR_H))
        self.bind(size=self._layout)
        self.add_widget(self.workspace)

        # иконки слева сверху, в столбик
        self.icons = BoxLayout(orientation="vertical", size_hint=(None, None),
                               spacing=dp(6), padding=dp(8))
        for app_id, (title, img, letter, color, _make) in APPS.items():
            ic = DesktopIcon(title, img, letter, color)
            ic.bind(on_release=lambda _i, a=app_id: self.open_app(a))
            self.icons.add_widget(ic)
        self.icons.size = (dp(100), len(APPS) * dp(102) + dp(16))
        self.workspace.add_widget(self.icons)

        self.taskbar = Taskbar(self)
        self.add_widget(self.taskbar)
        self.start_menu = StartMenu(self, pos=(0, TASKBAR_H))
        self.start_open = False

    def _layout(self, *_):
        self.workspace.height = self.height - TASKBAR_H
        self.workspace.y = TASKBAR_H
        self.icons.pos = (self.workspace.x, self.workspace.top - self.icons.height)
        for w in self.windows:
            if w.maximized:
                w.pos, w.size = self.workspace.pos, self.workspace.size

    # ----- меню «Пуск» -----
    def toggle_start(self, show=None):
        show = (not self.start_open) if show is None else show
        if show and not self.start_open:
            self.add_widget(self.start_menu)
        elif not show and self.start_open:
            self.remove_widget(self.start_menu)
        self.start_open = show

    def on_touch_down(self, touch):
        if self.start_open and not self.start_menu.collide_point(*touch.pos) \
                and not self.taskbar.collide_point(*touch.pos):
            self.toggle_start(False)
        return super().on_touch_down(touch)

    # ----- окна -----
    def open_app(self, app_id, maximized=False):
        for w in self.windows:  # уже открыто — просто показать
            if w.app_id == app_id:
                if w.parent is None:
                    self.workspace.add_widget(w)
                self.raise_window(w)
                return w
        title, _img, _l, _c, make = APPS[app_id]
        content = make(self)
        if content.parent:
            content.parent.remove_widget(content)
        ws = self.workspace
        w = AppWindow(self, app_id, title, content,
                      size=(min(ws.width * 0.92, dp(560)), min(ws.height * 0.8, dp(520))))
        n = len(self.windows) % 5
        w.pos = (ws.x + dp(20) + n * dp(24), ws.top - w.height - dp(20) - n * dp(24))
        self.windows.append(w)
        ws.add_widget(w)
        if maximized:
            w.toggle_max()
        self.taskbar.refresh()
        return w

    def raise_window(self, w):
        if w.parent is self.workspace and self.workspace.children[0] is not w:
            self.workspace.remove_widget(w)
            self.workspace.add_widget(w)

    def minimize(self, w):
        if w.parent:
            w.parent.remove_widget(w)
        self.taskbar.refresh()

    def toggle_window(self, w):
        if w.parent is None:
            self.workspace.add_widget(w)
        elif self.workspace.children[0] is w:
            self.minimize(w)
            return
        else:
            self.raise_window(w)
        self.taskbar.refresh()

    def close(self, w):
        if w.parent:
            w.parent.remove_widget(w)
        if w.content.parent is w:
            w.remove_widget(w.content)  # терминал можно открыть снова
        self.windows.remove(w)
        self.taskbar.refresh()
