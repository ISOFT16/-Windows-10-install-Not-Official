"""Консольная MicroOS для Windows (без графики): MicroOS-Console.exe"""

import os
import sys

# при запуске из исходников (python windows/console_main.py) добавляем корень проекта
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from microos.__main__ import main  # noqa: E402

if __name__ == "__main__":
    main()
