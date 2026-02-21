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

## Experiment 1: Test matrix

### Goal

Determine which keys work and which don't, now that the navigation freeze is
resolved. No code changes — just test and record.

### Design

No code changes. The keyboard pipeline from Issue 607 is already in place. Issue
608 fixed the navigation freeze that blocked testing. This experiment
systematically tests every key behavior from the issue goal.

### Verification

```bash
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://lite.duckduckgo.com
```

Click the search box to enter browse mode and focus the text field. Run through
each test and record pass/fail:

| #  | Test                       | Steps                                              | Expected                             | Result | Description                                                         |
| -- | -------------------------- | -------------------------------------------------- | ------------------------------------ | ------ | ------------------------------------------------------------------- |
| 1  | Character typing           | Type "hello"                                       | "hello" appears in text field        | y      |                                                                     |
| 2  | Enter submits              | Type "test", press Enter                           | Search results page loads            | y      |                                                                     |
| 3  | Backspace deletes          | Type "helloo", press Backspace                     | Last "o" deleted, "hello" remains    | y      |                                                                     |
| 4  | Tab moves focus            | Press Tab from search box                          | Focus moves to next element          | n      |                                                                     |
| 5  | Arrow left/right           | Type "hello", press Left 3x, type "X"              | "heXllo" — cursor moved, then insert | y      |                                                                     |
| 6  | Arrow up/down              | In a multi-line textarea, press Up/Down            | Cursor moves between lines           | y      |                                                                     |
| 7  | Home / End                 | Type "hello", press Home, type "X"                 | "Xhello" — cursor at start           | y      |                                                                     |
| 8  | Shift+arrow selects        | Type "hello", Shift+Left 3x, type "X"              | "heX" — selection replaced           | y      |                                                                     |
| 9  | Cmd+A selects all          | Type "hello", Cmd+A, type "X"                      | "X" — all text replaced              | n      | Cmd+A selects the terminal contents instead of the webpage contents |
| 10 | Cmd+C / Cmd+V              | Type "hello", Cmd+A, Cmd+C, click new field, Cmd+V | "hello" pasted into new field        | n      |                                                                     |
| 11 | Cmd+X cuts                 | Type "hello", Cmd+A, Cmd+X                         | Text field empty, clipboard has text | n      |                                                                     |
| 12 | Cmd+Z undoes               | Type "hello", Cmd+A, type "X", Cmd+Z               | "hello" restored                     | n      |                                                                     |
| 13 | Ctrl+Esc exits browse mode | Press Ctrl+Esc                                     | Exits browse mode (regression check) | y      |                                                                     |

For tests 6 and 10, if lite.duckduckgo.com doesn't have a suitable multi-line
textarea or second field, use a different site (e.g., a form test page or
Wikipedia's search).

Record each result as Pass, Fail, or N/A (if the site doesn't support that
test). For any Fail, note the observed behavior.
