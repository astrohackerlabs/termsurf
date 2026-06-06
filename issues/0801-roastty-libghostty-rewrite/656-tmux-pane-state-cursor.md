# Experiment 656: Tmux Pane State Cursor

## Description

Experiment 655 added a typed parser for `PaneState`/`list-panes` output but left
viewer dispatch as a no-op. Upstream Ghostty's `receivedPaneState` first applies
cursor-related state before terminal modes, mouse modes, scroll regions, and tab
stops.

This experiment applies the first safe subset of parsed pane state: cursor
position and cursor shape for each tracked pane. It should parse the full
`PaneState` output, ignore stale pane IDs, choose the target screen from
`alternate_on`, set in-bounds cursor coordinates using tmux's 0-based
coordinates, and map cursor shape text `block`, `underline`, and `bar` to
Roastty cursor visual styles. It must leave cursor visibility/blinking, terminal
modes, mouse modes, scroll region, tab stops, and alternate saved cursor
restoration for later experiments.

## Changes

- `roastty/src/terminal/terminal.rs`
  - Add a narrow tmux-facing helper to apply cursor position and cursor visual
    style to a requested primary or alternate screen.
  - Mutate the requested existing screen directly without switching the
    terminal's active screen and without allocating/initializing an alternate
    screen. Upstream uses `t.screens.get(screen_key)` for pane state; it does
    not switch screens while applying cursor state.
  - Ignore cursor coordinates that do not fit in the pane terminal dimensions,
    matching upstream's stale/MAX_INT guard.
  - Still apply a valid cursor shape when cursor coordinates are out of bounds;
    upstream's bounds guard skips only `cursorAbsolute`, then continues to shape
    mapping.
  - Ignore empty, `default`, and unknown cursor shape text.
- `roastty/src/terminal/tmux.rs`
  - Route `TmuxCommand::PaneState` command output through `parse_pane_states`.
  - Apply cursor position and cursor shape to tracked pane terminals.
  - Ignore pane state lines for unknown panes.
  - Treat malformed state output as a viewer-defunct condition.
  - Preserve command-queue continuation after successful pane state handling.
- Tests in `roastty/src/terminal/tmux.rs`
  - Verify primary pane state sets cursor position and cursor shape.
  - Verify alternate pane state applies to the alternate screen without changing
    the primary screen, switching the active screen, or allocating an alternate
    screen when none exists.
  - Verify stale pane IDs are ignored while later valid lines are still applied.
  - Verify out-of-bounds cursor coordinates are ignored without changing the
    previous cursor position, while a valid cursor shape on that same state line
    is still applied.
  - Verify `default` and unknown cursor shapes leave the previous visual style
    unchanged.
  - Verify malformed state output defuncts the viewer.
  - Verify successful pane state handling emits the next queued command.

## Verification

- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/656-tmux-pane-state-cursor.md`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `cargo test -p roastty terminal::tmux`
- `git diff --check`

## Design Review

**Result:** Not approved on first review.

Codex found two blockers. First, the terminal helper must explicitly mutate an
existing requested screen without switching active screen state or allocating
the alternate screen, because upstream uses `t.screens.get(screen_key)` rather
than switching screens. Second, the out-of-bounds cursor-position test also
needs to prove cursor shape is still applied, because upstream skips only
`cursorAbsolute` and continues with shape mapping. Both findings were valid and
the design was revised to include those constraints and tests.

**Re-review result:** Approved.

Codex confirmed the previous blockers were resolved and the revised helper
constraints now match upstream cursor behavior. It suggested, non-blockingly,
that alternate-screen verification may be clearer as separate fixtures for an
existing alternate screen and a missing alternate screen.

## Result

**Result:** Pass.

Roastty now routes `TmuxCommand::PaneState` output through the typed pane-state
parser and applies the cursor subset to tracked pane terminals. For each parsed
state line, stale pane IDs are ignored, `alternate_on` selects primary or
alternate, in-bounds tmux 0-based cursor coordinates are applied, and cursor
shape text maps `block`, `underline`, and `bar` to Roastty cursor visual styles.

The terminal helper mutates the requested existing screen directly. It does not
switch the active screen and does not allocate/initialize an alternate screen
when pane state refers to an absent alternate screen. Out-of-bounds cursor
coordinates are ignored without returning early, so a valid cursor shape on the
same state line is still applied. Empty, `default`, and unknown shapes leave the
previous cursor style unchanged.

Malformed pane-state output now defuncts the viewer, while successful state
handling preserves command-queue continuation. Cursor visibility/blinking,
terminal modes, mouse modes, scroll region, tab stops, and alternate saved
cursor restoration remain future pane-state work.

Verification passed:

- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/656-tmux-pane-state-cursor.md`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `cargo test -p roastty terminal::tmux` — 121 passed, 0 failed
- `git diff --check`

## Conclusion

Pane state application now has the cursor-position and cursor-shape subset wired
with the upstream no-switch/no-allocation behavior. The next tmux experiment
should apply the next state subset, most likely cursor visibility/blinking and
basic terminal modes, before moving on to mouse modes, scroll regions, tab
stops, and alternate saved cursor restoration.

## Completion Review

**Result:** Approved.

Codex found no blocking issues. It confirmed `PaneState` routes through
`parse_pane_states`, malformed output defuncts the viewer, stale pane IDs are
skipped, successful handling continues the queue, and the terminal helper
mutates the requested existing screen without switching active screens or
allocating an absent alternate screen.

Codex suggested strengthening the default/unknown shape test so it starts from a
non-default cursor style. That suggestion was applied: the test now sets
`underline` first, then verifies `default`, unknown, and empty shapes preserve
`Underline`.
