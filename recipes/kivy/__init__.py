"""Рецепт Kivy для python-for-android без лишних сетевых зависимостей.

Стандартный рецепт тянет requests/chardet/certifi/idna/urllib3 (нужны только
для kivy.network.UrlRequest). MicroOS их не использует, а свежий
python-for-android ломается на установке Android-wheel charset_normalizer.
Всё остальное берётся из стандартного рецепта без изменений.
"""

import importlib.util
import os

import pythonforandroid

_CORE_DIR = os.path.join(os.path.dirname(pythonforandroid.__file__), "recipes", "kivy")
_spec = importlib.util.spec_from_file_location(
    "p4a_core_kivy_recipe", os.path.join(_CORE_DIR, "__init__.py"))
_core = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_core)


class KivyRecipe(_core.KivyRecipe):
    python_depends = ["filetype"]

    def get_recipe_dir(self):
        return _CORE_DIR  # патчи лежат рядом со стандартным рецептом


recipe = KivyRecipe()
