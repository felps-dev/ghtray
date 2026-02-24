# GH Tray — Progress

## Current Phase
v0.2.1 — Released

## Next Step
Open for next iteration — see Known Issues / Future Work below

## Phase 1: Data Exploration — COMPLETE
- [x] All tasks complete. See `docs/phase1-data-exploration.md`

## Phase 2: Proof of Concept — COMPLETE
- [x] All tasks complete. Core logic validated against real GitHub data (40 PRs, 7 buckets)

### Lessons Learned
- `latestReviews` is deduplicated per reviewer — much better than `reviews`
- `reviewDecision` can be null in repos without branch protection
- Combined query costs only 5 points despite 4 aliased searches
- Fetch time ~4.5s (network latency). Dedup by PR node ID is necessary

## Phase 3: Final App — COMPLETE
- [x] 3.1 Tauri v2 workspace setup (`crates/ghtray-core` lib + `src-tauri` app)
- [x] 3.2 System tray & badge count (via `tray.set_title`)
- [x] 3.3 Native OS tray menu (replaced webview popup per user feedback)
- [x] 3.4 Settings window (poll interval + org/repo tree filter)
- [x] 3.5 Notifications & sound
- [x] 3.6 Startup & autostart
- [x] 3.7 Error handling & resilience
- [x] 3.8 Polish & cleanup

### Decisions & Deviations from IDEA.md
- **Native menu instead of webview popup** (IDEA.md §3.3): User preferred native OS menu
- **Single instance** via `tauri-plugin-single-instance` (user request, not in original spec)
- **No dock icon** via `set_activation_policy(Accessory)` in setup
- **Settings close hides window** instead of killing app (`RunEvent::WindowEvent` intercept)
- **Repo filter uses block-list** (`blocked_repos`): new repos appear by default
- **Org/repo tree in settings**: orgs are toggleable parents, individual repos are children
- **Bucket visibility**: users can hide/show individual sections via `hidden_buckets` in settings
- **Round author avatars**: downloaded from GitHub, converted to circular PNG via `image` crate
- **Relative time**: PR age shown as compact format (2m, 4h, 3d, 2mo, 1y)
- **Sorted by recency**: PRs within each bucket sorted by `updated_at` descending
- **gh CLI path resolution**: searches common paths (/opt/homebrew/bin, /usr/local/bin, etc.) so bundled .app works
- **Loading indicator**: badge shows ↻ during fetch
- **Error banner**: tray shows ⚠ and menu shows error message when gh/API fails
- **Logging**: errors written to `~/Library/Application Support/ghtray/ghtray.log`

### 3.6 — Startup & Autostart
- `tauri-plugin-autostart` — "Launch at login" toggle in settings
- Startup check: detects if `gh` is missing or unauthenticated, shows settings with error
- Starts silently in tray on subsequent launches

### 3.7 — Error Handling & Resilience
- Network/API failures show stale data with ⚠ badge and error banner in menu
- `gh` not found → clear message in menu + opens settings
- `gh` not authenticated → clear message + opens settings
- All errors logged to `ghtray.log` with timestamps (auto-truncated at 100KB)
- Malformed API responses handled via Result types (no panics)
- Fetch failure restores previous badge (no stuck loading indicator)

### 3.8 — Polish & Cleanup
- Removed unused `ui/index.html` (leftover from webview popup approach)
- "Refresh Now" runs on background thread (doesn't block menu)
- Settings window enlarged for new sections (640px height)

## v0.2.1 — Improvements
- [x] Configurable badge count — users select which sections count towards tray badge
- [x] Drag & drop section reorder in settings (persisted to `bucket_order`)
- [x] gh CLI status shown in settings with "Try Again" button
- [x] Tray icon shows ✗ when gh is missing/unauthenticated
- [x] Notification sound fix — uses macOS `afplay` instead of unreliable plugin
- [x] Refactored save_settings to payload struct
- [x] Public repo, LICENSE (MIT), README, CI/CD workflows
- [x] GitHub Actions: ci.yml (fmt/clippy/check/test) + release.yml (macOS binaries)

## Known Issues / Future Work
- Bot accounts (cursor, graphite-app) appear in `latestReviews` — need filtering strategy
- Very old PRs (years) clutter results — consider staleness cutoff
- `mergeable` field unreliable on first query (GitHub computes lazily)
- Pagination beyond 50 PRs per bucket not yet implemented
- Native menu lacks rich formatting (colors, custom layout) — webview popup is the path forward
- macOS-only — Linux/Windows support would need CI matrix expansion
