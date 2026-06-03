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

# Experiment 343: the shaper's reorder guard

## Description

Experiments 341–342 reset the `cell_offset` **unconditionally** at every new
cluster. Upstream resets only when
`is_first_codepoint_in_cluster and !is_after_glyph_from_current_or_next_clusters`.
This experiment ports the **`!is_after_glyph_from_current_or_next_clusters`**
term — the _reorder guard_: a glyph whose cluster is one we have **already
passed** (`cluster <= run_offset.cluster`, the max cluster seen so far) is a
reordered glyph rendered out of input order, and resetting the cell origin to
the current pen for it would mis-position it. So the reset is **skipped**, and
the glyph inherits the current cell.

The companion term `is_first_codepoint_in_cluster` (needing the backward walk
over the UTF-16 padding) is deferred to Experiment 344. It is true when the
glyph's string index is the **first codepoint of its cluster** in the input
stream. For the **scope this experiment covers** — runs where each cluster's
first emitted glyph maps to that cluster's first codepoint (ASCII 1:1, and more
generally non-ligating runs with no _within-cluster_ glyph reordering) —
`is_first_codepoint_in_cluster` is always true, so the upstream condition
reduces exactly to `!is_after`. This experiment is faithful for that scope
(monotonic and _cross-cluster_-reordered alike). Two cases still need Exp 344's
`is_first` term, where it can be false **without** `!is_after` already deciding
the skip:

- **ligatures** — a cluster's first codepoint is consumed into a ligature and
  produces no glyph, so the next glyph for that cluster is not its first
  codepoint; and
- **within-cluster reordering** — CoreText emits a mark/pre-base glyph (a later
  codepoint of the cluster) _before_ the base glyph, so the cluster's
  first-emitted glyph maps to a later codepoint.

Both are out of scope here and deferred to Experiment 344.

## Upstream behavior (`shaper/coretext.zig` `Shaper.shape`)

```zig
var run_offset: Offset = .{};    // pen x + max cluster seen, line-wide
const cluster = state.codepoints.items[index].cluster;
if (cell_offset.cluster != cluster) {
    const is_after_glyph_from_current_or_next_clusters =
        cluster <= run_offset.cluster;
    const is_first_codepoint_in_cluster = …;   // ← Exp 344 (the ligature term)
    if (is_first_codepoint_in_cluster and
        !is_after_glyph_from_current_or_next_clusters)
    {
        cell_offset = .{ .cluster = cluster, .x = run_offset.x };
    }
}
// …emit Cell{ .x = cell_offset.cluster, … }…
run_offset.x += advance.width;
run_offset.cluster = @max(run_offset.cluster, cluster);   // ← max cluster seen
```

`run_offset.cluster` is the maximum cluster among glyphs **already emitted**.
`is_after = cluster <= run_offset.cluster` is true when this glyph belongs to a
cluster at or before one we have already rendered — i.e. CoreText emitted it out
of order (a reordered mark, as in the Bengali/Chakma cases). When `is_after`,
the reset is skipped so the out-of-order glyph stays in the current cell instead
of snapping the cell origin back. When the glyph is a normal forward cluster
(`cluster > run_offset.cluster`), `is_after` is false and the reset happens as
before.

## Rust mapping (`roastty/src/font/face/coretext.rs`)

- Add a line-wide `let mut run_offset_cluster: u32 = 0;` before the run loop
  (upstream's `run_offset.cluster`).
- Make the `cell_offset` reset conditional on `!is_after`:
  ```rust
  if cell_cluster != cluster {
      // Skip the reset for a reordered glyph (one from a cluster we've already
      // passed); it inherits the current cell. (Exp 343: the `is_first`
      // ligature term is Exp 344 — always true for non-ligature runs.)
      let is_after = cluster <= run_offset_cluster;
      if !is_after {
          cell_cluster = cluster;
          cell_x = pen;
      }
  }
  ```
- After emitting the cell and advancing the pen, update the max cluster:
  ```rust
  run_offset_cluster = run_offset_cluster.max(cluster);
  ```

## Scope / faithfulness notes

- **Ported**: the reorder guard — `run_offset.cluster` max-tracking and the
  `!is_after_glyph_from_current_or_next_clusters` term that skips the
  `cell_offset` reset for a glyph from an already-seen cluster.
- **Faithful (scoped)**: the upstream condition `is_first && !is_after` equals
  `!is_after` exactly when `is_first` is true — i.e. for runs where each
  cluster's first **emitted** glyph maps to that cluster's first codepoint
  (ASCII 1:1, and non-ligating runs with no within-cluster glyph reordering).
  This experiment matches upstream for that scope, including **cross-cluster**
  reordering (the `[2, 1, 0]` case, where `is_first` is still true for each
  first-seen glyph).
- **Deferred to Exp 344**: the `is_first_codepoint_in_cluster` term (the
  backward walk over `state.codepoints` skipping surrogate padding). It can be
  false — without `!is_after` already forcing the skip — in two cases: a
  **ligature** (the cluster's first codepoint is consumed and produces no glyph)
  and **within-cluster reordering** (CoreText emits a later-codepoint
  mark/pre-base glyph before the base, so the first-emitted glyph for the
  cluster is not its first codepoint). Both need a complex-shaping font; this
  experiment's scope excludes them.
- **Deferred** (unchanged): the special-font fast path, the `Shaper` struct +
  `RunIterator`, the variation-axis score, and variations application.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/font/face/coretext.rs`: add `run_offset_cluster` tracking; gate
   the `cell_offset` reset on `!is_after`.
2. Tests (in `coretext.rs`):
   - `shape_run_reorder_skips_reset`: **mechanically** exercises the `is_after`
     guard (not full complex-shaping). `shape_run` over `['A', 'B', 'C']` with
     **synthetic descending clusters** `[2, 1, 0]` (a stand-in for reordered
     output: each later glyph maps to an earlier cluster) yields cells with
     `x = [2, 2, 2]` — `'A'` resets to cell `2`, then `'B'` (cluster `1 ≤ 2`)
     and `'C'` (cluster `0 ≤ 2`) are "after" a seen cluster, so their resets are
     skipped and they inherit cell `2`. Under the Experiment 342 unconditional
     reset this would have been `[2, 1, 0]`, so the test distinguishes the
     guard. Deterministic: Menlo emits one glyph per ASCII scalar in string
     order, so each first-seen glyph maps to its cluster's first codepoint
     (`is_first` true throughout — the deferred term does not affect this case).
   - `shape_run_forward_clusters_unchanged`: `shape_run` over `['A', 'B', 'C']`
     with `[0, 1, 2]` still yields `x = [0, 1, 2]` (every new cluster is
     forward, `is_after` is false, the reset always happens) — the guard does
     not disturb monotonic runs. The existing combining-marks (`[0, 0, 0, 1]` →
     folds) and all `shape_codepoints`-based tests (sequential,
     strictly-increasing clusters) still pass unchanged.
3. Format and test (`cargo fmt`, accept output).

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

- `shape_run` tracks `run_offset_cluster` (the max cluster emitted) and skips
  the `cell_offset` reset when `cluster <= run_offset_cluster` — faithful to
  upstream's `!is_after` term, and exact for the scoped runs (each cluster's
  first emitted glyph maps to its first codepoint; `is_first` deferred to Exp
  344);
- the reorder-skip and forward-clusters tests pass, and the existing shaping
  tests still pass unchanged;
- the `is_first` ligature term, the special-font path, the
  `Shaper`/`RunIterator`, the variation-axis score, and variations stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the reorder guard or the `run_offset.cluster`
tracking diverges from upstream (wrong comparison, updating the max before the
check, resetting when it should skip), or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **one Required
finding**, now fixed:

- **Required (fixed):** the draft claimed the `!is_after` term alone is faithful
  for _all_ non-ligature runs. That is too broad:
  `is_first_codepoint_in_cluster` can be false **without** a ligature when
  CoreText emits a mark/pre-base glyph (a later codepoint of a cluster) _before_
  the base glyph — _within-cluster_ reordering — so the cluster's first-emitted
  glyph maps to a later codepoint. In that case upstream would skip the reset
  (`is_first == false`) while `!is_after` alone would reset. The scope claim was
  narrowed to runs where each cluster's first emitted glyph maps to that
  cluster's first codepoint (ASCII 1:1 and non-ligating,
  non-within-cluster-reordered runs), and within-cluster mark reordering was
  added alongside ligatures as deferred to Experiment 344.

Codex confirmed the rest is correct: the `run_offset_cluster` update timing
(`max` after emit/advance, the check comparing against the max over prior
emitted glyphs) matches upstream; the `[2, 1, 0] → [2, 2, 2]` expectation is
correct against upstream for the ASCII 1:1 case (each first-seen glyph maps to
its cluster's first codepoint, so `is_first` is true throughout); `<=` is the
correct comparison (equal cluster counts as "current or next clusters already
seen", and `<` would diverge on equal-cluster edges); and the synthetic
descending-cluster test is a legitimate deterministic exercise of the `is_after`
branch, as long as it is described as mechanically testing the guard rather than
full complex-shaping (the test comment now says so).

Review artifacts:

- Prompt: `logs/codex-review/20260603-133420-139953-prompt.md` (design)
- Result: `logs/codex-review/20260603-133420-139953-last-message.md` (design)
