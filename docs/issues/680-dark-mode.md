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
