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

**Status: Pending**

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
