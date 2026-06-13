# Experiment 174: Phase F — conditional config runtime

## Description

Wire the config conditional-state machinery into Roastty's embedded app and
surface runtime paths.

Roastty already has a config-layer `change_conditional_state` implementation and
theme loading records `conditional::Key::Theme` when light/dark themes differ.
What is still missing is the upstream app/surface behavior: the app and each
surface remember their current light/dark conditional state, apply that state
when cloning/finalizing incoming configs, and request a soft config reload when
the platform color scheme changes.

This experiment does not attempt to finish all `finalize()` or theme-loading
parity. It is a bounded runtime wiring step for the Phase F "Conditional state
wiring (`changeConditionalState` + conditional reload)" roadmap item.

## Changes

- `roastty/src/lib.rs`
  - Add app-level and surface-level config conditional-state fields mirroring
    upstream `App.config_conditional_state` and
    `Surface.config_conditional_state`.
  - Add helpers that finalize an incoming `config::Config` after applying a
    requested conditional state through `Config::change_conditional_state`,
    falling back to the original config if no relevant conditional changed or if
    replay fails.
  - Use the app conditional state for `roastty_app_new` and
    `roastty_app_update_config` app-owned parsed config snapshots.
  - Use the app conditional state during `roastty_surface_new`, then initialize
    the new surface's own conditional state from the app, matching upstream
    `Surface.init`.
  - Use the surface conditional state for later `Surface::apply_config` and
    `roastty_surface_update_config` calls.
  - Preserve the original surface working-directory override when applying a
    conditional config during surface creation, matching upstream's
    `Surface.init` special case.
  - Make `roastty_app_set_color_scheme` update the app conditional theme state
    and dispatch a soft `reload_config` app action only when the light/dark
    state changes.
  - Make `roastty_surface_set_color_scheme` update the surface conditional theme
    state and dispatch a soft `reload_config` surface action only when the
    light/dark state changes, while preserving the existing terminal color
    scheme report behavior.
  - Validate app/surface color-scheme ABI integers before mutating state, so
    invalid values are ignored and do not trigger reload callbacks or terminal
    reports.

- `roastty/src/config/mod.rs`
  - Expose the minimal crate-private/test surface needed by `lib.rs` to apply
    conditional state without making conditional internals public API.
  - Add or extend focused tests if behavior is clearer at the config boundary.

- `roastty/tests/abi_harness.c`
  - Extend the C harness only if the app/surface callback behavior is easier to
    prove through the public ABI than Rust unit tests.

- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Link this experiment as `Designed`.
  - After result, add an operating note describing app/surface conditional
    config ownership and update the Phase F conditional-state roadmap item if
    the runtime reload path is fully wired.

## Verification

Before implementation:

- Codex-native adversarial design review approves this experiment.
- Commit the reviewed plan separately from the result.

After implementation:

- `cargo test -p roastty config_conditional_theme`
- `cargo test -p roastty config_theme_loading`
- `cargo test -p roastty app_set_color_scheme`
- `cargo test -p roastty surface_new_conditional`
- `cargo test -p roastty surface_set_color_scheme`
- `cargo test -p roastty surface_update_config`
- `cargo test -p roastty --test abi_harness`
- `cargo test -p roastty -- --test-threads=1`
- `cargo fmt -p roastty`
- `cargo fmt --check -p roastty`
- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/174-conditional-config-runtime.md issues/0802-libroastty-completion-and-mac-app/README.md`
- `git diff --check`

**Pass** = app and surface config snapshots apply the current light/dark
conditional state, platform color-scheme changes request soft config reloads on
the right app/surface target exactly when the conditional theme state changes,
new surfaces initialize from the app conditional state, later surface updates
use the surface-owned conditional state, surface working-directory overrides
survive conditional replay during creation, invalid color-scheme integers are
ignored, existing color-scheme terminal reports still work, and the full roastty
suite passes.

**Partial** = conditional configs can be applied manually but app/surface reload
callbacks, surface-specific state, or working-directory preservation remain
incomplete.

**Fail** = applying conditional state in the runtime path breaks config
finalization, theme replay, surface config updates, or existing color-scheme
report behavior.

## Design Review

**Reviewer:** Codex-native adversarial review subagent `Meitner`, fresh context.

**Initial verdict:** Changes required.

Findings and fixes:

- The first draft incorrectly assigned `roastty_surface_new` to the surface
  conditional-state owner. Fixed by making creation apply the app conditional
  state, then initializing the new surface's own conditional state from the app.
- The first draft did not prove surface creation or working-directory
  preservation. Fixed by adding a focused `surface_new_conditional` verification
  target and explicit pass criteria for initial app-state inheritance,
  surface-owned later updates, and working-directory override preservation.
- The first draft did not specify invalid color-scheme integers. Fixed by
  requiring app/surface ABI setters to ignore invalid values without triggering
  reload callbacks or terminal reports.

**Final verdict:** Approved. The re-review confirmed all prior required findings
were resolved and found no new required issues.

## Result

**Result:** Pass

Implemented app/surface conditional config runtime wiring in
`roastty/src/lib.rs` and exposed the minimal crate-private config replay hook in
`roastty/src/config/mod.rs`.

Key outcomes:

- `App` now stores both its current conditional state and a replay-capable
  source config, separate from the finalized app snapshot.
- App config creation/update finalizes through the app conditional state.
- New surfaces apply the app conditional state during creation and initialize
  their own surface conditional state from the app.
- Later surface config updates apply the surface-owned conditional state.
- App and surface color-scheme setters reject invalid ABI values, update state
  only on real light/dark transitions, and dispatch target-correct soft
  `reload_config` actions.
- Surface working-directory overrides remain owned by `RoasttySurfaceConfig`
  while conditional config replay updates config-derived fields.

Verification run:

- `cargo test -p roastty config_conditional_theme` — pass
- `cargo test -p roastty config_theme_loading` — pass
- `cargo test -p roastty app_set_color_scheme` — pass
- `cargo test -p roastty surface_new_conditional` — pass
- `cargo test -p roastty surface_set_color_scheme` — pass
- `cargo test -p roastty surface_update_config` — pass
- `cargo test -p roastty --test abi_harness` — pass; existing C enum-conversion
  warnings only
- `cargo test -p roastty -- --test-threads=1` — pass: 4873 passed, 0 failed, 4
  ignored; ABI harness and doc-tests also passed
- `cargo fmt -p roastty` — pass
- `cargo fmt --check -p roastty` — pass
- `git diff --check` — pass

## Conclusion

The Phase F conditional runtime path is now wired. The app owns the process-wide
conditional state and replay-capable source config; surfaces inherit that state
when created, then own their subsequent conditional state for surface-specific
updates. Platform light/dark changes now request soft reloads on the correct
app/surface target without accepting invalid color-scheme integers.

## Completion Review

**Reviewer:** Codex-native adversarial review subagent `Bacon`, fresh context.

**Initial verdict:** Changes required.

Finding and fix:

- The first result dispatched app/surface color-scheme `reload_config` actions
  with an empty storage payload, so callback consumers would see `soft = false`
  even though upstream sends `.reload_config` with `{ .soft = true }`. Fixed by
  adding the typed `RoasttyActionReloadConfig { soft: bool }` union member,
  mapping storage slot 0 through `action_u_from_storage` and the test inverse,
  dispatching color-scheme reloads with `soft_reload_config_storage()`, and
  asserting `records[0].storage[0] == 1` in both app and surface tests.

**Final verdict:** Approved. The re-review confirmed the prior required finding
was resolved and found no new required issues.
