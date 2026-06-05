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

# Experiment 573: split tree f16 structs (Split, Slot) via half::f16

## Description

This experiment **unblocks the `f16` work** by introducing the `half` crate
(Rust's standard IEEE-754 half-precision float, bit-identical to Zig's `f16`) as
a roastty dependency, and uses it to port the two `f16`-carrying leaf structs of
upstream `datastruct/split_tree.zig`: the `Split` node payload (`layout`,
`ratio: f16`, `left`/`right` handles) and the `Spatial.Slot` (`x`, `y`, `width`,
`height: f16`, with the `maxX` / `maxY` helpers). It extends
`terminal::split_tree` (Experiment 572 landed the `f16`-free `Handle` / `Layout`
/ `Direction` vocabulary).

`half` `2.7.1` is already present in the workspace `Cargo.lock` (a transitive
dependency), so adding it as a direct dependency of roastty resolves to the same
version with no new version selection.

## Upstream behavior

From `datastruct/split_tree.zig`:

- `Split` — the payload of a split node:
  ```zig
  pub const Split = struct {
      layout: Layout,
      ratio: f16,         // the fraction of space given to the left/top child
      left: Node.Handle,
      right: Node.Handle,
  };
  ```
- `Spatial.Slot` — a node's normalized 2D rectangle (all `f16`, in a 1×1 space):
  ```zig
  const Slot = struct {
      x: f16, y: f16, width: f16, height: f16,
      fn maxX(self: *const Slot) f16 { return self.x + self.width; }
      fn maxY(self: *const Slot) f16 { return self.y + self.height; }
  };
  ```

Zig's `f16` and `half::f16` are both IEEE-754 binary16, so values, rounding, and
arithmetic match bit-for-bit.

## Rust mapping (`roastty/src/terminal/split_tree.rs`)

```rust
use half::f16;

/// The payload of a split node (upstream `Split`): two child handles, the split orientation, and
/// the fraction of space given to the first (left / top) child.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Split {
    pub(crate) layout: Layout,
    pub(crate) ratio: f16,
    pub(crate) left: Handle,
    pub(crate) right: Handle,
}

/// A node's normalized 2D rectangle in the spatial representation (upstream `Spatial.Slot`); all
/// coordinates are in a 1×1 space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Slot {
    pub(crate) x: f16,
    pub(crate) y: f16,
    pub(crate) width: f16,
    pub(crate) height: f16,
}

impl Slot {
    /// The right edge, `x + width` (upstream `maxX`).
    pub(crate) fn max_x(self) -> f16 {
        self.x + self.width
    }

    /// The bottom edge, `y + height` (upstream `maxY`).
    pub(crate) fn max_y(self) -> f16 {
        self.y + self.height
    }
}
```

`half::f16` implements `Add`, so `self.x + self.width` is the faithful binary16
addition. The structs derive `PartialEq` but **not** `Eq` (`f16` is a float — no
total equality), and not the `Hash`/`Ord` that `f16` lacks.

## Scope / faithfulness notes

- **Dependency introduced**: `half = "2"` added to roastty's `[dependencies]`
  (already in the workspace lock at `2.7.1`). This is the shared half-precision
  float type that unblocks both split_tree's `f16` logic and (later)
  `background-image-opacity`.
- **Ported**: `split_tree`'s `Split` struct and `Spatial.Slot` (as
  `terminal::split_tree::{Split, Slot}`), with `Slot::max_x` / `Slot::max_y`.
- **Faithful**: `Split`'s four fields (`layout`, `ratio`, `left`, `right`) and
  `Slot`'s four `f16` coordinates are reproduced exactly; `max_x` / `max_y` are
  the same `x + width` / `y + height` binary16 additions; `half::f16` matches
  Zig's `f16` bit-for-bit (both IEEE-754 binary16).
- **Faithful adaptation**: `Spatial.Slot` is nested under `Spatial` upstream;
  here `Slot` sits at the `split_tree` module level alongside the `Spatial`
  container that will land with the deferred spatial logic. `Split.ratio` is
  `f16` (not narrowed to `f32`), preserving exact half-precision storage and
  round-tripping.
- **Deferred** (unchanged from Experiment 572): the immutable tree itself (the
  `Node` arena, view ref-counting, `init` / `clone` / `split` / `remove` /
  `goto` / `zoom` / `equalize` / `resize` / formatters) and the `Spatial`
  container + its normalization and `Spatial.Direction` navigation. Those build
  on these structs.
- No C ABI/header/ABI-inventory change (internal Rust). Extends
  `terminal::split_tree`.

## Changes

1. `roastty/Cargo.toml`: add `half = "2"` to `[dependencies]`.
2. `roastty/src/terminal/split_tree.rs`: add `use half::f16;`, the `Split`
   struct, and the `Slot` struct with `max_x` / `max_y`.
3. Tests (in `split_tree.rs`):
   - **Split fields / equality**: construct a `Split` and read its fields; two
     identical `Split`s are `==`; a different `ratio` makes them unequal.
   - **ratio round-trips**: `f16::from_f32(0.5)` stored in `ratio` reads back as
     `0.5` via `to_f32()`.
   - **Slot max_x / max_y**: using binary-exact half values (no decimal-rounding
     ambiguity) — `x=0.25, width=0.5` ⇒ `max_x == 0.75`; `y=0.125, height=0.25`
     ⇒ `max_y == 0.375`. The `max_y` case is also asserted against the explicit
     half operation (`f16::from_f32(0.125) + f16::from_f32(0.25)`) to compare
     like-for-like.
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty terminal::split_tree
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/terminal/split_tree.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `half = "2"` is added and resolves to the workspace-locked version, and
  `Split` / `Slot` reproduce upstream's fields and the `max_x` / `max_y`
  (`x + width` / `y + height`) helpers with faithful `f16` semantics — faithful
  to `datastruct/split_tree.zig`;
- the tests pass (Split fields/equality / ratio round-trip / Slot max_x-max_y),
  and the existing tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the struct fields, the `max_x` / `max_y` arithmetic,
or the `f16` semantics diverge from upstream, an unrelated item changes, or any
public C API/ABI changes.

## Design Review

Codex reviewed the design and found **no Required findings**, with one Optional
— adopted:

- **Optional (adopted)**: avoid asserting
  `f16::from_f32(0.1) + f16::from_f32(0.2) == f16::from_f32(0.3)` for `max_y` —
  with binary16, decimal inputs can land on adjacent representable values.
  Switched the `Slot` tests to binary-exact half fractions
  (`0.25 + 0.5 == 0.75`, `0.125 + 0.25 == 0.375`) and also assert `max_y()`
  against the explicit half operation
  `f16::from_f32(0.125) + f16::from_f32(0.25)`.

Codex confirmed that adding `half = "2"` is the right unblock for Zig `f16`
storage and finite split-tree arithmetic, that the `Split` and `Slot` fields are
faithful, that deriving `PartialEq` but not `Eq` is correct (`f16` is a float),
and that keeping `Slot` at the module level is an acceptable temporary
adaptation until the `Spatial` container lands.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d573-prompt.md`
- Result: `logs/codex-review/20260604-d573-last-message.md`
