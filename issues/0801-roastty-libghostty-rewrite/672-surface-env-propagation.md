+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
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
