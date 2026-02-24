# 🔔 GH Tray — Product & Engineering Specification

> A lightweight, native system tray application that keeps developers informed about their GitHub pull request activity without ever opening a browser. Think Graphite Bar, but open-source and powered by the `gh` CLI.

---

## 1. Project Idea

### The Problem

Developers working on active codebases are constantly context-switching between their editor and GitHub to check on pull request status. "Do I have anything to review?" "Did anyone approve my PR?" "Did that PR I reviewed get new commits?" These micro-interruptions add up and break flow.

Tools like Graphite Bar solve this beautifully — a persistent, glanceable tray icon with a badge count and a dropdown showing every PR that matters to you, categorized by where it sits in its lifecycle. But Graphite is a proprietary tool tied to its own ecosystem.

### The Solution

**GH Tray** is a free, open-source alternative that delivers the same experience using only the GitHub CLI (`gh`) as its data source. It runs as a system tray application built with Rust and Tauri v2, starts on boot, polls GitHub on a configurable interval, and surfaces PR activity through a badge count and a popup panel — no browser required.

### Core Principles

- **Invisible until needed.** No main window. No dock icon. Just a tray icon with a badge.
- **Accurate and timely.** The badge count should always reflect reality within one polling interval.
- **Lifecycle-aware.** PRs aren't just "open" or "closed." They move through a nuanced lifecycle (needs review → reviewed → new commits → re-review needed), and the app should track these transitions.
- **Configurable, not complicated.** Sane defaults out of the box, with a settings window for those who want control.
- **Lightweight.** Under 50MB memory, under 2 seconds to tray-ready on boot.

---

## 2. Project Architecture

### Technology Stack

| Layer | Technology | Rationale |
|---|---|---|
| Runtime | Tauri v2 | Native system tray support, small binary size, cross-platform |
| Backend | Rust | Performance, safety, direct system integration |
| Frontend | HTML/CSS/JS (or lightweight framework) | Minimal popup UI, settings window |
| Data Source | `gh` CLI (GraphQL via `gh api graphql`) | No OAuth token management, leverages user's existing auth |
| Local State | JSON file (or SQLite for scale) | Persist PR state between polls for transition detection |
| Notifications | Native OS notifications + optional sound | Non-intrusive alerting |

### System Overview

```
┌─────────────────────────────────────────────────────┐
│                    Operating System                  │
│                                                     │
│  ┌──────────┐    ┌──────────────────────────────┐   │
│  │ System   │    │         GH Tray (Tauri)       │   │
│  │ Tray     │◄───│                               │   │
│  │ [icon+   │    │  ┌─────────────────────────┐  │   │
│  │  badge]  │    │  │    Polling Engine        │  │   │
│  └────┬─────┘    │  │  (configurable interval) │  │   │
│       │          │  └───────────┬──────────────┘  │   │
│       ▼          │              │                  │   │
│  ┌──────────┐    │              ▼                  │   │
│  │ Popup    │    │  ┌─────────────────────────┐   │   │
│  │ Panel    │    │  │    gh CLI (subprocess)   │   │   │
│  │ (webview)│    │  │  GraphQL queries via     │   │   │
│  └──────────┘    │  │  `gh api graphql`        │   │   │
│                  │  └───────────┬──────────────┘   │   │
│  ┌──────────┐    │              │                  │   │
│  │ Settings │    │              ▼                  │   │
│  │ Window   │    │  ┌─────────────────────────┐   │   │
│  │ (webview)│    │  │   State Manager          │   │   │
│  └──────────┘    │  │  - Categorize PRs        │   │   │
│                  │  │  - Diff against cache     │   │   │
│                  │  │  - Emit transitions       │   │   │
│                  │  └───────────┬──────────────┘   │   │
│                  │              │                  │   │
│                  │              ▼                  │   │
│                  │  ┌─────────────────────────┐   │   │
│                  │  │   Local State File       │   │   │
│                  │  │   (JSON / SQLite)        │   │   │
│                  │  └─────────────────────────┘   │   │
│                  └──────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

### PR Lifecycle & State Model

Every PR the user cares about is categorized into exactly one bucket:

| Bucket | Description | Detection Logic |
|---|---|---|
| **Needs Your Review** | Someone requested your review and you haven't submitted one, OR you reviewed but new commits landed since, OR review was re-requested | Review request exists + (no review submitted, or last commit > last review, or re-request event) |
| **Waiting for Reviewers** | Your PRs awaiting review from others | Author is you + open + not draft + pending/no reviews |
| **Returned to You** | Your PRs where changes were requested or comments need response | Author is you + changes_requested review state |
| **Approved** | Your PRs approved and ready to merge | Author is you + approved review state |
| **Drafts** | Your open draft PRs | Author is you + draft = true |
| **Recently Merged** | PRs merged within configurable time window | Merged state + merged_at within window |
| **Waiting for Author** | PRs you reviewed where author hasn't responded | You submitted review + changes_requested + no new commits since your review |

### Transition Detection

The app maintains a snapshot mapping each PR ID to its current bucket, last commit SHA, last review timestamp, and review request timestamps. On every poll:

1. Fetch fresh data from GitHub.
2. Categorize every PR into its bucket.
3. Compare against the cached snapshot.
4. For every PR that changed buckets, emit a transition event.
5. Update the cache.

Transition events drive badge updates, notifications, and sound effects.

### Data Flow Per Poll Cycle

1. Timer fires → invoke `gh api graphql` with a single comprehensive query.
2. Parse JSON response, extract all PRs where user is involved.
3. For each PR, evaluate categorization rules and assign to a bucket.
4. Load previous state from local file.
5. Diff: identify new PRs, removed PRs, and bucket changes.
6. Update tray badge count based on user's configured categories.
7. Fire notifications for meaningful transitions (if enabled).
8. Save new state to local file.
9. Push updated data to frontend (popup will render fresh on next open).

---

## 3. Project Step-by-Step

### Phase 1: Data Exploration

**Goal:** Achieve complete understanding of the GitHub data layer before writing any application code.

**What to do:**

- [ ] Spend dedicated time in the terminal with `gh` exploring PR data.
- [ ] Map out which GraphQL fields and connections are needed to categorize PRs into every bucket.
- [ ] Test queries against real-world scenarios:
  - A PR with multiple reviewers at different states.
  - A PR you reviewed that received new commits after your review.
  - A PR where review was re-requested after you submitted.
  - A draft PR converted to ready for review.
  - A PR across a fork.
  - A PR with CI failures (does this affect state?).
  - A PR in a repo you don't own but were requested to review.
- [ ] Determine if a single GraphQL query can fetch everything or if multiple are needed.
- [ ] Document the exact query (or queries), response shapes, field meanings, and any API quirks.
- [ ] Identify rate limit costs per query and calculate sustainable polling intervals.
- [ ] Test edge cases: What happens when `gh` is not authenticated? When network is down? When a repo is archived?

**Output:** A "Discoveries" section in `PROGRESS.md` with all queries, response examples, edge cases, and decisions.

---

### Phase 2: Proof of Concept (CLI)

**Goal:** Validate the entire data pipeline — fetching, parsing, categorization, state diffing, and transition detection — in a throwaway CLI tool with zero UI complexity.

**What to do:**

- [ ] Create a simple Rust CLI binary (no Tauri, no UI).
- [ ] Implement `gh` subprocess calls using the queries designed in Phase 1.
- [ ] Parse GraphQL JSON responses into Rust structs (using serde).
- [ ] Implement the categorization engine — rules that assign each PR to exactly one bucket.
- [ ] Print a formatted terminal summary mirroring the planned popup layout:
  ```
  Needs Your Review (3)
    #10282 - Project/ai campaign message template assistant (repo-name)
    #9408  - Dealervault processed date tracking (repo-name)
    ...
  Waiting for Reviewers (4)
    ...
  ```
- [ ] Implement local state persistence — save current state to a JSON file after each run.
- [ ] Implement state diffing — on subsequent runs, load previous state, compare, and print transitions:
  ```
  [TRANSITION] #9408 moved from "Needs Your Review" → "Waiting for Author"
  [NEW] #10500 appeared in "Needs Your Review"
  ```
- [ ] Run on a loop with a configurable interval to simulate polling.
- [ ] Stress-test with your real GitHub activity. Validate that every PR lands in the correct bucket.
- [ ] Iterate on categorization rules until they match your expectations perfectly.

**Output:** A working CLI tool and a refined, battle-tested core logic module. Update `PROGRESS.md` with any rule adjustments, bugs found, and lessons learned.

---

### Phase 3: Build the Final App

**Goal:** Wrap the proven core logic in a polished Tauri v2 system tray application.

#### 3.1 — Project Setup
- [ ] Initialize a Tauri v2 project.
- [ ] Extract the PoC core logic (fetching, parsing, categorization, diffing) into a clean Rust module/library.
- [ ] Set up the project structure separating core logic, tray management, and frontend.

#### 3.2 — System Tray & Badge
- [ ] Register a system tray icon with Tauri's tray API.
- [ ] Implement dynamic badge count rendering on the icon.
- [ ] Wire up the polling engine to run on a background thread/task using the configured interval.
- [ ] Update the badge count on every poll cycle based on the user's selected categories.
- [ ] Implement icon color/state changes (e.g., red = attention needed, green = all clear, grey = offline/stale).

#### 3.3 — Tray Popup
- [ ] Build the popup panel UI (HTML/CSS/JS) — dark theme, compact, collapsible sections.
- [ ] Implement Tauri commands to expose PR data from backend to frontend.
- [ ] Render PR categories as collapsible sections with counts in the section headers.
- [ ] Each PR row: number, title, repo name, small status indicator (e.g., "new commits" badge).
- [ ] Clicking a PR row opens the URL in the default browser.
- [ ] Show last fetch timestamp and a manual "Refresh" button.
- [ ] Show a "Configure" link that opens the settings window.
- [ ] Pre-render popup content from cached state so it opens instantly.

#### 3.4 — Settings Window
- [ ] Build a separate Tauri window for settings.
- [ ] **Authentication section:**
  - Display `gh auth status` — logged-in user, avatar, scopes.
  - If unauthenticated, show a button to trigger `gh auth login`.
  - Handle auth state changes gracefully.
- [ ] **Polling section:**
  - Numeric input + unit selector (seconds, minutes, hours).
  - Enforce a minimum floor (e.g., 30 seconds) to avoid rate limiting.
  - Changes take effect immediately without app restart.
- [ ] **Badge configuration section:**
  - Checkboxes for each PR category.
  - Live preview of what the badge count would be.
- [ ] **Notification section:**
  - Toggle sound effects on/off.
  - Configure which transitions trigger sound/notification.
  - Option for native OS notifications.
  - Sound preview button.
- [ ] **Display section:**
  - Toggle which sections appear in the popup.
  - Reorder sections.
  - Configure "recently merged" time window.
- [ ] Persist all settings to a local config file (JSON or TOML).

#### 3.5 — Notifications & Sound
- [ ] Implement native OS notifications for PR transitions (using Tauri's notification API).
- [ ] Bundle a subtle notification sound.
- [ ] Play sound on configurable transition events.
- [ ] Respect the user's notification preferences from settings.
- [ ] Implement "do not disturb" awareness if possible on the platform.

#### 3.6 — Startup & Autostart
- [ ] Register the app as a login item / autostart on the OS.
- [ ] On first launch, detect if `gh` is authenticated:
  - If yes: start silently in the tray, do first fetch.
  - If no: open the settings window with the authentication section highlighted.
- [ ] On subsequent launches, start silently — no window, just tray icon.
- [ ] Handle the case where `gh` CLI is not installed — show a helpful message directing the user to install it.

#### 3.7 — Error Handling & Resilience
- [ ] Handle network failures gracefully — show stale data with a visual indicator.
- [ ] Handle `gh` auth expiry — detect and prompt re-authentication.
- [ ] Handle `gh` CLI not found — clear error message on first launch.
- [ ] Handle GitHub API rate limiting — detect, back off, show remaining budget in settings.
- [ ] Handle malformed API responses without crashing.
- [ ] Log errors to a local file for debugging.

#### 3.8 — Polish & Testing
- [ ] Test on macOS (menu bar behavior), Windows (system tray), and Linux (varies by DE).
- [ ] Ensure popup positioning is correct on each platform, including multi-monitor setups.
- [ ] Profile memory usage — target under 50MB.
- [ ] Profile startup time — target under 2 seconds to tray-ready.
- [ ] Test with large PR volumes (100+ open PRs across multiple repos).
- [ ] Test with edge cases: no open PRs, hundreds of repos, restricted repos, archived repos.

---

### Progress Tracking

Maintain a **`PROGRESS.md`** file at the repository root throughout all phases. Structure:

```markdown
# GH Tray — Progress

## Current Phase
Phase X — [description of current focus]

## Next Step
[The single immediate next action to take]

## Phase 1: Data Exploration
- [x] Completed task
- [ ] Pending task

### Discoveries
- [Query findings, API quirks, decisions made and why]

## Phase 2: Proof of Concept
- [ ] Task list

### Lessons Learned
- [Bugs found, rule adjustments, what surprised us]

## Phase 3: Final App
- [ ] 3.1 Project Setup
- [ ] 3.2 System Tray & Badge
- [ ] ...

## Known Issues / Open Questions
- [Carries forward across phases]
```

Update this file as part of the workflow, not as an afterthought. Every work session should start by reading it and end by updating it.

---

## 4. Final Considerations

### `gh` CLI Dependency

The app depends on `gh` being installed and authenticated on the user's machine. This is a deliberate trade-off: it avoids the complexity of managing OAuth tokens, refresh flows, and credential storage, but it means the user must have `gh` installed. The onboarding flow should make this painless — detect if `gh` exists, guide installation if not, and trigger auth if needed. In a future version, direct GitHub OAuth could be an option for users who don't want to install `gh`.

### Rate Limiting

GitHub's GraphQL API allows 5,000 points per hour. A well-crafted query fetching all relevant PRs should cost roughly 1-5 points depending on complexity. At a 2-minute polling interval, that's 30 queries per hour — well within budget. However, the app should still track the `X-RateLimit-Remaining` header (exposed through `gh` output) and degrade gracefully if the user has other tooling consuming their budget.

### Cross-Platform Tray Behavior

System tray behavior differs significantly across platforms. On macOS, it's a menu bar item with potential for a proper popover. On Windows, it's a notification area icon. On Linux, it depends on the desktop environment (GNOME, KDE, etc.) and may require additional libraries. Tauri v2 abstracts much of this, but popup positioning and interaction patterns will need platform-specific attention.

### Security

The app never handles GitHub credentials directly — `gh` manages all authentication. Local state files containing PR metadata (titles, numbers, URLs) should be stored in the OS-appropriate application data directory with default filesystem permissions. No sensitive data (tokens, passwords) is ever stored by the app.

### Multi-Account / Multi-Org

The `gh` CLI supports multiple authenticated accounts. For V1, the app should work with the default authenticated account. Supporting account switching could be a future enhancement.

### Performance Budget

The app should be imperceptible when idle. Target metrics:

| Metric | Target |
|---|---|
| Memory (idle) | < 50 MB |
| CPU (idle, between polls) | ~0% |
| Startup to tray-ready | < 2 seconds |
| Popup open to rendered | < 100ms |
| Single poll cycle duration | < 3 seconds |

---

## 5. Future Cool Improvements

These are explicitly out of scope for V1 but represent the natural evolution of the product.

**Quick Actions from Popup** — Approve, request changes, or merge a PR directly from the tray popup without opening a browser. A small action menu on each PR row with confirm dialogs to prevent accidents.

**Keyboard Shortcut** — Global hotkey (e.g., `Cmd+Shift+G`) to toggle the popup open/closed, making it even faster than clicking the tray icon.

**GitHub Actions / CI Status** — Show CI status alongside each PR (green check, red X, yellow spinner). This transforms the tool from "PR status" to "PR readiness" — you can see at a glance which PRs are ready to merge vs. which have failing checks.

**Team View** — A mode that shows PR activity for your entire team, not just PRs you're involved in. Useful for tech leads and managers who need to keep an eye on team throughput and review bottlenecks.

**Direct GitHub OAuth** — Remove the `gh` CLI dependency entirely by implementing OAuth device flow directly in the app. This makes installation simpler (one binary, no dependencies) and could enable features not available through `gh`.

**Custom Filters & Saved Views** — Let users create custom PR queries (e.g., "all PRs in repo X with label 'urgent'") and pin them as additional sections in the popup.

**Review Reminders** — If a PR has been sitting in "Needs Your Review" for more than N hours, escalate the notification — change the badge color, send a reminder notification, etc.

**PR Statistics & Insights** — Track review turnaround times, time-to-merge, and other metrics over time. Surface a small dashboard in the settings or a dedicated window.

**Slack / Discord Integration** — Optionally post to a Slack or Discord channel when specific transitions happen (e.g., "PR #1234 was approved and is ready to merge").

**Theme Customization** — Let users choose between dark/light themes or customize colors to match their setup.

**Multi-Account Support** — Switch between GitHub accounts (personal, work, client) from the settings window, each with their own polling and notification preferences.

**Conflict Detection** — Flag PRs that have merge conflicts, adding a visual indicator so you know before clicking into the browser.

**Stacked PRs Awareness** — If using a stacking workflow (like Graphite), detect PR chains and display them as a tree rather than flat list, showing which PRs are blocked by others in the stack.

---

*This document is the single source of truth for the GH Tray project. Keep it updated as decisions are made and scope evolves.*
