use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::models::CategorizedPr;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppState {
    pub last_fetch: Option<DateTime<Utc>>,
    pub prs: HashMap<String, CategorizedPr>,
}

pub fn data_dir() -> PathBuf {
    let base = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    #[cfg(target_os = "macos")]
    let dir = base.join("Library/Application Support/ghtray");
    #[cfg(not(target_os = "macos"))]
    let dir = base.join(".local/share/ghtray");

    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn state_file_path() -> PathBuf {
    data_dir().join("ghtray-state.json")
}

pub fn load_state() -> AppState {
    let path = state_file_path();
    if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        AppState::default()
    }
}

pub fn save_state(state: &AppState) -> Result<()> {
    let path = state_file_path();
    let json = serde_json::to_string_pretty(state)?;
    fs::write(&path, json)?;
    Ok(())
}
