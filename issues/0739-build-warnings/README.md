+++
status = "closed"
opened = "2026-03-11"
closed = "2026-03-11"
+++

# Issue 739: Build warnings in Roamium and Wezboard

## Goal

Clean build with zero warnings for both Roamium and Wezboard.

## Background

`./scripts/build.sh all --release` produces 5 warnings across two crates:

### Roamium (1 warning)

1. **`ts_destroy_browser_context` never used** (`roamium/src/ffi.rs:24`) — The
   FFI declaration exists but is never called because Roamium has no `Shutdown`
   handler. The `handle_message` dispatch (`dispatch.rs:223`) sends `Shutdown`
   to `_ => {}`. When the GUI sends `Shutdown`, Roamium should destroy all tabs,
   call `ts_destroy_browser_context`, and exit cleanly. Fix: add a `Shutdown`
   handler that calls it, which also eliminates the dead-code warning.

Research Content Shell's shutdown path
(`content/shell/browser/shell_browser_main_parts.cc` and
`shell_main_delegate.cc`) to understand the correct teardown sequence — which
objects to destroy, in what order, and whether
`BrowserContext::ShutdownStoragePartitions()` or similar cleanup is needed
before destroying the context.

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

### Content Shell shutdown research

Content Shell's shutdown sequence: close all Shell windows (destroying their
WebContents) → `Shell::Shutdown()` auto-called on last tab destroy → message
loop exits → `PostMainMessageLoopRun` destroys the BrowserContext via
`.reset()`.

The TermSurf C library's `ts_destroy_browser_context()` is a no-op — the default
browser context is owned by `ShellBrowserMainParts` and destroyed automatically
in `PostMainMessageLoopRun`. The function was a stub for multi-context (multiple
profiles per process) support, which we've decided against — TermSurf uses one
profile per process across all engines.

Ghostboard never calls `ts_destroy_browser_context`. Neither does Roamium.
Removing it from `ffi.rs` has no effect on Ghostboard or the Chromium C library
(the symbol stays exported but harmlessly unused).

## Experiments

### Experiment 1: Remove unused FFI declaration

#### Description

Remove the `ts_destroy_browser_context` FFI declaration from Roamium. The
function is a no-op in the C library and represents a multi-context design we've
rejected. Removing the declaration eliminates the dead-code warning.

#### Changes

**`roamium/src/ffi.rs`**

Remove line 24 (`pub fn ts_destroy_browser_context(ctx: TsBrowserContext);`).

#### Verification

1. `./scripts/build.sh roamium --release`
2. Zero warnings from Roamium.

**Result:** Pass

`./scripts/build.sh roamium --release` compiles with zero warnings. The removed
declaration had no callers in Roamium or Ghostboard, and the C library symbol
remains exported but harmlessly unused.

#### Conclusion

Removed `ts_destroy_browser_context` from `roamium/src/ffi.rs`. This was a stub
for multi-context support (multiple profiles per process) that TermSurf decided
against — each engine process serves exactly one profile. The function was a
no-op in `libtermsurf_chromium` and was never called by any consumer. Roamium
now builds warning-free. Four Wezboard warnings remain for the next experiment.

### Experiment 2: Fix four Wezboard warnings

#### Description

Fix all four remaining Wezboard build warnings in one pass. Each is a small,
independent change — no logic modifications, just dead code removal and
annotation.

#### Changes

**`wezboard/wezboard-gui/src/termsurf/mod.rs`**

Remove line 14 (`pub use state::SharedState;`). The re-export is unused — all
consumers import `SharedState` directly from `super::state::` within the
`termsurf` module. Outside code accesses state via the `shared_state` function
re-export on line 13.

**`wezboard/wezboard-gui/src/termwindow/render/pane.rs`**

Prefix `num_panes` with an underscore (`_num_panes`) on line 35. The parameter
is only forwarded to `paint_pane_opengl` — `paint_pane` itself doesn't use it.

**`wezboard/wezboard-gui/src/termsurf/state.rs`**

Add `#[allow(dead_code)]` above the `process` field on line 36. The field
intentionally holds the `Child` handle to keep the Roamium process alive. It
will be read when we add graceful server shutdown.

**`wezboard/wezboard-gui/src/frontend.rs`**

Remove the `first_ns_view` method at line 323. It extracts an `NSView` pointer
from the first window but is never called — superseded by the current
`CALayerHost` overlay approach. Easy to recreate if ever needed.

#### Verification

1. `./scripts/build.sh wezboard --release`
2. Zero warnings from Wezboard.

**Result:** Pass

`./scripts/build.sh wezboard --release` compiles with zero warnings. All four
fixes are mechanical — no logic changes, no behavioral differences.

#### Conclusion

Fixed all four Wezboard warnings: removed unused `SharedState` re-export,
prefixed unused `num_panes` parameter, annotated intentionally-stored `process`
field with `#[allow(dead_code)]`, and removed unused `first_ns_view` method.

## Conclusion

Both Roamium and Wezboard now build warning-free in release mode. Five warnings
were eliminated across two experiments:

1. **Roamium** — Removed the `ts_destroy_browser_context` FFI declaration, a
   no-op stub for multi-context support that TermSurf decided against (one
   profile per process).
2. **Wezboard** — Four mechanical fixes: dead re-export removal, unused
   parameter prefixing, dead_code annotation on an intentionally-held field, and
   unused method removal.

`./scripts/build.sh all --release` now produces zero warnings.
