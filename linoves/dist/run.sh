#!/usr/bin/env bash
# Запуск Linoves в QEMU: берётся самый новый образ Linoves-*.iso рядом с этим файлом.
cd "$(dirname "$0")"
ISO=$(ls -1 Linoves*.iso 2>/dev/null | sort -V | tail -n 1)
if [ -z "$ISO" ]; then echo "Linoves ISO not found"; exit 1; fi
exec qemu-system-x86_64 -m 128M -cdrom "$ISO"
