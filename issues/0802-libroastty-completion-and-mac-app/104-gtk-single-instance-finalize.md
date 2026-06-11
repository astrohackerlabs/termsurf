# Experiment 104: Phase F — GTK single-instance finalize

## Description

Add the next small upstream config-finalize behavior after Experiment 103:
`gtk-single-instance = detect` runtime defaulting.

Upstream `Config.finalize()` handles this after working-directory/default-shell
resolution and before the later scalar tail. The behavior is build-runtime
specific: for non-GTK app runtimes it does nothing; for GTK, explicit `true` and
`false` are preserved, while `detect` becomes `false` in a probable CLI
environment and `true` otherwise.

Roastty already has the `GtkSingleInstance` config enum and the
`probable_cli_environment()` context from Experiments 102-103. This experiment
should add the runtime-specific finalize decision without changing GTK runtime
behavior, app ABI, or the current embedded/mac production runtime. Production
Roastty should continue to behave like upstream's non-GTK runtime unless a later
build/runtime selection slice introduces a real GTK target.

This is a config-internal slice. It should not implement GTK single-instance
runtime behavior, app C ABI exports, link matcher mutation, quit-delay warning
logging, key-remap finalization, or a general build-config system.

## Changes

- `roastty/src/config/mod.rs`
  - Add a private app-runtime discriminator for config finalization with the
    current production runtime set to non-GTK.
  - Extend the private finalize context with that runtime discriminator.
  - During config finalization, preserve upstream ordering by applying
    `gtk-single-instance` defaulting after command/home/working-directory
    finalization and before the later scalar tail:
    - non-GTK runtime: leave `gtk_single_instance` unchanged;
    - GTK runtime + explicit `True` or `False`: preserve the explicit value;
    - GTK runtime + `Detect` + probable CLI: set `False`;
    - GTK runtime + `Detect` + not probable CLI: set `True`.
  - Add deterministic test-only finalize helpers or context constructors so
    tests can select GTK vs non-GTK and probable CLI without depending on the
    host launch environment.
  - Add focused tests proving:
    - non-GTK runtime leaves `Detect` unchanged;
    - GTK `Detect` becomes `False` for probable CLI;
    - GTK `Detect` becomes `True` outside probable CLI;
    - GTK explicit `True` and `False` survive finalization;
    - other scalar-finalize behavior still runs with the new context shape.

No GTK runtime behavior, app ABI, link matcher mutation, quit-delay warning,
key-remap finalization, or build-config system should be implemented in this
experiment.

## Verification

Pass criteria:

1. `cargo test -p roastty config_gtk_single_instance_finalize`
2. `cargo test -p roastty gtk_chrome_config`
3. `cargo test -p roastty config_command_home_finalize`
4. `cargo test -p roastty`
5. `cargo fmt --check`
6. `git diff --check`

The full `cargo test -p roastty` run must pass. The existing ABI harness may
print its known enum-conversion warnings, but no new failures are acceptable.

## Design Review

Codex-native adversarial review ran in fresh context with subagent
`019eb642-c612-7191-afd0-9876b4c39585`.

Verdict: **APPROVED**

Findings: None.

## Result

**Result:** Pass

Implemented config-internal GTK single-instance detect finalization.

- Added a private config-finalize app-runtime discriminator with production set
  to the current non-GTK runtime.
- Extended the private finalize context with that runtime discriminator.
- Applied upstream GTK-only `gtk-single-instance = detect` defaulting after
  command/home/working-directory finalization and before the later scalar tail.
- Left non-GTK runtime finalization unchanged, preserving `Detect` in
  production.
- Preserved explicit `gtk-single-instance = true` and `false`.
- Added deterministic tests for non-GTK no-op behavior, GTK probable-CLI
  `Detect -> False`, GTK non-CLI `Detect -> True`, explicit-value preservation,
  and later scalar-tail continuity.

Verification passed:

1. `cargo test -p roastty config_gtk_single_instance_finalize`
2. `cargo test -p roastty gtk_chrome_config`
3. `cargo test -p roastty config_command_home_finalize`
4. `cargo test -p roastty`
5. `cargo fmt --check`
6. `git diff --check`

The focused GTK finalize run passed 4 tests. The GTK chrome regression run
passed 1 test. The command/home regression run passed 8 tests. The full
`cargo test -p roastty` run passed 4588 unit tests, the ABI harness, and doc
tests. The ABI harness printed the existing 10 enum-conversion warnings.

## Conclusion

Roastty now ports upstream's GTK-only `gtk-single-instance = detect`
finalization while keeping the current embedded/mac production runtime non-GTK.
This removes the GTK runtime-default entry from the config-finalize gap without
claiming GTK runtime behavior or a build-config system. Remaining upstream
finalize gaps include link matcher mutation, quit-delay warning logging, and
key-remap finalization, plus broader byte-faithful config string storage.

## Completion Review

Codex-native adversarial review ran in fresh context with subagent
`019eb649-f89d-77d0-a328-f658354edbcc` after implementation and result
recording. The reviewer checked the experiment file, README, implementation diff
from the plan commit, changed source, and upstream `Config.zig`.

Verdict: **APPROVED**

Findings: None.
