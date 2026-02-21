# Issue 609: Keyboard Input (continued)

## Goal

Complete keyboard input on Chromium overlays. Backspace deletes, Tab moves
between form fields, Enter submits, arrow keys navigate within text, Cmd+A
selects all, Cmd+C copies, Cmd+V pastes. All keys that a user expects to work in
a browser text field work.

## Background

Issue 607 built the keyboard forwarding pipeline end to end:

- **Ghost (Zig):** `keyToWindowsVK` maps Ghostty's key enum to Windows VK codes.
  `sendKeyEvent` constructs XPC messages. The `keyCallback` forwarding block
  routes keys to Chromium when in browse mode.
- **Chromium (C++):** `HandleKeyEvent` receives the XPC message, constructs
  `NativeWebKeyboardEvent` (`kRawKeyDown` + `kChar` for characters, `kKeyUp` for
  release), and calls `ForwardKeyboardEvent`.
- **Ctrl+Esc:** Always exits browse mode, regardless of browser state.

Issue 607 Experiment 2 proved character typing works — letters appear in text
fields. But testing was blocked by a navigation freeze (pressing Enter or
clicking Search froze the overlay). Issue 608 fixed that: `PrimaryPageChanged`
now recreates the capturer when the `RenderWidgetHost` changes.

With the navigation freeze resolved, we can now test the full keyboard feature
set. The pipeline exists but has only been validated for character input. The
following remain untested:

| Key          | Expected behavior                       |
| ------------ | --------------------------------------- |
| Enter        | Submit form (was blocked by 608 freeze) |
| Backspace    | Delete character before cursor          |
| Tab          | Move to next focusable element          |
| Arrow keys   | Move cursor within text field           |
| Home / End   | Move to start / end of line             |
| Cmd+A        | Select all text                         |
| Cmd+C        | Copy selected text to clipboard         |
| Cmd+V        | Paste from clipboard                    |
| Cmd+X        | Cut selected text                       |
| Cmd+Z        | Undo                                    |
| Shift+arrows | Extend text selection                   |

### Potential issues

The current `HandleKeyEvent` constructs `NativeWebKeyboardEvent` with only
`windows_key_code` and `text` fields set. Other fields that Chromium may need:

- **`native_key_code`** — macOS keycode. Not currently set. Chromium may use
  this for some key handling paths.
- **`dom_code`** — USB HID usage code. Not currently set. Some Chromium features
  (e.g., keyboard shortcuts) may check this.
- **`dom_key`** — DOM key enum. Not currently set.
- **`is_system_key`** — Whether this is a system key event (Alt+key on Windows,
  Cmd+key on macOS). Not set. Chromium may need this for Cmd+C/V/A to trigger
  clipboard operations.

If basic keys work but Cmd shortcuts don't, these missing fields are the likely
cause.

### Clipboard

Cmd+C and Cmd+V require clipboard access. Two possibilities:

1. **Chromium handles it internally.** If `ForwardKeyboardEvent` with Cmd+C
   triggers Chromium's built-in copy command, the text is copied to the system
   clipboard (which Ghost can read). This is the ideal case — no extra work.
2. **Chromium doesn't handle it.** If Chromium's headless/content_shell mode
   doesn't wire up clipboard shortcuts, we may need to invoke clipboard commands
   explicitly via the `WebContents` editing API (`Copy()`, `Paste()`, etc.).

### Key files

- `ghost/src/apprt/xpc.zig` — `keyToWindowsVK`, `sendKeyEvent`
- `ghost/src/Surface.zig` — Key forwarding block in `keyCallback`
- `chromium/src/content/chromium_profile_server/browser/shell_browser_main_parts.cc`
  — `HandleKeyEvent`, XPC dispatch
- `chromium/src/content/chromium_profile_server/browser/shell_browser_main_parts.h`
  — `HandleKeyEvent` declaration

### Chromium branch

Create `146.0.7650.0-issue-609` from `146.0.7650.0-issue-608`. The 609 branch
builds on 608's capturer re-attach fix and 607's keyboard forwarding code.
