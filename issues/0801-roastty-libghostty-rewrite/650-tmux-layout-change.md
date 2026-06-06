+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
session = "019e9ad7-04a6-7b20-823a-fa6e3d24129f"
verdict = "approved"
+++

# Experiment 650: Tmux Layout Change

## Description

Port the tmux viewer `LayoutChange` notification as a window-layout-only update.

Experiment 649 left `LayoutChange` explicitly ignored because upstream's handler
crosses into `syncLayouts`. The first half of upstream `layoutChanged` has a
useful smaller boundary: find the existing window, parse the checked layout,
update that window's layout, and emit a `windows` action. This experiment should
implement only that pre-`syncLayouts` behavior.

It must not create/prune panes, queue pane capture/state commands, process pane
output, write to the PTY, or integrate with App/Surface runtime code.

## Changes

1. Extend command-queue `ControlNotification::LayoutChange` handling in
   `roastty/src/terminal/tmux.rs`:
   - find the existing `TmuxWindow` by `window_id`;
   - if the window is unknown, ignore the notification and emit no action;
   - parse the notification's `layout` with `Layout::parse_with_checksum`;
   - on parse failure, move the viewer to `Defunct` and emit `Exit`, matching
     upstream command-output error handling for failed layout parsing;
   - update the stored window layout;
   - emit `TmuxViewerAction::Windows(self.windows.clone())`;
   - do not use `visible_layout` or `raw_flags` yet.
2. Preserve command queue sequencing:
   - `LayoutChange` does not consume the in-flight command;
   - if a command is already queued/in flight, do not emit a command;
   - if the queue is empty, do not invent a new command.
3. Keep these upstream behaviors explicitly out of scope:
   - `syncLayouts`;
   - pane map creation and pruning;
   - queuing `PaneHistory`, `PaneVisible`, and `PaneState` from changed layouts;
   - pane output handling;
   - PTY writes and App/Surface runtime integration.
4. Add tests for:
   - known-window layout change updates only that window and emits the full
     window snapshot;
   - unknown-window layout change is ignored;
   - invalid checked layout defuncts and emits `Exit`;
   - layout change does not consume or emit pending queued commands;
   - `visible_layout` and `raw_flags` are ignored for now.
5. Keep the README's overall `tmux` checklist item unchecked, refining it after
   the result to say layout-change window updates are done while pane sync, PTY,
   and App integration remain missing.
6. Update this experiment file with result and review records.

## Verification

- `cargo test -p roastty terminal::tmux`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/650-tmux-layout-change.md`
- compare/read the Rust layout-change handling against:
  - `vendor/ghostty/src/terminal/tmux/viewer.zig` `layoutChanged`
  - `vendor/ghostty/src/terminal/tmux/viewer.zig` `syncLayouts` boundary
- `git diff --check`

Pass = Roastty's standalone tmux viewer updates known-window layouts from
`LayoutChange`, emits the updated window snapshot, keeps unknown windows as
no-ops, defuncts on invalid layouts, and leaves pane/layout synchronization and
runtime integration open.

Fail = invalid layouts are silently accepted, unknown windows mutate state,
queued commands are consumed or emitted incorrectly, pane sync is implemented
prematurely, or the README overclaims full tmux support.

## Design Review

Codex design review session `019e9ad7-04a6-7b20-823a-fa6e3d24129f` found no
blocking issues and approved the experiment for implementation. The reviewer
confirmed that the plan matches the pre-`syncLayouts` upstream boundary: known
windows update and emit a full `Windows` snapshot, unknown windows are ignored,
invalid layouts defunct the viewer, queued commands are not consumed or emitted,
`visible_layout` and `raw_flags` are ignored, and pane/runtime integration
remains out of scope.
