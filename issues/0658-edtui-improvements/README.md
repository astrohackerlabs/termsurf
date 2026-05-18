+++
status = "closed"
opened = "2026-02-27"
closed = "2026-02-27"
+++

# Issue 658: EdTUI Improvements

Better modes and keybindings for the TUI URL bar editor.

## Problem

Two issues with the current URL bar editing:

1. **No submode concept.** The TUI has `Mode::UrlEdit` but treats it as a single
   flat mode. The editor internally has Normal, Insert, Visual, and Search
   submodes, but the TUI doesn't model this hierarchy. There's no visual
   indicator in the URL bar showing which editor submode is active.

2. **Limited keybindings.** Only `i` enters the editor (always in Insert mode at
   end of URL). The editor state is recreated each time, losing cursor position.
   There's no way to enter Normal or Visual mode directly from Control mode.

## Solution

### Mode hierarchy

Rename `Mode::UrlEdit` to `Mode::Edit`. The TUI now has three top-level modes:

| TUI Mode | Description                                                |
| -------- | ---------------------------------------------------------- |
| Browse   | Viewport is active, keys go to Chromium                    |
| Control  | URL bar is focused, keys are TUI commands (q, i, A, Enter) |
| Edit     | URL bar is being edited, keys go to edtui editor           |

When the TUI is in **Edit mode**, all keypresses route to edtui, which manages
its own submodes: Normal, Insert, Visual, Search. The TUI stays in Edit mode
regardless of which editor submode is active. `Esc` always goes to edtui (e.g.,
Insert → Normal). `Ctrl+Esc` exits Edit → Control from any submode.

### Submode indicator

Add a mode label in the top-right corner of the URL bar block, matching the
pattern used for the profile name in the viewport container
(`tui/src/main.rs:363`). Shows NORMAL, INSERT, VISUAL, or SEARCH in purple.

### Persistent editor state

Stop recreating `EditorState` on every `i` press. Initialize it once (and when
the URL changes externally via navigation). The cursor position persists across
Control ↔ Edit transitions.

### New keybindings from Control mode

| Key | Action                                     |
| --- | ------------------------------------------ |
| `i` | Enter Edit/Insert, cursor at last position |
| `A` | Enter Edit/Insert, cursor at end of line   |
| `I` | Enter Edit/Insert, cursor at start of line |
| `n` | Enter Edit/Normal, cursor at last position |
| `v` | Enter Edit/Visual, cursor at last position |
| `V` | Enter Edit/Visual, entire line selected    |

All six are supported by the edtui API:

- **Mode setting**: `state.mode = EditorMode::Insert` (or Normal, Visual)
- **Cursor at end**: `state.cursor.col = state.lines.len_col(0).unwrap_or(0)`
  (Insert mode allows past-end)
- **Cursor at start**: `state.cursor = Index2::new(0, 0)`
- **Line selection**: `SelectLine.execute(&mut state)` — sets Visual mode with
  `line_mode = true`
- **Visual init**: `SwitchMode(EditorMode::Visual).execute(&mut state)` —
  creates empty selection at cursor

### Changes

In `tui/src/main.rs`:

1. **Rename mode.** `Mode::UrlEdit` → `Mode::Edit` throughout.

2. **Persistent editor state.** Remove the `EditorState::new(...)` call from the
   `i` keypress handler. Instead, sync editor content from URL only when the URL
   changes (external navigation, initial load).

3. **Ctrl+Esc in Edit mode.** Intercept `Ctrl+Esc` before passing to edtui: exit
   Edit → Control. Plain `Esc` always forwards to edtui.

4. **New keybindings.** Add `A`, `I`, `n`, `v`, `V` handlers in the
   `Mode::Control` match arm, each setting the appropriate editor mode/cursor
   and switching to `Mode::Edit`.

5. **Submode indicator.** In the Edit rendering branch, add a
   `.title_top(mode_label.alignment(Alignment::Right))` to the URL bar block,
   showing the current `EditorMode` as a colored label.

## Experiment 1: Submodes, persistent state, new keybindings

### Hypothesis

Persistent editor state with six entry keybindings, an inline submode indicator,
and proper Esc routing will make URL editing feel like a proper vim buffer.

### Test

1. Launch TUI, press `Esc` to Control, press `i` — Edit/Insert, cursor at end
2. Type some text, press `Esc` — editor goes to Normal (still in Edit mode)
3. Press `Ctrl+Esc` — exits Edit → Control
4. Press `i` — Edit/Insert, cursor where you left it (not reset)
5. Press `Ctrl+Esc` to Control, press `A` — Edit/Insert, cursor at end of line
6. Press `Ctrl+Esc` to Control, press `I` — Edit/Insert, cursor at start
7. Press `n` — Edit/Normal, cursor at last position
8. Press `v` — Edit/Visual, empty selection at cursor
9. Press `V` — Edit/Visual, entire line selected
10. In all Edit submodes, top-right of URL bar shows NORMAL/INSERT/VISUAL/SEARCH

### Result

Pass. All six keybindings work. Persistent editor state preserves cursor
position across Control ↔ Edit transitions. Purple submode indicator shows in
the top-right of the URL bar. Ctrl+Esc exits Edit → Control from any submode.

## Experiment 2: Fix mode indicators

### Problem

Two issues from Experiment 1:

1. The URL bar submode indicator shows plain text (`NORMAL`, `INSERT`, etc.)
   with no Nerd Font icon. Every other mode label in the TUI has an icon.
2. The status bar (bottom-right) shows the editor submode instead of `EDIT`. The
   status bar should always say `EDIT` when the TUI is in Edit mode. Only the
   URL bar shows submodes.

### Changes

In `tui/src/main.rs`:

1. **URL bar submode label** — Add Nerd Font icons to each submode, reusing the
   same icons currently on the status bar labels:
   - `\u{EA85}` NORMAL
   - `\u{F040}` INSERT
   - `\u{F14A}` VISUAL
   - `\u{F002}` SEARCH

2. **Status bar label** — Change the `Mode::Edit` arm from a submode match to a
   single `"\u{F044} EDIT"` label (pencil-square icon).

### Test

1. Press `i` from Control — status bar says `EDIT`, URL bar top-right says
   `INSERT`
2. Press `Esc` — status bar still says `EDIT`, URL bar says `NORMAL`
3. Press `v` — URL bar says `VISUAL`, status bar still `EDIT`
4. All four submodes show their icon in the URL bar

### Result

Pass. Status bar shows `EDIT` in all editor submodes. URL bar top-right shows
the submode with its Nerd Font icon. However, copy/paste has a bug — see
Experiment 3.

## Experiment 3: Fix line-mode yank newline

### Problem

edtui's line-mode yank prepends a newline to copied text. `CopyLine` (`yy`) does
`String::from('\n') + &line`, and line-mode visual selection (`V` then `y`)
inserts an empty row at index 0. Both cause a phantom newline at the start of
pasted text. For a single-line URL editor, this means pasting a yanked URL into
another application produces `\nhttp://example.com` instead of
`http://example.com`.

### Solution

edtui exposes `ClipboardTrait` and `EditorState::set_clipboard()`. Implement a
custom clipboard wrapper that strips leading newlines in `set_text` before
writing to the system clipboard. This intercepts all clipboard writes without
modifying vendored edtui source.

### Changes

In `tui/src/main.rs`:

1. **Custom clipboard struct** — `UrlClipboard` wraps `arboard::Clipboard`,
   implementing edtui's `ClipboardTrait`. `set_text` strips leading newlines.
   `get_text` passes through unchanged.

2. **Set clipboard on editor state** — After creating `EditorState`, call
   `editor_state.set_clipboard(UrlClipboard::new())` to install the wrapper.

### Test

1. Press `i` from Control, type a URL, press `Esc`
2. Press `yy` — yank the line
3. Paste into an external application — no leading newline
4. Press `V` then `y` — visual line yank
5. Paste into an external application — no leading newline
6. Press `v`, select part of the URL, press `y`
7. Paste — no leading newline, only selected text

### Result

Pass. Custom `UrlClipboard` strips leading newlines from edtui's line-mode
yanks. All three copy methods (`yy`, `Vy`, `v` selection `y`) paste cleanly
without phantom newlines. No changes to vendored edtui source.

## Conclusion

The TUI URL bar editor now feels like a proper vim buffer. Three experiments
delivered:

1. **Mode hierarchy** — `Mode::UrlEdit` renamed to `Mode::Edit` with submodes
   (Normal, Insert, Visual, Search) managed by edtui. The TUI tracks a single
   Edit mode; edtui handles submodes internally. `Ctrl+Esc` exits Edit → Control
   from any submode.

2. **Six vim keybindings** — `i`, `A`, `I` (Insert), `n` (Normal), `v`, `V`
   (Visual) enter Edit mode with the expected cursor position and selection
   state. Persistent `EditorState` preserves cursor across mode transitions.

3. **Mode indicators** — Status bar shows `EDIT` (with pencil icon) for all
   editor submodes. URL bar top-right shows the active submode with its Nerd
   Font icon in purple.

4. **Clean clipboard** — Custom `UrlClipboard` strips leading newlines from
   edtui's line-mode yanks, so copied URLs paste cleanly into external
   applications. No vendored edtui changes required.

Next: Issue 659 adds vim-style command mode (`:q`, `:w`, etc.) as a new TUI mode
with its own edtui instance.
