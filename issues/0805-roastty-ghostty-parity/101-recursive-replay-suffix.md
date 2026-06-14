# Experiment 101: Recursive replay suffix placement

## Description

Experiment 99 identified `LOAD-017` as the remaining structural CFG-221 load
gap. Pinned Ghostty has a replay ordering rule in
`Config.zig::loadRecursiveFiles`: before recursively loading `config-file`
entries, it removes the replay suffix beginning at the special `-e` marker,
loads recursive config files, records any recursive replay steps, and appends
the `-e` suffix back afterward. This prevents recursive `config-file` replay
steps from becoming initial-command arguments during later config replay.

Roastty currently records file and CLI replay entries but does not model a
special `-e` boundary in the replay list. This experiment will add the narrow
replay-boundary mechanism required to prove pinned Ghostty's recursive
config-file suffix placement without broadening CLI parsing beyond the
configuration layer.

## Changes

- `roastty/src/config/mod.rs`
  - Extend the internal replay model with a marker for the initial-command
    suffix boundary. The marker should be private/internal and should not format
    as a user config entry.
  - Add a focused helper or test-only entry point that can append the `-e`
    replay marker plus representative initial-command arguments, matching the
    pinned Ghostty replay shape enough to test ordering and marker-aware replay
    semantics.
  - Update recursive config-file loading so replay entries produced by recursive
    file loads are inserted before any existing `-e` marker and its suffix.
  - Preserve existing replay behavior for normal file, CLI, theme, and
    conditional rebuild paths when no `-e` marker is present.
  - Add focused tests proving:
    - recursive config-file replay entries are inserted before the `-e` marker;
    - the `-e` marker and suffix entries remain after the recursive replay
      entries in their original relative order;
    - recursive config-file replay entries keep a file/config-entry
      representation, not a suffix/argument representation;
    - replaying the entries into a fresh config applies recursive config values
      as config, not as part of the initial command suffix;
    - the replayed initial-command suffix is unchanged and contains only the
      representative original suffix arguments;
    - the marker-aware replay test is failure-sensitive: a config replay entry
      placed after the marker would not satisfy the recursive-value-applied
      assertion because it would be treated as suffix material or otherwise not
      applied as config;
    - existing no-`-e` recursive config-file loading and replay tests still
      pass.

- `issues/0805-roastty-ghostty-parity/config_load_inventory.py`
  - Promote `LOAD-017` from `Gap` to `Oracle complete` only if the ordering
    tests prove the `-e` suffix boundary behavior with marker-aware replay
    semantics.
  - Update evidence to name the new focused replay ordering tests.

- `issues/0805-roastty-ghostty-parity/config-load-inventory.md`
  - Regenerate the inventory.

- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-221 counts. CFG-221 must remain `Gap` if `LOAD-001` is still
    only `Audit covered`.

- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.
  - Add a learning only if implementation exposes a reusable replay-boundary
    rule for future reload/theme work.

## Verification

Pass criteria:

- New focused unit tests prove recursive replay entries are inserted before the
  `-e` marker and that the marker/suffix remain in order after them.
- A replay test proves recursive config-file entries are replayed as config
  entries instead of being swallowed into the initial-command suffix.
- The replay test also proves the initial-command suffix remains exactly the
  representative original suffix arguments, with no recursive config-file
  derived entry in the suffix.
- The ordering oracle must assert recursive replay entries have
  `ConfigSetSource::File` or an equivalent internal config-entry representation,
  appear before the first `-e` marker, replay into a fresh config with the
  recursive value applied, and preserve the original initial-command suffix.
- The marker-aware replay test must be failure-sensitive: if a config replay
  entry is after the `-e` marker, the fresh-config recursive value assertion
  would fail because the entry is treated as suffix material or is otherwise not
  applied as config.
- Existing recursive config-file and replay tests still pass:

  ```bash
  cargo test --manifest-path roastty/Cargo.toml config_recursive
  cargo test --manifest-path roastty/Cargo.toml config_replay
  cargo test --manifest-path roastty/Cargo.toml \
    config_theme_loading_preserves_user_replay_entries
  cargo test --manifest-path roastty/Cargo.toml \
    config_conditional_theme_rebuild_preserves_replay_entries_without_duplication
  ```

- The generated load inventory reports:
  - 18 total rows;
  - 17 `Oracle complete` rows;
  - 1 `Audit covered` row;
  - 0 `Gap` rows;
  - 1 incomplete row.
- `LOAD-017` is `Oracle complete`.
- CFG-221 remains `Gap` because `LOAD-001` remains `Audit covered`.
- CFG-217, CFG-218, CFG-219, and CFG-220 remain byte-for-byte unchanged from
  result commit `3001b1880` after final Markdown formatting.
- Hygiene passes:

  ```bash
  cargo fmt --manifest-path roastty/Cargo.toml
  PYTHONDONTWRITEBYTECODE=1 python3 \
    issues/0805-roastty-ghostty-parity/config_load_inventory.py \
    --output issues/0805-roastty-ghostty-parity/config-load-inventory.md \
    --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
  prettier --write --prose-wrap always --print-width 80 \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/101-recursive-replay-suffix.md \
    issues/0805-roastty-ghostty-parity/config-load-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
    issues/0805-roastty-ghostty-parity/config_load_inventory.py
  rm -rf issues/0805-roastty-ghostty-parity/__pycache__
  prettier --check \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/101-recursive-replay-suffix.md \
    issues/0805-roastty-ghostty-parity/config-load-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  git diff --check
  ```

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Initial verdict: Changes required.

Required findings:

- The initial design allowed a helper that only appended a marker and
  representative args, without requiring the replay path to model Ghostty's
  actual `-e` semantics where subsequent replay args are initial-command
  arguments.
- Replaying entries into a fresh config was necessary but not sufficient unless
  the test also proved the initial-command suffix stayed unchanged and contained
  only the original suffix args.
- The `LOAD-017` promotion gate needed a concrete ordering oracle, not
  list-position evidence alone.

Fix:

- Added marker-aware replay semantics to the design scope.
- Added pass criteria requiring recursive entries to remain file/config replay
  entries, appear before the first `-e` marker, apply as config during replay,
  and preserve the original initial-command suffix.
- Added a failure-sensitivity requirement: a config replay entry after the
  marker must fail the recursive-value-applied assertion because it is treated
  as suffix material or otherwise not applied as config.

Final verdict: Approved.
