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

# Experiment 466: grow the Config struct with the background-image group

## Description

Continuing the incremental growth of the aggregating `Config` struct
(Experiments 461–465), this experiment adds the **background-image** group:
`bg_image_opacity` (`f32`), `bg_image_position` (`BackgroundImagePosition`),
`bg_image_fit` (`BackgroundImageFit`), and `bg_image_repeat` (`bool`). Two are
already-ported leaf enums; the other two are scalar fields. This is the first
group with a **float** field (`bg_image_opacity`), validating the
`PartialEq`-not-`Eq` derive chosen in Experiment 461. The `bg_image` path itself
(a `Path` type not yet ported), the parser, and the rest of upstream `Config`
stay deferred.

## Upstream behavior

In `config/Config.zig`, the background-image group's field defaults:

```zig
@"background-image-opacity": f32 = 1.0,
@"background-image-position": BackgroundImagePosition = .center,
@"background-image-fit": BackgroundImageFit = .contain,
@"background-image-repeat": bool = false,
```

`background-image-opacity` defaults to `1.0`; `background-image-position`
defaults to `.center`; `background-image-fit` defaults to `.contain`;
`background-image-repeat` defaults to `false`.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
pub(crate) struct Config {
    // ... clipboard (461) … renderer-appearance (465) ...
    /// `background-image-opacity`.
    pub bg_image_opacity: f32,
    /// `background-image-position`.
    pub bg_image_position: BackgroundImagePosition,
    /// `background-image-fit`.
    pub bg_image_fit: BackgroundImageFit,
    /// `background-image-repeat`.
    pub bg_image_repeat: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // ... earlier groups ...
            bg_image_opacity: 1.0,
            bg_image_position: BackgroundImagePosition::Center,
            bg_image_fit: BackgroundImageFit::Contain,
            bg_image_repeat: false,
        }
    }
}
```

The defaults are upstream's Config-field defaults: `background-image-opacity`
`1.0`, `background-image-position` `Center`, `background-image-fit` `Contain`,
`background-image-repeat` `false`. The `f32` field is why `Config` derives
`PartialEq` and not `Eq`.

## Scope / faithfulness notes

- **Ported (bridged)**: the background-image field group of the aggregating
  `Config` struct (upstream `config.Config`) — the four fields and their
  `Default`.
- **Faithful**: the four fields use the already-ported types
  (`BackgroundImagePosition`, `BackgroundImageFit`) and scalars (`f32`, `bool`);
  their `Default` values match upstream's Config-field defaults (`1.0`,
  `.center`, `.contain`, `false`).
- **Faithful adaptation**: `bg_image_opacity` is a plain `f32` (upstream `f32`),
  exercising the `Config` `PartialEq`-not-`Eq` derive (Experiment 461). The
  struct continues to grow one coherent field group per experiment. The derive
  set is unchanged.
- **Deferred**: the `bg_image` path field itself (the `?Path` value — the `Path`
  type is not yet ported), the rest of upstream `Config`'s fields (added group
  by group later), the parser, the `changeConfig` machinery, and the
  conditional-config system. (Consumed by later slices; this experiment grows
  the struct with the background-image group's scalar / enum fields.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add the four fields `bg_image_opacity: f32`,
     `bg_image_position: BackgroundImagePosition`,
     `bg_image_fit: BackgroundImageFit`, `bg_image_repeat: bool` to `Config`,
     and their defaults (`1.0`, `Center`, `Contain`, `false`) to the `Default`
     impl.
2. Tests (in `config/mod.rs`):
   - extend the `Config::default()` assertion for the new fields:
     `bg_image_opacity == 1.0`,
     `bg_image_position == BackgroundImagePosition::Center`,
     `bg_image_fit == BackgroundImageFit::Contain`, `bg_image_repeat == false`;
     the existing group defaults still hold.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty config_default
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Config` gains the four background-image fields, and `Config::default()` sets
  their upstream defaults (`background-image-opacity` `1.0`,
  `background-image-position` `Center`, `background-image-fit` `Contain`,
  `background-image-repeat` `false`) while the earlier group defaults still hold
  — a faithful partial of upstream's `Config`;
- the tests pass (the new defaults; the existing defaults), and the existing
  tests still pass;
- the `bg_image` path field, the rest of upstream `Config`, and the parser stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a default is wrong, a field uses the wrong type, an
unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream: the defaults are correct
(`bg_image_opacity = 1.0`, `Config.zig:639`;
`bg_image_position = BackgroundImagePosition::Center`, the standalone `.center`
rather than `center-center`, `Config.zig:657` / `:9611`;
`bg_image_fit = BackgroundImageFit::Contain`, `Config.zig:687`;
`bg_image_repeat = false`, `Config.zig:698`); deferring
`background-image: ?Path = null` is reasonable (the `Path` type is not ported
yet and the scalar/enum behavior is self-contained, `Config.zig:618`); `f32` is
the right Rust type for upstream `f32` and `PartialEq` is sufficient (matching
the Experiment 461 choice); and the test plan is adequate (assert the four new
defaults and keep the existing groups covered as `Config::default()` grows).

Review artifacts:

- Prompt: `logs/codex-review/20260604-122311-d466-prompt.md` (design)
- Result: `logs/codex-review/20260604-122311-d466-last-message.md` (design)

## Result

**Result:** Pass

The `Config` struct now carries the background-image field group.

- `roastty/src/config/mod.rs`: `Config` gains `bg_image_opacity: f32`,
  `bg_image_position: BackgroundImagePosition`,
  `bg_image_fit: BackgroundImageFit`, and `bg_image_repeat: bool`;
  `Config::default()` sets their upstream Config-field defaults — `1.0`,
  `BackgroundImagePosition::Center`, `BackgroundImageFit::Contain`, `false`.

Test (in `config/mod.rs`): `config_default_clipboard_group` extended to assert
the four new background-image defaults (`1.0` / `Center` / `Contain` / `false`)
alongside the five prior groups' defaults; the modified-config inequality and
the `Clone`/`PartialEq` round-trip remain.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2952 passed, 0 failed (no regressions; the existing
  `config_default` test was extended).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates (font + renderer + config +
  `lib.rs`/header/`abi_harness.c`) clean; `git diff --check` clean.

## Conclusion

The aggregating `Config` struct now holds six field groups — clipboard (461),
mouse/click (462), shell-integration (463), notification (464),
renderer-appearance (465), and background-image — eighteen fields total, and the
first **float** field (`bg_image_opacity: f32`), which validates the
`PartialEq`-not-`Eq` derive chosen forward-lookingly in Experiment 461. The
`bg_image` path field (a `Path` type not yet ported), the parser, the
`changeConfig` machinery, the conditional-config system, and the remaining
upstream `Config` fields stay deferred.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no findings**. It confirmed the four background-image fields were added with
faithful defaults (`1.0`, standalone `Center`, `Contain`, `false`); deferring
the `bg_image` path remains the right scope (the `Path` type is not ported yet);
`f32` plus `PartialEq` is appropriate for this internal aggregate; and extending
the existing `Config::default()` test is adequate and keeps all prior groups
covered. No public C ABI/header impact; nothing needed to change before the
result commit.

Review artifacts:

- Prompt: `logs/codex-review/20260604-122508-r466-prompt.md` (result)
- Result: `logs/codex-review/20260604-122508-r466-last-message.md` (result)
