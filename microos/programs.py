"""Встроенные программы, которые можно запускать как процессы (`run <имя>`).

Каждая программа — генератор, получающий ctx (см. kernel.ProcContext).
"""

from __future__ import annotations

import time


def counter(ctx):
    """Считает от 1 до N (по умолчанию 10) раз в секунду."""
    n = int(ctx.args[0]) if ctx.args else 10
    for i in range(1, n + 1):
        ctx.print(f"счёт: {i}")
        yield ("sleep", 1)
    ctx.print("готово")


def clock(ctx):
    """Каждую секунду записывает текущее время."""
    while True:
        ctx.print(time.strftime("%H:%M:%S"))
        yield ("sleep", 1)


def primes(ctx):
    """Ищет простые числа до N, понемногу на каждом кванте времени."""
    limit = int(ctx.args[0]) if ctx.args else 5000
    found = []
    for n in range(2, limit + 1):
        if all(n % p for p in found if p * p <= n):
            found.append(n)
        if n % 50 == 0:
            yield  # отдаём процессор другим
    ctx.print(f"простых чисел до {limit}: {len(found)}")
    ctx.print("последние: " + ", ".join(map(str, found[-10:])))


def memhog(ctx):
    """Постепенно захватывает память (демонстрация менеджера памяти)."""
    kb = int(ctx.args[0]) if ctx.args else 1024
    got = 0
    while got < kb:
        ctx.malloc(64 * 1024)
        got += 64
        ctx.print(f"выделено {got} КБ")
        yield ("sleep", 0.2)
    ctx.print("держу память, пока меня не убьют (kill PID)")
    while True:
        yield ("sleep", 5)


def logger(ctx):
    """Пишет журнал работы в /var/log/logger.log каждые 5 секунд."""
    path = "/var/log/logger.log"
    i = 0
    while True:
        i += 1
        ctx.fs.write(path, f"{time.strftime('%H:%M:%S')} запись #{i}\n", append=True)
        ctx.print(f"запись #{i} в {path}")
        yield ("sleep", 5)


def spinner(ctx):
    """Бесконечный цикл без сна — нагружает процессор (смотрите ps)."""
    while True:
        yield


PROGRAMS = {
    "counter": counter,
    "clock": clock,
    "primes": primes,
    "memhog": memhog,
    "logger": logger,
    "spinner": spinner,
}
