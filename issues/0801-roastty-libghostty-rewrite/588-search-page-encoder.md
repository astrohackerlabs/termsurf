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

# Experiment 588: search page encoder (Node plain-text + cell map)

## Description

The search `SlidingWindow.append` (next slice) encodes a page node's text into
the window's data buffer, recording a **cell map** that maps each output byte
back to a page-relative cell coordinate. Upstream does this with
`PageFormatter.init(&node.data, .{ .emit = .plain, .unwrap = true })` plus a
`point_map`. roastty already ports the page formatter inline in `page_list.rs`
(`PlainPageFormat`, `push_codepoint_plain`, etc.), but those types are
module-private to `page_list` and selection-oriented; the search module
(`terminal::search`) cannot drive them.

This experiment exposes the **one focused entry point** the search needs: a
`Node` method that encodes the node's full page as plain, soft-unwrapped text
together with a per-byte cell map. It wraps the existing `PlainPageFormat` — no
new formatting logic — and keeps the formatter's selection-oriented fields
encapsulated. `append` itself (the trailing-newline rule, reverse handling,
buffer growth, integrity assertions) stays in the next slice.

## Upstream behavior (`terminal/search/sliding_window.zig`, within `append`)

```zig
const formatter: PageFormatter = formatter: {
    var formatter: PageFormatter = .init(&meta.node.data, .{
        .emit = .plain,
        .unwrap = true,
    });
    formatter.point_map = .{ .alloc = self.alloc, .map = &meta.cell_map };
    break :formatter formatter;
};
formatter.format(&encoded.writer) catch { return error.OutOfMemory; };
assert(meta.cell_map.items.len == encoded.written().len);
```

`PageFormatter` formats a single page:

- `emit = .plain` — plain text, no escape sequences.
- `unwrap = true` — soft-wrapped rows are joined; hard rows are separated by
  `\n` (no trailing newline — `append` adds that based on the last row's wrap).
- `trim` defaults `true` — trailing whitespace on text rows and trailing blank
  lines are trimmed.
- `point_map` records, for **each output byte**, the page-relative source
  `Coordinate` (so `cell_map.len() == written.len()`; a multi-byte codepoint
  contributes one entry per UTF-8 byte).

## Rust mapping (`roastty/src/terminal/page_list.rs`)

A new `Node` method `search_encode` builds a `PlainPageFormat` over the node's
**full page extent** (`screen_y_base = 0` so the coordinates are page-relative,
`start = (0, 0)`, `end = (size_cols - 1, size_rows - 1)`, `rectangle = false`,
`trim = true`, `unwrap = true`, no `trailing_state`, no `codepoint_map`) and
runs it with a fresh `cell_map`. The existing `push_codepoint_plain` already
pushes `len_utf8()` map entries per codepoint, so the per-byte invariant
(`cell_map.len() == text.len()`) holds.

```rust
impl Node {
    /// Encode this page's full contents as plain, soft-unwrapped text with a per-byte cell map
    /// (upstream `PageFormatter` with `emit: plain, unwrap: true`, plus its `point_map`). Used by
    /// the search subsystem (`SlidingWindow::append`). Each output byte gets one page-relative
    /// source coordinate, so `cell_map.len() == text.len()`. No trailing newline is added — the
    /// caller appends it based on the last row's wrap state.
    pub(in crate::terminal) fn search_encode(&self) -> (String, Vec<point::Coordinate>) {
        let mut text = String::new();
        let mut cell_map = Vec::new();
        let formatter = PlainPageFormat {
            node: self,
            screen_y_base: 0,
            start_x: 0,
            start_y: 0,
            end_x: self.page.size_cols().saturating_sub(1),
            end_y: self.page.size_rows().saturating_sub(1),
            rectangle: false,
            trim: true,
            unwrap: true,
            trailing_state: None,
            codepoint_map: None,
        };
        formatter.format(&mut text, Some(&mut cell_map));
        // Active in all build modes (upstream's inline assert): `append` relies on this contract to
        // map match byte offsets back to cells.
        assert_eq!(cell_map.len(), text.len());
        (text, cell_map)
    }
}
```

## Scope / faithfulness notes

- **Ported**: the search's page-encode step — upstream's
  `PageFormatter{emit: plain, unwrap: true}.format` with a `point_map` →
  `Node::search_encode`.
- **Faithful**: plain emit; `unwrap = true` (soft rows joined, hard rows
  `\n`-separated, no trailing newline); `trim = true` (the `PageFormatter` /
  `Options` default); page-relative coordinates (`screen_y_base = 0`); the
  per-byte cell-map invariant (`cell_map.len() == text.len()`, one entry per
  UTF-8 byte), asserted upstream and re-asserted here.
- **Faithful adaptation**: instead of constructing a public `PageFormatter` (as
  upstream's `append` does), roastty exposes a single encapsulated `Node` method
  over the already-ported `PlainPageFormat` — the page formatter lives inline in
  `page_list`, and a focused method keeps its selection-oriented fields
  (`start`/`end`/`rectangle`/`trailing_state`) hidden from the search module;
  upstream's `Allocator.Error` / writer-error paths vanish (Rust `String` /
  `Vec` are infallible here, aborting on OOM); `screen_y_base = 0` makes the
  coordinates page-relative, matching `PageFormatter`'s single-page coordinates.
- **Deferred**: `SlidingWindow::append` itself (the `Meta` construction, the
  trailing-newline rule from the last row's wrap, the reverse-direction
  reversal, the buffer growth, and `assertIntegrity`) — the next slice.
- No C ABI/header/ABI-inventory change (internal Rust). Adds one
  `pub(in crate::terminal)` method to `Node`.

## Changes

1. `roastty/src/terminal/page_list.rs`: add the `Node::search_encode` method (in
   an `impl Node` block).
2. Tests (in `page_list.rs`):
   - **basic two-row page**: a page whose screen text is `["abc", "de"]` encodes
     to `"abc\nde"` with `cell_map.len() == 6`, the first coordinate `(0, 0)`
     (the `a`), the `d` byte mapping to row `1` (`y == 1`), and the `\n` byte
     mapping to the previous emitted coordinate — the last byte of `c`'s cell,
     `(2, 0)` (the plain formatter maps a pending newline to the prior
     coordinate, not the next row), which matters because search matches can
     span line breaks.
   - **per-byte cell map for multibyte**: a row containing a 2-byte UTF-8
     codepoint (e.g. `"é"`) yields two cell-map entries for that codepoint (so
     `cell_map.len() == text.len()`), both pointing at the same source column.
   - **trailing spaces on a nonblank row trimmed**: a row `"ab  "` (trailing
     spaces) encodes to `"ab"` (`trim = true`), with `cell_map.len() == 2` — a
     deliberate faithfulness check since `trim` affects searchable text.
   - **trailing blanks trimmed**: a page with text only in the first row encodes
     to just that row's text (trailing blank rows trimmed), with the cell-map
     length matching.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty terminal::page_list
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/terminal/page_list.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Node::search_encode` reproduces upstream's search page-encode (plain, unwrap,
  trim, page-relative coordinates, per-byte cell map with
  `cell_map.len() == text.len()`) — faithful to the `PageFormatter` usage in
  `terminal/search/sliding_window.zig`;
- the tests pass (basic two-row / multibyte per-byte map / trailing blanks
  trimmed), and the existing tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the emit / unwrap / trim options, the page-relative
coordinates, or the per-byte cell-map invariant diverge from upstream, an
unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed the design and **approved the API boundary**
(`Node::search_encode` keeps the formatter internals private and exposes exactly
what search needs; `screen_y_base = 0` is correct for page-relative coordinates;
`trim = true` and `unwrap = true` match upstream defaults and `append` behavior;
the full-page bounds match `PageFormatter.init`'s defaults; and
`PlainPageFormat` already provides the per-output-byte point-map behavior search
needs), with one Required, two Optionals, and a Nit — all adopted:

- **Required (adopted)**: use `assert_eq!`, not `debug_assert_eq!`, for the
  `cell_map.len() == text.len()` invariant — upstream's `assert` is active in
  all build modes (its inline assert helper), and `append` relies on this
  contract to map match byte offsets back to cells. Changed to `assert_eq!`.
- **Optional (adopted)**: in the basic two-row test, also assert the newline
  byte's coordinate — the plain formatter maps a pending newline to the previous
  emitted coordinate (the last byte of `c`'s cell, `(2, 0)`), not the next row;
  worth locking down since matches can span line breaks.
- **Optional (adopted)**: add a trailing-spaces-on-a-nonblank-row test, since
  `trim = true` is a deliberate faithfulness choice affecting searchable text.
- **Nit (adopted)**: use `saturating_sub(1)` for `end_x` / `end_y` rather than
  `- 1`, making the nonzero-page assumption explicit and the helper less
  brittle.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d588-prompt.md`
- Result: `logs/codex-review/20260604-d588-last-message.md`
