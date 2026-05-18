+++
status = "closed"
opened = "2026-02-26"
closed = "2026-02-26"
+++

# Issue 646: Normal and Insert Modes

## Goal

Fix three problems with the TUI's edit mode:

1. **Show the real mode name.** The status bar (bottom right) displays "EDIT"
   for all edtui sub-modes. It should show "NORMAL" when in Vim normal mode and
   "INSERT" when in Vim insert mode, each with an appropriate Nerd Font glyph.

2. **Enter insert mode directly.** Pressing `i` (changed from `e`) from control
   mode should enter insert mode, not normal mode. Users want to type
   immediately.

3. **Fix Ctrl+Esc exit.** The hint bar promises `<ctrl+esc>` exits to control
   mode, but the keybinding is never handled. Pressing Ctrl+Esc does nothing. It
   should exit from either insert mode or normal mode back to control mode.

## Current state

### Mode enum

`tui/src/main.rs:27-32` defines three TUI modes:

```rust
enum Mode {
    Browse,
    Control,
    UrlEdit,
}
```

`UrlEdit` is a single mode that covers all edtui sub-modes (Normal, Insert,
Visual, Search). The TUI doesn't distinguish between them.

### Mode transitions

```
Browse ──Esc──> Control ──e──> UrlEdit ──Enter──> Browse
                    ^                                │
                    └── ctrl+esc (NOT IMPLEMENTED) ──┘
```

### Entering edit mode

`tui/src/main.rs:163-172` — pressing `e` in control mode:

```rust
KeyCode::Char('e') => {
    editor_state = EditorState::new(Lines::from(url.as_str()));
    let len = url.len();
    editor_state.cursor = edtui::Index2::new(0, len.saturating_sub(1));
    mode = Mode::UrlEdit;
    // ...
}
```

`EditorState::new()` always initializes in Normal mode
(`vendor/edtui/src/state.rs:69-83`). The user lands in normal mode and must
press `i` again to start typing.

### Key dispatch in UrlEdit

`tui/src/main.rs:181-200`:

```rust
Mode::UrlEdit => match key.code {
    KeyCode::Enter => {
        // Extract URL, navigate, switch to Browse.
    }
    _ => {
        // Pass everything else to edtui (including Escape).
        editor_handler.on_key_event(key, &mut editor_state);
    }
},
```

Enter is intercepted by the TUI. Everything else goes to edtui. There is no
check for Ctrl+Esc before the mode match.

### Status bar label

`tui/src/main.rs:430-434`:

```rust
let label = match mode {
    Mode::Browse => "\u{F059F} BROWSE",
    Mode::Control => "\u{F11C} CONTROL",
    Mode::UrlEdit => "\u{F040} EDIT",
};
```

All edtui sub-modes show the same "EDIT" label.

### Hint bar

`tui/src/main.rs:418-427` shows `<ctrl+esc> control` as a hint in UrlEdit mode,
but no code handles this keybinding. The global key handler
(`tui/src/main.rs:147-150`) only handles Ctrl+C.

### edtui modes

`vendor/edtui/src/state/mode.rs:1-23` defines four editor modes:

```rust
pub enum EditorMode {
    Normal,
    Insert,
    Visual,
    Search,
}
```

The current mode is stored in `editor_state.mode` and is readable by the TUI at
any time.

## Problems

### Problem 1: Mode label doesn't reflect edtui sub-mode

The label always shows "EDIT". It should read `editor_state.mode` and display:

- Normal mode → appropriate glyph + "NORMAL"
- Insert mode → appropriate glyph + "INSERT"

Need to find the most fitting Nerd Font glyphs for each.

### Problem 2: `e` enters normal mode instead of insert mode

`EditorState::new()` starts in Normal mode. The keybinding is `e`. Both should
change:

- Keybinding: `e` → `i` (mnemonic: insert)
- After creating the editor state, set `editor_state.mode = EditorMode::Insert`
  so the user can type immediately
- Update the hint bar in Control mode: `<e> edit url` → `<i> edit url`

### Problem 3: Ctrl+Esc doesn't exit edit mode

The hint bar shows `<ctrl+esc> control` but no code handles it. Need to add a
Ctrl+Esc check that:

- Works from any edtui sub-mode (normal, insert, visual)
- Switches the TUI mode back to Control
- Notifies the compositor via `send_mode_changed`
- Is checked before keys are dispatched to edtui

## Key files

- `tui/src/main.rs` — mode enum, key dispatch, status bar rendering
- `vendor/edtui/src/state/mode.rs` — EditorMode enum
- `vendor/edtui/src/state.rs` — EditorState struct and initialization

## Experiments

### Experiment 1: Change keybinding from `e` to `i`

**Goal:** Change the keybinding that enters edit mode from `e` to `i`, and enter
insert mode directly so the user can type immediately.

Two changes in `tui/src/main.rs`:

1. **Line 163** — change `KeyCode::Char('e')` to `KeyCode::Char('i')`.
2. **Line 168** — after `mode = Mode::UrlEdit;`, add
   `editor_state.mode = EditorMode::Insert;` so edtui starts in insert mode
   instead of normal mode. Requires `use edtui::EditorMode;` if not already
   imported.
3. **Line 410** — change the hint bar text from `"e"` to `"i"`.

**Result: Pass.** Keybinding changed, editor starts in insert mode. Also updated
`docs/keybindings.md` to document the `i` keybinding and UrlEdit mode.

### Experiment 2: Fix Ctrl+Esc exit from UrlEdit

**Goal:** Make Ctrl+Esc exit UrlEdit mode (from either insert or normal) back to
Control mode. Currently Ctrl+Esc is shown in the hint bar but never handled.

One change in `tui/src/main.rs`. Add a Ctrl+Esc check between the global Ctrl+C
handler (line 148) and the mode match (line 152):

```rust
// Ctrl+Esc returns to Control from any mode (Issue 646).
if key.code == KeyCode::Esc && key.modifiers.contains(KeyModifiers::CONTROL) {
    mode = Mode::Control;
    if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
        conn.send_mode_changed(pid, false);
    }
    continue;
}
```

This intercepts Ctrl+Esc before edtui ever sees it. It works from any mode —
Browse, UrlEdit (insert or normal) — and always lands in Control. The `continue`
skips the per-mode match so the key isn't double-handled.

From Browse mode, Ctrl+Esc duplicates plain Esc (both go to Control). That's
fine — it's consistent and harmless.

**Result: Fail.** Ctrl+Esc is not received by the TUI at all. The key
combination doesn't trigger a crossterm `KeyCode::Esc` with
`KeyModifiers::CONTROL`. The code is correct but the terminal never delivers the
event. Need to investigate how Ctrl+Esc is encoded by the terminal emulator.

### Experiment 3: Fix Ctrl+Esc in the GUI

**Goal:** Make Ctrl+Esc return to Control mode from UrlEdit (insert or normal).

#### Why Experiment 2 failed

Ctrl+Esc never reaches the TUI because the GUI intercepts it in
`gui/src/Surface.zig:2740-2747`:

```zig
// Ctrl+Esc exits browse mode (Issue 607 Experiment 1).
if (event.key == .escape and event.mods.ctrl and event.action == .press) {
    const xpc = @import("apprt/xpc.zig");
    if (xpc.isOverlayForwarding(self)) {
        xpc.notifyNonOverlayClicked(self);
        return .consumed;
    }
}
```

`isOverlayForwarding` (`xpc.zig:625-631`) returns `true` only when
`p.browsing == true`. When the TUI is in UrlEdit mode, it already sent
`mode_changed(browsing: false)`, so `isOverlayForwarding` returns `false`. The
Ctrl+Esc check falls through, and the key continues down the normal Ghostty key
processing pipeline — it never reaches the TUI as a terminal key event.

#### The fix

The Ctrl+Esc check should not be gated on `isOverlayForwarding`. It should fire
whenever there is an overlay pane, regardless of the browsing state. When
`browsing` is already `false` (UrlEdit mode), Ctrl+Esc should still send
`mode_changed(browsing: false)` to the TUI so it can reset to Control mode.

Two changes in `gui/src/Surface.zig:2740-2747`:

1. Replace the `isOverlayForwarding` check with a check that just verifies the
   surface has an overlay pane (use `surface_to_pane` lookup, not
   `isOverlayForwarding`).
2. Always send the mode change to the TUI and consume the key.

`notifyNonOverlayClicked` (`xpc.zig:665-673`) won't work as-is because it
early-returns when `p.browsing` is already `false`. We need a new function or
inline logic that:

- If `p.browsing` is `true`: set it to `false`, send `mode_changed(false)` to
  TUI, send `focus_changed(false)` to Chromium (same as today).
- If `p.browsing` is `false`: just send `mode_changed(false)` to TUI (so the TUI
  resets from UrlEdit to Control). No need to send `focus_changed` to Chromium
  since it's already unfocused.

In both cases, return `.consumed` so the key doesn't continue down the pipeline.

### Experiment 4: Investigate Ctrl+Esc terminal encoding

**Goal:** Understand why the TUI doesn't receive Ctrl+Esc through crossterm when
not in browse mode, and fix it.

#### What the GUI actually does

The existing Ctrl+Esc handler in `Surface.zig:2740-2747` is correct for browse
mode — it intercepts Ctrl+Esc before it reaches the Chromium forwarding code.
When not in browse mode, Ctrl+Esc falls through to normal key processing.

The key pipeline after the Ctrl+Esc check:

1. **Line 2749-2756** — Forward to Chromium if `isOverlayForwarding`. Not
   browsing, so this is skipped.
2. **Line 2790** — `maybeHandleBinding`. No default Ghostty binding for
   Ctrl+Esc, so this is skipped.
3. **Line 2893** — `encodeKey` → calls `legacy()` → calls
   `pcStyleFunctionKey()`.

#### The encoding

`gui/src/input/function_keys.zig:226` defines Ctrl+Esc as:

```
.{ .mods = .{ .ctrl = true }, .sequence = "\x1b[27;5;27~" }
```

This is the xterm `modify_other_keys` CSI 27 sequence. Ghostty encodes it and
sends it to the PTY. The bytes `\x1b[27;5;27~` DO reach the TUI process.

#### The problem

crossterm likely doesn't parse `\x1b[27;5;27~` as `KeyCode::Esc` with
`KeyModifiers::CONTROL`. It may drop it, misparse it, or emit it as an
unrecognized sequence.

#### The fix

Two options:

1. **Parse it in the TUI.** Read raw bytes from the PTY and match
   `\x1b[27;5;27~` manually before passing to crossterm's event parser. This is
   fragile and bypasses crossterm's design.

2. **Handle Ctrl+Esc in the GUI for all TUI panes.** The GUI already intercepts
   Ctrl+Esc in browse mode and sends `mode_changed` via XPC. Extend this to also
   fire when `browsing` is false — but still only when the surface has an
   overlay pane (i.e. a `web` TUI is running). This is the same approach as
   Experiment 3 but with the correct gate: check for pane existence, not
   forwarding state.

   In non-browse mode, Ctrl+Esc should not be consumed — it should be sent as an
   XPC message AND allowed to flow through to the terminal. This way the TUI
   receives the mode change via XPC (reliable) while other TUIs or programs in
   the terminal can still see the key.

   Wait — that's wrong too. Returning `.consumed` prevents the key from reaching
   the PTY. We can't both consume and not consume.

   The cleanest fix: send the XPC `mode_changed(false)` message and return
   `.consumed`. The TUI already handles `ModeChanged` at `main.rs:210-215`.
   Non-TermSurf terminals won't have a pane registered, so the gate is harmless.

Approach 2 is correct. Same as Experiment 3 but with the right understanding:
the GUI MUST intercept Ctrl+Esc for panes because the terminal encoding isn't
reliably parsed by crossterm. The `hasOverlayPane` gate is necessary — not to
protect non-overlay TUIs (who would receive the raw bytes), but because without
a pane there's no XPC connection to send to.

Changes (same three as Experiment 4, re-attempted with correct rationale):

**1. `gui/src/apprt/xpc.zig`** — add `hasOverlayPane`:

```zig
pub fn hasOverlayPane(surface: *CoreSurface) bool {
    return surface_to_pane.get(@intFromPtr(surface)) != null;
}
```

**2. `gui/src/apprt/xpc.zig`** — add `notifyCtrlEsc`:

```zig
pub fn notifyCtrlEsc(surface: *CoreSurface) void {
    const pane_id_key = surface_to_pane.get(@intFromPtr(surface)) orelse return;
    const p = panes.get(pane_id_key) orelse return;

    if (p.browsing) {
        p.browsing = false;
        sendFocusChanged(pane_id_key, false);
    }
    sendModeToWeb(p, false);
}
```

**3. `gui/src/Surface.zig:2740-2747`** — replace gate:

```zig
if (xpc.hasOverlayPane(self)) {
    xpc.notifyCtrlEsc(self);
    return .consumed;
}
```

**Result: Pass.** Ctrl+Esc now returns to Control mode from both insert and
normal mode.

### Experiment 5: Mode label reflects edtui sub-mode

**Goal:** Replace the static "EDIT" label in the bottom-right status bar with
the actual edtui mode name and a distinct Nerd Font glyph for each.

#### Glyphs

edtui has four modes. Proposed glyphs:

| Mode   | Glyph      | Unicode    | Name               | Rationale                               |
| ------ | ---------- | ---------- | ------------------ | --------------------------------------- |
| Normal | (terminal) | `\u{EA85}` | nf-cod-terminal    | Command/control mode                    |
| Insert | (pencil)   | `\u{F040}` | nf-fa-pencil       | Writing/editing (reuses old EDIT glyph) |
| Visual | (checkbox) | `\u{F14A}` | nf-fa-square-check | Selection                               |
| Search | (search)   | `\u{F002}` | nf-fa-search       | Magnifying glass                        |

#### Changes

One change in `tui/src/main.rs:431-435`. Replace:

```rust
let label = match mode {
    Mode::Browse => "\u{F059F} BROWSE",
    Mode::Control => "\u{F11C} CONTROL",
    Mode::UrlEdit => "\u{F040} EDIT",
};
```

With a match that reads `editor_state.mode` when in UrlEdit:

```rust
let label = match mode {
    Mode::Browse => "\u{F059F} BROWSE".to_string(),
    Mode::Control => "\u{F11C} CONTROL".to_string(),
    Mode::UrlEdit => match editor_state.mode {
        EditorMode::Normal => "\u{EA85} NORMAL".to_string(),
        EditorMode::Insert => "\u{F040} INSERT".to_string(),
        EditorMode::Visual => "\u{F14A} VISUAL".to_string(),
        EditorMode::Search => "\u{F002} SEARCH".to_string(),
    },
};
```

The `Paragraph::new()` call accepts `String`, so changing from `&str` to
`String` via `.to_string()` is fine.

**Result: Pass.** Status bar now shows NORMAL, INSERT, VISUAL, or SEARCH with
distinct glyphs when in UrlEdit mode.

### Experiment 6: Don't intercept Enter in Search mode

**Goal:** Fix Enter key dispatch so it passes through to edtui when in Search
mode instead of triggering navigation.

#### The bug

`tui/src/main.rs:182-196` always intercepts Enter in UrlEdit mode:

```rust
Mode::UrlEdit => match key.code {
    KeyCode::Enter => {
        // Extract URL, navigate, switch to Browse.
    }
    _ => {
        editor_handler.on_key_event(key, &mut editor_state);
    }
},
```

edtui's Search mode uses Enter to execute the search and return to Normal mode
(`vendor/edtui/src/events/key.rs:153-154`). But Enter never reaches edtui
because the TUI intercepts it first.

#### The fix

One change in `tui/src/main.rs:183`. Add a guard so Enter only triggers
navigation when edtui is NOT in Search mode:

```rust
KeyCode::Enter if editor_state.mode != EditorMode::Search => {
```

When edtui is in Search mode, Enter falls through to the `_` arm and reaches
edtui, which executes the search and switches back to Normal mode.

**Result: Pass.** Enter in Search mode now executes the search instead of
triggering navigation.

## Conclusion

All three goals are met across six experiments:

- **Keybinding and insert mode** (Exp 1): `i` enters UrlEdit in insert mode so
  the user can type immediately. No more double-pressing `i`.
- **Ctrl+Esc exit** (Exp 2–4): Ctrl+Esc returns to Control from any edtui
  sub-mode. Experiment 2 failed because crossterm can't parse the terminal
  encoding (`\x1b[27;5;27~`). The fix lives in the GUI (`Surface.zig`), which
  intercepts Ctrl+Esc and sends `mode_changed(false)` via XPC.
- **Mode labels** (Exp 5): The status bar shows NORMAL, INSERT, VISUAL, or
  SEARCH with distinct Nerd Font glyphs instead of a generic "EDIT".
- **Search Enter** (Exp 6): Enter passes through to edtui in Search mode instead
  of triggering navigation.
