#!/usr/bin/env bash
# Проверка запуска MicroOS в эмуляторе Android (вызывается из GitHub Actions).
set -u
PKG=org.microos.microos

adb wait-for-device
APK=$(ls apk/*.apk | head -n 1)
echo "Устанавливаю $APK"
adb install -r "$APK" || exit 1

adb logcat -c
adb shell monkey -p "$PKG" -c android.intent.category.LAUNCHER 1
sleep 45

adb logcat -d > logcat.txt
adb shell screencap -p /sdcard/screen.png && adb pull /sdcard/screen.png screen.png
PID=$(adb shell pidof "$PKG" | tr -d '\r')

echo "----- важные строки лога -----"
grep -E "python|MICROOS|AndroidRuntime|FATAL|dlopen|SDL" logcat.txt | tail -n 80

if grep -qE "MICROOS CRASH|FATAL EXCEPTION|Python for android ended" logcat.txt; then
  echo "ОШИБКА: приложение упало (см. logcat.txt)"
  exit 1
fi
if [ -z "$PID" ]; then
  echo "ОШИБКА: процесс приложения не найден — вылетело"
  exit 1
fi
echo "OK: MicroOS запущен (pid $PID)"
