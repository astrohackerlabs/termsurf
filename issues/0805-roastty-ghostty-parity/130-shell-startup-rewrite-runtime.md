# Experiment 130: Shell Startup Rewrite Runtime

## Description

`RUNTIME-009B2B2B3B` still includes remaining shell-specific startup rewrite
coverage. Experiment 124 proved the broad shell-integration runtime path:
terminal identity, resource-backed `TERMINFO`, shell feature env, env override
order, XDG setup markers, and zsh bootstrap behavior. It intentionally did not
claim exhaustive parity with pinned Ghostty's per-shell startup rewrite helper
tests.

Pinned Ghostty's focused `termio/shell_integration.zig` tests cover specific
rewrite behavior for:

- shell detection and forced-shell setup;
- bash `--posix`, `-c`, `-ic`, `--rcfile ... --posix`, and
  `--init-file ... --posix` fallback;
- bash `--norc`, `--noprofile`, `--rcfile`, `--init-file`, inherited `ENV`,
  `HISTFILE`, and `-`/`--` argument separator behavior;
- XDG setup with unset, existing, and missing resources;
- nushell `--execute 'use ghostty *'` injection, unsupported options that keep
  XDG env but skip command rewrite, and missing resources;
- zsh `ZDOTDIR` preservation and missing resources.

Roastty already has helper coverage for many of these cases, but the coverage is
not complete enough to remove the shell-specific rewrite clause from the
remaining terminal gap. This experiment will mirror the missing upstream helper
cases with Roastty-named expectations and add a static guard that checks the
covered helper surface against pinned Ghostty.

This experiment will split the terminal gap again:

- `RUNTIME-009B2B2B3B1`: **Oracle complete** for shell-specific startup rewrite
  helper coverage for bash, fish/elvish XDG setup, nushell, zsh, shell
  detection, forced shell setup, and missing-resource fallback.
- `RUNTIME-009B2B2B3B2`: **Gap** for unproven exotic OSC 7 URI edge cases and
  other remaining terminal behavior effects.

This experiment will not claim that the shell integration script bodies are
byte-identical to Ghostty, nor that every live shell has been launched in a real
PTY. It closes the helper rewrite coverage called out by the runtime inventory;
script-content and live-shell behavior can remain separate only if they are
named explicitly as follow-up work.

## Changes

- `roastty/src/termio/shell_integration.rs`
  - Add missing focused helper tests that mirror pinned Ghostty's shell startup
    rewrite cases:
    - shell detection, including `/bin/bash` on macOS and unsupported shells;
    - bash unsupported `--rcfile ... --posix` and `--init-file ... --posix`;
    - bash `--noprofile`, `--init-file`, `HISTFILE` unset/set, inherited `ENV`,
      and `-`/`--` separator behavior;
    - XDG missing-resource fallback;
    - nushell missing-resource fallback;
    - zsh setup without inherited `ZDOTDIR` and missing-resource fallback;
    - forced shell setup for every supported shell.
  - If any test exposes a real mismatch with pinned Ghostty's helper semantics,
    fix the helper while keeping Roastty naming (`ROASTTY_*`, `roastty.*`)
    intentional.
- `issues/0805-roastty-ghostty-parity/shell_startup_rewrite_runtime_parity.py`
  - Add a static guard that checks pinned Ghostty helper/test markers and
    Roastty helper/test markers for the closed shell startup rewrite slice.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Split `RUNTIME-009B2B2B3B` into the new Oracle-complete shell startup
    rewrite helper row and a reduced remaining terminal gap.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-223 summary. It must remain `Gap`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add the experiment link and update Learnings after the result.

## Verification

Pass criteria:

- Roastty has focused tests corresponding to pinned Ghostty's shell detection,
  forced shell, bash, XDG, nushell, zsh, and missing-resource rewrite cases.
- The tests assert Roastty-named env/script/module strings where app naming is
  intentionally different, and otherwise preserve Ghostty's helper semantics.
- `RUNTIME-009B2B2B3B1` becomes Oracle complete.
- `RUNTIME-009B2B2B3B2` remains `Gap` and still names exotic OSC 7 URI edge
  cases and other remaining terminal behavior effects.
- `CFG-223` remains `Gap`.

Commands:

```bash
cargo test --manifest-path roastty/Cargo.toml shell_integration
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/shell_startup_rewrite_runtime_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
cargo fmt --manifest-path roastty/Cargo.toml
cargo fmt --manifest-path roastty/Cargo.toml --check
prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/130-shell-startup-rewrite-runtime.md
git diff --check
```

Fail criteria:

- Any Roastty helper still differs from pinned Ghostty helper semantics outside
  intentional naming.
- The static guard cannot find the pinned Ghostty helper/test markers or the
  Roastty test markers.
- The inventory claims script-body parity, live-shell PTY parity, exotic OSC 7
  URI edge cases, other terminal effects, or CFG-223 complete.

## Design Review

**Reviewer:** Codex adversarial subagent with fresh context.

**Initial verdict:** Changes required.

The reviewer found that the verification commands listed only
`cargo fmt --manifest-path roastty/Cargo.toml --check`, while the experiment
plans Rust edits and the workflow requires running `cargo fmt`. The design was
updated to run `cargo fmt --manifest-path roastty/Cargo.toml` before the check.

**Re-review verdict:** Approved.

The reviewer confirmed the prior finding is resolved and reported no new
findings.
