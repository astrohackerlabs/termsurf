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

# Experiment 649: Tmux Window Refresh Notifications

## Description

Port the tmux viewer notification slice that refreshes window state without
entering pane synchronization.

Experiments 647 and 648 added viewer startup, command sequencing, tmux version
parsing, and `list-windows` parsing. The viewer still ignores all
non-command-output notifications once it reaches `CommandQueue`. Upstream's
`nextCommand` handles a few of those notifications before pane work:
`%session-changed` resets the viewer and starts a fresh `list-windows` flow,
`%window-add` queues `list-windows`, and several bookkeeping notifications are
ignored.

This experiment should implement that notification behavior only. It must not
port `layoutChanged`, `syncLayouts`, pane creation/pruning, pane output, PTY
writes, or App/Surface runtime integration.

## Changes

1. Extend `TmuxViewer::next_command_queue` to handle:
   - `ControlNotification::SessionChanged { id, .. }`;
   - `ControlNotification::WindowAdd { .. }`;
   - ignored notifications: `WindowRenamed`, `WindowPaneChanged`,
     `SessionsChanged`, `ClientDetached`, `ClientSessionChanged`, `Output`, and
     `LayoutChange`.
2. Add a helper for queueing one or more commands while already in
   `CommandQueue`:
   - append commands to the existing queue;
   - emit the newly queued command immediately only when the queue was empty
     before queueing, matching upstream's `command_consumed = queue.empty()`
     behavior;
   - do not emit a second command when another command is already in flight.
3. Implement command-queue `%session-changed` behavior:
   - update `session_id`;
   - clear stored windows;
   - clear pending commands;
   - preserve the stored tmux version;
   - emit `TmuxViewerAction::Windows(Vec::new())` so callers can clear their
     window state;
   - queue `ListWindows` and emit its command after the empty windows action.
4. Implement `%window-add` behavior:
   - ignore the specific window ID for now, matching upstream's full refresh;
   - queue `ListWindows`;
   - if no command is currently in flight, emit the `list-windows` command;
   - if another command is in flight, only append `ListWindows` and wait for the
     in-flight command output before emitting it.
5. Keep these upstream behaviors explicitly out of scope:
   - `layoutChanged` parsing/updating;
   - `syncLayouts`;
   - pane map creation and pruning;
   - queuing `PaneHistory`, `PaneVisible`, and `PaneState` from layouts;
   - pane output handling;
   - PTY writes and App/Surface runtime integration.
6. Add tests for:
   - session-changed in `CommandQueue` clears windows, preserves tmux version,
     records the new session ID, emits empty windows, and emits `list-windows`;
   - session-changed clears pending commands before queueing the new
     `ListWindows`;
   - window-add with an empty queue emits `list-windows`;
   - window-add with a non-empty queue appends `ListWindows` without emitting it
     until the in-flight command output is consumed;
   - ignored notifications do not change viewer state, windows, or queue length;
   - layout-change is explicitly ignored in this experiment.
7. Keep the README's overall `tmux` checklist item unchecked, refining it after
   the result to say window-refresh notifications are done while full viewer
   state, panes, PTY, and App integration remain missing.
8. Update this experiment file with result and review records.

## Verification

- `cargo test -p roastty terminal::tmux`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/649-tmux-window-refresh-notifications.md`
- compare/read the Rust notification handling against:
  - `vendor/ghostty/src/terminal/tmux/viewer.zig` `nextCommand` notification
    cases
  - `vendor/ghostty/src/terminal/tmux/viewer.zig` `sessionChanged`
  - `vendor/ghostty/src/terminal/tmux/viewer.zig` `windowAdd`
- `git diff --check`

Pass = Roastty's standalone tmux viewer refreshes the window list on
session/window-add notifications, preserves queue ordering, records empty-window
session resets, and leaves layout/pane/runtime integration open.

Fail = notifications are still silently ignored when they should refresh
windows, commands are emitted while another command is in flight, session reset
loses the tmux version, layout/pane behavior is added prematurely, or the README
overclaims full tmux support.

## Design Review

Codex design review session `019e9ad7-04a6-7b20-823a-fa6e3d24129f` found no
blocking issues and approved the experiment for implementation. The reviewer
confirmed that the plan matches upstream's usable notification slice:
`SessionChanged` clears windows and queues a fresh `ListWindows`, `WindowAdd`
queues a full refresh with correct in-flight command behavior, ignored
notifications remain no-ops, `LayoutChange` is intentionally out of scope, and
`syncLayouts`, panes, PTY writes, and App/Surface integration remain open.

## Result

**Result:** Pass

Implemented command-queue notification handling in
`roastty/src/terminal/tmux.rs`. `SessionChanged` now records the new session ID,
clears stored windows, clears pending commands, preserves the stored tmux
version, emits an empty `Windows` action, and queues/emits a fresh `ListWindows`
command. `WindowAdd` queues a full `ListWindows` refresh and emits it
immediately only when no command is already in flight.

The intended no-op notifications now stay explicit at the viewer boundary:
`WindowRenamed`, `WindowPaneChanged`, `SessionsChanged`, `ClientDetached`,
`ClientSessionChanged`, `Output`, and `LayoutChange` do not alter command-queue
state in this experiment. `LayoutChange` remains a deliberate future slice
because upstream's handler crosses into layout synchronization and pane
management.

Verification performed:

- `cargo fmt -p roastty`
- `cargo test -p roastty terminal::tmux` — 91 passed, 0 failed

Source comparison was against `vendor/ghostty/src/terminal/tmux/viewer.zig`
`nextCommand`, `sessionChanged`, and `windowAdd`.

## Completion Review

Codex completion review session `019e9ad7-04a6-7b20-823a-fa6e3d24129f` found no
blocking issues and approved the completed experiment. The reviewer confirmed
that command-queue `SessionChanged` records the new session, clears windows and
pending commands, preserves `tmux_version`, emits empty `Windows`, then emits
`ListWindows`; `WindowAdd` queues `ListWindows` and emits immediately only when
the queue was empty; ignored notifications remain no-ops; and `LayoutChange`,
`syncLayouts`, pane management, PTY, App, and Surface integration remain out of
scope.

The reviewer also ran:

- `cargo test -p roastty terminal::tmux` — 91 passed
- `cargo fmt -p roastty -- --check`
- `prettier --check ... README.md ... 649-tmux-window-refresh-notifications.md`
- `git diff --check`

## Conclusion

Roastty's standalone tmux viewer now refreshes window snapshots on session
changes and new-window notifications without starting pane synchronization. The
next tmux experiment should port either `layoutChanged` as a window-layout-only
update or begin the `syncLayouts` pane-state boundary.
