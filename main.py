"""Точка входа MicroOS (APK и ПК):  python main.py

Если приложение падает при запуске, вместо мгновенного закрытия
на экране показывается текст ошибки, а копия пишется в журнал
(Android logcat, метка "python") и в файл crash.txt рядом с диском.
"""

import os
import traceback


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
    try:
        from app import MicroOSApp
        MicroOSApp().run()
    except Exception:
        show_crash(traceback.format_exc())


if __name__ == "__main__":
    main()
