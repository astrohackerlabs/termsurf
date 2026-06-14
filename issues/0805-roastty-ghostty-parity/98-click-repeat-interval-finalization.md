# Experiment 98: Click repeat interval finalization

## Description

FINAL-008 is the last incomplete CFG-220 finalization row. Pinned Ghostty
finalizes `click-repeat-interval = 0` with:

```zig
internal_os.clickInterval() orelse 500
```

On macOS, pinned Ghostty's `internal_os.clickInterval()` calls
`NSEvent.doubleClickInterval`, multiplies seconds by 1000, and rounds up. On
other platforms, it returns `null`, so finalization falls back to `500`.

Roastty already has `crate::os::mouse::click_interval()` with the same platform
shape and conversion, but `Config::finalize_scalars` currently finalizes `0` to
the literal fallback `500` without consulting the OS helper. This experiment
will wire finalization through the OS helper and add a deterministic test seam
for both helper-present and helper-absent behavior.

If this passes, FINAL-008 should become `Oracle complete`, CFG-220 should move
to `Pass`, and config finalization parity should have 17 `Oracle complete` rows
and 0 incomplete rows.

## Changes

- `roastty/src/config/mod.rs`
  - Import or reference `crate::os::mouse`.
  - Add a small finalization helper for click-repeat interval defaulting.
  - When `click_repeat_interval == 0`, set it to
    `mouse::click_interval().unwrap_or(500)`.
  - Add a test-only finalization path or helper that accepts an injected
    `Option<u32>` click interval, so tests can prove both the OS-provided and
    fallback branches without depending on the host's System Settings value.
  - Extend or replace `mouse_behavior_finalize_resolves_and_clamps` so it
    proves:
    - injected `Some(321)` finalizes `0` to `321`;
    - injected `None` finalizes `0` to fallback `500`;
    - nonzero explicit values are preserved;
    - mouse scroll multiplier clamps still run in the same finalization pass.
  - Update `click_repeat_interval_config_parser_family_oracle` so it continues
    to prove parser-level `0` remains `0` before finalization, but does not call
    production `Config::finalize()` and assert a host-dependent literal `500`.
    Its finalization boundary check must use the deterministic injected helper,
    or defer finalization proof to the focused finalization oracle.

- `issues/0805-roastty-ghostty-parity/config_finalization_inventory.py`
  - Change only the `click-repeat interval defaulting` row from `Audit covered`
    to `Oracle complete`.
  - Cite the new/updated click-repeat finalization oracle and the existing
    Roastty OS helper / pinned Ghostty `NSEvent.doubleClickInterval` source
    equivalence.

- `issues/0805-roastty-ghostty-parity/config-finalization-inventory.md`
  - Regenerate the inventory. Counts should move from 16/1/0 to 17/0/0.

- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-220 from the finalization inventory. CFG-220 should move to
    `Pass`, with 17 rows `Oracle complete`, 0 rows not `Oracle complete`, and 0
    finalization gaps.

- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.
  - Add a learning if the implementation confirms the OS-helper parity and
    closes CFG-220.

## Verification

Pass criteria:

- Rust formatting is applied and checked:

  ```bash
  cargo fmt --manifest-path roastty/Cargo.toml
  cargo fmt --manifest-path roastty/Cargo.toml -- --check
  ```

- The focused Rust oracles pass:

  ```bash
  cargo test --manifest-path roastty/Cargo.toml mouse_behavior_finalize_resolves_and_clamps
  cargo test --manifest-path roastty/Cargo.toml click_repeat_interval_config_parser_family_oracle
  cargo test --manifest-path roastty/Cargo.toml click_interval_matches_platform_shape
  cargo test --manifest-path roastty/Cargo.toml seconds_to_millis_ceil_matches_upstream_conversion
  ```

- Source assertions prove:
  - pinned Ghostty still calls `internal_os.clickInterval() orelse 500` for
    `click-repeat-interval` finalization;
  - pinned Ghostty's macOS helper still uses `NSEvent.doubleClickInterval`,
    multiplies by 1000, and rounds up;
  - Roastty's OS helper still uses `NSEvent::doubleClickInterval()` on macOS,
    returns `None` on non-macOS targets, and rounds up seconds to milliseconds;
  - Roastty finalization uses `mouse::click_interval().unwrap_or(500)`.

- The finalization inventory generator exits successfully and reports:

  ```text
  finalization_rows=17
  oracle_complete=17
  audit_covered=0
  gap=0
  ```

- A matrix assertion verifies:
  - FINAL-008 is `Oracle complete`;
  - FINAL-008 cites the click-repeat finalization oracle and OS helper parity;
  - exactly 17 finalization rows exist;
  - exactly 17 rows are `Oracle complete`;
  - exactly 0 rows are not `Oracle complete`;
  - exactly 0 rows are `Gap`;
  - CFG-220 is `Pass`;
  - CFG-220 points to `config-finalization-inventory.md`;
  - CFG-220 notes the 17/0/0 generated counts.

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
    issues/0805-roastty-ghostty-parity/98-click-repeat-interval-finalization.md \
    issues/0805-roastty-ghostty-parity/config-finalization-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  prettier --check \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/98-click-repeat-interval-finalization.md \
    issues/0805-roastty-ghostty-parity/config-finalization-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  git diff --check
  ```

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Initial verdict: Changes required.

Required findings:

- The design did not account for the existing
  `click_repeat_interval_config_parser_family_oracle` assertion that production
  `Config::finalize()` turns a parsed `0` into literal `500`. After routing
  production finalization through the OS helper, that assertion would become
  host-setting-dependent on macOS and the original verification would not catch
  the stale oracle.

Fix:

- Amended the design to explicitly update
  `click_repeat_interval_config_parser_family_oracle` so parser semantics still
  prove parsed `0` remains `0` before finalization, while any finalization
  boundary check uses the deterministic injected helper or is deferred to the
  focused finalization oracle.
- Added `click_repeat_interval_config_parser_family_oracle` to the required
  verification commands.

Final verdict: Approved.

Re-review confirmed the stale host-dependent assertion is now covered by the
experiment plan.

## Result

**Result:** Pass.

Roastty now finalizes `click-repeat-interval = 0` through
`mouse::click_interval().unwrap_or(500)`, matching pinned Ghostty's
`internal_os.clickInterval() orelse 500` behavior. The scalar finalization test
uses an injected click interval to prove both the OS-provided and fallback
branches deterministically, while the parser-family oracle still proves parsed
`0` remains `0` before finalization without making a host-dependent production
`Config::finalize()` assertion.

FINAL-008 is now `Oracle complete`. The regenerated finalization inventory has
17 `Oracle complete` rows, 0 incomplete rows, and 0 finalization gaps. CFG-220
is now `Pass`.

Verification output:

```text
finalization_rows=17
oracle_complete=17
audit_covered=0
gap=0
click_interval_os_helper=true finalization_rows=17 oracle_complete=17 incomplete=0 gaps=0 cfg220=Pass protected_cfg217_219_unchanged=true
```

Focused Rust checks passed:

```bash
cargo test --manifest-path roastty/Cargo.toml mouse_behavior_finalize_resolves_and_clamps
cargo test --manifest-path roastty/Cargo.toml click_repeat_interval_config_parser_family_oracle
cargo test --manifest-path roastty/Cargo.toml click_interval_matches_platform_shape
cargo test --manifest-path roastty/Cargo.toml seconds_to_millis_ceil_matches_upstream_conversion
```

Additional checks passed:

```bash
cargo fmt --manifest-path roastty/Cargo.toml -- --check
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  issues/0805-roastty-ghostty-parity/config_finalization_inventory.py
prettier --check \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/98-click-repeat-interval-finalization.md \
  issues/0805-roastty-ghostty-parity/config-finalization-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Conclusion

CFG-220 validation and finalization behavior is now complete. The next config
facet is CFG-221, source precedence and repeated-file load semantics.

## Completion Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Required findings: None.

The reviewer verified that the result was still uncommitted, scope was limited
to FINAL-008 and expected issue artifacts, production finalization now routes
through `mouse::click_interval().unwrap_or(500)`, tests cover injected OS value,
fallback `500`, nonzero preservation, scroll clamping in the scalar finalization
pass, and parser/finalization boundary without a host-dependent production
finalize assertion, pinned Ghostty and Roastty OS helper shapes match, the
inventory has 17 `Oracle complete` rows and 0 incomplete rows, CFG-220 is `Pass`
with 17/0/0 counts, CFG-217 through CFG-219 are unchanged from the plan commit,
and the formatting, focused Rust, whitespace, source, matrix, and inventory
checks passed.
