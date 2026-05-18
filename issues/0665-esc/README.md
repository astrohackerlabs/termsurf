+++
status = "closed"
opened = "2026-02-28"
closed = "2026-02-28"
+++

# Issue 665: Escape Key Navigation

Replace `Ctrl+Esc` with `Esc` for exiting modes in the TUI.

## Problem

The TUI uses `Ctrl+Esc` to exit Browse, Edit, and Command modes back to Control
mode (Issues 658, 659). This is awkward — `Esc` is the standard key for exiting
modes in vim-like editors. However, `Esc` is already used by edtui to switch
from Insert/Visual/Search mode to Normal mode within the editor. Simply mapping
`Esc` to "exit the editor" would prevent users from reaching Normal mode.

The `Ctrl+Esc` intercept lives in the Zig GUI layer, not the Rust TUI.
`Surface.zig` (line ~2741) catches `Ctrl+Esc` and sends an XPC notification to
the TUI via `xpc.notifyCtrlEsc()` — the key is consumed and never reaches the
terminal as a key event. The Rust TUI receives mode-change signals over XPC, not
raw keystrokes. Bare `Esc` passes through Zig to the terminal normally.

## Solution

Make `Esc` context-sensitive based on the editor's current mode:

- **Browse mode**: `Esc` exits to Control mode (replaces `Ctrl+Esc`).
- **Edit mode**: If the editor is in Normal mode, `Esc` exits Edit → Control.
  Otherwise, `Esc` is passed to edtui (which switches Insert/Visual/Search →
  Normal).
- **Command mode**: Same logic as Edit mode — if in Normal mode, `Esc` exits
  Command → Control. Otherwise, edtui handles it.

This matches vim's behavior: pressing `Esc` once enters Normal mode, pressing
`Esc` again exits the context.

### Changes

1. **Zig intercept** (`gui/src/Surface.zig`, line ~2741) — change the `Ctrl+Esc`
   check to bare `Esc`: replace `event.key == .escape and
event.mods.ctrl` with
   `event.key == .escape and !event.mods.ctrl` (or just `event.key == .escape`
   with no modifier requirement). This makes bare `Esc` trigger the XPC
   notification to the TUI instead of `Ctrl+Esc`.

2. **Browse mode** (`tui/src/main.rs`, line ~258) — no change to handler logic
   (already handles the XPC mode-change signal). Update the status bar hint
   (line ~649) from `"\u{2303}esc "` to `"esc "`.

3. **Edit mode** (`tui/src/main.rs`, lines ~352–375) — replace the `Ctrl+Esc`
   check with:
   - If `key.code == KeyCode::Esc` and `editor_state.mode == EditorMode::Normal`
     → exit to Control mode
   - Otherwise, pass the key to edtui (which handles `Esc` for mode switching)

4. **Command mode** (`tui/src/main.rs`, lines ~378–395) — same pattern:
   - If `key.code == KeyCode::Esc` and `cmd_state.mode == EditorMode::Normal` →
     exit to Control mode
   - Otherwise, pass the key to the command editor handler

### Concerns

- **Browser loses Esc** — in Browse mode, `Esc` will no longer reach the
  browser. This means the browser can't use `Esc` to close dialogs, exit
  fullscreen, etc. Acceptable for now since we control the browser integration.
- **Double-tap feel** — users in Insert mode must press `Esc` twice to fully
  exit (once for Normal, once for Control). This is standard vim behavior and
  should feel natural.

## Experiment 1: Replace Ctrl+Esc with bare Esc

### Hypothesis

Changing the Zig intercept from `Ctrl+Esc` to bare `Esc`, updating the Rust TUI
Edit/Command handlers to use context-sensitive `Esc` (Normal mode → exit,
otherwise → edtui), and updating all status bar hints will produce vim-standard
escape behavior across all modes.

### Changes

1. **Zig intercept** — in `gui/src/Surface.zig` (line ~2741), change
   `event.mods.ctrl` to `!event.mods.ctrl` so bare `Esc` (without Ctrl) triggers
   the XPC notification. Rename `notifyCtrlEsc` to `notifyEsc` in both
   `Surface.zig` and `gui/src/apprt/xpc.zig` (line ~681). Update the comments in
   both files.

   Before:

   ```zig
   // Ctrl+Esc always returns to control mode (Issue 646 Experiment 4).
   if (event.key == .escape and event.mods.ctrl and event.action == .press) {
       const xpc = @import("apprt/xpc.zig");
       if (xpc.hasOverlayPane(self)) {
           xpc.notifyCtrlEsc(self);
           return .consumed;
       }
   }
   ```

   After:

   ```zig
   // Esc in browse mode returns to control mode (Issue 665).
   if (event.key == .escape and !event.mods.ctrl and event.action == .press) {
       const xpc = @import("apprt/xpc.zig");
       if (xpc.isOverlayForwarding(self)) {
           xpc.notifyEsc(self);
           return .consumed;
       }
   }
   ```

2. **XPC function** — in `gui/src/apprt/xpc.zig` (line ~681), rename
   `notifyCtrlEsc` to `notifyEsc` and update the doc comment.

   Before:

   ```zig
   /// Called when Ctrl+Esc is pressed. Always returns to control mode,
   /// regardless of the current browsing state (Issue 646).
   pub fn notifyCtrlEsc(surface: *CoreSurface) void {
   ```

   After:

   ```zig
   /// Called when Esc is pressed. Always returns to control mode,
   /// regardless of the current browsing state (Issue 665).
   pub fn notifyEsc(surface: *CoreSurface) void {
   ```

3. **Edit mode** — in `tui/src/main.rs` (lines ~352–375), replace the `Ctrl+Esc`
   check with a context-sensitive `Esc` check. If the editor is in Normal mode,
   `Esc` exits to Control. Otherwise, edtui handles it.

   Before:

   ```rust
   // Ctrl+Esc exits Edit → Control (Issue 658).
   if key.code == KeyCode::Esc && key.modifiers.contains(KeyModifiers::CONTROL)
   {
       mode = Mode::Control;
   } else if key.code == KeyCode::Enter
   ```

   After:

   ```rust
   // Esc in Normal mode exits Edit → Control (Issue 665).
   if key.code == KeyCode::Esc && editor_state.mode == EditorMode::Normal
   {
       mode = Mode::Control;
   } else if key.code == KeyCode::Enter
   ```

4. **Command mode** — in `tui/src/main.rs` (lines ~378–395), same pattern.

   Before:

   ```rust
   // Ctrl+Esc exits Command → Control (Issue 659).
   if key.code == KeyCode::Esc && key.modifiers.contains(KeyModifiers::CONTROL)
   {
       mode = Mode::Control;
   } else if key.code == KeyCode::Enter && cmd_state.mode != EditorMode::Search
   ```

   After:

   ```rust
   // Esc in Normal mode exits Command → Control (Issue 665).
   if key.code == KeyCode::Esc && cmd_state.mode == EditorMode::Normal
   {
       mode = Mode::Control;
   } else if key.code == KeyCode::Enter && cmd_state.mode != EditorMode::Search
   ```

5. **Status bar hints** — in `tui/src/main.rs`, update all three `⌃esc` hints
   (lines ~649, ~663, ~669) from `"\u{2303}esc "` to `"esc "`.

### Test

1. `cd gui && zig build` — compiles without errors
2. `cd tui && cargo build` — compiles without errors
3. In Browse mode, press `Esc` — exits to Control
4. In Browse mode, press `Ctrl+Esc` — does NOT exit (bare Esc only)
5. In Control mode, press `i` to enter Edit/Insert — press `Esc` once → enters
   Normal mode (edtui handles it). Press `Esc` again → exits to Control
6. In Control mode, press `:` to enter Command/Insert — press `Esc` once →
   Normal mode. Press `Esc` again → exits to Control
7. Status bar shows `esc` (not `⌃esc`) in Browse, Edit, and Command modes

### Result

Pass, with one critical fix. The initial implementation used `hasOverlayPane` in
the Zig intercept, which fires for any overlay regardless of mode. This caused
bare Esc to be consumed by Zig in Edit/Command modes too — the key never reached
the terminal, so edtui couldn't handle Insert → Normal transitions. The fix was
changing `hasOverlayPane` to `isOverlayForwarding`, which only returns true when
the pane is in browse mode AND focused. Now:

- **Browse mode**: Zig intercepts Esc (before it reaches Chromium's key
  forwarding), sends XPC exit signal → Control mode.
- **Edit/Command modes**: Zig's `isOverlayForwarding` returns false, so Esc
  passes through to the terminal. Crossterm delivers it to the Rust TUI, where
  the context-sensitive logic checks the editor submode: Normal → exit to
  Control, otherwise → edtui handles the mode switch.

Key lesson: the Zig intercept sits above the Chromium key forwarding block in
`Surface.zig`. Both use early `return .consumed`. The intercept must be scoped
to browse mode only (`isOverlayForwarding`), because in Edit/Command modes the
key forwarding block is already inactive and Esc needs to flow through to the
terminal naturally.

## Conclusion

One experiment replaced `Ctrl+Esc` with context-sensitive `Esc` across all
modes. The Zig GUI layer intercepts bare Esc only in browse mode (via
`isOverlayForwarding`) to prevent the key from being forwarded to Chromium. In
Edit and Command modes, Esc passes through to the terminal where the Rust TUI
checks the editor submode: Normal mode exits to Control, any other submode lets
edtui handle the transition (Insert → Normal, Visual → Normal, etc.). This
produces standard vim double-tap behavior — one Esc for Normal, another for
exit.
