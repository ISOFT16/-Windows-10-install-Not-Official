#!/usr/bin/env bash
# Собрать загрузочный ISO-образ с GRUB (для QEMU, VirtualBox, VMware или флешки).
# Нужно: grub-mkrescue, xorriso, mtools
#   Ubuntu/Debian: sudo apt install grub-pc-bin grub-common xorriso mtools
# Компьютер должен загружаться в режиме BIOS/Legacy (CSM).
#
# В образе два ядра:
#   linoves.elf      — просит GRUB включить графику 1024x768 (рабочий стол на реальном ПК);
#   linoves-text.elf — обычное ядро (текстовый режим и самотесты).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release
cargo build --release --features grub-video --target-dir target/grub
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2 | cut -d. -f1,2)
CODENAME=$(grep -m1 'pub const CODENAME' src/main.rs | cut -d'"' -f2)
rm -rf isodir
mkdir -p isodir/boot/grub
cp target/grub/x86_64-unknown-none/release/linoves isodir/boot/linoves.elf
cp target/x86_64-unknown-none/release/linoves isodir/boot/linoves-text.elf
cat > isodir/boot/grub/grub.cfg <<CFG
insmod all_video
set timeout=3
set default=0

menuentry "Linoves ${VERSION} (${CODENAME})" {
    multiboot /boot/linoves.elf
    boot
}

menuentry "Linoves ${VERSION} (${CODENAME}) - text mode" {
    multiboot /boot/linoves-text.elf text
    boot
}

menuentry "Linoves ${VERSION} (${CODENAME}) - self-test" {
    multiboot /boot/linoves-text.elf selftest
    boot
}
CFG
mkdir -p dist
grub-mkrescue -o dist/Linoves.iso isodir 2>/dev/null
rm -rf isodir
echo "Готово: dist/Linoves.iso (Linoves ${VERSION}, ${CODENAME})"
echo "Проверка: qemu-system-x86_64 -m 128M -cdrom dist/Linoves.iso"
