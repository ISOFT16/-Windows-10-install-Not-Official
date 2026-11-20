@echo off
rem Запуск Linoves в QEMU (двойной щелчок по этому файлу).
cd /d "%~dp0"
where qemu-system-x86_64 >nul 2>nul
if %errorlevel%==0 (
    qemu-system-x86_64 -m 128M -cdrom Linoves.iso
) else if exist "C:\Program Files\qemu\qemu-system-x86_64.exe" (
    "C:\Program Files\qemu\qemu-system-x86_64.exe" -m 128M -cdrom Linoves.iso
) else (
    echo QEMU not found. Install QEMU or add its folder to PATH.
    pause
)
