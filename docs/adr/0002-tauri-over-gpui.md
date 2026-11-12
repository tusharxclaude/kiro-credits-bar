---
status: accepted
---

# Build the tray app with Tauri, not GPUI

The app is a menu bar / system tray widget with a small details panel, and it must run on macOS, Windows and Linux. We chose Tauri (Rust backend, web UI) because tray support is mature on all three platforms. GPUI, Zed's pure-Rust framework, is lighter, but its tray and menu bar support is thin and its Linux and Windows support is newer.

## Consequences

The UI is web technology rather than pure Rust, which costs more memory than GPUI. If GPUI's tray support matures, switching is a rewrite of the UI layer, not the core logic, which should stay in plain Rust, independent of the UI.
