+++
status = "closed"
opened = "2026-02-28"
closed = "2026-02-28"
+++

# Issue 680: Dark Mode

Web content doesn't respond to system dark mode. Pages that use
`prefers-color-scheme` always see "light". TermSurf needs to forward the system
color scheme to Chromium so CSS media queries work, and let users override it.

## Background

### What Already Works

The terminal side has a full dark mode chain:

1. **macOS KVO observer** (`AppDelegate.swift:295`) watches
   `NSApp.effectiveAppearance` for system theme changes
2. **C API** `termsurf_app_set_color_scheme()` propagates to
   `App.colorSchemeEvent()` in Zig
3. **Per-surface** `Surface.colorSchemeCallback()` updates
   `config_conditional_state.theme` and triggers config reload
4. **Terminal reporting** sends DSR `\x1B[?997;1n` (dark) or `\x1B[?997;0n`
   (light) to child processes
5. **Config override** `window-theme = auto|system|light|dark` lets users force
   a theme

The chain breaks at the TermSurf→Chromium boundary. No XPC message tells
Chromium about the color scheme.

### How Chromium Handles Dark Mode

Chromium uses `WebPreferences::preferred_color_scheme` to control the CSS
`prefers-color-scheme` media query.

**WebPreferences fields**
(`blink/public/common/web_preferences/web_preferences.h`):

```cpp
blink::mojom::PreferredColorScheme preferred_color_scheme =
    blink::mojom::PreferredColorScheme::kLight;

bool force_dark_mode_enabled = false;
```

- `preferred_color_scheme` — tells web content whether the user prefers dark or
  light. This is what CSS `@media (prefers-color-scheme: dark)` checks.
- `force_dark_mode_enabled` — separate feature that auto-inverts light pages.
  Not what we want here.

**Embedder hook** (`content/public/browser/content_browser_client.h`):

```cpp
virtual void OverrideWebPreferences(
    WebContents* web_contents,
    SiteInstance& main_frame_site,
    blink::web_pref::WebPreferences* prefs) {}
```

**Current TermSurf Chromium server**
(`content/chromium_profile_server/browser/shell_content_browser_client.cc:577`):

```cpp
void ShellContentBrowserClient::OverrideWebPreferences(
    WebContents* web_contents,
    SiteInstance& main_frame_site,
    blink::web_pref::WebPreferences* prefs) {
  if (base::CommandLine::ForCurrentProcess()->HasSwitch(
          switches::kForceDarkMode)) {
    prefs->preferred_color_scheme = blink::mojom::PreferredColorScheme::kDark;
  } else {
    prefs->preferred_color_scheme = blink::mojom::PreferredColorScheme::kLight;
  }
  // ...
}
```

This only checks the `--force-dark-mode` command-line flag at startup. It never
updates dynamically. The fix is to make it read from per-tab state that XPC
messages can update.

### XPC Messages (Current)

**GUI → Chromium** (sent from `gui/src/apprt/xpc.zig`):

- `set_overlay`, `create_tab`, `resize`, `navigate`
- Mouse/keyboard/focus events

**Chromium → GUI** (sent from
`chromium_profile_server/browser/shell_browser_main_parts.cc`):

- `tab_ready`, `ca_context`, `cursor_changed`, `url_changed`, `loading_state`,
  `title_changed`

No color scheme message in either direction.

### XPC Message Dispatch (GUI Side)

`gui/src/apprt/xpc.zig` dispatches on the `action` field:

```zig
if (std.mem.eql(u8, action_str, "set_overlay")) {
    handleSetOverlay(msg);
} else if (std.mem.eql(u8, action_str, "hello")) {
    handleHello(msg);
} else if (std.mem.eql(u8, action_str, "tab_ready")) {
    handleTabReady(msg);
// ... 8 more handlers
}
```

### XPC Message Dispatch (Chromium Side)

`chromium_profile_server/browser/shell_browser_main_parts.cc` dispatches on
`action`:

- `create_tab`, `resize`, `mouse_event`, `scroll_event`, `mouse_move`,
  `focus_changed`, `key_event`, `navigate`

No `set_color_scheme` handler.

## Experiment 1: Forward color scheme via XPC

### Hypothesis

Adding a `set_color_scheme` XPC message from TermSurf to Chromium, and making
Chromium read it in `OverrideWebPreferences`, will make `prefers-color-scheme`
work for web content.

### Changes

#### 1. Chromium: Add per-tab color scheme state

In `shell_browser_main_parts.h`, add a `preferred_color_scheme` field to the
per-tab state (alongside `web_contents`, `pane_id`, etc.). Default to `kDark`.

#### 2. Chromium: Read `dark` field in `create_tab` handler

In `shell_browser_main_parts.cc`, read the `dark` bool from the `create_tab`
message and store it in the per-tab state. `create_tab` is the hello message for
each tab — the tab starts with the correct color scheme from the first frame.

#### 3. Chromium: Handle `set_color_scheme` XPC message

In `shell_browser_main_parts.cc`, add a handler for
`action = "set_color_scheme"` with fields:

- `pane_id` (string) — which tab
- `dark` (bool) — true for dark, false for light

The handler updates the per-tab `preferred_color_scheme` and calls
`web_contents->OnWebPreferencesChanged()` to push the new value to the renderer.
This handles dynamic changes (e.g. user toggles macOS dark mode while a tab is
open).

#### 4. Chromium: Read per-tab state in `OverrideWebPreferences`

Change `ShellContentBrowserClient::OverrideWebPreferences()` to look up the
tab's `preferred_color_scheme` instead of only checking the command-line flag.
Fall back to the command-line flag if no per-tab state exists.

#### 5. GUI: Send `dark` in `create_tab`

In `gui/src/apprt/xpc.zig`, add a `dark` bool field to the `create_tab` XPC
message, set from `surface.config_conditional_state.theme`.

#### 6. GUI: Send `set_color_scheme` on theme change

In `Surface.colorSchemeCallback()`, after updating the config conditional state,
send a `set_color_scheme` XPC message to the Chromium server if this surface has
an active browser pane.

### Test

1. Open TermSurf, navigate to a page that uses `prefers-color-scheme` (e.g.
   `https://googlechrome.github.io/samples/dark-mode/`)
2. Toggle macOS dark mode in System Settings → Appearance
3. Page should reactively switch between dark and light styles
4. Open a new pane in dark mode — it should start in dark mode immediately

### Result: SUCCESS

`prefers-color-scheme` works. Pages respond to the system color scheme both on
initial load and dynamically when toggling macOS appearance.

## Experiment 2: `:colorscheme` command

### Hypothesis

Adding a `:colorscheme dark|light|system` command to the TUI's command mode will
let users manually override the browser pane's color scheme without changing
system settings.

### Design

The chain: **TUI → GUI → Chromium**.

1. User types `:colorscheme dark` (or `:col d` via prefix matching)
2. TUI dispatches command, sends `set_color_scheme` XPC message to GUI
3. GUI receives it in xpc.zig, forwards to Chromium via the existing
   `handleColorSchemeChanged`
4. Chromium updates `WebPreferences::preferred_color_scheme` and the page
   re-evaluates `prefers-color-scheme`

Three arguments:

- `dark` — force dark mode on the browser pane
- `light` — force light mode on the browser pane
- `system` — read the current system theme and apply it (one-shot, not
  persistent tracking)

This only affects the browser pane's CSS `prefers-color-scheme`. The terminal
itself continues to follow system settings via the existing KVO chain.

### Changes

#### 1. TUI: Add `colorscheme` command (`tui/src/main.rs`)

The current `Command.exec` signature is `fn(args: &[&str]) -> CommandResult`
with no access to the compositor. Add a new `CommandResult` variant:

```rust
enum CommandResult {
    Quit,
    SetColorScheme(String), // "dark", "light", "system"
    None,
}
```

Add the command to the `COMMANDS` table:

```rust
Command {
    name: "colorscheme",
    exec: |args| {
        match args.first().map(|s| *s) {
            Some("dark" | "d") => CommandResult::SetColorScheme("dark".into()),
            Some("light" | "l") => CommandResult::SetColorScheme("light".into()),
            Some("system" | "s") => CommandResult::SetColorScheme("system".into()),
            _ => CommandResult::None,
        }
    },
},
```

Handle the new result in the command dispatch match (where we have access to the
compositor and pane_id):

```rust
CommandResult::SetColorScheme(scheme) => {
    if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
        conn.send_set_color_scheme(pid, &scheme);
    }
}
```

#### 2. TUI: Add `send_set_color_scheme` to XPC client (`tui/src/xpc.rs`)

New method on `CompositorConnection`:

```rust
pub fn send_set_color_scheme(&self, pane_id: &str, scheme: &str) {
    // Send { action: "set_color_scheme", pane_id, scheme }
}
```

Follows the same pattern as `send_navigate`.

#### 3. GUI: Handle `set_color_scheme` from TUI (`gui/src/apprt/xpc.zig`)

Add a new handler in the TUI message dispatcher for
`action = "set_color_scheme"`:

- Read `scheme` string field (`"dark"`, `"light"`, or `"system"`)
- Look up the pane's overlay surface
- For `dark`/`light`: call `handleColorSchemeChanged(surface, dark)`
- For `system`: read `surface.config_conditional_state.theme` (already reflects
  the current system setting via KVO) and forward that

No Chromium changes needed — it already handles `set_color_scheme` XPC messages
from Experiment 1.

### Test

1. Open TermSurf, navigate to a dark-mode-aware page
2. Type `:colorscheme light` — page switches to light styles
3. Type `:col d` — page switches back to dark styles (prefix matching)
4. Type `:colorscheme system` — page follows current macOS appearance
5. Verify `:colorscheme` with no argument or invalid argument is a no-op

### Result: SUCCESS

`:colorscheme dark`, `:col l`, and `:col s` all work. The browser pane switches
color scheme immediately via TUI → GUI → Chromium XPC chain.

## Conclusion

Dark mode now works end-to-end across the full TermSurf stack.

**Experiment 1** bridged the last gap in the color scheme chain — from the GUI's
Zig core through XPC to Chromium. The Chromium fork stores a per-tab
`preferred_color_scheme`, receives it via the `dark` field on `create_tab`
(initial state) and the `set_color_scheme` action (dynamic updates), and applies
it through `OverrideWebPreferences`. On the GUI side,
`Surface.colorSchemeCallback` now forwards system appearance changes to Chromium
automatically. Default is dark, because this is a terminal.

**Experiment 2** gave users direct control via the `:colorscheme` command in the
TUI's vim-style command mode. `:col d`, `:col l`, and `:col s` override the
browser pane's color scheme to dark, light, or current system setting. The
command flows TUI → GUI → Chromium through the same XPC `set_color_scheme`
message that Experiment 1 established.

### Files changed

**Chromium** (branch `146.0.7650.0-issue-680`):

- `shell_browser_main_parts.h` — per-tab `preferred_color_scheme` state,
  `SetColorScheme` and `GetColorSchemeForWebContents` methods
- `shell_browser_main_parts.cc` — `dark` field in `create_tab`,
  `set_color_scheme` XPC handler, color scheme lookup by WebContents
- `shell_content_browser_client.cc` — `OverrideWebPreferences` reads per-tab
  state instead of command-line flag

**GUI** (`gui/`):

- `src/apprt/xpc.zig` — `dark` in `sendCreateTab`, `handleColorSchemeChanged`
  for system changes, `handleSetColorScheme` for TUI commands
- `src/Surface.zig` — XPC forwarding in `colorSchemeCallback`

**TUI** (`tui/`):

- `src/main.rs` — `colorscheme` command with `dark`/`light`/`system` args
- `src/xpc.rs` — `send_set_color_scheme` XPC method
