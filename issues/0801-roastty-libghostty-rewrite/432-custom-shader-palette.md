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

# Experiment 432: the custom-shader palette update (update_palette)

## Description

`updateCustomShaderUniformsFromState` updates the custom-shader uniforms from
the terminal state's colors — the 256-color palette and the
background/foreground/cursor/selection colors. This experiment ports its first,
most distinctive piece: the **256-color palette** loop, which normalizes each
palette color to `[r/255, g/255, b/255, 1.0]`. It takes the `Palette` as a
parameter (deferring the rest of the from-state group, the `dirty` gate, and the
live terminal state). It builds on the `CustomShaderUniforms` value type
(Experiment 428).

## Upstream behavior

In `updateCustomShaderUniformsFromState` (`renderer/generic.zig`), the palette
is copied from the terminal colors, each channel normalized to `[0, 1]` with an
opaque alpha:

```zig
// 256-color palette
for (colors.palette, 0..) |color, i| {
    uniforms.palette[i] = .{
        @as(f32, @floatFromInt(color.r)) / 255.0,
        @as(f32, @floatFromInt(color.g)) / 255.0,
        @as(f32, @floatFromInt(color.b)) / 255.0,
        1.0,
    };
}
```

Each of the 256 palette entries becomes a `vec4` of the normalized RGB plus
alpha `1.0`.

## Rust mapping (`roastty/src/renderer/shadertoy.rs`)

roastty's `Palette` is `[Rgb; 256]`. `update_palette` mirrors the loop:

```rust
impl CustomShaderUniforms {
    /// Update the 256-color palette uniform (the palette loop of upstream
    /// `updateCustomShaderUniformsFromState`): each palette color becomes a
    /// `vec4` of the normalized RGB (`channel / 255`) with an opaque alpha.
    pub(crate) fn update_palette(&mut self, palette: &Palette) {
        for (i, color) in palette.iter().enumerate() {
            self.palette[i] = [
                f32::from(color.r) / 255.0,
                f32::from(color.g) / 255.0,
                f32::from(color.b) / 255.0,
                1.0,
            ];
        }
    }
}
```

Each entry is `[r/255, g/255, b/255, 1.0]`, matching upstream's
`@floatFromInt / 255.0` with the `1.0` alpha. Only `palette` is touched.

## Scope / faithfulness notes

- **Ported (bridged)**: `CustomShaderUniforms::update_palette` — the 256-color
  palette loop of upstream's `updateCustomShaderUniformsFromState`.
- **Faithful**: each of the 256 entries is `[r/255, g/255, b/255, 1.0]` (the
  channels normalized to `[0, 1]`, opaque alpha), matching upstream; only the
  `palette` field is touched.
- **Faithful adaptation**: the `Palette` (`[Rgb; 256]`) is a parameter (upstream
  reads `self.terminal_state.colors.palette`); the `f32::from(u8) / 255.0`
  matches `@floatFromInt / 255.0`.
- **Deferred**: the rest of `updateCustomShaderUniformsFromState` (the
  background / foreground / cursor / cursor-text / selection colors, the
  `cursor_visible` and cursor-style fields), the `dirty` gate, the live terminal
  state, and the `has_custom_shaders` gate. (Consumed by a later slice; this
  experiment lands and tests the palette update.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/shadertoy.rs`:
   - add `CustomShaderUniforms::update_palette(&mut self, palette: &Palette)`.
     Import `Palette` from `crate::terminal::color`.
2. Tests (in `shadertoy.rs`):
   - a `Palette` (zeroed, with `[5] = Rgb(255, 128, 0)` and
     `[255] = Rgb(0, 0, 255)`) → `update_palette` sets
     `palette[0] == [0.0, 0.0, 0.0, 1.0]`,
     `palette[5] == [1.0, 128.0 / 255.0, 0.0, 1.0]`,
     `palette[255] == [0.0, 0.0, 1.0, 1.0]`; and the other uniform fields (e.g.
     `background_color`, `focus`, `frame`) untouched.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty update_palette
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `update_palette` sets each of the 256 palette entries to
  `[r/255, g/255, b/255, 1.0]` from the `Palette` and touches nothing else —
  faithful to upstream's palette loop;
- the test passes (the normalized entries with the opaque alpha; the untouched
  fields), and the existing tests still pass;
- the rest of the from-state group, the `dirty` gate, and the live state stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a palette entry is computed wrong (the normalization
or the alpha), an unrelated uniform field is changed, or any public C API/ABI
changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed the design is a faithful slice of upstream
`updateCustomShaderUniformsFromState`: it iterates all 256 `Palette` entries and
writes `[r / 255.0, g / 255.0, b / 255.0, 1.0]` into the matching
`CustomShaderUniforms.palette` slot, with `Palette = [Rgb; 256]` and
`[[f32; 4]; 256]` lining up exactly. It confirmed `f32::from(u8) / 255.0`
matches upstream's `@floatFromInt(...) / 255.0` and the opaque alpha `1.0` is
correct, and that taking `&Palette` as a parameter is an appropriate boundary
while the rest of the from-state group and the dirty gate remain deferred. It
judged the planned test to cover low, middle, and final entries plus untouched
representative fields.

Review artifacts:

- Prompt: `logs/codex-review/20260604-094305-d432-prompt.md` (design)
- Result: `logs/codex-review/20260604-094305-d432-last-message.md` (design)
