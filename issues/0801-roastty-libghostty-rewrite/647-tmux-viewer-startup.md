+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 647: Tmux Viewer Startup

## Description

Port the first tmux viewer state-machine slice: startup and initial command
queue flow.

The tmux helper stack now has control notifications, layout parsing, output
format helpers, DCS entry, and command string formatting. Upstream's viewer
starts after DCS entry by waiting for the initial startup block, waiting for the
first `%session-changed`, then queueing `tmux_version` and `list_windows`. When
the version command output arrives, it stores the tmux version and emits the
next queued command.

This experiment should implement only that viewer startup/command-queue
foundation. It must not parse list-windows output, create windows or panes, sync
layouts, consume pane output, write to the PTY, or integrate with App / Surface.

## Changes

1. Extend `roastty/src/terminal/tmux.rs` with:
   - `TmuxViewer`;
   - `TmuxViewerState::{StartupBlock, StartupSession, CommandQueue, Defunct}`;
   - `TmuxViewerAction::{Exit, Command(String)}`;
   - internal `VecDeque<TmuxCommand>` command queue;
   - `TmuxViewer::new`;
   - `TmuxViewer::next(ControlNotification) -> Vec<TmuxViewerAction>`;
   - test accessors for state, session ID, tmux version, and queue length if
     needed.
2. Port upstream startup behavior from `viewer.zig`:
   - initial state is `StartupBlock`;
   - `Exit` in startup or command-queue states moves to `Defunct` and returns
     one `Exit` action;
   - inputs after defunct return no actions;
   - startup block accepts `BlockEnd` or `BlockErr` and moves to
     `StartupSession`;
   - `SessionChanged` in `StartupSession` records the session ID, queues
     `TmuxVersion` and `ListWindows`, moves to `CommandQueue`, and emits the
     first command;
   - command-queue `BlockEnd` / `BlockErr` consumes the oldest queued command;
   - `TmuxVersion` command output is trimmed and parsed with
     `parse_output_values(TMUX_VERSION_VARIABLES, ..., TMUX_VERSION_DELIMITER)`;
   - after a command is consumed, emit the next queued command if one exists;
   - command-queue `BlockEnd` / `BlockErr` with an empty queue is allowed,
     ignored, and emits no action;
   - `ControlNotification::Enter` is ignored by this Rust API boundary because
     DCS entry consumes it before handing tmux notifications to the viewer.
3. Keep later viewer behavior explicitly out of scope:
   - `ListWindows` output may be consumed without parsing in this experiment,
     which is a temporary scoped divergence from upstream;
   - pane history, pane visible, pane state, layout changes, window add/remove,
     pane output, and session reset behavior remain future experiments.
4. Add tests for:
   - immediate exit and post-defunct ignored input;
   - `Exit` from `StartupSession` and `CommandQueue`;
   - block-end and block-error startup transition;
   - non-startup notifications ignored during startup;
   - session-changed startup emits `display-message` and records session ID;
   - version block output stores the version and emits `list-windows`;
   - empty version output does not replace an existing stored version by
     manually queueing a second `TmuxVersion` in the test;
   - list-windows output consumes the queued command without parsing or actions;
   - `BlockErr` while in `CommandQueue` consumes the in-flight command and emits
     the next queued command;
   - command output with an empty queue is ignored and emits no action;
   - `Enter` is ignored at the viewer API boundary;
   - user command output consumes queue entries without side effects if a user
     command is ever queued in tests.
5. Keep the README's overall `tmux` checklist item unchecked, refining it after
   the result to say viewer startup is done while full viewer state, PTY, and
   App integration remain missing.
6. Update this experiment file with result and review records.

## Verification

- `cargo test -p roastty terminal::tmux`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/647-tmux-viewer-startup.md`
- compare/read the Rust viewer startup against:
  - `vendor/ghostty/src/terminal/tmux/viewer.zig` startup, command queue, and
    `receivedTmuxVersion` sections
  - `vendor/ghostty/src/terminal/tmux/output.zig`
- `git diff --check`

Pass = Roastty has a tested standalone tmux viewer startup state machine that
emits the initial command sequence and stores tmux version output, while the
README keeps list-windows parsing, pane/window synchronization, PTY, and
App/Surface integration open.

Fail = startup state transitions diverge from upstream, command queue ordering
is wrong, tmux version parsing is not tested, list-windows parsing is
accidentally overclaimed, or viewer/runtime integration is added prematurely.

## Design Review

Initial Codex design review session `019e9ad7-04a6-7b20-823a-fa6e3d24129f`
requested revisions:

- specify command-output behavior when the command queue is empty;
- add tests for `Exit` from `StartupSession` and `CommandQueue`;
- add command-queue `BlockErr` coverage;
- make the empty-version test setup concrete;
- record `ListWindows` consumption-without-parsing as a temporary scoped
  divergence from upstream;
- specify how this Rust API handles `ControlNotification::Enter`.

The plan was revised to address those findings.

Follow-up review in the same session approved the revised design for
implementation. The reviewer confirmed that empty command-output behavior, exit
coverage, command-queue `BlockErr`, empty-version setup, scoped list-windows
divergence, and `Enter` handling are now specified. The optional suggestion to
test ignored `Enter` was added to the plan before the plan commit.
