use chrono::Utc;
use std::fs::OpenOptions;
use std::io::Write;

use crate::state::data_dir;

fn log_path() -> std::path::PathBuf {
    data_dir().join("ghtray.log")
}

pub fn log_error(msg: &str) {
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S");
    let line = format!("[{timestamp}] ERROR: {msg}\n");

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
    {
        let _ = file.write_all(line.as_bytes());
    }

    // Keep log file under 100KB by truncating if needed
    if let Ok(meta) = std::fs::metadata(log_path()) {
        if meta.len() > 100_000 {
            if let Ok(content) = std::fs::read_to_string(log_path()) {
                // Keep last ~50KB
                let keep = &content[content.len().saturating_sub(50_000)..];
                let _ = std::fs::write(log_path(), keep);
            }
        }
    }
}
