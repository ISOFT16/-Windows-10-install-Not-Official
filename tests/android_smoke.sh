#!/usr/bin/env bash
# Проверка запуска MicroOS в эмуляторе Android (вызывается из GitHub Actions).
set -u
PKG=org.microos.microos

adb wait-for-device
APK=$(ls apk/*.apk | head -n 1)
echo "Устанавливаю $APK"
adb install -r "$APK" || exit 1

adb logcat -c
adb logcat -b crash -c 2>/dev/null
adb shell monkey -p "$PKG" -c android.intent.category.LAUNCHER 1

# каждые 3 секунды проверяем, жив ли процесс, и запоминаем момент гибели
ALIVE=0
for i in $(seq 1 15); do
  sleep 3
  PID=$(adb shell pidof "$PKG" | tr -d '\r')
  if [ -n "$PID" ]; then
    ALIVE=$((ALIVE + 1)); echo "t=$((i * 3))s: жив (pid $PID)"
  else
    echo "t=$((i * 3))s: процесса нет"
  fi
done

adb logcat -d -v threadtime > logcat.txt
adb shell screencap -p /sdcard/screen.png && adb pull /sdcard/screen.png screen.png

echo "===== буфер сбоев (crash) ====="
adb logcat -d -b crash -v threadtime | tail -n 120
echo "===== строки приложения и системы о нём ====="
grep -iE "python|microos|AndroidRuntime|fatal|signal|tombstone|backtrace|died|ANR|SDL/APP|libc " logcat.txt \
  | grep -v nativeloader | tail -n 150

PID=$(adb shell pidof "$PKG" | tr -d '\r')
if [ -z "$PID" ]; then
  echo "===== повторный запуск: отчёт faulthandler о прошлом сбое ====="
  adb logcat -c
  adb shell monkey -p "$PKG" -c android.intent.category.LAUNCHER 1 >/dev/null
  sleep 12
  adb logcat -d -v threadtime | grep -A60 "MICROOS PREVIOUS NATIVE CRASH" | head -n 80
fi
if grep -qE "MICROOS CRASH" logcat.txt; then
  echo "ОШИБКА: Python-исключение при старте (см. выше)"
  exit 1
fi
if [ -z "$PID" ]; then
  echo "ОШИБКА: процесс приложения пропал — вылетело"
  exit 1
fi
echo "OK: MicroOS работает (pid $PID)"
