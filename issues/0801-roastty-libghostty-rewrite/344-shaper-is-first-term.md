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

# Experiment 344: the shaper's `is_first` term

## Description

Experiment 343 made the `cell_offset` reset conditional on `!is_after`,
deferring the companion term `is_first_codepoint_in_cluster`. This experiment
ports that term, completing the full upstream condition
`is_first_codepoint_in_cluster and !is_after_glyph_from_current_or_next_clusters`.

`is_first_codepoint_in_cluster` walks **backward** from the glyph's string index
over the input codepoints (skipping surrogate-pad units, whose codepoint is `0`)
and is true when the nearest real predecessor has a **different** cluster — i.e.
this glyph is the first codepoint of its cluster in the input stream. When it is
**false**, the glyph maps to a _later_ codepoint of its cluster whose first
codepoint did not produce its own glyph: a **ligature** (the first codepoint was
consumed) or **within-cluster reordering** (a mark/pre-base glyph emitted before
the base). In those cases the reset is skipped so the glyph stays aligned to the
ligature/base instead of snapping the cell origin to the grid.

## Upstream behavior (`shaper/coretext.zig` `Shaper.shape`)

```zig
const is_first_codepoint_in_cluster = blk: {
    var i = index;
    while (i > 0) {
        i -= 1;
        const codepoint = state.codepoints.items[i];
        if (codepoint.codepoint == 0) continue;   // skip surrogate-pair padding
        break :blk codepoint.cluster != cluster;
    } else break :blk true;                        // no real predecessor → first
};

if (is_first_codepoint_in_cluster and
    !is_after_glyph_from_current_or_next_clusters)
{
    cell_offset = .{ .cluster = cluster, .x = run_offset.x };
}
```

`state.codepoints` is indexed by UTF-16 offset; a surrogate pair's low half is a
padding entry with `codepoint == 0`. The walk skips those padding units (they
are not real codepoints), finds the nearest real predecessor, and reports
whether its cluster differs. If there is no real predecessor (the start of the
run, or only padding precedes), the glyph is the first codepoint → `true`.

## Rust mapping (`roastty/src/font/face/coretext.rs`)

- In `shape_run`, build a `pads: Vec<bool>` parallel to `clusters` (same UTF-16
  indexing): for each input entry's `ch`, the **first** UTF-16 unit carries the
  real codepoint and any extra unit (a surrogate low half) is padding. A unit is
  "padding" exactly when its codepoint is `0` — matching upstream's
  `codepoint.codepoint == 0`, which also treats a real `U+0000` as padding:
  ```rust
  for u in 0..ch.len_utf16() {
      clusters.push(cp.cluster);
      pads.push(if u == 0 { cp.codepoint == 0 } else { true });
  }
  ```
- Extract the backward walk as a free function, mirroring upstream exactly:
  ```rust
  /// Whether the glyph at UTF-16 index `idx` is the first codepoint of `cluster`
  /// in the input stream: walk backward skipping surrogate-pad units (codepoint
  /// `0`) and report whether the nearest real predecessor has a different
  /// cluster. No real predecessor ⇒ first. Faithful port of upstream's
  /// `is_first_codepoint_in_cluster`.
  fn is_first_codepoint_in_cluster(
      clusters: &[u32],
      pads: &[bool],
      idx: usize,
      cluster: u32,
  ) -> bool {
      let mut j = idx;
      while j > 0 {
          j -= 1;
          if pads[j] {
              continue;
          }
          return clusters[j] != cluster;
      }
      true
  }
  ```
- Gate the reset on the full condition:
  ```rust
  if cell_cluster != cluster {
      let is_after = cluster <= run_offset_cluster;
      let is_first = is_first_codepoint_in_cluster(&clusters, &pads, idx, cluster);
      if is_first && !is_after {
          cell_cluster = cluster;
          cell_x = pen;
      }
  }
  ```

## Scope / faithfulness notes

- **Ported**: the `is_first_codepoint_in_cluster` backward walk (with surrogate-
  pad skipping) and the full upstream reset condition `is_first && !is_after`.
  With Experiment 343's `run_offset.cluster` tracking, this completes the
  cluster→cell reset heuristic of `Shaper.shape`.
- **Behavior preserved**: the existing tests are unchanged, but the reason is
  subtle. `is_first` is only consulted when the reset branch is _entered_, i.e.
  when `cell_cluster != cluster` (a cluster transition). In every existing case
  (ASCII 1:1, combining marks, the `[2, 1, 0]` reorder, the surrogate collapse),
  the **first emitted glyph of each cluster maps to that cluster's first
  codepoint**, so `is_first` is true at exactly those reset-relevant transitions
  — leaving the output identical to Experiment 343. A _later_ same-cluster glyph
  (e.g. the second combining mark) may itself have `is_first == false`, but it
  does **not** enter the reset branch (`cell_cluster == cluster` already), so
  its `is_first` value is never used. The new term changes output only for
  ligatures / within-cluster reordering (a glyph that enters the reset branch
  while mapping to a non-first codepoint of its cluster), which need a
  complex-shaping font to produce.
- **Deferred** (unchanged): the special-font fast path, the `Shaper` struct +
  `RunIterator` (which would feed real grapheme clusters and ligature runs), the
  variation-axis score, and variations application.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/font/face/coretext.rs`: build the `pads` array in `shape_run`;
   add the free `is_first_codepoint_in_cluster` walk; gate the reset on
   `is_first && !is_after`.
2. Tests (in `coretext.rs`):
   - `is_first_codepoint_in_cluster_walk`: a focused unit test of the extracted
     free function over **synthetic** `clusters`/`pads` arrays —
     deterministically covering the walk's logic:
     - `idx == 0` → `true` (no predecessor);
     - nearest real predecessor in a **different** cluster → `true`
       (`clusters = [3, 5]`, `pads = [f, f]`, `idx = 1`, `cluster = 5`);
     - nearest real predecessor in the **same** cluster → `false`
       (`clusters = [5, 5]`, `pads = [f, f]`, `idx = 1`, `cluster = 5`);
     - a surrogate-pad unit is **skipped** to reach the real predecessor
       (`clusters = [5, 5, 5]`, `pads = [f, t, f]`, `idx = 2`, `cluster = 5` →
       skips the pad at `1`, finds the same-cluster real unit at `0` → `false`;
       and `clusters = [3, 3, 5]`, `pads = [f, t, f]`, `idx = 2`, `cluster = 5`
       → `true`);
     - only padding precedes → `true` (`clusters = [9, 9]`, `pads = [t, f]`,
       `idx = 1`, `cluster = 9` — matching `clusters[idx]` as in production; the
       lone predecessor is padding and is skipped, so the loop exhausts →
       `true`).
   - `shape_run_full_condition_regression`: re-asserts the Experiment 343
     outcomes under the full condition — `['A', 'B', 'C']` with `[2, 1, 0]`
     still `→ [2, 2, 2]`, with `[0, 1, 2]` still `→ [0, 1, 2]`, and the
     combining-marks `[0, 0, 0, 1]` still folds — confirming `is_first` does not
     disturb these cases (it is true at each reset-relevant cluster transition,
     where the first emitted glyph of the new cluster maps to its first
     codepoint).
   - All existing `shape_*` tests still pass unchanged (the surrogate-collapse
     test now additionally exercises the pad-skipping walk at runtime).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty shape
cargo test -p roastty is_first
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `shape_run` builds the `pads` array and gates the `cell_offset` reset on the
  full `is_first && !is_after` condition, with `is_first_codepoint_in_cluster`
  faithfully porting upstream's backward, pad-skipping walk;
- the walk unit test and the full-condition regression pass, and all existing
  shaping tests still pass unchanged;
- the special-font path, the `Shaper`/`RunIterator`, the variation-axis score,
  and variations stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the walk diverges from upstream (wrong direction,
not skipping padding, wrong cluster comparison, mishandling `idx == 0`), the
full condition is mis-assembled, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **one Required
finding**, now fixed:

- **Required (fixed):** the faithfulness wording overclaimed that `is_first` is
  "true throughout" the existing cases (notably combining marks). That is not
  generally true — a _later_ same-cluster glyph can have `is_first == false`
  because its nearest real predecessor shares the cluster. The accurate reason
  the existing tests are unchanged: `is_first` is only consulted when the reset
  branch is entered (`cell_cluster != cluster`), and at exactly those cluster
  transitions in the scoped cases the first emitted glyph maps to its first
  codepoint (`is_first` true); same-cluster later glyphs may have
  `is_first == false` but never reach the reset branch. The scope/notes and the
  regression-test wording were corrected accordingly.

Codex confirmed the rest is faithful: the `pads` construction
(`pad = codepoint == 0` for the first unit, `true` for surrogate low halves)
mirrors upstream's padded `state.codepoints` and its `codepoint == 0` skip,
including the real `U+0000` edge; the extracted walk is byte-faithful (starts at
`idx`, walks backward, skips pads, compares the nearest real predecessor's
cluster, returns `true` for `idx == 0` or all-padding predecessors);
`is_first && !is_after` is the correct full condition; and the synthetic walk
vectors are correct. Per Codex's clarity suggestion, the "only padding precedes"
vector now uses `cluster = 9` to match `clusters[idx]` as in production.
Implementation guidance folded in: keep the helper free/private, keep
`pads.len() == clusters.len()`, and call it only with the same UTF-16 `idx` used
to load `cluster`.

Review artifacts:

- Prompt: `logs/codex-review/20260603-134109-330535-prompt.md` (design)
- Result: `logs/codex-review/20260603-134109-330535-last-message.md` (design)
