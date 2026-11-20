# Windows (PowerShell): собрать ядро и прогнать самотесты в QEMU без окна.
#   powershell -ExecutionPolicy Bypass -File scripts\selftest.ps1
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
cargo build --release
if ($LASTEXITCODE -ne 0) { exit 1 }
& qemu-system-x86_64 -m 128M -display none -serial stdio -no-reboot `
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 `
    -append selftest -kernel target/x86_64-unknown-none/release/linoves
if ($LASTEXITCODE -eq 33) { Write-Host "SELFTEST PASSED"; exit 0 }
Write-Host "SELFTEST FAILED (QEMU exit code $LASTEXITCODE)"
exit 1
