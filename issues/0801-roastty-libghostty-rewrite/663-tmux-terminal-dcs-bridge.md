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

# Experiment 663: Tmux Terminal DCS Bridge

## Description

Experiment 662 completed the viewer-side live `%output` path. Roastty's DCS
handler already recognizes tmux control mode DCS commands and emits
`dcs::Command::Tmux(ControlNotification)`, but `Terminal::dcs_command` currently
drops those commands. That leaves the viewer reachable only from direct unit
tests, not from the normal terminal byte stream.

This experiment wires tmux DCS commands into `Terminal`:

- DCS tmux enter creates terminal-owned `TmuxViewer` state.
- DCS tmux exit drops that state.
- Other tmux notifications are forwarded to the viewer.
- Viewer `Command(String)` actions are written back through the existing PTY
  response path, matching upstream Ghostty's `messageWriter(writeReq(command))`
  behavior at Roastty's current abstraction level.
- Viewer `Exit` actions drop the viewer and cached windows but do not otherwise
  terminate the terminal. Upstream stream handling ignores viewer `.exit`
  actions because the DCS connection exit later performs cleanup; Roastty's
  terminal bridge owns cached tmux state directly, so clearing on viewer exit is
  an intentional local adaptation to avoid stale viewer/window state after
  malformed tmux output defuncts the viewer.
- Viewer `Windows(Vec<TmuxWindow>)` actions are stored in terminal-owned state
  for later App/surface integration but are not exposed through the public ABI
  yet.

This is the narrow App-integration precursor: it proves the normal terminal
stream can enter tmux control mode, drive the viewer, and write tmux commands
back to the PTY. Full OS PTY spawn/read loops, renderer wakeups, and App/surface
window presentation remain out of scope.

## Changes

- `roastty/src/terminal/terminal.rs`
  - Add optional terminal-owned tmux viewer state and a cached tmux window list.
  - Initialize that state with the terminal.
  - Clear that state through both terminal reset paths: `Terminal::reset()` and
    RIS-triggered `full_reset()`.
  - Replace the current no-op `dcs::Command::Tmux(_)` arm with a tmux command
    handler.
  - On `ControlNotification::Enter`, create a fresh `TmuxViewer` only when no
    viewer is active.
  - On `ControlNotification::Exit`, clear the viewer and cached windows.
  - For other notifications, forward to the active viewer if one exists;
    notifications without a viewer are ignored, matching upstream's logged
    no-viewer path.
  - Convert viewer actions:
    - `Command(command)` writes `command.as_bytes()` through
      `write_pty_response_bytes`.
    - `Windows(windows)` stores the latest windows for future App integration
      and does not itself write PTY bytes. Any queued follow-up capture commands
      still write through the normal `Command` action path.
    - `Exit` clears the viewer and cached windows.
  - Keep parser raw-byte `%output` parity deferred; this experiment uses the
    existing `ControlNotification` stream.
- Tests in `roastty/src/terminal/terminal.rs`
  - Verify feeding a full tmux DCS startup sequence through
    `Terminal::next_slice` writes the first viewer command to PTY response.
  - Verify later session/version/list-windows command flow writes subsequent
    commands through the same PTY response path.
  - Verify DCS exit clears viewer state and later tmux payload is ignored until
    another enter.
  - Verify viewer `Exit` clears viewer/window state by driving a malformed
    command result, such as malformed list-windows or layout output, through the
    normal DCS stream.
  - Verify viewer `Windows` actions are cached internally without writing PTY
    bytes by using a no-new-pane scenario, such as an empty list-windows result
    or a layout-change for an already tracked pane after the command queue is
    empty.
  - Verify both `Terminal::reset()` and RIS-triggered full reset clear tmux
    viewer/window state.

## Verification

- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/663-tmux-terminal-dcs-bridge.md`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `cargo test -p roastty terminal::terminal::tests::terminal_tmux`
- `cargo test -p roastty terminal::dcs terminal::tmux`
- `git diff --check`

## Design Review

**Result:** Approved after amendments.

Codex first found three plan gaps: the design needed to document and test the
intentional `ViewerAction::Exit` divergence from upstream, the `Windows`
cache/no-PTY test needed a precise no-new-pane scenario, and reset coverage
needed to include both `Terminal::reset()` and RIS-triggered `full_reset()`.

The design now records the upstream divergence and Roastty-specific rationale
for clearing viewer/window state on viewer exit, adds a malformed-output
viewer-exit test, specifies a no-new-pane `Windows` cache test, and explicitly
clears/tests both reset paths. Codex re-reviewed the amended design and approved
it for plan commit and implementation with no remaining blockers.
