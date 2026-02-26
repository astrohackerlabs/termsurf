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

### Experiment 4: Implement GUI-side Ctrl+Esc fix

**Goal:** Implement the fix designed in Experiment 3.

#### TUI side: already works

`tui/src/main.rs:210-215` handles `ModeChanged { browsing: false }` by setting
`mode = Mode::Control`. No TUI changes needed.

#### GUI side: two changes

**1. New function in `gui/src/apprt/xpc.zig`.**

Add `notifyCtrlEsc` next to `notifyNonOverlayClicked` (after line 673). Unlike
`notifyNonOverlayClicked`, it does not early-return when `browsing` is false:

```zig
/// Called when Ctrl+Esc is pressed. Always returns to control mode,
/// regardless of the current browsing state (Issue 646).
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

**2. Update `gui/src/Surface.zig:2740-2747`.**

Replace `isOverlayForwarding` with `isOverlayBrowsing` — no, that still checks
`p.browsing`. Instead, use a simple pane existence check. The simplest approach:
call `notifyCtrlEsc` which handles both cases, and gate only on whether the
surface has a pane at all.

Replace:

```zig
if (xpc.isOverlayForwarding(self)) {
    xpc.notifyNonOverlayClicked(self);
    return .consumed;
}
```

With:

```zig
if (xpc.hasOverlayPane(self)) {
    xpc.notifyCtrlEsc(self);
    return .consumed;
}
```

This requires a third small addition — `hasOverlayPane` in `xpc.zig`:

```zig
/// Returns true if the surface has an overlay pane registered.
pub fn hasOverlayPane(surface: *CoreSurface) bool {
    return surface_to_pane.get(@intFromPtr(surface)) != null;
}
```
