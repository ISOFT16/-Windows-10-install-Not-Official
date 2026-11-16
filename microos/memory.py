"""Менеджер памяти: страничная модель с выделением страниц процессам."""

from __future__ import annotations

PAGE_SIZE = 4096  # байт в одной странице


class OutOfMemory(Exception):
    pass


class MemoryManager:
    def __init__(self, total_bytes: int = 16 * 1024 * 1024):
        self.total_pages = total_bytes // PAGE_SIZE
        # pages[i] = pid владельца или None, если страница свободна
        self.pages: list[int | None] = [None] * self.total_pages

    @property
    def total(self) -> int:
        return self.total_pages * PAGE_SIZE

    def used_pages(self) -> int:
        return sum(1 for p in self.pages if p is not None)

    @property
    def used(self) -> int:
        return self.used_pages() * PAGE_SIZE

    @property
    def free(self) -> int:
        return self.total - self.used

    def alloc(self, pid: int, size: int) -> list[int]:
        """Выделить процессу pid не менее size байт. Возвращает номера страниц."""
        need = max(1, -(-size // PAGE_SIZE))
        free_idx = [i for i, p in enumerate(self.pages) if p is None]
        if len(free_idx) < need:
            raise OutOfMemory(f"нужно {need} стр., свободно {len(free_idx)}")
        got = free_idx[:need]
        for i in got:
            self.pages[i] = pid
        return got

    def free_pid(self, pid: int) -> int:
        """Освободить все страницы процесса. Возвращает число освобождённых."""
        n = 0
        for i, p in enumerate(self.pages):
            if p == pid:
                self.pages[i] = None
                n += 1
        return n

    def usage_of(self, pid: int) -> int:
        return sum(1 for p in self.pages if p == pid) * PAGE_SIZE


def human(n: int) -> str:
    for unit in ("Б", "КБ", "МБ", "ГБ"):
        if n < 1024 or unit == "ГБ":
            return f"{n:.0f} {unit}" if unit == "Б" else f"{n:.1f} {unit}"
        n /= 1024
    return str(n)
