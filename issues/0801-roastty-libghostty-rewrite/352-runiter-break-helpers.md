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

# Experiment 352: the run iterator's break-and-style helpers

## Description

The `RunIterator.next()` loop (upstream `shaper/run.zig`) groups a terminal
row's cells into runs, breaking on a handful of conditions and deriving the font
style and presentation per cell. Several of those decisions are **pure
functions** of already-extracted values (style flags, codepoints) — independent
of the terminal `Cell` iteration. This experiment ports those three
font-internal helpers into a new `font/run.rs` module (the future home of the
`RunIterator`): the bold/italic-flags → `Style` mapping, the "bad ligature"
run-split check, and the grapheme presentation derivation. The cell-walking
`next()` loop, the selection/cursor/spacer breaks, and `comparableStyle` (which
needs `terminal::Style`) stay deferred.

## Upstream behavior (`shaper/run.zig` `next()`)

```zig
// Font style from the cell's style flags.
const font_style: font.Style = style: {
    if (style.flags.bold) {
        if (style.flags.italic) break :style .bold_italic;
        break :style .bold;
    }
    if (style.flags.italic) break :style .italic;
    break :style .regular;
};

// "Bad ligature" split: break a run between plain codepoints that would form a
// commonly-undesirable ligature (fl, fi, st).
if (prev_cell.content_tag == .codepoint and cell.content_tag == .codepoint) {
    switch (prev_cp) {
        'f' => { if (cp == 'l' or cp == 'i') break; },
        's' => { if (cp == 't') break; },
        else => {},
    }
}

// Presentation from the grapheme's first codepoint (only when the cell has a
// grapheme; a non-grapheme defers to the font grid's emoji default).
const presentation: ?font.Presentation = if (cell.hasGrapheme()) p: {
    const cps = graphemes[j];
    if (cps[0] == 0xFE0E) break :p .text;
    if (cps[0] == 0xFE0F) break :p .emoji;
    break :p null;
} else null;
```

## Rust mapping (`roastty/src/font/run.rs`, new)

```rust
//! The run iterator — grouping a terminal row's cells into shaping runs.

use crate::font::{Presentation, Style};

/// The font [`Style`] for a cell's bold/italic flags. Faithful port of upstream
/// `next()`'s `font_style` derivation.
pub(crate) fn font_style(bold: bool, italic: bool) -> Style {
    match (bold, italic) {
        (true, true) => Style::BoldItalic,
        (true, false) => Style::Bold,
        (false, true) => Style::Italic,
        (false, false) => Style::Regular,
    }
}

/// Whether a run should split between two adjacent plain codepoints to avoid a
/// commonly-undesirable ligature (`fl`, `fi`, `st`). Faithful port of upstream
/// `next()`'s bad-ligature break.
pub(crate) fn is_bad_ligature_break(prev_cp: u32, cp: u32) -> bool {
    // `const` bindings so the match arms read as the ASCII letters (a cast
    // expression like `b'f' as u32` is not a valid match pattern).
    const F: u32 = b'f' as u32;
    const L: u32 = b'l' as u32;
    const I: u32 = b'i' as u32;
    const S: u32 = b's' as u32;
    const T: u32 = b't' as u32;
    match prev_cp {
        F => cp == L || cp == I,
        S => cp == T,
        _ => false,
    }
}

/// The explicit presentation a grapheme's first codepoint forces, or `None`.
/// A variation selector `U+FE0E` forces text, `U+FE0F` forces emoji; any other
/// first codepoint leaves the presentation to the font grid's default. Faithful
/// port of upstream `next()`'s grapheme presentation derivation.
pub(crate) fn presentation_for_grapheme(first_cp: u32) -> Option<Presentation> {
    match first_cp {
        0xFE0E => Some(Presentation::Text),
        0xFE0F => Some(Presentation::Emoji),
        _ => None,
    }
}
```

- `roastty/src/font/mod.rs`: add `pub(crate) mod run;`.

## Scope / faithfulness notes

- **Ported**: the three pure decision helpers of `RunIterator.next()` — the
  bold/italic → `Style` mapping, the `fl`/`fi`/`st` bad-ligature split, and the
  `FE0E`/`FE0F` grapheme presentation.
- **Faithful**: the `Style` mapping matches upstream's nested-`if` order; the
  ligature set is exactly `f`→`l`/`i` and `s`→`t`; the presentation selectors
  are exactly `FE0E` (text) / `FE0F` (emoji), all else `None`.
- **Deferred** (to the cell-walking `next()`): extracting the style flags /
  codepoints / graphemes from a terminal `Cell`; `comparableStyle` (needs
  `terminal::Style`); the selection/cursor/spacer break conditions; the
  `content_tag == .codepoint` guard around the bad-ligature check (the caller
  applies it before calling `is_bad_ligature_break`); and the `TextRun`/run
  hash. (Consumed by tests now; `#![allow(dead_code)]` covers the not-yet-wired
  path.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/font/run.rs` (new): add `font_style`, `is_bad_ligature_break`,
   `presentation_for_grapheme`.
2. `roastty/src/font/mod.rs`: register `pub(crate) mod run;`.
3. Tests (in `run.rs`):
   - `font_style_combinations`: `(false,false)`→`Regular`,
     `(true,false)`→`Bold`, `(false,true)`→`Italic`, `(true,true)`→`BoldItalic`.
   - `bad_ligature_breaks`: `('f','l')`/`('f','i')`/`('s','t')` → `true`;
     `('f','x')`/`('s','x')`/`('a','b')`/`('l','f')` → `false`.
   - `presentation_for_grapheme_selectors`: `0xFE0E`→`Text`, `0xFE0F`→`Emoji`,
     `'a'`→`None`.
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty font_style
cargo test -p roastty ligature
cargo test -p roastty presentation_for_grapheme
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `font_style`, `is_bad_ligature_break`, and `presentation_for_grapheme`
  reproduce the corresponding `next()` derivations exactly;
- the three tests pass, and the existing tests still pass;
- the cell-walking `next()`, the selection/cursor/spacer breaks,
  `comparableStyle`, and the `TextRun` stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if any helper diverges from upstream (wrong style
order, wrong ligature set, wrong selectors), or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **one Required
finding**, now fixed:

- **Required (fixed):** the draft's `is_bad_ligature_break` used cast
  expressions as match patterns (`b'f' as u32 => …`), which Rust does not
  accept. Changed to `const` bindings (`const F: u32 = b'f' as u32; …`) used as
  the match patterns — valid and still readable as the ASCII letters.

Codex confirmed the rest with no semantic concerns: `font_style` is an exact
port of the nested bold/italic precedence (bold-with-italic → `BoldItalic`);
`is_bad_ligature_break` is correctly directional (`prev → cur`) and limited to
`fl`/`fi`/`st`; `presentation_for_grapheme` maps only `FE0E` → `Text` and `FE0F`
→ `Emoji`; deferring the `content_tag == .codepoint` guard to the caller is a
clean split (the helper is only the codepoint-pair decision); and a new
`font/run.rs` module is the right home for these pure run-iterator helpers, with
the cell walking and `TextRun` still deferred.

Review artifacts:

- Prompt: `logs/codex-review/20260603-145612-857941-prompt.md` (design)
- Result: `logs/codex-review/20260603-145612-857941-last-message.md` (design)
