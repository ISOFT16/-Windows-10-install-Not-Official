"""Точка входа MicroOS (APK и ПК):  python main.py

Если приложение падает при запуске, вместо мгновенного закрытия
на экране показывается текст ошибки, а копия пишется в журнал
(Android logcat, метка "python") и в файл crash.txt рядом с диском.
"""

from __future__ import annotations

import faulthandler
import os
import traceback

# Отчёт о прошлом нативном сбое (segfault и т.п.), записанный faulthandler.
PREVIOUS_CRASH = ""


def _setup_faulthandler():
    """Пишем трассировку нативных сбоев в файл и показываем её при следующем запуске."""
    global PREVIOUS_CRASH
    base = os.environ.get("ANDROID_PRIVATE") or os.path.join(os.path.expanduser("~"), ".microos")
    try:
        os.makedirs(base, exist_ok=True)
        path = os.path.join(base, "microos_fault.txt")
        if os.path.exists(path):
            with open(path, encoding="utf-8", errors="replace") as f:
                PREVIOUS_CRASH = f.read().strip()
        if PREVIOUS_CRASH:
            print("MICROOS PREVIOUS NATIVE CRASH:\n" + PREVIOUS_CRASH)
        fault_file = open(path, "w")
        faulthandler.enable(file=fault_file, all_threads=True)
        return fault_file
    except Exception:
        print("MICROOS: faulthandler не включён\n" + traceback.format_exc())
        return None


def show_crash(text):
    print("MICROOS CRASH:\n" + text)  # попадает в logcat
    try:
        from kivy.app import App
        from kivy.uix.label import Label
        from kivy.uix.scrollview import ScrollView

        class CrashApp(App):
            title = "MicroOS — ошибка"

            def build(self):
                try:
                    path = os.path.join(self.user_data_dir, "crash.txt")
                    with open(path, "w", encoding="utf-8") as f:
                        f.write(text)
                except Exception:
                    pass
                sv = ScrollView()
                lbl = Label(text="MicroOS не смог запуститься.\n"
                                 "Пришлите этот текст разработчику:\n\n" + text,
                            halign="left", valign="top", size_hint_y=None,
                            color=(1, 0.6, 0.6, 1), padding=(10, 10))
                lbl.bind(width=lambda l, w: setattr(l, "text_size", (w, None)),
                         texture_size=lambda l, s: setattr(l, "height", s[1]))
                sv.add_widget(lbl)
                return sv

        CrashApp().run()
    except Exception:
        print("MICROOS: не удалось показать экран ошибки\n" + traceback.format_exc())


def main():
    _fault_file = _setup_faulthandler()  # noqa: F841 (файл должен оставаться открытым)
    try:
        from app import MicroOSApp
        MicroOSApp(previous_crash=PREVIOUS_CRASH).run()
    except Exception:
        show_crash(traceback.format_exc())


if __name__ == "__main__":
    main()
