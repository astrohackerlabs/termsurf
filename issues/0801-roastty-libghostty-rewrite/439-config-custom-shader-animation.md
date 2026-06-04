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

# Experiment 439: the custom-shader-animation config enum and its animate decision (CustomShaderAnimation, should_animate)

## Description

This experiment ports the `custom-shader-animation` config enum —
`CustomShaderAnimation { False, True, Always }` — **and the consumer logic**
that decides whether the renderer keeps its custom-shader animation draw timer
running. Upstream's render thread switches on the config (and the focused state)
to decide whether to animate; this experiment captures that as a
`CustomShaderAnimation::should_animate(focused)` method. It continues the
custom-shader work (Experiments 428–435) at the config level; the render-thread
draw-timer call site stays deferred.

## Upstream behavior

In `config/Config.zig`, the enum and its `Config` field (default `.true`):

```zig
@"custom-shader-animation": CustomShaderAnimation = .true,

pub const CustomShaderAnimation = enum(c_int) {
    false,
    true,
    always,
};
```

In `renderer/Thread.zig`'s `syncDrawTimer`, the config and the focused state
decide whether to keep the animation draw timer active:

```zig
if (@hasDecl(rendererpkg.Renderer, "hasAnimations") and self.renderer.hasAnimations()) {
    switch (self.config.custom_shader_animation) {
        // Always animate
        .always => break :skip,
        // Only when focused
        .true => if (self.flags.focused) break :skip,
        // Never animate
        .false => {},
    }
}
// ... falls through to stopping the draw timer
```

`break :skip` keeps the draw timer running (animate); falling through stops it.
So `always` animates unconditionally, `true` animates only when the window is
focused, and `false` never animates.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// The `custom-shader-animation` config (upstream `CustomShaderAnimation`):
/// whether custom-shader animations run. The `Config` default is `True`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CustomShaderAnimation {
    /// Never animate.
    False,
    /// Animate only when the window is focused.
    True,
    /// Always animate, focused or not.
    Always,
}

impl CustomShaderAnimation {
    /// Whether the custom-shader animation draw timer should run, given the
    /// window's focused state (upstream `Thread.zig`'s `syncDrawTimer` switch):
    /// `Always` always animates, `True` animates only when `focused`, `False`
    /// never animates.
    pub(crate) fn should_animate(self, focused: bool) -> bool {
        match self {
            CustomShaderAnimation::Always => true,
            CustomShaderAnimation::True => focused,
            CustomShaderAnimation::False => false,
        }
    }
}
```

`should_animate` returns whether the draw timer keeps running: `Always → true`,
`True → focused`, `False → false` — exactly the upstream `switch`'s
`break :skip` condition. The `match` is exhaustive (no wildcard).

## Scope / faithfulness notes

- **Ported (bridged)**: the `CustomShaderAnimation` config enum
  (`config/Config.zig`) and its animate decision
  (`CustomShaderAnimation::should_animate`, upstream's `Thread.zig`
  `syncDrawTimer` switch).
- **Faithful**: the enum has the three upstream variants (`false`, `true`,
  `always`); `should_animate` returns `true` for `Always`, `focused` for `True`,
  and `false` for `False` — exactly the `break :skip` (keep animating) condition
  of the upstream switch, with an exhaustive `match`.
- **Faithful adaptation**: upstream declares the enum `enum(c_int)` for
  `ghostty.h` extern compatibility; in roastty this config is internal
  (`pub(crate)`, not yet crossing roastty's C ABI), so a plain Rust enum is the
  faithful internal mapping (a `#[repr(C)]` would be added if/when roastty
  exposes it across its C boundary). The consumer is modeled as a method
  returning the animate decision (upstream inlines the switch with
  `break :skip`); the `hasAnimations` gate and the draw-timer mutation are the
  deferred render-thread wiring.
- **Deferred**: the `Config` struct / parsing (and the `.true` field default),
  and the render-thread `syncDrawTimer` call site (the `hasAnimations` gate and
  the `draw_active` draw-timer toggle) that consumes this decision. (Consumed by
  a later slice; this experiment lands the enum and the decision.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add `pub(crate) enum CustomShaderAnimation { False, True, Always }` (derive
     `Debug, Clone, Copy, PartialEq, Eq`) and
     `CustomShaderAnimation::should_animate(self, focused: bool) -> bool`
     (exhaustive `match`).
2. Tests (in `config/mod.rs`):
   - `should_animate`: the full truth table over the three variants ×
     `focused ∈ {true, false}` — `Always` → `true`/`true`, `True` →
     `true`/`false`, `False` → `false`/`false`; plus the variants distinct and a
     `Copy`/`Eq` round-trip.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty custom_shader_animation
cargo test -p roastty should_animate
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `CustomShaderAnimation` has the three upstream variants and `should_animate`
  returns `true` for `Always`, `focused` for `True`, `false` for `False` via an
  exhaustive `match` — faithful to upstream's enum and `syncDrawTimer` switch;
- the tests pass (the full truth table; the distinct variants), and the existing
  tests still pass;
- the `Config` struct and the render-thread call site stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a variant is missing/extra, `should_animate` maps a
case the wrong way (e.g. `True` ignoring `focused`), a wildcard `match` arm
hides a future variant, an unrelated item changes, or any public C API/ABI
changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream: the variants match
exactly (`false`, `true`, `always`, `Config.zig:5244`); the default `.true`
belongs on the deferred `Config` field (`Config.zig:3067`), not the enum;
`should_animate(focused)` exactly extracts the `syncDrawTimer` `break :skip`
condition (`Thread.zig:303`, `Always → true`, `True → focused`, `False → false`,
with falling through stopping the timer at `Thread.zig:313`, so the returned
bool's meaning is right); a plain Rust enum is appropriate while this stays
`pub(crate)` internal (upstream's `enum(c_int)` is for `ghostty.h` extern
compat; roastty can add `repr(C)` if it crosses its C ABI later); and the
exhaustive `match` and full truth-table test are the right shape.

Review artifacts:

- Prompt: `logs/codex-review/20260604-102644-d439-prompt.md` (design)
- Result: `logs/codex-review/20260604-102644-d439-last-message.md` (design)

## Result

**Result:** Pass

The custom-shader-animation config enum and its animate decision are now live.

- `roastty/src/config/mod.rs`:
  `pub(crate) enum CustomShaderAnimation { False, True, Always }` (upstream
  `CustomShaderAnimation`) and
  `CustomShaderAnimation::should_animate(self, focused: bool) -> bool` — an
  exhaustive `match` (`Always → true`, `True → focused`, `False → false`), the
  extraction of upstream's `syncDrawTimer` `break :skip` condition.

Test (in `config/mod.rs`): `custom_shader_animation_should_animate_truth_table`
— the full 3×2 truth table (`Always → true/true`, `True → true/false`,
`False → false/false`), the variants distinct, `Copy`/`Eq`.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2926 passed, 0 failed (+1, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates (font + renderer + config +
  `lib.rs`/header/`abi_harness.c`) clean; `git diff --check` clean.

## Conclusion

The config layer now carries `CustomShaderAnimation` and its animate decision —
the third config slice in a row (after `FontShapingBreak` and
`GraphemeWidthMethod`) to land its consumer logic alongside the type, here the
bool the render thread's `syncDrawTimer` uses to keep the animation draw timer
running. The `Config` struct / parsing and the render-thread call site (the
`hasAnimations` gate and the `draw_active` toggle) stay deferred, as does the
eventual `#[repr(C)]` if this config ever crosses roastty's C ABI. The
config-type family — consistently pairing a config type with its behavior —
remains a clean, gated way to advance the rewrite while the larger coupled
subsystems stay deferred.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no findings**. It confirmed faithfulness against the vendored upstream:
`CustomShaderAnimation::{False, True, Always}` matches the `enum(c_int)`
variants (`Config.zig:5244`); the deferred default `.true` (`Config.zig:3067`)
is documented; `should_animate` exactly extracts the `syncDrawTimer` condition
(`Thread.zig:303`, `Always → true`, `True → focused`, `False → false`, with
`false` corresponding to falling through to stop the timer at `Thread.zig:313`);
and a plain enum remains appropriate while internal. It judged the full 3×2
truth-table test adequate. No public C ABI/header impact; nothing needed to
change before the result commit.

Review artifacts:

- Prompt: `logs/codex-review/20260604-102848-r439-prompt.md` (result)
- Result: `logs/codex-review/20260604-102848-r439-last-message.md` (result)
