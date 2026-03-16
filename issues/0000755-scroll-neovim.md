# Issue 755: Scroll broken in neovim when webviews are open

## Goal

Mouse scroll works in fullscreen TUIs like neovim at all times — whether browser
overlays are open or not.

## Background

### The bug

Scrolling with the Apple Magic Mouse in neovim does not work in Wezboard when
any browser overlay is open. It works fine in three other cases:

1. Scrolling in neovim in **WezTerm** (upstream) — works
2. Scrolling in neovim in **Wezboard with no webviews open** — works
3. Scrolling in neovim in **Wezboard with webviews open** — broken

The mere presence of a browser overlay somewhere in the window breaks scroll
event delivery to fullscreen TUIs, even when the cursor is not over the overlay.

### What we changed

Issue 731 added `RawScrollEvent` to the window layer (`window.rs`) to forward
scroll phase data to browser overlays. This event is dispatched before the
normal `VertWheel`/`HorzWheel` mouse events that drive terminal scrolling.
WezTerm does not have `RawScrollEvent` at all.

Issue 752 changed the scroll handler to iterate all overlay panes
(`try_forward_scroll_any_pane`) instead of only the active pane. The handler
sets `raw_scroll_consumed` based on whether any overlay consumed the scroll. A
flag in `mouseevent.rs` checks `raw_scroll_consumed` and suppresses the
duplicate wheel event if the raw scroll was already forwarded to a browser.

### What needs to happen

Find why the presence of browser overlays interferes with scroll event delivery
to terminal panes and fix it. The scroll path must work correctly whether zero,
one, or many browser overlays are open.

## Experiments

### Experiment 1: Debug logging to diagnose the hit test

#### Description

Add `log::info!` lines to the scroll forwarding path to see exactly what happens
on each scroll event: which panes are candidates, what their overlay bounds are,
what cursor coordinates are being tested, and whether the hit test matches. This
will immediately reveal whether the bug is a false-positive hit test, a
coordinate mismatch, or something else.

#### Where logs go

Wezboard writes logs to a file, not stdout/stderr. The log file is at:

```
~/.local/share/termsurf/wezboard/wezboard-gui-log-{pid}.txt
```

To watch logs in real time while testing:

```bash
tail -f ~/.local/share/termsurf/wezboard/wezboard-gui-log-*.txt
```

The log level defaults to `info`. Override with `WEZBOARD_LOG` env var if
needed.

#### Changes

**`wezboard/wezboard-gui/src/termsurf/input.rs`**

1. In `try_forward_scroll_any_pane()`, after collecting candidates, log the
   candidate count and each pane's overlay bounds:

   ```rust
   log::info!(
       "scroll_any_pane: cursor=({},{}) candidates={}",
       coords.x, coords.y, candidates.len()
   );
   ```

2. In `try_forward_raw_scroll()`, log when the hit test matches AND when it
   misses, including the overlay bounds:

   ```rust
   log::info!(
       "scroll hit_test: pane={} overlay=({},{},{},{}) cursor=({},{}) → {}",
       pane_id, ox, oy, ow, oh, mx, my, matched
   );
   ```

3. In `hit_test_overlay_at()`, add the same overlay bounds to the log.

**`wezboard/wezboard-gui/src/termwindow/mouseevent.rs`**

4. At the `raw_scroll_consumed` suppression check (~line 657), log when a wheel
   event is suppressed:

   ```rust
   log::info!("VertWheel/HorzWheel SUPPRESSED by raw_scroll_consumed");
   ```

#### Verification

1. Build: `scripts/build.sh wezboard`
2. Run Wezboard from the terminal to capture stdout/stderr:
   `wezboard/target/debug/wezboard-gui 2>&1 | tee /tmp/wezboard-scroll.log`
3. Open a browser overlay in one tab, open neovim in another tab
4. Scroll over neovim and observe stdout output
5. The logs will show whether the hit test is matching falsely, what coordinates
   are involved, and whether the wheel event is being suppressed

**Result:** Fail

The experiment design was wrong about where logs go. Wezboard's `log::info!`
writes to a log file
(`~/.local/share/termsurf/wezboard/wezboard-gui-log-*.txt`), not to
stdout/stderr. The debug logging was added correctly but could not be observed
at the expected location. The user had to capture stdout/stderr directly to see
the output.

The logging did reveal the root cause: **pane=2** (a webview on an inactive tab)
has overlay bounds `(26,95,2054,1980)` that cover nearly the entire window. The
hit test matches this invisible overlay and consumes scroll events intended for
neovim. The bug is that `try_forward_scroll_any_pane` does not filter out panes
on inactive tabs.

#### Conclusion

The hit test iterates all panes with `ca_layer_host != 0`, including panes on
inactive tabs. An overlay on a hidden tab still has bounds that overlap with the
visible tab's content area, causing false-positive hits. The fix: filter
candidates to only include panes on the currently active tab.
