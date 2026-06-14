# Experiment 97: Auto-update channel finalization

## Description

Experiment 96 left two CFG-220 finalization rows incomplete. FINAL-015 is the
smaller one: pinned Ghostty finalizes an unset `auto-update-channel` to
`build_config.release_channel`, and the pinned build derives that channel from
the app version.

Pinned Ghostty evidence:

- `vendor/ghostty/build.zig.zon` sets `.version = "1.3.2-dev"`;
- `vendor/ghostty/src/build/Config.zig` maps a version with a non-empty
  prerelease component to `.tip`;
- `vendor/ghostty/src/build_config.zig` exports that derived value as
  `release_channel`;
- `vendor/ghostty/src/config/Config.zig::finalize` assigns
  `auto-update-channel = build_config.release_channel` when the config value is
  unset.

Roastty already pins `PINNED_BUILD_RELEASE_CHANNEL` to `ReleaseChannel::Tip`,
and `config_finalize_scalar_tail` proves unset `auto_update_channel` finalizes
to `Tip` while an explicit value is preserved. This experiment will promote only
FINAL-015 from `Audit covered` to `Oracle complete` by recording the source
equivalence in the finalization inventory generator.

It will not modify Rust code, build scripts, parser behavior, formatter
behavior, click-repeat interval behavior, reload behavior, or runtime/UI config
behavior.

## Changes

- `issues/0805-roastty-ghostty-parity/config_finalization_inventory.py`
  - Change only the `auto-update-channel default` row from `Audit covered` to
    `Oracle complete`.
  - Replace missing-evidence text with evidence that combines pinned Ghostty
    source-derived `tip` channel proof and Roastty's existing
    `config_finalize_scalar_tail` finalization oracle.
  - Preserve `click-repeat interval defaulting` as `Audit covered`.

- `issues/0805-roastty-ghostty-parity/config-finalization-inventory.md`
  - Regenerate the inventory. Counts should move from 15/2/0 to 16/1/0.

- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-220 from the finalization inventory. CFG-220 should remain
    `Gap`, with 16 rows `Oracle complete`, 1 row not `Oracle complete`, and 0
    finalization gaps.

- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.
  - Add a learning only if the verification discovers a reusable
    finalization-proof rule or a concrete mismatch.

## Verification

Pass criteria:

- Source assertions prove pinned Ghostty's derived release channel is `tip`:
  - `vendor/ghostty/build.zig.zon` contains `.version = "1.3.2-dev"`;
  - `vendor/ghostty/src/build/Config.zig` still derives `.tip` from non-empty
    prerelease versions;
  - `vendor/ghostty/src/build_config.zig` still exports
    `build_config.release_channel` from build options;
  - `vendor/ghostty/src/config/Config.zig::finalize` still uses
    `build_config.release_channel` for unset `auto-update-channel`.

- The focused Rust oracle passes:

  ```bash
  cargo test --manifest-path roastty/Cargo.toml config_finalize_scalar_tail
  ```

- The finalization inventory generator exits successfully and reports:

  ```text
  finalization_rows=17
  oracle_complete=16
  audit_covered=1
  gap=0
  ```

- A matrix assertion verifies:
  - FINAL-015 is `Oracle complete`;
  - FINAL-015 cites `config_finalize_scalar_tail` and the pinned Ghostty
    release-channel source evidence;
  - FINAL-008 remains `Audit covered`;
  - exactly 17 finalization rows exist;
  - exactly 16 rows are `Oracle complete`;
  - exactly 1 row is not `Oracle complete`;
  - exactly 0 rows are `Gap`;
  - CFG-220 remains `Gap`;
  - CFG-220 points to `config-finalization-inventory.md`;
  - CFG-220 notes the 16/1/0 generated counts.

- The generator must not disturb CFG-217, CFG-218, or CFG-219. Capture all three
  full matrix rows before running the generator and assert they are
  byte-for-byte unchanged after generation and final Markdown formatting.

- Python and Markdown hygiene pass:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
    issues/0805-roastty-ghostty-parity/config_finalization_inventory.py
  rm -rf issues/0805-roastty-ghostty-parity/__pycache__
  prettier --write --prose-wrap always --print-width 80 \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/97-auto-update-channel-finalization.md \
    issues/0805-roastty-ghostty-parity/config-finalization-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  prettier --check \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/97-auto-update-channel-finalization.md \
    issues/0805-roastty-ghostty-parity/config-finalization-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  git diff --check
  ```

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Findings: None.

The reviewer verified that the README links Experiment 97 as `Designed`, the
experiment has the required sections, the scope only promotes FINAL-015 while
preserving FINAL-008 and CFG-220 as incomplete, pinned Ghostty source evidence
supports `tip` for this checkout, Roastty pins the same `ReleaseChannel::Tip`
default and preserves explicit `Stable`, the focused
`config_finalize_scalar_tail` test passed, and no implementation had started
beyond the README link and design document.

## Result

**Result:** Pass.

FINAL-015 now uses pinned Ghostty source evidence plus Roastty's existing scalar
finalization oracle as explicit finalization evidence and is marked
`Oracle complete`. FINAL-008 click-repeat interval defaulting remains
`Audit covered`.

Verification output:

```text
finalization_rows=17
oracle_complete=16
audit_covered=1
gap=0
ghostty_release_channel=tip finalization_rows=17 oracle_complete=16 incomplete=1 gaps=0 cfg220=Gap protected_cfg217_219_unchanged=true
```

The source assertion confirmed that:

- pinned Ghostty `build.zig.zon` uses version `1.3.2-dev`;
- Ghostty's build config derives `.tip` from non-empty prerelease versions;
- `build_config.zig` exports that derived release channel;
- pinned `Config.finalize` assigns unset `auto-update-channel` from
  `build_config.release_channel`;
- Roastty pins `PINNED_BUILD_RELEASE_CHANNEL` to `ReleaseChannel::Tip`.

The focused Rust oracle passed:

```bash
cargo test --manifest-path roastty/Cargo.toml config_finalize_scalar_tail
```

The test passed with 1 passed, 0 failed, and 4966 filtered out.

Additional checks passed:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  issues/0805-roastty-ghostty-parity/config_finalization_inventory.py
prettier --check \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/97-auto-update-channel-finalization.md \
  issues/0805-roastty-ghostty-parity/config-finalization-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Conclusion

Auto-update-channel default finalization is now complete for CFG-220. The
finalization inventory has 16 `Oracle complete` rows and 1 remaining
audit-covered row: click-repeat interval app OS defaulting.

## Completion Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Findings: None.

The reviewer verified that only the five requested issue/inventory files
changed, no Rust code changed, the result was still uncommitted, FINAL-015 is
`Oracle complete` and cites both pinned Ghostty release-channel evidence and
`config_finalize_scalar_tail`, FINAL-008 remains `Audit covered`, the inventory
counts are 17 rows, 16 `Oracle complete`, 1 incomplete, and 0 `Gap`, CFG-220
remains `Gap` with 16/1/0 counts, CFG-217 through CFG-219 are unchanged from the
plan commit, the pinned source chain proves `tip`, and the focused Rust,
Prettier, whitespace, syntax, and table/matrix checks passed.
