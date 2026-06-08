+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.result]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"
+++

# Experiment 849: minimum-contrast config + MetalUniforms::from_config

## Description

Exp 846/848 made `FrameRenderKnobs::from_config` fully config-faithful (the
row-format/overlay knobs). The other config-derived half of the renderer is the
**`MetalUniforms`** — `FrameRenderer::new` takes a caller-built `MetalUniforms`,
and `MetalUniforms::new` (`shaders.rs:170`) consumes five config-derived values:
`min_contrast`, `background`, `background_opacity`, `colorspace`, `blending`.

This experiment closes that half: it ports the one missing config option,
`minimum-contrast`, and adds `MetalUniforms::from_config(&Config)` so the caller
builds the uniforms from a `Config` instead of loose literals — mirroring
`FrameRenderKnobs::from_config`.

Type wiring (all verified):

- `min_contrast: f32` ← `config.minimum_contrast` (f64), **clamped to
  `[1, 21]`** at the use site (upstream clamps in `finalize`, `Config.zig:4680`;
  roastty has no finalize, so clamp here — same pattern as Exp 848's
  `faint-opacity`). Upstream default `1.0` (`Config.zig:776`).
- `background: Rgb` ← `config.background.to_terminal_rgb()`
  (`config/mod.rs:1390`).
- `background_opacity: f64` ← `config.background_opacity`.
- `colorspace: WindowColorspace` ← `config.window_colorspace` (the **same** type
  — `shaders.rs` imports `WindowColorspace`/`AlphaBlending` from
  `crate::config`).
- `blending: AlphaBlending` ← `config.alpha_blending`.

## Changes

### config/mod.rs — port minimum-contrast

Mirroring the f64 option `background-opacity` (`set_f64_field` / `entry_float`):

- **Struct field** `pub minimum_contrast: f64` with the upstream-key doc
  comment, placed in upstream-declaration order — upstream `minimum-contrast`
  (776) sits between `selection-background` (708) and `cursor-color` (851), so
  the formatter entry and keys-vec entry go at that slot (verified against the
  exact ordered-keys test; roastty's own cursor/selection ordering is checked
  during implementation and the slot adjusted to match what the test emits).
- **Default** `minimum_contrast: 1.0`.
- **Parse arm**
  `"minimum-contrast" => self.minimum_contrast = set_f64_field(value, default.minimum_contrast)?`
  (stored raw; clamped at use).
- **Formatter entry** `entry_float`, at the matching position.

### shaders.rs — MetalUniforms::from_config

```rust
impl MetalUniforms {
    /// Build the per-frame uniforms from a `Config` (Issue 801, Exp 849).
    /// `min_contrast` is clamped to `[1, 21]` at this use site (roastty has no
    /// config finalize step), matching upstream's finalize clamp.
    pub(crate) fn from_config(config: &Config) -> Self {
        Self::new(
            config.minimum_contrast.clamp(1.0, 21.0) as f32,
            config.background.to_terminal_rgb(),
            config.background_opacity,
            config.window_colorspace,
            config.alpha_blending,
        )
    }
}
```

(`use crate::config::Config;` added to `shaders.rs`.)

## Verification

Per the bounded-run convention (15-min cap, Central-stamped, single tracked
task, no poll-watcher). Fast non-Metal unit tests
(`MetalUniforms::new`/`from_config` need no GPU):

- **config defaults/parse:** a default `Config` has `minimum_contrast == 1.0`;
  `minimum-contrast 5.0` parses to 5.0; the formatter round-trips it; the
  ordered-keys test passes with the new key.
- **`MetalUniforms::from_config` sources the values:** since `MetalUniforms`
  derives `PartialEq`,
  `assert_eq!(MetalUniforms::from_config(&Config::default()), MetalUniforms::new(1.0, default_bg.to_terminal_rgb(), 1.0, WindowColorspace::Srgb, AlphaBlending::Native))`
  — a single exact-equality assertion.
- **clamp at use:** a `Config` with `minimum-contrast 50.0` (stored raw) →
  `from_config().min_contrast == 21.0`; `minimum-contrast 0.0` →
  `min_contrast == 1.0`.
- **config-sourced value flows:** `minimum-contrast 7.0` →
  `from_config().min_contrast == 7.0`.
- `cargo build -p roastty` — no warnings. `cargo fmt -p roastty -- --check` —
  clean. Full suite via `scripts/bounded-run.sh` (default parallelism) stays
  green. No-ghostty grep on changed lines — clean. `git diff --check` — clean.

**Pass** = the new config + `MetalUniforms::from_config` tests pass and the full
suite stays green. **Partial/Fail** = any test fails or the suite regresses.

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: APPROVED, no Required findings.** Independently verified
every load-bearing claim: the types line up and `from_config` compiles (`new`'s
signature at `shaders.rs:170`; `config.background.to_terminal_rgb()` is the
non-optional `Rgb` at `config/mod.rs:1390`, not the `Option` at 1466;
colorspace/blending are the same `crate::config` types imported at
`shaders.rs:6`); the clamp+cast is faithful (`clamp(1,21)` = upstream
`@min(21,@max(1,..))`, default 1.0→1.0); `new` is device-free (no metal-device
guard needed); **`MetalUniforms` derives `PartialEq` (`shaders.rs:124`)**, so
the test is a single
`assert_eq!(from_config(default), new(1.0, default_bg, 1.0, Srgb, Native))`
(defaults `Srgb`/`Native`/`1.0` confirmed); raw-store+clamp-at-use is the
established `background-opacity`/faint pattern; `minimum_contrast` is genuinely
absent; the exact-equality ordered-keys test fully constrains placement. Two
minors, both adopted:

- **Optional — state the slot.** **Fixed:** the design names the upstream
  neighbors (`selection-background` 708 < `minimum-contrast` 776 <
  `cursor-color` 851), with the final slot confirmed against the keys test
  during implementation.
- **Nit — use `assert_eq!`.** **Fixed:** the verification uses a single
  exact-equality assertion (PartialEq).

## Conclusion

_(to be written after the run)_
