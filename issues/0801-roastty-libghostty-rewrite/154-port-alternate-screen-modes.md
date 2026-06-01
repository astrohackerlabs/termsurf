# Experiment 154: Port Alternate-Screen Modes

## Description

Port Ghostty's primary/alternate screen switching behavior for DEC modes `47`,
`1047`, `1048`, and `1049`.

Roastty already knows these mode IDs:

- `?47` / `Mode::AltScreenLegacy`
- `?1047` / `Mode::AltScreen`
- `?1048` / `Mode::SaveCursor`
- `?1049` / `Mode::AltScreenSaveCursorClearEnter`

But current runtime behavior only toggles mode bits. The existing
`terminal_stream_csi_deferred_modes_toggle_state_without_faked_side_effects`
test proves the gap: entering/exiting alternate screen modes does not switch
screen storage, clear the alternate screen, save/restore cursor state, or
isolate primary and alternate screen contents.

Upstream Ghostty source references:

- `vendor/ghostty/src/terminal/ScreenSet.zig`:
  - stores `primary` and lazily initialized `alternate` screens;
  - alternate screen uses `max_scrollback = 0`;
  - `switchTo()` changes the active screen key.
- `vendor/ghostty/src/terminal/Terminal.zig`:
  - `switchScreen()` initializes/switches screens, ends hyperlink state on the
    old screen, carries charset state to the new screen, clears selection, and
    marks the terminal for redraw;
  - `switchScreenMode()` implements mode-specific behavior for `47`, `1047`, and
    `1049`;
  - RIS removes the alternate screen and returns to primary.
- `vendor/ghostty/src/terminal/stream_terminal.zig`:
  - mode `47` calls `switchScreenMode(.@"47", enabled)`;
  - mode `1047` calls `switchScreenMode(.@"1047", enabled)`;
  - mode `1049` calls `switchScreenMode(.@"1049", enabled)`;
  - mode `1048` saves cursor on set and restores cursor on reset.

This experiment should replace the deliberate fake-side-effect boundary with the
first real two-screen implementation. It should stay limited to terminal grid
state and cursor behavior; Kitty graphics, selection, renderer callbacks, and
app integration remain out of scope.

## Changes

1. Add screen-set state.
   - Replace `TerminalScreens { active: Screen }` with a primary/alternate
     screen set and an active key.
   - Initialize primary immediately.
   - Lazily initialize alternate on first switch with the same rows/cols and
     `max_scrollback = 0`.
   - Keep formatter, test helpers, and runtime operations pointed at the active
     screen.
   - Add active-screen helper methods to avoid scattering active-key
     conditionals through terminal runtime code.

2. Implement screen switching.
   - If switching to the already active screen, do not replace screen storage.
   - Before switching away from a screen, clear the active cursor hyperlink
     state on the old screen, matching Ghostty's `switchScreen()`.
   - Carry charset state from the old screen to the new active screen.
   - Clear selection only if Roastty already has active selection state in this
     layer; do not invent new selection behavior in this experiment.
   - Mark the newly active visible rows dirty so renderers know the visible grid
     changed.
   - Do not add Kitty graphics dirty handling in this experiment.

3. Implement `?47` behavior.
   - On set: switch to alternate screen.
   - On reset: switch to primary screen.
   - If the screen actually changes, copy cursor state from the old screen to
     the new screen, excluding hyperlink state.
   - Do not clear either screen.
   - Alternate screen content must survive leaving and re-entering `?47`.

4. Implement `?1047` behavior.
   - On set: switch to alternate screen.
   - On reset: if currently on alternate, erase the complete alternate display
     before switching back to primary. This must use the existing erase-display
     content-clear path, not a full screen reset, so cursor/charset/saved-cursor
     state is not accidentally reset.
   - If the screen actually changes, copy cursor state from the old screen to
     the new screen, excluding hyperlink state.
   - Primary screen content must survive.
   - Alternate screen content must be gone after leaving and re-entering
     `?1047`.

5. Implement `?1049` behavior.
   - On set: save the cursor on the currently active screen before switching,
     switch to alternate, erase the complete alternate display on entry, and
     copy the old cursor to alternate if the screen actually changed. The erase
     must use the existing erase-display content-clear path, not a full screen
     reset.
   - On reset: switch to primary and restore the saved primary cursor.
   - Primary screen content must survive.
   - Alternate screen content does not need to survive a later `?1049` entry
     because `?1049` clears on entry.

6. Implement `?1048` behavior.
   - On set: save cursor on the active screen.
   - On reset: restore cursor on the active screen.
   - Do not switch screens.

7. Reset behavior.
   - RIS/full reset must return to primary, reset primary, remove or reset
     alternate storage, reset mode bits, and leave future alternate entry with a
     blank alternate screen.
   - Preserve Experiment 153's `previous_char` reset.

## Verification

Run:

```bash
cargo fmt
cargo test -p roastty alt_screen
cargo test -p roastty save_cursor
cargo test -p roastty ris
cargo test -p roastty
```

Required test coverage:

- Screen-set tests:
  - terminal starts on primary;
  - alternate is initialized lazily;
  - alternate uses zero scrollback capacity;
  - active-screen formatting reads the current active screen;
  - switching screens clears cursor hyperlink state on the old screen;
  - charset state is carried to the new active screen.
- Mode `?47` runtime tests:
  - primary content survives entering and leaving;
  - alternate content survives leaving and re-entering;
  - cursor state is copied on entry and on exit when the active screen changes;
  - re-setting `?47` while already on alternate does not clear alternate
    content.
- Mode `?1047` runtime tests:
  - primary content survives entering and leaving;
  - alternate content is cleared when leaving from alternate;
  - re-entering `?1047` shows a blank alternate screen;
  - cursor state is copied on entry and exit when the active screen changes.
- Mode `?1049` runtime tests:
  - first setting from primary saves the primary cursor before switching;
  - setting switches to alternate and clears alternate on entry;
  - resetting switches to primary and restores the saved primary cursor;
  - primary content survives;
  - a second `?1049` entry clears stale alternate content.
  - re-setting `?1049` while already on alternate clears alternate content but
    does not corrupt the saved primary cursor restored by a later `?1049` reset.
- Mode `?1048` runtime tests:
  - set saves cursor on the active screen;
  - reset restores cursor on the active screen;
  - it works independently on primary and alternate;
  - it does not switch screens.
- Regression tests:
  - existing mode set/reset/report behavior still passes;
  - existing save/restore cursor `ESC 7` / `ESC 8` behavior still passes;
  - existing RIS full reset tests still pass and now also cover active screen
    returning to primary and alternate storage clearing;
  - raw C1 CSI behavior remains unchanged;
  - no public ABI, renderer, PTY, browser overlay, or app behavior changes.

## Non-Negotiable Invariants

- Do not add public ABI or app integration.
- Do not add Linux or other non-macOS platform paths.
- Do not add Kitty graphics, image redraw, renderer callbacks, or selection
  behavior beyond preserving existing state if it already exists.
- Do not implement DECCOLM (`?3`) resizing behavior in this experiment.
- Do not change terminal size, tabstop behavior, PTY behavior, browser overlay
  behavior, or protocol behavior.
- Do not preserve hyperlinks when copying cursor state across screens.
- Do not add `ghostty_*` names. Use Roastty names except when citing upstream
  Ghostty source paths or behavior.
- Run `cargo fmt` and accept its output.

## Failure Criteria

This experiment fails if:

- alternate-screen modes only toggle mode bits without switching storage;
- primary and alternate content are not isolated;
- `?47` clears alternate content;
- `?1047` fails to erase alternate content when leaving from alternate;
- `?1049` fails to save/restore the primary cursor or fails to clear alternate
  on entry;
- `?1047` or `?1049` use full screen reset semantics instead of complete
  erase-display semantics for their mode-specific clears;
- `?1048` switches screens;
- switching screens preserves cursor hyperlink state;
- charset state is lost across screen switches;
- RIS leaves the terminal on alternate screen or leaves stale alternate content
  visible on the next alternate entry;
- the patch adds public ABI, renderer/app behavior, PTY behavior, browser
  overlay behavior, DECCOLM resizing, Kitty graphics, or non-macOS platform
  paths.

## Design Review

Initial Codex review found two real design issues:

- `?1049` set was incorrectly described as always saving the primary cursor;
  Ghostty saves the cursor on the currently active screen before switching.
- Mode-specific clears for `?1047` and `?1049` needed to be explicitly
  erase-display clears, not full screen resets.

The design was updated to match Ghostty's active-screen save behavior, require a
test for re-setting `?1049` while already on alternate, and clarify that `?1047`
/ `?1049` clears use complete erase-display semantics.

Follow-up Codex review approved the design with no findings.

## Result

**Result:** Pass

Implemented real primary/alternate screen storage for Roastty's terminal runtime
and wired DEC private modes `?47`, `?1047`, `?1048`, and `?1049` to
Ghostty-style screen behavior instead of only toggling mode bits.

The implementation now:

- stores primary and lazily initialized alternate screens in `TerminalScreens`;
- initializes the alternate screen with scrollback disabled;
- routes runtime operations, formatters, and test helpers through the active
  screen;
- clears cursor hyperlink state on the old screen when switching;
- carries charset state to the newly active screen;
- copies cursor state across `?47` / `?1047` switches without copying hyperlink
  state;
- preserves `?47` alternate contents across leave/re-enter;
- clears alternate contents for `?1047` on leave using complete erase-display
  semantics;
- saves/restores cursor state for `?1048` on the active screen without
  switching;
- saves the active cursor before `?1049` entry, clears alternate on entry, and
  restores the saved primary cursor on reset;
- makes RIS return to primary, reset primary, and drop stale alternate storage.

The work also corrected zero-scrollback behavior for explicit linefeed and
pending-wrap scrolling paths so the alternate screen can remain history-free.

Verification run:

```bash
cargo fmt
cargo test -p roastty alt_screen
cargo test -p roastty save_cursor
cargo test -p roastty ris
cargo test -p roastty
```

Results:

- `cargo test -p roastty alt_screen`: 8 passed.
- `cargo test -p roastty save_cursor`: 4 passed.
- `cargo test -p roastty ris`: 5 passed.
- `cargo test -p roastty`: 1694 unit tests passed, ABI harness passed, 0 doc
  tests.

## Conclusion

Experiment 154 completes the first real two-screen terminal model in Roastty.
Alternate-screen modes now mutate terminal storage, cursor state, and
erase-display behavior in the same layer as other stream actions, while keeping
public ABI, renderer, PTY, browser overlay, and app integration untouched.

The next experiment can move on to another terminal subsystem; alternate-screen
mode behavior no longer needs to remain deferred.
