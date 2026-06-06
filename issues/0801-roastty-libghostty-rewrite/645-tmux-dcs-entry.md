+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 645: Tmux DCS Entry

## Description

Wire the existing tmux control parser into Roastty's DCS handler.

Experiments 642 through 644 added standalone tmux control, layout, and output
helpers. Upstream DCS handling recognizes `DCS 1000 p` as tmux control-mode
entry, emits an `enter` notification immediately, forwards payload bytes through
the tmux control parser, and emits `exit` when the DCS sequence unhooks.

This experiment should port only that terminal-core DCS entry behavior. It must
not build the tmux viewer, command queue, PTY write path, or App/Surface event
plumbing. Terminal stream handling may continue to treat emitted tmux
notifications as no-ops until a later integration experiment consumes them.

## Changes

1. Extend `roastty/src/terminal/dcs.rs`:
   - import `super::tmux`;
   - add `Command::Tmux(tmux::ControlNotification)`;
   - add `State::Tmux(tmux::ControlParser)`;
   - make `DCS 1000 p` with no intermediates enter tmux state and return
     `ControlNotification::Enter`;
   - forward payload bytes through `ControlParser::put`;
   - return parser notifications when they are emitted;
   - return `ControlNotification::Exit` on `unhook`;
   - if `ControlParser::put` returns an error, enter `State::Ignore` and return
     `None`, matching existing over-capacity DCS behavior;
   - preserve upstream's double-exit behavior for malformed payloads: if the
     parser emits `Exit` before the DCS terminator, the DCS handler remains in
     tmux state and `unhook` emits the normal implicit `Exit` as well;
   - keep unknown DCS commands ignored.
2. Update `roastty/src/terminal/terminal.rs`:
   - handle `dcs::Command::Tmux(_)` as a no-op for now;
   - keep terminal rendering and PTY responses unchanged by tmux DCS sequences.
3. Add focused DCS tests mirroring upstream `terminal/dcs.zig` tmux behavior:
   - enter/implicit exit for `DCS 1000 p`;
   - payload notification case such as `%sessions-changed`;
   - malformed-payload early `Exit` followed by implicit unhook `Exit`;
   - parser over-capacity/error path entering ignore state;
   - negative matchers for wrong params, extra params, and intermediates.
4. Update the existing terminal-stream tmux regression so it verifies no visible
   terminal side effects, not that the lower-level DCS handler ignores tmux;
   include cursor-position assertions alongside visible text, PTY response, and
   dirty-row checks.
5. Keep the README's overall `tmux` checklist item unchecked, refining it after
   the result to say control/layout/output helpers and DCS entry are done while
   viewer, PTY, and App integration remain missing.
6. Update this experiment file with result and review records.

## Verification

- `cargo test -p roastty terminal::dcs`
- `cargo test -p roastty terminal::tmux`
- `cargo test -p roastty terminal::terminal::tests::terminal_stream_dcs_command_tmux_and_unknown_are_ignored`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/645-tmux-dcs-entry.md`
- compare/read the Rust DCS path against:
  - `vendor/ghostty/src/terminal/dcs.zig`
  - `vendor/ghostty/src/terminal/tmux/control.zig`
  - `vendor/ghostty/src/termio/stream_handler.zig` tmux notification handling
    boundary
- `git diff --check`

Pass = `DCS 1000 p` produces tmux enter/payload/exit notifications at the DCS
handler boundary, terminal stream behavior remains visibly unchanged until a
later consumer is wired, and the README keeps viewer/PTY/App integration open.

Fail = tmux DCS payloads are still ignored by the DCS handler, unknown DCS
commands regress, terminal rendering/PTY behavior changes prematurely, or the
experiment overclaims viewer integration.

## Design Review

Initial Codex design review session `019e9acb-1cd2-7080-82b6-8fc6854488ac`
requested revisions:

- specify how `ControlParser::put` errors map into DCS `State::Ignore`;
- clarify malformed-payload early `Exit` plus implicit unhook `Exit` behavior;
- call out the new `super::tmux` import;
- include negative DCS matcher tests and tmux over-capacity/error coverage;
- strengthen the terminal-stream no-op regression with cursor-position checks.

The plan was revised to address those findings.

Follow-up review in Codex session `019e9acc-08fb-7b31-a4ed-4a5777dc8cdf`
approved the revised design. The reviewer confirmed that the parser error path,
early-plus-implicit exit behavior, negative matchers, over-capacity coverage,
and terminal-stream cursor-position checks are now specified. The reviewer also
called out an implementation detail: initialize the tmux parser with the DCS
handler's `max_bytes` so `Handler::with_max_bytes(...)` can exercise the tmux
over-capacity path without a one-megabyte payload.
