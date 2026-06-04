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

# Experiment 465: grow the Config struct with the renderer-appearance group

## Description

Continuing the incremental growth of the aggregating `Config` struct
(Experiments 461–464), this experiment adds the **renderer-appearance** group:
`window_colorspace`, `alpha_blending`, `background_blur`, and
`window_padding_color` — all already-ported leaf enums (`WindowColorspace`,
`AlphaBlending`, `BackgroundBlur`, `WindowPaddingColor`). It adds the four
fields and their upstream `Config`-field defaults to `Config` and its `Default`.
The parser and the rest of upstream `Config` stay deferred.

## Upstream behavior

In `config/Config.zig`, the renderer-appearance group's field defaults:

```zig
@"alpha-blending": AlphaBlending =
    if (builtin.os.tag == .macos) .native else .@"linear-corrected",
@"background-blur": BackgroundBlur = .false,
@"window-padding-color": WindowPaddingColor = .background,
@"window-colorspace": WindowColorspace = .srgb,
```

`alpha-blending` defaults to `.native` on macOS (`.linear-corrected` elsewhere);
`background-blur` defaults to `.false`; `window-padding-color` defaults to
`.background`; `window-colorspace` defaults to `.srgb`. roastty is macOS-only,
so `alpha-blending` is `Native`.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
pub(crate) struct Config {
    // ... clipboard (461), mouse/click (462), shell-integration (463),
    //     notification (464) ...
    /// `window-colorspace`.
    pub window_colorspace: WindowColorspace,
    /// `alpha-blending`.
    pub alpha_blending: AlphaBlending,
    /// `background-blur`.
    pub background_blur: BackgroundBlur,
    /// `window-padding-color`.
    pub window_padding_color: WindowPaddingColor,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // ... earlier groups ...
            window_colorspace: WindowColorspace::Srgb,
            alpha_blending: AlphaBlending::Native,
            background_blur: BackgroundBlur::False,
            window_padding_color: WindowPaddingColor::Background,
        }
    }
}
```

The defaults are upstream's Config-field defaults: `window-colorspace` `Srgb`,
`alpha-blending` macOS `Native`, `background-blur` `False`,
`window-padding-color` `Background`.

## Scope / faithfulness notes

- **Ported (bridged)**: the renderer-appearance field group of the aggregating
  `Config` struct (upstream `config.Config`) — the four fields and their
  `Default`.
- **Faithful**: the four fields use the already-ported types
  (`WindowColorspace`, `AlphaBlending`, `BackgroundBlur`, `WindowPaddingColor`);
  their `Default` values match upstream's Config-field defaults (`.srgb`, macOS
  `.native`, `.false`, `.background`).
- **Faithful adaptation**: the macOS-only `alpha-blending` default is `Native`
  (upstream's `.macos => .native`); roastty is macOS-only, so the OS `if` is
  resolved to the macOS arm (matching the `copy-on-select` macOS-resolution in
  Experiment 461). The struct continues to grow one coherent field group per
  experiment. The derive set (`Clone`/`PartialEq`) is unchanged.
- **Deferred**: the rest of upstream `Config`'s fields (added group by group in
  later slices), the parser, the `changeConfig` machinery, and the
  conditional-config system. (Consumed by later slices; this experiment grows
  the struct with the renderer-appearance group.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add the four fields `window_colorspace: WindowColorspace`,
     `alpha_blending: AlphaBlending`, `background_blur: BackgroundBlur`,
     `window_padding_color: WindowPaddingColor` to `Config`, and their defaults
     (`Srgb`, `Native`, `False`, `Background`) to the `Default` impl. Add
     `WindowColorspace` and `WindowPaddingColor` to the test-module imports if
     needed.
2. Tests (in `config/mod.rs`):
   - extend the `Config::default()` assertion for the new fields:
     `window_colorspace == WindowColorspace::Srgb`,
     `alpha_blending == AlphaBlending::Native`,
     `background_blur == BackgroundBlur::False`,
     `window_padding_color == WindowPaddingColor::Background`; the existing
     group defaults still hold.
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

- `Config` gains the four renderer-appearance fields, and `Config::default()`
  sets their upstream defaults (`window-colorspace` `Srgb`, macOS
  `alpha-blending` `Native`, `background-blur` `False`, `window-padding-color`
  `Background`) while the earlier group defaults still hold — a faithful partial
  of upstream's `Config`;
- the tests pass (the new defaults; the existing defaults), and the existing
  tests still pass;
- the rest of upstream `Config` and the parser stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a default is wrong (e.g. `alpha-blending` not macOS
`Native`), a field uses the wrong type, an unrelated item changes, or any public
C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream:
`window_colorspace = WindowColorspace::Srgb` matches `.srgb`
(`Config.zig:2142`); `alpha_blending = AlphaBlending::Native` is correct for
macOS-only roastty (upstream's macOS branch, `Config.zig:400`);
`background_blur = BackgroundBlur::False` matches `.false` (`Config.zig:1061`);
`window_padding_color = WindowPaddingColor::Background` matches `.background`
(`Config.zig:1999`); the group is coherent (all feed renderer/window appearance
and the leaf types are ported); the field names are consistent with the config
keys and existing Rust naming; and the test plan is adequate (assert the four
new defaults and keep the existing groups covered as `Default` grows).

Review artifacts:

- Prompt: `logs/codex-review/20260604-121908-d465-prompt.md` (design)
- Result: `logs/codex-review/20260604-121908-d465-last-message.md` (design)

## Result

**Result:** Pass

The `Config` struct now carries the renderer-appearance field group.

- `roastty/src/config/mod.rs`: `Config` gains
  `window_colorspace: WindowColorspace`, `alpha_blending: AlphaBlending`,
  `background_blur: BackgroundBlur`, and
  `window_padding_color: WindowPaddingColor`; `Config::default()` sets their
  upstream Config-field defaults — `WindowColorspace::Srgb`,
  `AlphaBlending::Native` (macOS), `BackgroundBlur::False`,
  `WindowPaddingColor::Background`.

Test (in `config/mod.rs`): `config_default_clipboard_group` extended to assert
the four new appearance defaults alongside the existing clipboard, mouse/click,
shell-integration, and notification defaults; the modified-config inequality and
the `Clone`/`PartialEq` round-trip remain. Added `WindowColorspace` /
`WindowPaddingColor` to the test-module imports.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2952 passed, 0 failed (no regressions; the existing
  `config_default` test was extended).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates (font + renderer + config +
  `lib.rs`/header/`abi_harness.c`) clean; `git diff --check` clean.

## Conclusion

The aggregating `Config` struct now holds five field groups — clipboard (461),
mouse/click (462), shell-integration (463), notification (464), and
renderer-appearance — fourteen fields total. The macOS-only `alpha-blending`
resolution to `Native` reuses the OS-`switch` resolution pattern established for
`copy-on-select` (Experiment 461). The parser, the `changeConfig` machinery, the
conditional-config system, and the remaining upstream `Config` fields stay
deferred.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no findings**. It confirmed the four renderer-appearance fields were added
with faithful defaults (`Srgb`, macOS `Native`, `False`, `Background`);
resolving `alpha_blending` to the macOS arm is correct for roastty's macOS-only
scope; extending the existing `Config::default()` test is adequate and keeps all
prior groups covered as the aggregate grows; and the deferred parser /
`changeConfig` / conditional-config work remains properly scoped. No public C
ABI/header impact; nothing needed to change before the result commit.

Review artifacts:

- Prompt: `logs/codex-review/20260604-122110-r465-prompt.md` (result)
- Result: `logs/codex-review/20260604-122110-r465-last-message.md` (result)
