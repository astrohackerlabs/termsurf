# Experiment 124: Shell Integration Runtime Split

## Description

`RUNTIME-009B2B` still combines exact nonzero scrollback byte quota,
shell-integration environment behavior, terminfo/terminal identity behavior,
configured/static title-report surface-title behavior, and remaining terminal
behavior effects. Recent inspection found that Roastty already has focused
Termio runtime tests for the shell-integration and terminal identity slice:

- fallback terminal identity without resources sets `TERM=xterm-256color` and
  `COLORTERM=truecolor`;
- resource-backed terminal identity sets configured `TERM`,
  `COLORTERM=truecolor`, `TERMINFO`, and `ROASTTY_RESOURCES_DIR`;
- inherited stale terminal identity env is overwritten before child launch;
- explicit env overrides win after terminal identity and shell-integration env;
- `ROASTTY_SHELL_FEATURES` reflects configured shell integration features,
  including cursor blink/steady;
- forced zsh integration reaches the child env and sources inherited `ZDOTDIR`
  through the shell-integration bootstrap.

Pinned Ghostty's corresponding path lives in `termio/Exec.zig` and
`termio/shell_integration.zig`: it sets `GHOSTTY_RESOURCES_DIR`, `TERM`,
`COLORTERM`, and `TERMINFO` from the resources directory, falls back to
`xterm-256color` without resources, computes `GHOSTTY_SHELL_FEATURES`, prepends
shell-integration paths to `XDG_DATA_DIRS` for supported shells, and rewrites
zsh setup through its integration bootstrap.

This experiment will split that already-proven shell-integration/terminal
identity runtime slice out of `RUNTIME-009B2B`. It will not claim exact nonzero
scrollback byte-quota parity, configured/static surface-title reporting parity,
or every possible shell-specific startup rewrite; those remain in the follow-up
terminal gap.

## Changes

- `issues/0805-roastty-ghostty-parity/shell_integration_runtime_parity.py`
  - Add a static guard that checks pinned Ghostty's terminal identity and shell
    feature/setup markers in `Exec.zig` and `shell_integration.zig`.
  - Check Roastty's `TermioSpawnOptions`, terminal identity setup,
    shell-integration setup call, feature env setup, zsh setup, XDG data dir
    setup, explicit-env override ordering, and existing runtime test names.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Split `RUNTIME-009B2B` into:
    - `RUNTIME-009B2B1`: `Oracle complete` for shell-integration feature env,
      terminal identity, resource-backed `TERMINFO`, and zsh bootstrap runtime
      behavior.
    - `RUNTIME-009B2B2`: `Gap` for exact nonzero scrollback byte quota,
      configured/static title-report surface-title behavior, remaining
      shell-specific startup rewrite coverage, and other remaining terminal
      behavior effects.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the runtime inventory script.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add the experiment link and update Learnings after implementation with any
    durable finding.

## Verification

Pass criteria:

- `cargo test --manifest-path roastty/Cargo.toml termio_env`
- `cargo test --manifest-path roastty/Cargo.toml spawn_with_options_sets_shell_feature_env_even_when_integration_is_none`
- `cargo test --manifest-path roastty/Cargo.toml zsh_integration_spawn_with_options`
- `cargo test --manifest-path roastty/Cargo.toml shell_integration`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/shell_integration_runtime_parity.py`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
- A matrix assertion verifies:
  - `RUNTIME-009B2B1` is `Oracle complete`;
  - `RUNTIME-009B2B1` evidence and guard command name terminal identity,
    `TERMINFO`, shell feature env, zsh bootstrap, and the static parity guard;
  - `RUNTIME-009B2B2` remains `Gap`;
  - `RUNTIME-009B2B2` still names exact nonzero scrollback byte quota,
    configured/static title-report surface-title behavior, remaining
    shell-specific startup rewrite coverage, and other remaining terminal
    behavior effects;
  - CFG-223 remains `Gap` until all runtime/UI rows are closed.
- `git diff --check`
- No generated `__pycache__` remains under the issue directory.

Fail criteria:

- The static guard cannot find the pinned Ghostty terminal identity or shell
  feature/setup path.
- Roastty's runtime tests do not actually launch child processes through Termio
  and inspect the child-visible environment.
- The split claims exact nonzero `scrollback-limit` byte-quota parity or
  configured/static surface-title reporting parity.
- The split claims exhaustive shell-specific integration parity beyond the
  covered feature-env, terminal identity, resource/terminfo, env override, and
  zsh bootstrap slice.

## Design Review

An adversarial Codex subagent reviewed the design with fresh context.

Verdict: **Approved**.

The reviewer reported no findings. It confirmed the README links Experiment 124
as `Designed`, the experiment has the required sections, the scope is narrow
enough for the Issue 805 runtime parity split, the plan avoids overclaiming
nonzero scrollback byte-quota parity, configured/static surface-title report
parity, or exhaustive shell-specific startup rewrite parity, and the
verification includes focused runtime tests, a static parity guard, inventory
regeneration, matrix assertions, `git diff --check`, and no-`__pycache__`
criteria.

## Result

**Result:** Pass

Added `shell_integration_runtime_parity.py` and split the runtime terminal
inventory so the already-proven shell-integration/terminal identity slice is
tracked separately from the remaining terminal gap. `RUNTIME-009B2B1` is now
`Oracle complete` for shell-integration feature env, terminal identity,
resource-backed `TERMINFO`, explicit env override order, and zsh bootstrap
runtime effects. `RUNTIME-009B2B2` remains `Gap` for exact nonzero scrollback
byte quota, configured/static title-report surface-title behavior, remaining
shell-specific startup rewrite coverage, and other remaining terminal behavior
effects. CFG-223 remains `Gap`.

Verification passed:

- `cargo test --manifest-path roastty/Cargo.toml termio_env`
- `cargo test --manifest-path roastty/Cargo.toml spawn_with_options_sets_shell_feature_env_even_when_integration_is_none`
- `cargo test --manifest-path roastty/Cargo.toml zsh_integration_spawn_with_options`
- `cargo test --manifest-path roastty/Cargo.toml shell_integration`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/shell_integration_runtime_parity.py`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
- Matrix assertion for `RUNTIME-009B2B1`, `RUNTIME-009B2B2`, and CFG-223
- `git diff --check`
- No generated `__pycache__` remained under the issue directory.

## Conclusion

The terminal shell-integration gap is smaller than the broad `RUNTIME-009B2B`
row implied. Roastty already has runtime tests that launch children through
Termio and inspect child-visible env for terminal identity, resource-backed
`TERMINFO`, explicit env override ordering, shell feature env, and zsh bootstrap
behavior. Exact byte-quota scrollback, configured/static surface-title
reporting, and broader shell-specific startup rewrite coverage remain open.

## Completion Review

An adversarial Codex subagent reviewed the completed experiment with fresh
context.

Verdict: **Approved**.

The reviewer reported no required findings. It independently reran the targeted
Termio env tests, shell feature env test, zsh integration tests, shell
integration tests, static shell integration parity guard, read-only matrix
assertion, `git diff --check`, and no-`__pycache__` check. It did not rerun the
exact inventory regeneration command because that command writes generated
markdown and updates the matrix under the reviewer's read-only discipline, but
it inspected the generated output and found the counts and statuses internally
consistent. It also confirmed the result commit had not yet been made.
