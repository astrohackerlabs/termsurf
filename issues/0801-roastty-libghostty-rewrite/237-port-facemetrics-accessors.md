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

# Experiment 237: Port the Remaining `FaceMetrics` Accessors

## Description

Complete the `FaceMetrics` convenience accessors. Experiment 236 ported
`line_height`, `effective_cap_height`, and `effective_ex_height`; this slice
ports the remaining six methods from upstream `font/Metrics.zig` that the
(deferred) `calc` consumes for underline/strikethrough/ascii/ideograph
derivation.

The methods fall into **two groups with different fallback rules**:

- **Sizes** (`>0` guard — use the stored value only when present and positive):
  `asciiHeight`, `icWidth`, `underlineThickness`, `strikethroughThickness`.
- **Positions** (plain `orelse` — **no** sign guard, because positions are valid
  whether positive or negative; upstream notes this explicitly):
  `underlinePosition`, `strikethroughPosition`.

### Methods to port (upstream)

```
pub fn asciiHeight(self) f64 {
    if (self.ascii_height) |v| if (v > 0) return v;
    return 1.5 * self.capHeight();
}
pub fn icWidth(self) f64 {
    if (self.ic_width) |v| if (v > 0) return v;
    return @min(self.asciiHeight(), 2 * self.cell_width);
}
pub fn underlineThickness(self) f64 {
    if (self.underline_thickness) |v| if (v > 0) return v;
    return 0.15 * self.exHeight();
}
pub fn strikethroughThickness(self) f64 {
    if (self.strikethrough_thickness) |v| if (v > 0) return v;
    return self.underlineThickness();
}
// positions: no >0 guard
pub fn underlinePosition(self) f64 {
    return self.underline_position orelse -self.underlineThickness();
}
pub fn strikethroughPosition(self) f64 {
    return self.strikethrough_position orelse (self.exHeight() + self.strikethroughThickness()) * 0.5;
}
```

### Rust mapping (continuing the `effective_*` naming from Exp 236)

- `effective_ascii_height()` → `ascii_height` if `Some` and `> 0`, else
  `1.5 * effective_cap_height()`.
- `effective_ic_width()` → `ic_width` if `Some` and `> 0`, else
  `effective_ascii_height().min(2.0 * cell_width)` (Zig `@min`).
- `effective_underline_thickness()` → `underline_thickness` if `Some` and `> 0`,
  else `0.15 * effective_ex_height()`.
- `effective_strikethrough_thickness()` → `strikethrough_thickness` if `Some`
  and `> 0`, else `effective_underline_thickness()`.
- `effective_underline_position()` →
  `underline_position.unwrap_or(-effective_underline_thickness())` — **no `> 0`
  guard**: a stored value is used even if negative.
- `effective_strikethrough_position()` →
  `strikethrough_position.unwrap_or((effective_ex_height() + effective_strikethrough_thickness()) * 0.5)`
  — **no `> 0` guard**.

### Faithfulness and scope notes

- The size methods keep the `Some(value)` + `value > 0.0` guard exactly; the
  position methods use `unwrap_or` with **no** guard (a negative stored position
  is honored), matching upstream's explicit "positions, not sizes" note.
- All `pub(crate)`, added to `impl FaceMetrics` in
  `roastty/src/font/metrics.rs`.
- No `calc`/`Minimums`/constraint behavior — only these accessors.
- No C ABI, header, or ABI inventory changes; no new dependencies.

## Changes

1. `roastty/src/font/metrics.rs`: add the six `pub(crate)` methods to
   `impl FaceMetrics` with upstream doc comments and the exact fallback formulas
   above.

2. Tests in `roastty/src/font/metrics.rs` (the `face_sample()` helper exists; it
   has `ascent 12 → cap 9 → ex 6.75`, `cell_width 8`):
   - `effective_ascii_height`: `Some(20.0)` → `20.0`;
     `None`/`Some(0.0)`/negative → `1.5 * 9.0 = 13.5`.
   - `effective_ic_width`: `Some(10.0)` → `10.0`; `ic_width = Some(0.0)` and a
     negative value fall through to the min (proving the same `> 0` size guard);
     with `ic_width = None` and `ascii_height = None` → `min(13.5, 2*8) = 13.5`;
     and a case where `2*cell_width` is the smaller (e.g. `cell_width = 5` →
     `min(13.5, 10) = 10`).
   - `effective_underline_thickness`: `Some(2.0)` → `2.0`; `None`/non-positive →
     `0.15 * 6.75 = 1.0125`.
   - `effective_strikethrough_thickness`: `Some(3.0)` → `3.0`;
     `None`/non-positive → equals `effective_underline_thickness()`.
   - `effective_underline_position`: `Some(-2.0)` → `-2.0` (negative honored, no
     guard); `None` → `-effective_underline_thickness()`.
   - `effective_strikethrough_position`: `Some(-1.5)` → `-1.5`; `None` →
     `(ex + strike_thickness) * 0.5`.

3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo test -p roastty font
cargo test -p roastty
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- all six accessors reproduce upstream exactly, including the `>0` guard on the
  four size methods and **no** guard on the two position methods;
- the listed tests pass (notably the `ic_width` min-of-two-cells branch and the
  negative-position-honored cases);
- no `calc`/`Minimums`/constraint scope is pulled in;
- no C ABI, header, or ABI inventory changes;
- `cargo fmt` accepted and `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if an accessor turns out to need a different
signature once `calc` consumes it.

The experiment **fails** if a formula or guard diverges from upstream (e.g.
applying a `>0` guard to a position, or the wrong `ic_width` min), if `calc`
behavior leaks in, or if any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation.

Review artifacts:

- Prompt: `logs/codex-review/20260602-082830-954336-prompt.md`
- Result: `logs/codex-review/20260602-082830-954336-last-message.md`

Codex confirmed the formulas and the size-vs-position guard split are correct
(the four size methods use the `> 0` guard; `underlinePosition`/
`strikethroughPosition` use plain fallback with no sign validation, matching the
upstream "positions, not sizes" note), and that `@min`→`.min()` and
`2 * cell_width`→`2.0 * cell_width` are correct.

One Low finding, fixed in the design above before this commit:

1. the `effective_ic_width` tests did not prove the `> 0` guard for a
   non-positive stored value — added `ic_width = Some(0.0)` and a negative case
   that fall through to `min(effective_ascii_height(), 2.0 * cell_width)`.
