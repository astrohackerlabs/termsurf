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

# Experiment 286: z2d port — the SparseCoverageBuffer

# Description

The next z2d slice: the **`SparseCoverageBuffer`**
(`vendor/z2d/src/internal/raster/sparse_coverage.zig`, derived from tiny-skia's
`alpha_runs`) — a run-length-encoded coverage accumulator for a single scanline.
The multisample rasterizer records per-sub-scanline coverage spans into it, then
reads the accumulated coverage back run by run. It is self-contained (no polygon
dependency) and ships with a thorough upstream test suite that ports directly.

## Upstream behavior

- RLE storage: `values: []u8` (the coverage at each run start) and `lengths`
  (the run length at each run start), plus `len` (the covered extent) and
  `capacity`. Only run-start indices hold meaningful `values[x]`/`lengths[x]`;
  the caller walks runs by reading `get(x)` and advancing by the returned
  length. The `lengths` storage uses a `u8`/`u16`/`u32` union chosen by capacity
  — purely a memory optimization, behaviorally identical to `u32`.
- `init(capacity)`: zeroed `values`/`lengths`, `len = 0`.
- `reset()`: `len = 0`, `lengths[0] = 0`.
- `get(x)` → `(values[x], lengths[x])`; `put(x, value, len)` writes a run start
  (`assert x + len <= capacity`); `putValue(x, value)` writes just the value.
- `extend(x, len)`: ensures runs exist so `[x, x+len)` can be addressed,
  splitting existing runs at `x` and `x+len` as needed (and appending zero runs
  past `len`). Three cases: `x == len` (append one run), `x > len` (append a gap
  run then the span), else split from the front (`splitInner(0, x)`), extend
  past the end if `x+len > len`, then `splitInner(x, len)`.
- `splitInner(x, len)`: walk runs from `x`; when the remaining `len` falls
  inside a run, split it into `(value, rem)` and `(value, current_len - rem)`.
- `addSpan(x, value, len)`: `extend(x, len)`, then add `value` to every run's
  coverage across `[x, x+len)`. `addSingle(x, value)`: `extend(x, 1)` then add
  to the single run.

## Rust mapping (`roastty/src/font/sprite/raster.rs`)

- `struct SparseCoverageBuffer { values: Vec<u8>, lengths: Vec<u32>, len: u32, capacity: u32 }`
  — `lengths` is `u32` (the union is a memory-only optimization; `u32` is the
  faithful behavioral equivalent).
- `fn new(capacity: u32) -> SparseCoverageBuffer` (`init`);
  `fn reset(&mut self)`.
- `fn get(&self, x: u32) -> (u8, u32)`; `fn put(&mut self, x, value, len)`
  (`assert!(x + len <= capacity)`); `fn put_value(&mut self, x, value)`.
- `fn extend(&mut self, x: u32, len: u32)` and
  `fn split_inner(&mut self, x, len)` — the faithful span-splitting ports.
- `fn add_span(&mut self, x: u32, value: u8, len: u32)` and
  `fn add_single(&mut self, x: u32, value: u8)`.

## Scope / faithfulness notes

- **Deferred**: the multisample rasterizer `run`, the fill/stroke plotters, and
  `Canvas::line`/`fill`/`stroke` — later z2d slices.
- The `LengthStorage` union is rendered as a plain `Vec<u32>`; upstream
  documents it as a memory optimization only, so this is behaviorally identical.
- The `u8` coverage accumulation wraps the same way (`value` adds are bounded by
  the caller, per upstream's contract); we use `u8` add (matching `+=` on `u8`).
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/raster.rs`: add `SparseCoverageBuffer` (+ `new`/
   `reset`/`get`/`put`/`put_value`/`extend`/`split_inner`/`add_span`/
   `add_single`).
2. Tests — the upstream `extend` suite ported directly (each builds runs with
   `put`, sets `len`, calls `extend`, and checks `len` + `get` at the run
   boundaries):
   - `extend_basic`: runs `(0,0,4)`,`(4,0,4)`,`len=8`; `extend(2,5)` → `len=8`,
     `get(0)=(0,2)`, `get(2)=(0,2)`, `get(4)=(0,3)`, `get(7)=(0,1)`.
   - `extend_new_zero` / `extend_new_nonzero`: `extend(0,5)` → `(0,5)`;
     `extend(2,5)` → `get(0)=(0,2)`, `get(2)=(0,5)`, `len=7`.
   - `extend_split_end_no_extend` / `extend_split_end_with_extend`:
     `extend(7,1)` and `extend(7,3)` over `(0,0,4),(4,0,4),len=8`.
   - `extend_append_after_end`: `extend(8,2)`.
   - `extend_past_end`: cap 11, `(0,0,4),(4,0,4),len=8`; `extend(9,2)` →
     `len=11`, `get(0)=(0,4)`, `get(4)=(0,4)`, `get(8)=(0,1)`, `get(9)=(0,2)`.
   - `extend_zero_len`: `extend(0,0)` → `get(0)` length is `0`.
   - `extend_split_to_capacity`: cap 255, `extend(192,63)` → `get(0)=(0,192)`,
     `get(192)=(0,63)`, and walking the runs yields exactly 2 spans.
   - Plus `add_span_accumulates`: two overlapping `add_span` calls accumulate
     the coverage value per run (e.g. `add_span(0,1,5)` then `add_span(2,1,5)`
     gives coverage `1` on `[0,2)`, `2` on `[2,5)`, `1` on `[5,7)`), and
     `add_single_accumulates`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty raster
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `SparseCoverageBuffer` reproduces z2d's RLE `extend`/`split_inner` span logic
  and the `add_span`/`add_single` accumulation, verified against the ported
  upstream tests;
- the rasterizer, plotters, and `Canvas` path methods stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the RLE representation needs a different shape
to serve the (next) rasterizer faithfully.

The experiment **fails** if the coverage-buffer behavior diverges from z2d or
any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and raised one **Required**
finding: the test plan claimed the upstream `extend` suite was ported directly
but omitted three real upstream cases — `put something past end of buffer`,
`zero len`, and `split up to exactly capacity`. Fixed by adding
`extend_past_end` (cap 11, `extend(9,2)`), `extend_zero_len`, and
`extend_split_to_capacity` (cap 255, `extend(192,63)`, 2 spans) as direct
transcriptions. Codex confirmed everything else is faithful: `Vec<u32>` for
lengths is behaviorally equivalent, and `extend`/`split_inner`,
`add_span`/`add_single`, `get`/`put`/`put_value`/ `reset`, and the
caller-bounded `u8` accumulation match upstream.

Review artifacts:

- Prompt: `logs/codex-review/20260603-055805-453461-prompt.md`
- Result: `logs/codex-review/20260603-055805-453461-last-message.md`

## Result

**Result:** Pass

`roastty/src/font/sprite/raster.rs` gained `SparseCoverageBuffer`
(`values: Vec<u8>`, `lengths: Vec<u32>`, `len`, `capacity`) with
`new`/`reset`/`covered_len`/`get`/`put`/`put_value`/`extend`/`split_inner`/
`add_span`/`add_single` — the faithful RLE span-splitting and accumulation
ports.

Tests (the upstream `extend` suite transcribed directly, plus accumulation):

- `extend_basic`, `extend_new_zero`, `extend_new_nonzero`,
  `extend_split_end_no_extend`, `extend_split_end_with_extend`,
  `extend_append_after_end`, `extend_past_end` (cap 11), `extend_zero_len`,
  `extend_split_to_capacity` (cap 255, 2 spans) — each matching the upstream
  `get`/`len` expectations.
- `add_span_accumulates` — `add_span(0,1,5)` then `add_span(2,1,5)` gives
  coverage `1`/`2`/`1` on `[0,2)`/`[2,5)`/`[5,7)`.
- `add_single_accumulates` — `add_single` bumps a single run's coverage.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty raster` → 28 passed (11 new).
- `cargo test -p roastty` → 2529 passed, 0 failed (no regressions; +11).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The RLE coverage accumulator is in place, verified against z2d's own `extend`
suite. The z2d port now has all three rasterizer ingredients: the `Polygon`
tessellation, the `WorkingEdgeSet` active-edge-table, and the
`SparseCoverageBuffer`. The next slice is the **multisample-4× rasterizer
`run`** — it drives the `WorkingEdgeSet` over each pixel row's four
sub-scanlines, records the filtered spans into the `SparseCoverageBuffer`
(coverage `1` per sample), then writes each run's accumulated coverage as
`alpha = coverage * (256 / 16)` (clamped) source-over into the alpha8 `Canvas`
buffer. After that come the `fill_plotter` (path → `Polygon`) and
`stroke_plotter` (stroke outline → `Polygon`, butt caps), then wiring
`Canvas::line`/`fill`/`stroke` — which unblocks the box-drawing diagonals, arcs,
circle/ellipse pieces, and geometric curves. Alongside the sprite font remain
the discovery consumer, the UCD emoji-presentation default, codepoint overrides,
the shaper, the Nerd Font attribute table, and SVG color detection.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**. It confirmed `new`/`reset`/`get`/`put`/`put_value`, `extend`,
`split_inner`, `add_span`, and `add_single` match `sparse_coverage.zig`, that
`Vec<u32>` is a behavioral equivalent for the length storage, and that the
ported upstream `extend` tests plus the accumulation tests have the correct
expectations. It judged the gates clean.

Review artifacts:

- Prompt: `logs/codex-review/20260603-060122-668447-prompt.md`
- Result: `logs/codex-review/20260603-060122-668447-last-message.md`
