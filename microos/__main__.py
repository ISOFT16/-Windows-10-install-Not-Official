"""Консольный запуск MicroOS на компьютере:  python -m microos"""

import getpass
import os
import sys
import time

from .boot import boot, boot_messages


def default_disk():
    return os.environ.get("MICROOS_DISK") or os.path.join(os.path.expanduser("~"), ".microos", "disk.json")


def setup_readline(shell):
    try:
        import readline
    except ImportError:
        return

    def completer(text, state):
        line = readline.get_line_buffer()
        done = shell.complete(line)
        opts = [done[len(line) - len(text):]] if done != line else []
        if not opts and " " not in line:
            opts = shell.completions(line)
        return opts[state] if state < len(opts) else None

    readline.set_completer(completer)
    readline.set_completer_delims(" /")
    readline.parse_and_bind("tab: complete")


def main():
    disk = sys.argv[1] if len(sys.argv) > 1 else default_disk()
    while True:
        for msg in boot_messages():
            print(msg)
            time.sleep(0.08)
        kernel, shell = boot(disk)
        setup_readline(shell)
        print()
        print(shell.banner())
        while not shell.power:
            try:
                if shell.secret:
                    line = getpass.getpass(shell.prompt)
                else:
                    line = input(shell.prompt)
            except EOFError:
                shell.power = "shutdown"
                break
            except KeyboardInterrupt:
                print("^C")
                shell._done_asking()  # отменить ввод пароля / выйти из редактора
                continue
            out = shell.execute(line)
            if shell.clear_requested:
                shell.clear_requested = False
                print("\033[2J\033[H", end="")
            if out:
                print(out)
        kernel.shutdown()
        if shell.power != "reboot":
            print("Питание можно отключить. До свидания!")
            return
        print()


if __name__ == "__main__":
    main()
