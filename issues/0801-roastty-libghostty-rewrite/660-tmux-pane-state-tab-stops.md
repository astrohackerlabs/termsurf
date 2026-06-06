+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5"
reasoning = "medium"
+++

# Experiment 660: Tmux Pane State Tab Stops

## Description

Experiment 659 restored the tmux pane-state vertical scroll-region subset.
Upstream Ghostty's next pane-state step restores tab stops from the
comma-separated `pane_tabs` field.

This experiment applies the parsed `pane_tabs` field to tracked pane terminals.
The restore path should clear all current tab stops first, then set each valid
0-based column listed by tmux. Invalid tokens, overflowing values, and columns
outside the pane width should be ignored without defuncting the viewer.

Alternate saved cursor restoration, live pane output, PTY writes, and App
integration remain out of scope.

## Changes

- `roastty/src/terminal/terminal.rs`
  - Add a narrow tmux-facing helper to restore tab stops from a pane-state
    `pane_tabs` string.
  - Clear all existing tab stops before parsing, matching upstream's
    `t.tabstops.reset(0)` behavior.
  - Split on commas, parse each token as a `usize`, ignore parse failures and
    out-of-range columns, and set valid 0-based columns.
  - Treat an empty `pane_tabs` string as "clear all tab stops".
- `roastty/src/terminal/tmux.rs`
  - Call the tab-stop helper after cursor, mode, mouse, and scroll-region
    pane-state restoration.
  - Add test-only pane helpers for setting and reading tab stops without
    exposing the private `Tabstops` type.
  - Preserve existing behavior for malformed pane-state output, stale pane IDs,
    and command-queue continuation.
  - Extend the pane-state fixture support so tests can supply explicit
    `pane_tabs` strings while keeping the other fields at focused defaults.
- Tests in `roastty/src/terminal/tmux.rs`
  - Verify pane-state tab stops clear existing stops and set the listed valid
    columns.
  - Verify an empty `pane_tabs` string clears all tab stops.
  - Verify invalid tokens, overflowing numeric values, and columns outside the
    pane width are ignored while valid entries still apply.
  - Verify `alternate_on = true` still restores terminal-wide tab stops rather
    than making tab stops screen-local or dependent on the active screen.
  - Verify stale pane IDs do not apply tab-stop changes while a later valid pane
    state line still does.
  - Keep malformed pane-state output and command-queue continuation coverage in
    the tmux pane-state test set.

## Design Review

**Result:** Approved with one low-risk test addition.

Codex found no blocking issues. It confirmed that clearing all tab stops before
parsing, treating an empty `pane_tabs` string as clear-all, and ignoring
invalid, overflowing, or out-of-range entries match upstream Ghostty's tmux
viewer.

Codex suggested adding an explicit `alternate_on = true` regression test because
upstream chooses the target screen only for cursor/style restoration; tab stops
are terminal-wide. Roastty also stores tab stops on `Terminal`, not `Screen`,
but the design now requires a test to guard that boundary.

## Verification

- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/660-tmux-pane-state-tab-stops.md`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `cargo test -p roastty terminal::tmux`
- `git diff --check`

## Result

**Result:** Pass.

Roastty now restores tmux pane-state tab stops after cursor, mode, mouse, and
scroll-region restoration. The new terminal helper clears all current tab stops
first, then parses the comma-separated `pane_tabs` string and sets valid 0-based
columns. Empty `pane_tabs` clears all stops, and invalid tokens, overflowing
numeric values, and columns outside the pane width are ignored without
defuncting the viewer.

The tmux tests use pane-level test helpers to seed and inspect tab stops without
exposing the private `Tabstops` type. Coverage proves valid restore, clear-all
behavior, invalid/out-of-range entry skipping, stale pane handling with a later
valid line, and the design-review-requested `alternate_on = true` case showing
tab stops remain terminal-wide rather than screen-local.

Verification passed:

- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/660-tmux-pane-state-tab-stops.md`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `cargo test -p roastty terminal::tmux` — 136 passed, 0 failed
- `git diff --check`

## Conclusion

Pane-state restoration now covers cursor state, non-mouse modes, mouse modes,
vertical scroll regions, and tab stops. The remaining parsed pane-state field
from upstream's current restore block is alternate saved cursor position; after
that, tmux work can move toward live pane output, PTY writes, and App
integration.

## Completion Review

**Result:** Approved.

Codex found no issues. It confirmed that `Terminal::apply_tmux_tabstops_state`
clears all tab stops first, parses comma-separated `usize` values, ignores
invalid or overflowing tokens, and sets only columns inside the pane width. It
also confirmed that `TmuxViewer` calls the helper after scroll-region
restoration, matching upstream ordering and terminal-wide tab-stop handling.

Codex judged the tests sufficient for this slice: valid restore, empty
`pane_tabs` clear-all behavior, invalid/overflowing/out-of-range entries, stale
pane isolation with later valid application, and the `alternate_on = true`
terminal-wide regression case. It also confirmed the recorded result and
conclusion accurately describe the implementation and remaining pane-state work.
