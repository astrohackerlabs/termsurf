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

# Experiment 235: Port the Font `Metrics` Struct

## Description

Continue the font subsystem by porting the `Metrics` value type from upstream
`font/Metrics.zig` — the recommended cell dimensions and decoration
positions/thicknesses for a monospace grid using a given font. `Metrics` is the
output of the (deferred) `calc` routine and is consumed by the sprite renderer
and the cell-layout path.

`font/Metrics.zig` is ~816 lines (the struct, a `Minimums` table, a
`FaceMetrics` input struct, the `calc` derivation, constraint application, and
tests). Per the risk-based sizing rule this is split: **this experiment ports
only the `Metrics` struct (pure data)**; `FaceMetrics`, `Minimums`, `calc`,
constraint application, and their tests are later slices.

### Fields to port (exact upstream names, order, and types)

| Field                     | Type  | Meaning                                           |
| ------------------------- | ----- | ------------------------------------------------- |
| `cell_width`              | `u32` | recommended cell width                            |
| `cell_height`             | `u32` | recommended cell height                           |
| `cell_baseline`           | `u32` | pixels from cell bottom to text baseline          |
| `underline_position`      | `u32` | pixels from cell top to underline top             |
| `underline_thickness`     | `u32` | underline thickness                               |
| `strikethrough_position`  | `u32` | pixels from cell top to strikethrough top         |
| `strikethrough_thickness` | `u32` | strikethrough thickness                           |
| `overline_position`       | `i32` | pixels from cell top to overline top (may be < 0) |
| `overline_thickness`      | `u32` | overline thickness                                |
| `box_thickness`           | `u32` | box-drawing line thickness                        |
| `cursor_thickness`        | `u32` | cursor sprite thickness (upstream default `1`)    |
| `cursor_height`           | `u32` | cursor sprite height                              |
| `icon_height`             | `f64` | nerd-font icon constraint height                  |
| `icon_height_single`      | `f64` | nerd-font icon constraint height, single-cell     |
| `face_width`              | `f64` | unrounded face width (scaling)                    |
| `face_height`             | `f64` | unrounded face height (scaling)                   |
| `face_y`                  | `f64` | offset from cell bottom to face bbox bottom       |

### Faithfulness and scope notes

- `overline_position` is **signed** (`i32`) — it can be negative to sit above
  the cell top; all other integer fields are `u32`. `icon_height`,
  `icon_height_single`, `face_width`, `face_height`, `face_y` are `f64`.
- `cursor_thickness` has an **upstream default of `1`** (it is
  user-config-driven, not font-derived). Rust struct literals require every
  field, so the port keeps `cursor_thickness: u32` as a plain field and
  documents that `calc`/config sets it to `1` by default; the default is applied
  at construction by the deferred `calc` slice, not by the struct. No `Default`
  derive is added (the other 16 fields have no sensible default).
- Placed in `roastty/src/font/metrics.rs`, wired from `font/mod.rs`.
- `Metrics` derives `Debug, Clone, Copy, PartialEq` — **not `Eq`**, because it
  carries `f64` fields.
- No `FaceMetrics`/`Minimums`/`calc`/constraint behavior — only the struct.
- No C ABI, header, or ABI inventory changes; no new dependencies.

## Changes

1. Create `roastty/src/font/metrics.rs`:
   - Module attribution comment ("upstream `font/Metrics.zig`", no literal
     `ghostty` token).
   - `pub(crate) struct Metrics { ... }` with the 17 fields above (`pub` fields,
     upstream doc comments), deriving `Debug, Clone, Copy, PartialEq`.

2. `roastty/src/font/mod.rs`: add `pub(crate) mod metrics;`.

3. Tests in `roastty/src/font/metrics.rs`:
   - `metrics_holds_fields`: construct a `Metrics` with distinct values and read
     every field back.
   - `metrics_overline_position_is_signed`: a negative `overline_position`
     round-trips.
   - `metrics_face_fields_are_f64`: a fractional `face_width`/`icon_height`
     round-trips (confirms the `f64` fields).

4. Format and test (`cargo fmt`, accept output).

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

- `Metrics` is ported with the exact upstream field names, order, and types
  (signed `i32` `overline_position`, the five `f64` fields, the rest `u32`);
- `PartialEq` (not `Eq`) is derived because of the `f64` fields;
- the field round-trip, signed-overline, and `f64` tests pass;
- no `FaceMetrics`/`Minimums`/`calc`/constraint scope is pulled in;
- no C ABI, header, or ABI inventory changes;
- `cargo fmt` accepted and `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if a field's type or the `cursor_thickness`
default handling needs revisiting once `calc` is designed.

The experiment **fails** if a field type/name diverges from upstream (e.g. an
unsigned `overline_position`, or deriving `Eq` over `f64`), if `calc`/constraint
behavior leaks in, or if any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no issues**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-081931-849868-prompt.md`
- Result: `logs/codex-review/20260602-081931-849868-last-message.md`

Codex confirmed all 17 field names, order, and types match upstream exactly
(signed `i32` `overline_position`, the five `f64` fields, the rest `u32`), that
deriving `PartialEq` but not `Eq` is correct because of the `f64` fields, and
that keeping `cursor_thickness` as a plain `u32` field (deferring the upstream
`= 1` default to the `calc`/config path) is the faithful Rust shape for this
slice. The three tests are adequate. No changes required.

## Result

**Result:** Pass

Created `roastty/src/font/metrics.rs` with `pub(crate) struct Metrics` (the 17
fields with upstream names, order, types, and doc comments — signed `i32`
`overline_position`, the five `f64` fields, the rest `u32`), deriving
`Debug, Clone, Copy, PartialEq` (not `Eq`). `cursor_thickness` is a plain `u32`
field documented as config-driven (default applied by the deferred `calc`).
Wired `pub(crate) mod metrics;` into `font/mod.rs`.

Tests added (3): `metrics_holds_fields` (all 17 fields round-trip),
`metrics_overline_position_is_signed` (negative value), and
`metrics_face_fields_are_f64` (fractional `face_width`/`icon_height`).

### Verification

```bash
cargo fmt -p roastty
cargo test -p roastty font
cargo test -p roastty
```

Observed:

- `font`: 7 passed (4 prior + 3 new).
- Full `roastty`: 2283 unit tests passed (2280 prior + 3 new), plus the C ABI
  harness passed.
- `cargo fmt -p roastty -- --check`: clean.
- `cargo build -p roastty`: no warnings.
- No-`ghostty`-name gates passed for `roastty/src/font` and for
  `roastty/src/lib.rs`, `roastty/include/roastty.h`,
  `roastty/tests/abi_harness.c`.
- `git diff --check`: clean.

No C ABI, header, or ABI inventory changes; no `FaceMetrics`/`Minimums`/`calc`/
constraint scope pulled in.

### Completion Review

Codex reviewed the completed implementation and found **no issues** ("nothing
needs to change before the result commit").

Review artifacts:

- Prompt: `logs/codex-review/20260602-082059-392885-prompt.md`
- Result: `logs/codex-review/20260602-082059-392885-last-message.md`

Codex confirmed all 17 fields match upstream in order and type
(`overline_position` `i32`, the five `f64` fields, the rest `u32`), that
`PartialEq` without `Eq` is correct, that `cursor_thickness` as a plain `u32`
with the deferred default is the right scope, that the three tests are correct,
and that the module wiring is internal with no ABI exposure.

## Conclusion

Experiment 235 succeeds. The font module now has the `Metrics` value type — the
computed cell dimensions and decoration positions a font produces. Both Codex
gates passed with zero findings.

The remaining `font/Metrics.zig` slices build on this struct: the `FaceMetrics`
input type and `Minimums` table, then `calc` (deriving `Metrics` from a face's
raw metrics with rounding/clamping — the substantive behavior, with the upstream
tests), and constraint application. Those are the next font experiments, leading
toward the glyph `Atlas` and the CoreText face/rasterization core.
