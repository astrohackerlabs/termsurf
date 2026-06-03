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

# Experiment 341: the shaper's cluster→cell mapping

## Description

Experiments 338–340 set each cell's `x` to the raw CoreText UTF-16 **string
index** and computed `x_offset` against the line-wide pen. Upstream instead maps
each glyph back to its **cluster** (the terminal cell it belongs to) and tracks
a per-cell `cell_offset` — the cluster and the pen-x captured at that cell's
start — so `Cell.x` is the cluster and `x_offset = position.x − cell_offset.x`.
This experiment ports that cluster→cell mapping and the `cell_offset` tracking.

The one piece still deferred is the **ligature/mark heuristic** (the
_conditional_ `cell_offset` reset that avoids re-aligning glyphs which mark a
ligature). For monotonic forward (LTR, non-ligature) runs — the common case —
upstream's condition is always true at each new cluster, so the reset is
**unconditional** there. This experiment ports that unconditional reset;
Experiment 342 will add the heuristic for ligatures and out-of-order marks.

## Upstream behavior (`shaper/coretext.zig` `Shaper.shape`)

```zig
var run_offset: Offset = .{};    // pen x + max cluster seen, line-wide
var cell_offset: Offset = .{};   // the current cell's starting x + cluster
for (line.getGlyphRuns()) |ctrun| {
    for (glyphs, advances, positions, indices) |glyph, advance, position, index| {
        const cluster = state.codepoints.items[index].cluster;
        if (cell_offset.cluster != cluster) {
            // …ligature heuristic decides whether to reset… (deferred to Exp 342)
            // For monotonic forward runs the reset always happens:
            cell_offset = .{ .cluster = cluster, .x = run_offset.x };
        }
        const x_offset = position.x - cell_offset.x;
        cell_buf.append(.{
            .x = @intCast(cell_offset.cluster),
            .x_offset = @intFromFloat(@round(x_offset)),
            .y_offset = @intFromFloat(@round(position.y)),
            .glyph_index = glyph,
        });
        run_offset.x += advance.width;
        run_offset.cluster = @max(run_offset.cluster, cluster);
    }
}
```

`state.codepoints.items[index]` maps a **UTF-16 string index** back to the
source codepoint and its **cluster**. Upstream pads surrogate pairs so the array
is indexed by UTF-16 offset; both halves of a non-BMP scalar share one cluster.
The cluster is the glyph's terminal cell. `cell_offset.x` is the pen value
captured when that cell started, so within a multi-glyph cell `x_offset` is
measured from the cell's origin (not the running pen), while `Cell.x` is the
cell (cluster).

## Cluster source for this slice

Upstream's clusters come from the terminal grid via the `RunIterator` (still
deferred). Until that lands, this slice uses the **input scalar index** as each
codepoint's cluster — i.e. for a contiguous run with no grapheme grouping, each
scalar is its own cell, which is the faithful stand-in. `Cell.x` therefore
becomes the input-scalar cluster rather than the raw UTF-16 index; the two
coincide for BMP text and differ only when a non-BMP scalar (a surrogate pair)
precedes a glyph (the cluster collapses the pair to one cell).

## Rust mapping (`roastty/src/font/face/coretext.rs`)

- Build the run's `text` and a parallel `clusters: Vec<u32>` together: for each
  input scalar at index `i` that is a valid `char`, push `i` once per UTF-16
  unit (`ch.len_utf16()` — `1` for BMP, `2` for a surrogate pair). `clusters` is
  then indexed by the same UTF-16 offset CoreText reports, and both halves of a
  surrogate pair share the scalar's cluster (mirroring upstream's padding).
- Declare line-wide, before the run loop: `let mut pen: f64 = 0.0;` (the
  `run_offset.x`), `let mut cell_cluster: u32 = 0;` and
  `let mut cell_x: f64 = 0.0;` (the `cell_offset`). They persist across runs, as
  upstream's do.
- Per glyph `k`:
  ```rust
  let idx = indices[k].max(0) as usize;
  debug_assert!(idx < clusters.len());
  let cluster = clusters.get(idx).copied().unwrap_or(0);
  if cell_cluster != cluster {
      // Exp 341: unconditional reset (the ligature heuristic is Exp 342).
      cell_cluster = cluster;
      cell_x = pen;
  }
  cells.push(shape::Cell {
      x: cell_cluster as u16,
      x_offset: (positions[k].x - cell_x).round() as i16,
      y_offset: positions[k].y.round() as i16,
      glyph_index: glyphs[k] as u32,
  });
  pen += advances[k].width;
  ```
  (`run_offset.cluster` — the max-cluster bookkeeping — is only consumed by the
  deferred heuristic, so it is added in Experiment 342, not here.)

## Scope / faithfulness notes

- **Ported**: the cluster→cell mapping (`Cell.x` = the glyph's cluster) and the
  `cell_offset` tracking (`x_offset = position.x − cell_offset.x`, with
  `cell_offset.x` captured at each cell start), line-wide across runs — the core
  cell-positioning of `Shaper.shape` for monotonic runs.
- **Faithful simplification (deferred to Exp 342)**: the _conditional_ reset —
  the ligature/mark heuristic (`is_first_codepoint_in_cluster` and
  `!is_after_glyph_from_current_or_next_clusters`) and its `run_offset.cluster`
  bookkeeping. For monotonic forward runs the upstream condition is always true
  at a new cluster, so the unconditional reset here is **identical** to
  upstream; it diverges only for ligatures and out-of-order marks (complex
  shaping), which Exp 342 handles.
- **Faithful stand-in (deferred to the `RunIterator`)**: the cluster source —
  here the input scalar index, pending the terminal-grid clusters.
- **Deferred** (unchanged): the special-font fast path, the `Shaper` struct +
  `RunIterator`, the variation-axis score, and variations application.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/font/face/coretext.rs`: build the `clusters` array alongside
   `text`; track `cell_cluster`/`cell_x` line-wide; set `Cell.x` to the cluster
   and `x_offset` to `position.x − cell_x`.
2. Update the existing `shape_ascii_monospace` comment (the `x` is now the
   cluster, which equals the index for ASCII — the assertion `x == i` is
   unchanged).
3. Tests (in `coretext.rs`):
   - `shape_cluster_collapses_surrogate`: shaping
     `['A', 0x1D400 (𝐀, non-BMP), 'B']` yields a cell whose `glyph_index` equals
     `face.glyph_index('B')` and whose `x` is `2` (the cluster), **not** `3`
     (the raw UTF-16 index). This proves the surrogate pair collapses to one
     cell and `Cell.x` is the cluster, not the UTF-16 offset. ('A' is cluster
     `0`, 𝐀 is cluster `1` spanning UTF-16 units `1–2`, 'B' is cluster `2` at
     UTF-16 unit `3`; whatever font the host uses for 𝐀, CoreText assigns 'B'
     string index `3` → cluster `2`.)
   - `shape_clusters_monospace`: Menlo `"ABC"` → cells with `x = 0, 1, 2` and
     all `x_offset == 0` (each ASCII scalar is its own cell; `cell_x` resets to
     the pen at each, so the offset is zero). Confirms the cell path matches the
     prior per-glyph result for the common case. The existing
     `shape_ascii_monospace`/`shape_plain_offsets_zero`/`shape_advances_monotonic`/
     `shape_ltr_stays_sorted` tests still pass.
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

- `shape_codepoints` maps each glyph to its cluster (via the UTF-16-indexed
  `clusters` array), sets `Cell.x` to the cluster, and computes
  `x_offset = round(position.x − cell_x)` with line-wide `cell_offset` tracking
  and an unconditional reset at each new cluster — faithful to upstream for
  monotonic runs;
- the surrogate-collapse and clusters-monospace tests pass, and the existing
  shaping tests still pass;
- the ligature heuristic, the special-font path, the `Shaper`/`RunIterator`, the
  variation-axis score, and variations stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the host shapes the non-BMP scalar in a way
that changes the `'B'` string index (unexpected; the cluster mapping is still
exercised by the monospace test).

The experiment **fails** if the cluster mapping, the `cell_offset` tracking, or
the `x_offset` computation diverges from upstream for monotonic runs, or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no Required
findings**. It confirmed the scoped monotonic-forward case is sound:

- The **unconditional reset matches upstream** at forward cluster boundaries. At
  cluster `0`, upstream skips the reset block (`cell_offset.cluster` is already
  `0`), but `cell_offset = {cluster: 0, x: 0}` is already that exact state, so
  the result is identical. For later forward clusters
  `cluster > run_offset.cluster`, so
  `is_after_glyph_from_current_or_next_clusters` is false and the cluster's
  first glyph has `is_first_codepoint_in_cluster == true` — upstream resets
  exactly as planned.
- Building `clusters` by pushing the scalar's cluster once per UTF-16 unit
  **faithfully mirrors upstream's surrogate padding** (both halves of a pair map
  to the same cluster — confirmed against `addCodepoint` lines 658–681).
- `x_offset = position.x − cell_x`, with `cell_x` captured at the cell start
  (not the running pen), is upstream's `position.x − cell_offset.x`.
- The surrogate-collapse test is a sound deterministic probe: `'B'` has CoreText
  string index `3` after `'A'` + the non-BMP scalar, and the UTF-16-indexed
  cluster table maps that to scalar cluster `2`.

Two **non-blocking notes**, both folded into the plan: find the `'B'` cell by
`glyph_index == face.glyph_index('B')` (already the test design), and add a
`debug_assert!(idx < clusters.len())` to make the index-validity invariant
visible without changing release behavior.

Review artifacts:

- Prompt: `logs/codex-review/20260603-131919-793042-prompt.md` (design)
- Result: `logs/codex-review/20260603-131919-793042-last-message.md` (design)

## Result

**Result:** Pass

The shaper now maps each glyph to its cell.

- `roastty/src/font/face/coretext.rs`: `shape_codepoints` builds `text` and a
  parallel `clusters: Vec<u32>` in lockstep — pushing each valid scalar's input
  index once per `ch.len_utf16()` (`1` BMP / `2` surrogate) — so `clusters` is
  indexed by the same UTF-16 offset CoreText reports and both halves of a
  surrogate pair share the scalar's cluster (mirroring upstream's `addCodepoint`
  padding `{codepoint: 0, cluster}`). It tracks line-wide
  `cell_cluster`/`cell_x` (upstream's `cell_offset`, init `{0, 0.0}`), and per
  glyph maps `cluster = clusters[idx]` (with
  `debug_assert!(idx < clusters.len())`), resets `cell_cluster`/`cell_x = pen`
  on a new cluster (unconditional — faithful for monotonic runs), and emits
  `Cell { x: cell_cluster, x_offset: round(position.x − cell_x), y_offset: round(position.y), glyph_index }`.
  The `shape_codepoints` doc comment and the `shape_ascii_monospace` assertion
  comment were updated (`x` is now the cluster, `== index` for ASCII).

Tests: `shape_clusters_monospace` (Menlo `"ABC"` → `x = 0, 1, 2`, all
`x_offset == 0`), `shape_cluster_collapses_surrogate` (`['A', 0x1D400, 'B']` →
the cell with `glyph_index == face.glyph_index('B')` has `x == 2`, the cluster,
**not** `3`, the raw UTF-16 index — confirming the surrogate pair collapses to
one cell). The surrogate-collapse test passes on the host.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2748 passed, 0 failed (+2, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

`Shaper.shape`'s cluster→cell positioning is ported for monotonic runs: glyphs
map back to their cluster (cell) through the UTF-16-indexed `clusters` table,
`Cell.x` is the cluster (collapsing surrogate pairs), and `x_offset` is measured
from the cell origin captured at each cell's start. The surrogate-collapse test
proves the cluster mapping is no longer the raw UTF-16 index.

The remaining shaper work: the **ligature/mark heuristic** (Exp 342 — the
_conditional_ `cell_offset` reset via `is_first_codepoint_in_cluster` and
`!is_after_glyph_from_current_or_next_clusters`, plus the `run_offset.cluster`
max-tracking it consumes); the **special-font** fast path (codepoint == glyph);
and the `Shaper` struct with its run state, caching, and the **`RunIterator`**
over terminal cells (which supplies the real grid clusters). The deferred
**variation-axis** `score()` refinement and **variations** application also
remain.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no Required findings**. It confirmed: `clusters` is built in lockstep with
`text` (one entry per UTF-16 unit) so CoreText string indices map back to the
intended cluster, and surrogate pairs sharing the scalar index mirror upstream's
dummy `{codepoint: 0, cluster}` padding; `cell_cluster = 0`/`cell_x = 0.0`
matches upstream's `cell_offset = .{}`, the first cluster `0` correctly does not
reset (already in the right state), and later monotonic forward clusters reset
to the current `pen` matching upstream; `x_offset = position.x − cell_x`
faithfully measures from the captured cell origin, not the running pen;
`Cell.x = cell_cluster as u16` matches upstream's
`@intCast(cell_offset.cluster)`; and the surrogate-collapse test (finding `'B'`
by glyph id) is the right assertion, avoiding run-order/fallback noise. The only
non-blocking caveat — that invalid `u32` values before valid scalars
intentionally leave gaps in cluster values while the UTF-16 lookup stays aligned
(skipped scalars add neither text nor cluster entries) — matches the documented
"input scalar index" stand-in, and the real path is fed valid codepoints from
the run iterator. The deferred scope (ligature heuristic, `run_offset.cluster`,
special-font, `Shaper`/`RunIterator`, variations) is intact.

Review artifacts:

- Result review: `logs/codex-review/20260603-132247-241631-last-message.md`
