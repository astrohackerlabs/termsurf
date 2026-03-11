# Issue 739: Build warnings in Roamium and Wezboard

## Goal

Clean build with zero warnings for both Roamium and Wezboard.

## Background

`./scripts/build.sh all --release` produces 5 warnings across two crates:

### Roamium (1 warning)

1. **`ts_destroy_browser_context` never used** (`roamium/src/ffi.rs:24`) — The
   FFI declaration exists for the `libtermsurf_chromium` C function but is never
   called from Rust. The function destroys a browser context (profile). Roamium
   currently never destroys contexts — it runs one context for its lifetime and
   the OS reclaims resources on exit. The declaration should stay (it's part of
   the C API surface and will be needed for graceful profile cleanup), but needs
   an `#[allow(dead_code)]` annotation to silence the warning.

### Wezboard (4 warnings)

2. **Unused import `state::SharedState`**
   (`wezboard-gui/src/termsurf/mod.rs:14`) — `pub use state::SharedState` is
   re-exported but never imported by any code outside the `termsurf` module.
   Internal submodules import `SharedState` directly from `super::state::`. The
   re-export can be removed.

3. **Unused variable `num_panes`**
   (`wezboard-gui/src/termwindow/render/pane.rs:35`) — The `paint_pane` method
   receives `num_panes` but only uses it in the `paint_pane_opengl` path (line
   589). The `paint_pane` method itself doesn't use it — it just forwards to
   `paint_pane_box_model` or `paint_pane_opengl`. Prefix with underscore.

4. **Field `process` never read** (`wezboard-gui/src/termsurf/state.rs:36`) —
   `Server.process` stores the `Child` handle from `Command::new().spawn()` but
   is never read back. It exists to keep the `Child` alive (dropping it doesn't
   kill the process, but it's good practice to hold it for future
   `wait()`/`kill()` calls). Needs `#[allow(dead_code)]` — the field is
   intentionally stored for future use.

5. **Method `first_ns_view` never used** (`wezboard-gui/src/frontend.rs:323`) —
   A helper that extracts the `NSView` pointer from the first window. Not
   currently called. It was likely written for overlay setup but superseded by
   the current `CALayerHost` approach. Can be removed since it's unused and easy
   to recreate if needed.
