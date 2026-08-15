use chrono::Utc;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;

const LOG_ROTATE_BYTES: u64 = 2 * 1024 * 1024;

pub struct Logger {
    file_writer: Option<BufWriter<File>>,
    service_mode: bool,
    log_path: Option<String>,
}

impl Logger {
    pub fn new(log_path: &str, service_mode: bool) -> Result<Self, String> {
        if let Some(parent) = Path::new(log_path).parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create log directory: {}", e))?;
            }
        }

        rotate_if_needed(log_path)?;
        let file = open_append(log_path)?;

        Ok(Logger {
            file_writer: Some(BufWriter::new(file)),
            service_mode,
            log_path: Some(log_path.to_string()),
        })
    }

    pub fn silent() -> Self {
        Logger {
            file_writer: None,
            service_mode: true,
            log_path: None,
        }
    }

    fn log_message(&mut self, level: &str, message: &str) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let log_line = format!("[{}] {}: {}", timestamp, level, message);

        if !self.service_mode {
            println!("{}", log_line);
        }

        if let Some(path) = self.log_path.clone() {
            if should_rotate(&path) {
                if let Some(mut writer) = self.file_writer.take() {
                    let _ = writer.flush();
                }
                if rotate_if_needed(&path).is_ok() {
                    self.file_writer = open_append(&path).ok().map(BufWriter::new);
                }
            }
        }

        if let Some(ref mut writer) = self.file_writer {
            if let Err(e) = writeln!(writer, "{}", log_line) {
                eprintln!("Failed to write to log file: {}", e);
            } else if let Err(e) = writer.flush() {
                eprintln!("Failed to flush log file: {}", e);
            }
        }
    }

    pub fn info(&mut self, message: &str) {
        self.log_message("INFO", message);
    }

    pub fn warn(&mut self, message: &str) {
        self.log_message("WARN", message);
    }

    pub fn error(&mut self, message: &str) {
        self.log_message("ERROR", message);
    }

    #[allow(dead_code)]
    pub fn debug(&mut self, message: &str) {
        self.log_message("DEBUG", message);
    }
}

impl Drop for Logger {
    fn drop(&mut self) {
        if let Some(ref mut writer) = self.file_writer {
            let _ = writer.flush();
        }
    }
}

fn open_append(log_path: &str) -> Result<File, String> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|e| format!("Failed to open log file {}: {}", log_path, e))
}

fn should_rotate(log_path: &str) -> bool {
    fs::metadata(log_path)
        .map(|meta| meta.len() >= LOG_ROTATE_BYTES)
        .unwrap_or(false)
}

fn rotate_if_needed(log_path: &str) -> Result<(), String> {
    if !should_rotate(log_path) {
        return Ok(());
    }
    let rotated = format!("{log_path}.1");
    let _ = fs::remove_file(&rotated);
    fs::rename(log_path, &rotated)
        .map_err(|e| format!("Failed to rotate log file {}: {}", log_path, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_threshold_is_two_megabytes() {
        assert_eq!(LOG_ROTATE_BYTES, 2 * 1024 * 1024);
    }
}
