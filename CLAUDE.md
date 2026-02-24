# GH Tray — Claude Instructions

## Onboarding
1. Read `PROGRESS.md` — current phase, next step, known issues
2. Read `IDEA.md` — full spec, architecture, PR lifecycle model, phased plan
3. Check `docs/` for detailed notes on completed phases

## Workflow
- Start every session by reading `PROGRESS.md`
- End every session by updating `PROGRESS.md`
- Document phase-specific findings in `docs/phaseN-*.md`
- Only touch `IDEA.md` if the plan diverges from what's written there
- **User tests the app** — never launch the binary yourself, just build
- Always run `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` before committing

## Stack
- Rust + Tauri v2 (system tray app, no dock icon)
- Frontend: HTML/CSS/JS in webview (settings window only)
- Tray popup: **native OS menu** (not webview)
- Data: `gh api graphql` subprocess calls (no direct OAuth)
- State: `~/Library/Application Support/ghtray/ghtray-state.json`
- Config: `~/Library/Application Support/ghtray/ghtray-config.json`

## Key Decisions
- Single GraphQL query with 4 aliased searches (5 rate limit points)
- `latestReviews` over `reviews` (deduplicated per reviewer)
- Dedup PRs by node ID across search results
- `@me` in search queries (no need to pass username)
- Repo filter uses **block-list** (`blocked_repos`) so new repos show by default
- `set_activation_policy(Accessory)` hides dock icon on macOS
- Settings window close is intercepted → hides instead of killing app
- `tauri-plugin-single-instance` prevents concurrent instances
- Badge count is **configurable** — user picks which sections count (`badge_buckets` in config)
- Section display order is **user-controlled** — drag & drop in settings (`bucket_order` in config)
- Notification sound uses `afplay /System/Library/Sounds/Glass.aiff` (tauri-plugin-notification `.sound()` is unreliable on macOS)
- `save_settings` uses a single `SaveSettingsPayload` struct (avoids Tauri's arg limit)
- gh CLI status is exposed in settings with a "Try Again" button; tray shows `✗` when gh is broken

## Building & Running
```
cargo build                    # build workspace
cargo clippy --workspace --all-targets -- -D warnings  # lint check
cargo fmt --all -- --check     # format check
target/debug/ghtray            # user runs manually to test
```

## CI/CD
- `.github/workflows/ci.yml` — runs on push to `main` + PRs: fmt, clippy, check, test
- `.github/workflows/release.yml` — triggers on GitHub Release: builds macOS aarch64 + x86_64 .dmg via `tauri-apps/tauri-action`
- To release: `gh release create v0.x.x --title "GH Tray v0.x.x" --generate-notes`

## Config Fields (AppConfig)
| Field | Type | Default | Description |
|---|---|---|---|
| `poll_interval_secs` | u64 | 120 | Polling interval (min 30) |
| `merged_window_days` | i64 | 7 | How far back to show merged PRs |
| `blocked_repos` | HashSet<String> | empty | Repos to hide (block-list) |
| `notifications_enabled` | bool | true | Desktop notifications |
| `notification_sound` | bool | true | Play sound with notifications |
| `hidden_buckets` | HashSet<String> | empty | Sections to hide from tray |
| `badge_buckets` | HashSet<String> | needs_your_review, returned_to_you | Sections that count in badge |
| `bucket_order` | Vec<String> | empty (uses default) | Custom section display order |

## Project Structure
```
IDEA.md                            — spec (source of truth)
PROGRESS.md                        — progress tracker (read first, update last)
docs/phase1-data-exploration.md    — API findings
Cargo.toml                         — workspace root
crates/ghtray-core/
  src/lib.rs                       — pub modules
  src/models.rs                    — GraphQL types, Bucket enum, CategorizedPr
  src/github.rs                    — fetch, categorize, filter, diff, avatars
  src/config.rs                    — AppConfig (poll, repos, buckets, badge, order)
  src/state.rs                     — AppState persistence
  src/logging.rs                   — Error logging to file
src-tauri/
  Cargo.toml                       — tauri app deps
  tauri.conf.json                  — app config (tray icon, no windows)
  capabilities/default.json        — permissions
  icons/tray.png, icon.png         — placeholder icons
  src/lib.rs                       — tray setup, menu builder, polling, settings commands
  src/main.rs                      — entry point (calls lib::run)
ui/
  settings.html                    — settings window (org/repo tree, bucket reorder, gh status)
.github/workflows/
  ci.yml                           — CI: fmt + clippy + check + test
  release.yml                      — Release: macOS binary builds
```

## Known Issues / Future Work
- Bot accounts (cursor, graphite-app) appear in `latestReviews` — need filtering strategy
- Very old PRs (years) clutter results — consider staleness cutoff
- `mergeable` field unreliable on first query (GitHub computes lazily)
- Pagination beyond 50 PRs per bucket not yet implemented
- Native menu lacks rich formatting — webview popup is the path for richer UI
- macOS-only for now — Linux/Windows support would need CI matrix expansion
