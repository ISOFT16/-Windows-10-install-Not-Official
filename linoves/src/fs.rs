//! Простая файловая система в оперативной памяти (RAM-диск).
//! Лимиты на число и размер файлов не дают одной команде занять всю память.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::sync::IrqMutex;

pub const MAX_FILES: usize = 128;
pub const MAX_FILE_SIZE: usize = 64 * 1024;
pub const MAX_NAME: usize = 32;

#[derive(Debug, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    InvalidName,
    TooManyFiles,
    FileTooLarge,
    OutOfMemory,
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            FsError::NotFound => "file not found",
            FsError::InvalidName => "invalid file name (use 1-32 chars: a-z A-Z 0-9 . _ -)",
            FsError::TooManyFiles => "too many files (limit 128)",
            FsError::FileTooLarge => "file too large (limit 64 KiB)",
            FsError::OutOfMemory => "out of memory",
        };
        f.write_str(s)
    }
}

static FILES: IrqMutex<BTreeMap<String, String>> = IrqMutex::new(BTreeMap::new());

fn check_name(name: &str) -> Result<(), FsError> {
    let ok = !name.is_empty()
        && name.len() <= MAX_NAME
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-');
    if ok {
        Ok(())
    } else {
        Err(FsError::InvalidName)
    }
}

fn make_string(s: &str) -> Result<String, FsError> {
    let mut out = String::new();
    out.try_reserve_exact(s.len()).map_err(|_| FsError::OutOfMemory)?;
    out.push_str(s);
    Ok(out)
}

pub fn write(name: &str, data: &str) -> Result<(), FsError> {
    check_name(name)?;
    if data.len() > MAX_FILE_SIZE {
        return Err(FsError::FileTooLarge);
    }
    let content = make_string(data)?;
    let mut files = FILES.lock();
    if let Some(existing) = files.get_mut(name) {
        *existing = content;
        return Ok(());
    }
    if files.len() >= MAX_FILES {
        return Err(FsError::TooManyFiles);
    }
    let key = make_string(name)?;
    files.insert(key, content);
    Ok(())
}

pub fn append(name: &str, data: &str) -> Result<(), FsError> {
    check_name(name)?;
    let mut files = FILES.lock();
    match files.get_mut(name) {
        Some(existing) => {
            if existing.len() + data.len() > MAX_FILE_SIZE {
                return Err(FsError::FileTooLarge);
            }
            existing.try_reserve(data.len()).map_err(|_| FsError::OutOfMemory)?;
            existing.push_str(data);
            Ok(())
        }
        None => {
            drop(files);
            write(name, data)
        }
    }
}

pub fn read(name: &str) -> Result<String, FsError> {
    check_name(name)?;
    let files = FILES.lock();
    match files.get(name) {
        Some(s) => make_string(s),
        None => Err(FsError::NotFound),
    }
}

pub fn remove(name: &str) -> Result<(), FsError> {
    check_name(name)?;
    match FILES.lock().remove(name) {
        Some(_) => Ok(()),
        None => Err(FsError::NotFound),
    }
}

pub fn list() -> Vec<(String, usize)> {
    FILES.lock().iter().map(|(k, v)| (k.clone(), v.len())).collect()
}

pub fn usage() -> (usize, usize) {
    let files = FILES.lock();
    (files.len(), files.values().map(|v| v.len()).sum())
}

pub fn init() {
    const FILES: [(&str, &str); 9] = [
        (
            "readme.txt",
            "Добро пожаловать в Linoves!\n\n\
             Это RAM-диск: файлы хранятся в памяти до перезагрузки.\n\
             Откройте файл двойным щелчком в программе «Файлы»,\n\
             измените его в Блокноте и сохраните (Ctrl+S).\n",
        ),
        (
            "about.txt",
            "Linoves — 64-битная операционная система, написанная на Rust.\n\
             У каждого исключения процессора есть обработчик, у стека ядра есть\n\
             защитная страница, а куча проверяет каждое освобождение памяти.\n",
        ),
        (
            "keys.txt",
            "Полезные клавиши\n\n\
             Alt+Shift      — переключить раскладку (RU / EN)\n\
             Ctrl+S         — сохранить файл в Блокноте\n\
             Esc            — закрыть меню\n\
             Ctrl+Alt+Del   — перезагрузка\n\
             Стрелки        — управление змейкой\n",
        ),
        (
            "todo.txt",
            "Список дел\n\n\
             [x] Рабочий стол\n\
             [x] Окна и анимации\n\
             [x] Блокнот, калькулятор, рисование\n\
             [ ] Сеть (проводная)\n\
             [ ] Простой браузер\n\
             [ ] Работа с флешкой\n",
        ),
        (
            "notes.txt",
            "Заметки\n\nЗдесь можно писать что угодно. Не забудьте сохранить!\n",
        ),
        (
            "shopping.txt",
            "Покупки\n\n- хлеб\n- молоко\n- яблоки\n- новая мышка\n",
        ),
        (
            "poem.txt",
            "Тихо гудит процессор,\nбьются в памяти байты,\nокна встают над столом —\nсистема проснулась.\n",
        ),
        (
            "hello.rs",
            "fn main() {\n    println!(\"Привет из Linoves!\");\n}\n",
        ),
        (
            "changelog.txt",
            "Linoves 1.4 (C5)\n\n\
             - новые приложения: Терминал, Блокнот, Калькулятор, Рисование,\n\
               Монитор системы, Настройки, Змейка\n\
             - анимации окон и меню\n\
             - русская раскладка клавиатуры\n",
        ),
    ];
    for (name, text) in FILES.iter() {
        let _ = write(name, text);
    }
}
