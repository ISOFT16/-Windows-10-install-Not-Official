#!/usr/bin/env bash
# Запуск Linoves в QEMU
cd "$(dirname "$0")"
exec qemu-system-x86_64 -m 128M -cdrom Linoves.iso
