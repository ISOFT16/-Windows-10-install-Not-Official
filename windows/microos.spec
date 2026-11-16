# -*- mode: python -*-
# Сборка MicroOS для Windows 7+ (PyInstaller 5, Python 3.7):
#   pyinstaller --noconfirm windows/microos.spec
# Результат: dist/MicroOS/ (MicroOS.exe — графика, MicroOS-Console.exe — консоль)

import os
from kivy_deps import sdl2, glew
from kivy.tools.packaging.pyinstaller_hooks import get_deps_minimal, hookspath, runtime_hooks

ROOT = os.path.abspath(os.path.join(SPECPATH, ".."))
deps = get_deps_minimal(video=None, audio=None, camera=None, spelling=None)
deps["hiddenimports"] = deps.get("hiddenimports", []) + ["app", "desktop"]
# pywin32 не нужен; его загрузчик (add_dll_directory) не работает на чистой Windows 7
PYWIN32 = ["win32api", "win32con", "win32gui", "win32file", "pywintypes", "pythoncom",
           "win32com", "win32timezone", "pywin32_system32"]
deps["excludes"] = deps.get("excludes", []) + PYWIN32
try:
    from kivy_deps import angle
    dep_bins = sdl2.dep_bins + glew.dep_bins + angle.dep_bins
except ImportError:
    dep_bins = sdl2.dep_bins + glew.dep_bins

gui = Analysis(
    [os.path.join(ROOT, "main.py")],
    pathex=[ROOT],
    datas=[(os.path.join(ROOT, "assets"), "assets")],
    hookspath=hookspath(),
    runtime_hooks=runtime_hooks(),
    **deps,
)
console = Analysis(
    [os.path.join(ROOT, "windows", "console_main.py")],
    pathex=[ROOT],
    excludes=["kivy", "kivy_deps"] + PYWIN32,
)

gui_exe = EXE(
    PYZ(gui.pure), gui.scripts, [], exclude_binaries=True,
    name="MicroOS", console=False, icon=os.path.join(ROOT, "assets", "icon.ico"),
)
console_exe = EXE(
    PYZ(console.pure), console.scripts, [], exclude_binaries=True,
    name="MicroOS-Console", console=True, icon=os.path.join(ROOT, "assets", "icon.ico"),
)

COLLECT(
    gui_exe, gui.binaries, gui.zipfiles, gui.datas,
    console_exe, console.binaries, console.zipfiles, console.datas,
    *[Tree(p) for p in dep_bins],
    name="MicroOS",
)
