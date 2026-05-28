# GH Tray — Progress

## Current Phase
v0.5.1 — Defensive guard for blocked-repo notifications

## Next Step
Test on real account that notifications/sounds stay silent for unchecked repos.

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

## v0.3.0 — Improvements
- [x] Configurable max PR age — hide PRs older than X days (0 = no limit)
- [x] Configurable gh CLI path — user can override auto-detected path in settings

## v0.4.0 — Resilience & UX
- [x] Persist `all_prs` to state file; restore into tray at startup so cached PRs show even if first fetch fails
- [x] Persistent notification ledger keyed on `(bucket, commit_sha)`; only updated when a notification fires. Fixes the GitHub-outage repeat-notify bug — flickering data (partial GraphQL responses, transient null `reviewDecision`) no longer causes "PR Approved" to fire on every successful fetch
- [x] `notified_seeded` flag silences first-fetch / upgrade-time notification flood
- [x] LRU cap on the notified ledger (1000 entries, oldest by `recorded_at` evicted)
- [x] Detect `errors[]` in GraphQL JSON response — surfaces partial-failure messages that previously slipped through serde
- [x] Tray menu error banner is clickable → opens `ghtray.log`
- [x] Tray tooltip shows actual error text (not generic "Error (check settings)")
- [x] Bucket section headers are clickable → opens every PR in that bucket in the browser
- [x] Settings Cancel button now uses backend `hide_settings` command (JS `getCurrentWindow().hide()` was failing silently)

## v0.5.1 — Blocked-repo notification guard
- [x] Belt-and-suspenders check in `send_notifications`: explicit `config.is_repo_allowed(&note.repo)` guard ensures a desktop notification (and its sound) never fires for a repo the user has unchecked in settings, even if upstream filtering ever regresses. `PendingNotification` gained a `repo` field to support the check without a PR lookup.

## v0.5.0 — Notifications & Settings UX
- [x] Custom notification sound — pick any macOS system sound (Glass, Pop, Tink, Bottle, Frog, Funk, Hero, Morse, Ping, Purr, Sosumi, Submarine) or a custom file path. `Preview` button plays via `afplay`. New `preview_sound` Tauri command, `notification_sound_path` in config.
- [x] Per-event notification & sound toggles — `notify_buckets` + `sound_buckets` HashSets in config. Master toggles still gate everything; the matrix lets a user e.g. silence the "Approved" sound while keeping the popup. Filtering happens in `send_notifications`, not `pending_notifications`, so the dedup ledger stays consistent across toggle changes.
- [x] Last-notified indicator per PR row — `NotificationKey` gained a `notified_at` field (separate from `recorded_at`, which keeps its LRU role). Set only when a desktop notification actually fires. `CategorizedPr` carries `last_notified_at` populated by `enrich_with_notified` after each fetch and on startup restore. Renders as ` · 🔔 5m` suffix on the tray row.
- [x] Per-bucket sort — `bucket_sort: HashMap<String, String>` in config. Sort keys: `updated_desc/asc`, `notified_desc/asc`, `created_desc/asc`, `number_desc/asc`. `None` values sink regardless of direction. New `github::sort_prs` helper replaces the hardcoded `updated_at`-desc sort in `rebuild_tray_menu`.
- [x] Settings webview redesign — sidebar nav (General / Polling / Notifications / Sections / Repositories / About), 780×640 window. Refined dark palette, monospace numeric nav labels, terminal-style section slugs. Bucket rows in `Sections` gain a sort dropdown next to visibility + badge. Notifications tab hosts master toggles, sound picker + custom path + Preview, and a per-event Notify/Sound matrix limited to notifiable buckets (`Bucket::notification_title().is_some()`).

### Architecture Decisions
- **Stayed on text-only menu items** — Tauri's safe API exposes `MenuItem`/`IconMenuItem`/`PredefinedMenuItem`/`Submenu`/`CheckMenuItem` with text, icon, accelerator, and check state. No `attributedTitle`, `NSMenuItem.view`, or tooltip pass-through. Going AppKit-direct via `objc2` for one timestamp per row would mean rewriting tray construction in `unsafe` — bad ROI. The "🔔 5m" suffix lives inside the existing label string.
- **Two-timestamp ledger** — `recorded_at` (always bumped on bucket transition, used for LRU + dedup) and `notified_at` (only bumped when a real notification fires). Lets muted-bucket transitions stay tracked for dedup without polluting the "last notified" display.
- **`PendingNotification.bucket`** added so `send_notifications` can apply the per-bucket gate without re-deriving from the title string.

## Known Issues / Future Work
- Bot accounts (cursor, graphite-app) appear in `latestReviews` — need filtering strategy
- `mergeable` field unreliable on first query (GitHub computes lazily)
- Pagination beyond 50 PRs per bucket not yet implemented
- Native menu lacks rich formatting (colors, custom layout) — webview popup is the path forward
- macOS-only — Linux/Windows support would need CI matrix expansion
