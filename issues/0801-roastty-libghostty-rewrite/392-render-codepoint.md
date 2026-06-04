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

# Experiment 392: the codepoint render path

## Description

The renderer can render a glyph by **index + glyph id**
(`SharedGrid::render_glyph`) and a **sprite** by codepoint, but it cannot yet
render an arbitrary **Unicode codepoint** as a real font glyph — resolving the
codepoint to a face, looking up its glyph id, and rendering. Upstream calls this
`renderCodepoint`, and it is what the **lock cursor** (`0xF023`) and preedit
text use. This experiment ports `renderCodepoint` as
`SharedGrid::render_codepoint` (plus a small `CodepointResolver::glyph_index`
helper), so the lock cursor can be wired next (Experiment 393). This experiment
is the render path itself, unit-tested in the font module.

## Upstream behavior

`SharedGrid.renderCodepoint` (`font/SharedGrid.zig`):

```zig
pub fn renderCodepoint(self, alloc, cp, style, p, opts) !?Render {
    // Get the font that has the codepoint.
    const index = try self.getIndex(alloc, cp, style, p) orelse return null;
    // Get the glyph id within that face.
    const glyph_index = glyph_index: {
        const face = try self.resolver.collection.getFace(index);
        break :glyph_index face.glyphIndex(cp) orelse return null;
    };
    // Render the glyph id.
    return try self.renderGlyph(alloc, index, glyph_index, opts);
}
```

So: resolve the codepoint to a face index; if no font has it, return `null`. Get
the glyph id from that face's cmap; if the face lacks it, return `null`.
Otherwise render the glyph id via `renderGlyph` (the same atlas/cache path
roastty already ports).

## Rust mapping

A `glyph_index` helper on `CodepointResolver` (`font/codepoint_resolver.rs`),
mirroring the existing `get_presentation` (which already does
`self.collection.get_face(index)?`):

```rust
/// The glyph id for codepoint `cp` in the face at `index` (its cmap lookup), or
/// `None` if the face lacks the codepoint. Upstream `face.glyphIndex(cp)`.
pub(crate) fn glyph_index(&self, index: Index, cp: u32) -> Result<Option<u16>, EntryError> {
    let face = self.collection.get_face(index)?;
    Ok(face.glyph_index(cp))
}
```

And `render_codepoint` on `SharedGrid` (`font/shared_grid.rs`):

```rust
/// Render a Unicode codepoint as a real font glyph: resolve `cp` to a face
/// ([`CodepointResolver::get_index`]), look up its glyph id
/// ([`CodepointResolver::glyph_index`]), and render it ([`Self::render_glyph`]).
/// Returns `None` if no font has the codepoint or the resolved face lacks it.
/// Faithful port of upstream `SharedGrid.renderCodepoint` — used for the lock
/// cursor and preedit text.
pub(crate) fn render_codepoint(
    &mut self,
    cp: u32,
    style: Style,
    presentation: Option<Presentation>,
    opts: &RenderOptions,
) -> Result<Option<Render>, ResolverRenderError> {
    let Some(index) = self.resolver.get_index(cp, style, presentation) else {
        return Ok(None);
    };
    let Some(glyph_index) = self.resolver.glyph_index(index, cp)? else {
        return Ok(None);
    };
    Ok(Some(self.render_glyph(index, u32::from(glyph_index), opts)?))
}
```

`Style` is imported into `shared_grid.rs`. The `EntryError` from `glyph_index`
converts to `ResolverRenderError` via the existing `?` conversion (the same one
`render_glyph` uses for `get_presentation`).

## Scope / faithfulness notes

- **Ported (bridged)**: `SharedGrid::render_codepoint` (upstream
  `renderCodepoint`) and the `CodepointResolver::glyph_index` cmap helper — the
  codepoint→face→glyph id→render path.
- **Faithful**: the three steps match upstream exactly — `get_index` (no font →
  `None`), the face's `glyph_index` (face lacks it → `None`), then
  `render_glyph`; `style`/`presentation` thread through to `get_index` as
  upstream's `style`/`p`. The render itself is the already-faithful
  `render_glyph` (atlas selection + cache). `glyph_index` mirrors
  `get_presentation`'s `collection.get_face(index)?` shape.
- **Faithful adaptation**: `render_codepoint` returns `Result<Option<Render>>`
  (upstream `!?Render`); `glyph_index` lives on the resolver (which owns the
  collection) and is called by `SharedGrid`, since roastty's resolver keeps
  `collection` private — the same boundary `get_presentation` uses. Upstream's
  per-call lock is not needed (roastty's grid is `&mut`).
- **Deferred**: wiring `render_codepoint` into the lock cursor's `add_cursor`
  branch (Experiment 393); preedit (not ported); the under-cursor recolor; the
  column-ordered decoration merge + link double-underline; the Metal upload.
  (Consumed by tests now.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/font/codepoint_resolver.rs`: add the `glyph_index(index, cp)`
   helper.
2. `roastty/src/font/shared_grid.rs`: add `render_codepoint`; import `Style`.
3. Tests (in `shared_grid.rs`):
   - `render_codepoint('M', Regular, Some(Text))` over a Menlo grid → `Some`,
     the glyph has nonzero size and `presentation == Text` (the resolve→glyph
     id→render path works);
   - `render_codepoint(0xE000, Regular, Some(Text))` (a PUA codepoint Menlo
     lacks, discovery disabled) → `None` (no font has it).
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty render_codepoint
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `SharedGrid::render_codepoint` resolves a codepoint to a face, looks up its
  glyph id, and renders it (returning `None` when no font has the codepoint or
  the face lacks it) — faithful to upstream's `renderCodepoint`;
- the tests pass (`'M'` renders to a `Some` text glyph; a PUA codepoint Menlo
  lacks returns `None`), and the existing tests still pass;
- the lock-cursor wiring, preedit, and the Metal upload stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the render path is wrong (the codepoint not
resolved, the glyph id not looked up from the face's cmap, a missing codepoint
not returning `None`), or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed the design is faithful to upstream's three-step
`renderCodepoint` path (resolve the face index, look up the face cmap glyph id,
then render through the existing `render_glyph` atlas/cache path, returning
`None` at either missing-codepoint step). It confirmed
`CodepointResolver::glyph_index` is the right boundary (the resolver owns the
private collection and already exposes face-derived helpers like
`get_presentation`), and that the sequential `&mut` / `&` / `&mut` borrow flow
is sound (the resolved `Index` is `Copy` and no borrow is held across calls). It
judged the sprite-index concern acceptable for this scoped path (a real-font
codepoint renderer for lock/preedit glyphs, and upstream also goes through
`collection.getFace(index)` after `getIndex`), and that the tests cover the
successful real-glyph path and the missing-codepoint `None` path.

Review artifacts:

- Prompt: `logs/codex-review/20260603-204801-377579-prompt.md` (design)
- Result: `logs/codex-review/20260603-204801-377579-last-message.md` (design)

## Result

**Result:** Pass

The codepoint render path is now ported.

- `roastty/src/font/codepoint_resolver.rs`:
  `CodepointResolver::glyph_index(index, cp) -> Result<Option<u16>, EntryError>`
  = `self.collection.get_face(index)? .glyph_index(cp)` (mirrors
  `get_presentation`'s `collection.get_face(index)?`).
- `roastty/src/font/shared_grid.rs`:
  `SharedGrid::render_codepoint(cp, style, presentation, opts) -> Result<Option<Render>, ResolverRenderError>`
  — the faithful three-step port: `get_index` (no font → `Ok(None)`),
  `glyph_index` (face lacks it → `Ok(None)`), else
  `render_glyph(index, u32::from(gid), opts)`. `Style` is imported; `EntryError`
  converts to `ResolverRenderError` via the existing `?`. `render_codepoint` is
  `pub(crate)` (no production caller yet — the lock cursor is Experiment 393 —
  but reachable in the library crate, so no dead-code warning).

Tests (in `shared_grid.rs`):

- `render_codepoint_renders_a_present_glyph` — `'M'` over a Menlo grid → `Some`,
  a nonzero text glyph, **and** equal to `render_glyph` of the directly-resolved
  `'M'` cmap glyph id (proving the lookup found the right glyph id, not the
  codepoint).
- `render_codepoint_missing_codepoint_is_none` — `0xE000` (a PUA codepoint Menlo
  lacks, discovery disabled) → `None`.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2852 passed, 0 failed (+2, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates (font + renderer) clean; `git diff --check` clean.

## Conclusion

`SharedGrid::render_codepoint` is the faithful port of upstream's
`renderCodepoint` — a Unicode codepoint resolves to a face, its cmap glyph id is
looked up, and the glyph renders through the existing atlas/cache path,
returning `None` when no font has the codepoint. This is the prerequisite for
the **lock cursor** (which renders `0xF023`): the next experiment wires
`add_cursor`'s `Lock` branch to call `render_codepoint`.

The remaining renderer-bridge work: wiring the lock cursor (Experiment 393 —
call `render_codepoint(0xF023)` in `add_cursor`, falling back to a cleared
cursor when no font has the lock glyph, as roastty embeds no Nerd Font); the
under-cursor text recolor; the column-ordered decoration merge + link
double-underline; and the **Metal upload** of `Contents`.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no findings**. It confirmed the implementation matches the approved design:
`glyph_index` is the direct `get_face(index)?.glyph_index(cp)` helper, and
`SharedGrid::render_codepoint` follows the upstream sequence exactly
(`get_index` missing → `Ok(None)`, the cmap lookup missing → `Ok(None)`,
otherwise render the resolved glyph id through `render_glyph`). It noted the
tests are stronger than the plan (the `'M'` case proves the rendered output
equals the direct render of the face cmap glyph id, and the `0xE000` case covers
the missing-codepoint `None` path with discovery off), and that the changes are
internal Rust only with no public C ABI/header impact. Nothing needed to change
before the result commit.

Review artifacts:

- Result review: `logs/codex-review/20260603-205021-689984-last-message.md`
