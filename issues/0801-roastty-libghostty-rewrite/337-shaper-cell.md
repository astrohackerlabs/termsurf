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

# Experiment 337: the shaper's output cell

## Description

The **shaper** turns a run of terminal cells into positioned glyphs. It is the
largest remaining font subsystem and is **unstarted** in roastty. This
experiment lays its foundation: the shaper's **output contract** — the
`shape::Cell` (a single shaped glyph with its position) and `shape::Options`
(shaping options). These are the value types every shaper backend produces and
consumes; the CoreText shaping that fills them
(`CFAttributedString → CTLine → CTRun → Cell`) and the run iterator are the next
experiments.

## Upstream behavior (`font/shape.zig`)

```zig
/// A single glyph within a terminal to render for a shaping call. Only cells
/// with a glyph to render are present.
pub const Cell = struct {
    /// X position of this cell relative to the run's offset (runs are within a
    /// single row, so the caller adds this to the run offset and the row's Y).
    x: u16,
    /// An additional offset to apply when rendering.
    x_offset: i16 = 0,
    y_offset: i16 = 0,
    /// The glyph index for this cell (valid for the run's font/GroupCache).
    glyph_index: u32,
};

/// Options for shapers.
pub const Options = struct {
    /// Font features to apply when shaping (applied globally for now).
    features: []const []const u8 = &.{},
};
```

A `Cell` is the shaper's per-glyph output: an `x` position relative to the run,
an optional `(x_offset, y_offset)` rendering nudge, and the `glyph_index`.
`Options` carries the font features to apply during shaping.

## Rust mapping (`roastty/src/font/shape.rs`, new)

- `roastty/src/font/mod.rs`: add `pub(crate) mod shape;`.
- `roastty/src/font/shape.rs`:

  ```rust
  /// A single shaped glyph to render, output by the shaper. Faithful port of
  /// upstream `shape.Cell`.
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
  pub(crate) struct Cell {
      /// X position relative to the run's offset (runs are within one row).
      pub x: u16,
      /// An additional render offset.
      pub x_offset: i16,
      pub y_offset: i16,
      /// The glyph index (valid for the run's font).
      pub glyph_index: u32,
  }

  /// Options controlling shaping. Faithful port of upstream `shape.Options`.
  #[derive(Debug, Clone, Default, PartialEq, Eq)]
  pub(crate) struct Options {
      /// Font features to apply when shaping (e.g. `"liga"`, `"calt"`), applied
      /// globally for now.
      pub features: Vec<String>,
  }
  ```

  (`x_offset`/`y_offset` default to `0` — upstream's field defaults; the module
  doc notes the shaper subsystem's scope.)

## Scope / faithfulness notes

- **Ported**: the shaper's output value types — `shape::Cell` and
  `shape::Options` — establishing the shaper module and its output contract.
- **Deferred**: everything that _produces_ a `Cell` — the `RunIterator`
  (splitting a row into single-font runs), the `RunIteratorHook` (accumulating
  codepoints), and the CoreText `Shaper.shape` (the
  `CFAttributedString`/`CTLine`/`CTRun` pipeline) — and the `RunOptions` (which
  depends on the terminal's cell/grid types). Those are subsequent experiments.
- No C ABI/header/ABI-inventory change (`shape::Cell`/`Options` are internal
  Rust).

## Changes

1. `roastty/src/font/shape.rs`: the `Cell` and `Options` types, with the module
   doc establishing the shaper subsystem.
2. `roastty/src/font/mod.rs`: declare `pub(crate) mod shape;`.
3. Tests (in `shape.rs`):
   - `cell_defaults`: `Cell::default()` is all-zero (`x == 0`, `x_offset == 0`,
     `y_offset == 0`, `glyph_index == 0`).
   - `cell_construction`: a `Cell { x: 3, glyph_index: 42, .. }` keeps the set
     fields and zero-defaults `x_offset`/`y_offset`; the signed offsets hold
     negatives (`x_offset: -2, y_offset: -5`).
   - `cell_eq_copy`: `Cell` is `Copy` and `PartialEq` (two equal cells compare
     equal; a differing `glyph_index` compares unequal).
   - `options_default_empty`: `Options::default().features` is empty; a
     populated `Options` keeps its features.
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

- `shape::Cell` and `shape::Options` reproduce upstream's `shape.Cell`/`Options`
  (the fields, types, and defaults), establishing the shaper module;
- the defaults, construction, equality, and options tests pass;
- the run iterator, the hook, and the CoreText shaping stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if a field cannot be represented as upstream types
it (none expected — the types are plain integers and strings).

The experiment **fails** if the `Cell`/`Options` fields, types, or defaults
diverge from upstream, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no Required
findings**. It verified `Cell` matches upstream exactly (`x: u16`,
`x_offset: i16 = 0`, `y_offset: i16 = 0`, `glyph_index: u32`), that **no
`cluster` field** should be added (upstream tracks the cluster outside the
output `Cell`, in the run), that `Options { features: Vec<String> }` is a
faithful Rust-owned mapping of `[]const []const u8` (feature tags are
textual/ASCII), that deriving `Default` on `Cell` is acceptable for an internal
value type (it cleanly supports the offset defaults even though upstream
requires `x`/`glyph_index` at construction sites), and that deferring the
`RunIterator`, `RunIteratorHook`, `RunOptions` (which depends on the
grid/render-state types), the feature-list API, and the CoreText shaping
pipeline is the right slice. It confirmed the planned tests cover the contract
(zero defaults, signed offsets, copy/equality, empty/default options).

Review artifacts:

- Prompt: `logs/codex-review/20260603-124943-560974-prompt.md`
- Result: `logs/codex-review/20260603-124943-560974-last-message.md`
