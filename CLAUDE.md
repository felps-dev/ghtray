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

## Building & Running
```
cargo build                    # build workspace
target/debug/ghtray            # user runs manually to test
```

## Project Structure
```
IDEA.md                            — spec (source of truth)
PROGRESS.md                        — progress tracker (read first, update last)
docs/phase1-data-exploration.md    — API findings
Cargo.toml                         — workspace root
crates/ghtray-core/
  src/lib.rs                       — pub modules
  src/models.rs                    — GraphQL types, Bucket enum, CategorizedPr
  src/github.rs                    — fetch, categorize, filter, diff
  src/config.rs                    — AppConfig (poll interval, blocked_repos)
  src/state.rs                     — AppState persistence
src-tauri/
  Cargo.toml                       — tauri app deps
  tauri.conf.json                  — app config (tray icon, no windows)
  capabilities/default.json        — permissions
  icons/tray.png, icon.png         — placeholder icons
  src/lib.rs                       — tray setup, menu builder, polling, settings commands
  src/main.rs                      — entry point (calls lib::run)
ui/
  settings.html                    — settings window (org/repo tree + poll config)
  index.html                       — UNUSED (leftover from webview popup)
```
