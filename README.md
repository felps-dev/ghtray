# GH Tray

A lightweight, native macOS system tray app that keeps you informed about your GitHub pull request activity — without ever opening a browser.

Think [Graphite Bar](https://graphite.dev), but open-source and powered by the GitHub CLI (`gh`).

<!-- Screenshot of tray menu with PRs grouped by bucket -->
![GH Tray in action](docs/images/screenshot-tray.png)

---

## Features

- **Lives in your menu bar.** No dock icon. No main window. Just a tray icon with a badge count.
- **PR lifecycle awareness.** PRs are categorized into smart buckets:
  - Needs Your Review
  - Changes Requested on Yours
  - Approved, Ready to Merge
  - CI Failing
  - Waiting for Review
  - Reviewed by Others
  - Drafts
- **Round author avatars** in the tray menu for quick identification.
- **Native notifications** when PR states change (new review requests, approvals, CI failures).
- **Configurable polling** — set your own interval (default: 60s).
- **Repo filtering** — block-list repos you don't care about via a settings UI with org/repo tree.
- **Bucket visibility** — hide sections you don't need.
- **Launch at login** — starts silently in the background.
- **Lightweight** — small Rust binary, minimal resource usage.

<!-- Screenshot of settings window with org/repo tree -->
![Settings window](docs/images/screenshot-settings.png)

---

## How It Works

GH Tray uses the GitHub CLI (`gh`) to run GraphQL queries against the GitHub API. It categorizes your PRs by their lifecycle state and surfaces them in a native macOS menu. No OAuth tokens to manage — it piggybacks on your existing `gh auth` session.

```
Tray Icon  →  Badge Count (unread PRs)
    ↓
Click      →  Native Menu (PRs grouped by bucket)
    ↓
Click PR   →  Opens in browser
```

---

## Installation

### From GitHub Releases (recommended)

1. Go to [Releases](https://github.com/felps-dev/ghtray/releases)
2. Download the `.dmg` for your architecture:
   - **Apple Silicon** (M1/M2/M3/M4): `GH Tray_x.x.x_aarch64.dmg`
   - **Intel**: `GH Tray_x.x.x_x64.dmg`
3. Open the `.dmg` and drag **GH Tray** to your Applications folder

> [!IMPORTANT]
> **macOS Gatekeeper warning:** Since GH Tray is not notarized by Apple, macOS may block it from running. To fix this, run:
> ```bash
> xattr -dr com.apple.quarantine '/Applications/GH Tray.app'
> ```
> This removes the quarantine flag added by macOS when downloading apps from the internet. Only do this if you trust the source (i.e., you downloaded from this repository's releases page or built it yourself).

### Build from source

**Prerequisites:**
- [Rust](https://rustup.rs/) (stable, edition 2024)
- [GitHub CLI](https://cli.github.com/) (`gh`) — installed and authenticated (`gh auth login`)

```bash
# Clone the repository
git clone https://github.com/felps-dev/ghtray.git
cd ghtray

# Build
cargo build --release

# The binary is at target/release/ghtray
# Or build the .app bundle:
cargo install tauri-cli --version "^2"
cargo tauri build
```

---

## Prerequisites

GH Tray requires the [GitHub CLI](https://cli.github.com/) to be installed and authenticated:

```bash
# Install gh (if you haven't already)
brew install gh

# Authenticate
gh auth login
```

GH Tray will detect if `gh` is missing or unauthenticated and guide you through setup.

---

## Usage

1. Launch **GH Tray** — it appears as an icon in your menu bar
2. The badge count shows how many PRs need your attention
3. Click the tray icon to see PRs grouped by lifecycle state
4. Click any PR to open it in your browser
5. Right-click or use the menu to access **Settings** or **Refresh**

### Settings

Access settings from the tray menu. You can configure:

- **Poll interval** — how often to check GitHub (default: 60 seconds)
- **Blocked repos** — toggle off repos/orgs you want to ignore
- **Hidden buckets** — hide PR categories you don't care about
- **Launch at login** — start GH Tray automatically on boot

---

## Architecture

```
ghtray/
├── crates/ghtray-core/     # Core library (models, GitHub API, config, state)
│   └── src/
│       ├── models.rs        # GraphQL types, Bucket enum, CategorizedPr
│       ├── github.rs        # Fetch, categorize, filter, diff
│       ├── config.rs        # AppConfig (poll interval, blocked repos)
│       └── state.rs         # State persistence
├── src-tauri/               # Tauri app (tray, menu, polling, commands)
│   └── src/
│       ├── lib.rs           # Tray setup, menu builder, polling
│       └── main.rs          # Entry point
└── ui/
    └── settings.html        # Settings window (vanilla HTML/CSS/JS)
```

**Key design decisions:**
- Single GraphQL query with 4 aliased searches (only 5 rate limit points)
- `latestReviews` for deduplicated per-reviewer state
- Dedup PRs by node ID across search results
- `@me` in search queries (no username needed)
- Block-list for repos (new repos show by default)

---

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes
4. Push to the branch
5. Open a Pull Request

---

## License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.

---

## Acknowledgments

- Inspired by [Graphite Bar](https://graphite.dev)
- Built with [Tauri v2](https://v2.tauri.app/) and [Rust](https://www.rust-lang.org/)
- Powered by the [GitHub CLI](https://cli.github.com/)
