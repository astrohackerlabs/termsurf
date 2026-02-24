# Issue 637: Editable URL Bar

## Goal

Make the URL bar in the `web` TUI editable using edtui, a Vim-inspired text
editor widget for ratatui. Users should be able to edit the URL with full Vim
keybindings and press Enter to navigate.

## Background

The URL bar (`tui/src/main.rs`) is currently a read-only `Paragraph` widget that
displays the current URL. It updates via `CompositorMessage::UrlChanged` from
the compositor but cannot be edited by the user. There is no way to navigate to
a new URL by typing — the only navigation is via links clicked in the browser
pane.

[edtui](https://github.com/preiter93/edtui) is a Vim-inspired text editor widget
for ratatui. It provides full Vim keybindings (normal, insert, visual modes),
horizontal scrolling for long lines, undo/redo, cursor management, and a
customizable key handler. A local copy lives at `vendor/edtui/`.

## Current state

- **URL bar**: `Paragraph` widget in `layout[0]`, displays `url: String`
- **Modes**: `Browse` and `Control` (enum at line 24)
- **Mode switching**: `Esc` exits Browse, `Enter` enters Browse
- **Key dispatch**: lines 136–155, per-mode match
- **Compositor sync**: `send_set_overlay()` sends URL, `UrlChanged` receives it
- **No text input state**: no cursor, no edit buffer, no text input crate

## Mode design

This is the hardest part. The TUI currently has two modes. Adding an editable
URL bar introduces a third mode that itself contains Vim sub-modes. The
transitions must feel natural to a Vim user.

### Current modes

```
Browse ──Esc──> Control ──Enter──> Browse
```

- **Browse**: Keys forwarded to Chromium. URL bar is read-only.
- **Control**: Keys handled by TUI. Can quit (`q`), enter Browse (`Enter`).

### New mode: UrlEdit

```
Browse ──Esc──> Control ──e──> UrlEdit ──Enter──> Browse
                    ^                       │
                    └───────Esc (normal)─────┘
```

- **UrlEdit**: Keys handled by edtui. URL bar is editable with Vim keybindings.

Inside UrlEdit, edtui manages its own Vim modes (Normal, Insert, Visual). The
TUI does not need to track these — edtui handles them internally. The TUI only
needs to intercept two keys at the boundary:

- **Enter** (from any edtui mode): Navigate to the edited URL. Switch to Browse.
- **Escape when edtui is in Normal mode**: Cancel edit. Switch to Control.

The Escape key is context-dependent:

- If edtui is in Insert or Visual mode, Escape returns to edtui Normal mode
  (standard Vim behavior — edtui handles this internally).
- If edtui is already in Normal mode, Escape exits UrlEdit and returns to
  Control mode (the TUI intercepts this).

This is natural Vim behavior. In Vim, pressing Escape in Normal mode does
nothing harmful. Here, it means "I'm done editing, go back."

### Entering UrlEdit

From Control mode, pressing `e` enters UrlEdit. The edit buffer is initialized
with the current URL. edtui starts in Normal mode with the cursor at the end.

A typical flow:

1. Press Esc to leave Browse and enter Control
2. Press `e` to enter UrlEdit (edtui Normal mode, cursor at end of URL)
3. Press `A` to append (edtui Insert mode)
4. Type the new URL
5. Press Enter to navigate (switches to Browse)

Or to edit a URL already in the bar:

1. Esc → `e` → use `w`/`b`/`h`/`l` to navigate → `ciw` to change a word → type →
   Enter

Or to cancel:

1. Esc → `e` → edit some text → Esc (back to edtui Normal) → Esc (back to
   Control)

### Mode summary

| Mode    | URL bar          | Keys go to | Enter           | Escape                    |
| ------- | ---------------- | ---------- | --------------- | ------------------------- |
| Browse  | read-only        | Chromium   | —               | → Control                 |
| Control | read-only        | TUI        | → Browse        | —                         |
| UrlEdit | editable (edtui) | edtui      | navigate+Browse | → edtui Normal or Control |

## edtui configuration

### Single-line mode

edtui has no built-in single-line mode. We enforce it by:

1. **Removing newline keybindings** from the `KeyEventHandler`:
   - Remove Enter → `LineBreak(1)` (Insert mode)
   - Remove `o` → `AppendNewline(1)` (Normal mode)
   - Remove `O` → `InsertNewline(1)` (Normal mode)
2. **Rebinding Enter** to trigger navigation (handled by the TUI, not edtui).
3. **Patching `insert_char`** in edtui's `helper.rs` to strip `\n` from pasted
   text. This prevents multi-line pastes from creating new lines.
4. **Setting `wrap(false)`** on `EditorView` to enable horizontal scrolling
   instead of line wrapping.

### Horizontal scrolling

With `wrap(false)`, edtui's `update_viewport_horizontal()` in `state/view.rs`
shifts the viewport to keep the cursor visible. Long URLs that don't fit in the
bar will scroll left/right as the cursor moves. This works out of the box.

### Theme

edtui's `EditorView` accepts a `theme()` to match the Tokyo Night palette used
by the TUI.

## Rendering

In UrlEdit mode, replace the `Paragraph` widget with edtui's `EditorView` widget
in `layout[0]`. The `EditorView` renders the edit buffer with cursor and handles
all display internally (cursor position, horizontal scroll, selection
highlighting).

In Browse and Control modes, continue rendering the read-only `Paragraph` as
before.

## Navigation

When Enter is pressed in UrlEdit mode:

1. Extract the edited URL from edtui's `EditorState`
2. Update the TUI's `url` string
3. Send the new URL to the compositor (either via `send_set_overlay()` with the
   new URL, or via a new `send_navigate()` XPC action)
4. Switch to Browse mode and notify the compositor (`send_mode_changed(true)`)

## Dependencies

Add edtui as a dependency in `tui/Cargo.toml`. Use the crates.io version or a
path dependency to `vendor/edtui/` if we need to patch `insert_char`.
