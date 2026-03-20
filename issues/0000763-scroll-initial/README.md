+++
status = "open"
opened = "2026-03-20"
+++

# Issue 763: Scroll doesn't work until keyboard pane switch

## Goal

Scrolling should work on a browser overlay immediately after it opens, and after
any pane switch — whether by keyboard or mouse click.

## Background

### The problem

When a browser overlay opens, scrolling doesn't work. If the user switches to
another pane with a keyboard shortcut and switches back, scrolling starts
working. But if the user clicks to switch panes instead of using keyboard
shortcuts, scrolling remains broken.

### Root cause

The `pane.visible` flag controls whether scroll events are forwarded to browser
overlays. `try_forward_scroll_any_pane()` in `input.rs:631` filters panes by
three conditions: `tab_id != 0`, `ca_layer_host != 0`, and `p.visible`. If
`visible` is `false`, scroll events are silently dropped.

The `visible` flag is only set by `sync_overlay_visibility()` in `conn.rs:1494`,
which only runs during `WindowInvalidated` notifications.

The two pane-switching paths diverge:

- **Keyboard** (`activate_pane_direction` in `tab.rs:1439`): Calls
  `set_active_idx()` (emits `PaneFocused`) and then explicitly emits
  `WindowInvalidated` (line 1451). This triggers `sync_overlay_visibility()`,
  which sets `pane.visible = true`. Scroll works.

- **Mouse click** (`mouseevent.rs:695`): Calls `tab.set_active_idx()` (emits
  `PaneFocused`) but does NOT emit `WindowInvalidated`.
  `sync_overlay_visibility()` never runs, so `pane.visible` stays `false`.
  Scroll is dropped.

The initial open is also broken because new overlays are initialized with
`visible = false`, and the first `sync_overlay_visibility()` call only happens
when a `WindowInvalidated` notification fires.

### Fix

Call `sync_overlay_visibility()` from `handle_pane_focus()` in `input.rs:497`.
This function runs on both keyboard and mouse pane switches (it receives
`MuxNotification::PaneFocused`). Adding the visibility sync there ensures
`pane.visible` is always up to date, regardless of how the pane was activated.

For the initial open, `sync_overlay_visibility()` should also run when the
overlay is first created or when `TabReady` is handled in `conn.rs`.

### Scope

Wezboard-only change. One or two call sites in the GUI code.
