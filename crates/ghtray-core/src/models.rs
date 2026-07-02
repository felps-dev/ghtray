use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── GraphQL response types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct GqlResponse {
    pub data: GqlData,
}

#[derive(Debug, Deserialize)]
pub struct GqlData {
    #[serde(rename = "needsReview")]
    pub needs_review: SearchResult,
    pub authored: SearchResult,
    #[serde(rename = "reviewedByMe")]
    pub reviewed_by_me: SearchResult,
    #[serde(rename = "recentlyMerged")]
    pub recently_merged: SearchResult,
}

#[derive(Debug, Deserialize)]
pub struct SearchResult {
    #[serde(rename = "issueCount")]
    pub _issue_count: u32,
    pub nodes: Vec<PullRequest>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PullRequest {
    pub id: String,
    pub number: u32,
    pub title: String,
    pub url: String,
    #[serde(rename = "isDraft")]
    pub is_draft: Option<bool>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(rename = "mergedAt")]
    pub merged_at: Option<DateTime<Utc>>,
    pub repository: Repository,
    pub author: Option<Actor>,
    #[serde(rename = "reviewDecision")]
    pub review_decision: Option<String>,
    #[serde(rename = "latestReviews")]
    pub latest_reviews: Option<ReviewConnection>,
    pub commits: Option<CommitConnection>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Repository {
    #[serde(rename = "nameWithOwner")]
    pub name_with_owner: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Actor {
    pub login: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ReviewConnection {
    pub nodes: Vec<Review>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Review {
    pub author: Option<Actor>,
    pub state: String,
    #[serde(rename = "submittedAt")]
    pub submitted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CommitConnection {
    pub nodes: Vec<CommitNode>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CommitNode {
    pub commit: CommitInfo,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CommitInfo {
    pub oid: String,
    #[serde(rename = "committedDate")]
    pub committed_date: DateTime<Utc>,
    #[serde(rename = "statusCheckRollup")]
    pub status_check_rollup: Option<StatusCheck>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StatusCheck {
    pub state: String,
}

// ── Bucket model ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Bucket {
    NeedsYourReview,
    WaitingForReviewers,
    ReturnedToYou,
    Approved,
    Drafts,
    RecentlyMerged,
    WaitingForAuthor,
}

impl Bucket {
    pub fn label(&self) -> &'static str {
        match self {
            Bucket::NeedsYourReview => "Needs Your Review",
            Bucket::WaitingForReviewers => "Waiting for Reviewers",
            Bucket::ReturnedToYou => "Returned to You",
            Bucket::Approved => "Approved",
            Bucket::Drafts => "Drafts",
            Bucket::RecentlyMerged => "Recently Merged",
            Bucket::WaitingForAuthor => "Waiting for Author",
        }
    }

    /// Notification title for entering this bucket, if any.
    /// Used by the ledger-based notification check in `github::pending_notifications`.
    pub fn notification_title(&self) -> Option<&'static str> {
        match self {
            Bucket::NeedsYourReview => Some("Review Requested"),
            Bucket::ReturnedToYou => Some("Changes Requested"),
            Bucket::Approved => Some("PR Approved"),
            Bucket::RecentlyMerged => Some("PR Merged"),
            _ => None,
        }
    }

    /// Buckets where both open and draft PRs can actually land, making the
    /// per-status filter meaningful. Authored drafts always route to the
    /// Drafts bucket (see `categorize_authored`), so Approved / Returned to
    /// You / Waiting for Reviewers are open-only, Drafts is draft-only, and
    /// Recently Merged is merged-only. Only searches over *other people's*
    /// PRs can mix: a review request on a draft (Needs Your Review) or a
    /// reviewed PR later converted to draft (Waiting for Author).
    pub fn status_filterable(&self) -> bool {
        matches!(self, Bucket::NeedsYourReview | Bucket::WaitingForAuthor)
    }

    pub fn id(&self) -> &'static str {
        match self {
            Bucket::NeedsYourReview => "needs_your_review",
            Bucket::WaitingForReviewers => "waiting_for_reviewers",
            Bucket::ReturnedToYou => "returned_to_you",
            Bucket::Approved => "approved",
            Bucket::Drafts => "drafts",
            Bucket::RecentlyMerged => "recently_merged",
            Bucket::WaitingForAuthor => "waiting_for_author",
        }
    }

    pub fn display_order() -> &'static [Bucket] {
        &[
            Bucket::NeedsYourReview,
            Bucket::WaitingForReviewers,
            Bucket::ReturnedToYou,
            Bucket::Approved,
            Bucket::Drafts,
            Bucket::RecentlyMerged,
            Bucket::WaitingForAuthor,
        ]
    }

    pub fn from_id(id: &str) -> Option<Bucket> {
        match id {
            "needs_your_review" => Some(Bucket::NeedsYourReview),
            "waiting_for_reviewers" => Some(Bucket::WaitingForReviewers),
            "returned_to_you" => Some(Bucket::ReturnedToYou),
            "approved" => Some(Bucket::Approved),
            "drafts" => Some(Bucket::Drafts),
            "recently_merged" => Some(Bucket::RecentlyMerged),
            "waiting_for_author" => Some(Bucket::WaitingForAuthor),
            _ => None,
        }
    }
}

// ── Categorized PR (for display and caching) ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorizedPr {
    pub id: String,
    pub number: u32,
    pub title: String,
    pub url: String,
    pub repo: String,
    pub author: String,
    pub bucket: Bucket,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub last_commit_sha: Option<String>,
    pub last_commit_date: Option<DateTime<Utc>>,
    pub ci_status: Option<String>,
    /// Whether the PR is a draft. Drives the per-bucket status filter and
    /// the "✎" marker on tray rows outside the Drafts bucket.
    #[serde(default)]
    pub is_draft: bool,
    /// When the most recent desktop notification fired for this PR.
    /// Populated by enriching against the persistent notification ledger
    /// after each fetch. Drives the "🔔 5m" suffix in the tray menu.
    #[serde(default)]
    pub last_notified_at: Option<DateTime<Utc>>,
}

impl CategorizedPr {
    /// Status ID used by the per-bucket status filter
    /// (`AppConfig::bucket_hidden_statuses`).
    pub fn status_id(&self) -> &'static str {
        if self.is_draft { "draft" } else { "open" }
    }
}

/// Format a datetime as a compact relative time string (e.g., "2m", "4h", "3d", "2mo", "1y")
pub fn relative_time(dt: DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now.signed_duration_since(dt);

    let mins = diff.num_minutes();
    if mins < 1 {
        return "now".to_string();
    }
    if mins < 60 {
        return format!("{mins}m");
    }
    let hours = diff.num_hours();
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = diff.num_days();
    if days < 30 {
        return format!("{days}d");
    }
    if days < 365 {
        return format!("{}mo", days / 30);
    }
    format!("{}y", days / 365)
}

// ── Notifications ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PendingNotification {
    pub pr_id: String,
    pub bucket: Bucket,
    /// Full "owner/name" — used by `send_notifications` to guard against
    /// firing for blocked repos even if upstream filtering missed them.
    pub repo: String,
    pub title: &'static str,
    pub body: String,
}

pub fn short_repo(repo: &str) -> &str {
    repo.split('/').next_back().unwrap_or(repo)
}
