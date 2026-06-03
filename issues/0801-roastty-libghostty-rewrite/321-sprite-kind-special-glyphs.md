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

# Experiment 321: the sprite-kind special glyph dispatch

## Description

The eleven special sprite draw functions (underlines, strikethrough, overline,
the four cursor shapes) are already ported in `draw.rs`, but they are **only
reachable from their own unit tests** — nothing dispatches a codepoint to them.
Upstream reaches them through the `Sprite` enum: a band of synthetic codepoints
at `cp >= Sprite.start` (`maxInt(u21) + 1 = 0x20_0000`, just above the Unicode
maximum `0x10_FFFF`), each mapping to the special function that shares its name.
This experiment ports that band — a `Sprite` enum and a `draw_special` dispatch
wired into `draw_codepoint` — so the special glyphs render through the same
codepoint pipeline (`draw_codepoint` → `has_codepoint` → `render_codepoint`) as
every other sprite.

## Upstream behavior (`sprite.zig`, `sprite/Face.zig` `getDrawFn`)

```zig
pub const Sprite = enum(u32) {
    pub const start: u32 = std.math.maxInt(u21) + 1;  // 0x20_0000
    pub const end: u32 = std.math.maxInt(u32);
    underline = start,
    underline_double,
    underline_dotted,
    underline_dashed,
    underline_curly,
    strikethrough,
    overline,
    cursor_rect,
    cursor_hollow_rect,
    cursor_bar,
    cursor_underline,
};

fn getDrawFn(cp: u32) ?*const DrawFn {
    // Special sprites (cursors, underlines, etc.) are drawn by `Special`
    // functions that share the enum field's name.
    if (cp >= Sprite.start) switch (@as(Sprite, @enumFromInt(cp))) {
        inline else => |sprite| return @field(special, @tagName(sprite)),
    };
    // ...the codepoint ranges (box, braille, …)…
}
```

`getDrawFn` checks the special band **first**: a `cp >= Sprite.start` resolves
to its `special` function before any range is consulted. (The bands cannot
overlap — the ranges are all `<= 0x10_FFFF` and the special band starts at
`0x20_0000`.) Every special function shares the dispatch signature
`fn(cp, canvas, width, height, metrics)`.

## Rust mapping

- `roastty/src/font/sprite/mod.rs` (or `draw.rs`): add
  ```rust
  #[repr(u32)]
  pub(crate) enum Sprite {
      Underline = Sprite::START,
      UnderlineDouble, UnderlineDotted, UnderlineDashed, UnderlineCurly,
      Strikethrough, Overline,
      CursorRect, CursorHollowRect, CursorBar, CursorUnderline,
  }
  ```
  with `pub(crate) const START: u32 = 0x20_0000;` (`= maxInt(u21) + 1`) and a
  `fn from_codepoint(cp: u32) -> Option<Sprite>` matching the eleven exact
  values (returning `None` for any other `cp`, including `cp >= START` past
  `CursorUnderline` — roastty returns `None` rather than reproducing upstream's
  `@enumFromInt` panic on an invalid value, since these codepoints are generated
  internally and an unknown one should be a non-sprite, not a crash).
- `roastty/src/font/sprite/draw.rs`: add
  `fn draw_special(cp: u32, width: u32, height: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool`
  —
  `match Sprite::from_codepoint(cp) { Some(kind) => { <call the matching draw fn>; true } None => false }`.
  Each arm calls the existing `draw_underline`/`…`/`draw_cursor_underline` (all
  take `(canvas, width, height, metrics)`).
- `roastty/src/font/sprite/draw.rs` `draw_codepoint`: prepend the special check
  so it runs **first**, matching upstream's `getDrawFn` order:
  `draw_special(cp, width, h, metrics, canvas) || draw_box_lines(...) || …`. The
  special band uses the (possibly widened) `width` and
  `h = metrics.cell_height`, so cursors honor the wide-glyph factoring
  (Experiment 320). `has_codepoint` and `render_codepoint` inherit the special
  glyphs automatically (both route through `draw_codepoint`).

## Scope / faithfulness notes

- **Ported**: the `Sprite` enum band and the `getDrawFn` special dispatch — the
  eleven special sprite kinds become drawable codepoints through the existing
  pipeline. The sprite-`Face` codepoint→glyph path is now complete (codepoint
  ranges **and** the special band).
- **Deferred**: the resolver/shaper side that _produces_ these synthetic
  codepoints (deciding to draw an underline as sprite `0x20_0000`); a range-only
  `has_codepoint` fast path; and the collection's own sprite coverage.
- One faithful deviation: `from_codepoint` returns `None` for an out-of-range
  `cp >= START` instead of upstream's `@enumFromInt` panic — safer, and
  unreachable in practice (the values are internally generated).
- No C ABI/header/ABI-inventory change (`Sprite`/`Glyph`/`Atlas` are internal
  Rust).

## Changes

1. `roastty/src/font/sprite/mod.rs` (or `draw.rs`): add the `Sprite` enum, the
   `START` constant, and `from_codepoint`.
2. `roastty/src/font/sprite/draw.rs`: add `draw_special`; prepend it to
   `draw_codepoint`.
3. Tests:
   - `draw_special_dispatches`: for each of the eleven kinds
     (`START`..=`START + 10`), `draw_special` returns `true` and inks the canvas
     identically to the direct call (assert per-pixel equality against a fresh
     canvas drawn with the matching `draw_*` function).
   - `draw_special_excludes`: a `cp` in the band past the last kind
     (`START + 50`) and a normal `cp` (`0x2500`, a box codepoint) both return
     `false` from `draw_special` (the box codepoint is **not** a special — it
     must still route through the range families).
   - `from_codepoint_maps_each`: `from_codepoint` returns the right variant for
     each of the eleven values and `None` for `START - 1`, `START + 11`, and
     `0x41`.
   - `draw_codepoint_special`: `draw_codepoint(START, width, &m, canvas)`
     (underline) draws ink (the special band is reachable from the unified
     dispatch); `has_codepoint(START + 7, &m)` (`cursor_rect`) is `true`;
     `has_codepoint(START + 50, &m)` is `false`.
   - `render_codepoint_special`: `render_codepoint(START + 7, &m, None, atlas)`
     (`cursor_rect`, a full-cell rect) returns `Ok(Some(glyph))` with
     `glyph.width > 0 && glyph.height > 0`.
   - `render_codepoint_special_wide`:
     `render_codepoint(START + 7, &m, Some(2), atlas)` (a wide cursor rect)
     trims wider than the `Some(1)` render — proving cursors honor the
     wide-glyph factoring through the special band.
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty sprite
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- the `Sprite` enum + `draw_special` reproduce upstream's `getDrawFn` special
  band (the eleven kinds at `cp >= 0x20_0000`, checked before the ranges), and
  the special glyphs render through `draw_codepoint`/`has_codepoint`/
  `render_codepoint`;
- the dispatch, exclusion, mapping, unified-dispatch, render, and wide-cursor
  tests confirm the band and its non-overlap with the codepoint ranges;
- the resolver/shaper production side, the range-only fast path, and the
  collection coverage stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if a special kind needs metrics the single-cell
path does not thread.

The experiment **fails** if the special band, its ordering, or its non-overlap
with the codepoint ranges diverges from upstream, or any public C API/ABI
changes.

## Design Review

Codex reviewed this design before implementation and found **no Required
findings**. Verified against the vendored upstream: `Sprite.start` is
`maxInt(u21)

- 1 =
  0x20_0000`, and the planned variant order matches upstream exactly (underline, double, dotted, dashed, curly, strikethrough, overline, cursor_rect, cursor_hollow_rect, cursor_bar, cursor_underline), each mapping to the correctly-named special function. It confirmed the band cannot overlap the codepoint ranges (Unicode ends at `0x10_FFFF`; the band starts at `0x20_0000`); that prepending `draw_special`to`draw_codepoint`is faithful to`getDrawFn`(which checks`cp >=
  Sprite.start`before the generated ranges); that returning`None`for an out-of-range special-band value is an acceptable safe deviation from upstream's`@enumFromInt`
  panic (the values are internal synthetic ones, so treating an unknown one as a
  non-sprite avoids importing a crash); and that the test plan covers mapping,
  direct dispatch, invalid-band exclusion, normal- codepoint non-special
  behavior, the unified dispatch, the render path, and the wide-cursor width
  propagation. No Optional findings.

Review artifacts:

- Prompt: `logs/codex-review/20260603-103357-319610-prompt.md`
- Result: `logs/codex-review/20260603-103357-319610-last-message.md`
