use ghtray_core::config::AppConfig;
use ghtray_core::github::{self, GhStatus};
use ghtray_core::logging;
use ghtray_core::models::{self, Bucket, CategorizedPr, Transition};
use ghtray_core::state;
use serde::Serialize;
use std::sync::Mutex;
use tauri::{
    AppHandle, Manager, RunEvent, WindowEvent,
    WebviewUrl, WebviewWindowBuilder,
    image::Image,
    menu::{IconMenuItem, IconMenuItemBuilder, MenuBuilder, MenuItem, MenuItemBuilder, PredefinedMenuItem},
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
    repos: Vec<RepoEntry>,
}

#[derive(Debug, Clone, Serialize)]
struct BucketEntry {
    id: String,
    label: String,
    visible: bool,
}

#[derive(Debug, Clone, Serialize)]
struct SettingsData {
    poll_interval_secs: u64,
    merged_window_days: i64,
    notifications_enabled: bool,
    notification_sound: bool,
    autostart: bool,
    buckets: Vec<BucketEntry>,
    orgs: Vec<OrgEntry>,
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
                    let short_name = full_name.split('/').nth(1).unwrap_or(&full_name).to_string();
                    let enabled = config.is_repo_allowed(&full_name);
                    RepoEntry { full_name, short_name, enabled, pr_count: count }
                })
                .collect();
            OrgEntry { name: owner, repos: repo_entries }
        })
        .collect();

    let buckets: Vec<BucketEntry> = Bucket::display_order()
        .iter()
        .map(|b| BucketEntry {
            id: b.id().to_string(),
            label: b.label().to_string(),
            visible: config.is_bucket_visible(b.id()),
        })
        .collect();

    let autostart = app.autolaunch().is_enabled().unwrap_or(false);

    SettingsData {
        poll_interval_secs: config.poll_interval_secs,
        merged_window_days: config.merged_window_days,
        notifications_enabled: config.notifications_enabled,
        notification_sound: config.notification_sound,
        autostart,
        buckets,
        orgs,
    }
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: tauri::State<'_, GhTrayState>,
    poll_interval_secs: u64,
    merged_window_days: i64,
    blocked_repos: Vec<String>,
    notifications_enabled: bool,
    notification_sound: bool,
    hidden_buckets: Vec<String>,
    autostart: bool,
) -> Result<(), String> {
    let mut config = state.config.lock().unwrap();
    config.poll_interval_secs = poll_interval_secs.max(30);
    config.merged_window_days = merged_window_days.max(1);
    config.blocked_repos = blocked_repos.into_iter().collect();
    config.notifications_enabled = notifications_enabled;
    config.notification_sound = notification_sound;
    config.hidden_buckets = hidden_buckets.into_iter().collect();
    config.save().map_err(|e| e.to_string())?;

    // Update autostart
    let mgr = app.autolaunch();
    let currently_enabled = mgr.is_enabled().unwrap_or(false);
    if autostart && !currently_enabled {
        let _ = mgr.enable();
    } else if !autostart && currently_enabled {
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

fn rebuild_tray_menu(app: &AppHandle, prs: &[CategorizedPr], config: &AppConfig) -> tauri::Result<()> {
    enum AnyItem {
        Text(MenuItem<tauri::Wry>),
        Icon(IconMenuItem<tauri::Wry>),
        Sep(PredefinedMenuItem<tauri::Wry>),
    }

    let mut items: Vec<AnyItem> = Vec::new();

    // Show error banner if present
    let app_state = app.state::<GhTrayState>();
    if let Some(err) = app_state.last_error.lock().unwrap().as_ref() {
        items.push(AnyItem::Text(
            MenuItemBuilder::with_id("error_msg", format!("⚠ {}", truncate(err, 50)))
                .enabled(false)
                .build(app)?
        ));
        items.push(AnyItem::Sep(PredefinedMenuItem::separator(app)?));
    }

    let mut has_buckets = false;

    for bucket in Bucket::display_order() {
        if !config.is_bucket_visible(bucket.id()) {
            continue;
        }
        let mut bucket_prs: Vec<&CategorizedPr> = prs.iter().filter(|pr| pr.bucket == *bucket).collect();
        if bucket_prs.is_empty() {
            continue;
        }

        // Sort by most recently updated first
        bucket_prs.sort_by(|a, b| {
            let a_time = a.updated_at.or(a.created_at);
            let b_time = b.updated_at.or(b.created_at);
            b_time.cmp(&a_time)
        });

        if has_buckets {
            items.push(AnyItem::Sep(PredefinedMenuItem::separator(app)?));
        }
        has_buckets = true;

        items.push(AnyItem::Text(
            MenuItemBuilder::with_id(format!("bucket_{}", bucket.id()), format!("{} ({})", bucket.label(), bucket_prs.len()))
                .enabled(false)
                .build(app)?
        ));

        for pr in &bucket_prs {
            let repo_short = pr.repo.split('/').last().unwrap_or(&pr.repo);
            let ci = ci_indicator(pr.ci_status.as_deref());
            let age = pr.created_at.map(|d| models::relative_time(d)).unwrap_or_default();
            let age_suffix = if age.is_empty() { String::new() } else { format!(" · {age}") };

            let label = format!("  #{} {}{} ({}){}", pr.number, truncate(&pr.title, 36), ci, repo_short, age_suffix);

            if let Some(avatar_path) = github::avatar_path(&pr.author) {
                if let Ok(bytes) = std::fs::read(&avatar_path) {
                    if let Ok(icon) = Image::from_bytes(&bytes) {
                        items.push(AnyItem::Icon(
                            IconMenuItemBuilder::new(&label)
                                .id(format!("pr_{}", pr.id))
                                .icon(icon)
                                .enabled(true)
                                .build(app)?
                        ));
                        continue;
                    }
                }
            }

            items.push(AnyItem::Text(
                MenuItemBuilder::with_id(format!("pr_{}", pr.id), &label)
                    .enabled(true)
                    .build(app)?
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
                .build(app)?
        ));
    }

    items.push(AnyItem::Sep(PredefinedMenuItem::separator(app)?));
    items.push(AnyItem::Text(MenuItemBuilder::with_id("action_refresh", "↻ Refresh Now").enabled(true).build(app)?));
    items.push(AnyItem::Text(MenuItemBuilder::with_id("action_settings", "Settings...").enabled(true).build(app)?));
    items.push(AnyItem::Text(MenuItemBuilder::with_id("action_quit", "Quit GH Tray").enabled(true).build(app)?));

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

fn send_notifications(app: &AppHandle, transitions: &[Transition], config: &AppConfig) {
    if !config.notifications_enabled {
        return;
    }

    for transition in transitions {
        if let Some((title, body)) = transition.notification_text() {
            let mut builder = app.notification().builder()
                .title(title)
                .body(&body);

            if config.notification_sound {
                builder = builder.sound("default");
            }

            let _ = builder.show();
        }
    }
}

// ── Loading indicator ────────────────────────────────────────────────────────

fn set_loading(app: &AppHandle, loading: bool) {
    if let Some(tray) = app.tray_by_id("main") {
        if loading {
            let _ = tray.set_title(Some("↻"));
            let _ = tray.set_tooltip(Some("GH Tray — Fetching..."));
        }
    }
}

// ── Fetch + state ───────────────────────────────────────────────────────────

fn do_fetch(app: &AppHandle) {
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

            let all_prs = github::categorize_all(&response.data, &viewer_login);
            let filtered = github::filter_prs(all_prs.clone(), &config);

            let authors: Vec<String> = filtered.iter()
                .map(|pr| pr.author.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            github::ensure_avatars(&authors);

            let old_state = state::load_state();
            let transitions = github::diff_states(&old_state.prs, &filtered);

            if old_state.last_fetch.is_some() {
                send_notifications(app, &transitions, &config);
            }

            let new_state = state::AppState {
                last_fetch: Some(chrono::Utc::now()),
                prs: filtered.iter().map(|pr| (pr.id.clone(), pr.clone())).collect(),
            };
            let _ = state::save_state(&new_state);

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
        .filter(|pr| matches!(pr.bucket, Bucket::NeedsYourReview | Bucket::ReturnedToYou))
        .count();

    if let Some(tray) = app.tray_by_id("main") {
        let has_error = app.state::<GhTrayState>().last_error.lock().unwrap().is_some();
        let title = if has_error {
            "⚠".to_string()
        } else if count > 0 {
            format!("{count}")
        } else {
            String::new()
        };
        let _ = tray.set_title(Some(&title));

        let tooltip = if has_error {
            "GH Tray — Error (check settings)".to_string()
        } else if count > 0 {
            format!("GH Tray — {count} action item(s)")
        } else {
            "GH Tray — All clear".to_string()
        };
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
        _ => {
            if let Some(pr_id) = id.strip_prefix("pr_") {
                let state = app.state::<GhTrayState>();
                let prs = state.prs.lock().unwrap();
                if let Some(pr) = prs.iter().find(|p| p.id == pr_id) {
                    let url = pr.url.clone();
                    drop(prs);
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
        .inner_size(420.0, 640.0)
        .resizable(false)
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

// ── Startup checks ──────────────────────────────────────────────────────────

fn check_startup(app: &AppHandle) {
    match github::check_gh_status() {
        GhStatus::Ok => {
            // All good — start silently
        }
        GhStatus::NotInstalled => {
            logging::log_error("gh CLI not found");
            let state = app.state::<GhTrayState>();
            *state.last_error.lock().unwrap() = Some("gh CLI not installed. Install from https://cli.github.com".to_string());
            let config = state.config.lock().unwrap().clone();
            update_tray(app, &[], &config);
            open_settings(app);
        }
        GhStatus::NotAuthenticated(_) => {
            logging::log_error("gh CLI not authenticated");
            let state = app.state::<GhTrayState>();
            *state.last_error.lock().unwrap() = Some("gh not authenticated. Run `gh auth login` in terminal".to_string());
            let config = state.config.lock().unwrap().clone();
            update_tray(app, &[], &config);
            open_settings(app);
        }
    }
}

// ── Tray setup ──────────────────────────────────────────────────────────────

fn setup_tray(app: &AppHandle) {
    let Some(tray) = app.tray_by_id("main") else { return };
    let default_config = AppConfig::default();
    let _ = rebuild_tray_menu(app, &[], &default_config);

    let app_handle = app.clone();
    tray.on_menu_event(move |_tray, event| {
        handle_menu_event(&app_handle, event.id().as_ref());
    });
}

// ── Entry point ─────────────────────────────────────────────────────────────

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .manage(GhTrayState::new())
        .invoke_handler(tauri::generate_handler![get_settings, save_settings])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            setup_tray(&app.handle());
            check_startup(&app.handle());
            start_polling(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let RunEvent::WindowEvent { label, event: WindowEvent::CloseRequested { api, .. }, .. } = &event {
            if label == "settings" {
                api.prevent_close();
                if let Some(window) = app_handle.get_webview_window("settings") {
                    let _ = window.hide();
                }
            }
        }
    });
}
