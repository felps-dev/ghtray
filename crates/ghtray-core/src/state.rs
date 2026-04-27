use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::models::{Bucket, CategorizedPr};

/// Key identifying a notification we've already emitted for a PR.
/// We re-notify only when the bucket changes or a new commit appears.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotificationKey {
    pub bucket: Bucket,
    pub commit_sha: Option<String>,
    /// Wall-clock time the entry was last written — used for LRU eviction.
    #[serde(default)]
    pub recorded_at: Option<DateTime<Utc>>,
}

/// Soft cap on the notified ledger; older entries are evicted past this size.
pub const NOTIFIED_LEDGER_CAP: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppState {
    pub last_fetch: Option<DateTime<Utc>>,
    /// Unfiltered, categorized PRs from the last successful fetch.
    /// Used to restore the tray at startup before the first live fetch.
    #[serde(default)]
    pub all_prs: Vec<CategorizedPr>,
    /// Per-PR record of the last notification we emitted.
    /// Updated only when a notification fires, so flickering fetches
    /// (partial GraphQL data, transient null reviewDecision) don't re-trigger.
    #[serde(default)]
    pub notified: HashMap<String, NotificationKey>,
    /// True once the ledger has been seeded from a real fetch. False on
    /// fresh installs and on upgrade from older state files (where this
    /// field is missing) — first fetch seeds silently to avoid a flood.
    #[serde(default)]
    pub notified_seeded: bool,
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

/// Trim the notified ledger to NOTIFIED_LEDGER_CAP entries by dropping the
/// oldest by `recorded_at`. Entries without a timestamp are evicted first.
pub fn trim_notified(notified: &mut HashMap<String, NotificationKey>) {
    if notified.len() <= NOTIFIED_LEDGER_CAP {
        return;
    }
    let mut entries: Vec<(String, Option<DateTime<Utc>>)> = notified
        .iter()
        .map(|(id, key)| (id.clone(), key.recorded_at))
        .collect();
    // Oldest (None first, then ascending time) at the front.
    entries.sort_by(|a, b| match (a.1, b.1) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(x), Some(y)) => x.cmp(&y),
    });
    let to_drop = notified.len() - NOTIFIED_LEDGER_CAP;
    for (id, _) in entries.into_iter().take(to_drop) {
        notified.remove(&id);
    }
}
