"""MicroOS для Android (и для ПК) — графический терминал на Kivy.

Запускается через main.py (точка входа APK), который ловит ошибки старта.
"""

from __future__ import annotations

import os
import traceback

from kivy.app import App
from kivy.clock import Clock
from kivy.core.window import Window
from kivy.graphics import Color, Rectangle
from kivy.metrics import dp, sp
from kivy.uix.boxlayout import BoxLayout
from kivy.uix.button import Button
from kivy.uix.label import Label
from kivy.uix.scrollview import ScrollView
from kivy.uix.textinput import TextInput
from kivy.utils import platform

from microos import NAME
from microos.boot import boot, boot_messages

BG = (0.04, 0.05, 0.07, 1)
FG = (0.75, 0.95, 0.75, 1)
BAR = (0.10, 0.12, 0.16, 1)
FONT = "RobotoMono-Regular"
MAX_LINES = 600
FONT_SIZE = sp(13)


class Terminal(BoxLayout):
    def __init__(self, app, **kw):
        super().__init__(orientation="vertical", **kw)
        self.app = app
        with self.canvas.before:  # свой тёмный фон: терминал живёт в окне рабочего стола
            Color(*BG)
            self._bg = Rectangle(pos=self.pos, size=self.size)
        self.bind(pos=lambda w, v: setattr(self._bg, "pos", v),
                  size=lambda w, v: setattr(self._bg, "size", v))
        self.font_size = FONT_SIZE
        self.lines: list[str] = []
        self.hist_pos = None

        # --- экран вывода ---
        self.scroll = ScrollView(do_scroll_x=False, bar_width=dp(4))
        self.screen = Label(text="", font_name=FONT, font_size=self.font_size, color=FG,
                            halign="left", valign="top", size_hint_y=None,
                            padding=(dp(8), dp(8)))
        self.screen.bind(width=self._rewrap, texture_size=self._resize)
        self.scroll.add_widget(self.screen)
        self.add_widget(self.scroll)

        # --- строка ввода ---
        row = BoxLayout(size_hint_y=None, height=dp(44), padding=(dp(6), 0), spacing=dp(4))
        self.prompt = Label(text="", font_name=FONT, font_size=self.font_size, color=(0.5, 0.8, 1, 1),
                            size_hint_x=None, halign="left", valign="middle")
        self.prompt.bind(texture_size=lambda l, s: setattr(l, "width", min(s[0], self.width * 0.5)))
        self.input = TextInput(multiline=False, font_name=FONT, font_size=self.font_size,
                               background_color=BAR, foreground_color=FG, cursor_color=FG,
                               write_tab=False, text_validate_unfocus=False,
                               padding=(dp(6), dp(12)))
        self.input.bind(on_text_validate=self.on_enter)
        row.add_widget(self.prompt)
        row.add_widget(self.input)
        self.add_widget(row)

        # --- панель клавиш для телефона ---
        keys = BoxLayout(size_hint_y=None, height=dp(42), spacing=dp(2), padding=dp(2))
        for label, cb in (("Tab", self.key_tab), ("↑", self.key_up), ("↓", self.key_down),
                          ("Ctrl+C", self.key_cancel), ("ls", lambda: self.quick("ls")),
                          ("help", lambda: self.quick("help")), ("A-", lambda: self.zoom(-1)),
                          ("A+", lambda: self.zoom(1))):
            b = Button(text=label, font_size=sp(14), background_normal="", font_name="DejaVuSans",
                       background_color=BAR, color=FG)
            b.bind(on_release=lambda _b, f=cb: f())
            keys.add_widget(b)
        self.add_widget(keys)

    # ---------- вывод ----------
    def write(self, text):
        if text is None:
            return
        self.lines.extend(str(text).split("\n"))
        del self.lines[:-MAX_LINES]
        self.screen.text = "\n".join(self.lines)
        Clock.schedule_once(lambda dt: setattr(self.scroll, "scroll_y", 0), 0.05)

    def clear(self):
        self.lines = []
        self.screen.text = ""

    def _rewrap(self, *_):
        self.screen.text_size = (self.screen.width, None)
        self.update_columns()

    def update_columns(self):
        """Сообщить оболочке, сколько символов помещается в строку."""
        if self.app.shell:
            char_w = self.font_size * 0.6  # ширина символа моноширинного шрифта
            self.app.shell.columns = max(20, int((self.screen.width - dp(16)) / char_w))

    def _resize(self, *_):
        self.screen.height = max(self.screen.texture_size[1], self.scroll.height)

    def zoom(self, d):
        self.font_size = max(sp(8), min(sp(28), self.font_size + sp(d)))
        for w in (self.screen, self.prompt, self.input):
            w.font_size = self.font_size
        self.update_columns()

    def refresh_prompt(self):
        sh = self.app.shell
        self.prompt.text = sh.prompt if sh else ""
        self.input.password = bool(sh and sh.secret)

    def focus(self, *_):
        self.input.focus = True

    # ---------- ввод ----------
    def on_enter(self, *_):
        sh = self.app.shell
        if sh is None:
            return
        line = self.input.text
        self.input.text = ""
        self.hist_pos = None
        shown = "*" * len(line) if sh.secret else line
        self.write(sh.prompt + shown)
        try:
            out = sh.execute(line)
        except Exception:
            out = "внутренняя ошибка:\n" + traceback.format_exc()
        if sh.clear_requested:
            sh.clear_requested = False
            self.clear()
        if out:
            self.write(out)
        self.refresh_prompt()
        if sh.power:
            self.app.power(sh.power)
            return
        Clock.schedule_once(self.focus, 0.1)

    def quick(self, cmd):
        self.input.text = cmd
        self.on_enter()

    def key_tab(self):
        sh = self.app.shell
        if not sh:
            return
        line = self.input.text
        done = sh.complete(line)
        if done == line:
            opts = sh.completions(line)
            if len(opts) > 1:
                self.write("  ".join(opts))
        self.input.text = done
        self.input.cursor = (len(done), 0)
        self.focus()

    def _hist(self, step):
        sh = self.app.shell
        if not sh or not sh.history:
            return
        n = len(sh.history)
        pos = n if self.hist_pos is None else self.hist_pos
        pos = max(0, min(n, pos + step))
        self.hist_pos = pos
        self.input.text = sh.history[pos] if pos < n else ""
        self.input.cursor = (len(self.input.text), 0)
        self.focus()

    def key_up(self):
        self._hist(-1)

    def key_down(self):
        self._hist(1)

    def key_cancel(self):
        sh = self.app.shell
        if sh:
            sh._done_asking()
        self.input.text = ""
        self.write("^C")
        self.refresh_prompt()
        self.focus()


class MicroOSApp(App):
    title = NAME

    def __init__(self, previous_crash="", **kw):
        super().__init__(**kw)
        self.previous_crash = previous_crash

    def build(self):
        Window.clearcolor = BG
        if platform == "android":
            Window.softinput_mode = "below_target"
        Window.bind(on_keyboard=self._on_key)
        self.kernel = None
        self.shell = None
        self.term = Terminal(self)
        from desktop import Desktop
        self.desktop = Desktop(self)
        Clock.schedule_once(lambda dt: self.desktop.open_app("terminal", maximized=True), 0)
        Clock.schedule_once(lambda dt: self.start_boot(), 0.2)
        return self.desktop

    def disk_path(self):
        try:
            base = self.user_data_dir  # на Android — личная папка приложения
        except OSError:
            base = os.path.join(os.path.expanduser("~"), ".microos")
        return os.path.join(base, "disk.json")

    def start_boot(self):
        self.term.clear()
        msgs = boot_messages()

        def step(i):
            if i < len(msgs):
                self.term.write(msgs[i])
                Clock.schedule_once(lambda dt: step(i + 1), 0.15)
                return
            try:
                self.kernel, self.shell = boot(self.disk_path())
            except Exception:
                self.term.write("ОШИБКА ЗАГРУЗКИ:\n" + traceback.format_exc())
                return
            self.term.write("")
            if self.previous_crash:
                self.term.write("ВНИМАНИЕ: прошлый запуск завершился сбоем. "
                                "Пришлите этот текст разработчику:\n" + self.previous_crash)
                self.previous_crash = ""
            self.term.write(self.shell.banner())
            self.term.update_columns()
            self.term.refresh_prompt()
            self.term.focus()
        step(0)

    def power(self, mode):
        if self.kernel:
            self.kernel.shutdown()
        self.kernel = self.shell = None
        self.term.refresh_prompt()
        if mode == "reboot":
            Clock.schedule_once(lambda dt: self.start_boot(), 1.0)
        else:
            self.term.write("Питание можно отключить. До свидания!")
            Clock.schedule_once(lambda dt: self.stop(), 1.5)

    def _on_key(self, window, key, *args):
        # клавиша "Назад" на Android (27) не закрывает систему мгновенно
        if key == 27:
            if self.shell and self.shell.pending:
                self.term.key_cancel()
            return True
        if key == 273 and self.term.input.focus:  # стрелка вверх на клавиатуре
            self.term.key_up()
            return True
        if key == 274 and self.term.input.focus:
            self.term.key_down()
            return True
        if key == 9 and self.term.input.focus:  # Tab
            self.term.key_tab()
            return True
        return False

    def on_pause(self):
        if self.kernel:
            self.kernel.fs.sync()
        return True

    def on_resume(self):
        self.term.focus()

    def on_stop(self):
        if self.kernel:
            self.kernel.shutdown()
