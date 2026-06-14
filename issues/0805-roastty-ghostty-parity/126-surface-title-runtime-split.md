# Experiment 126: Surface Title Runtime Split

## Description

`RUNTIME-009B2B2` still combines several unrelated terminal leftovers: nonzero
scrollback byte-quota parity, configured/static title behavior, remaining
shell-specific startup rewrites, and other terminal behavior toggles. Pinned
Ghostty's surface-title behavior is separable:

- if `title` is configured, surface initialization sends a `.set_title` app
  action with that configured static title;
- if no static title is configured and the launch command is a direct command,
  surface initialization sends a `.set_title` app action with argv[0];
- shell commands do not get command-derived static titles;
- non-empty terminal title messages are ignored when a static config title is
  set;
- config updates with a configured `title` send a `.set_title` app action with
  the new configured title.

Roastty already has typed app action plumbing for `ROASTTY_ACTION_SET_TITLE`,
test action recording for title payloads, parsed `title` config storage, and
terminal title state. It does not yet have a callback-safe live PTY path for
terminal title changes: `TermioWorker::spawn` correctly rejects terminals with
effect callbacks installed, so this experiment must not install `title_changed`
callbacks on worker-owned terminals. Instead, it should carry a title-change
signal through `TermioPump`, analogous to the existing bell count path.

This experiment will split configured/static surface-title runtime behavior out
of `RUNTIME-009B2B2`. It will not claim exact nonzero scrollback byte-quota
parity, shell-specific startup rewrite parity beyond the already split
shell-integration slice, or the broader macOS titlebar/window walkthrough in
`RUNTIME-011`.

## Changes

- `roastty/src/termio.rs`
  - Extend `TermioPump` with a terminal-title change field that is populated
    after PTY output changes the terminal title.
  - Keep `TermioWorker::spawn` callback rejection intact; do not install
    terminal title callbacks for worker-owned terminals.
- `roastty/src/lib.rs`
  - Add surface startup/update logic that dispatches `ROASTTY_ACTION_SET_TITLE`
    for configured static titles and direct command argv[0] titles, matching the
    pinned Ghostty startup branches.
  - Dispatch non-empty PTY/OSC title changes through the new pump title field
    only when no static `title` is configured.
  - Add focused runtime tests for configured title startup, direct-command
    fallback, shell-command no-op, config-update title dispatch, non-empty OSC
    title dispatch without a static title, and non-empty OSC title suppression
    with a static title.
- `issues/0805-roastty-ghostty-parity/surface_title_runtime_parity.py`
  - Add a static guard that checks pinned Ghostty's configured title,
    direct-command title, static-title suppression, and config-update title
    markers against Roastty's runtime/test markers.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Split `RUNTIME-009B2B2` into:
    - `RUNTIME-009B2B2A`: `Oracle complete` for configured/static surface-title
      startup/update and non-empty OSC title suppression/dispatch behavior.
    - `RUNTIME-009B2B2B`: `Gap` for exact nonzero scrollback byte quota,
      empty-title/PWD fallback semantics, remaining shell-specific startup
      rewrite coverage, and other remaining terminal behavior effects.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the runtime inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate from the runtime inventory script so CFG-223 reflects the split.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add the experiment link and update Learnings after implementation with any
    durable finding.

## Verification

Pass criteria:

- `cargo test --manifest-path roastty/Cargo.toml surface_title_runtime`
- `cargo test --manifest-path roastty/Cargo.toml termio_title`
- `cargo test --manifest-path roastty/Cargo.toml worker_rejects_terminal_with_callbacks`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/surface_title_runtime_parity.py`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
- A matrix assertion inside
  `issues/0805-roastty-ghostty-parity/surface_title_runtime_parity.py` verifies:
  - `RUNTIME-009B2B2A` is `Oracle complete`;
  - `RUNTIME-009B2B2A` evidence and guard command name configured title, direct
    command title, non-empty OSC title dispatch, static title suppression, and
    the static parity guard;
  - `RUNTIME-009B2B2B` remains `Gap`;
  - `RUNTIME-009B2B2B` still names exact nonzero scrollback byte quota,
    empty-title/PWD fallback semantics, remaining shell-specific startup rewrite
    coverage, and other remaining terminal behavior effects;
  - CFG-223 remains `Gap` until all runtime/UI rows are closed.
- `prettier --check --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/126-surface-title-runtime-split.md issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/config-runtime-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md`
- `cargo fmt --manifest-path roastty/Cargo.toml -- --check`
- `git diff --check`
- No generated `__pycache__` remains under the issue directory.

Fail criteria:

- The implementation installs terminal title callbacks on worker-owned terminals
  or weakens `TermioWorker::spawn` callback rejection.
- The split claims exact nonzero `scrollback-limit` byte-quota parity or
  remaining shell-specific startup rewrite parity.
- Static configured titles do not suppress subsequent non-empty OSC title app
  actions.
- The generated inventory or matrix marks CFG-223 `Pass` while
  `RUNTIME-009B2B2B` or any other runtime/UI row remains a gap.

## Design Review

An adversarial Codex subagent reviewed the design with fresh context.

Initial verdict: **Changes required**.

The reviewer found one required issue: the design overclaimed OSC title
suppression/dispatch because pinned Ghostty also has empty-title reset and OSC 7
PWD fallback semantics. The design was narrowed to non-empty OSC title
dispatch/suppression, and `RUNTIME-009B2B2B` now explicitly keeps
empty-title/PWD fallback semantics as a remaining gap.

Re-review verdict: **Approved**.

The reviewer confirmed the required finding was resolved and reported no new
required findings.

## Result

**Result:** Pass

Roastty now matches the pinned Ghostty surface-title slice covered by this
experiment:

- configured `title` dispatches `ROASTTY_ACTION_SET_TITLE` at startup;
- config updates with a new configured `title` dispatch the updated title;
- direct command argv[0] dispatches as the startup title when no static title is
  configured;
- shell commands do not dispatch a command-derived startup title;
- non-empty OSC/PTY title changes dispatch through the surface app action path
  when no static title is configured;
- static configured titles suppress later non-empty OSC title app actions;
- worker-owned terminals still reject installed effect callbacks, and live title
  changes travel through `TermioPump` instead of terminal callbacks.

Verification passed:

- `cargo test --manifest-path roastty/Cargo.toml surface_title_runtime` — 3
  passed.
- `cargo test --manifest-path roastty/Cargo.toml termio_title` — 1 passed.
- `cargo test --manifest-path roastty/Cargo.toml worker_rejects_terminal_with_callbacks`
  — 1 passed.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/surface_title_runtime_parity.py`
  — passed.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  — regenerated 35 runtime rows: 28 Oracle complete, 30 closed, 5 incomplete, 5
  gaps; CFG-223 remains Gap.
- `prettier --check --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/126-surface-title-runtime-split.md issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/config-runtime-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md`
  — passed.
- `cargo fmt --manifest-path roastty/Cargo.toml -- --check` — passed.
- `git diff --check` — passed.
- No generated `__pycache__` directory remains under the issue directory.

`RUNTIME-009B2B2` is split into `RUNTIME-009B2B2A` and `RUNTIME-009B2B2B`.
`RUNTIME-009B2B2A` is `Oracle complete` for configured/static surface-title
startup/update and non-empty OSC title dispatch/suppression effects.
`RUNTIME-009B2B2B` remains `Gap` for exact nonzero scrollback byte quota,
empty-title/PWD fallback semantics, remaining shell-specific startup rewrite
coverage, and other remaining terminal behavior effects.

## Conclusion

Configured/static surface-title behavior is no longer part of the broad terminal
leftovers gap. Future CFG-223 work should keep empty-title/PWD fallback and
remaining shell/title startup rewrite semantics separate, because this
experiment intentionally proves only non-empty title dispatch and static-title
suppression.

## Completion Review

An adversarial Codex subagent reviewed the completed experiment with fresh
context.

Verdict: **Approved**.

The reviewer reported no findings. It independently verified the targeted
surface-title tests, Termio title test, callback rejection test, static parity
guard, prettier check, Rust format check, and `git diff --check`. It also
confirmed the generated inventory/matrix rows keep `RUNTIME-009B2B2B` and
CFG-223 as gaps, and that the result remained uncommitted at review time.
