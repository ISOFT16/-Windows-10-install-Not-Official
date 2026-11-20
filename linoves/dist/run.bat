@echo off
rem Запуск Linoves в QEMU (двойной щелчок по этому файлу).
rem Берётся образ Linoves-*.iso, который лежит рядом с этим файлом.
cd /d "%~dp0"
set "ISO="
for %%f in (Linoves*.iso) do set "ISO=%%f"
if not defined ISO (
    echo Linoves ISO not found next to this file.
    pause
    exit /b 1
)
where qemu-system-x86_64 >nul 2>nul
if %errorlevel%==0 (
    qemu-system-x86_64 -m 128M -cdrom "%ISO%"
) else if exist "C:\Program Files\qemu\qemu-system-x86_64.exe" (
    "C:\Program Files\qemu\qemu-system-x86_64.exe" -m 128M -cdrom "%ISO%"
) else if exist "D:\qemu\qemu-system-x86_64.exe" (
    "D:\qemu\qemu-system-x86_64.exe" -m 128M -cdrom "%ISO%"
) else (
    echo QEMU not found. Install QEMU or add its folder to PATH.
    pause
)
