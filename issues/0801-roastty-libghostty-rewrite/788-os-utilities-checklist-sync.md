+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "default"
reasoning = "medium"

[review.result]
agent = "codex"
model = "default"
reasoning = "medium"
+++

# Experiment 788: OS Utilities Checklist Sync

## Description

The Issue 801 checklist still says the `os/` utilities are ad hoc and have no
dedicated module, but the current Roastty tree already contains a dedicated
`roastty/src/os/` module set. The specific utilities named by the checklist have
focused Rust implementations:

- temp directories and temporary paths: `os::temp_dir` and `os::file`;
- file descriptor limits and temp path generation: `os::file`;
- environment variable composition: `os::env`;
- hostname validation and local hostname detection: `os::hostname`;
- locale/i18n helpers and C locale initialization: `os::locale` and `os::i18n`.

This experiment verifies those existing modules and updates the checklist
wording to reflect the current state. It does not add new OS code and does not
claim that all platform/resource work is complete. PTY, command launch, shell
integration resources, app bundle resources, and Swift app integration remain
tracked by their existing broader checklist rows.

## Changes

- `issues/0801-roastty-libghostty-rewrite/README.md`
  - Change the `os/` utilities checklist row from unchecked/missing to checked
    for the named helper set: tmpdir, file, env, hostname, locale/i18n.
  - Add scoped wording that resource lookup, shell integration resources, and
    app/frontend integration remain elsewhere and are not closed by this sync.
  - Add the Experiment 788 index entry.
- `issues/0801-roastty-libghostty-rewrite/788-os-utilities-checklist-sync.md`
  - Record the verification evidence and review result.

## Verification

- Inspect current modules:
  - `roastty/src/os/mod.rs`
  - `roastty/src/os/temp_dir.rs`
  - `roastty/src/os/file.rs`
  - `roastty/src/os/env.rs`
  - `roastty/src/os/hostname.rs`
  - `roastty/src/os/locale.rs`
  - `roastty/src/os/i18n.rs`
- Run focused OS utility tests:
  - `cargo test -p roastty os::temp_dir -- --nocapture --test-threads=1`
  - `cargo test -p roastty os::file -- --nocapture --test-threads=1`
  - `cargo test -p roastty os::env -- --nocapture --test-threads=1`
  - `cargo test -p roastty os::hostname -- --nocapture --test-threads=1`
  - `cargo test -p roastty os::locale -- --nocapture --test-threads=1`
  - `cargo test -p roastty os::i18n -- --nocapture --test-threads=1`
- Run adjacent integration checks that use these helpers:
  - `cargo test -p roastty xdg -- --nocapture --test-threads=1`
  - `cargo test -p roastty default_shell -- --nocapture --test-threads=1`
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/788-os-utilities-checklist-sync.md`
- Run:
  - `git diff --check`

The experiment passes if the named OS utility modules exist, focused tests pass,
the README checklist is updated with scoped wording that does not overclaim
unrelated OS/resource/frontend work, and markdown/diff checks pass. It is
Partial if some named helper area lacks direct test evidence. It fails if the
README row is accurate and there is still no dedicated OS utility module for the
named helper set.

## Design Review

Codex reviewed the design and found no blocking findings. The review approved
the checklist update as scoped to the named helper set without overclaiming PTY
policy, resources, shell integration, or frontend/app work.

## Result

**Result:** Pass

The dedicated `roastty/src/os/` helper modules named in the checklist row exist,
and the focused tests for those helpers passed:

- `cargo test -p roastty os::temp_dir -- --nocapture --test-threads=1`: 2 passed
- `cargo test -p roastty os::file -- --nocapture --test-threads=1`: 5 passed
- `cargo test -p roastty os::env -- --nocapture --test-threads=1`: 6 passed
- `cargo test -p roastty os::hostname -- --nocapture --test-threads=1`: 4 passed
- `cargo test -p roastty os::locale -- --nocapture --test-threads=1`: 12 passed
- `cargo test -p roastty os::i18n -- --nocapture --test-threads=1`: 6 passed

Adjacent integration checks that depend on the OS helper behavior also passed:

- `cargo test -p roastty xdg -- --nocapture --test-threads=1`: 3 passed
- `cargo test -p roastty default_shell -- --nocapture --test-threads=1`: 6
  passed

Formatting and diff hygiene checks passed:

- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/788-os-utilities-checklist-sync.md`
- `git diff --check`

The checklist row now marks the named `os/` utility helper set as complete while
explicitly leaving resource lookup, shell integration resources, PTY policy, and
frontend/app integration to their existing checklist rows.

## Conclusion

The Issue 801 checklist was stale for the named OS utility helpers. Roastty
already has dedicated modules and focused tests for tmpdir/temp paths, file
helpers, env path composition, hostname checks, locale probing, and i18n locale
helpers, so the row can be checked with scoped wording. This experiment did not
close broader platform integration work such as resource lookup, shell
integration resources, PTY policy, or frontend/app integration.

## Completion Review

Codex initially found one blocking documentation issue: the result recorded the
focused test outcomes but omitted the required Prettier and `git diff --check`
outcomes. After those verification lines were added, Codex re-reviewed the
result and found no blocking findings. The follow-up review approved the README
status/provenance, verification record, and scoped checklist wording for the
result commit.
