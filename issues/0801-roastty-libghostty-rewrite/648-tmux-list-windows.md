+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
session = "019e9ad7-04a6-7b20-823a-fa6e3d24129f"
verdict = "approved"

[review.result]
agent = "codex"
session = "019e9ad7-04a6-7b20-823a-fa6e3d24129f"
verdict = "approved"
+++

# Experiment 648: Tmux List Windows

## Description

Port the next tmux viewer slice: parsing `list-windows` command output into a
typed viewer window snapshot and emitting that snapshot to callers.

Experiment 647 introduced the standalone viewer startup state machine and queued
`TmuxVersion` followed by `ListWindows`, but deliberately consumed `ListWindows`
output without parsing it. Upstream `receivedListWindows` parses each non-empty
line using the `list_windows` format, validates the tmux layout checksum,
creates `Window` records, emits a `windows` action, and then calls
`syncLayouts`. This experiment should stop before `syncLayouts`: pane creation,
pane command queuing, pane pruning, PTY writes, and App/Surface integration
remain later work.

## Changes

1. Extend `roastty/src/terminal/tmux.rs` with a typed window snapshot:
   - `TmuxWindow { id, width, height, layout }`;
   - `TmuxViewerAction::Windows(Vec<TmuxWindow>)`;
   - a `windows: Vec<TmuxWindow>` field on `TmuxViewer`;
   - test accessors for the current window list if needed.
2. Add list-windows parsing helpers:
   - split command output on `\n`;
   - trim ASCII `space`, `tab`, and `\r` from each line;
   - skip empty lines;
   - parse each non-empty line with
     `parse_output_values(LIST_WINDOWS_VARIABLES, line, LIST_WINDOWS_DELIMITER)`;
   - require the parsed values to match `SessionId`, `WindowId`, `WindowWidth`,
     `WindowHeight`, `WindowLayout`;
   - ignore the parsed session ID for now, matching upstream's current
     `receivedListWindows` behavior;
   - parse `WindowLayout` with `Layout::parse_with_checksum`;
   - return an error for UTF-8, output-format, type-shape, or layout parse
     failures.
3. Wire `TmuxCommand::ListWindows` in `received_command_output`:
   - successful parse replaces `self.windows`;
   - emit `TmuxViewerAction::Windows(self.windows.clone())`;
   - then emit the next queued command if one exists, preserving command queue
     sequencing;
   - malformed non-empty list-windows output moves the viewer to `Defunct` and
     emits `Exit`, matching upstream command-output error handling;
   - empty output replaces the current window list with an empty list and emits
     `Windows(Vec::new())`.
4. Keep these upstream behaviors explicitly out of scope:
   - `syncLayouts`;
   - pane map creation and pruning;
   - queuing `PaneHistory`, `PaneVisible`, and `PaneState` from layout panes;
   - `WindowAdd`, `WindowRenamed`, `WindowPaneChanged`, and session reset flows;
   - PTY writes and App/Surface runtime integration.
5. Add tests for:
   - startup flow parses a valid single-window `ListWindows` output and emits a
     `Windows` action;
   - multiple windows and trailing blank lines are parsed in order;
   - empty list-windows output clears existing windows and emits an empty
     `Windows` action;
   - invalid output format defuncts and emits `Exit`;
   - invalid layout checksum defuncts and emits `Exit`;
   - successful `ListWindows` output emits the next queued command when one is
     queued after it;
   - stored viewer windows match the last emitted snapshot.
6. Keep the README's overall `tmux` checklist item unchecked, refining it after
   the result to say list-windows parsing is done while full viewer state, PTY,
   and App integration remain missing.
7. Update this experiment file with result and review records.

## Verification

- `cargo test -p roastty terminal::tmux`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/648-tmux-list-windows.md`
- compare/read the Rust list-windows parser against:
  - `vendor/ghostty/src/terminal/tmux/viewer.zig` `receivedListWindows`
  - `vendor/ghostty/src/terminal/tmux/viewer.zig` `syncLayouts` boundary to
    confirm what remains out of scope
  - `vendor/ghostty/src/terminal/tmux/output.zig`
- `git diff --check`

Pass = Roastty's standalone tmux viewer parses `list-windows` output into typed
windows, emits a windows action, stores the snapshot, and keeps pane/layout sync
and runtime integration open.

Fail = malformed list-windows output is silently ignored, valid windows are not
stored/emitted, command queue ordering changes, pane sync is implemented
prematurely, or the README overclaims full tmux viewer support.

## Design Review

Codex design review session `019e9ad7-04a6-7b20-823a-fa6e3d24129f` found no
blocking issues and approved the experiment for implementation. The reviewer
confirmed that the plan ports typed window snapshots, `Windows` actions,
`LIST_WINDOWS_VARIABLES` parsing, `Layout::parse_with_checksum`, malformed
output defunct behavior, and empty-output clearing while keeping `syncLayouts`,
pane state, PTY writes, and App/Surface integration out of scope.

## Result

**Result:** Pass

Implemented `TmuxWindow` snapshots and `TmuxViewerAction::Windows` in
`roastty/src/terminal/tmux.rs`. `TmuxViewer` now stores the current window
snapshot and handles `TmuxCommand::ListWindows` output by parsing each non-empty
line with `LIST_WINDOWS_VARIABLES`, validating the checked layout with
`Layout::parse_with_checksum`, replacing the stored snapshot, and emitting a
`Windows` action.

Successful list-windows output preserves command queue sequencing: any command
queued after `ListWindows` is emitted after the `Windows` action. Empty
list-windows output clears existing windows and emits an empty snapshot.
Malformed output, invalid UTF-8, invalid field shapes, and invalid layout
checksums defunct the viewer and emit `Exit`, matching upstream command-output
error handling.

The intended upstream boundary remains intact. This experiment does not call or
port `syncLayouts`, does not create/prune pane state, does not queue pane
history/visible/state commands from layouts, and does not integrate with PTY,
App, or Surface runtime code.

Verification performed:

- `cargo fmt -p roastty`
- `cargo test -p roastty terminal::tmux` — 86 passed, 0 failed

Source comparison was against `vendor/ghostty/src/terminal/tmux/viewer.zig`
`receivedListWindows` and the `syncLayouts` boundary, plus
`vendor/ghostty/src/terminal/tmux/output.zig`.

## Completion Review

Codex completion review session `019e9ad7-04a6-7b20-823a-fa6e3d24129f` found no
blocking issues and approved the completed experiment. The reviewer confirmed
that `TmuxWindow`, `TmuxViewerAction::Windows`, `LIST_WINDOWS_VARIABLES`
parsing, `Layout::parse_with_checksum`, snapshot storage/emission, empty-output
clearing, malformed-output defunct behavior, and command sequencing all match
the plan while `syncLayouts`, panes, PTY, App, and Surface integration remain
out of scope.

The reviewer also ran:

- `cargo test -p roastty terminal::tmux` — 86 passed
- `cargo fmt -p roastty -- --check`
- `prettier --check ... README.md ... 648-tmux-list-windows.md`
- `git diff --check`

## Conclusion

Roastty's standalone tmux viewer can now parse and publish the initial window
list snapshot. The next tmux experiment should decide whether to port the
`syncLayouts` pane-state boundary or first handle window/session notifications
that refresh the window list.
