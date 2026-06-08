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

# Experiment 848: Port faint-opacity and background-opacity-cells

## Description

Exp 846's `FrameRenderKnobs::from_config` left two knobs as ghostty-default
placeholder constants because roastty's `Config` lacked the options:
`faint_opacity` (= `128`) and `background_opacity_cells` (= `false`). This
experiment ports both options into `Config` and sources them in `from_config`,
removing the last two placeholders (leaving only `minimum-contrast`, which feeds
the `MetalUniforms` contrast uniform — a separate path, a later slice).

Upstream (`vendor/ghostty/src/config/Config.zig`):

- `@"background-opacity-cells": bool = false` (line 1019).
- `@"faint-opacity": f64 = 0.5` (line 3716), **clamped to `[0, 1]`** in
  `finalize` (line 4708). roastty has no general config `finalize` step, so this
  experiment clamps `faint-opacity` at the **use site** (`from_config`, where it
  converts to the `u8` knob), storing the raw f64 in `Config` — consistent with
  roastty not finalize-clamping other options. (A future `Config::finalize`
  could move the clamp; noted.)

## Changes

`roastty/src/config/mod.rs` and `roastty/src/renderer/frame_renderer.rs`
(production code + tests).

### config/mod.rs — port the two options

Mirroring the existing bool (`background-image-repeat`) and f64
(`background-opacity`) options:

- **Struct fields** (with upstream-key doc comments):
  `pub background_opacity_cells: bool` (right after `background_opacity`);
  `pub faint_opacity: f64` (right after `bold_color`).
- **Defaults:** `background_opacity_cells: false`, `faint_opacity: 0.5`.
- **Parse arms:**
  `"background-opacity-cells" => self.background_opacity_cells = set_bool_field(value, default.background_opacity_cells)?`;
  `"faint-opacity" => self.faint_opacity = set_f64_field(value, default.faint_opacity)?`
  (stored raw; clamped at use).
- **Formatter entries:** `entry_bool` for cells (after `background-opacity`),
  `entry_float` for faint (after `bold-color`).
- **Ordered-keys formatter test:** add the two keys at the formatter positions.
  The exact slot (esp. `faint-opacity`, since `bold-color` is `Option` and
  absent for a default `Config`) is verified by running the keys test and
  matching the emitted order.

### frame_renderer.rs — source them in from_config

- `from_config`: `background_opacity_cells: config.background_opacity_cells`
  (was `false`);
  `faint_opacity: (config.faint_opacity.clamp(0.0, 1.0) * 255.0).ceil() as u8`
  (was `128`). Update the `FrameRenderKnobs` doc comment: only `alpha` /
  `overlay_alpha` remain faithful opaque constants; the two placeholders are
  gone.

## Verification

Per the bounded-run convention (15-min cap, Central-stamped, single tracked
task, no poll-watcher). Fast unit tests:

- **config defaults:** a default `Config` has
  `background_opacity_cells == false`, `faint_opacity == 0.5`.
- **config parse:** `background-opacity-cells true` → true; `faint-opacity 0.25`
  → 0.25; the formatter round-trips both; the ordered-keys test passes with the
  two new keys.
- **from_config sources them:** a `Config` with `background-opacity-cells` set
  and `faint-opacity 0.0` → `from_config` gives
  `background_opacity_cells == true` and `faint_opacity == 0`;
  `faint-opacity 1.0` → `faint_opacity == 255`; **clamp at use:**
  `faint-opacity 2.0` (stored raw) → `from_config` gives `faint_opacity == 255`
  (clamped); the `Config::default()` case still gives `faint_opacity == 128`
  (ceil(0.5×255)) and `background_opacity_cells == false`.
- The 846 `from_config` tests still pass (the defaults are unchanged: 128 /
  false).
- `cargo build -p roastty` — no warnings. `cargo fmt -p roastty -- --check` —
  clean. Full suite via `scripts/bounded-run.sh` (default parallelism) stays
  green. No-ghostty grep on changed lines — clean. `git diff --check` — clean.

**Pass** = the new config + from_config tests pass and the full suite stays
green. **Partial/Fail** = any test fails or the suite regresses.

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: APPROVED, no Required findings.** Verified against
source + upstream: the defaults match (`Config.zig:1019` false / `:3716` 0.5);
the clamp-at-use knob value is **identical** to upstream for all inputs
(upstream clamps in finalize before `generic.zig:623`'s `@ceil(x*255)`; roastty
clamps in the same conversion — 2.0→255 both ways, 0.5→128), and the only
divergence (the raw round-tripped Config value) already exists for
`background-opacity` (its tests store/emit `-0.25`/`1.5` raw), so the rationale
is accurate and disclosed; `entry_optional` writes a void line for `None`, so
`bold-color` **is** emitted in the default keys-test (last key) and
`faint-opacity` after it lands in its own trailing slot (not bold-color's);
`background-opacity-cells` after `background-opacity` inserts cleanly; the
helpers compose (same as the mirrored options); the 846 defaults (128/false) are
preserved; NaN faint-opacity casts to 0 gracefully (no panic); scope is coherent
(`minimum-contrast` is a distinct `MetalUniforms` path, deferred).

- **Nit — doc comment.** The `FrameRenderKnobs` doc still calls
  `faint_opacity`/`background_opacity_cells` placeholders. **Addressed:** the
  change already rewrites it to say only `alpha`/`overlay_alpha` remain
  constants (using "upstream", never the forbidden literal).

## Conclusion

_(to be written after the run)_
