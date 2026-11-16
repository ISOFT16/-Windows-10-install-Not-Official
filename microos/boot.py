"""Загрузчик: создаёт ядро и оболочку, выдаёт сообщения загрузки."""

from __future__ import annotations

from . import NAME, __version__
from .kernel import Kernel
from .shell import Shell

BOOT_STEPS = [
    "BIOS: проверка оборудования ... OK",
    "Загрузчик MicroBoot: чтение ядра ... OK",
    "Ядро: инициализация менеджера памяти ... OK",
    "Ядро: монтирование корневой файловой системы ... OK",
    "Ядро: запуск планировщика процессов ... OK",
    "init: запуск командной оболочки msh ... OK",
]


def boot(disk_path=None):
    """Загрузить систему. Возвращает (kernel, shell)."""
    kernel = Kernel(disk_path)
    kernel.start()
    return kernel, Shell(kernel)


def boot_messages():
    return [f"{NAME} {__version__}"] + BOOT_STEPS
