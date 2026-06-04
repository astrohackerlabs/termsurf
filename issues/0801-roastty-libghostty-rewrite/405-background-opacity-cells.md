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

# Experiment 405: the background-opacity-cells alpha branch

## Description

`rebuild_bg_row` computes a per-cell background alpha. So far it ports four of
upstream's five `bg_alpha` arms (selected → opaque, inverse → opaque, explicit
background → opaque, otherwise → transparent), but the **third** arm —
`background-opacity-cells` — was deferred (noted in the function's doc comment).
This experiment ports that arm: when the user enables `background-opacity-cells`
and the cell has an explicit background (and is neither selected nor inverse),
its background alpha is the window background opacity applied **per cell**
(`alpha × background_opacity`, truncated) instead of fully opaque. This makes a
cell's own background color translucent when the user wants every cell tinted by
the configured transparency.

## Upstream behavior

In `rebuildCells` (`renderer/generic.zig`), the `bg_alpha` block, in order:

```zig
const bg_alpha: u8 = bg_alpha: {
    const default: u8 = 255;

    // Cells that are selected should be fully opaque.
    if (selected != .false) break :bg_alpha default;

    // Cells that are reversed should be fully opaque.
    if (style.flags.inverse) break :bg_alpha default;

    // If the user requested to have opacity on all cells, apply it.
    if (self.config.background_opacity_cells and bg_style != null) {
        var opacity: f64 = @floatFromInt(default);
        opacity *= self.config.background_opacity;
        break :bg_alpha @intFromFloat(opacity);
    }

    // Cells that have an explicit bg color should be fully opaque.
    if (bg_style != null) break :bg_alpha default;

    // Otherwise, we won't draw the bg for this cell, …
    break :bg_alpha 0;
};
```

So the `background_opacity_cells` arm sits **after** the selected and inverse
arms (which keep those cells opaque) and **before** the plain
explicit-background arm (which it overrides for explicit-background cells).
`background_opacity` is the config's window opacity, clamped to `[0, 1]` at load
(generic.zig:608); the product is truncated to an integer (`@intFromFloat`,
toward zero).

## Rust mapping (`roastty/src/renderer/cell.rs`)

`rebuild_bg_row` currently collapses the selected/inverse/explicit-background
arms into one `alpha` branch:

```rust
let bg_alpha = if selected || cell.style.flags.inverse || has_explicit_bg {
    alpha
} else {
    0
};
```

This experiment expands it to interleave the `background_opacity_cells` arm in
upstream's order:

```rust
let bg_alpha = if selected || cell.style.flags.inverse {
    alpha
} else if background_opacity_cells && has_explicit_bg {
    // Per-cell opacity: the window background opacity applied to this cell's own
    // background. Truncated toward zero (upstream `@intFromFloat`).
    (f64::from(alpha) * background_opacity) as u8
} else if has_explicit_bg {
    alpha
} else {
    0
};
```

When `background_opacity_cells` is `false` this is identical to the current
behavior (`selected || inverse → alpha`; the opacity arm is skipped;
`has_explicit_bg → alpha`; else `0`) — a pure extension. `rebuild_bg_row` gains
two parameters, `background_opacity_cells: bool` and `background_opacity: f64`
(the clamped `[0, 1]` window opacity); `rebuild_viewport` gains the same two and
threads them to `rebuild_bg_row`.

`alpha` is roastty's opaque value (upstream's `default = 255`), so
`f64::from(alpha) * background_opacity` matches upstream's
`255 × background_opacity` when `alpha = 255`; the parameter keeps it general.
The `as u8` float→int cast in Rust saturates and truncates toward zero, matching
`@intFromFloat` for the in-range product.

## Scope / faithfulness notes

- **Ported (bridged)**: the `background-opacity-cells` `bg_alpha` arm — an
  explicit-background, non-selected, non-inverse cell takes
  `alpha × background_opacity` (truncated) when the feature is on.
- **Faithful**: the arm sits in upstream's exact order (after selected/inverse,
  before plain explicit-background); the product is truncated toward zero
  (`@intFromFloat`); the feature only affects cells with an explicit background
  (a default-background cell stays transparent, a selected/inverse cell stays
  opaque). With the feature off, behavior is unchanged.
- **Faithful adaptation**: the per-cell opacity and the feature flag are
  parameters (upstream reads them from `self.config`); roastty's caller will
  supply the clamped `background_opacity` and the `background_opacity_cells`
  flag. The clamping to `[0, 1]` stays the caller's responsibility (upstream
  clamps at config load), as does the live config wiring.
- **Deferred**: the live config wiring (reading `background-opacity` and
  `background-opacity-cells` into the call) and the production
  `rebuild_viewport` caller (still test-only); the Metal upload of `Contents`.
  (Consumed by tests now.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/cell.rs`:
   - `rebuild_bg_row`: add `background_opacity_cells: bool` and
     `background_opacity: f64` params (last two); interleave the
     `background_opacity_cells` arm into the `bg_alpha` computation in
     upstream's order. Update its doc comment (drop the "deferred" note).
   - `rebuild_viewport`: add the same two params (last two) and thread them to
     `rebuild_bg_row`.
   - Update the existing `rebuild_bg_row` / `rebuild_viewport` test call sites
     (`false, 1.0` — feature off, full opacity: unchanged behavior).
2. Tests (in `cell.rs`):
   - `rebuild_bg_row` with `background_opacity_cells = true`,
     `background_opacity = 0.5`, `alpha = 255` over a row with an explicit-bg
     cell, a default-bg cell, a selected explicit-bg cell, and an inverse cell:
     the explicit-bg cell's alpha is `127` (`255 × 0.5` truncated), the
     default-bg cell stays transparent (`0`), the selected cell stays opaque
     (`255`), and the inverse cell stays opaque (`255`);
   - a control with `background_opacity_cells = false` (and any
     `background_opacity`): the explicit-bg cell stays fully opaque (`alpha`),
     proving the feature-off path is unchanged.
   - a **covering-derived** background with `background_opacity_cells = true`: a
     full-block cell (`U+2588`, explicit foreground, `bg_color: None`) — the
     full-block twist makes its resolved background `Some` (the fg color), but
     it has **no explicit background** — stays alpha `0`. This proves the arm
     keys on `has_explicit_bg` (upstream's `bg_style != null`), **not** on the
     resolved background being `Some`; an implementation using
     `colors.bg.is_some()` would wrongly apply opacity here.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty rebuild_bg_row
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `rebuild_bg_row` applies `alpha × background_opacity` (truncated) to an
  explicit-background, non-selected, non-inverse cell when
  `background_opacity_cells` is on, in upstream's arm order, and is unchanged
  when it is off — faithful to upstream's `bg_alpha` block;
- the tests pass (the per-cell opacity alpha; default-bg transparent;
  selected/inverse opaque; the feature-off control), and the existing tests
  still pass (updated for the new signatures, passing `false, 1.0`);
- the live config wiring, the production `rebuild_viewport` caller, and the
  Metal upload stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the opacity arm is mis-ordered (e.g. overriding
selected/inverse, or not overriding plain explicit-background), the product is
rounded instead of truncated, a default-background cell is affected, the
feature-off path changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with one
**Low** finding (no Required), now addressed:

- **Low (addressed):** add a feature-on test for a **covering-derived**
  background with no explicit `bg_style` — e.g. the full-block twist (`U+2588`,
  explicit foreground, `bg_color: None`) with `background_opacity_cells = true`.
  Upstream's opacity arm keys on `bg_style != null`, not on the resolved
  background being `Some`; in roastty terms the branch must use
  `has_explicit_bg`, not `colors.bg.is_some()`. The planned default-bg case
  catches ordinary `None`, but not an implementation that wrongly applies
  opacity to a full-block covering-derived background. That cell must stay alpha
  `0`. The test list now includes this case.

Codex confirmed the rest is faithful: the branch order (after selected/inverse,
before plain explicit-background), the selected/inverse precedence, the
explicit-background-only behavior, truncation via the in-range `as u8` cast
(matching `@intFromFloat`), caller-owned clamping of `background_opacity`, and
the deferred config wiring and production caller.

Review artifacts:

- Prompt: `logs/codex-review/20260604-065742-d405-prompt.md` (design)
- Result: `logs/codex-review/20260604-065742-d405-last-message.md` (design)
