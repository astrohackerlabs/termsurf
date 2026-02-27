# Issue 659: Command Mode

Vim-style command mode for the TUI, triggered by `:` from Control mode.

## Problem

The TUI uses `q` to quit from Control mode. This is unintuitive for vim users
who expect `:q`. More broadly, the TUI has no command input — all actions are
single-key bindings. As the TUI grows, it needs an extensible way to accept
multi-character commands.

## Solution

### New TUI mode

Add `Mode::Command` as a fourth top-level mode. The full mode hierarchy becomes:

| TUI Mode | Description                                        |
| -------- | -------------------------------------------------- |
| Browse   | Viewport active, keys go to Chromium               |
| Control  | URL bar focused, keys are TUI commands             |
| Edit     | URL bar being edited, keys go to URL edtui         |
| Command  | Command line active, keys go to command-line edtui |

### Separate editor instance

Command mode uses its own `EditorState` and `EditorEventHandler`, independent of
the URL editor. This keeps command input isolated — typing `:q` doesn't modify
the URL. The command editor starts fresh each time `:` is pressed (no persistent
state across invocations, matching vim behavior).

### Command line rendering

When in Command mode, replace the status bar hints area (bottom-left) with a
command-line editor showing `:` followed by the edtui input. The `:` prefix is
rendered as static text, not part of the editor content. The status bar label
(bottom-right) shows `COMMAND`.

### Keybindings

From **Control mode**:

| Key | Action             |
| --- | ------------------ |
| `:` | Enter Command mode |

From **Command mode**:

| Key     | Action                           |
| ------- | -------------------------------- |
| `Enter` | Execute command, exit to Control |
| `Esc`   | Cancel command, exit to Control  |

### Supported commands

Start with a minimal set:

| Command | Action |
| ------- | ------ |
| `q`     | Quit   |

### Changes

In `tui/src/main.rs`:

1. **Add `Mode::Command`.** Fourth variant in the `Mode` enum.

2. **Command editor state.** Separate `EditorState` and `EditorEventHandler` for
   the command line. Created fresh on each `:` press. Same single-line config as
   the URL editor (no newline keybindings).

3. **`:` keybinding in Control mode.** Creates a new command `EditorState`, sets
   it to Insert mode, switches to `Mode::Command`.

4. **Command mode key handling.** `Enter` extracts the command text and
   dispatches it. `Esc` cancels and returns to Control. Everything else forwards
   to the command editor.

5. **Command dispatch.** Match on the extracted command string: `"q"` quits.
   Unknown commands are ignored (return to Control silently for now).

6. **Command line rendering.** In `Mode::Command`, render the command editor in
   the status bar hints area with a `:` prefix. Use the same `EditorTheme` as
   the URL editor but without a border.

7. **Status bar label.** Add `Mode::Command` arm: `"\u{F120} COMMAND"` (terminal
   icon).

8. **Remove `q` from Control mode.** Quit is now `:q` only.

## Experiment 1: URL bar title label

### Hypothesis

Adding a "URL" title to the top-left of the URL bar block establishes the
labeling pattern that will later switch to "COMMAND" when command mode is added.

### Changes

In `tui/src/main.rs`:

1. **Add title to URL bar block.** Use `.title_top("URL")` on the URL bar
   `Block` in both the Edit and non-Edit rendering branches. Style it to match
   the border color of the current mode.

### Test

1. Launch TUI — URL bar shows "URL" in top-left corner
2. Press `Esc` to Control — title still shows, styled in cyan
3. Press `i` to Edit — title still shows, styled in purple
4. Press `Enter` to Browse — title still shows, styled in dim border color

### Result

Pass. "URL" title appears in the top-left of the URL bar in all three modes,
styled to match the border color (dim in Browse, cyan in Control, purple in
Edit).
