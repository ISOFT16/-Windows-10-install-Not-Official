#!/usr/bin/env bash
# Собрать загрузочный ISO-образ с GRUB (для VirtualBox, VMware или флешки).
# Нужно: grub-mkrescue, xorriso, mtools
#   Ubuntu/Debian: sudo apt install grub-pc-bin grub-common xorriso mtools
# Компьютер должен загружаться в режиме BIOS/Legacy (CSM): ядро выводит текст
# через VGA, которого нет в чистом UEFI.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2 | cut -d. -f1,2)
CODENAME=$(grep -m1 'pub const CODENAME' src/main.rs | cut -d'"' -f2)
rm -rf isodir
mkdir -p isodir/boot/grub
cp target/x86_64-unknown-none/release/rustos isodir/boot/rustos.elf
cat > isodir/boot/grub/grub.cfg <<CFG
set timeout=3
set default=0

menuentry "RustOS ${VERSION} (${CODENAME})" {
    multiboot /boot/rustos.elf
    boot
}

menuentry "RustOS ${VERSION} (${CODENAME}) - self-test" {
    multiboot /boot/rustos.elf selftest
    boot
}
CFG
mkdir -p dist
grub-mkrescue -o dist/RustOS.iso isodir 2>/dev/null
rm -rf isodir
echo "Готово: dist/RustOS.iso (RustOS ${VERSION}, ${CODENAME})"
echo "Проверка: qemu-system-x86_64 -m 128M -cdrom dist/RustOS.iso"
