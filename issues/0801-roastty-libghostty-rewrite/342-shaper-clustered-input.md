+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"
+++

# Experiment 342: the shaper's clustered input

## Description

Experiment 341 maps glyphs to clusters, but derives the cluster from the **input
scalar index** — so every codepoint is its own cell and a grapheme like `n̈`
(base + two combining marks) splits across three cells instead of one.
Upstream's `shape` is fed a stream of `(codepoint, cluster)` pairs (via
`addCodepoint`), where the **cluster is supplied by the caller** (the run
iterator, which groups a grapheme's codepoints into one terminal cell). This
experiment ports that **clustered input contract**: a faithful
`(codepoint, cluster)` shaping entry point, with `shape_codepoints(&[u32])` kept
as a thin wrapper that assigns the sequential cluster (preserving Experiment
341's behavior).

This makes the grapheme-cluster behavior testable now — without the full
`RunIterator` — by supplying the clusters directly, exactly as upstream's tests
do. The _conditional_ ligature/mark heuristic (the skip logic) remains deferred
to Experiment 343; the unconditional reset from Experiment 341 already
reproduces the combining-marks-share-a-cell behavior when the clusters are
supplied.

## Upstream behavior (`shaper/coretext.zig`)

```zig
// The run iterator feeds (codepoint, cluster) pairs; the cluster is the
// terminal cell, which groups a grapheme's codepoints:
pub fn addCodepoint(self: RunIteratorHook, cp: u32, cluster: u32) !void {
    // …append cp to the UTF-16 string (with surrogate padding)…
    try state.codepoints.append(alloc, .{ .codepoint = cp, .cluster = cluster });
    // …pad entry { .codepoint = 0, .cluster = cluster } for a surrogate pair…
}

// shape() then reads the cluster back per glyph:
const cluster = state.codepoints.items[index].cluster;
```

So the shaping input is fundamentally a list of `(codepoint, cluster)` pairs,
and `Cell.x` is the cluster the caller assigned. For `n̈a` with grapheme
clustering, the run iterator assigns cluster `0` to `n` and both `U+0308` marks,
and cluster `1` to `a` — yielding cells `x = [0, 0, 0, 1]` (upstream's "shape
Combining characters" test).

## Rust mapping (`roastty/src/font/face/coretext.rs`, `shape.rs`)

- `roastty/src/font/shape.rs`: add the input pair type, mirroring upstream's
  `RunState.codepoints` entries:
  ```rust
  /// One input codepoint paired with its cluster (the source cell). Mirrors
  /// upstream's `addCodepoint(cp, cluster)` stream.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub(crate) struct Codepoint {
      pub codepoint: u32,
      pub cluster: u32,
  }
  ```
- `roastty/src/font/face/coretext.rs`: rename the current shaping body to
  `shape_run(&self, run: &[shape::Codepoint]) -> Vec<shape::Cell>`, building
  `text` + `clusters` from `run` (push each entry's `cluster` once per UTF-16
  unit, instead of the scalar index). The `cell_offset` tracking and the
  unconditional reset are unchanged from Experiment 341.
- Keep `shape_codepoints(&self, codepoints: &[u32])` as a thin wrapper that maps
  each scalar to `shape::Codepoint { codepoint, cluster: i as u32 }` and calls
  `shape_run` — preserving all existing behavior and tests.

## Scope / faithfulness notes

- **Ported**: the clustered shaping input — the `(codepoint, cluster)` stream
  that `shape` consumes, with the caller-supplied cluster driving `Cell.x`. This
  is upstream's actual shaping contract (`RunState.codepoints`).
- **Faithful**: `shape_codepoints` (sequential cluster) is now a convenience
  wrapper over `shape_run`; its output is byte-for-byte what Experiment 341
  produced.
- **Deferred to Exp 343**: the _conditional_ reset (the ligature/mark heuristic:
  `is_first_codepoint_in_cluster` and
  `!is_after_glyph_from_current_or_next_clusters`, plus `run_offset.cluster`).
  With supplied clusters and the unconditional reset, marks that share their
  base's cluster already stay in the base's cell (no reset is attempted for
  them); the heuristic only changes behavior for ligatures and out-of-order
  (reordered) marks, which Exp 343 covers.
- **Deferred** (unchanged): the special-font fast path, the full `Shaper` +
  `RunIterator` (which would supply real grapheme clusters), the variation-axis
  score, and variations application.
- No C ABI/header/ABI-inventory change (`shape::Codepoint` and `shape_run` are
  internal `pub(crate)` Rust).

## Changes

1. `roastty/src/font/shape.rs`: add the `Codepoint { codepoint, cluster }` input
   struct.
2. `roastty/src/font/face/coretext.rs`: extract `shape_run(&[shape::Codepoint])`
   from the current `shape_codepoints` body (cluster from the input entry);
   reduce `shape_codepoints(&[u32])` to a wrapper assigning the sequential
   cluster.
3. Tests (in `coretext.rs`):
   - `shape_run_combining_marks`: `shape_run` over
     `[(‘n’, 0), (0x0308, 0), (0x0308, 0), (‘a’, 1)]` (base + two combining
     diaereses grouped into cell `0`, then `a` in cell `1`) yields cells whose
     `x` are all `≤ 1`, with at least one cell at `x == 0` and the final cell at
     `x == 1` — i.e. the marks fold into the base's cell (`0`), **not** their
     own cells (`1`/`2`, which the sequential mapping would have produced).
     Robust to how many glyphs the host emits for the grapheme.
   - `shape_run_matches_sequential_wrapper`:
     `shape_codepoints(&['A', 'B', 'C'])` equals `shape_run` over
     `[(‘A’, 0), (‘B’, 1), (‘C’, 2)]` — the wrapper is exactly the
     sequential-cluster case.
   - The existing `shape_*` tests (which call `shape_codepoints`) still pass
     unchanged.
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty shape
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `shape_run` consumes a `(codepoint, cluster)` stream and drives `Cell.x` from
  the supplied cluster, and `shape_codepoints` is a faithful sequential-cluster
  wrapper over it;
- the combining-marks and wrapper-equivalence tests pass, and the existing
  shaping tests still pass unchanged;
- the ligature/mark heuristic, the special-font path, the
  `Shaper`/`RunIterator`, the variation-axis score, and variations stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the host emits an unexpected glyph
decomposition for `n̈` that changes the cell count (the cluster-folding
post-condition is still asserted on whatever cells are produced).

The experiment **fails** if `shape_run` does not faithfully consume the
clustered input, the wrapper changes existing behavior, or any public C API/ABI
changes.

## Design Review

Codex reviewed this design before implementation and found **no Required
findings**. It confirmed the scoped contract is sound:

- With supplied clusters `[0, 0, 0, 1]`, the **unconditional reset is enough**
  for the combining-mark case: cluster-`0` glyphs see `cell_cluster == 0`, so no
  reset is attempted for the base or the marks; when the `a` glyph maps to
  cluster `1`, `cell_cluster != 1` and the cell origin resets to the current pen
  — yielding `x = [0, 0, 0, 1]` without the heuristic.
- `shape::Codepoint { codepoint, cluster }` is the right Rust representation of
  upstream's `addCodepoint(cp, cluster)` input contract, with `shape_run`
  building the UTF-16-indexed reverse table (and surrogate padding) from the
  supplied clusters.
- `shape_codepoints` as a sequential wrapper preserves Experiment 341 behavior
  so long as it assigns `cluster: i as u32` and `shape_run` remains the place
  that filters invalid `char::from_u32` scalars (keeping the cluster/UTF-16
  alignment).
- The wrapper-equivalence test is strong and should catch accidental extraction
  drift.

Non-blocking test note (acknowledged): `shape_run_combining_marks` is robust but
intentionally weak if CoreText precomposes the base + marks into fewer glyphs —
`final x == 1` still holds (`a` is the final glyph in this simple LTR string),
but the test may not always prove separate mark glyphs were emitted. That is
acceptable for this experiment's clustered-input contract; the distinctive
heuristic paths are Experiment 343's concern.

Review artifacts:

- Prompt: `logs/codex-review/20260603-132730-187558-prompt.md` (design)
- Result: `logs/codex-review/20260603-132730-187558-last-message.md` (design)

## Result

**Result:** Pass

The shaper now accepts the caller-supplied cluster stream.

- `roastty/src/font/shape.rs`: added
  `pub(crate) struct Codepoint { codepoint: u32, cluster: u32 }`
  (Debug/Clone/Copy/PartialEq/Eq), mirroring upstream's `RunState.codepoints`
  entries fed by `addCodepoint(cp, cluster)`.
- `roastty/src/font/face/coretext.rs`: extracted the shaping body into
  `shape_run(&self, run: &[shape::Codepoint]) -> Vec<shape::Cell>`, which builds
  `text` + `clusters` from the run (pushing each entry's `cluster` once per
  `ch.len_utf16()`, skipping invalid `char::from_u32` so `clusters` stays
  aligned with the CoreText UTF-16 indices). The `cell_offset` tracking, the
  unconditional reset, the non-LTR sort, and the offsets are unchanged from
  Experiments 339–341. `shape_codepoints(&[u32])` is now a thin wrapper
  assigning each scalar the sequential cluster (`i`) and calling `shape_run`.

Tests: `shape_run_combining_marks`
(`[(‘n’, 0), (0x0308, 0), (0x0308, 0), (‘a’, 1)]` → cells non-empty, all
`x ≤ 1`, some `x == 0`, last `x == 1`), confirming the marks fold into the
base's cell; `shape_run_matches_sequential_wrapper` (`shape_codepoints("ABC")`
== `shape_run([(‘A’, 0), (‘B’, 1), (‘C’, 2)])`), confirming the wrapper is
behavior-preserving. All prior `shape_*` tests pass unchanged.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2750 passed, 0 failed (+2, no regressions).
- `cargo build -p roastty` → no warnings (`Codepoint` is used by
  `shape_codepoints`).
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The shaper's input contract is now upstream's `(codepoint, cluster)` stream:
`shape_run` consumes caller-supplied clusters (grouping a grapheme's codepoints
into one cell), and `shape_codepoints` is a sequential-cluster convenience
wrapper. The combining-marks test proves that supplied clusters fold a
grapheme's marks into the base's cell — the behavior the `RunIterator` will rely
on.

The remaining shaper work: the **ligature/mark heuristic** (Exp 343 — the
_conditional_ `cell_offset` reset via `is_first_codepoint_in_cluster` and
`!is_after_glyph_from_current_or_next_clusters`, plus the `run_offset.cluster`
max-tracking it consumes — now testable against the clustered input); the
**special-font** fast path (codepoint == glyph); and the `Shaper` struct with
its run state, caching, and the **`RunIterator`** over terminal cells (which
supplies real grapheme clusters). The deferred **variation-axis** `score()`
refinement and **variations** application also remain.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no Required findings**. It confirmed: `shape_run` consumes caller-supplied
`(codepoint, cluster)` pairs and builds the UTF-16-indexed `clusters` table from
`cp.cluster` (including surrogate-pair duplication), mirroring upstream's
`addCodepoint` + padding model; `shape_codepoints` preserves Experiment 341
behavior by assigning `cluster = i` before calling `shape_run`, with invalid
scalar filtering still in the shared body (so the "skip invalid but keep the
input-index cluster" behavior is unchanged); `shape::Codepoint` is the right
internal representation of the caller-facing `addCodepoint` stream (the
surrogate dummy entries remain an internal detail of the reverse table); and the
combining-marks test soundly verifies the clustered-input contract (supplied
clusters constrain output to cells `0`/`1`, with `'a'` landing in cluster `1`).
The deferred scope is intact — no `run_offset.cluster` bookkeeping or
conditional heuristic was introduced, and the special-font path, full
`Shaper`/`RunIterator`, and variations remain deferred.

Review artifacts:

- Result review: `logs/codex-review/20260603-133025-465033-last-message.md`
