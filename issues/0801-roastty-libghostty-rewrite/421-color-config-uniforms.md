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

# Experiment 421: the color-config uniforms (WindowColorspace + AlphaBlending + update_color_config)

## Description

Experiment 420 ported `min_contrast` from `changeConfig` but deferred the
color-space and blending **bool** uniforms (`use_display_p3`,
`use_linear_blending`, `use_linear_correction`) because they read two config
enums — `WindowColorspace` and `AlphaBlending` — that roastty did not have (it
has no config module yet). This experiment opens the **config layer** with those
two small, well-defined leaf enums (faithful ports of upstream's config enums)
and ports the bool-setting half of `changeConfig` as
`MetalUniforms::update_color_config`. The broader config subsystem (parsing, the
full `Config` struct, the rest of the config keys) stays deferred — this is the
minimal, concrete entry point the renderer needs.

## Upstream behavior

In `changeConfig` (`renderer/generic.zig`), after `min_contrast`, the
color-space and blending bools are set from config:

```zig
self.uniforms.bools.use_display_p3 = config.colorspace == .@"display-p3";
self.uniforms.bools.use_linear_blending = config.blending.isLinear();
self.uniforms.bools.use_linear_correction = config.blending == .@"linear-corrected";
```

The config enums (`config/Config.zig`) are:

```zig
pub const WindowColorspace = enum { srgb, @"display-p3" };

pub const AlphaBlending = enum {
    native, linear, @"linear-corrected",
    pub fn isLinear(self) bool {
        return switch (self) { .native => false, .linear, .@"linear-corrected" => true };
    }
};
```

So `use_display_p3` is set when the colorspace is `display-p3`;
`use_linear_blending` when the blending is linear (`linear` or
`linear-corrected`); `use_linear_correction` only when the blending is
`linear-corrected`.

## Rust mapping

A new `roastty/src/config` module holds the two enums (faithful to upstream):

```rust
// roastty/src/config/mod.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowColorspace {
    Srgb,
    DisplayP3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlphaBlending {
    Native,
    Linear,
    LinearCorrected,
}

impl AlphaBlending {
    /// Whether this blending mode is linear (upstream `isLinear`): `native` is
    /// not linear; `linear` and `linear-corrected` are.
    pub(crate) fn is_linear(self) -> bool {
        matches!(self, AlphaBlending::Linear | AlphaBlending::LinearCorrected)
    }
}
```

`MetalUniforms::update_color_config` sets the three bools (the bool half of
`changeConfig`):

```rust
impl MetalUniforms {
    /// Update the color-space and blending bool uniforms (upstream
    /// `changeConfig`): `use_display_p3` (the colorspace is Display P3),
    /// `use_linear_blending` (the blending is linear), and `use_linear_correction`
    /// (the blending is linear-corrected).
    pub(crate) fn update_color_config(
        &mut self,
        colorspace: WindowColorspace,
        blending: AlphaBlending,
    ) {
        self.bools.use_display_p3 = colorspace == WindowColorspace::DisplayP3;
        self.bools.use_linear_blending = blending.is_linear();
        self.bools.use_linear_correction = blending == AlphaBlending::LinearCorrected;
    }
}
```

The three assignments match upstream field-for-field; only the three color-space
bools are touched (the fourth bool, `cursor_wide`, is the cursor group's).

## Scope / faithfulness notes

- **Ported (bridged)**: the config `WindowColorspace` and `AlphaBlending` enums
  (with `is_linear`), and `MetalUniforms::update_color_config` (the
  `use_display_p3` / `use_linear_blending` / `use_linear_correction` bools) —
  upstream's config enums + the bool half of `changeConfig`.
- **Faithful**: the enum variants match upstream (`srgb`/`display-p3`;
  `native`/`linear`/`linear-corrected`); `is_linear` is upstream's `isLinear`
  (native → false, the two linear modes → true); the three bool assignments
  match `changeConfig` exactly, touching only those three bools.
- **Faithful adaptation**: the enums open a new `config` module (upstream's
  config home); `update_color_config` mutates an existing `MetalUniforms`
  (upstream mutates `self.uniforms`) and takes the two enums as parameters
  (upstream reads `config.colorspace` / `config.blending`).
- **Deferred**: the rest of the config subsystem (parsing, the full `Config`
  struct, the other config keys), the `padding_extend` flags, the macOS glass
  override, a full production `MetalUniforms` constructor, and the live
  config-change call site. (Consumed by a later slice; this experiment lands and
  tests the enums + the color-config bools.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs` (new): the `WindowColorspace` and `AlphaBlending`
   enums (with `AlphaBlending::is_linear`).
2. `roastty/src/lib.rs`: add `mod config;`.
3. `roastty/src/renderer/metal/shaders.rs`:
   - add
     `MetalUniforms::update_color_config(&mut self, colorspace: WindowColorspace, blending: AlphaBlending)`
     setting the three color-space bools. Import the two enums from
     `crate::config`.
4. Tests:
   - in `config`: `AlphaBlending::is_linear` — `Native → false`,
     `Linear → true`, `LinearCorrected → true`;
   - in `shaders.rs`: `update_color_config` over `(Srgb, Native)` →
     `(false, false, false)`; `(DisplayP3, Linear)` → `(true, true, false)`;
     `(DisplayP3, LinearCorrected)` → `(true, true, true)`; and the other
     uniform fields (e.g. `cursor_wide`, `screen_size`, `min_contrast`)
     untouched.
5. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty update_color_config
cargo test -p roastty is_linear
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- the `WindowColorspace` / `AlphaBlending` enums match upstream (variants +
  `is_linear`), and `update_color_config` sets the three color-space bools
  exactly as `changeConfig` does (touching nothing else) — faithful to upstream;
- the tests pass (the `is_linear` truth table; the three bool combinations; the
  untouched fields), and the existing tests still pass;
- the rest of the config subsystem and the other uniform groups stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates (now including `roastty/src/config`) and
  `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if an enum variant or `is_linear` is wrong, a bool is
set from the wrong condition, an unrelated uniform field is changed, or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed a new top-level `config` module is the right home for
these enums — upstream owns `WindowColorspace` and `AlphaBlending` in
`config/Config.zig`, the renderer consumes them, and putting the faithful leaf
enums in `roastty/src/config/mod.rs` is a reasonable minimal entry point without
pulling in the full config subsystem prematurely (keeping them renderer-local
would make the later config layer harder to grow cleanly). It confirmed the enum
mapping is faithful (`WindowColorspace::{Srgb, DisplayP3}` ↔
`srgb`/`display-p3`; `AlphaBlending::{Native, Linear, LinearCorrected}` ↔
`native`/`linear`/`linear-corrected`; `is_linear()` false for `Native`, true for
both linear modes), that `update_color_config` matches `changeConfig`
field-for-field and correctly leaves `cursor_wide` out of this group, and that
including `roastty/src/config` in the no-`ghostty` grep gate is the right
update. It judged the planned tests sufficient (the truth table, the three
uniform bool combinations, and the untouched-field boundary).

Review artifacts:

- Prompt: `logs/codex-review/20260604-084243-d421-prompt.md` (design)
- Result: `logs/codex-review/20260604-084243-d421-last-message.md` (design)
