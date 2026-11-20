#!/usr/bin/env bash
# Собрать ядро и прогнать встроенные самотесты в QEMU без окна.
# Код выхода 0 = все тесты прошли.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release
set +e
timeout 120 qemu-system-x86_64 -m 128M -display none -serial stdio -no-reboot \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    -append selftest -kernel target/x86_64-unknown-none/release/linoves </dev/null
code=$?
set -e
# isa-debug-exit: 0x10 -> 33 (успех), 0x11 -> 35 (ошибка)
if [ "$code" -eq 33 ]; then
    echo "SELFTEST PASSED"
    exit 0
fi
echo "SELFTEST FAILED (QEMU exit code $code)"
exit 1
