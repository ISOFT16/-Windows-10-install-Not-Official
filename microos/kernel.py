"""Ядро MicroOS: процессы, кооперативный планировщик, системный журнал.

Процесс — это генератор Python. Каждый `yield` отдаёт управление
планировщику (квант времени закончился). `yield ("sleep", секунды)`
усыпляет процесс. Планировщик работает в отдельном потоке по кругу
(round-robin), поэтому фоновые процессы живут, пока вы работаете в оболочке.
"""

import hashlib
import threading
import time

from . import NAME, __version__
from .fs import VFS
from .memory import MemoryManager, OutOfMemory

READY, SLEEPING, DONE, KILLED = "готов", "спит", "завершён", "убит"
TICK = 0.05  # длительность такта планировщика, с


class Process:
    def __init__(self, pid, name, user, args):
        self.pid = pid
        self.name = name
        self.user = user
        self.args = args
        self.state = READY
        self.started = time.time()
        self.wake_at = 0.0
        self.cpu_ticks = 0
        self.stdout: list[str] = []
        self.gen = None

    @property
    def alive(self):
        return self.state in (READY, SLEEPING)


class ProcContext:
    """То, что видит программа изнутри: её API к ядру."""

    def __init__(self, kernel, proc):
        self.kernel = kernel
        self.proc = proc
        self.args = proc.args
        self.fs = kernel.fs

    def print(self, *items):
        line = " ".join(str(i) for i in items)
        self.proc.stdout.append(line)
        del self.proc.stdout[:-200]  # храним последние 200 строк

    def malloc(self, size):
        return self.kernel.mem.alloc(self.proc.pid, size)


def hash_password(pw: str) -> str:
    return hashlib.sha256(("microos:" + pw).encode()).hexdigest()


class Kernel:
    def __init__(self, disk_path=None, memory=16 * 1024 * 1024):
        self.boot_time = time.time()
        self.fs = VFS(disk_path)
        self.mem = MemoryManager(memory)
        self.procs: dict[int, Process] = {}
        self.next_pid = 2  # pid 1 — оболочка
        self.lock = threading.RLock()
        self.dmesg: list[str] = []
        self.programs = {}
        self._running = False
        self._thread = None
        self.log(f"{NAME} {__version__}: ядро загружено")
        self.log(f"память: {self.mem.total // 1024} КБ, страниц: {self.mem.total_pages}")
        self._setup_fs()
        from .programs import PROGRAMS
        self.programs.update(PROGRAMS)
        self.log(f"зарегистрировано программ: {len(self.programs)}")

    # ---------- журнал ----------
    def log(self, msg):
        t = time.time() - self.boot_time
        self.dmesg.append(f"[{t:9.3f}] {msg}")
        del self.dmesg[:-500]

    # ---------- первичная настройка диска ----------
    def _setup_fs(self):
        fs = self.fs
        if fs.exists("/etc/passwd"):
            self.log("диск смонтирован")
            return
        self.log("диск пуст — форматирование и установка системы")
        for d in ("/bin", "/etc", "/home", "/tmp", "/var/log", "/root"):
            fs.mkdir(d, parents=True)
        fs._node("/tmp")["owner"] = "*"
        fs.write("/etc/passwd", f"root:{hash_password('root')}\nuser:{hash_password('')}\n")
        fs.write("/etc/hostname", "microos\n")
        fs.write("/etc/motd", f"Добро пожаловать в {NAME} {__version__}!\n"
                              "Введите 'help', чтобы увидеть список команд.\n")
        fs.mkdir("/home/user", user="root")
        fs._node("/home/user")["owner"] = "user"
        fs.write("/home/user/readme.txt",
                 "Это ваш домашний каталог.\n"
                 "Попробуйте: ls, cat, edit, run clock, ps, neofetch\n", user="user")
        fs.write("/home/user/hello.py",
                 "# Скрипт на Python внутри MicroOS. Запуск: py hello.py\n"
                 "print('Привет из MicroOS!')\n"
                 "for i in range(1, 6):\n"
                 "    print(i, '*', i, '=', i * i)\n", user="user")
        fs.sync()

    # ---------- пользователи ----------
    def users(self) -> dict[str, str]:
        result = {}
        for line in self.fs.read("/etc/passwd").splitlines():
            if ":" in line:
                name, h = line.split(":", 1)
                result[name] = h
        return result

    def check_password(self, user, pw):
        return self.users().get(user) == hash_password(pw)

    def set_password(self, user, pw):
        u = self.users()
        u[user] = hash_password(pw)
        self.fs.write("/etc/passwd", "".join(f"{k}:{v}\n" for k, v in u.items()))

    def add_user(self, user, pw):
        if user in self.users():
            raise ValueError(f"пользователь {user} уже существует")
        if not user.isidentifier():
            raise ValueError("недопустимое имя пользователя")
        self.set_password(user, pw)
        home = f"/home/{user}"
        if not self.fs.exists(home):
            self.fs.mkdir(home)
            self.fs._node(home)["owner"] = user
        self.log(f"создан пользователь {user}")

    # ---------- процессы ----------
    def spawn(self, name, args=(), user="root"):
        if name not in self.programs:
            raise KeyError(name)
        with self.lock:
            pid = self.next_pid
            self.next_pid += 1
            proc = Process(pid, name, user, list(args))
            try:
                self.mem.alloc(pid, 64 * 1024)  # стек + код процесса
            except OutOfMemory as e:
                raise OutOfMemory(f"не удалось запустить {name}: {e}")
            proc.gen = self.programs[name](ProcContext(self, proc))
            self.procs[pid] = proc
        self.log(f"запущен процесс {pid} ({name}) от {user}")
        return pid

    def kill(self, pid, state=KILLED):
        with self.lock:
            proc = self.procs.get(pid)
            if proc is None or not proc.alive:
                return False
            proc.state = state
            try:
                proc.gen.close()
            except Exception:
                pass
            freed = self.mem.free_pid(pid)
        self.log(f"процесс {pid} ({proc.name}): {state}, освобождено страниц: {freed}")
        return True

    def reap(self):
        """Удалить из таблицы завершённые процессы."""
        with self.lock:
            dead = [pid for pid, p in self.procs.items() if not p.alive]
            for pid in dead:
                del self.procs[pid]
        return len(dead)

    def step(self):
        """Один такт планировщика: каждый готовый процесс получает квант."""
        now = time.time()
        with self.lock:
            procs = list(self.procs.values())
        for proc in procs:
            with self.lock:
                if not proc.alive:
                    continue
                if proc.state == SLEEPING:
                    if now < proc.wake_at:
                        continue
                    proc.state = READY
                try:
                    req = next(proc.gen)
                    proc.cpu_ticks += 1
                    if isinstance(req, tuple) and req and req[0] == "sleep":
                        proc.state = SLEEPING
                        proc.wake_at = now + float(req[1])
                except StopIteration:
                    self.kill(proc.pid, DONE)
                except OutOfMemory as e:
                    proc.stdout.append(f"нехватка памяти: {e}")
                    self.kill(proc.pid)
                except Exception as e:  # падение процесса не роняет ядро
                    proc.stdout.append(f"аварийное завершение: {type(e).__name__}: {e}")
                    self.kill(proc.pid)

    # ---------- жизненный цикл ----------
    def start(self):
        if self._running:
            return
        self._running = True
        self._thread = threading.Thread(target=self._loop, name="scheduler", daemon=True)
        self._thread.start()
        self.log("планировщик запущен")

    def _loop(self):
        while self._running:
            self.step()
            time.sleep(TICK)

    def shutdown(self):
        for pid in list(self.procs):
            self.kill(pid)
        self._running = False
        if self._thread:
            self._thread.join(timeout=1)
        self.fs.sync()
        self.log("система остановлена")

    def uptime(self):
        return time.time() - self.boot_time
