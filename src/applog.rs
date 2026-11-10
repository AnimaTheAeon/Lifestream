//! Small in-memory log shown on the Log page, mirrored to a file.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::Mutex;

use once_cell::sync::Lazy;

const MAX_LINES: usize = 600;

pub struct LogLine {
    pub time: String,
    pub level: Level,
    pub text: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

static LINES: Lazy<Mutex<VecDeque<LogLine>>> = Lazy::new(|| Mutex::new(VecDeque::new()));
static FILE: Lazy<Mutex<Option<std::fs::File>>> = Lazy::new(|| {
    let path = crate::settings::data_dir().join("lifestream.log");
    // Keep the file from growing forever.
    if std::fs::metadata(&path).map(|m| m.len() > 4_000_000).unwrap_or(false) {
        let _ = std::fs::rename(&path, crate::settings::data_dir().join("lifestream.old.log"));
    }
    Mutex::new(
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok(),
    )
});

pub fn push(level: Level, text: impl Into<String>) {
    let text = text.into();
    let time = chrono::Local::now().format("%H:%M:%S").to_string();
    if let Ok(mut f) = FILE.lock() {
        if let Some(f) = f.as_mut() {
            let tag = match level {
                Level::Info => "INFO",
                Level::Warn => "WARN",
                Level::Error => "ERROR",
            };
            let _ = writeln!(
                f,
                "{} {tag} {text}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
            );
        }
    }
    if let Ok(mut lines) = LINES.lock() {
        lines.push_back(LogLine { time, level, text });
        while lines.len() > MAX_LINES {
            lines.pop_front();
        }
    }
}

pub fn info(text: impl Into<String>) {
    push(Level::Info, text)
}
pub fn warn(text: impl Into<String>) {
    push(Level::Warn, text)
}
pub fn error(text: impl Into<String>) {
    push(Level::Error, text)
}

pub fn with_lines<R>(f: impl FnOnce(&VecDeque<LogLine>) -> R) -> R {
    let lines = LINES.lock().unwrap();
    f(&lines)
}

pub fn clear() {
    if let Ok(mut lines) = LINES.lock() {
        lines.clear();
    }
}
