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
    let _ = write(
        "readme.txt",
        "Welcome to Linoves!\n\
         This is a RAM disk: files live in memory until the computer is restarted.\n\
         Try: ls, cat readme.txt, write note.txt hello, append note.txt world, rm note.txt\n",
    );
    let _ = write(
        "about.txt",
        "Linoves is a small 64-bit kernel written in Rust.\n\
         Every CPU exception has a handler, the kernel stack has a guard page,\n\
         double faults run on their own stack and the heap checks every free().\n",
    );
}
