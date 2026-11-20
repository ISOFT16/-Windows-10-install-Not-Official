#!/usr/bin/env bash
# Запуск RustOS в QEMU
cd "$(dirname "$0")"
exec qemu-system-x86_64 -m 128M -cdrom RustOS.iso
