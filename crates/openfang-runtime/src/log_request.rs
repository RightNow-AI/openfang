use chrono::Local;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

struct LogFileCache {
    file: Mutex<Option<std::fs::File>>,
    date_str: Mutex<String>,
    log_dir: PathBuf,
    dir_created: Mutex<bool>,
}

impl LogFileCache {
    fn new() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));

        let log_dir = exe_dir.join("logs");
        if !log_dir.exists() {
            let _ = fs::create_dir_all(&log_dir);
        }

        Self {
            file: Mutex::new(None),
            date_str: Mutex::new(String::new()),
            log_dir,
            dir_created: Mutex::new(false),
        }
    }

    fn get_file(&self) -> std::sync::MutexGuard<'_, Option<std::fs::File>> {
        let now = Local::now();
        let date_str = now.format("%Y-%m-%d").to_string();

        let mut date_guard = self.date_str.lock().unwrap();
        if date_str != *date_guard {
            let mut created = self.dir_created.lock().unwrap();
            if !*created {
                let _ = fs::create_dir_all(&self.log_dir);
                *created = true;
            }
            *date_guard = date_str;
        }

        self.file.lock().unwrap()
    }
}

static LOG_CACHE: OnceLock<LogFileCache> = OnceLock::new();

pub fn log_message(message: &str) {
    let cache = LOG_CACHE.get_or_init(LogFileCache::new);

    let mut file_guard = cache.get_file();
    let now = Local::now();

    if file_guard.is_none() {
        let date_str = now.format("%Y-%m-%d").to_string();
        let log_file = cache.log_dir.join(format!("{}.log", date_str));
        if !log_file.exists() {
            let _ = fs::create_dir_all(&cache.log_dir);
        }
        if let Ok(file) = OpenOptions::new().create(true).append(true).open(&log_file) {
            *file_guard = Some(file);
        }
    }

    if let Some(file) = file_guard.as_mut() {
        let timestamp = now.format("%Y-%m-%d %H:%M:%S").to_string();
        let log_line = format!("[{}] {}\n", timestamp, message.replace("\\n", "\n"));

        let _ = file.write_all(log_line.as_bytes());
    }
}
