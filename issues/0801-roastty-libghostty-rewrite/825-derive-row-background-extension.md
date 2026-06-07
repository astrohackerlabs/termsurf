+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "default"
reasoning = "medium"

[review.result]
agent = "codex"
model = "default"
reasoning = "medium"
+++

# Experiment 825: Derive Row Background Extension

## Description

Port upstream `renderer/row.zig`'s `neverExtendBg` decision into Roastty's
prepared renderer data path. Experiment 824 can refine Metal `padding_extend`
from prepared per-row `never_extend` booleans, but those booleans are not yet
derived from terminal row contents. Upstream derives them from two kinds of row
state:

- row semantic prompt metadata (`prompt` and `prompt_continuation` never
  extend), and
- each terminal cell's content and resolved background, including explicit
  background-color cells and perfect-fit Powerline glyphs.

Roastty's current `RunOptions`/`RunCell` bridge has most of the data the
renderer needs, but not all of it. It carries codepoints, styles, widths, and
plain-codepoint-vs-other content, but it does not carry row semantic prompt
state or the explicit color stored in terminal background-color cells. This
experiment first exposes that missing prepared metadata, then derives the
`never_extend` row decisions from it. It does not wire those decisions into the
live renderer loop yet.

This experiment does not collect renderer-thread state, mutate `Contents`,
format rows, update Metal uniforms, upload buffers, draw frames, pace redraws,
or add surface lifecycle integration.

## Changes

- `roastty/src/font/run.rs`
  - Add a small renderer-facing row semantic prompt enum, defaulting to `None`,
    with variants matching upstream's relevant row states:
    - `None`,
    - `Prompt`,
    - `PromptContinuation`.
  - Add the semantic prompt field to `RunOptions`.
  - Add a `RunCell` field that preserves explicit terminal background-color cell
    content as a `Color` value (`Palette` or `Rgb`) while leaving ordinary
    codepoint/default cells as `None`.
  - Keep existing `is_codepoint` semantics intact so the shaper and previous row
    formatting paths do not treat background-color cells as text.
- `roastty/src/terminal/page.rs`
  - When decoding `shape_run_cells`, populate the new explicit background field
    for `ContentTag::BgColorPalette` and `ContentTag::BgColorRgb`.
  - Leave codepoint and grapheme cells without explicit background-cell content.
- `roastty/src/terminal/page_list.rs`
  - When assembling each visible row's `RunOptions`, copy the terminal row's
    semantic prompt into the new renderer-facing enum.
- `roastty/src/renderer/cell.rs`
  - Add `row_never_extend_bg` as a faithful value-level port of upstream
    `neverExtendBg` over prepared `RunOptions` data:
    - `Prompt` and `PromptContinuation` rows return true.
    - Explicit background-color cells return true if their resolved background
      equals the default background.
    - Codepoint/grapheme cells return true for perfect-fit Powerline glyphs.
    - Codepoint/grapheme cells return true when their style resolves to no
      background or to the default background.
    - Otherwise the row returns false.
  - Add a helper that derives a `Vec<bool>` row-never-extend vector from a slice
    of `RunOptions`, a palette, and the default background. The vector is
    indexed by viewport row so it can be passed directly to Experiment 824's
    `FramePaddingExtendInput`.
- `issues/0801-roastty-libghostty-rewrite/README.md`
  - Add this experiment to the index as `Designed`.
  - After implementation, update the renderer tracker to say prepared
    `rowNeverExtendBg` decisions can be derived from renderer row data, while
    live renderer-loop wiring remains open.

## Verification

- Inspect:
  - `vendor/ghostty/src/renderer/row.zig` `neverExtendBg`.
  - `vendor/ghostty/src/renderer/generic.zig` `rebuildRow` padding-extension
    branch.
  - `roastty/src/font/run.rs`.
  - `roastty/src/terminal/page.rs`.
  - `roastty/src/terminal/page_list.rs`.
  - `roastty/src/renderer/cell.rs`.
- Run Rust formatting:
  - `cargo fmt -p roastty`
- Run targeted tests:
  - `cargo test -p roastty renderer::cell::tests::row_never_extend -- --nocapture`
  - `cargo test -p roastty terminal::page::tests::shape_run_cells -- --nocapture`
  - `cargo test -p roastty terminal::page_list::tests::shape_run_options -- --nocapture`
  - `cargo test -p roastty font::run -- --nocapture`
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/825-derive-row-background-extension.md`
- Run:
  - `git diff --check`

The experiment passes if Roastty can derive row-level `never_extend` booleans
from prepared row semantic/content/style data with tests covering every upstream
early-return case. It is Partial if the metadata is exposed but one heuristic
needs a follow-up. It fails if faithful derivation requires live renderer
integration before the prepared data path can represent the needed row state.

## Design Review

Codex reviewed the design and approved it for the plan commit with no blockers.
The review confirmed that row semantic prompt state plus explicit
background-color cell content is sufficient to port upstream `neverExtendBg`
without live renderer-loop wiring.

The review also confirmed that the behavior matches upstream for semantic prompt
rows, explicit palette/RGB background-color cells, codepoint/grapheme cells with
no/default/non-default backgrounds, and perfect-fit Powerline glyphs. It noted
that implementation tests should include both true and false cases for
palette/RGB background cells, styled codepoint/grapheme cells, default/no
background cells, semantic prompts, and Powerline/non-Powerline glyphs.
