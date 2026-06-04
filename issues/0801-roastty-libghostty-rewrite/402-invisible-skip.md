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

# Experiment 402: the invisible (concealed) foreground skip

## Description

A cell with the **invisible** flag (SGR 8, conceal) draws its background but
**no foreground** — no glyph and no decorations. Upstream skips the foreground
with a `continue` after writing the background. roastty's `rebuild_row` does not
yet honor the flag — a concealed cell currently draws its glyph and decorations.
This experiment adds the skip: in the column loop, a concealed cell emits no
underline, overline, glyph, or strikethrough, while the glyph cursor still
**advances** past its glyph(s) (roastty's shaper shapes a concealed cell's
glyph, so the cursor must consume it to stay aligned). The background is
unchanged (`rebuild_bg_row` already draws every cell, matching upstream's
bg-then-skip order).

## Upstream behavior

In `rebuildCells` (`renderer/generic.zig`), after the background cell is written
and before the foreground:

```zig
// If the invisible flag is set on this cell then we don't need to render any
// foreground elements, so we just skip all glyphs with this x coordinate.
//
// NOTE: This behavior matches xterm. … The decision has been made here to match
// xterm's behavior for this.
if (style.flags.invisible) {
    continue;
}
// (then: link underline, overline, glyph(s), strikethrough)
```

So a concealed cell keeps its **background** (already written above) but draws
no foreground — matching xterm (decorations are concealed too, unlike
Alacritty).

## Rust mapping (`roastty/src/renderer/cell.rs`)

In `rebuild_row`'s column loop, a `conceal` guard wraps the foreground emission;
the glyph cursor advances regardless (so a concealed cell's shaped glyph is
consumed, keeping the cursor aligned with later cells):

```rust
let flags = cell.style.flags;
// A concealed cell (SGR 8, invisible) draws no foreground (matching xterm).
let conceal = flags.invisible;

// 1./2. Underline (with the link override) + overline — skipped if concealed.
if !conceal {
    let is_link = link_ranges.iter().any(|&[s, e]| grid_pos[0] >= s && grid_pos[0] <= e);
    let underline = link_underline(is_link, flags.underline);
    if underline != Underline::None {
        let underline_color = …;
        add_underline(contents, grid, grid_pos, underline, underline_color, rgba[3])?;
    }
    if flags.overline {
        add_overline(contents, grid, grid_pos, fg, rgba[3])?;
    }
}

// 3. The glyph(s): always advance the cursor; emit only when not concealed.
while run_i < row_runs.len() && glyph_i >= row_runs[run_i].glyphs.len() { run_i += 1; glyph_i = 0; }
if run_i < row_runs.len() {
    let run = &row_runs[run_i];
    debug_assert!(…);
    let opts = render_options(…);
    let cp = infos[col].codepoint;
    while glyph_i < run.glyphs.len()
        && usize::from(run.run.offset) + usize::from(run.glyphs[glyph_i].x) == col
    {
        if !conceal {
            add_glyph(contents, grid, grid_pos, run.run.font_index, &run.glyphs[glyph_i],
                fg, rgba[3], no_min_contrast(cp), &opts)?;
        }
        glyph_i += 1;
    }
}

// 4. Strikethrough — skipped if concealed.
if !conceal && flags.strikethrough {
    add_strikethrough(contents, grid, grid_pos, fg, rgba[3])?;
}
```

The background pass (`rebuild_bg_row`) is unchanged — a concealed cell's
background is still drawn (upstream writes the bg before the `invisible`
`continue`). The `fg_colors` builder is unchanged.

## Scope / faithfulness notes

- **Ported (bridged)**: the `invisible`/conceal foreground skip in `rebuild_row`
  — a concealed cell draws its background but no underline, overline, glyph, or
  strikethrough.
- **Faithful**: the skip matches upstream's
  `if (style.flags.invisible) continue;` placed after the background and before
  the foreground — the background is drawn, the whole foreground (decorations
  **and** glyph, per xterm) is skipped. The link-underline override is inside
  the skipped block (a concealed link cell draws no underline), as upstream
  (`continue` precedes the link underline).
- **Faithful adaptation**: roastty's column loop advances the glyph cursor even
  for a concealed cell (emitting nothing), because roastty's shaper produces a
  glyph for the concealed cell and the cursor must consume it to stay aligned
  with later columns — upstream's `continue` relies on its own shaper-cursor
  handling; the net per-cell effect (no foreground) is identical, and later
  cells' glyphs land at the correct columns.
- **Deferred**: the under-preedit/under-cursor cell skipping and the
  `rebuild_viewport` cursor/preedit assembly; the Metal upload. (Consumed by
  tests now.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/renderer/cell.rs`: in `rebuild_row`'s column loop, add the
   `conceal = flags.invisible` guard around the underline/overline and
   strikethrough steps, and around the per-glyph `add_glyph` (the glyph cursor
   advances regardless). Update its doc comment.
2. Tests (in `cell.rs`):
   - a **concealed** cell with an underline + overline + strikethrough **and** a
     glyph → `fg_rows` has **no** foreground cell for it (everything skipped);
   - a **cursor-alignment** case: a 2-cell row where cell 0 is concealed (with a
     shaped glyph) and cell 1 is visible (with a shaped glyph) → only cell 1's
     glyph is emitted, at **column 1** (the cursor advanced past the concealed
     glyph; if it had not, the visible cell would emit the concealed glyph at
     column 0);
   - a non-concealed control cell draws its foreground normally (existing
     tests).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty rebuild_row
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `rebuild_row` skips the entire foreground (decorations + glyph) of a concealed
  cell while advancing the glyph cursor — faithful to upstream's `invisible`
  `continue`, with the background unchanged;
- the tests pass (the concealed cell draws no foreground; the cursor stays
  aligned so a later visible cell's glyph lands at the right column), and the
  existing tests still pass;
- the under-preedit/cursor skipping and the Metal upload stay deferred;
  `rebuild_bg_row` is unchanged;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a concealed cell still draws foreground, the glyph
cursor misaligns (a later cell emits the concealed glyph), the background is
wrongly skipped, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed the behavior is faithful — an invisible/concealed cell
keeps its background but skips the **entire** foreground (the link underline
override, the ordinary underline, the overline, the glyphs, and the
strikethrough) — and that keeping `rebuild_bg_row` unchanged is correct because
upstream writes the background before the invisible foreground skip. It judged
the glyph-cursor adaptation sound: since roastty's shaped runs can still contain
glyphs for concealed cells, the cursor must consume the glyphs at that column
while suppressing `add_glyph`, otherwise later visible cells would misalign —
preserving the same rendered result as upstream (no foreground for the concealed
cell, later columns still correct). It judged the tests sufficient (both the
visible behavior and the cursor-alignment failure mode).

Review artifacts:

- Prompt: `logs/codex-review/20260604-062906-721957-prompt.md` (design)
- Result: `logs/codex-review/20260604-062906-721957-last-message.md` (design)

## Result

**Result:** Pass

A concealed cell's foreground is now skipped.

- `roastty/src/renderer/cell.rs`: in `rebuild_row`'s column loop,
  `let conceal = flags.invisible;` wraps the underline (including the
  `is_link`/`link_underline` override) and overline in `if !conceal`, guards the
  per-glyph `add_glyph` with `if !conceal` (the glyph cursor advances
  regardless, consuming the concealed cell's shaped glyph), and guards the
  strikethrough with `if !conceal`. `rebuild_bg_row` is unchanged — a concealed
  cell's background is still drawn (upstream writes the bg before the
  `invisible` skip). The doc comment is updated.

Test (in `cell.rs`): `rebuild_row_skips_concealed_foreground` — a concealed cell
carrying an underline + overline + strikethrough **and** a glyph draws **no**
foreground; and a cursor-alignment case where cell 0 is concealed (shaped glyph)
and cell 1 is a **plain** visible cell (shaped glyph, no decorations) → exactly
one foreground vertex, the glyph at column 1, with its `glyph_pos` matching a
direct render of `'B'` (the exact `render_options` `rebuild_row` used for
column 1) — proving the cursor consumed the concealed `'A'` and emitted `'B'` at
the right column.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2861 passed, 0 failed (+1, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates (font + renderer) clean; `git diff --check` clean.

## Conclusion

A concealed cell (SGR 8, `invisible`) now draws its background but no foreground
— no underline (link override included), overline, glyph, or strikethrough —
matching xterm, with the glyph cursor advancing so later cells stay aligned. The
renderer bridge now honors the `invisible` flag.

The remaining renderer-bridge work: the under-preedit/under-cursor cell skipping
and the `rebuild_viewport` cursor/preedit assembly (which depend on the live
render `State`/`Mouse` threading model, ported separately); and the **Metal
upload** of `Contents`.

## Completion Review

Codex reviewed the completed implementation and result and **approved** after
one **Required** finding was fixed:

- **Required (fixed):** the first cursor-alignment test asserted the foreground
  cells were at column 1 and that some vertex was at `[1, 0]`, but the visible
  cell's own decorations were also at `[1, 0]` — so a broken implementation that
  failed to advance the cursor (and emitted no visible glyph) could still pass.
  The test now uses a **plain** visible second cell (no decorations), asserts
  exactly one foreground vertex, that it is at `[1, 0]`, and that its
  `glyph_pos` matches a direct render of `'B'` with the same `render_options` —
  genuinely proving the concealed `'A'` glyph was consumed and the visible `'B'`
  glyph emitted at the correct column.

The re-review confirmed the fix and that the code path is faithful (concealed
cells skip the underline/link override, overline, glyph emission, and
strikethrough while still advancing the shaped-glyph cursor; `rebuild_bg_row`
unchanged so backgrounds still draw), with no public C ABI/header impact.

Review artifacts:

- Result review (1st):
  `logs/codex-review/20260604-063142-919862-last-message.md`
- Result review (re-review):
  `logs/codex-review/20260604-063343-233315-last-message.md`
