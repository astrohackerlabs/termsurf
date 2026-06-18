# Keybindings

All TermSurf-specific keybindings. Anything not listed here falls back to
Ghostty defaults or user-configured Ghostty keybindings.

## `web` TUI keybindings

These are handled by the `web` TUI process (`webtui/src/main.rs`) via crossterm
key events received through the terminal PTY.

| Key    | Mode    | Action                      | Notes                                        |
| ------ | ------- | --------------------------- | -------------------------------------------- |
| Esc    | Browse  | Switch to Control           | Sends `mode_changed(browsing: false)` to GUI |
| Enter  | Control | Switch to Browse            | Sends `mode_changed(browsing: true)` to GUI  |
| i      | Control | Edit URL (insert at cursor) | Opens editor in insert mode (Issue 646)      |
| A      | Control | Edit URL (insert at end)    | Cursor jumps to end of line (Issue 658)      |
| I      | Control | Edit URL (insert at start)  | Cursor jumps to start of line (Issue 658)    |
| n      | Control | Edit URL (normal mode)      | Cursor at last position (Issue 658)          |
| v      | Control | Edit URL (visual mode)      | Empty selection at cursor (Issue 658)        |
| V      | Control | Edit URL (visual line)      | Entire URL selected (Issue 658)              |
| :      | Control | Enter Command mode          | Yellow command bar (Issue 659)               |
| Cmd+C  | Control | Copy current URL            | Shows brief `url copied` feedback            |
| q      | Control | Quit                        |                                              |
| Ctrl+C | Any     | Force quit                  |                                              |

## GUI keybindings

These are handled in TermSurf's Zig core (`ghostboard/src/Surface.zig`),
intercepted in `keyCallback` before keybinding processing.

| Key | Mode   | Action            | Notes                                                                                                                                                            |
| --- | ------ | ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Esc | Browse | Switch to Control | Calls `notifyEsc` which sends `mode_changed(browsing: false)` to `web` and `focus_changed(false)` to Chromium. Gated on `isOverlayForwarding` (Issues 607, 665). |

## Browser navigation keybindings

These are forwarded from the GUI to Chromium via protobuf socket messages.
Chromium handles them internally via its default keybinding logic.

| Key   | Mode   | Action  | Notes                                       |
| ----- | ------ | ------- | ------------------------------------------- |
| Cmd+[ | Browse | Back    | Forwarded as key event, handled by Chromium |
| Cmd+] | Browse | Forward | Forwarded as key event, handled by Chromium |
| Cmd+R | Browse | Reload  | Forwarded as key event, handled by Chromium |

## Commands

Entered via `:` in Control mode. Exact match only (Issue 772).

| Command              | Shortcut | Action                      |
| -------------------- | -------- | --------------------------- |
| `:quit`              | `:q`     | Quit                        |
| `:dark [on\|off\|s]` | `:da`    | Toggle/set dark mode        |
| `:devtools [dir]`    | `:de`    | Open DevTools in split pane |

## Modes

The `web` TUI has four modes. The GUI only tracks a boolean (`browsing` or not).

| Mode        | GUI sees | Behavior                                                  |
| ----------- | -------- | --------------------------------------------------------- |
| **Control** | `false`  | Terminal keybindings active (default on startup)          |
| **Browse**  | `true`   | Keyboard/mouse goes to the browser                        |
| **Edit**    | `false`  | Vim-style URL editing with Normal/Insert submodes (edtui) |
| **Command** | `false`  | `:` prefix command entry                                  |

The GUI does not distinguish between non-browse modes. Control, Edit, and
Command all map to `browsing: false` from the GUI's perspective.

## Mode synchronization

Mode state is shared between the GUI and `web` via `mode_changed` protobuf
socket messages on the existing direct connection (Issue 513). Both sides send
and receive:

- **`web` changes mode** (Esc, Enter) → sends `mode_changed` to GUI
- **GUI changes mode** (Esc in browse mode) → sends `mode_changed` to `web`
- **Initial mode** is set via the `browsing` field in the `set_overlay` message

## Ghostty fallbacks

All other keybindings (splits, tabs, copy/paste, font size, etc.) are Ghostty
defaults. See the [Ghostty documentation](https://ghostty.org/docs) for the full
list. Users can customize these via Ghostty's configuration file.
