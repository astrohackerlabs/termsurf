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

# Experiment 287: z2d port — the MSAA supersampled-span distributor

## Description

The multisample-4× rasterizer records each filled span (in `scale`-supersampled
x-coordinates) into the `SparseCoverageBuffer` by distributing it into
per-device- pixel coverage counts: a device pixel spans `scale` (= 4) horizontal
subpixels, so a fully-covered pixel gets coverage `scale` and a
partially-covered edge pixel gets the fraction. This is the `addSpan` helper in
`vendor/z2d/src/internal/raster/multisample.zig` (distinct from the buffer's own
`addSpan`). It is self-contained — it only needs the (already-ported)
`SparseCoverageBuffer` — and ships with an upstream test. Porting it now keeps
the full rasterizer `run` (next) a smaller step.

## Upstream behavior (`multisample.addSpan`)

`addSpan(cb, x, len)` (with `x`/`len` in supersampled coords, pre-clamped
non-negative):

- `panic` if `x + len > cb.capacity * scale`; return if `len == 0`.
- `start_x = x / scale`; `start_offset = x - start_x * scale` (the subpixel
  offset of the start within its device pixel).
- If `start_offset == 0 and len >= scale` (the span starts on a pixel boundary
  and is at least one pixel wide):
  - `front_len = len / scale` full pixels →
    `cb.addSpan(start_x, scale, front_len)`;
  - `end_coverage = min(scale, len - front_len * scale)`; if `> 0`,
    `cb.addSingle(start_x + front_len, end_coverage)` (the trailing partial
    pixel).
- Else (starts mid-pixel):
  - `start_coverage = min(scale, min(len, scale - start_offset))` →
    `cb.addSingle(start_x, start_coverage)` (the leading partial pixel);
  - `after_start = len - start_coverage`; `mid_len = after_start / scale` full
    pixels → if `> 0`, `cb.addSpan(start_x + 1, scale, mid_len)`;
  - `end_coverage = min(scale, after_start - mid_len * scale)`; if `> 0`,
    `cb.addSingle(start_x + 1 + mid_len, end_coverage)`.

## Rust mapping (`roastty/src/font/sprite/raster.rs`)

- `const MSAA_SCALE: u32 = 4` (z2d's multisample `scale`).
- `fn add_supersampled_span(cb: &mut SparseCoverageBuffer, x: u32, len: u32)` —
  the faithful port (upstream's module-private `addSpan`), using
  `SparseCoverageBuffer::add_span`/`add_single` (Experiment 286). The capacity
  guard becomes an `assert!`.

## Scope / faithfulness notes

- **Deferred**: the multisample rasterizer `run` (the scanline loop driving the
  `WorkingEdgeSet` into this distributor and compositing into the surface), the
  fill/stroke plotters, and `Canvas::line`/`fill`/`stroke` — later z2d slices.
- `u8`/`u32` arithmetic mirrors z2d; the caller supplies pre-clamped coordinates
  per upstream's contract.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/sprite/raster.rs`: add `MSAA_SCALE` and
   `add_supersampled_span`.
2. Tests (deterministic, scale 4):
   - `supersample_span_triangle` (the full upstream `addSpan` test): on a
     `1024`-capacity buffer, the four accumulated spans of a triangle
     cross-section — `(200,400)` → `get(50)=(4,100)`; then `(201,398)` →
     `get(50)=(7,1)`, `get(51)=(8,98)`, `get(149)=(7,1)`; then `(202,396)` →
     `get(50)=(9,1)`, `get(51)=(12,98)`, `get(149)=(9,1)`; then `(203,394)` →
     `get(50)=(10,1)`, `get(51)=(16,98)`, `get(149)=(10,1)`; and walking the
     runs yields exactly 4 spans.
   - `supersample_span_partial_start`: `add_supersampled_span(2, 4)` →
     `get(0) == (2, 1)`, `get(1) == (2, 1)` (`x=2..6` spans the second half of
     pixel 0 and the first half of pixel 1, each `2/4` coverage).
   - `supersample_span_full_plus_partial`: `add_supersampled_span(0, 6)` →
     `get(0) == (4, 1)`, `get(1) == (2, 1)` (pixel 0 full, pixel 1 half).
   - `supersample_span_zero`: `add_supersampled_span(0, 0)` is a no-op.
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

- `add_supersampled_span` reproduces z2d's `addSpan` boundary/partial-pixel
  coverage distribution, verified by the upstream test and the partial cases;
- the rasterizer `run`, plotters, and `Canvas` path methods stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the distribution needs a different integer
shape to match upstream exactly.

The experiment **fails** if the coverage distribution diverges from z2d or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and raised one **Required**
finding: `supersample_span_full` claimed to be the upstream `addSpan` test but
ported only its first assertion. Fixed by porting the **complete** upstream test
as `supersample_span_triangle` — the four accumulated spans of a triangle
cross-section (`(200,400)`, `(201,398)`, `(202,396)`, `(203,394)`) with the
edge/full/edge coverages at each step (`get(50)`/`get(51)`/`get(149)`) and the
4-run total. Codex confirmed the helper design itself is faithful: the boundary
case, the mid-pixel case, the `len == 0` short-circuit, the capacity `assert!`,
and the listed partial cases all compute as stated.

Review artifacts:

- Prompt: `logs/codex-review/20260603-060336-602790-prompt.md`
- Result: `logs/codex-review/20260603-060336-602790-last-message.md`
