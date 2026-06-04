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

# Experiment 424: the BackgroundBlur config enum (enabled + is_macos_glass)

## Description

The macOS-glass `bg_color` override (the deferred half of Experiment 419 — on a
glass style the background alpha is forced to `0` so the glass effect supplies
the opacity) reads the `background-blur` config. This experiment ports that
config type, `BackgroundBlur`, into the config layer: the tagged-union enum, its
`enabled()` method (upstream's), and an `is_macos_glass()` helper (the predicate
the glass override consumes). The override itself, and the config parsing
(`parseCLI`) and C-value (`cval`) of the type, stay deferred — this lands the
config enum the override needs (a precursor, like Experiment 423's powerline
predicate).

## Upstream behavior

`BackgroundBlur` (`config/Config.zig`) is a tagged union:

```zig
pub const BackgroundBlur = union(enum) {
    false,
    true,
    @"macos-glass-regular",
    @"macos-glass-clear",
    radius: u8,

    pub fn enabled(self: BackgroundBlur) bool {
        return switch (self) {
            .false => false,
            .true => true,
            .radius => |v| v > 0,
            // We treat these as true because they both imply some blur!
            .@"macos-glass-regular", .@"macos-glass-clear" => true,
        };
    }
    // parseCLI / cval … (config parsing + C bridge)
};
```

The macOS-glass `bg_color` override (in `updateFrame`) keys on the two glass
variants:

```zig
if (comptime builtin.os.tag == .macos) switch (self.config.background_blur) {
    .@"macos-glass-regular", .@"macos-glass-clear" => self.uniforms.bg_color[3] = 0,
    else => {},
};
```

So the override needs "is this a macOS glass style?" — the two glass variants.

## Rust mapping (`roastty/src/config/mod.rs`)

`BackgroundBlur` joins the config module (after Experiments 421/422):

```rust
/// The `background-blur` config (upstream `BackgroundBlur`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackgroundBlur {
    False,
    True,
    MacosGlassRegular,
    MacosGlassClear,
    Radius(u8),
}

impl BackgroundBlur {
    /// Whether background blur is enabled (upstream `enabled`): `False` off;
    /// `True` and the two glass styles on; `Radius(v)` on when `v > 0`.
    pub(crate) fn enabled(self) -> bool {
        match self {
            BackgroundBlur::False => false,
            BackgroundBlur::True => true,
            BackgroundBlur::Radius(v) => v > 0,
            BackgroundBlur::MacosGlassRegular | BackgroundBlur::MacosGlassClear => true,
        }
    }

    /// Whether this is a macOS glass style — the condition for the glass
    /// `bg_color` alpha override (upstream's `updateFrame` glass `switch`).
    pub(crate) fn is_macos_glass(self) -> bool {
        matches!(
            self,
            BackgroundBlur::MacosGlassRegular | BackgroundBlur::MacosGlassClear
        )
    }
}
```

The `radius: u8` union payload becomes the `Radius(u8)` variant. `enabled()`
matches upstream's `switch` (the `radius` arm is `v > 0`; both glass styles are
`true`). `is_macos_glass()` is the two glass variants — the override's
predicate.

## Scope / faithfulness notes

- **Ported (bridged)**: the `BackgroundBlur` config enum, its `enabled()` method
  (upstream's), and an `is_macos_glass()` helper (the macOS-glass `bg_color`
  override's predicate) — upstream's config type + the glass condition.
- **Faithful**: the variants match upstream (`false`/`true`/
  `macos-glass-regular`/`macos-glass-clear`/`radius: u8`); `enabled()` matches
  upstream's `switch` (`Radius(v) → v > 0`; the glass styles `true`);
  `is_macos_glass()` is the two glass variants (upstream's override `switch`
  arms).
- **Faithful adaptation**: the tagged union becomes a Rust enum with a
  `Radius(u8)` variant; `is_macos_glass()` is a named helper for the inline
  override `switch` (upstream inlines it). The enum opens with the renderer's
  needs; the config layer is `#![allow(dead_code)]`.
- **Deferred**: the macOS-glass `bg_color` alpha override itself (in a later
  uniform/frame slice), `parseCLI` (config parsing) and `cval` (the C bridge),
  and the rest of the config subsystem. (Consumed by a later slice; this
  experiment lands and tests the enum.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add the
   `BackgroundBlur { False, True, MacosGlassRegular, MacosGlassClear, Radius(u8) }`
   enum with `enabled` and `is_macos_glass`.
2. Tests (in `config`):
   - `enabled` — `False → false`, `True → true`, `Radius(0) → false`,
     `Radius(5) → true`, `MacosGlassRegular → true`, `MacosGlassClear → true`;
   - `is_macos_glass` — true for the two glass styles, false for `False` /
     `True` / `Radius`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty background_blur
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- the `BackgroundBlur` enum matches upstream's variants, `enabled()` matches
  upstream's `switch` (including `Radius(v) → v > 0`), and `is_macos_glass()` is
  exactly the two glass styles — faithful to upstream;
- the tests pass (the `enabled` truth table; the `is_macos_glass` cases), and
  the existing tests still pass;
- the glass `bg_color` override, `parseCLI` / `cval`, and the rest of config
  stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a variant or method arm is wrong (e.g. `enabled` for
`Radius(0)`, or `is_macos_glass` including a non-glass variant), or any public C
API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed the `BackgroundBlur` enum is a faithful Rust mapping of
upstream's tagged union (`False` / `True` / `MacosGlassRegular` /
`MacosGlassClear` / `Radius(u8)`); that `enabled()` matches upstream exactly,
including the important `Radius(v) => v > 0` behavior (`Radius(0)` disabled,
positive radii enabled) and both glass variants returning true; and that
`is_macos_glass()` is an acceptable local helper (upstream expresses that
condition inline in the `updateFrame` switch, and naming it gives the future
`bg_color[3] = 0` override a precise predicate without changing behavior). It
judged deferring `parseCLI`, `cval`, the actual glass alpha override, and the
broader config subsystem reasonable for this precursor slice, and the planned
tests sufficient (the truth table and the glass predicate cases).

Review artifacts:

- Prompt: `logs/codex-review/20260604-085935-d424-prompt.md` (design)
- Result: `logs/codex-review/20260604-085935-d424-last-message.md` (design)

## Result

**Result:** Pass

The `BackgroundBlur` config enum is now live.

- `roastty/src/config/mod.rs`: the
  `BackgroundBlur { False, True, MacosGlassRegular, MacosGlassClear, Radius(u8) }`
  enum, with `enabled()` (`False` off; `True` and the two glass styles on;
  `Radius(v)` on when `v > 0`) and `is_macos_glass()` (the two glass styles —
  the macOS-glass `bg_color` override's predicate).

Tests (in `config`):

- `background_blur_enabled_truth_table` — `False` false, `True` true,
  `Radius(0)` false, `Radius(5)` true, both glass styles true.
- `background_blur_is_macos_glass_only_for_glass_styles` — the two glass styles
  true; `False` / `True` / `Radius(5)` false.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2904 passed, 0 failed (+2, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates (font + renderer + config +
  `lib.rs`/header/`abi_harness.c`) clean; `git diff --check` clean.

## Conclusion

The config layer now has four enums (`WindowColorspace`, `AlphaBlending`,
`WindowPaddingColor`, `BackgroundBlur`). With `BackgroundBlur::is_macos_glass`
in place, the macOS-glass `bg_color` alpha override (setting `bg_color[3] = 0`
under a glass style — the deferred half of Experiment 419) is now portable as a
small follow-up. The other remaining renderer-bridge work: the full
`neverExtendBg` and its per-row `padding_extend` refinement (awaiting the
renderer's terminal-core row/cell representation), a production `MetalUniforms`
constructor composing the update groups, and the live per-frame call sites;
`parseCLI` / `cval` and the broader config subsystem stay deferred.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no findings**. It confirmed the implementation faithfully maps upstream's
`BackgroundBlur` tagged union (`False` / `True` / `MacosGlassRegular` /
`MacosGlassClear` / `Radius(u8)`); that `enabled()` matches upstream exactly
(including the critical `Radius(v) => v > 0` behavior); and that
`is_macos_glass()` is exactly the two glass variants used by upstream's
`updateFrame` switch for the future `bg_color[3] = 0` override. It judged the
tests to cover both truth tables clearly, and the deferred parsing / C-bridge /
glass-override scope correct. No public C ABI/header impact; nothing needed to
change before the result commit.

Review artifacts:

- Prompt: `logs/codex-review/20260604-090118-r424-prompt.md` (result)
- Result: `logs/codex-review/20260604-090118-r424-last-message.md` (result)
