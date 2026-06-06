# Experiment 655: Tmux Pane State Parser

## Description

Tmux pane bootstrap now captures pane history and visible content, then queues a
`PaneState` command. Applying pane state directly is a larger step because
upstream restores cursor positions, cursor style, terminal modes, mouse modes,
focus/bracketed-paste flags, scroll regions, and tab stops.

This experiment builds the typed parser for `list-panes` output only. It should
turn each non-empty `PaneState` output line into a `TmuxPaneState` struct using
the existing `LIST_PANES_VARIABLES` order and `parse_output_values` helper. It
does not apply the parsed state to `Terminal` yet; that remains the next
experiment.

## Changes

- `roastty/src/terminal/tmux.rs`
  - Add a `TmuxPaneState` struct with typed fields for every value emitted by
    `TmuxCommand::PaneState`.
  - Add a parser for one `list-panes` output line.
  - Add a parser for full `PaneState` command output that trims blank lines,
    parses each non-empty line, and returns all parsed pane states.
  - Keep `TmuxCommand::PaneState` command output as a no-op in viewer dispatch
    for this experiment; application to tracked terminals is future work.
- Tests in `roastty/src/terminal/tmux.rs`
  - Verify a representative `PaneState` line parses into the expected typed
    state, including prefixed pane ID, booleans, cursor shape text, cursor
    colour text, scroll region values, and tab-stop text.
  - Verify multiline output trims blank lines and carriage returns.
  - Verify blank-only output returns an empty state list.
  - Verify malformed state lines fail without partial success.
  - Verify viewer dispatch still consumes `PaneState` output and emits the next
    queued command without mutating tracked panes.

## Verification

- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/655-tmux-pane-state-parser.md`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `cargo test -p roastty terminal::tmux`
- `git diff --check`

## Design Review

**Result:** Approved.

Codex found no blocking issues. It agreed the parser-only scope is appropriate
because upstream `receivedPaneState` mutates many terminal fields and is too
large for one safe step. It also confirmed keeping viewer `PaneState` dispatch
as a no-op is coherent as long as command-queue continuation is preserved.

The review suggested two additional tests: assert `cursor_colour` in the
representative parser fixture and add a blank-only output case. Both suggestions
were added to the design.

## Result

**Result:** Pass.

Roastty now has a typed `TmuxPaneState` parser for `PaneState`/`list-panes`
output. The parser uses the existing `LIST_PANES_VARIABLES` order and
`parse_output_values` helper, trims blank lines and carriage returns, preserves
text fields such as cursor shape, cursor colour, and pane tab stops, and fails
the full parse if any non-empty line is malformed.

Viewer dispatch intentionally still treats `TmuxCommand::PaneState` as a no-op
for terminal state application. The new dispatch test locks down the intended
intermediate behavior: the command output is consumed, the next queued command
is emitted, and tracked pane terminal content is not mutated yet.

Verification passed:

- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/655-tmux-pane-state-parser.md`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `cargo test -p roastty terminal::tmux` — 114 passed, 0 failed
- `git diff --check`

## Conclusion

Pane state output now has a tested typed parser. The next tmux experiment should
route `TmuxCommand::PaneState` through this parser and apply a first safe subset
of state to tracked pane terminals, likely cursor position and cursor visibility
before broader mode, mouse, scroll-region, and tab-stop restoration.

## Completion Review

**Result:** Approved.

Codex found no blocking or non-blocking issues. It confirmed `TmuxPaneState`
covers every value in `LIST_PANES_VARIABLES`, `parse_pane_state` consumes fields
in command-format order, full-output parsing trims blank lines and carriage
returns consistently with upstream, malformed non-empty lines fail the parse,
and viewer dispatch remains intentionally parser-only/no-op while preserving
command queue continuation.
