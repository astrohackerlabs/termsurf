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

# Experiment 227: Port Renderer Cell Codepoint Classification

## Description

Begin porting upstream `renderer/cell.zig` by landing its **pure
codepoint-classification predicates** into a new `renderer::cell` module. These
classify a codepoint as a covering glyph, a graphics element (box drawing, block
element, legacy computing, Powerline), or a fixed-width space, and decide
whether minimum-contrast adjustment should be skipped. They are leaf helpers
that the later cell-render-data builder (`Contents`) and `constraintWidth` call.

`renderer/cell.zig` is 680 lines and its core `Contents` builder depends on the
shader cell-vertex types and the font API, while `constraintWidth` and
`isSymbol` depend on `terminal.page.Cell` and the generated Unicode symbols
table. Per the risk-based sizing rule those are separate slices. This experiment
ports only the self-contained predicates — pure `u32` range checks with no
font/shader/terminal or table dependency — which makes them predictable and
independently testable.

### Functions to port (all over a `u32` codepoint, mirroring upstream `u21`)

- `is_covering(cp) -> bool`: true only for `U+2588` FULL BLOCK.
- `no_min_contrast(cp) -> bool`: true for graphics elements (delegates to
  `is_graphics_element`).
- `is_graphics_element(cp) -> bool`:
  `is_box_drawing || is_block_element || is_legacy_computing || is_powerline`.
- `is_box_drawing(cp) -> bool`: `0x2500..=0x257F`.
- `is_block_element(cp) -> bool`: `0x2580..=0x259F`.
- `is_legacy_computing(cp) -> bool`: `0x1FB00..=0x1FBFF` or `0x1CC00..=0x1CEBF`
  (the Unicode 16.0 supplement).
- `is_powerline(cp) -> bool`: `0xE0B0..=0xE0D7`.
- `is_space(cp) -> bool`: `0x0020` (SPACE) or `0x2002` (EN SPACE) — kept as a
  fixed-width-forcing predicate.

### Faithfulness and scope notes

- Codepoints use `u32` (the codebase convention for codepoints, e.g.
  `renderer::state::Codepoint.codepoint`), faithfully covering upstream `u21`.
- `is_covering` and `no_min_contrast` are `pub(crate)` (upstream `pub fn`);
  `is_graphics_element`, the per-block helpers, and `is_space` stay private
  (upstream private — `isGraphicsElement` and friends are private, and their
  only callers, `no_min_contrast` now and `constraintWidth` later, live in this
  same module). The currently-caller-less helpers are included now as the
  coherent predicate set and are covered by tests under `#![allow(dead_code)]`.
- Do **not** port `isSymbol` (needs the generated Unicode symbols table),
  `constraintWidth` (needs `terminal.page.Cell` and `isSymbol`), or the
  `Contents` builder / `Key` / `CellType` (need the shader cell-vertex types and
  font API).
- No C ABI, header, or ABI inventory changes; no new dependencies.

## Changes

1. Create `roastty/src/renderer/cell.rs`:
   - Module-level `#![allow(dead_code)]` with a "consumed by later renderer
     slices" comment; "upstream `renderer/cell.zig`" attribution (no literal
     `ghostty` token).
   - Implement the eight functions above with `matches!` range checks mirroring
     upstream exactly. `is_covering` and `no_min_contrast` are `pub(crate)`; the
     rest (including `is_graphics_element`) are private, matching upstream.

2. Wire the module from `roastty/src/renderer/mod.rs` with
   `pub(crate) mod cell;` (kept internal; no public API or ABI).

3. Tests in `renderer/cell.rs` (upstream has no isolated predicate tests — they
   are exercised only through `constraintWidth`, deferred — so these are Roastty
   boundary tests):
   - `is_box_drawing_bounds`: `0x24FF` false, `0x2500`/`0x257F` true, `0x2580`
     false.
   - `is_block_element_bounds`: `0x257F` false, `0x2580`/`0x259F` true, `0x25A0`
     false.
   - `is_legacy_computing_bounds`: `0x1FAFF` false, `0x1FB00`/`0x1FBFF` true,
     `0x1FC00` false; `0x1CBFF` false, `0x1CC00`/`0x1CEBF` true, `0x1CEC0`
     false.
   - `is_powerline_bounds`: `0xE0AF` false, `0xE0B0`/`0xE0D7` true, `0xE0D8`
     false.
   - `is_graphics_element_covers_each_block`: one true sample from each of the
     four ranges, and a non-graphics char (`'a'`) false.
   - `is_covering_only_full_block`: `0x2588` true; both neighbors `0x2587` and
     `0x2589` false (both still inside the block-element range, proving the
     predicate is `U+2588`-only, not a range).
   - `no_min_contrast_matches_graphics`: equals `is_graphics_element` for a
     graphics sample (true) and `'a'` (false).
   - `is_space_fixed_width`: `0x0020` and `0x2002` true; `0x2003` and `'a'`
     false.

4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo test -p roastty renderer::cell
cargo test -p roastty renderer
cargo test -p roastty
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/renderer/cell.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- the eight predicates are implemented with the exact upstream ranges;
- the boundary tests pass (each range's edges and a just-outside value);
- `isSymbol`/`constraintWidth`/`Contents` are not pulled in;
- no C ABI, header, or ABI inventory changes;
- `cargo fmt` accepted and `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if a predicate turns out to need `isSymbol` or
cell context that should be reordered first.

The experiment **fails** if any range diverges from upstream (off-by-one bounds,
missing the legacy-computing supplement range), if deferred scope leaks in, or
if any public API/ABI changes.

## Design Review

Codex reviewed this design before implementation.

Review artifacts:

- Prompt: `logs/codex-review/20260602-072951-620497-prompt.md`
- Result: `logs/codex-review/20260602-072951-620497-last-message.md`

Codex confirmed every range is faithful (covering `0x2588`; box
`0x2500..=0x257F`; block `0x2580..=0x259F`; legacy `0x1FB00..=0x1FBFF` and
`0x1CC00..=0x1CEBF`; powerline `0xE0B0..=0xE0D7`; space `0x0020`/`0x2002`), that
`u32` is the right representation for upstream `u21`, and that including
`is_space` now is acceptable under the predicate-slice framing.

Two real (Low) findings, fixed in the design above before this commit:

1. `is_graphics_element` was `pub(crate)` but upstream `isGraphicsElement` is
   private; its only callers live in this module, so it is now private, matching
   upstream.
2. the `is_covering` test now also checks the just-above neighbor `0x2589`
   (still inside the block range) is false, better proving the `U+2588`-only
   behavior.
