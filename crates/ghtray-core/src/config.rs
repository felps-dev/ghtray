use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;

use crate::models::Bucket;
use crate::state::data_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Poll interval in seconds (minimum 30)
    pub poll_interval_secs: u64,
    /// Merged PR window in days
    pub merged_window_days: i64,
    /// Blocked repos (full "owner/name") — empty means show all
    pub blocked_repos: HashSet<String>,
    /// Whether notifications are enabled
    #[serde(default = "default_true")]
    pub notifications_enabled: bool,
    /// Whether to play sound with notifications
    #[serde(default = "default_true")]
    pub notification_sound: bool,
    /// Bucket IDs to hide from the tray menu (empty = show all)
    #[serde(default)]
    pub hidden_buckets: HashSet<String>,
    /// Bucket IDs that count towards the badge number
    #[serde(default = "default_badge_buckets")]
    pub badge_buckets: HashSet<String>,
    /// Custom display order for buckets (list of bucket IDs)
    #[serde(default)]
    pub bucket_order: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_badge_buckets() -> HashSet<String> {
    HashSet::from([
        "needs_your_review".to_string(),
        "returned_to_you".to_string(),
    ])
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 120,
            merged_window_days: 7,
            blocked_repos: HashSet::new(),
            notifications_enabled: true,
            notification_sound: true,
            hidden_buckets: HashSet::new(),
            badge_buckets: default_badge_buckets(),
            bucket_order: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn config_path() -> std::path::PathBuf {
        data_dir().join("ghtray-config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, json)?;
        Ok(())
    }

    pub fn is_repo_allowed(&self, repo: &str) -> bool {
        !self.blocked_repos.contains(repo)
    }

    pub fn poll_interval_secs_clamped(&self) -> u64 {
        self.poll_interval_secs.max(30)
    }

    pub fn is_bucket_visible(&self, bucket_id: &str) -> bool {
        !self.hidden_buckets.contains(bucket_id)
    }

    pub fn counts_for_badge(&self, bucket_id: &str) -> bool {
        self.badge_buckets.contains(bucket_id)
    }

    /// Returns the bucket display order. Uses custom order if set, otherwise default.
    pub fn ordered_buckets(&self) -> Vec<Bucket> {
        if self.bucket_order.is_empty() {
            return Bucket::display_order().to_vec();
        }

        let mut ordered: Vec<Bucket> = self
            .bucket_order
            .iter()
            .filter_map(|id| Bucket::from_id(id))
            .collect();

        // Append any missing buckets (e.g. newly added ones)
        for b in Bucket::display_order() {
            if !ordered.contains(b) {
                ordered.push(*b);
            }
        }

        ordered
    }
}
