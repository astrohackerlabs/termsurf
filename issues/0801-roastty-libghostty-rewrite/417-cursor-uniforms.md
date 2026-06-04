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

# Experiment 417: the cursor uniform group (clear_cursor + update_block_cursor)

## Description

Experiments 415–416 ported the screen-size and font-grid uniform groups. This
experiment ports the **cursor** uniform group from `drawFrame`'s cursor block:
the default clear (`cursor_pos = max` → "no cursor") and the block-cursor set
(`cursor_pos` via the already-ported `block_cursor_pos`, `cursor_wide`, and
`cursor_color`). The cursor-color resolution (`cursor-text` vs the cell
background — already ported as `cursor_text_color`) is the caller's, supplied as
a parameter; the live cursor state (`state.cursor.viewport`, the
style/visibility gating) stays deferred.

## Upstream behavior

In `drawFrame` (`renderer/generic.zig`), the cursor uniforms are cleared by
default, then set for a block cursor:

```zig
// Clear our cursor by default.
self.uniforms.cursor_pos = .{ std.math.maxInt(u16), std.math.maxInt(u16) };
…
if (style == .block) {
    const wide = state.cursor.cell.wide;
    self.uniforms.cursor_pos = .{
        switch (wide) {
            .narrow, .spacer_head, .wide => cursor_vp.x,
            .spacer_tail => cursor_vp.x -| 1,
        },
        @intCast(cursor_vp.y),
    };
    self.uniforms.bools.cursor_wide = switch (wide) {
        .narrow, .spacer_head => false,
        .wide, .spacer_tail => true,
    };
    // uniform_color resolved from cursor-text or the cell bg …
    self.uniforms.cursor_color = .{ uniform_color.r, uniform_color.g, uniform_color.b, 255 };
}
```

The clear sets only `cursor_pos` to the sentinel `(maxInt, maxInt)` (the shader
reads that as "no cursor"). The block branch sets `cursor_pos` (spacer-tail
backstep), `cursor_wide` (true for wide / spacer-tail), and an opaque
`cursor_color`.

## Rust mapping (`roastty/src/renderer/metal/shaders.rs`)

roastty's `block_cursor_pos(x, y, wide) -> ([u16; 2], bool)` already computes
the spacer-tail-adjusted position and the wide flag. The two cursor-uniform
operations are methods on `MetalUniforms`:

```rust
impl MetalUniforms {
    /// Clear the cursor uniform: set `cursor_pos` to the sentinel
    /// `(u16::MAX, u16::MAX)`, which the shader reads as "no cursor" (upstream's
    /// default clear). Only `cursor_pos` is touched.
    pub(crate) fn clear_cursor(&mut self) {
        self.cursor_pos = [u16::MAX, u16::MAX];
    }

    /// Set the block-cursor uniforms (upstream's `style == .block` branch): the
    /// `cursor_pos` (via `block_cursor_pos`, with the spacer-tail backstep), the
    /// `cursor_wide` flag, and the opaque `cursor_color`. `color` is the resolved
    /// cursor color (`cursor-text` vs the cell background — `cursor_text_color`).
    pub(crate) fn update_block_cursor(&mut self, x: u16, y: u16, wide: Wide, color: Rgb) {
        let (pos, cursor_wide) = block_cursor_pos(x, y, wide);
        self.cursor_pos = pos;
        self.bools.cursor_wide = cursor_wide;
        self.cursor_color = [color.r, color.g, color.b, 255];
    }
}
```

`block_cursor_pos` is upstream's switch (spacer-tail → `x - 1` saturating, wide
flag true for wide/spacer-tail); the `cursor_color` is opaque (`alpha = 255`).

## Scope / faithfulness notes

- **Ported (bridged)**: the cursor uniform group — `clear_cursor` (the default
  `cursor_pos` sentinel) and `update_block_cursor` (`cursor_pos` via
  `block_cursor_pos`, `cursor_wide`, opaque `cursor_color`), upstream's
  `drawFrame` cursor block.
- **Faithful**: the clear sets only `cursor_pos = (max, max)`; the block set
  uses `block_cursor_pos` (spacer-tail backstep + wide flag) and an opaque
  `cursor_color` with `alpha = 255`, matching upstream field-for-field.
- **Faithful adaptation**: the resolved cursor `color` is a parameter (upstream
  computes `uniform_color` inline from `cursor-text` / the cell background via
  `cursor_text_color`, already ported); the cursor position `(x, y)` is a
  parameter (upstream reads `cursor_vp` from the live cursor state). `wide` is
  the cell's `Wide`.
- **Deferred**: the live cursor gating (the cursor visibility / style / preedit
  checks, reading `state.cursor.viewport`), the `cursor_text_color` resolution
  at the call site, the non-block cursor styles (the sprite path via
  `add_cursor`/`Contents`, separate from the uniform), and a full production
  `MetalUniforms` constructor. (Consumed by a later slice; this experiment lands
  and tests the cursor uniform group.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/metal/shaders.rs`:
   - add `MetalUniforms::clear_cursor(&mut self)` and
     `MetalUniforms::update_block_cursor(&mut self, x: u16, y: u16, wide: Wide, color: Rgb)`.
     Import `block_cursor_pos` (from `crate::renderer::cell`), `Wide` (from
     `crate::font::run`), and `Rgb` (from `crate::terminal::color`).
2. Tests (in `shaders.rs`):
   - `clear_cursor` sets `cursor_pos` to `[u16::MAX, u16::MAX]` and leaves the
     other cursor fields (`cursor_color`, `bools.cursor_wide`) and unrelated
     fields untouched;
   - `update_block_cursor` with `Wide::Narrow` at `(3, 5)`, color
     `Rgb(10, 20, 30)` → `cursor_pos == [3, 5]`, `cursor_wide == false`,
     `cursor_color == [10, 20, 30, 255]`;
   - `update_block_cursor` with `Wide::SpacerTail` at `(4, 2)` →
     `cursor_pos == [3, 2]` (the spacer-tail backstep) and
     `cursor_wide == true`; other (non-cursor) fields untouched.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty clear_cursor
cargo test -p roastty update_block_cursor
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `clear_cursor` sets only the `cursor_pos` sentinel, and `update_block_cursor`
  sets `cursor_pos` (via `block_cursor_pos`), `cursor_wide`, and the opaque
  `cursor_color` — faithful to upstream's cursor uniform block;
- the tests pass (the clear; the narrow and spacer-tail block cursors; the
  untouched fields), and the existing tests still pass;
- the live cursor gating, the color resolution at the call site, and the
  non-block sprite path stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the clear or block-cursor fields are set wrong (e.g.
the spacer-tail backstep or the wide flag), an unrelated uniform field is
changed, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed the design is faithful to upstream's cursor uniform
block: `clear_cursor` should set only `cursor_pos` to `[u16::MAX, u16::MAX]` —
upstream does not reset `cursor_color` or `cursor_wide`, and the shader's cursor
match is gated by `cursor_pos`, so leaving those fields alone is correct.
`update_block_cursor` is the right shape: using the already-ported
`block_cursor_pos` preserves the spacer-tail backstep and the wide-flag rules,
and `cursor_color = [r, g, b, 255]` matches upstream's opaque uniform color.
Passing the resolved cursor text/background color in as `Rgb` is a clean
boundary; the live cursor visibility/style/preedit checks, the color resolution,
and the non-block sprite path are separate upstream concerns and reasonable to
defer. It judged the planned tests to cover the important cases (the sentinel
clear with untouched fields, the narrow block cursor, the spacer-tail backstep /
wide flag, the opaque color, and the unrelated fields unchanged).

Review artifacts:

- Prompt: `logs/codex-review/20260604-082502-d417-prompt.md` (design)
- Result: `logs/codex-review/20260604-082502-d417-last-message.md` (design)
