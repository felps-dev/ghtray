use anyhow::{Context, Result, bail};
use chrono::{Duration, Utc};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::sync::OnceLock;

use crate::config::AppConfig;
use crate::models::*;

// ── gh CLI path resolution ─────────────────────────────────────────────────

static GH_PATH: OnceLock<String> = OnceLock::new();

/// Resolve the full path to `gh` CLI. Searches common Homebrew/system paths
/// since bundled macOS apps don't inherit the user's shell PATH.
fn gh_bin() -> &'static str {
    GH_PATH.get_or_init(|| {
        let candidates = [
            "/opt/homebrew/bin/gh",          // Apple Silicon Homebrew
            "/usr/local/bin/gh",             // Intel Homebrew
            "/usr/bin/gh",                   // System
            "/run/current-system/sw/bin/gh", // NixOS
        ];

        // Try common known paths first
        for path in &candidates {
            if std::path::Path::new(path).exists() {
                return path.to_string();
            }
        }

        // Fallback: ask the user's login shell where gh lives
        if let Ok(output) = Command::new("/bin/sh")
            .args(["-l", "-c", "which gh"])
            .output()
        {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() && std::path::Path::new(&path).exists() {
                return path;
            }
        }

        // Last resort: hope it's in PATH
        "gh".to_string()
    })
}

// ── GraphQL query builder ───────────────────────────────────────────────────

fn build_query(merged_since: &str) -> String {
    let pr_fields = r#"
        id number title url isDraft createdAt updatedAt
        repository { nameWithOwner }
        author { login }
        reviewDecision
        latestReviews(first: 10) {
          nodes { author { login } state submittedAt }
        }
        reviewRequests(first: 10) {
          nodes { requestedReviewer { ... on User { login } ... on Team { name } } }
        }
        commits(last: 1) {
          nodes { commit { oid committedDate statusCheckRollup { state } } }
        }"#;

    format!(
        r#"{{
  needsReview: search(query: "is:pr is:open review-requested:@me", type: ISSUE, first: 50) {{
    issueCount
    nodes {{ ... on PullRequest {{ {pr_fields} }} }}
  }}
  authored: search(query: "is:pr is:open author:@me", type: ISSUE, first: 50) {{
    issueCount
    nodes {{ ... on PullRequest {{ {pr_fields} }} }}
  }}
  reviewedByMe: search(query: "is:pr is:open reviewed-by:@me -author:@me -review-requested:@me", type: ISSUE, first: 50) {{
    issueCount
    nodes {{ ... on PullRequest {{ {pr_fields} }} }}
  }}
  recentlyMerged: search(query: "is:pr is:merged author:@me merged:>{merged_since}", type: ISSUE, first: 20) {{
    issueCount
    nodes {{ ... on PullRequest {{
      id number title url createdAt mergedAt
      repository {{ nameWithOwner }}
      author {{ login }}
    }} }}
  }}
}}"#
    )
}

// ── gh CLI status check ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum GhStatus {
    Ok,
    NotInstalled,
    NotAuthenticated(String),
}

/// Quick check: is `gh` installed and authenticated?
pub fn check_gh_status() -> GhStatus {
    let bin = gh_bin();
    if bin == "gh"
        && Command::new("which")
            .arg("gh")
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
    {
        // If gh_bin() fell back to "gh" and `which` can't find it
        if Command::new(bin).arg("--version").output().is_err() {
            return GhStatus::NotInstalled;
        }
    }

    match Command::new(bin).args(["auth", "status"]).output() {
        Err(_) => GhStatus::NotInstalled,
        Ok(output) => {
            if output.status.success() {
                GhStatus::Ok
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                GhStatus::NotAuthenticated(stderr)
            }
        }
    }
}

// ── gh CLI interaction ──────────────────────────────────────────────────────

pub fn get_viewer_login() -> Result<String> {
    let output = Command::new(gh_bin())
        .args(["api", "graphql", "-f", "query={ viewer { login } }"])
        .output()
        .context("Failed to execute `gh` CLI. Is it installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh auth check failed: {}", stderr.trim());
    }

    #[derive(Deserialize)]
    struct Resp {
        data: Data,
    }
    #[derive(Deserialize)]
    struct Data {
        viewer: Viewer,
    }
    #[derive(Deserialize)]
    struct Viewer {
        login: String,
    }

    let resp: Resp = serde_json::from_slice(&output.stdout)?;
    Ok(resp.data.viewer.login)
}

pub fn fetch_prs(merged_days: i64) -> Result<GqlResponse> {
    let merged_since = (Utc::now() - Duration::days(merged_days))
        .format("%Y-%m-%d")
        .to_string();

    let query = build_query(&merged_since);

    let output = Command::new(gh_bin())
        .args(["api", "graphql", "-f", &format!("query={query}")])
        .output()
        .context("Failed to execute `gh` CLI. Is it installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not logged in") || stderr.contains("authentication") {
            bail!("gh CLI is not authenticated. Run `gh auth login` first.");
        }
        bail!("gh api graphql failed: {}", stderr.trim());
    }

    let response: GqlResponse =
        serde_json::from_slice(&output.stdout).context("Failed to parse GraphQL response")?;

    Ok(response)
}

// ── Categorization engine ───────────────────────────────────────────────────

fn extract_commit_info(
    pr: &PullRequest,
) -> (
    Option<String>,
    Option<chrono::DateTime<Utc>>,
    Option<String>,
) {
    pr.commits
        .as_ref()
        .and_then(|c| c.nodes.first())
        .map_or((None, None, None), |node| {
            (
                Some(node.commit.oid.clone()),
                Some(node.commit.committed_date),
                node.commit
                    .status_check_rollup
                    .as_ref()
                    .map(|s| s.state.clone()),
            )
        })
}

fn make_pr(pr: &PullRequest, bucket: Bucket) -> CategorizedPr {
    let (sha, date, ci) = extract_commit_info(pr);
    CategorizedPr {
        id: pr.id.clone(),
        number: pr.number,
        title: pr.title.clone(),
        url: pr.url.clone(),
        repo: pr.repository.name_with_owner.clone(),
        author: pr
            .author
            .as_ref()
            .map(|a| a.login.clone())
            .unwrap_or_default(),
        bucket,
        created_at: pr.created_at,
        updated_at: pr.updated_at,
        last_commit_sha: sha,
        last_commit_date: date,
        ci_status: ci,
    }
}

fn categorize_authored(pr: &PullRequest) -> CategorizedPr {
    let bucket = if pr.is_draft.unwrap_or(false) {
        Bucket::Drafts
    } else if pr.review_decision.as_deref() == Some("APPROVED") {
        Bucket::Approved
    } else if pr.review_decision.as_deref() == Some("CHANGES_REQUESTED") {
        Bucket::ReturnedToYou
    } else {
        Bucket::WaitingForReviewers
    };
    make_pr(pr, bucket)
}

fn categorize_reviewed_by_me(pr: &PullRequest, viewer: &str) -> CategorizedPr {
    let _my_review = pr.latest_reviews.as_ref().and_then(|reviews| {
        reviews
            .nodes
            .iter()
            .find(|r| r.author.as_ref().is_some_and(|a| a.login == viewer))
    });

    make_pr(pr, Bucket::WaitingForAuthor)
}

pub fn categorize_all(data: &GqlData, viewer: &str) -> Vec<CategorizedPr> {
    let mut results = Vec::new();
    let mut seen_ids = HashSet::new();

    for pr in &data.needs_review.nodes {
        if seen_ids.insert(pr.id.clone()) {
            results.push(make_pr(pr, Bucket::NeedsYourReview));
        }
    }

    for pr in &data.authored.nodes {
        if seen_ids.insert(pr.id.clone()) {
            results.push(categorize_authored(pr));
        }
    }

    for pr in &data.reviewed_by_me.nodes {
        if seen_ids.insert(pr.id.clone()) {
            results.push(categorize_reviewed_by_me(pr, viewer));
        }
    }

    for pr in &data.recently_merged.nodes {
        if seen_ids.insert(pr.id.clone()) {
            results.push(CategorizedPr {
                id: pr.id.clone(),
                number: pr.number,
                title: pr.title.clone(),
                url: pr.url.clone(),
                repo: pr.repository.name_with_owner.clone(),
                author: pr
                    .author
                    .as_ref()
                    .map(|a| a.login.clone())
                    .unwrap_or_default(),
                bucket: Bucket::RecentlyMerged,
                created_at: pr.created_at,
                updated_at: pr.merged_at, // use merged_at as the "updated" time
                last_commit_sha: None,
                last_commit_date: pr.merged_at,
                ci_status: None,
            });
        }
    }

    results
}

// ── Filtering ───────────────────────────────────────────────────────────────

pub fn filter_prs(prs: Vec<CategorizedPr>, config: &AppConfig) -> Vec<CategorizedPr> {
    if config.blocked_repos.is_empty() {
        return prs;
    }
    prs.into_iter()
        .filter(|pr| config.is_repo_allowed(&pr.repo))
        .collect()
}

/// Extract repos grouped by owner, sorted. Returns (owner, [(repo_full_name, pr_count)])
pub fn extract_repo_tree(prs: &[CategorizedPr]) -> Vec<(String, Vec<(String, usize)>)> {
    let mut owner_repos: HashMap<String, HashMap<String, usize>> = HashMap::new();
    for pr in prs {
        let parts: Vec<&str> = pr.repo.splitn(2, '/').collect();
        if parts.len() == 2 {
            *owner_repos
                .entry(parts[0].to_string())
                .or_default()
                .entry(pr.repo.clone())
                .or_insert(0) += 1;
        }
    }

    let mut result: Vec<(String, Vec<(String, usize)>)> = owner_repos
        .into_iter()
        .map(|(owner, repos)| {
            let mut repo_list: Vec<(String, usize)> = repos.into_iter().collect();
            repo_list.sort_by(|a, b| a.0.cmp(&b.0));
            (owner, repo_list)
        })
        .collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

// ── Avatar caching ──────────────────────────────────────────────────────────

pub fn avatars_dir() -> std::path::PathBuf {
    let dir = crate::state::data_dir().join("avatars");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Download missing GitHub avatars for the given authors. Skips already-cached ones.
/// Downloads the image (GitHub often serves JPEG), decodes it, applies a circular
/// mask with transparency, and saves as PNG.
pub fn ensure_avatars(authors: &[String]) {
    let dir = avatars_dir();
    for author in authors {
        let path = dir.join(format!("{author}.png"));
        if path.exists()
            && std::fs::metadata(&path)
                .map(|m| m.len() > 0)
                .unwrap_or(false)
        {
            continue;
        }
        let tmp = dir.join(format!("{author}.tmp"));
        let url = format!("https://github.com/{author}.png?size=64");
        let ok = Command::new("curl")
            .args(["-sL", "--max-time", "5", "-o"])
            .arg(&tmp)
            .arg(&url)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if ok && tmp.exists() {
            if let Ok(bytes) = std::fs::read(&tmp)
                && let Ok(img) = image::load_from_memory(&bytes)
            {
                let _ = make_circular_png(&img, 64, &path);
            }
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

/// Resize an image to `size x size`, apply a circular mask, and save as RGBA PNG.
fn make_circular_png(img: &image::DynamicImage, size: u32, path: &std::path::Path) -> Result<()> {
    use image::ImageEncoder;
    use image::codecs::png::PngEncoder;
    use image::{Rgba, RgbaImage};

    let resized = img.resize_exact(size, size, image::imageops::FilterType::Lanczos3);
    let rgba = resized.to_rgba8();
    let mut output = RgbaImage::new(size, size);

    let center = size as f64 / 2.0;
    let radius = center;

    for (x, y, pixel) in rgba.enumerate_pixels() {
        let dx = x as f64 - center + 0.5;
        let dy = y as f64 - center + 0.5;
        let dist = (dx * dx + dy * dy).sqrt();

        if dist <= radius {
            // Smooth anti-aliased edge (1px feather)
            let alpha = if dist > radius - 1.0 {
                ((radius - dist) * 255.0) as u8
            } else {
                pixel[3]
            };
            output.put_pixel(x, y, Rgba([pixel[0], pixel[1], pixel[2], alpha]));
        }
    }

    let file = std::fs::File::create(path)?;
    let encoder = PngEncoder::new(std::io::BufWriter::new(file));
    encoder.write_image(output.as_raw(), size, size, image::ExtendedColorType::Rgba8)?;

    Ok(())
}

/// Generate a minidenticon-style avatar for a username and save as circular PNG.
/// Produces a 5x5 symmetric grid with a color derived from the name hash.
pub fn generate_identicon(username: &str, path: &std::path::Path) -> Result<()> {
    use image::codecs::png::PngEncoder;
    use image::{ImageEncoder, Rgba, RgbaImage};

    let size: u32 = 64;
    let grid = 5u32;
    let cell = size / grid; // 12px per cell, 4px padding

    // Simple hash (djb2)
    let mut hash: u64 = 5381;
    for b in username.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u64);
    }

    // Derive color from hash — pick from a set of saturated hues
    let hue = (hash % 360) as f64;
    let (r, g, b_col) = hsl_to_rgb(hue, 0.65, 0.55);
    let fg = Rgba([r, g, b_col, 255]);
    let bg = Rgba([30u8, 30, 50, 255]); // dark background matching app theme

    // Build 5x5 grid — horizontally symmetric, so columns 0,1,2 define everything
    // Use 15 bits from the hash for the 15 unique cells (3 cols x 5 rows)
    let bits = hash >> 8; // skip lowest bits (used for color)
    let mut grid_cells = [[false; 5]; 5];
    for (row, grid_row) in grid_cells.iter_mut().enumerate() {
        for col in 0..3 {
            let bit_idx = row * 3 + col;
            let on = (bits >> bit_idx) & 1 == 1;
            grid_row[col] = on;
            grid_row[4 - col] = on; // mirror
        }
    }

    // Render to image
    let padding = (size - grid * cell) / 2;
    let mut img = RgbaImage::from_pixel(size, size, bg);

    for (row, grid_row) in grid_cells.iter().enumerate() {
        for (col, &filled) in grid_row.iter().enumerate() {
            if !filled {
                continue;
            }
            let x0 = padding + col as u32 * cell;
            let y0 = padding + row as u32 * cell;
            for dy in 0..cell {
                for dx in 0..cell {
                    if x0 + dx < size && y0 + dy < size {
                        img.put_pixel(x0 + dx, y0 + dy, fg);
                    }
                }
            }
        }
    }

    // Apply circular mask
    let center = size as f64 / 2.0;
    let radius = center;
    let mut output = RgbaImage::new(size, size);
    for (x, y, pixel) in img.enumerate_pixels() {
        let dx = x as f64 - center + 0.5;
        let dy = y as f64 - center + 0.5;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist <= radius {
            let alpha = if dist > radius - 1.0 {
                ((radius - dist) * 255.0) as u8
            } else {
                pixel[3]
            };
            output.put_pixel(x, y, Rgba([pixel[0], pixel[1], pixel[2], alpha]));
        }
    }

    let file = std::fs::File::create(path)?;
    let encoder = PngEncoder::new(std::io::BufWriter::new(file));
    encoder.write_image(output.as_raw(), size, size, image::ExtendedColorType::Rgba8)?;

    Ok(())
}

/// Generate identicon avatars for demo mode (no network needed).
pub fn ensure_demo_avatars(authors: &[String]) {
    let dir = avatars_dir();
    for author in authors {
        let path = dir.join(format!("{author}.png"));
        if path.exists()
            && std::fs::metadata(&path)
                .map(|m| m.len() > 0)
                .unwrap_or(false)
        {
            continue;
        }
        let _ = generate_identicon(author, &path);
    }
}

/// Convert HSL to RGB (h in 0..360, s and l in 0..1)
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match h as u32 {
        0..60 => (c, x, 0.0),
        60..120 => (x, c, 0.0),
        120..180 => (0.0, c, x),
        180..240 => (0.0, x, c),
        240..300 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

/// Get the cached avatar path for a given author, if it exists.
pub fn avatar_path(author: &str) -> Option<std::path::PathBuf> {
    let path = avatars_dir().join(format!("{author}.png"));
    if path.exists()
        && std::fs::metadata(&path)
            .map(|m| m.len() > 0)
            .unwrap_or(false)
    {
        Some(path)
    } else {
        None
    }
}

// ── State diffing ───────────────────────────────────────────────────────────

pub fn diff_states(
    old_prs: &HashMap<String, CategorizedPr>,
    new_prs: &[CategorizedPr],
) -> Vec<Transition> {
    let mut transitions = Vec::new();
    let new_map: HashMap<&str, &CategorizedPr> =
        new_prs.iter().map(|pr| (pr.id.as_str(), pr)).collect();

    for pr in new_prs {
        match old_prs.get(&pr.id) {
            None => transitions.push(Transition::New { pr: pr.clone() }),
            Some(old_pr) if old_pr.bucket != pr.bucket => {
                transitions.push(Transition::Moved {
                    pr: pr.clone(),
                    from: old_pr.bucket,
                });
            }
            _ => {}
        }
    }

    for (id, old_pr) in old_prs {
        if !new_map.contains_key(id.as_str()) {
            transitions.push(Transition::Removed { pr: old_pr.clone() });
        }
    }

    transitions
}
