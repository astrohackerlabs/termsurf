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

## Experiments

### Experiment 1: Call sync_overlay_visibility from PaneFocused handler

#### Description

Add a `sync_overlay_visibility()` call to the `PaneFocused` notification handler
in `termwindow/mod.rs`. This is the same place that already calls
`handle_pane_focus()`. Since `PaneFocused` fires on both keyboard and mouse pane
switches, this ensures visibility is always synced.

This also fixes the initial open: `PaneFocused` fires when the first pane is
activated after creation.

#### Changes

**1. Wezboard: `wezboard-gui/src/termwindow/mod.rs`**

In the `PaneFocused` handler (~line 1352), add the active-pane-ID gathering and
`sync_overlay_visibility()` call — the same pattern used in the
`WindowInvalidated` handler at lines 1331-1341:

```rust
MuxNotification::PaneFocused(pane_id) => {
    crate::termsurf::input::handle_pane_focus(pane_id);

    // Sync overlay visibility so pane.visible is correct
    // for scroll forwarding (Issue 763).
    let mut active_ids = std::collections::HashSet::new();
    for window_id in mux.iter_windows() {
        if let Some(w) = mux.get_window(window_id) {
            if let Some(tab) = w.get_active() {
                for positioned in tab.iter_panes() {
                    active_ids.insert(positioned.pane.pane_id().to_string());
                }
            }
        }
    }
    crate::termsurf::conn::sync_overlay_visibility(&active_ids);

    // Also handled by clientpane
    self.update_title_post_status();
}
```

This duplicates the active-ID gathering from the `WindowInvalidated` handler. A
future cleanup could extract this into a helper, but for now the duplication is
minimal and keeps the fix contained.

#### Verification

```bash
scripts/build.sh wezboard
```

| #   | Test                           | Steps                                            | Expected                 |
| --- | ------------------------------ | ------------------------------------------------ | ------------------------ |
| 1   | Scroll works on first open     | Open `web localhost:9616`, scroll on a long page | Page scrolls immediately |
| 2   | Scroll works after kb switch   | Switch pane with keyboard, switch back, scroll   | Page scrolls             |
| 3   | Scroll works after mouse click | Click another pane, click back, scroll           | Page scrolls             |
| 4   | Hidden tabs don't scroll       | Open two tabs, scroll on visible tab             | Only visible tab scrolls |

**Result:** Fail

Test 1 fails — scrolling still doesn't work on first open. Tests 2-3 were not
tested because Test 1 is the primary symptom.

#### Conclusion

The fix addresses pane switching but not the initial open. The timing is wrong:
`PaneFocused` fires when the terminal pane is created, _before_ the TUI sends
`SetOverlay` and the browser overlay is added to `st.panes`. By the time the
overlay exists, `sync_overlay_visibility` has already run and missed it.

The sequence on initial open:

1. Pane created → `PaneFocused` fires → `sync_overlay_visibility` runs → overlay
   doesn't exist yet in `st.panes` → nothing to set visible
2. TUI sends `SetOverlay` → overlay created in `st.panes` with `visible = false`
3. `TabReady` comes back → `BrowserReady` sent to TUI
4. No further `sync_overlay_visibility` call → `visible` stays `false`

The next experiment should set `visible = true` when the overlay is first
created — either in `handle_tab_ready` or when `SetOverlay` adds the pane. The
`PaneFocused` change from this experiment is still useful for the mouse-click
pane switching case and should be kept.
