# Experiment 103: Phase F — command and home finalize

## Description

Add the next upstream config-finalize behavior after Experiment 102:
default-shell resolution and `WorkingDirectory::Home` resolution.

Upstream `Config.finalize()` chooses the working-directory default first, then
enters the expensive desktop-only lookup block when either `command` is unset or
the chosen working directory is `home`. In that block, it prefers `$SHELL` for
the default command only in probable CLI environments, falls back to the passwd
shell, and converts `working-directory = home` to the passwd home directory when
available or `inherit` otherwise. If the command remains unset on Unix, upstream
logs a warning but does not synthesize `sh`; the later runtime launch path owns
that fallback.

Roastty now has a config-finalize context from Experiment 102, plus an
`os::passwd::Entry` helper and a config `Command` surface. This experiment
should extend the context and config finalization so the Rust config layer makes
the same default command / home-directory decisions deterministically.

Roastty's current config `Command::Shell` and `WorkingDirectory::Path` store
UTF-8 `String`s, while upstream stores byte slices. This experiment should use
UTF-8-compatible shell/home values and record non-UTF-8 byte-faithfulness as a
remaining representation gap, rather than broadening the config string model in
the same slice.

This is still a config-internal slice. It should not change the surface-start
runtime fallback path, add app C ABI exports, add GTK single-instance defaults,
add link matcher mutation, add key-remap finalization, or rewrite config string
storage.

## Changes

- `roastty/src/config/mod.rs`
  - Extend the private finalize context with deterministic inputs for:
    - `$SHELL`;
    - passwd shell;
    - passwd home.
  - Populate the production context from `std::env::var_os("SHELL")` and
    `os::passwd::get()`, reusing the probable-CLI decision from Experiment 102.
  - During config finalization, preserve upstream ordering by resolving command
    and `WorkingDirectory::Home` immediately after the working-directory default
    decision and before explicit `~/...` path finalization:
    - preserve an explicitly configured `command`;
    - if `command` is unset and the launch is probable CLI, use `$SHELL` when it
      is present and UTF-8, including an empty string;
    - if `command` is still unset, use the passwd shell when it is present and
      UTF-8, including an empty string;
    - if `working_directory` is `Home`, convert it to a passwd-home `Path` when
      the passwd home is present and UTF-8, including an empty string;
    - if `working_directory` is `Home` and no UTF-8 passwd home is present,
      convert it to `Inherit`;
    - if `command` remains unset on Unix, leave it unset.
  - Keep `WorkingDirectory::Path("~/...")` expansion from Experiment 102.
  - Add deterministic test-only finalize helpers or context constructors so
    tests can supply shell/passwd values without depending on the host account.
  - Add focused tests proving:
    - probable CLI uses `$SHELL` before passwd shell;
    - desktop / non-probable CLI ignores `$SHELL` and uses passwd shell;
    - an explicit command is preserved;
    - passwd shell is used when `$SHELL` is missing or not allowed;
    - empty-but-present `$SHELL`, passwd shell, and passwd home values are
      treated as present, matching upstream;
    - command remains unset when no usable Unix shell source exists;
    - `WorkingDirectory::Home` becomes a passwd-home `Path`;
    - `WorkingDirectory::Home` becomes `Inherit` when no UTF-8 passwd home is
      present;
    - explicit `Inherit` and explicit paths are preserved, with `~/...`
      expansion still applied.

No runtime launch fallback changes, app ABI, GTK defaulting, link matcher
mutation, key-remap finalization, or config string representation rewrite should
be implemented in this experiment.

## Verification

Pass criteria:

1. `cargo test -p roastty config_command_home_finalize`
2. `cargo test -p roastty config_working_directory_finalize`
3. `cargo test -p roastty command_config`
4. `cargo test -p roastty`
5. `cargo fmt --check`
6. `git diff --check`

The full `cargo test -p roastty` run must pass. The existing ABI harness may
print its known enum-conversion warnings, but no new failures are acceptable.

## Design Review

Codex-native adversarial review ran in fresh context with subagent
`019eb633-e566-7fb0-847a-c2492e25c96b`.

Verdict: **CHANGES REQUIRED**

Findings:

- Required: the initial design incorrectly treated empty `$SHELL` and passwd
  shell values as unusable, while upstream uses them whenever they are present.
- Required: the initial design did not define empty passwd-home handling and
  implied that empty home would become `Inherit`, while upstream converts any
  present home to a path.

Fix:

- Updated the design to use presence semantics after UTF-8 conversion: empty
  `$SHELL`, passwd shell, and passwd home values are present and should be used.
  The only scoped representation gap is non-UTF-8 shell/home values.

Re-review verdict: **APPROVED**

The reviewer confirmed both required findings were resolved and found no new
required findings.

## Result

**Result:** Pass

Implemented config-internal command/default-home finalization.

- Extended the private config-finalize context with `$SHELL`, passwd shell, and
  passwd home values.
- Populated the production context from `std::env::var_os("SHELL")` and
  `os::passwd::get()`.
- Resolved the default command after the working-directory default decision:
  probable CLI contexts use present UTF-8 `$SHELL` first, including an empty
  string; otherwise config finalization falls back to the present UTF-8 passwd
  shell, including an empty string.
- Preserved explicitly configured commands.
- Converted `WorkingDirectory::Home` to a present UTF-8 passwd-home path,
  including an empty string, and converted it to `Inherit` when no UTF-8 passwd
  home is present.
- Kept Experiment 102 `~/...` path expansion after the command/home resolution
  step.
- Added deterministic tests for env-vs-passwd shell precedence, desktop `$SHELL`
  rejection, explicit command preservation, empty-present values, no shell
  sources, non-UTF-8 scoped gaps, passwd-home path conversion, missing-home
  inheritance, and the updated working-directory finalize behavior.

Verification passed:

1. `cargo test -p roastty config_command_home_finalize`
2. `cargo test -p roastty config_working_directory_finalize`
3. `cargo test -p roastty command_config`
4. `cargo test -p roastty`
5. `cargo fmt --check`
6. `git diff --check`

The focused command/home run passed 8 tests. The working-directory regression
run passed 6 tests. The command-config run passed 1 test. The full
`cargo test -p roastty` run passed 4584 unit tests, the ABI harness, and doc
tests. The ABI harness printed the existing 10 enum-conversion warnings.

## Conclusion

Roastty now ports the Unix config-finalize default command and
`WorkingDirectory::Home` resolution block for UTF-8 shell/home values. This
removes the main default-shell/home portion of the upstream finalize gap while
leaving byte-faithful non-UTF-8 config storage, runtime launch fallback cleanup,
GTK single-instance defaults, link matcher mutation, and key-remap finalization
for later experiments.

## Completion Review

Codex-native adversarial review ran in fresh context with subagent
`019eb63d-3569-7461-88d8-90e04a0dcb74`.

Verdict: **APPROVED**

Findings: None.
