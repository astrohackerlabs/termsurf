+++
status = "closed"
opened = "2026-02-02"
closed = "2026-02-02"
+++

# Issue 337: Browser Refresh (Cmd+R)

## Product Requirements

### User Story

As a user browsing the web in TermSurf, I want to refresh the current page using
the familiar Cmd+R keyboard shortcut, so that I can reload content without
retyping the URL.

### Acceptance Criteria

1. **Cmd+R** reloads the current page
2. **Cmd+Shift+R** performs a hard reload (ignore cache)
3. Refresh works in both Browse mode and Control mode
4. Works when a webview pane is focused

### Keybindings

| Shortcut    | Action                     | Notes                        |
| ----------- | -------------------------- | ---------------------------- |
| Cmd+R       | Reload page                | Standard browser shortcut    |
| Cmd+Shift+R | Reload page (ignore cache) | Hard refresh, bypasses cache |

### Non-Requirements (Out of Scope)

- Loading indicator during refresh (future enhancement)
- Pull-to-refresh gesture (not applicable to terminal)

## Technical Context

This follows the same pattern as issue 335 (back/forward navigation):

1. GUI intercepts Cmd+R / Cmd+Shift+R in `keyevent.rs`
2. Sends XPC message to profile server
3. Profile server calls CEF's `browser.reload()` or
   `browser.reload_ignore_cache()`

### CEF Methods

From `cef-rs` bindings, the Browser object has:

- `reload()` — Normal reload
- `reload_ignore_cache()` — Hard reload, bypasses cache

## Files Involved

- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — Add `send_reload()` method
- `ts3/wezterm-gui/src/termwindow/keyevent.rs` — Intercept Cmd+R / Cmd+Shift+R
- `ts3/termsurf-profile/src/main.rs` — Add ReloadTask and XPC handler

---

## ts2 Implementation

In ts2, refresh is handled in `keyevent.rs` (lines 497-511) alongside back/forward:

```rust
KeyCode::Char('r') => {
    log::info!("[CEF] Cmd+R: reload for pane {}", pane_id);
    if let Some(browser) = self.browser_states.borrow().get(&pane_id) {
        browser.reload();
    }
    true
}
KeyCode::Char('R') => {
    // Cmd+Shift+R (uppercase R with SHIFT modifier)
    log::info!("[CEF] Cmd+Shift+R: hard reload for pane {}", pane_id);
    if let Some(browser) = self.browser_states.borrow().get(&pane_id) {
        browser.reload_ignore_cache();
    }
    true
}
```

Note: Cmd+Shift+R produces uppercase `'R'` due to the Shift modifier.

---

## Experiments

### Experiment 1: XPC-based reload commands

**Status: Failed**

Follow the same pattern as issue 335 (back/forward navigation).

**Failure reason:** Cmd+R is intercepted by WezTerm's default keybinding for
`ReloadConfiguration` before our `handle_webview_key_event` handler runs. Unlike
Cmd+[ and Cmd+] which have no default bindings, Cmd+R is bound in WezTerm's
default key table (see `docs/config/default-keys.md` line 67).

#### Step 1: Add XPC methods in GUI (webview_xpc.rs)

Add `send_reload()` and `send_reload_ignore_cache()` methods after
`send_go_forward()`:

```rust
/// Send reload command to the browser (issue 337)
pub fn send_reload(&self, pane_id: PaneId) -> bool {
    let msg = XpcDictionary::new();
    msg.set_string("action", "reload");

    if self.send_command(pane_id, &msg) {
        log::info!("[XPC] Sent reload to pane {}", pane_id);
        true
    } else {
        false
    }
}

/// Send reload_ignore_cache command to the browser (issue 337)
pub fn send_reload_ignore_cache(&self, pane_id: PaneId) -> bool {
    let msg = XpcDictionary::new();
    msg.set_string("action", "reload_ignore_cache");

    if self.send_command(pane_id, &msg) {
        log::info!("[XPC] Sent reload_ignore_cache to pane {}", pane_id);
        true
    } else {
        false
    }
}
```

#### Step 2: Intercept Cmd+R in Browse mode (keyevent.rs)

Add handlers after the Cmd+] handler (go_forward):

```rust
// Handle Cmd+R (reload) - issue 337
let is_cmd_r = window_key.key_is_down
    && window_key.modifiers.contains(Modifiers::SUPER)
    && !window_key.modifiers.contains(Modifiers::SHIFT)
    && matches!(&window_key.key, KeyCode::Char('r'));

if is_cmd_r {
    log::info!("[NAV] Cmd+R detected, sending reload to browser");
    drop(overlays);
    if let Some(xpc_manager) = crate::termwindow::webview_xpc::get_xpc_manager() {
        xpc_manager.send_reload(pane_id);
    }
    return Some(true);
}

// Handle Cmd+Shift+R (hard reload) - issue 337
let is_cmd_shift_r = window_key.key_is_down
    && window_key.modifiers.contains(Modifiers::SUPER)
    && window_key.modifiers.contains(Modifiers::SHIFT)
    && matches!(&window_key.key, KeyCode::Char('r') | KeyCode::Char('R'));

if is_cmd_shift_r {
    log::info!("[NAV] Cmd+Shift+R detected, sending reload_ignore_cache to browser");
    drop(overlays);
    if let Some(xpc_manager) = crate::termwindow::webview_xpc::get_xpc_manager() {
        xpc_manager.send_reload_ignore_cache(pane_id);
    }
    return Some(true);
}
```

#### Step 3: Intercept Cmd+R in Control mode (keyevent.rs)

Add the same handlers in the `WebviewMode::Control` arm.

#### Step 4: Add reload tasks in profile server (termsurf-profile/src/main.rs)

Add ReloadTask and ReloadIgnoreCacheTask after GoForwardTask:

```rust
// ====== Reload Task ======
//
// Task for reloading the page via CEF's browser.reload().
// Issue 337: Browser refresh.

wrap_task! {
    pub struct ReloadTask {
        state: Arc<BrowserState>,
    }

    impl Task {
        fn execute(&self) {
            if let Some(browser) = self.state.browser.lock().unwrap().as_ref() {
                println!("[NAV] Calling browser.reload()");
                browser.reload();
            } else {
                println!("[NAV] ReloadTask: no browser");
            }
        }
    }
}

// ====== Reload Ignore Cache Task ======
//
// Task for hard reload via CEF's browser.reload_ignore_cache().
// Issue 337: Browser refresh (bypass cache).

wrap_task! {
    pub struct ReloadIgnoreCacheTask {
        state: Arc<BrowserState>,
    }

    impl Task {
        fn execute(&self) {
            if let Some(browser) = self.state.browser.lock().unwrap().as_ref() {
                println!("[NAV] Calling browser.reload_ignore_cache()");
                browser.reload_ignore_cache();
            } else {
                println!("[NAV] ReloadIgnoreCacheTask: no browser");
            }
        }
    }
}
```

#### Step 5: Handle XPC actions in profile server (termsurf-profile/src/main.rs)

Add handlers after the "go_forward" case:

```rust
"reload" => {
    // Issue 337: Reload page
    let state_guard = deferred_for_handler.lock().unwrap();
    let Some(bs) = state_guard.as_ref() else {
        println!("Profile: reload ignored (state not ready)");
        return;
    };

    println!("[NAV] Received reload command");

    let bs = Arc::clone(bs);
    drop(state_guard);

    let mut task = ReloadTask::new(bs);
    cef::post_task(cef::ThreadId::UI, Some(&mut task));
}
"reload_ignore_cache" => {
    // Issue 337: Hard reload (bypass cache)
    let state_guard = deferred_for_handler.lock().unwrap();
    let Some(bs) = state_guard.as_ref() else {
        println!("Profile: reload_ignore_cache ignored (state not ready)");
        return;
    };

    println!("[NAV] Received reload_ignore_cache command");

    let bs = Arc::clone(bs);
    drop(state_guard);

    let mut task = ReloadIgnoreCacheTask::new(bs);
    cef::post_task(cef::ThreadId::UI, Some(&mut task));
}
```

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
web example.com
# Press Cmd+R → page should reload
# Press Cmd+Shift+R → page should hard reload (bypass cache)
# Press Ctrl+C to enter Control mode
# Press Cmd+R → should still reload
```

Check logs:
```bash
tail -f /tmp/termsurf-gui.log | grep NAV
tail -f /tmp/termsurf-profile-*.log | grep NAV
```

---

### Experiment 2: Remove Cmd+R from WezTerm's default keybindings

**Status: Partial Success**

Remove the Cmd+R → ReloadConfiguration default keybinding. WezTerm auto-reloads
config files when they change, so the manual reload shortcut is unnecessary.
This frees Cmd+R for browser refresh.

#### Rationale

- WezTerm's `automatically_reload_config` is enabled by default
- Config changes are detected and applied automatically
- Manual reload via Cmd+R is redundant
- Removing it allows Experiment 1's handler in `key_event_impl` to receive the key

#### Step 1: Remove Cmd+R keybinding from commands.rs

In `ts3/wezterm-gui/src/commands.rs`, find the `ReloadConfiguration` entry
(around line 1266) and remove the keybinding:

```rust
// Before:
ReloadConfiguration => CommandDef {
    brief: "Reload configuration".into(),
    doc: "Reloads the configuration file".into(),
    keys: vec![(Modifiers::SUPER, "r".into())],
    args: &[],
    menubar: &["TermSurf"],
    icon: Some("md_reload"),
},

// After:
ReloadConfiguration => CommandDef {
    brief: "Reload configuration".into(),
    doc: "Reloads the configuration file".into(),
    keys: vec![],  // Removed Cmd+R binding
    args: &[],
    menubar: &["TermSurf"],
    icon: Some("md_reload"),
},
```

The menu item remains in the TermSurf menu but without a keyboard shortcut.

#### Step 2: Update default-keys.md documentation

Remove the Cmd+R line from `ts3/docs/config/default-keys.md` (line 67):

```markdown
| `SUPER`          | `r`    | `ReloadConfiguration` |
```

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
web example.com
# Press Cmd+R → page should reload (not config)
# Press Cmd+Shift+R → page should hard reload
# In terminal pane (no webview): Cmd+R should do nothing
# TermSurf menu should still show "Reload configuration" (without shortcut)
```

Check logs:
```bash
tail -f /tmp/termsurf-gui.log | grep NAV
tail -f /tmp/termsurf-profile-*.log | grep NAV
```

#### Conclusion

**What works:**
- Cmd+R successfully reloads the browser page
- Removing the default keybinding allowed our handler in `key_event_impl` to
  receive the key

**What doesn't work:**
- Cmd+Shift+R (hard reload) does not trigger
- The Ctrl+Shift+R binding for ReloadConfiguration still exists in WezTerm's
  defaults (see `default-keys.md` line 68), but Cmd+Shift+R is a different
  combination and should not conflict
- The issue may be with how the Shift modifier affects key detection — ts2 notes
  that Cmd+Shift+R produces uppercase `'R'` due to the Shift modifier

**Next steps:**
- Investigate why Cmd+Shift+R is not being detected
- Check if the key arrives as `KeyCode::Char('R')` (uppercase) vs
  `KeyCode::Char('r')` (lowercase) when Shift is held

---

### Experiment 3: Match uppercase 'R' without checking SHIFT modifier

**Status: Failed**

Follow ts2's pattern: match on `KeyCode::Char('R')` directly instead of checking
for `Modifiers::SHIFT`. When Shift is held, the character arrives as uppercase.

**Failure reason:** Cmd+Shift+R still does not trigger. The key difference between
ts2 and ts3 may be the event type: ts2 handles this in `raw_key_event_impl` with
`RawKeyEvent`, while ts3 handles it in `key_event_impl` with `KeyEvent`. The key
representation may differ between these event types.

#### Rationale

ts2's working implementation (lines 497-511) does NOT check for `Modifiers::SHIFT`:

```rust
// ts2 only checks SUPER, then matches on character case
if key.key_is_down && key.modifiers.contains(Modifiers::SUPER) {
    let handled = match &key.key {
        KeyCode::Char('r') => { /* Cmd+R */ }
        KeyCode::Char('R') => { /* Cmd+Shift+R - uppercase from Shift */ }
        _ => false,
    };
}
```

Our ts3 code redundantly checks both `Modifiers::SHIFT` AND the character, which
may cause the match to fail if the SHIFT modifier is consumed when producing the
uppercase character.

#### Step 1: Simplify Cmd+Shift+R detection in Browse mode (keyevent.rs)

Replace the current `is_cmd_shift_r` check:

```rust
// Before:
let is_cmd_shift_r = window_key.key_is_down
    && window_key.modifiers.contains(Modifiers::SUPER)
    && window_key.modifiers.contains(Modifiers::SHIFT)
    && matches!(&window_key.key, KeyCode::Char('r') | KeyCode::Char('R'));

// After:
let is_cmd_shift_r = window_key.key_is_down
    && window_key.modifiers.contains(Modifiers::SUPER)
    && matches!(&window_key.key, KeyCode::Char('R'));
```

Note: Only match uppercase `'R'` — this implicitly requires Shift to be held.

#### Step 2: Apply same fix in Control mode (keyevent.rs)

Apply the same simplification to the Control mode handler.

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
web example.com
# Press Cmd+R → page should reload
# Press Cmd+Shift+R → page should hard reload (bypass cache)
# Press Ctrl+C to enter Control mode
# Press Cmd+Shift+R → should still hard reload
```

Check logs:
```bash
tail -f /tmp/termsurf-gui.log | grep NAV
tail -f /tmp/termsurf-profile-*.log | grep NAV
# Expected: "[NAV] Cmd+Shift+R detected" when pressing Cmd+Shift+R
```

---

### Experiment 4: Handle shortcuts in raw_key_event_impl like ts2

**Status: Success**

Handle Cmd+R and Cmd+Shift+R directly in `raw_key_event_impl` instead of deferring
to `key_event_impl`. This matches ts2's working approach exactly.

#### Rationale

ts2 handles browser shortcuts in `raw_key_event_impl` (lines 466-520):
1. Checks if pane has a browser in Browse mode
2. Matches Cmd+R → `browser.reload()`
3. Matches Cmd+Shift+R (uppercase 'R') → `browser.reload_ignore_cache()`
4. Calls `key.set_handled()` and returns

ts3 currently only skips keybinding processing in `raw_key_event_impl`, then
defers handling to `key_event_impl`. The key representation may differ between
`RawKeyEvent` and `KeyEvent`, causing Cmd+Shift+R to fail.

#### Step 1: Handle shortcuts in raw_key_event_impl (keyevent.rs)

Replace the current Browse mode early-return block (around lines 466-491) with
actual shortcut handling like ts2:

```rust
// Skip keybinding processing when webview is active in Browse mode
// AND handle browser shortcuts directly here (ts2 pattern)
#[cfg(target_os = "macos")]
{
    use crate::termwindow::webview_socket::{get_server, WebviewMode};

    let pane_id = pane.pane_id();
    if let Some(server) = get_server() {
        let state = server.state();
        let overlays = state.read().unwrap();
        if let Some(overlay) = overlays.overlays.get(&pane_id) {
            if overlay.mode == WebviewMode::Browse {
                // Handle browser shortcuts in Browse mode (issue 337)
                if key.key_is_down && key.modifiers.contains(Modifiers::SUPER) {
                    let handled = match &key.key {
                        KeyCode::Char('r') => {
                            log::info!("[NAV] Cmd+R in raw_key_event: reload");
                            drop(overlays);
                            if let Some(xpc_manager) =
                                crate::termwindow::webview_xpc::get_xpc_manager()
                            {
                                xpc_manager.send_reload(pane_id);
                            }
                            true
                        }
                        KeyCode::Char('R') => {
                            log::info!("[NAV] Cmd+Shift+R in raw_key_event: hard reload");
                            drop(overlays);
                            if let Some(xpc_manager) =
                                crate::termwindow::webview_xpc::get_xpc_manager()
                            {
                                xpc_manager.send_reload_ignore_cache(pane_id);
                            }
                            true
                        }
                        _ => false,
                    };
                    if handled {
                        key.set_handled();
                        return;
                    }
                }

                // For other keys in Browse mode: skip keybinding processing
                // but let KeyEvent flow through to key_event_impl
                if key.key_is_down {
                    log::debug!(
                        "[Webview] Skipping keybindings in Browse mode: {:?}",
                        key.key
                    );
                }
                return; // Early return, NO set_handled()
            }
        }
    }
}
```

#### Step 2: Keep key_event_impl handlers for Control mode

The existing handlers in `key_event_impl` can remain for Control mode, or we can
add similar handling to `raw_key_event_impl` for Control mode as well.

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
web example.com
# Press Cmd+R → page should reload
# Press Cmd+Shift+R → page should hard reload (bypass cache)
```

Check logs:
```bash
tail -f /tmp/termsurf-gui.log | grep NAV
# Expected: "[NAV] Cmd+R in raw_key_event" or "[NAV] Cmd+Shift+R in raw_key_event"
```

---

## Conclusion

Browser refresh is now fully implemented:

- **Cmd+R** reloads the current page
- **Cmd+Shift+R** performs a hard reload (bypasses cache)

### Key Learnings

1. **Remove conflicting default keybindings**: WezTerm's default Cmd+R →
   `ReloadConfiguration` binding had to be removed since config auto-reloads anyway.

2. **Handle shortcuts in `raw_key_event_impl`, not `key_event_impl`**: The key
   representation differs between `RawKeyEvent` and `KeyEvent`. ts2 handles browser
   shortcuts in `raw_key_event_impl` where the uppercase 'R' is correctly detected
   for Cmd+Shift+R. Deferring to `key_event_impl` caused Cmd+Shift+R to fail.

3. **Match on character case, not SHIFT modifier**: When Shift is held, the key
   arrives as uppercase `KeyCode::Char('R')`. Matching on the character directly
   (like ts2) is simpler and more reliable than checking `Modifiers::SHIFT`.

### Files Modified

- `ts3/wezterm-gui/src/commands.rs` — Removed Cmd+R default keybinding
- `ts3/wezterm-gui/src/termwindow/keyevent.rs` — Handle Cmd+R/Cmd+Shift+R in
  `raw_key_event_impl`
- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — Added `send_reload()` and
  `send_reload_ignore_cache()` XPC methods
- `ts3/termsurf-profile/src/main.rs` — Added ReloadTask, ReloadIgnoreCacheTask,
  and XPC handlers
- `ts3/docs/config/default-keys.md` — Removed Cmd+R from documentation
