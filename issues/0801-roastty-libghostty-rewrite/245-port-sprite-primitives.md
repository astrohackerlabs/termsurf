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

# Experiment 245: Port the sprite canvas geometric primitives and `Color`

## Description

Begin the sprite font subsystem (`font/sprite/`), which procedurally draws
box-drawing, block, Powerline, Braille, and legacy-computing glyphs straight
into the atlas. This first slice ports the small, dependency-free **geometric
primitives** that are the vocabulary of all sprite drawing — `Point`, `Line`,
`Box` (with its `rect` normalization), `Rect`, `Triangle`, `Quad` — and the
`Color` alpha type, all from the top of upstream `font/sprite/canvas.zig`.

The `Canvas` itself (and the rest of `canvas.zig`) is **deferred**: it wraps a
`z2d` surface (a Zig 2D vector-graphics library), so it needs a separate
experiment to choose and wire a Rust rasterization backend. The primitives are
entirely independent of `z2d` (they are plain coordinate structs), so they port
cleanly now and unblock the later `Canvas` and `draw/` work. This is a small
foundation slice — pure value types with one piece of logic (`Box::rect`) —
sized deliberately small because it opens a new subsystem.

### Upstream primitives (`canvas.zig` lines 9–75)

```zig
pub fn Point(comptime T: type) type { return struct { x: T, y: T }; }
pub fn Line(comptime T: type) type { return struct { p0: Point(T), p1: Point(T) }; }
pub fn Box(comptime T: type) type {
    return struct {
        p0: Point(T), p1: Point(T),
        pub fn rect(self: Box(T)) Rect(T) {
            const tl_x = @min(self.p0.x, self.p1.x);
            const tl_y = @min(self.p0.y, self.p1.y);
            const br_x = @max(self.p0.x, self.p1.x);
            const br_y = @max(self.p0.y, self.p1.y);
            return .{ .x = tl_x, .y = tl_y, .width = br_x - tl_x, .height = br_y - tl_y };
        }
    };
}
pub fn Rect(comptime T: type) type { return struct { x: T, y: T, width: T, height: T }; }
pub fn Triangle(comptime T: type) type { return struct { p0: Point(T), p1: Point(T), p2: Point(T) }; }
pub fn Quad(comptime T: type) type { return struct { p0: Point(T), p1: Point(T), p2: Point(T), p3: Point(T) }; }

/// We only use alpha-channel so a pixel can only be "on" or "off".
pub const Color = enum(u8) { on = 255, off = 0, _ };
```

`Box.rect` normalizes a box given by any two opposite corners into a top-left
`Rect` with non-negative `width`/`height`. The primitives are instantiated at
`i32`, `u32`, `f64`, and `usize` across the sprite code.

`Color` is an `enum(u8)` with the `_` non-exhaustive tag: it has named endpoints
`on` (`255`) and `off` (`0`) but also carries **arbitrary intermediate alpha**,
used via `@intFromEnum(color)` (the alpha byte written to the canvas) and
`@enumFromInt(byte)` (e.g. a rounded float shade, or `@intFromEnum(shade)` from
the block/shade tables). So `Color` is semantically a `u8` alpha with two named
values, not a closed enum.

### Rust mapping

New `roastty/src/font/sprite/` module (`pub(crate) mod sprite;` in
`font/mod.rs`, a `sprite/mod.rs` that declares `pub(crate) mod canvas;`, and the
primitives in `sprite/canvas.rs` — mirroring upstream's `canvas.zig`; the
`Canvas` struct lands later in the same file).

- Generic value structs, each `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`
  (the derived `Eq` bound simply does not apply to the `f64` instantiation,
  which still gets `PartialEq`):
  - `pub(crate) struct Point<T> { pub x: T, pub y: T }`
  - `pub(crate) struct Line<T> { pub p0: Point<T>, pub p1: Point<T> }`
  - `pub(crate) struct Box<T> { pub p0: Point<T>, pub p1: Point<T> }`
  - `pub(crate) struct Rect<T> { pub x: T, pub y: T, pub width: T, pub height: T }`
  - `pub(crate) struct Triangle<T> { pub p0: Point<T>, pub p1: Point<T>, pub p2: Point<T> }`
  - `pub(crate) struct Quad<T> { pub p0: Point<T>, pub p1: Point<T>, pub p2: Point<T>, pub p3: Point<T> }`
- `impl<T: PartialOrd + Sub<Output = T> + Copy> Box<T> { pub(crate) fn rect(self) -> Rect<T> }`:
  the min/max normalization. Use `PartialOrd`-based manual min/max
  (`if a < b { a } else { b }`) rather than `Ord`/`std::cmp::min` so the same
  impl covers the `f64` instantiation (Rust's `Ord` is not implemented for
  floats); this matches upstream `@min`/`@max` for all non-NaN inputs the sprite
  code produces.
- `Color` as a newtype over the alpha byte (not a Rust `enum`, since it carries
  arbitrary `u8`):
  - `pub(crate) struct Color(pub u8)` (`Debug, Clone, Copy, PartialEq, Eq`),
  - the named endpoints as **associated** constants (in `impl Color`), so they
    read like upstream's `Color.on`/`Color.off` and `Color::ON.0` resolves:
    `pub(crate) const ON: Color = Color(255);` and
    `pub(crate) const OFF: Color = Color(0);`,
  - the byte is read as `color.0` (the analog of `@intFromEnum`), and any alpha
    is built as `Color(byte)` (the analog of `@enumFromInt`). A short doc
    comment records this `enum(u8)`-with-`_` → newtype rationale.

### Faithfulness and scope notes

- Only the primitives and `Color` are ported; `Canvas` and everything that
  touches `z2d` are deferred to the experiment that selects a Rust 2D backend,
  and the `draw/` glyph tables come after that.
- `Box::rect` uses `PartialOrd` min/max instead of `Ord` purely so one generic
  impl serves both integer and float `T`; the result is identical to upstream
  for the (non-NaN) coordinates the sprite code uses.
- Upstream has no unit tests for these primitives, so this slice adds Rust-side
  equivalent tests (per the issue's Test Parity rule) — chiefly for the one
  piece of logic, `Box::rect`.
- No C ABI, header, or ABI inventory changes; no new dependencies.

## Changes

1. `roastty/src/font/mod.rs`: add `pub(crate) mod sprite;` and a one-line note
   in the module doc.

2. `roastty/src/font/sprite/mod.rs` (new): module doc +
   `pub(crate) mod canvas;`.

3. `roastty/src/font/sprite/canvas.rs` (new): `Point`, `Line`, `Box` (+`rect`),
   `Rect`, `Triangle`, `Quad`, and `Color` (+`ON`/`OFF`).

4. Tests in `roastty/src/font/sprite/canvas.rs`:
   - `box_rect_normalizes`:
     `Box { p0: Point { x: 3, y: 5 }, p1: Point { x: 1, y: 9 } }.rect()` (`i32`,
     corners given out of order) `== Rect { x: 1, y: 5, width: 2, height: 4 }`.
   - `box_rect_already_ordered`: a `u32` box with `p0` already top-left yields
     the same-origin `Rect` with the expected `width`/`height`.
   - `box_rect_float`: an `f64` box returns the expected `Rect<f64>` (exercises
     the `PartialOrd` min/max on the float instantiation).
   - `color_alpha`: `Color::ON.0 == 255`, `Color::OFF.0 == 0`,
     `Color(128).0 == 128`.
   - `primitive_construction`: build a `Line`/`Triangle`/`Quad` and read a
     couple of fields back (a smoke test that the generic field layout is as
     expected).

5. Format and test (`cargo fmt`, accept output).

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

- `Point`/`Line`/`Box`/`Rect`/`Triangle`/`Quad` match upstream's fields and are
  generic over `T`;
- `Box::rect` reproduces the min/max normalization for integer and float `T`;
- `Color` is a `u8`-alpha newtype with `ON`/`OFF` endpoints and round-trips an
  arbitrary alpha byte;
- the five Rust-side tests pass;
- `Canvas`/`z2d` and the `draw/` tables are cleanly deferred;
- no C ABI, header, or ABI inventory changes;
- `cargo fmt` accepted and `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the deferred `Canvas` forces a representation
change on a primitive (e.g. a coordinate type or a `Color` conversion helper).

The experiment **fails** if `Box::rect` mis-normalizes, if `Color` cannot
represent intermediate alpha, or if any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation.

Review artifacts:

- Prompt: `logs/codex-review/20260602-093019-883495-prompt.md`
- Result: `logs/codex-review/20260602-093019-883495-last-message.md`

Codex confirmed the primitive field names/order match upstream, that `Box::rect`
normalization is correct, that `PartialOrd + Sub<Output = T> + Copy` is the
right bound for integer plus non-NaN float coordinates, that deriving `Eq` on
the generic structs is fine (`Point<f64>` simply will not implement `Eq`), and
that the `Color` newtype is the right representation for upstream's
`enum(u8) { on, off, _ }` because downstream needs arbitrary alpha bytes.

One finding, fixed in the design above before this commit:

1. **Medium — `Color::ON`/`OFF` must be associated constants.** The mapping
   listed `pub(crate) const ON: Color = …` which reads as module-level
   constants, but the tests (and upstream's `Color.on`/`Color.off`) need
   `Color::ON`. Fixed to specify them inside `impl Color`.

## Result

**Result:** Pass

Added the new `roastty/src/font/sprite/` module (`pub(crate) mod sprite;` in
`font/mod.rs`, `sprite/mod.rs` declaring `pub(crate) mod canvas;`) and
`sprite/canvas.rs` with the generic value structs `Point`, `Line`, `Box`,
`Rect`, `Triangle`, `Quad` (all `Debug, Clone, Copy, PartialEq, Eq`),
`Box::rect` (the `PartialOrd`-based min/max normalization, one impl covering
integer and float `T`), and `Color` (a `Color(pub u8)` newtype with associated
`ON`/`OFF` constants), with a doc comment recording the `enum(u8)`-with-`_` →
newtype rationale.

Tests added (5): `box_rect_normalizes` (`i32`, swapped corners →
`Rect { 1, 5, 2, 4 }`), `box_rect_already_ordered` (`u32`), `box_rect_float`
(`f64`, exercising the `PartialOrd` min/max), `color_alpha`
(`ON`/`OFF`/arbitrary byte), and `primitive_construction`.

### Verification

```bash
cargo fmt -p roastty
cargo test -p roastty canvas
cargo test -p roastty
```

Observed:

- `canvas`: 5 passed.
- Full `roastty`: 2333 unit tests passed (2328 prior + 5 new), plus the C ABI
  harness passed.
- `cargo fmt -p roastty -- --check`: clean.
- `cargo build -p roastty`: no warnings.
- No-`ghostty`-name gates passed for `roastty/src/font` and for
  `roastty/src/lib.rs`, `roastty/include/roastty.h`,
  `roastty/tests/abi_harness.c`.
- `git diff --check`: clean.

No C ABI, header, or ABI inventory changes; `Canvas`/`z2d` and the `draw/`
tables cleanly deferred.

### Completion Review

Codex reviewed the completed implementation and found **no issues** ("nothing
needs to change before the result commit").

Review artifacts:

- Prompt: `logs/codex-review/20260602-093312-491226-prompt.md`
- Result: `logs/codex-review/20260602-093312-491226-last-message.md`

Codex confirmed the primitive field names/order match upstream, that `Box::rect`
uses the intended `PartialOrd + Sub<Output = T> + Copy` impl and normalizes
correctly, that `Color` is `Color(pub u8)` with associated `ON`/`OFF` (the
design-gate fix implemented), that the module wiring is correct with
`Canvas`/`draw` deferred, and that the five tests assert the expected values.

## Conclusion

Experiment 245 succeeds, opening the `font/sprite/` subsystem with its
geometric-primitive vocabulary (`Point`/`Line`/`Box`/`Rect`/`Triangle`/`Quad`)
and the `Color` alpha newtype. Both Codex gates passed (one design finding fixed
— the associated `Color::ON`/`OFF` constants; zero result findings).

The next sprite slice is the `Canvas` itself, which requires choosing and wiring
a Rust 2D rasterization backend to replace upstream's `z2d` (the candidate is a
small alpha-only software rasterizer, since the sprite canvas only ever writes
the alpha channel — `tiny-skia` or a hand-rolled scanline filler over the
already-ported primitives). That backend decision deserves its own foundation
experiment before the `draw/` glyph tables (box-drawing, block, Braille,
Powerline, legacy-computing) can be ported on top. Alternatively, the font layer
can advance the CoreText face/rasterization path in parallel; the next
experiment will pick whichever is the smaller, better-tested next step.
