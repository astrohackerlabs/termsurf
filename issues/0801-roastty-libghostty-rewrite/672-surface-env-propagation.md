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

# Experiment 672: Surface Env Propagation

## Description

Experiment 671 starts a surface worker from copied command, working-directory,
and initial-input configuration, but it still ignores `RoasttySurfaceConfig`
environment variables. The next slice is to copy valid env vars into the surface
and pass them into the spawned child process.

This experiment keeps the launch policy otherwise unchanged: `/bin/sh` remains
the default shell, command strings still run through `/bin/sh -lc`, and
foreground PID, tty-name, renderer wakeup, grid resize, and full draw/refresh
remain deferred.

## Changes

- `roastty/src/os/pty.rs`
  - Extend `PtyCommand` with owned environment variables.
  - Add `PtyCommand::env(key, value)` and apply them with `Command::env` before
    spawning the child.
  - Add a focused test that a PTY child sees an injected env var.
- `roastty/src/termio.rs`
  - Add env support to cwd-aware spawning:
    - introduce
      `TermioSpawnOptions { cwd: Option<PathBuf>, env: Vec<(String, String)> }`,
      or an equivalent internal option type;
    - keep existing `Termio::spawn` and `Termio::spawn_with_cwd` wrappers;
    - add a `spawn_with_options` path used by surfaces.
  - Add a test that `Termio` passes env vars to the child.
- `roastty/src/lib.rs`
  - Copy `RoasttySurfaceConfig.env_vars` into owned surface state at
    `roastty_surface_new`.
  - Treat `env_vars == NULL` as no env entries regardless of `env_var_count`; do
    not form a slice from a null env array.
  - Copy only entries whose key and value pointers are non-null and valid UTF-8.
    Values may be empty. Keys must be non-empty and must not contain `=`.
    Invalid entries are skipped, matching the current permissive config-string
    behavior.
  - Preserve configured order when applying env vars. Duplicate keys are
    allowed; the last configured value wins because entries are applied to
    `Command::env` in order.
  - Pass copied env vars into the surface's `Termio` spawn path.
  - Keep copied env vars alive independently of the caller-provided env array
    and source C strings.
- Tests in `roastty/src/lib.rs`
  - Start a surface with command `printf \"$ROASTTY_TEST_ENV\"` and a configured
    env var, then tick/snapshot and assert the value is visible.
  - Create the env array and its C strings in a nested scope, drop them before
    `roastty_surface_start`, then assert the copied env var still reaches the
    child.
  - Include invalid/null env entries alongside a valid entry and assert launch
    still succeeds while the valid entry is propagated.
  - Set `env_vars = NULL` with a nonzero count and assert launch succeeds with
    no env entries read.
  - Include empty-key and `=`-containing-key entries alongside a valid entry and
    assert only the valid entry is propagated.
  - Configure duplicate keys and assert the last value wins.
  - Continue using `os::pty::PTY_COMMAND_LOCK` for subprocess tests.

## Design Review

**Result:** Approved after amendments.

Codex found three env-contract gaps: null `env_vars` with nonzero
`env_var_count` needed explicit semantics, valid keys needed to reject empty
strings and `=`, and duplicate-key behavior needed to be stated.

The design now treats a null env array as empty regardless of count, skips
non-UTF-8/null/empty/`=`-containing keys while allowing empty values, applies
env vars in configured order, and defines duplicate keys as last-wins with tests
for each case.

## Verification

- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/672-surface-env-propagation.md`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `cargo test -p roastty os::pty`
- `cargo test -p roastty termio`
- `cargo test -p roastty surface`
- `git diff --check`

## Result

**Result:** Pass.

Roastty now propagates surface-configured environment variables into PTY child
processes. `PtyCommand` owns env vars and applies them with `Command::env`.
`Termio` has `TermioSpawnOptions` for cwd plus env while keeping its existing
convenience spawn wrappers. Surfaces copy valid env entries from
`RoasttySurfaceConfig` into owned state and pass them through the termio launch
path used by `roastty_surface_start`.

The env copy rules are permissive and explicit: a null env array means no env
entries regardless of count; null/non-UTF-8 entries are skipped; keys must be
non-empty and must not contain `=`; values may be empty; duplicate keys are
applied in order, so the last value wins.

Focused tests cover PTY-level env propagation, Termio-level env propagation,
surface env propagation, copied env ownership after source C strings are
dropped, null env array with nonzero count, invalid/null entries alongside valid
entries, empty values, and duplicate-key last-wins behavior.

Verification passed:

- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `cargo test -p roastty os::pty` — 14 passed, 0 failed
- `cargo test -p roastty termio` — 17 passed, 0 failed
- `cargo test -p roastty surface` — 25 passed, 0 failed
- `git diff --check`

## Conclusion

Surface worker launch now honors command, working directory, env vars, and
initial input from copied surface config. The remaining PTY/frontend launch gaps
are configured shell policy beyond `/bin/sh`, foreground PID, tty-name, renderer
wakeups, terminal grid resize, and the broader draw/refresh lifecycle.

## Completion Review

**Result:** Approved after provenance fix.

Codex found no implementation bugs in the env propagation path. It confirmed
that `PtyCommand` owns and applies env vars in order, `TermioSpawnOptions`
preserves existing wrappers while adding cwd+env launch, and `Surface` copies
env entries into owned state and passes them through `roastty_surface_start`.
Codex also confirmed the invalid/null/duplicate semantics match the approved
design: null env arrays are empty, invalid entries are skipped, empty and
`=`-containing keys are rejected, empty values are allowed, and duplicate keys
are applied in order so the last value wins.

The only result-review finding was missing provenance. The experiment
frontmatter and README agent tuple now record the result review.
