use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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
    /// Blocked organizations (the "owner" part of "owner/name"). Every repo
    /// under a blocked org is hidden, including ones not yet seen — so muting
    /// an org also mutes future repos under it. Empty means none blocked.
    #[serde(default)]
    pub blocked_orgs: HashSet<String>,
    /// Whether notifications are enabled (master switch)
    #[serde(default = "default_true")]
    pub notifications_enabled: bool,
    /// Whether to play sound with notifications (master switch)
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
    /// Max age in days for PRs to display (0 = no limit)
    #[serde(default)]
    pub max_pr_age_days: u64,
    /// Custom path to gh CLI binary (empty = auto-detect)
    #[serde(default)]
    pub gh_cli_path: String,
    /// Path or system sound name for notifications. Empty = default Glass.aiff.
    /// A bare name (e.g. "Pop") resolves to /System/Library/Sounds/{name}.aiff.
    #[serde(default)]
    pub notification_sound_path: String,
    /// Bucket IDs that fire desktop notifications. Subset of notifiable buckets.
    #[serde(default = "default_notify_buckets")]
    pub notify_buckets: HashSet<String>,
    /// Bucket IDs that play sound when their notification fires.
    #[serde(default = "default_notify_buckets")]
    pub sound_buckets: HashSet<String>,
    /// Per-bucket sort key. Missing keys default to "updated_desc".
    /// Values: updated_desc/asc, notified_desc/asc, created_desc/asc, number_desc/asc.
    #[serde(default)]
    pub bucket_sort: HashMap<String, String>,
    /// Per-bucket hidden PR statuses ("open" / "draft"). A PR whose status is
    /// in its bucket's set is hidden from the tray, the badge, and
    /// notifications. Missing bucket = show all statuses (block-list, so new
    /// statuses surface by default).
    #[serde(default)]
    pub bucket_hidden_statuses: HashMap<String, HashSet<String>>,
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

/// Default set of buckets that fire notifications — matches the set of
/// buckets where `Bucket::notification_title()` returns `Some`.
fn default_notify_buckets() -> HashSet<String> {
    HashSet::from([
        "needs_your_review".to_string(),
        "returned_to_you".to_string(),
        "approved".to_string(),
        "recently_merged".to_string(),
    ])
}

/// Default sort key used when a bucket has no explicit override.
pub const DEFAULT_SORT_KEY: &str = "updated_desc";

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 120,
            merged_window_days: 7,
            blocked_repos: HashSet::new(),
            blocked_orgs: HashSet::new(),
            notifications_enabled: true,
            notification_sound: true,
            hidden_buckets: HashSet::new(),
            badge_buckets: default_badge_buckets(),
            bucket_order: Vec::new(),
            max_pr_age_days: 0,
            gh_cli_path: String::new(),
            notification_sound_path: String::new(),
            notify_buckets: default_notify_buckets(),
            sound_buckets: default_notify_buckets(),
            bucket_sort: HashMap::new(),
            bucket_hidden_statuses: HashMap::new(),
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
        // An org-level block hides every repo under that owner, including
        // repos that weren't visible when the org was muted (the block-list
        // is a snapshot; the org-list is not).
        let owner = repo.split('/').next().unwrap_or(repo);
        !self.blocked_orgs.contains(owner) && !self.blocked_repos.contains(repo)
    }

    pub fn is_org_blocked(&self, owner: &str) -> bool {
        self.blocked_orgs.contains(owner)
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

    /// Should a notification fire when a PR enters this bucket?
    /// Master `notifications_enabled` toggle wins.
    pub fn notify_for_bucket(&self, bucket_id: &str) -> bool {
        self.notifications_enabled && self.notify_buckets.contains(bucket_id)
    }

    /// Should a sound play when a notification fires for this bucket?
    /// Master `notification_sound` toggle wins.
    pub fn sound_for_bucket(&self, bucket_id: &str) -> bool {
        self.notification_sound && self.sound_buckets.contains(bucket_id)
    }

    /// Is a PR status ("open" / "draft") visible within a bucket?
    /// Missing bucket entry means all statuses show.
    pub fn is_status_visible(&self, bucket_id: &str, status: &str) -> bool {
        self.bucket_hidden_statuses
            .get(bucket_id)
            .is_none_or(|hidden| !hidden.contains(status))
    }

    /// Resolve the sort key for a bucket, falling back to the default.
    pub fn sort_for_bucket(&self, bucket_id: &str) -> &str {
        self.bucket_sort
            .get(bucket_id)
            .map(String::as_str)
            .unwrap_or(DEFAULT_SORT_KEY)
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
