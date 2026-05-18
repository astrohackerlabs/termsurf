+++
status = "closed"
opened = "2026-02-27"
closed = "2026-02-27"
+++

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

## Experiment 2: Command bar UI

### Hypothesis

A fourth TUI mode (`Mode::Command`) with its own edtui instance, rendered in a
yellow command bar that replaces the URL bar, will establish the command mode
UI. No commands are executed — `Enter` simply exits back to Control. State is
discarded on exit.

### Changes

Add a Tokyo Night yellow constant:

```rust
const YELLOW: Color = Color::Rgb(0xe0, 0xaf, 0x68);
```

In `tui/src/main.rs`:

1. **Add `Mode::Command` to the enum.** Fourth variant.

2. **Command editor state.** Add `cmd_state: EditorState` and
   `cmd_handler: EditorEventHandler` as separate variables. `cmd_state` is
   created fresh each time `:` is pressed — state is not preserved across
   invocations. Same single-line config as the URL editor (no newline
   keybindings). Uses `UrlClipboard` for clean yanks.

3. **`:` keybinding in Control mode.** Creates a fresh `EditorState` in Insert
   mode, switches to `Mode::Command`.

4. **Command mode key handling.** Same pattern as Edit mode: `Ctrl+Esc` exits
   Command → Control. `Enter` (when not in Search submode) exits Command →
   Control. No command dispatch — all input is discarded on exit. `Esc` and all
   other keys forward to `cmd_handler` (so `Esc` goes from Insert → Normal, not
   back to Control).

5. **Command bar rendering.** When in `Mode::Command`, render a command bar in
   `layout[1]` (replacing the URL bar). The command bar is a yellow-bordered
   block titled "COMMAND". Inside the block's inner area, use
   `Layout::horizontal` to split into a 2-char prefix area (`:`) and the
   remaining space for the command edtui. The `:` is rendered as a static
   `Paragraph`, styled in yellow. The edtui editor renders in the remaining
   space without its own block (the outer block provides the border).

6. **Submode indicator.** Same as the URL bar: top-right of the command bar
   block shows NORMAL/INSERT/VISUAL/SEARCH with Nerd Font icons, styled in
   yellow.

7. **Border color.** Add `Mode::Command => (YELLOW, BORDER)` to the border color
   match.

8. **Status bar.** Add `Mode::Command` arms:
   - Label: `"\u{F120} COMMAND"` (terminal icon)
   - Hints: `<enter> execute  <ctrl+esc> control`

### Test

1. Launch TUI, press `Esc` to Control
2. Press `:` — URL bar replaced by yellow-bordered command bar titled "COMMAND",
   with `:` prefix and cursor after it, status bar says `COMMAND`
3. Top-right of command bar shows `INSERT` with icon in yellow
4. Type some text, press `Esc` — editor goes to Normal (still in Command mode),
   top-right shows `NORMAL`
5. Press `Ctrl+Esc` — exits Command → Control, URL bar reappears
6. Press `:` again — command bar appears with empty input (state not preserved)
7. Type some text, press `Enter` — exits to Control, input discarded
8. Verify URL editor state is unaffected by command mode input
9. Press `q` — TUI quits (still works from Control mode)
10. Relaunch, press `i` to Edit — URL editor still works, cursor preserved

### Result

Pass. Yellow command bar replaces the URL bar when `:` is pressed. Submode
indicators display with Nerd Font icons in yellow. `Ctrl+Esc` and `Enter` both
exit to Control. State is discarded between invocations. URL editor state is
unaffected.

## Experiment 3: Command dispatch with prefix matching

### Hypothesis

A static command table with prefix-based matching will provide vim-style
unambiguous command resolution, letting `:q` match `:quit` without requiring the
full word.

### Changes

In `tui/src/main.rs`:

1. **`CommandResult` enum.** Represents the outcome of a command:
   - `Quit` — exit the TUI
   - `None` — unknown or ambiguous command (no-op)

2. **`Command` struct.** Static command definition with a name and a handler
   function: `fn(args: &[&str]) -> CommandResult`.

3. **`COMMANDS` table.** `const` slice of commands. Starts with one entry:
   - `quit` — returns `CommandResult::Quit`

4. **`dispatch` function.** Splits input into prefix + args. Filters `COMMANDS`
   by `name.starts_with(prefix)`. If exactly one match, calls its handler. If
   zero or multiple matches (ambiguous), returns `CommandResult::None`.

5. **Wire into Command mode.** On `Enter`, extract the command text from
   `cmd_state`, call `dispatch`, and match on the result:
   - `Quit` — break the event loop
   - `None` — return to Control silently

### Test

1. Press `:`, type `quit`, press `Enter` — TUI quits
2. Relaunch, press `:`, type `q`, press `Enter` — TUI quits (prefix match)
3. Relaunch, press `:`, type `qu`, press `Enter` — TUI quits
4. Relaunch, press `:`, type `nonsense`, press `Enter` — returns to Control
5. Relaunch, press `:`, press `Enter` (empty input) — returns to Control

### Result

Pass. Prefix matching works: `:q`, `:qu`, `:qui`, `:quit` all quit. Unknown and
empty commands return to Control silently. Static command table is extensible.

## Experiment 4: Remove q shortcut, update hints

### Hypothesis

Removing the bare `q` shortcut from Control mode and updating the status bar
hints to show `:q` will make the quit workflow consistent with command mode.

### Changes

In `tui/src/main.rs`:

1. **Remove `KeyCode::Char('q') => break`** from the `Mode::Control` match arm.

2. **Update Control mode hints.** Replace `<q> quit` with `<:q> quit`.

### Test

1. Launch TUI, press `Esc` to Control
2. Press `q` — nothing happens
3. Status bar hints show `<:q> quit` instead of `<q> quit`
4. Press `:`, type `q`, press `Enter` — TUI quits

### Result

Pass. Bare `q` no longer quits. Hints show `<:q> quit`. Command mode is the only
way to quit (besides `Ctrl+C`).

## Experiment 5: Restyle status bar hints

### Hypothesis

Removing the `<>` wrappers and dimming descriptions instead will free up space,
allow showing full key sequences like `:q↵`, and look cleaner.

### Changes

In `tui/src/main.rs`:

1. **New hint format.** Replace `<key> description` with `key description`,
   where `key` is bright (FG) and `description` is dim (DIM). Separate hints
   with two spaces. Use `↵` to represent Enter in key sequences.

2. **Update all four mode hint lines:**
   - **Browse:** `cmd+[ back  cmd+] fwd  cmd+r reload  ctrl+esc control`
   - **Control:** `:q↵ quit  i edit url  ↵ browse`
   - **Edit:** `↵ navigate  ctrl+esc control`
   - **Command:** `↵ execute  ctrl+esc control`

### Test

1. Launch TUI — no `<>` characters in hints
2. Key sequences are bright, descriptions are dim
3. Control mode shows `:q↵ quit` (full key sequence including Enter)
4. All four modes display correctly with the new style

### Result

Pass. Clean hint style with bright keys and dim descriptions. `:q⏎ quit` shows
the full key sequence. No `<>` wrappers. All four modes render correctly.
