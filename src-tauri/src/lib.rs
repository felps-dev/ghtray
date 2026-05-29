use ghtray_core::config::AppConfig;
use ghtray_core::github::{self, GhStatus};
use ghtray_core::logging;
use ghtray_core::models::{self, Bucket, CategorizedPr, PendingNotification};
use ghtray_core::state::{self, NotificationKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

static DEMO_MODE: AtomicBool = AtomicBool::new(false);

fn is_demo() -> bool {
    DEMO_MODE.load(Ordering::Relaxed)
}
use tauri::{
    AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent,
    image::Image,
    menu::{
        IconMenuItem, IconMenuItemBuilder, MenuBuilder, MenuItem, MenuItemBuilder,
        PredefinedMenuItem,
    },
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;

// ── App state ───────────────────────────────────────────────────────────────

struct GhTrayState {
    viewer: Mutex<Option<String>>,
    prs: Mutex<Vec<CategorizedPr>>,
    all_prs: Mutex<Vec<CategorizedPr>>,
    config: Mutex<AppConfig>,
    last_error: Mutex<Option<String>>,
}

impl GhTrayState {
    fn new() -> Self {
        Self {
            viewer: Mutex::new(None),
            prs: Mutex::new(Vec::new()),
            all_prs: Mutex::new(Vec::new()),
            config: Mutex::new(AppConfig::load()),
            last_error: Mutex::new(None),
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let t: String = s.chars().take(max.saturating_sub(3)).collect();
        format!("{t}...")
    } else {
        s.to_string()
    }
}

fn ci_indicator(status: Option<&str>) -> &'static str {
    match status {
        Some("SUCCESS") => " ✓",
        Some("FAILURE") | Some("ERROR") => " ✗",
        Some("PENDING") | Some("EXPECTED") => " ◐",
        _ => "",
    }
}

// ── Tauri commands (for settings window) ────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct RepoEntry {
    full_name: String,
    short_name: String,
    enabled: bool,
    pr_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct OrgEntry {
    name: String,
    /// False when the whole org is blocked (every repo under it muted,
    /// including future ones). Distinct from "all known repos unchecked".
    blocked: bool,
    repos: Vec<RepoEntry>,
}

#[derive(Debug, Clone, Serialize)]
struct BucketEntry {
    id: String,
    label: String,
    visible: bool,
    badge: bool,
    sort: String,
    /// True when this bucket has a notification title — i.e. it's one of
    /// the buckets that can fire desktop notifications. The settings UI
    /// uses this to scope the per-event notify/sound matrix.
    notifiable: bool,
    notify: bool,
    sound: bool,
}

#[derive(Debug, Clone, Serialize)]
struct GhStatusInfo {
    ok: bool,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct SettingsData {
    poll_interval_secs: u64,
    merged_window_days: i64,
    max_pr_age_days: u64,
    notifications_enabled: bool,
    notification_sound: bool,
    notification_sound_path: String,
    autostart: bool,
    buckets: Vec<BucketEntry>,
    orgs: Vec<OrgEntry>,
    /// Full current block sets — the UI merges these on save so it can
    /// preserve blocks for orgs/repos that have no PRs right now and thus
    /// don't appear in the tree.
    blocked_repos: Vec<String>,
    blocked_orgs: Vec<String>,
    gh_status: GhStatusInfo,
    gh_cli_path: String,
    detected_gh_path: String,
    app_version: String,
}

#[tauri::command]
fn get_settings(app: AppHandle, state: tauri::State<'_, GhTrayState>) -> SettingsData {
    let config = state.config.lock().unwrap();
    let all_prs = state.all_prs.lock().unwrap();
    let tree = github::extract_repo_tree(&all_prs);

    let orgs: Vec<OrgEntry> = tree
        .into_iter()
        .map(|(owner, repos)| {
            let repo_entries: Vec<RepoEntry> = repos
                .into_iter()
                .map(|(full_name, count)| {
                    let short_name = full_name
                        .split('/')
                        .nth(1)
                        .unwrap_or(&full_name)
                        .to_string();
                    let enabled = config.is_repo_allowed(&full_name);
                    RepoEntry {
                        full_name,
                        short_name,
                        enabled,
                        pr_count: count,
                    }
                })
                .collect();
            let blocked = config.is_org_blocked(&owner);
            OrgEntry {
                name: owner,
                blocked,
                repos: repo_entries,
            }
        })
        .collect();

    let buckets: Vec<BucketEntry> = config
        .ordered_buckets()
        .iter()
        .map(|b| {
            let id = b.id();
            BucketEntry {
                id: id.to_string(),
                label: b.label().to_string(),
                visible: config.is_bucket_visible(id),
                badge: config.counts_for_badge(id),
                sort: config.sort_for_bucket(id).to_string(),
                notifiable: b.notification_title().is_some(),
                notify: config.notify_buckets.contains(id),
                sound: config.sound_buckets.contains(id),
            }
        })
        .collect();

    let autostart = app.autolaunch().is_enabled().unwrap_or(false);

    let gh_status = if is_demo() {
        GhStatusInfo {
            ok: true,
            message: "Demo mode".to_string(),
        }
    } else {
        match github::check_gh_status() {
            GhStatus::Ok => GhStatusInfo {
                ok: true,
                message: "Connected".to_string(),
            },
            GhStatus::NotInstalled => GhStatusInfo {
                ok: false,
                message: "gh CLI not installed. Install from https://cli.github.com".to_string(),
            },
            GhStatus::NotAuthenticated(msg) => GhStatusInfo {
                ok: false,
                message: format!("Not authenticated. Run `gh auth login`. {msg}"),
            },
        }
    };

    SettingsData {
        poll_interval_secs: config.poll_interval_secs,
        merged_window_days: config.merged_window_days,
        max_pr_age_days: config.max_pr_age_days,
        notifications_enabled: config.notifications_enabled,
        notification_sound: config.notification_sound,
        notification_sound_path: config.notification_sound_path.clone(),
        autostart,
        buckets,
        orgs,
        blocked_repos: config.blocked_repos.iter().cloned().collect(),
        blocked_orgs: config.blocked_orgs.iter().cloned().collect(),
        gh_status,
        gh_cli_path: config.gh_cli_path.clone(),
        detected_gh_path: github::auto_detected_gh_path(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[tauri::command]
fn preview_sound(spec: String) {
    play_system_sound(&spec);
}

#[tauri::command]
fn open_log() {
    let path = ghtray_core::state::data_dir().join("ghtray.log");
    let _ = tauri_plugin_opener::open_path(path, None::<&str>);
}

#[tauri::command]
fn hide_settings(app: AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.hide();
    }
}

#[tauri::command]
fn check_gh(state: tauri::State<'_, GhTrayState>) -> GhStatusInfo {
    if is_demo() {
        return GhStatusInfo {
            ok: true,
            message: "Demo mode".to_string(),
        };
    }
    match github::check_gh_status() {
        GhStatus::Ok => {
            *state.last_error.lock().unwrap() = None;
            GhStatusInfo {
                ok: true,
                message: "Connected".to_string(),
            }
        }
        GhStatus::NotInstalled => GhStatusInfo {
            ok: false,
            message: "gh CLI not installed. Install from https://cli.github.com".to_string(),
        },
        GhStatus::NotAuthenticated(msg) => GhStatusInfo {
            ok: false,
            message: format!("Not authenticated. Run `gh auth login`. {msg}"),
        },
    }
}

#[derive(Debug, Clone, Deserialize)]
struct SaveSettingsPayload {
    poll_interval_secs: u64,
    merged_window_days: i64,
    max_pr_age_days: u64,
    blocked_repos: Vec<String>,
    blocked_orgs: Vec<String>,
    notifications_enabled: bool,
    notification_sound: bool,
    notification_sound_path: String,
    hidden_buckets: Vec<String>,
    badge_buckets: Vec<String>,
    bucket_order: Vec<String>,
    notify_buckets: Vec<String>,
    sound_buckets: Vec<String>,
    bucket_sort: HashMap<String, String>,
    autostart: bool,
    gh_cli_path: String,
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: tauri::State<'_, GhTrayState>,
    payload: SaveSettingsPayload,
) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    config.poll_interval_secs = payload.poll_interval_secs.max(30);
    config.merged_window_days = payload.merged_window_days.max(1);
    config.max_pr_age_days = payload.max_pr_age_days;
    config.blocked_repos = payload.blocked_repos.into_iter().collect();
    config.blocked_orgs = payload.blocked_orgs.into_iter().collect();
    config.notifications_enabled = payload.notifications_enabled;
    config.notification_sound = payload.notification_sound;
    config.notification_sound_path = payload.notification_sound_path;
    config.hidden_buckets = payload.hidden_buckets.into_iter().collect();
    config.badge_buckets = payload.badge_buckets.into_iter().collect();
    config.bucket_order = payload.bucket_order;
    config.notify_buckets = payload.notify_buckets.into_iter().collect();
    config.sound_buckets = payload.sound_buckets.into_iter().collect();
    config.bucket_sort = payload.bucket_sort;
    config.gh_cli_path = payload.gh_cli_path;

    // Apply gh CLI path override
    let gh_override = if config.gh_cli_path.is_empty() {
        None
    } else {
        Some(config.gh_cli_path.clone())
    };
    github::set_gh_cli_path(gh_override);

    config.save().map_err(|e| e.to_string())?;

    // Update autostart
    let mgr = app.autolaunch();
    let currently_enabled = mgr.is_enabled().unwrap_or(false);
    if payload.autostart && !currently_enabled {
        let _ = mgr.enable();
    } else if !payload.autostart && currently_enabled {
        let _ = mgr.disable();
    }

    // Re-filter and update tray
    let all_prs = state.all_prs.lock().unwrap().clone();
    let filtered = github::filter_prs(all_prs, &config);

    *state.prs.lock().unwrap() = filtered.clone();
    update_tray(&app, &filtered, &config);
    drop(config);

    Ok(())
}

// ── Native tray menu ────────────────────────────────────────────────────────

fn rebuild_tray_menu(
    app: &AppHandle,
    prs: &[CategorizedPr],
    config: &AppConfig,
) -> tauri::Result<()> {
    enum AnyItem {
        Text(MenuItem<tauri::Wry>),
        Icon(IconMenuItem<tauri::Wry>),
        Sep(PredefinedMenuItem<tauri::Wry>),
    }

    let mut items: Vec<AnyItem> = Vec::new();

    // Show error banner if present — clickable so the user can open the
    // log file and see the full message that didn't fit in the menu.
    let app_state = app.state::<GhTrayState>();
    if let Some(err) = app_state.last_error.lock().unwrap().as_ref() {
        items.push(AnyItem::Text(
            MenuItemBuilder::with_id(
                "action_view_log",
                format!("⚠ {} — click for log", truncate(err, 60)),
            )
            .enabled(true)
            .build(app)?,
        ));
        items.push(AnyItem::Sep(PredefinedMenuItem::separator(app)?));
    }

    let mut has_buckets = false;

    for bucket in config.ordered_buckets() {
        let bucket = &bucket;
        if !config.is_bucket_visible(bucket.id()) {
            continue;
        }
        let mut bucket_prs: Vec<&CategorizedPr> =
            prs.iter().filter(|pr| pr.bucket == *bucket).collect();
        if bucket_prs.is_empty() {
            continue;
        }

        github::sort_prs(&mut bucket_prs, config.sort_for_bucket(bucket.id()));

        if has_buckets {
            items.push(AnyItem::Sep(PredefinedMenuItem::separator(app)?));
        }
        has_buckets = true;

        items.push(AnyItem::Text(
            MenuItemBuilder::with_id(
                format!("bucket_{}", bucket.id()),
                format!("{} ({}) — open all", bucket.label(), bucket_prs.len()),
            )
            .enabled(true)
            .build(app)?,
        ));

        for pr in &bucket_prs {
            let repo_short = pr.repo.split('/').next_back().unwrap_or(&pr.repo);
            let ci = ci_indicator(pr.ci_status.as_deref());
            let age = pr.created_at.map(models::relative_time).unwrap_or_default();
            let age_suffix = if age.is_empty() {
                String::new()
            } else {
                format!(" · {age}")
            };
            // Last-notified indicator — populated from the persistent ledger
            // only when a desktop notification actually fired for this PR.
            let notified_suffix = pr
                .last_notified_at
                .map(|t| format!(" · 🔔 {}", models::relative_time(t)))
                .unwrap_or_default();

            let label = format!(
                "  #{} {}{} ({}){}{}",
                pr.number,
                truncate(&pr.title, 36),
                ci,
                repo_short,
                age_suffix,
                notified_suffix
            );

            if let Some(avatar_path) = github::avatar_path(&pr.author)
                && let Ok(bytes) = std::fs::read(&avatar_path)
                && let Ok(icon) = Image::from_bytes(&bytes)
            {
                items.push(AnyItem::Icon(
                    IconMenuItemBuilder::new(&label)
                        .id(format!("pr_{}", pr.id))
                        .icon(icon)
                        .enabled(true)
                        .build(app)?,
                ));
                continue;
            }

            items.push(AnyItem::Text(
                MenuItemBuilder::with_id(format!("pr_{}", pr.id), &label)
                    .enabled(true)
                    .build(app)?,
            ));
        }
    }

    if !has_buckets {
        let msg = if app_state.last_error.lock().unwrap().is_some() {
            "Unable to fetch PRs"
        } else {
            "No pull requests"
        };
        items.push(AnyItem::Text(
            MenuItemBuilder::with_id("empty", msg)
                .enabled(false)
                .build(app)?,
        ));
    }

    items.push(AnyItem::Sep(PredefinedMenuItem::separator(app)?));
    items.push(AnyItem::Text(
        MenuItemBuilder::with_id("action_refresh", "↻ Refresh Now")
            .enabled(true)
            .build(app)?,
    ));
    items.push(AnyItem::Text(
        MenuItemBuilder::with_id("action_settings", "Settings...")
            .enabled(true)
            .build(app)?,
    ));
    items.push(AnyItem::Text(
        MenuItemBuilder::with_id("action_quit", "Quit GH Tray")
            .enabled(true)
            .build(app)?,
    ));

    let mut builder = MenuBuilder::new(app);
    for item in &items {
        match item {
            AnyItem::Text(i) => builder = builder.item(i),
            AnyItem::Icon(i) => builder = builder.item(i),
            AnyItem::Sep(i) => builder = builder.item(i),
        }
    }
    let menu = builder.build()?;

    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(menu))?;
    }

    Ok(())
}

// ── Notifications ────────────────────────────────────────────────────────────

fn send_notifications(
    app: &AppHandle,
    pending: &[PendingNotification],
    config: &AppConfig,
    notified: &mut HashMap<String, NotificationKey>,
) {
    if !config.notifications_enabled {
        return;
    }

    let now = chrono::Utc::now();

    for note in pending {
        // Per-repo guard: filter_prs already excludes blocked repos before
        // pending_notifications sees them, but this explicit check ensures
        // a notification will never fire for a repo the user has unchecked
        // — even if filtering ever regresses.
        if !config.is_repo_allowed(&note.repo) {
            continue;
        }
        // Per-bucket gate: master toggle already passed above, but a bucket
        // can be individually muted in settings.
        if !config.notify_for_bucket(note.bucket.id()) {
            continue;
        }

        let builder = app
            .notification()
            .builder()
            .title(note.title)
            .body(&note.body);

        if config.sound_for_bucket(note.bucket.id()) {
            // tauri-plugin-notification's .sound() is unreliable on macOS,
            // so we fire the notification silently and play the sound ourselves.
            play_system_sound(&config.notification_sound_path);
        }

        let _ = builder.show();

        // Stamp the ledger entry now that a real notification was emitted.
        // This drives the "🔔 5m" indicator next to each PR row.
        if let Some(entry) = notified.get_mut(&note.pr_id) {
            entry.notified_at = Some(now);
        }
    }
}

/// Resolve a notification sound spec into an absolute filesystem path:
/// - empty → default Glass.aiff
/// - bare name (no `/`) → /System/Library/Sounds/{spec}.aiff
/// - contains `/` → used as-is
fn resolve_sound_path(spec: &str) -> String {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return "/System/Library/Sounds/Glass.aiff".to_string();
    }
    if trimmed.contains('/') {
        return trimmed.to_string();
    }
    format!("/System/Library/Sounds/{trimmed}.aiff")
}

/// Play a notification sound via `afplay`. `spec` accepts a macOS system
/// sound name (e.g. "Glass", "Pop") or an absolute path.
fn play_system_sound(spec: &str) {
    let path = resolve_sound_path(spec);
    std::thread::spawn(move || {
        let _ = std::process::Command::new("afplay").arg(path).spawn();
    });
}

// ── Demo mode ────────────────────────────────────────────────────────────────

fn demo_prs() -> Vec<CategorizedPr> {
    use chrono::{Duration, Utc};

    let now = Utc::now();

    let pr = |id: &str,
              number: u32,
              title: &str,
              repo: &str,
              author: &str,
              bucket: Bucket,
              ci: Option<&str>,
              hours_ago: i64| CategorizedPr {
        id: id.to_string(),
        number,
        title: title.to_string(),
        url: format!("https://github.com/{repo}/pull/{number}"),
        repo: repo.to_string(),
        author: author.to_string(),
        bucket,
        created_at: Some(now - Duration::hours(hours_ago)),
        updated_at: Some(now - Duration::hours(hours_ago / 2)),
        last_commit_sha: Some(format!("abc{id}")),
        last_commit_date: Some(now - Duration::hours(hours_ago / 2)),
        ci_status: ci.map(String::from),
        last_notified_at: Some(now - Duration::minutes(hours_ago * 15)),
    };

    vec![
        // Needs Your Review
        pr(
            "d1",
            342,
            "Add OAuth2 PKCE flow",
            "acme/backend",
            "olivia-dev",
            Bucket::NeedsYourReview,
            Some("SUCCESS"),
            2,
        ),
        pr(
            "d2",
            187,
            "Migrate users table to UUIDs",
            "acme/backend",
            "james-eng",
            Bucket::NeedsYourReview,
            Some("SUCCESS"),
            5,
        ),
        pr(
            "d3",
            891,
            "Add dark mode support",
            "acme/web-app",
            "sarah-ui",
            Bucket::NeedsYourReview,
            Some("PENDING"),
            1,
        ),
        pr(
            "d4",
            56,
            "Bump dependencies (Feb 2026)",
            "acme/infra",
            "dependabot",
            Bucket::NeedsYourReview,
            Some("SUCCESS"),
            8,
        ),
        // Returned to You (changes requested)
        pr(
            "d5",
            204,
            "Refactor payment processing",
            "acme/backend",
            "demo-user",
            Bucket::ReturnedToYou,
            Some("FAILURE"),
            24,
        ),
        pr(
            "d6",
            723,
            "Fix race condition in queue worker",
            "acme/backend",
            "demo-user",
            Bucket::ReturnedToYou,
            Some("SUCCESS"),
            48,
        ),
        // Approved
        pr(
            "d7",
            445,
            "Add retry logic to webhook delivery",
            "acme/backend",
            "demo-user",
            Bucket::Approved,
            Some("SUCCESS"),
            3,
        ),
        pr(
            "d8",
            112,
            "Update onboarding flow copy",
            "acme/web-app",
            "demo-user",
            Bucket::Approved,
            Some("SUCCESS"),
            6,
        ),
        // Waiting for Reviewers
        pr(
            "d9",
            890,
            "Implement rate limiting middleware",
            "acme/backend",
            "demo-user",
            Bucket::WaitingForReviewers,
            Some("SUCCESS"),
            12,
        ),
        pr(
            "d10",
            334,
            "Add E2E tests for checkout",
            "acme/web-app",
            "demo-user",
            Bucket::WaitingForReviewers,
            Some("PENDING"),
            4,
        ),
        // Waiting for Author
        pr(
            "d11",
            567,
            "Add GraphQL subscriptions",
            "acme/backend",
            "mike-gql",
            Bucket::WaitingForAuthor,
            Some("SUCCESS"),
            72,
        ),
        // CI Failing (Drafts bucket used as example)
        pr(
            "d12",
            901,
            "WIP: New dashboard layout",
            "acme/web-app",
            "demo-user",
            Bucket::Drafts,
            None,
            168,
        ),
        // Recently Merged
        pr(
            "d13",
            200,
            "Fix memory leak in connection pool",
            "acme/backend",
            "demo-user",
            Bucket::RecentlyMerged,
            Some("SUCCESS"),
            26,
        ),
        pr(
            "d14",
            88,
            "Add Terraform module for Redis",
            "acme/infra",
            "demo-user",
            Bucket::RecentlyMerged,
            Some("SUCCESS"),
            50,
        ),
    ]
}

// ── Loading indicator ────────────────────────────────────────────────────────

fn set_loading(app: &AppHandle, loading: bool) {
    if let Some(tray) = app.tray_by_id("main")
        && loading
    {
        let _ = tray.set_title(Some("↻"));
        let _ = tray.set_tooltip(Some("GH Tray — Fetching..."));
    }
}

// ── Fetch + state ───────────────────────────────────────────────────────────

fn do_fetch(app: &AppHandle) {
    if is_demo() {
        do_fetch_demo(app);
    } else {
        do_fetch_live(app);
    }
}

fn do_fetch_demo(app: &AppHandle) {
    let app_state = app.state::<GhTrayState>();
    let config = app_state.config.lock().unwrap().clone();

    let all_prs = demo_prs();
    let filtered = github::filter_prs(all_prs.clone(), &config);

    // Generate identicon avatars for all demo authors
    let authors: Vec<String> = filtered
        .iter()
        .map(|pr| pr.author.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    github::ensure_demo_avatars(&authors);

    *app_state.last_error.lock().unwrap() = None;
    *app_state.all_prs.lock().unwrap() = all_prs;
    *app_state.prs.lock().unwrap() = filtered.clone();

    update_tray(app, &filtered, &config);
}

fn do_fetch_live(app: &AppHandle) {
    let app_state = app.state::<GhTrayState>();

    set_loading(app, true);

    {
        let mut viewer = app_state.viewer.lock().unwrap();
        if viewer.is_none() {
            match github::get_viewer_login() {
                Ok(login) => *viewer = Some(login),
                Err(e) => {
                    let msg = format!("{e}");
                    logging::log_error(&msg);
                    *app_state.last_error.lock().unwrap() = Some(msg);
                    let config = app_state.config.lock().unwrap().clone();
                    let prs = app_state.prs.lock().unwrap().clone();
                    update_tray(app, &prs, &config);
                    return;
                }
            }
        }
    }

    let viewer_login = app_state.viewer.lock().unwrap().clone().unwrap_or_default();
    let config = app_state.config.lock().unwrap().clone();

    match github::fetch_prs(config.merged_window_days) {
        Ok(response) => {
            // Clear any previous error
            *app_state.last_error.lock().unwrap() = None;

            let mut all_prs = github::categorize_all(&response.data, &viewer_login);
            let mut filtered = github::filter_prs(all_prs.clone(), &config);

            let authors: Vec<String> = filtered
                .iter()
                .map(|pr| pr.author.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            github::ensure_avatars(&authors);

            let mut saved = state::load_state();
            let needs_seed = !saved.notified_seeded;

            // Ledger-based dedup: only re-notify on genuine state change
            // (bucket or commit_sha differs from last notification we emitted).
            // pending_notifications also seeds the ledger; we silently skip the
            // emit on first run / upgrade so users don't get a flood of
            // notifications for PRs that were already on screen.
            let pending = github::pending_notifications(&filtered, &mut saved.notified);
            state::trim_notified(&mut saved.notified);
            if !needs_seed {
                send_notifications(app, &pending, &config, &mut saved.notified);
            }

            // Copy fresh notified_at timestamps from the ledger onto each PR
            // so the tray row can render "🔔 5m" without re-reading state.
            github::enrich_with_notified(&mut all_prs, &saved.notified);
            github::enrich_with_notified(&mut filtered, &saved.notified);

            // Persist unfiltered all_prs so we can restore the tray on the
            // next launch even if the first fetch fails.
            saved.last_fetch = Some(chrono::Utc::now());
            saved.all_prs = all_prs.clone();
            saved.notified_seeded = true;
            let _ = state::save_state(&saved);

            *app_state.all_prs.lock().unwrap() = all_prs;
            *app_state.prs.lock().unwrap() = filtered.clone();

            update_tray(app, &filtered, &config);
        }
        Err(e) => {
            let msg = format!("{e}");
            logging::log_error(&msg);
            *app_state.last_error.lock().unwrap() = Some(msg);
            let prs = app_state.prs.lock().unwrap().clone();
            update_tray(app, &prs, &config);
        }
    }
}

fn update_tray(app: &AppHandle, prs: &[CategorizedPr], config: &AppConfig) {
    let count = prs
        .iter()
        .filter(|pr| config.counts_for_badge(pr.bucket.id()))
        .count();

    if let Some(tray) = app.tray_by_id("main") {
        let state = app.state::<GhTrayState>();
        let error_ref = state.last_error.lock().unwrap();
        let has_error = error_ref.is_some();
        let is_gh_error = error_ref
            .as_ref()
            .map(|e| e.contains("not installed") || e.contains("not authenticated"))
            .unwrap_or(false);
        drop(error_ref);

        let title = if is_gh_error {
            "\u{2717}".to_string() // ✗
        } else if has_error {
            "\u{26A0}".to_string() // ⚠
        } else if count > 0 {
            format!("{count}")
        } else {
            String::new()
        };
        let _ = tray.set_title(Some(&title));

        let error_ref = state.last_error.lock().unwrap();
        let tooltip = if is_gh_error {
            "GH Tray — gh CLI error (check settings)".to_string()
        } else if let Some(msg) = error_ref.as_ref() {
            format!("GH Tray — {}", truncate(msg, 200))
        } else if count > 0 {
            format!("GH Tray — {count} action item(s)")
        } else {
            "GH Tray — All clear".to_string()
        };
        drop(error_ref);
        let _ = tray.set_tooltip(Some(&tooltip));
    }

    let _ = rebuild_tray_menu(app, prs, config);
}

// ── Menu click handler ──────────────────────────────────────────────────────

fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        "action_quit" => app.exit(0),
        "action_refresh" => {
            let app_clone = app.clone();
            std::thread::spawn(move || do_fetch(&app_clone));
        }
        "action_settings" => open_settings(app),
        "action_view_log" => {
            let path = ghtray_core::state::data_dir().join("ghtray.log");
            let _ = tauri_plugin_opener::open_path(path, None::<&str>);
        }
        _ => {
            if let Some(pr_id) = id.strip_prefix("pr_") {
                let state = app.state::<GhTrayState>();
                let prs = state.prs.lock().unwrap();
                if let Some(pr) = prs.iter().find(|p| p.id == pr_id) {
                    let url = pr.url.clone();
                    drop(prs);
                    let _ = tauri_plugin_opener::open_url(&url, None::<&str>);
                }
            } else if let Some(bucket_id) = id.strip_prefix("bucket_") {
                let Some(bucket) = Bucket::from_id(bucket_id) else {
                    return;
                };
                let state = app.state::<GhTrayState>();
                let prs = state.prs.lock().unwrap();
                let urls: Vec<String> = prs
                    .iter()
                    .filter(|pr| pr.bucket == bucket)
                    .map(|pr| pr.url.clone())
                    .collect();
                drop(prs);
                for url in urls {
                    let _ = tauri_plugin_opener::open_url(&url, None::<&str>);
                }
            }
        }
    }
}

fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let _ = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("GH Tray Settings")
        .inner_size(780.0, 640.0)
        .resizable(false)
        .disable_drag_drop_handler() // Allow HTML5 drag-and-drop in the webview
        .build();
}

// ── Polling ─────────────────────────────────────────────────────────────────

fn start_polling(app: AppHandle) {
    std::thread::spawn(move || {
        do_fetch(&app);

        loop {
            let interval = {
                let state = app.state::<GhTrayState>();
                let config = state.config.lock().unwrap();
                std::time::Duration::from_secs(config.poll_interval_secs_clamped())
            };
            std::thread::sleep(interval);
            do_fetch(&app);
        }
    });
}

// ── Startup cache restore ───────────────────────────────────────────────────

/// Load the last persisted `all_prs` into in-memory state and render the tray
/// immediately. This way the user sees their last-known PRs before the first
/// live fetch — and survives a failed first fetch on a flaky network.
fn restore_cached_prs(app: &AppHandle) {
    let saved = state::load_state();
    if saved.all_prs.is_empty() {
        return;
    }

    let app_state = app.state::<GhTrayState>();
    let config = app_state.config.lock().unwrap().clone();
    let mut all_prs = saved.all_prs;
    // Restore the "🔔 ..." indicator immediately on startup from the
    // persisted ledger so the cached tray rows look identical to the
    // live ones, rather than blanking until the first refresh fires.
    github::enrich_with_notified(&mut all_prs, &saved.notified);
    let mut filtered = github::filter_prs(all_prs.clone(), &config);
    github::enrich_with_notified(&mut filtered, &saved.notified);

    let authors: Vec<String> = filtered
        .iter()
        .map(|pr| pr.author.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    github::ensure_avatars(&authors);

    *app_state.all_prs.lock().unwrap() = all_prs;
    *app_state.prs.lock().unwrap() = filtered.clone();

    update_tray(app, &filtered, &config);
}

// ── Startup checks ──────────────────────────────────────────────────────────

fn check_startup(app: &AppHandle) {
    if is_demo() {
        return;
    }
    match github::check_gh_status() {
        GhStatus::Ok => {
            // All good — start silently
        }
        GhStatus::NotInstalled => {
            logging::log_error("gh CLI not found");
            let state = app.state::<GhTrayState>();
            *state.last_error.lock().unwrap() =
                Some("gh CLI not installed. Install from https://cli.github.com".to_string());
            let config = state.config.lock().unwrap().clone();
            update_tray(app, &[], &config);
            open_settings(app);
        }
        GhStatus::NotAuthenticated(_) => {
            logging::log_error("gh CLI not authenticated");
            let state = app.state::<GhTrayState>();
            *state.last_error.lock().unwrap() =
                Some("gh not authenticated. Run `gh auth login` in terminal".to_string());
            let config = state.config.lock().unwrap().clone();
            update_tray(app, &[], &config);
            open_settings(app);
        }
    }
}

// ── Tray setup ──────────────────────────────────────────────────────────────

fn setup_tray(app: &AppHandle) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    let default_config = AppConfig::default();
    let _ = rebuild_tray_menu(app, &[], &default_config);

    let app_handle = app.clone();
    tray.on_menu_event(move |_tray, event| {
        handle_menu_event(&app_handle, event.id().as_ref());
    });
}

// ── Entry point ─────────────────────────────────────────────────────────────

pub fn run() {
    if std::env::args().any(|a| a == "--demo") {
        DEMO_MODE.store(true, Ordering::Relaxed);
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .manage(GhTrayState::new())
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            check_gh,
            hide_settings,
            preview_sound,
            open_log
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            // Apply custom gh CLI path from config before any gh calls
            {
                let state = app.state::<GhTrayState>();
                let config = state.config.lock().unwrap();
                if !config.gh_cli_path.is_empty() {
                    github::set_gh_cli_path(Some(config.gh_cli_path.clone()));
                }
            }

            setup_tray(app.handle());
            restore_cached_prs(app.handle());
            check_startup(app.handle());
            start_polling(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let RunEvent::WindowEvent {
            label,
            event: WindowEvent::CloseRequested { api, .. },
            ..
        } = &event
            && label == "settings"
        {
            api.prevent_close();
            if let Some(window) = app_handle.get_webview_window("settings") {
                let _ = window.hide();
            }
        }
    });
}
