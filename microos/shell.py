"""Командная оболочка MicroOS (msh).

Оболочка не зависит от способа ввода-вывода: метод execute() принимает
строку и возвращает текст. Поэтому один и тот же код работает и в
консоли на компьютере, и в графическом приложении на Android.

Поддерживается: конвейеры `|`, перенаправление `>` и `>>`, цепочки `;`,
переменные `$VAR`, история, автодополнение по Tab.
"""

from __future__ import annotations

import ast
import calendar
import fnmatch
import math
import operator
import posixpath
import re
import sys
import threading
import time

from . import NAME, __version__
from .fs import FSError
from .memory import human
from .kernel import OutOfMemory


class ShellError(Exception):
    pass


COMMANDS = {}


def command(name, usage, aliases=()):
    def deco(fn):
        fn.usage = usage
        COMMANDS[name] = fn
        for a in aliases:
            COMMANDS[a] = fn
        fn.cmd_name = name
        return fn
    return deco


LOGO = r"""
  __  __ _               ___  ____
 |  \/  (_) ___ _ __ ___/ _ \/ ___|
 | |\/| | |/ __| '__/ _ \ | | \___ \
 | |  | | | (__| | | (_) | |_| |___) |
 |_|  |_|_|\___|_|  \___/\___/|____/
"""


class Shell:
    def __init__(self, kernel):
        self.k = kernel
        self.fs = kernel.fs
        self.user = "user"
        self.user_stack: list[tuple[str, str]] = []
        self.cwd = "/home/user"
        self.env = {"HOME": "/home/user", "USER": "user", "SHELL": "/bin/msh", "PATH": "/bin"}
        self.history: list[str] = []
        self.pending = None   # обработчик следующей строки (редактор, пароль)
        self.pending_prompt = ""
        self.secret = False   # следующий ввод — пароль (скрывать)
        self.clear_requested = False
        self.power = None     # "shutdown" / "reboot"
        self.columns = 80     # ширина экрана в символах (интерфейс может менять)

    # ---------- приглашение ----------
    @property
    def prompt(self):
        if self.pending:
            return self.pending_prompt
        home = self.env.get("HOME", "")
        cwd = self.cwd
        if home and (cwd == home or cwd.startswith(home + "/")):
            cwd = "~" + cwd[len(home):]
        sign = "#" if self.user == "root" else "$"
        return f"{self.user}@microos:{cwd}{sign} "

    def banner(self):
        motd = self.fs.read("/etc/motd") if self.fs.isfile("/etc/motd") else ""
        return LOGO.strip("\n") + "\n\n" + motd

    # ---------- вспомогательное ----------
    def path(self, p):
        if p == "~" or p.startswith("~/"):
            p = self.env.get("HOME", "/") + p[1:]
        return self.fs.resolve(p, self.cwd)

    def _expand(self, line):
        def repl(m):
            name = m.group(1) or m.group(2)
            if name == "?":
                return "0"
            return self.env.get(name, "")
        return re.sub(r"\$\{(\w+)\}|\$(\w+|\?)", repl, line)

    def ask(self, prompt, handler, secret=False):
        """Попросить ввести следующую строку (для паролей, редактора и т.п.)."""
        self.pending = handler
        self.pending_prompt = prompt
        self.secret = secret

    def _done_asking(self):
        self.pending = None
        self.pending_prompt = ""
        self.secret = False

    # ---------- выполнение ----------
    def execute(self, line: str) -> str:
        if self.pending:
            handler = self.pending
            self._done_asking()
            try:
                out = handler(line)
            except (ShellError, FSError, ValueError) as e:
                out = f"ошибка: {e}"
            self.fs.sync()
            return out or ""

        line = line.strip()
        if not line or line.startswith("#"):
            return ""
        if line.startswith("!") and line[1:].isdigit():
            idx = int(line[1:]) - 1
            if not 0 <= idx < len(self.history):
                return "msh: нет такой команды в истории"
            line = self.history[idx]
        self.history.append(line)
        del self.history[:-500]

        try:
            tokens = list(self._tokenize(self._expand(line)))
        except ValueError as e:
            return f"msh: ошибка разбора: {e}"

        outputs = []
        for seq in self._split(tokens, ";"):
            if not seq:
                continue
            with self.k.lock:
                out = self._run_pipeline(seq)
            if out:
                outputs.append(out.rstrip("\n"))
            if self.power:
                break
        try:
            self.fs.sync()
        except OSError as e:
            outputs.append(f"msh: не удалось записать диск: {e}")
        return "\n".join(outputs)

    @staticmethod
    def _tokenize(line):
        """Разбивает строку на слова и операторы | ; & > >>, учитывая кавычки.

        Свой разборщик вместо shlex: в Python 3.7 shlex не отделяет «;» от слова.
        """
        tokens, buf = [], []
        quoted = False  # было ли слово в кавычках (тогда пустое слово "" тоже токен)
        quote = None
        i, n = 0, len(line)

        def flush():
            nonlocal quoted
            if buf or quoted:
                tokens.append("".join(buf))
                buf.clear()
            quoted = False

        while i < n:
            c = line[i]
            if quote:
                if c == quote:
                    quote = None
                elif c == "\\" and quote == '"' and i + 1 < n and line[i + 1] in '"\\$':
                    i += 1
                    buf.append(line[i])
                else:
                    buf.append(c)
            elif c in "\"'":
                quote, quoted = c, True
            elif c == "\\" and i + 1 < n:
                i += 1
                buf.append(line[i])
            elif c.isspace():
                flush()
            elif c in "|;&":
                flush()
                tokens.append(c)
            elif c == ">":
                flush()
                if i + 1 < n and line[i + 1] == ">":
                    tokens.append(">>")
                    i += 1
                else:
                    tokens.append(">")
            else:
                buf.append(c)
            i += 1
        if quote:
            raise ValueError("не закрыта кавычка")
        flush()
        return tokens

    @staticmethod
    def _split(tokens, sep):
        cur = []
        for t in tokens:
            if t == sep:
                yield cur
                cur = []
            else:
                cur.append(t)
        yield cur

    def _run_pipeline(self, tokens):
        redirect = None
        if tokens and tokens[-1] == "&":
            tokens = tokens[:-1]
        for op in (">>", ">"):
            if op in tokens:
                i = tokens.index(op)
                if i + 1 >= len(tokens):
                    return "msh: не указан файл для перенаправления"
                redirect = (op, tokens[i + 1])
                tokens = tokens[:i] + tokens[i + 2:]
                break
        data = None
        for seg in self._split(tokens, "|"):
            if not seg:
                return "msh: синтаксическая ошибка рядом с '|'"
            data = self._run_one(seg[0], seg[1:], data)
            if data is None:
                data = ""
        if redirect:
            op, target = redirect
            try:
                text = data if (not data or data.endswith("\n")) else data + "\n"
                self.fs.write(self.path(target), text, self.user, append=(op == ">>"))
            except FSError as e:
                return f"msh: {e}"
            return ""
        return data

    def _run_one(self, name, args, stdin):
        fn = COMMANDS.get(name)
        if fn is None:
            # исполняемый скрипт msh или python из файловой системы
            p = self.path(name)
            if "/" in name and self.fs.isfile(p):
                if p.endswith(".py"):
                    fn, args = COMMANDS["py"], [name] + args
                else:
                    fn, args = COMMANDS["sh"], [name] + args
            else:
                return f"msh: команда не найдена: {name}  (введите help)"
        try:
            return fn(self, args, stdin)
        except ShellError as e:
            return f"{fn.cmd_name}: {e}"
        except FSError as e:
            return f"{fn.cmd_name}: {e}"
        except OutOfMemory as e:
            return f"{fn.cmd_name}: нехватка памяти: {e}"
        except (ValueError, IndexError, KeyError) as e:
            return f"{fn.cmd_name}: неверные аргументы ({e}). Использование: {fn.usage}"

    # ---------- автодополнение ----------
    def complete(self, line: str) -> str:
        parts = line.split(" ")
        word = parts[-1]
        if len(parts) == 1:
            cands = sorted(c for c in COMMANDS if c.startswith(word))
        else:
            d, base = posixpath.split(word)
            dpath = self.path(d or ".")
            try:
                names = self.fs.listdir(dpath)
            except FSError:
                return line
            cands = []
            for n in names:
                if n.startswith(base):
                    full = posixpath.join(d, n) if d else n
                    if self.fs.isdir(posixpath.join(dpath, n)):
                        full += "/"
                    cands.append(full)
        if not cands:
            return line
        prefix = cands[0]
        for c in cands[1:]:
            while not c.startswith(prefix):
                prefix = prefix[:-1]
        if len(cands) == 1 and len(parts) == 1:
            prefix += " "
        parts[-1] = prefix
        return " ".join(parts)

    def completions(self, line: str) -> list[str]:
        word = line.split(" ")[-1]
        if " " not in line:
            return sorted(c for c in COMMANDS if c.startswith(word))
        return []


# ====================== КОМАНДЫ ======================

def _need_root(sh):
    if sh.user != "root":
        raise ShellError("нужны права root (используйте su)")


def _flags(args):
    flags = set()
    rest = []
    for a in args:
        if a.startswith("-") and len(a) > 1 and not a[1:].isdigit():
            flags.update(a[1:])
        else:
            rest.append(a)
    return flags, rest


def _read_input(sh, args, stdin):
    if args:
        return "".join(sh.fs.read(sh.path(a)) for a in args)
    return stdin or ""


@command("help", "help [команда]", aliases=("?",))
def cmd_help(sh, args, stdin):
    if args:
        fn = COMMANDS.get(args[0])
        if not fn:
            raise ShellError(f"нет такой команды: {args[0]}")
        return f"{fn.usage}\n  {(fn.__doc__ or '').strip()}"
    seen, lines = set(), []
    for name, fn in sorted(COMMANDS.items()):
        if fn in seen:
            continue
        seen.add(fn)
        lines.append(f"  {fn.usage:<28} {(fn.__doc__ or '').strip()}")
    return (f"{NAME} {__version__} — доступные команды:\n" + "\n".join(lines) +
            "\n\nКонвейеры: cmd | cmd   Перенаправление: > и >>   Цепочки: ;")


# ---------- файлы ----------
@command("pwd", "pwd")
def cmd_pwd(sh, args, stdin):
    """текущий каталог"""
    return sh.cwd


@command("cd", "cd [каталог]")
def cmd_cd(sh, args, stdin):
    """сменить каталог"""
    target = sh.path(args[0]) if args else sh.env.get("HOME", "/")
    if args and args[0] == "-":
        target = sh.env.get("OLDPWD", sh.cwd)
    if not sh.fs.isdir(target):
        raise ShellError(f"нет такого каталога: {args[0] if args else target}")
    sh.env["OLDPWD"] = sh.cwd
    sh.cwd = target
    return ""


@command("ls", "ls [-la] [путь]", aliases=("dir",))
def cmd_ls(sh, args, stdin):
    """список файлов"""
    flags, rest = _flags(args)
    p = sh.path(rest[0]) if rest else sh.cwd
    if sh.fs.isfile(p):
        names, base = [posixpath.basename(p)], posixpath.dirname(p)
    else:
        names, base = sh.fs.listdir(p), p
    if "a" not in flags:
        names = [n for n in names if not n.startswith(".")]
    if "l" not in flags:
        return "  ".join(n + ("/" if sh.fs.isdir(posixpath.join(base, n)) else "") for n in names)
    out = []
    for n in names:
        st = sh.fs.stat(posixpath.join(base, n))
        kind = "d" if st["type"] == "dir" else "-"
        t = time.strftime("%d.%m %H:%M", time.localtime(st["mtime"]))
        out.append(f"{kind} {st['owner']:<6} {st['size']:>7}  {t}  {n}")
    return "\n".join(out) if out else ""


@command("cat", "cat <файл>...")
def cmd_cat(sh, args, stdin):
    """показать содержимое"""
    return _read_input(sh, args, stdin)


@command("echo", "echo [текст]")
def cmd_echo(sh, args, stdin):
    """вывести текст"""
    return " ".join(args)


@command("touch", "touch <файл>...")
def cmd_touch(sh, args, stdin):
    """создать пустой файл"""
    if not args:
        raise ShellError("укажите файл")
    for a in args:
        sh.fs.touch(sh.path(a), sh.user)
    return ""


@command("mkdir", "mkdir [-p] <каталог>...")
def cmd_mkdir(sh, args, stdin):
    """создать каталог"""
    flags, rest = _flags(args)
    if not rest:
        raise ShellError("укажите имя каталога")
    for a in rest:
        sh.fs.mkdir(sh.path(a), sh.user, parents="p" in flags)
    return ""


@command("rm", "rm [-r] <путь>...", aliases=("del",))
def cmd_rm(sh, args, stdin):
    """удалить файл или каталог"""
    flags, rest = _flags(args)
    if not rest:
        raise ShellError("укажите, что удалить")
    for a in rest:
        p = sh.path(a)
        if p == "/":
            raise ShellError("удалять корень запрещено")
        sh.fs.remove(p, sh.user, recursive="r" in flags or "f" in flags)
    return ""


@command("rmdir", "rmdir <каталог>")
def cmd_rmdir(sh, args, stdin):
    """удалить пустой каталог"""
    for a in args:
        p = sh.path(a)
        if not sh.fs.isdir(p):
            raise ShellError(f"это не каталог: {a}")
        sh.fs.remove(p, sh.user)
    return ""


@command("cp", "cp <откуда> <куда>", aliases=("copy",))
def cmd_cp(sh, args, stdin):
    """копировать"""
    _, rest = _flags(args)
    sh.fs.copy(sh.path(rest[0]), sh.path(rest[1]), sh.user)
    return ""


@command("mv", "mv <откуда> <куда>", aliases=("move", "ren"))
def cmd_mv(sh, args, stdin):
    """переместить / переименовать"""
    sh.fs.move(sh.path(args[0]), sh.path(args[1]), sh.user)
    return ""


@command("tree", "tree [путь]")
def cmd_tree(sh, args, stdin):
    """дерево каталогов"""
    root = sh.path(args[0]) if args else sh.cwd
    if not sh.fs.isdir(root):
        raise ShellError(f"нет такого каталога: {root}")
    lines = [root]
    dirs = files = 0

    def walk(p, prefix):
        nonlocal dirs, files
        names = sh.fs.listdir(p)
        for i, n in enumerate(names):
            last = i == len(names) - 1
            full = posixpath.join(p, n)
            isd = sh.fs.isdir(full)
            lines.append(prefix + ("`-- " if last else "|-- ") + n + ("/" if isd else ""))
            if isd:
                dirs += 1
                walk(full, prefix + ("    " if last else "|   "))
            else:
                files += 1
    walk(root, "")
    lines.append(f"\nкаталогов: {dirs}, файлов: {files}")
    return "\n".join(lines)


@command("find", "find [путь] [-name шаблон]")
def cmd_find(sh, args, stdin):
    """поиск файлов"""
    pattern = "*"
    if "-name" in args:
        i = args.index("-name")
        pattern = args[i + 1]
        args = args[:i] + args[i + 2:]
    root = sh.path(args[0]) if args else sh.cwd
    return "\n".join(p for p, _ in sh.fs.walk(root) if fnmatch.fnmatch(posixpath.basename(p), pattern))


@command("stat", "stat <путь>")
def cmd_stat(sh, args, stdin):
    """информация о файле"""
    p = sh.path(args[0])
    st = sh.fs.stat(p)
    t = time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(st["mtime"]))
    kind = "каталог" if st["type"] == "dir" else "файл"
    size = f"{st['size']} элем." if st["type"] == "dir" else f"{st['size']} байт"
    return f"  Путь: {p}\n   Тип: {kind}\nРазмер: {size}\nВладелец: {st['owner']}\nИзменён: {t}"


@command("chown", "chown <пользователь> <путь>")
def cmd_chown(sh, args, stdin):
    """сменить владельца (root)"""
    _need_root(sh)
    user, p = args[0], sh.path(args[1])
    if user not in sh.k.users() and user != "*":
        raise ShellError(f"нет такого пользователя: {user}")
    if not sh.fs.exists(p):
        raise ShellError(f"нет такого файла: {args[1]}")
    sh.fs._node(p)["owner"] = user
    return ""


# ---------- текст ----------
@command("grep", "grep [-i] <шаблон> [файл]")
def cmd_grep(sh, args, stdin):
    """поиск строк"""
    flags, rest = _flags(args)
    pat = re.compile(rest[0], re.I if "i" in flags else 0)
    text = _read_input(sh, rest[1:], stdin)
    return "\n".join(l for l in text.splitlines() if pat.search(l))


@command("wc", "wc [файл]")
def cmd_wc(sh, args, stdin):
    """строки, слова, символы"""
    text = _read_input(sh, args, stdin)
    return f"{len(text.splitlines())} {len(text.split())} {len(text)}"


def _n_arg(args, default=10):
    n, rest = default, []
    it = iter(args)
    for a in it:
        if a == "-n":
            n = int(next(it))
        elif a.startswith("-") and a[1:].isdigit():
            n = int(a[1:])
        else:
            rest.append(a)
    return n, rest


@command("head", "head [-n N] [файл]")
def cmd_head(sh, args, stdin):
    """первые строки"""
    n, rest = _n_arg(args)
    return "\n".join(_read_input(sh, rest, stdin).splitlines()[:n])


@command("tail", "tail [-n N] [файл]")
def cmd_tail(sh, args, stdin):
    """последние строки"""
    n, rest = _n_arg(args)
    lines = _read_input(sh, rest, stdin).splitlines()
    return "\n".join(lines[-n:] if n else [])


@command("sort", "sort [-r] [файл]")
def cmd_sort(sh, args, stdin):
    """сортировка строк"""
    flags, rest = _flags(args)
    return "\n".join(sorted(_read_input(sh, rest, stdin).splitlines(), reverse="r" in flags))


@command("uniq", "uniq [файл]")
def cmd_uniq(sh, args, stdin):
    """убрать повторы подряд"""
    out = []
    for l in _read_input(sh, args, stdin).splitlines():
        if not out or out[-1] != l:
            out.append(l)
    return "\n".join(out)


@command("edit", "edit <файл>", aliases=("nano", "notepad"))
def cmd_edit(sh, args, stdin):
    """простой текстовый редактор"""
    if not args:
        raise ShellError("укажите файл")
    p = sh.path(args[0])
    lines = sh.fs.read(p).splitlines() if sh.fs.isfile(p) else []
    if sh.fs.isdir(p):
        raise ShellError(f"это каталог: {args[0]}")

    def show():
        if not lines:
            return "  (файл пуст)"
        return "\n".join(f"{i:>3}| {l}" for i, l in enumerate(lines, 1))

    def handler(line):
        cmd = line.strip()
        if cmd == ":w":
            sh.fs.write(p, "\n".join(lines) + ("\n" if lines else ""), sh.user)
            return f"сохранено: {p} ({len(lines)} строк)"
        if cmd == ":q":
            return "выход без сохранения"
        out = ""
        if cmd == ":p":
            out = show()
        elif cmd == ":c":
            lines.clear()
            out = "файл очищен"
        elif re.fullmatch(r":d\s*\d+", cmd):
            n = int(cmd[2:])
            if 1 <= n <= len(lines):
                out = f"удалена строка {n}: {lines.pop(n - 1)}"
            else:
                out = "нет такой строки"
        elif re.fullmatch(r":r\s*\d+\s.*", cmd):
            m = re.match(r":r\s*(\d+)\s(.*)", cmd)
            n = int(m.group(1))
            if 1 <= n <= len(lines):
                lines[n - 1] = m.group(2)
                out = f"строка {n} заменена"
            else:
                out = "нет такой строки"
        elif cmd == ":h":
            out = EDIT_HELP
        else:
            lines.append(line)
        sh.ask(f"{len(lines) + 1:>3}> ", handler)
        return out

    sh.ask(f"{len(lines) + 1:>3}> ", handler)
    return f"-- редактор: {p} --\n{show()}\n{EDIT_HELP}"


EDIT_HELP = ("Вводите строки — они добавляются в конец.\n"
             ":w сохранить и выйти  :q выйти  :p показать  :d N удалить строку\n"
             ":r N текст заменить строку  :c очистить  :h справка")


# ---------- процессы ----------
@command("ps", "ps", aliases=("jobs", "tasklist"))
def cmd_ps(sh, args, stdin):
    """список процессов"""
    rows = [f"{'PID':>5} {'ПОЛЬЗ.':<7} {'СОСТОЯНИЕ':<9} {'ТАКТЫ':>7} {'ПАМЯТЬ':>9} {'ВРЕМЯ':>7}  ИМЯ",
            f"{1:>5} {sh.user:<7} {'работает':<9} {'-':>7} {'-':>9} {_fmt_time(sh.k.uptime()):>7}  msh"]
    for p in sorted(sh.k.procs.values(), key=lambda p: p.pid):
        rows.append(f"{p.pid:>5} {p.user:<7} {p.state:<9} {p.cpu_ticks:>7} "
                    f"{human(sh.k.mem.usage_of(p.pid)):>9} {_fmt_time(time.time() - p.started):>7}  "
                    f"{' '.join([p.name] + p.args)}")
    return "\n".join(rows)


@command("top", "top")
def cmd_top(sh, args, stdin):
    """процессы и загрузка системы"""
    alive = [p for p in sh.k.procs.values() if p.alive]
    head = (f"время работы {_fmt_time(sh.k.uptime())}, процессов: {len(alive)}, "
            f"память: {human(sh.k.mem.used)} / {human(sh.k.mem.total)}\n")
    procs = sorted(alive, key=lambda p: -p.cpu_ticks)
    rows = [f"{'PID':>5} {'ТАКТЫ':>8} {'ПАМЯТЬ':>9}  ИМЯ"]
    rows += [f"{p.pid:>5} {p.cpu_ticks:>8} {human(sh.k.mem.usage_of(p.pid)):>9}  {p.name}" for p in procs]
    return head + "\n".join(rows)


@command("run", "run <программа> [аргументы]", aliases=("start",))
def cmd_run(sh, args, stdin):
    """запустить фоновый процесс"""
    if not args:
        return "Доступные программы:\n" + "\n".join(
            f"  {n:<10} {(f.__doc__ or '').strip()}" for n, f in sorted(sh.k.programs.items()))
    try:
        pid = sh.k.spawn(args[0], args[1:], sh.user)
    except KeyError:
        raise ShellError(f"нет такой программы: {args[0]} (список: run)")
    return f"[{pid}] запущен {args[0]}  — вывод: out {pid}, остановить: kill {pid}"


@command("kill", "kill <PID>...", aliases=("taskkill",))
def cmd_kill(sh, args, stdin):
    """завершить процесс"""
    if not args:
        raise ShellError("укажите PID")
    out = []
    for a in args:
        pid = int(a)
        if pid == 1:
            out.append("нельзя убить оболочку (используйте shutdown)")
            continue
        p = sh.k.procs.get(pid)
        if p and sh.user != "root" and p.user != sh.user:
            out.append(f"{pid}: отказано в доступе")
        elif sh.k.kill(pid):
            out.append(f"процесс {pid} завершён")
        else:
            out.append(f"{pid}: нет такого живого процесса")
    return "\n".join(out)


@command("out", "out <PID> [-n N]")
def cmd_out(sh, args, stdin):
    """вывод фонового процесса"""
    n, rest = _n_arg(args, 20)
    p = sh.k.procs.get(int(rest[0]))
    if not p:
        raise ShellError(f"нет процесса {rest[0]}")
    lines = p.stdout[-n:]
    return "\n".join(lines) if lines else "(процесс ещё ничего не вывел)"


@command("reap", "reap")
def cmd_reap(sh, args, stdin):
    """убрать завершённые процессы"""
    return f"удалено записей: {sh.k.reap()}"


# ---------- система ----------
def _fmt_time(sec):
    sec = int(sec)
    h, rem = divmod(sec, 3600)
    m, s = divmod(rem, 60)
    return f"{h}:{m:02}:{s:02}" if h else f"{m}:{s:02}"


@command("uptime", "uptime")
def cmd_uptime(sh, args, stdin):
    """время работы"""
    alive = sum(1 for p in sh.k.procs.values() if p.alive)
    return f"{time.strftime('%H:%M:%S')} работает {_fmt_time(sh.k.uptime())}, процессов: {alive + 1}"


@command("free", "free", aliases=("mem",))
def cmd_free(sh, args, stdin):
    """использование памяти"""
    m = sh.k.mem
    bar_len = 30
    used = int(bar_len * m.used / m.total)
    return (f"всего:    {human(m.total)}\nзанято:   {human(m.used)} ({m.used_pages()} стр.)\n"
            f"свободно: {human(m.free)}\n[{'#' * used}{'.' * (bar_len - used)}]")


@command("df", "df")
def cmd_df(sh, args, stdin):
    """использование диска"""
    files, dirs, size = sh.fs.usage()
    return f"диск: {sh.fs.image_path or '(в памяти)'}\nфайлов: {files}, каталогов: {dirs}, данных: {human(size)}"


@command("dmesg", "dmesg")
def cmd_dmesg(sh, args, stdin):
    """журнал ядра"""
    return "\n".join(sh.k.dmesg)


@command("date", "date")
def cmd_date(sh, args, stdin):
    """дата и время"""
    return time.strftime("%A, %d %B %Y, %H:%M:%S")


@command("cal", "cal [месяц год]")
def cmd_cal(sh, args, stdin):
    """календарь"""
    t = time.localtime()
    month = int(args[0]) if args else t.tm_mon
    year = int(args[1]) if len(args) > 1 else t.tm_year
    return calendar.month(year, month)


@command("uname", "uname [-a]")
def cmd_uname(sh, args, stdin):
    """информация о системе"""
    if "-a" in args:
        return f"{NAME} microos {__version__} python-{sys.version.split()[0]} {sys.platform}"
    return NAME


@command("hostname", "hostname")
def cmd_hostname(sh, args, stdin):
    """имя компьютера"""
    return sh.fs.read("/etc/hostname").strip()


@command("neofetch", "neofetch", aliases=("about", "ver"))
def cmd_neofetch(sh, args, stdin):
    """сведения о системе с логотипом"""
    m = sh.k.mem
    info = [f"{sh.user}@microos", "-" * 16, f"ОС: {NAME} {__version__}",
            f"Ядро: microkernel/py{sys.version.split()[0]}", f"Время работы: {_fmt_time(sh.k.uptime())}",
            f"Оболочка: msh", f"Процессов: {1 + sum(p.alive for p in sh.k.procs.values())}",
            f"Память: {human(m.used)} / {human(m.total)}", f"Платформа: {sys.platform}"]
    logo = LOGO.strip("\n").splitlines()
    w = max(len(l) for l in logo)
    if w + 3 + max(len(i) for i in info) > sh.columns:  # узкий экран: друг под другом
        return "\n".join(logo + [""] + info)
    rows = max(len(logo), len(info))
    return "\n".join(f"{(logo[i] if i < len(logo) else ''):<{w}}   {(info[i] if i < len(info) else '')}"
                     for i in range(rows))


@command("clear", "clear", aliases=("cls",))
def cmd_clear(sh, args, stdin):
    """очистить экран"""
    sh.clear_requested = True
    return ""


@command("history", "history")
def cmd_history(sh, args, stdin):
    """история команд (повтор: !N)"""
    return "\n".join(f"{i:>4}  {c}" for i, c in enumerate(sh.history, 1))


@command("env", "env")
def cmd_env(sh, args, stdin):
    """переменные окружения"""
    return "\n".join(f"{k}={v}" for k, v in sorted(sh.env.items()))


@command("export", "export ИМЯ=значение", aliases=("set",))
def cmd_export(sh, args, stdin):
    """задать переменную"""
    if not args:
        return cmd_env(sh, args, stdin)
    for a in args:
        if "=" not in a:
            raise ShellError("формат: ИМЯ=значение")
        k, v = a.split("=", 1)
        sh.env[k] = v
    return ""


@command("unset", "unset ИМЯ")
def cmd_unset(sh, args, stdin):
    """удалить переменную"""
    for a in args:
        sh.env.pop(a, None)
    return ""


@command("sync", "sync")
def cmd_sync(sh, args, stdin):
    """записать данные на диск"""
    sh.fs.sync()
    return "данные записаны на диск"


@command("shutdown", "shutdown", aliases=("poweroff", "halt"))
def cmd_shutdown(sh, args, stdin):
    """выключить систему"""
    sh.power = "shutdown"
    return "Система выключается..."


@command("reboot", "reboot")
def cmd_reboot(sh, args, stdin):
    """перезагрузка"""
    sh.power = "reboot"
    return "Перезагрузка..."


# ---------- пользователи ----------
@command("whoami", "whoami")
def cmd_whoami(sh, args, stdin):
    """текущий пользователь"""
    return sh.user


@command("users", "users")
def cmd_users(sh, args, stdin):
    """список пользователей"""
    return "  ".join(sh.k.users())


def _switch_user(sh, user):
    sh.user_stack.append((sh.user, sh.cwd))
    sh.user = user
    home = "/root" if user == "root" else f"/home/{user}"
    sh.env.update(USER=user, HOME=home)
    if sh.fs.isdir(home):
        sh.cwd = home


@command("su", "su [пользователь]")
def cmd_su(sh, args, stdin):
    """войти как другой пользователь"""
    user = args[0] if args else "root"
    if user not in sh.k.users():
        raise ShellError(f"нет такого пользователя: {user}")
    if sh.user == "root":
        _switch_user(sh, user)
        return ""

    def handler(pw):
        if sh.k.check_password(user, pw):
            _switch_user(sh, user)
            sh.k.log(f"su: вход {user}")
            return f"вы вошли как {user} (выход: exit)"
        sh.k.log(f"su: неверный пароль для {user}")
        return "su: неверный пароль"
    sh.ask("Пароль: ", handler, secret=True)
    return f"Пароль {user} (по умолчанию у root: root):"


@command("exit", "exit", aliases=("logout",))
def cmd_exit(sh, args, stdin):
    """выйти из su"""
    if not sh.user_stack:
        return "это основной сеанс; для выключения — shutdown"
    sh.user, sh.cwd = sh.user_stack.pop()
    sh.env.update(USER=sh.user, HOME="/root" if sh.user == "root" else f"/home/{sh.user}")
    return f"вы снова {sh.user}"


@command("useradd", "useradd <имя>", aliases=("adduser",))
def cmd_useradd(sh, args, stdin):
    """создать пользователя (root)"""
    _need_root(sh)
    name = args[0]
    if name in sh.k.users():
        raise ShellError(f"пользователь {name} уже существует")

    def handler(pw):
        sh.k.add_user(name, pw)
        return f"пользователь {name} создан, домашний каталог /home/{name}"
    sh.ask("Новый пароль: ", handler, secret=True)
    return ""


@command("passwd", "passwd [пользователь]")
def cmd_passwd(sh, args, stdin):
    """сменить пароль"""
    user = args[0] if args else sh.user
    if user != sh.user:
        _need_root(sh)
    if user not in sh.k.users():
        raise ShellError(f"нет такого пользователя: {user}")

    def handler(pw):
        sh.k.set_password(user, pw)
        return f"пароль для {user} изменён"
    sh.ask("Новый пароль: ", handler, secret=True)
    return ""


# ---------- программирование ----------
_OPS = {ast.Add: operator.add, ast.Sub: operator.sub, ast.Mult: operator.mul,
        ast.Div: operator.truediv, ast.FloorDiv: operator.floordiv, ast.Mod: operator.mod,
        ast.Pow: operator.pow, ast.USub: operator.neg, ast.UAdd: operator.pos}
_MATH = {k: getattr(math, k) for k in dir(math) if not k.startswith("_")}
_MATH.update(abs=abs, round=round, min=min, max=max)


def _calc(node):
    if isinstance(node, ast.Expression):
        return _calc(node.body)
    if isinstance(node, ast.Constant) and isinstance(node.value, (int, float)):
        return node.value
    if sys.version_info < (3, 8) and isinstance(node, ast.Num):  # Python 3.7
        return node.n
    if isinstance(node, ast.BinOp) and type(node.op) in _OPS:
        left, right = _calc(node.left), _calc(node.right)
        if isinstance(node.op, ast.Pow) and abs(right) > 1000:
            raise ValueError("слишком большая степень")
        return _OPS[type(node.op)](left, right)
    if isinstance(node, ast.UnaryOp) and type(node.op) in _OPS:
        return _OPS[type(node.op)](_calc(node.operand))
    if isinstance(node, ast.Name) and node.id in _MATH:
        return _MATH[node.id]
    if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id in _MATH:
        return _MATH[node.func.id](*[_calc(a) for a in node.args])
    raise ValueError("недопустимое выражение")


@command("calc", "calc <выражение>")
def cmd_calc(sh, args, stdin):
    """калькулятор: calc 2+2*sqrt(16)"""
    expr = " ".join(args) or (stdin or "").strip()
    if not expr:
        raise ShellError("введите выражение, например: calc (1+2)*3")
    try:
        res = _calc(ast.parse(expr.replace("^", "**"), mode="eval"))
    except ZeroDivisionError:
        raise ShellError("деление на ноль")
    except SyntaxError:
        raise ShellError("ошибка в выражении")
    if isinstance(res, float) and res.is_integer() and abs(res) < 1e15:
        res = int(res)
    return str(res)


class _Timeout(Exception):
    pass


_SAFE_BUILTINS = {n: __builtins__[n] if isinstance(__builtins__, dict) else getattr(__builtins__, n)
                  for n in ("abs", "all", "any", "bool", "chr", "dict", "divmod", "enumerate",
                            "filter", "float", "format", "int", "isinstance", "len", "list", "map",
                            "max", "min", "ord", "pow", "range", "repr", "reversed", "round", "set",
                            "sorted", "str", "sum", "tuple", "zip", "Exception", "ValueError",
                            "TypeError", "KeyError", "IndexError", "ZeroDivisionError")}


@command("py", "py <файл.py> [аргументы]", aliases=("python",))
def cmd_py(sh, args, stdin):
    """запустить Python-скрипт из MicroOS"""
    if not args:
        raise ShellError("укажите скрипт, например: py hello.py")
    p = sh.path(args[0])
    code = sh.fs.read(p)
    out: list[str] = []
    limit = 5.0

    def _print(*a, sep=" ", end="\n"):
        out.append(sep.join(str(x) for x in a) + end)
        if len(out) > 5000:
            raise _Timeout("слишком много вывода")

    def _read(path):
        return sh.fs.read(sh.path(path))

    def _write(path, data, append=False):
        sh.fs.write(sh.path(path), str(data), sh.user, append)

    g = {"__builtins__": dict(_SAFE_BUILTINS, print=_print), "__name__": "__main__",
         "math": math, "time": time, "args": args[1:], "stdin": stdin or "",
         "read_file": _read, "write_file": _write, "listdir": lambda d=".": sh.fs.listdir(sh.path(d))}
    result = {}

    def target():
        deadline = time.time() + limit

        def tracer(frame, event, arg):
            if time.time() > deadline:
                raise _Timeout(f"превышено время выполнения ({limit:.0f} с)")
            return tracer
        sys.settrace(tracer)
        try:
            exec(compile(code, p, "exec"), g)
        except BaseException as e:
            result["err"] = e
        finally:
            sys.settrace(None)

    th = threading.Thread(target=target, daemon=True)
    th.start()
    th.join(limit + 1)
    text = "".join(out)
    err = result.get("err")
    if err is not None:
        if isinstance(err, SyntaxError):
            text += f"SyntaxError: {err.msg} (строка {err.lineno})"
        else:
            text += f"{type(err).__name__}: {err}"
    return text.rstrip("\n")


@command("sh", "sh <скрипт>", aliases=("source",))
def cmd_sh(sh, args, stdin):
    """выполнить файл с командами msh"""
    lines = sh.fs.read(sh.path(args[0])).splitlines()
    outs = []
    for line in lines:
        o = sh.execute(line)
        if o:
            outs.append(o)
        if sh.power:
            break
    return "\n".join(outs)
