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

# Experiment 436: the background-image placement config enums (BackgroundImageFit, BackgroundImagePosition)

## Description

The config layer (`roastty/src/config/mod.rs`) holds the leaf config enums the
renderer consumes (`WindowColorspace`, `AlphaBlending`, `WindowPaddingColor`,
`BackgroundBlur`). This experiment adds the **background-image placement** pair
the renderer reads: `BackgroundImageFit` (how the image is scaled) and
`BackgroundImagePosition` (where it is anchored). Both are plain enums — the
upstream `Config` field defaults (`.contain`, `.center`) live on the deferred
`Config` struct, not the enums, matching the existing config-enum pattern.

## Upstream behavior

In `config/Config.zig`, the two enums and their `Config` field defaults:

```zig
@"background-image-position": BackgroundImagePosition = .center,
@"background-image-fit": BackgroundImageFit = .contain,

pub const BackgroundImagePosition = enum {
    @"top-left",
    @"top-center",
    @"top-right",
    @"center-left",
    @"center-center",
    @"center-right",
    @"bottom-left",
    @"bottom-center",
    @"bottom-right",
    center,
};

pub const BackgroundImageFit = enum {
    contain,
    cover,
    stretch,
    none,
};
```

`BackgroundImageFit` selects the scaling: `contain` (fit inside, preserve
aspect), `cover` (fill, preserve aspect), `stretch` (fill, ignore aspect),
`none` (no scaling). `BackgroundImagePosition` anchors the image to one of nine
grid positions, plus `center` (the field default). The renderer reads
`config.bg_image_fit` and `config.bg_image_position` (with `bg_image`,
`bg_image_opacity`, and the `bg_image_repeat` bool) to place the background
image.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// How a background image is scaled to the window (upstream
/// `BackgroundImageFit`; the `Config` default is `Contain`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackgroundImageFit {
    /// Scale to fit inside the window, preserving aspect ratio.
    Contain,
    /// Scale to fill the window, preserving aspect ratio (cropping overflow).
    Cover,
    /// Stretch to fill the window, ignoring aspect ratio.
    Stretch,
    /// No scaling; the image is drawn at its native size.
    None,
}

/// Where a background image is anchored in the window (upstream
/// `BackgroundImagePosition`; the `Config` default is `Center`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackgroundImagePosition {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    CenterCenter,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    Center,
}
```

Both are plain enums (upstream has no methods on them); the variant sets match
upstream exactly. The `Config` field defaults (`.contain` / `.center`) are
documented but not encoded here — they belong to the deferred `Config` struct,
consistent with the other config enums in this module.

## Scope / faithfulness notes

- **Ported (bridged)**: the `BackgroundImageFit` and `BackgroundImagePosition`
  config enums (`config/Config.zig`).
- **Faithful**: `BackgroundImageFit` has the four upstream variants (`contain`,
  `cover`, `stretch`, `none`); `BackgroundImagePosition` has the ten upstream
  variants (the nine grid anchors plus `center`). The names map the upstream
  hyphenated tags to Rust `CamelCase` (`top-left` → `TopLeft`, `center-center` →
  `CenterCenter`, `center` → `Center`).
- **Faithful adaptation**: the `Config` field defaults (`.contain` / `.center`)
  are documented in the enum docs but not encoded as a `Default` — the existing
  config enums keep their defaults on the (deferred) `Config` struct fields, not
  the enums, and this slice follows that.
- **Deferred**: the `Config` struct and its field defaults / parsing, the
  `bg_image` / `bg_image_opacity` / `bg_image_repeat` fields, and the renderer's
  background-image placement math that consumes these enums. (Consumed by a
  later slice; this experiment lands the placement enums.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add `pub(crate) enum BackgroundImageFit { Contain, Cover, Stretch, None }`
     and
     `pub(crate) enum BackgroundImagePosition { TopLeft, …, BottomRight, Center }`
     (both derive `Debug, Clone, Copy, PartialEq, Eq`).
2. Tests (in `config/mod.rs`):
   - `BackgroundImageFit`: an array listing **every** variant
     (`[Contain, Cover, Stretch, None]`) with `assert_eq!(fits.len(), 4)` — this
     locks the exact upstream set; plus a representative `assert_ne!`
     (`Contain != None`) and a `Copy`/`Eq` round-trip.
   - `BackgroundImagePosition`: an array listing **every** variant (the nine
     grid anchors plus `Center`) with `assert_eq!(positions.len(), 10)` —
     locking the exact upstream set; plus a representative `assert_ne!`
     (`CenterCenter != Center`) and a `Copy`/`Eq` round-trip.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty background_image
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `BackgroundImageFit` has exactly the four upstream variants and
  `BackgroundImagePosition` exactly the ten upstream variants — faithful to
  `config/Config.zig`;
- the tests pass (the distinct variants for each), and the existing tests still
  pass;
- the `Config` struct, the defaults, and the placement math stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if either enum is missing a variant or has an extra/
misnamed one, a default is wrongly encoded onto the enum, an unrelated item
changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **one
Low finding** (now folded into the tests), no Required or Recommended findings.
It verified the variant sets directly against the vendored upstream:
`BackgroundImagePosition` has the nine hyphenated grid positions plus the
standalone `center` (`Config.zig:9611`) and `BackgroundImageFit` has `contain`,
`cover`, `stretch`, `none` (`Config.zig:9625`); the CamelCase mapping is right
(including `center-center → CenterCenter` and standalone `center → Center`);
keeping the defaults off the enums matches the roastty config pattern (upstream
defaults live on the `Config` fields, `Config.zig:657` / `:687`); and porting
the pair together is appropriately bounded (the renderer consumes them together
for background-image placement).

- **Low (fixed)**: the planned tests were representative-only; for a slice whose
  main requirement is the exact variant set, each test should reference every
  variant. Folded into the tests: each enum's test lists all variants in an
  array with an `assert_eq!(.len(), N)` (4 for `Fit`, 10 for `Position`),
  directly protecting the upstream sets.

Review artifacts:

- Prompt: `logs/codex-review/20260604-100953-d436-prompt.md` (design)
- Result: `logs/codex-review/20260604-100953-d436-last-message.md` (design)

## Result

**Result:** Pass

The background-image placement config enums are now live.

- `roastty/src/config/mod.rs`:
  `pub(crate) enum BackgroundImageFit { Contain, Cover, Stretch, None }`
  (upstream `BackgroundImageFit`) and
  `pub(crate) enum BackgroundImagePosition { TopLeft, TopCenter, TopRight, CenterLeft, CenterCenter, CenterRight, BottomLeft, BottomCenter, BottomRight, Center }`
  (upstream `BackgroundImagePosition`), both deriving
  `Debug, Clone, Copy, PartialEq, Eq`. The `Config` field defaults (`.contain` /
  `.center`) are documented on the enums but kept off them, consistent with the
  existing config enums.

Tests (in `config/mod.rs`):

- `background_image_fit_has_the_four_upstream_variants` — an array of all four
  variants, `assert_eq!(len, 4)`, `assert_ne!(Contain, None)`, `Copy`/`Eq`
  round-trip.
- `background_image_position_has_the_ten_upstream_variants` — an array of all
  ten variants, `assert_eq!(len, 10)`, `assert_ne!(CenterCenter, Center)`,
  `Copy`/`Eq` round-trip.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2922 passed, 0 failed (+2, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates (font + renderer + config +
  `lib.rs`/header/`abi_harness.c`) clean; `git diff --check` clean.

## Conclusion

The config layer now carries the background-image placement pair alongside the
earlier leaf enums. The `Config` struct itself (the field defaults, parsing, and
the `bg_image` / `bg_image_opacity` / `bg_image_repeat` fields) and the
renderer's background-image placement math stay deferred. The config-enum family
remains a clean, gated way to advance the broader rewrite while the larger
renderer slices (the live per-frame call sites, the external-dependency shader
loader, and the `neverExtendBg` terminal-core access) and the other subsystems
stay deferred.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no findings**. It confirmed faithfulness against the vendored upstream:
`BackgroundImageFit::{Contain, Cover, Stretch, None}` matches `Config.zig:9625`;
`BackgroundImagePosition` has the exact nine grid anchors plus standalone
`Center` (`Config.zig:9611`); and the defaults are correctly documented but not
encoded on the enums (upstream keeps them on the `Config` fields,
`Config.zig:657` / `:687`). It judged the Low resolved — both tests reference
every variant and assert the expected counts, plus distinctness and `Copy`/`Eq`.
No public C ABI/header impact; nothing needed to change before the result
commit.

Review artifacts:

- Prompt: `logs/codex-review/20260604-101233-r436-prompt.md` (result)
- Result: `logs/codex-review/20260604-101233-r436-last-message.md` (result)
