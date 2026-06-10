+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
+++

# Experiment 53: Phase E — Unicode table and grapheme parity

## Description

Experiments 51 and 52 added a Ghostty-shaped Unicode facade and rewired
`Terminal::print()` to consume it. The behavior is intentionally representative:
`roastty/src/unicode/mod.rs` still contains hand-written ranges and a simplified
grapheme-break state, while Ghostty uses generated Unicode properties from
`vendor/ghostty/src/unicode/props_uucode.zig` / `props_table.zig` and an 8KB
precomputed grapheme-break table in `vendor/ghostty/src/unicode/grapheme.zig`.

This experiment replaces the representative Unicode implementation with
generated Rust data and a Ghostty-faithful grapheme state machine shape. The
normal Cargo build must not depend on `vendor/ghostty`, because that checkout is
gitignored; generated Rust artifacts must be committed under `roastty/src/` and
the generator/verifier must be runnable when the pinned vendor checkout is
present.

## Changes

- Add a Unicode table generator/verifier script:
  `scripts/roastty-app/generate-unicode-tables.py`.
  - Source property data from Ghostty's generated LUT output
    (`vendor/ghostty/.zig-cache/.../props.zig`).
  - Source grapheme transition data from Ghostty's
    `uucode.x.grapheme.computeGraphemeBreakNoControl` precompute logic by
    generating the same full key space:
    `BreakState × GraphemeBreakNoControl × GraphemeBreakNoControl`.
  - Translate Ghostty property values into Rust arrays without hand-editing the
    generated data.
  - Support exactly two commands:
    `scripts/roastty-app/generate-unicode-tables.py --generate` rewrites the
    committed Rust artifacts, and
    `scripts/roastty-app/generate-unicode-tables.py --check` regenerates to a
    temporary file and fails if the committed artifacts differ.
  - Fail clearly if `vendor/ghostty` is absent or stale, but do not make normal
    `cargo test -p roastty` require the vendor checkout.
- Add committed Rust generated data under `roastty/src/unicode/`, for example
  `tables.rs` and `grapheme_table.rs`.
  - Keep Ghostty's three-stage lookup shape: stage1 maps `cp >> 8`, stage2 maps
    the low byte, and stage3 stores unique `Properties` values.
  - Represent every Ghostty `uucode.x.types.GraphemeBreakNoControl` value needed
    by the table, including the Indic-conjunct classes that are absent from the
    current representative enum.
  - Preserve Ghostty's out-of-range fallback: width `1`,
    `width_zero_in_grapheme = true`, `grapheme_break = Other`,
    `emoji_vs_base = false`.
  - Commit the full grapheme transition table generated from every packed
    Ghostty `Precompute.Key` index. The Rust table must encode both the break
    result and the next `BreakState`, matching Ghostty's `Precompute.Value`.
- Replace the hand-written property classifier in `roastty/src/unicode/mod.rs`
  with table lookup.
  - Keep the public call shape introduced by Exp51/52:
    `get(codepoint) -> Properties` and
    `grapheme_break(previous, current, &mut BreakState) -> bool`.
  - Remove or demote the representative range helpers so the table is the single
    source of truth.
  - Preserve existing terminal call sites; `Terminal::print()` should not need
    another rewrite.
- Replace the simplified grapheme state with a Rust port of Ghostty's
  precomputed break-state transition table.
  - Match Ghostty's `BreakState` semantics closely enough for sequential calls
    to handle long emoji ZWJ sequences, emoji modifiers, regional indicators,
    Hangul syllable clusters, spacing marks, prepend, and Indic conjunct
    sequences.
  - Prefer a compact precomputed transition table over ad hoc conditionals if
    that keeps the implementation closer to
    `vendor/ghostty/src/unicode/grapheme.zig`.
  - Add a non-sample parity verifier path that checks every generated transition
    entry against Ghostty's precompute key/value output. The verifier must cover
    all states and all non-control grapheme break property pairs, not only
    representative strings.
- Tests
  - Keep the existing representative tests from Exp51/52, updating expected
    values only where the full Ghostty table intentionally differs from the
    temporary facade.
  - Add table-shape tests: stage lengths are nonzero, lookup covers `0x0000`,
    `0x00FF`, `0x0100`, `0x10FFFF`, and out-of-range fallback.
  - Add focused property parity cases for:
    - ASCII printable width and C0/control width behavior after terminal control
      filtering.
    - CJK width-2 examples.
    - combining marks, spacing marks, Hangul V/T, and variation selectors.
    - emoji presentation bases and Extended Pictographic values used by the live
      `unicode-width` recipe.
    - at least one Indic conjunct break class absent from Exp51's enum.
  - Add focused grapheme-break cases from Ghostty's `unicode/grapheme.zig` tests
    for emoji modifier and long emoji ZWJ family sequence.
  - Add additional parity cases derived from the full Ghostty transition table
    for regional indicator pairs, Hangul L/V/T/LV/LVT, and Indic
    conjunct/linker/extend behavior.
  - Add a verifier test or script that compares the committed Rust generated
    table to Ghostty's generated `props.zig` when the vendor checkout is
    present. This may be opt-in or script-level, but it must be run and recorded
    for this experiment.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Add this experiment to the index as `Designed`.
  - After implementation, record the durable generation/regeneration command and
    any remaining Unicode limitations.

## Verification

- Run formatting:
  - `cargo fmt -- roastty/src/unicode/mod.rs roastty/src/unicode/tables.rs roastty/src/unicode/grapheme_table.rs`
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/README.md issues/0802-libroastty-completion-and-mac-app/53-unicode-table-parity.md`
- Run generator/verifier:
  - `scripts/roastty-app/generate-unicode-tables.py --check`
  - The `--check` command must verify both committed Rust artifacts:
    `roastty/src/unicode/tables.rs` and `roastty/src/unicode/grapheme_table.rs`.
- Run targeted tests:
  - `cargo test -p roastty unicode`
  - `cargo test -p roastty terminal_stream_print`
- Run full Roastty tests:
  - `cargo test -p roastty`
- Run shell syntax checks if the generator is a shell script:
  - `bash -n scripts/roastty-app/live-ab-smoke.sh`
  - `bash -n scripts/roastty-app/live-ab-matrix.sh`
  - `python3 -m py_compile scripts/roastty-app/generate-unicode-tables.py`
- Run the Unicode live A/B recipe:
  - `scripts/roastty-app/live-ab-smoke.sh --recipe unicode-width --max-mismatch-ratio 1 --max-mean-channel-delta 255`
- Run `git diff --check`.
- Run `git status --short` and verify only intended source/docs/scripts are
  present; no vendor cache files or screenshots are committed.

**Pass** = Roastty uses committed generated Unicode property data and a
Ghostty-faithful grapheme transition table at the existing `unicode::get` /
`unicode::grapheme_break` call sites; the generator/verifier proves parity with
the pinned Ghostty data; focused, full, and live A/B checks pass; no normal
build step depends on gitignored `vendor/ghostty`.

**Partial** = the generated property table lands and all terminal behavior/tests
pass, but the full grapheme transition table or vendor verifier exposes a
bounded parity gap that needs a follow-up experiment; record the exact missing
class or sequence.

**Fail** = the Ghostty Unicode data cannot be generated or represented in
Roastty without a larger build-system decision first.

## Design Review

**Reviewer:** Codex-native adversarial subagent (`multi_agent_v1.spawn_agent`,
fresh context, read-only). **Initial verdict: CHANGES REQUIRED. Final verdict:
APPROVED.**

The reviewer found two Required issues. First, the initial design only required
focused grapheme cases, which would not prove parity with Ghostty's full
precomputed transition table. Fixed by requiring generation/checking of every
`BreakState × GraphemeBreakNoControl × GraphemeBreakNoControl` key and matching
both break result and next state. Second, the initial generator plan used a
wildcard path and deferred the exact command, which made the plan too vague for
a reproducible plan commit. Fixed by naming
`scripts/roastty-app/generate-unicode-tables.py --generate` and `--check`, and
by naming the committed output files. The reviewer also noted an Optional
over-attribution of regional/Hangul/Indic cases to Ghostty's `grapheme.zig`
tests; fixed by separating the actual Ghostty tests from additional transition
table parity cases. Re-review approved with no remaining Required findings.

## Result

**Result:** Pass

Roastty now uses committed generated Unicode artifacts at the existing
`unicode::get` and `unicode::grapheme_break` call sites:

- `scripts/roastty-app/generate-unicode-tables.py` reads Ghostty's generated
  `vendor/ghostty/.zig-cache/.../props.zig`, translates the three-stage property
  LUT into Rust, runs a temporary Zig 0.15 package that calls Ghostty's vendored
  `uucode.x.grapheme.computeGraphemeBreakNoControl` for the full
  `BreakState × GraphemeBreakNoControl × GraphemeBreakNoControl` matrix, and
  rustfmt-normalizes both outputs.
- `roastty/src/unicode/tables.rs` commits the generated `STAGE1`, `STAGE2`, and
  `STAGE3` property arrays.
- `roastty/src/unicode/grapheme_table.rs` commits the generated break/next-state
  transition array.
- `roastty/src/unicode/mod.rs` performs table lookup for codepoint properties
  and table-driven grapheme transitions, while preserving the `Properties`,
  `BreakState`, `get`, and `grapheme_break` API used by `Terminal::print()`.

The generated Ghostty property data intentionally differs from the temporary
Exp51 facade for combining marks and Hangul V/T codepoints: the committed table
reports standalone width `0` with `width_zero_in_grapheme = true`, rather than
the representative facade's width `1`. The terminal print behavior remains
covered by the Exp52 print tests and the live A/B oracle.

Verification run:

- `scripts/roastty-app/generate-unicode-tables.py --generate`
- `cargo fmt -- roastty/src/unicode/mod.rs roastty/src/unicode/tables.rs roastty/src/unicode/grapheme_table.rs`
- `python3 -m py_compile scripts/roastty-app/generate-unicode-tables.py`
- `scripts/roastty-app/generate-unicode-tables.py --check` — verifies property
  data against Ghostty's generated `props.zig` cache and grapheme transitions
  against vendored uucode via Zig 0.15.
- `cargo test -p roastty unicode` — 29 passed, 0 failed.
- `cargo test -p roastty terminal_stream_print` — 21 passed, 0 failed.
- `cargo test -p roastty` — 4,447 unit tests passed, ABI harness passed, doc
  tests passed.
- `bash -n scripts/roastty-app/live-ab-smoke.sh`
- `bash -n scripts/roastty-app/live-ab-matrix.sh`
- `scripts/roastty-app/live-ab-smoke.sh --recipe unicode-width --max-mismatch-ratio 1 --max-mean-channel-delta 255`
  — Pass, content `mean_channel_delta=3.8097902777777777`,
  `mismatch_ratio=0.04077847222222222`.
- `git diff --check`

## Conclusion

The Phase-E Unicode implementation no longer depends on representative
hand-written ranges for the property path or the grapheme state machine. The
normal Roastty build and test path consumes committed Rust artifacts only, while
`scripts/roastty-app/generate-unicode-tables.py --check` provides the durable
vendor-parity gate when the pinned Ghostty checkout is present.

## Completion Review

**Reviewer:** Codex-native adversarial subagent (`multi_agent_v1.spawn_agent`,
fresh context, read-only). **Initial verdict: CHANGES REQUIRED. Final verdict:
APPROVED.**

The reviewer found one Required issue: the first generator implementation built
the grapheme transition table from a Python hand-port of Ghostty's no-control
grapheme rules, so `--check` did not independently prove parity with vendored
Ghostty/uucode. Fixed by removing the Python rule port and making the generator
create a temporary Zig 0.15 package that depends on vendored `uucode` and calls
`uucode.x.grapheme.computeGraphemeBreakNoControl` for every
`BreakState × GraphemeBreakNoControl × GraphemeBreakNoControl` tuple. The
reviewer also raised an Optional concern that the property generator did not
implement the planned deterministic fallback; fixed by narrowing the experiment
and README text to state that property generation requires Ghostty's generated
`props.zig` cache. Re-review approved with no remaining Required findings.
