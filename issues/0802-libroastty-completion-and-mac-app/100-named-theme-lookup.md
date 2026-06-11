# Experiment 100: Phase F — named theme lookup

## Description

Extend the theme finalization work from Experiment 99 from absolute paths to
upstream-style named theme lookup.

Upstream `config/theme.zig` resolves a non-absolute `theme` value by searching
theme directories in priority order: the user's XDG config `ghostty/themes`
directory first, then the bundled resources `themes` directory. Roastty should
do the renamed equivalent: user `roastty/themes` first, then the app resources
`themes` directory. A non-absolute theme name that contains path separators is
invalid and must not be treated as a relative path.

This experiment should keep the previous absolute-path loading and replay
priority unchanged. It should add the missing named-theme locator and reuse the
same load-into-fresh-config, replay-user-config, preserve-replay-list behavior
once a named theme resolves to a regular readable file.

This is still not the full theme system. Conditional reload /
`changeConditionalState`, full diagnostic string parity, theme replay
conditionalization, bundled default theme inventory validation, app C ABI
exposure, and runtime resource installation remain later work.

## Changes

- `roastty/src/config/loader.rs`
  - Add a small helper for the user theme directory, resolving
    `$XDG_CONFIG_HOME/roastty/themes` or `$HOME/.config/roastty/themes` through
    the existing XDG config resolver.
- `roastty/src/config/mod.rs`
  - Add an internal theme location struct for finalization. Its default
    constructor should include:
    - the user theme directory from `loader`;
    - the app resource theme directory from `os::resources_dir::resources_dir()`
      when available, joined with `themes`.
  - Add a test-only finalization entry point that accepts explicit theme
    locations so tests can avoid mutating global process environment or relying
    on the local app bundle layout.
  - Refactor theme file loading so both absolute paths and resolved named paths
    share the existing regular-file/read/replay path.
  - For non-absolute names:
    - reject names whose basename differs from the original name, matching
      upstream's "path separators require an absolute path" behavior;
    - probe locations in upstream priority order;
    - continue past `NotFound`;
    - stop and report other IO errors;
    - reject non-regular paths;
    - report all tried paths when the name is not found anywhere.
  - Preserve Exp99 behavior:
    - absolute paths still load directly;
    - theme values load before user replay;
    - user file/CLI config still overrides theme-file config;
    - original replay entries remain on the finalized config;
    - different light/dark theme names still convert `window-theme = auto` to
      `system`, including failed lookups.
  - Add focused tests proving:
    - a named theme loads from the user theme directory;
    - the user theme directory wins over the resources theme directory;
    - a named theme falls back to resources when absent from the user directory;
    - path-separator names are rejected without probing relative paths;
    - not-found reports the searched paths in order;
    - a non-regular named theme path is rejected;
    - a non-`NotFound` named-theme open error is reported without falling back;
    - absolute-path theme behavior from Exp99 still passes.

No conditional-state change API, runtime config reload, resource packaging, full
theme diagnostic text parity, or app ABI changes should be implemented in this
experiment.

## Verification

Pass criteria:

1. `cargo test -p roastty config_theme_loading`
2. `cargo test -p roastty config_finalize_scalar_tail`
3. `cargo test -p roastty config_replay`
4. `cargo test -p roastty`
5. `cargo fmt --check`
6. `git diff --check`

The full `cargo test -p roastty` run must pass. The existing ABI harness may
print its known enum-conversion warnings, but no new failures are acceptable.

## Design Review

Codex-native adversarial review ran in fresh context with subagent
`019eb600-7452-7bc3-aab7-771c4eefe594`.

Verdict: **APPROVED**

Findings: None.
