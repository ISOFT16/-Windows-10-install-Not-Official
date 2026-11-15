[app]

# Название приложения на телефоне
title = MicroOS

# Имя пакета: итоговый идентификатор org.microos.microos
package.name = microos
package.domain = org.microos

# Исходники: main.py и пакет microos/
source.dir = .
source.include_exts = py,png,txt
source.exclude_dirs = tests, recipes, bin, .buildozer, .github, .git, __pycache__
source.exclude_patterns = buildozer.spec, README.md

version = 1.0.0

# Python и Kivy (графический интерфейс)
requirements = python3,kivy==2.3.1

icon.filename = %(source.dir)s/assets/icon.png
presplash.filename = %(source.dir)s/assets/presplash.png
android.presplash_color = #0A0C12

orientation = portrait, landscape
fullscreen = 0

# --- Android ---
android.api = 34
android.minapi = 24
# Только arm64: она есть почти на всех телефонах. Сборка сразу под несколько
# архитектур ломает шаг pip в python-for-android.
android.archs = arm64-v8a

# Свой рецепт Kivy без requests и прочих сетевых зависимостей (см. recipes/kivy)
p4a.local_recipes = ./recipes
android.accept_sdk_license = True
android.allow_backup = True
# Особые разрешения не нужны: диск MicroOS хранится в личной папке приложения
android.permissions =

# Отладочная сборка APK (устанавливается напрямую, без Google Play)
android.release_artifact = apk
android.debug_artifact = apk

[buildozer]
log_level = 2
warn_on_root = 0
