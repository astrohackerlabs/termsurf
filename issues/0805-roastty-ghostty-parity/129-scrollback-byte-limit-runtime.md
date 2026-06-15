# Experiment 129: Scrollback Byte Limit Runtime

## Description

`RUNTIME-009B2B2B3` still includes exact nonzero `scrollback-limit` byte quota
parity. Experiment 117 intentionally proved only the `scrollback-limit = 0`
no-history slice because Roastty's app-to-termio bridge mapped nonzero config
values to unbounded history.

Further inspection shows Roastty's lower terminal storage already has a
byte-sized page limit:

- `roastty/src/terminal/page_list.rs` stores `explicit_max_size`,
  `min_max_size`, `page_size`, and prunes on growth against `max_size()`;
- `PageList::init(cols, rows, max_size)` treats `Some(0)` as no scrollback and
  nonzero values as byte limits clamped by the active-area minimum;
- pinned Ghostty's `Screen.Options.max_scrollback` has the same documented
  shape: `0` means no scrollback, nonzero means byte limit clamped to support at
  least the active area.

The main parity gap is therefore the bridge naming and config mapping:
`scrollback_limit_to_rows` currently returns `Some(0)` only for zero and `None`
for every nonzero value. This experiment will pass parsed nonzero
`scrollback-limit` values into terminal initialization as byte limits and prove
bounded runtime behavior with deterministic unit/integration guards.

This experiment will split the remaining terminal row:

- `RUNTIME-009B2B2B3A`: **Oracle complete** for parsed nonzero
  `scrollback-limit` byte-limit wiring, clamped active-area minimum behavior,
  and bounded history pruning through the existing page-list byte quota.
- `RUNTIME-009B2B2B3B`: **Gap** for remaining shell-specific startup rewrite
  coverage, unproven exotic OSC 7 URI edge cases, and other remaining terminal
  behavior effects.

This experiment will not claim every possible scrollback edge case in pinned
Ghostty's terminal test suite. It will prove the config/runtime byte-limit
bridge and the existing page-list byte-pruning mechanism that enforces it.

## Changes

- `roastty/src/lib.rs`
  - Replace `scrollback_limit_to_rows` with a byte-limit mapping helper, or
    rename the helper to reflect byte semantics.
  - Map every parsed `scrollback-limit` value to `Some(limit)` when creating
    `TermioSpawnOptions`, instead of mapping only zero.
  - Keep `usize::MAX`/ABI-only direct terminal behavior compatible with existing
    callers where that path intentionally represents unbounded direct terminal
    state.
  - Update `config_scrollback_limit_runtime_nonzero_allows_surface_history` or
    add a new focused test that proves a small nonzero parsed value is bounded
    while a larger parsed value preserves more history for the same workload.
- `roastty/src/termio.rs`
  - Rename `max_scrollback_rows` to `max_scrollback_bytes` if the change stays
    local and improves correctness; otherwise document the byte semantics at the
    field and call sites.
  - Keep existing zero/default behavior intact.
- `roastty/src/terminal/terminal.rs` and `roastty/src/terminal/screen.rs`
  - Rename `max_scrollback_rows` parameters to `max_scrollback_bytes` where
    practical, because the value is passed through to `PageList::init` as a byte
    limit.
  - Add or update terminal-core tests proving `Some(0)` disables history and a
    small nonzero byte limit prunes old history while preserving at least the
    active area.
- `roastty/src/terminal/page_list.rs`
  - Add a narrow guard if needed to expose or prove byte-limit pruning behavior
    without relying on fragile row counts.
- `issues/0805-roastty-ghostty-parity/scrollback_byte_limit_runtime_parity.py`
  - Add a static guard checking pinned Ghostty's `max_scrollback` markers and
    Roastty's byte-limit bridge/test markers.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Split `RUNTIME-009B2B2B3` into the new Oracle-complete scrollback byte limit
    row and a reduced remaining terminal gap.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-223 summary; it must remain `Gap`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Update Learnings and the experiment index.

## Verification

Pass criteria:

- Parsed `scrollback-limit = 0` still disables PTY-backed surface history.
- Parsed nonzero `scrollback-limit` values are passed to the terminal/page-list
  byte limit instead of being treated as unbounded.
- A small nonzero byte limit prunes old history for a deterministic workload,
  while a larger nonzero byte limit keeps more history for the same workload.
- The active area remains available even when the configured byte limit is
  smaller than the active-area minimum, matching Ghostty's documented clamp.
- Default/unset direct terminal behavior remains compatible with existing tests.
- `RUNTIME-009B2B2B3A` is Oracle complete, `RUNTIME-009B2B2B3B` remains `Gap`,
  and `CFG-223` remains `Gap`.

Commands:

```bash
cargo test --manifest-path roastty/Cargo.toml config_scrollback_limit_runtime
cargo test --manifest-path roastty/Cargo.toml terminal_stream_scrollback_byte_limit
cargo test --manifest-path roastty/Cargo.toml page_list_scrollback_byte_limit
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/scrollback_byte_limit_runtime_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
cargo fmt --manifest-path roastty/Cargo.toml --check
prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/129-scrollback-byte-limit-runtime.md
git diff --check
```

Fail criteria:

- Nonzero parsed `scrollback-limit` still maps to unbounded history.
- The experiment relies only on row-count behavior without proving that the
  byte-limit bridge reaches `PageList`.
- The experiment marks remaining shell rewrites, exotic OSC 7 URI edge cases,
  other terminal effects, or CFG-223 complete.

## Design Review

**Reviewer:** Codex adversarial subagent with fresh context.

**Verdict:** Approved.

The reviewer reported no findings.
