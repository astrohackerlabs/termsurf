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

# Experiment 585: split tree formatText (textual tree dump)

## Description

This experiment ports `formatText` (and its helper `formatTextInner`) from
upstream `datastruct/split_tree.zig` — the indented, recursive textual dump of a
split tree, used for debugging and tests. It is the simpler of the two
formatters; the ASCII-art `formatDiagram` and the combined `format` stay
deferred. It extends `terminal::split_tree`.

## Upstream behavior

```zig
pub fn formatText(self, writer) !void {
    if (self.nodes.len == 0) { try writer.writeAll("empty"); return; }
    try self.formatTextInner(writer, .root, 0);
}

fn formatTextInner(self, writer, current, depth) !void {
    for (0..depth) |_| try writer.writeAll("  ");                 // two spaces per depth
    if (self.zoomed) |z| if (z == current) try writer.writeAll("(zoomed) ");
    switch (self.nodes[current.idx()]) {
        .leaf => |v| if (@hasDecl(View, "splitTreeLabel"))
            try writer.print("leaf: {s}\n", .{v.splitTreeLabel()})
            else try writer.print("leaf: {d}\n", .{current}),       // the node index
        .split => |s| {
            try writer.print("split (layout: {t}, ratio: {d:.2})\n", .{ s.layout, s.ratio });
            try self.formatTextInner(writer, s.left, depth + 1);
            try self.formatTextInner(writer, s.right, depth + 1);
        },
    }
}
```

An empty tree prints `empty`. Otherwise each node prints on its own line,
indented two spaces per depth: a leaf is `leaf: {index}` (or `leaf: {label}` if
the view type provides `splitTreeLabel`); a split is
`split (layout: {tag}, ratio: {n.nn})` (the layout's tag name, the ratio to two
decimals), followed by its two children one depth deeper. A node equal to
`zoomed` is prefixed with `(zoomed) `.

## Rust mapping (`roastty/src/terminal/split_tree.rs`)

A recursive write into a `String`. The `splitTreeLabel` view-label path is
deferred (it needs a view-label trait); roastty uses the node-index path
(`leaf: {index}`), matching upstream's `else` branch.

```rust
use std::fmt::Write as _;

impl<V> SplitTree<V> {
    /// Write the tree as an indented textual dump (upstream `formatText`). An empty tree writes
    /// `empty`.
    pub(crate) fn format_text(&self, out: &mut String) {
        if self.nodes.is_empty() {
            out.push_str("empty");
            return;
        }
        self.format_text_inner(out, Handle::ROOT, 0);
    }

    fn format_text_inner(&self, out: &mut String, current: Handle, depth: usize) {
        for _ in 0..depth {
            out.push_str("  ");
        }
        if self.zoomed == Some(current) {
            out.push_str("(zoomed) ");
        }
        match &self.nodes[current.idx()] {
            Node::Leaf(_) => {
                let _ = writeln!(out, "leaf: {}", current.idx());
            }
            Node::Split(s) => {
                let s = *s;
                let layout = match s.layout {
                    Layout::Horizontal => "horizontal",
                    Layout::Vertical => "vertical",
                };
                let _ = writeln!(out, "split (layout: {}, ratio: {:.2})", layout, s.ratio.to_f32());
                self.format_text_inner(out, s.left, depth + 1);
                self.format_text_inner(out, s.right, depth + 1);
            }
        }
    }
}
```

## Scope / faithfulness notes

- **Ported**: `formatText` / `formatTextInner` → `SplitTree::format_text` /
  `format_text_inner`.
- **Faithful**: the empty-tree → `empty`; the two-spaces-per-depth indentation;
  the `(zoomed) ` prefix on the zoomed node; the leaf line (`leaf: {index}`);
  the split line (`split (layout: {tag}, ratio: {n.nn})`, with the layout tag
  name and the ratio to two decimals); and the pre-order recursion (node, then
  left, then right one depth deeper) are all reproduced.
- **Faithful adaptation**: writes into a `String` via `std::fmt::Write` rather
  than a Zig writer; `{t}` (the enum tag) becomes the matched `"horizontal"` /
  `"vertical"`; `{d:.2}` becomes Rust's `{:.2}` on `ratio.to_f32()` (the `f16` →
  `f32` widening is lossless, so the two-decimal rounding matches upstream's
  formatting of the `f16` directly); `{d}` on the handle becomes
  `current.idx()`.
- **Deferred**: the `splitTreeLabel` view-label leaf path (needs a view-label
  trait — roastty uses the index path, upstream's `else` branch); the ASCII-art
  `formatDiagram`; and the combined `format` (which runs `formatDiagram` then
  `formatText`). After this, those are the only remaining split_tree pieces.
- No C ABI/header/ABI-inventory change (internal Rust). Extends
  `terminal::split_tree`.

## Changes

1. `roastty/src/terminal/split_tree.rs`: add `use std::fmt::Write as _;`,
   `SplitTree::format_text`, and `format_text_inner`, and update the module doc
   comment to note `format_text` landed (leaving `format_diagram` / `format`
   deferred).
2. Tests (in `split_tree.rs`):
   - **empty**: `format_text` writes `empty`.
   - **single leaf**: writes `leaf: 0\n`.
   - **horizontal split**: a 2-leaf horizontal split (ratio `0.5`) writes
     `split (layout: horizontal, ratio: 0.50)\n  leaf: 1\n  leaf: 2\n`.
   - **vertical split / ratio formatting**: a vertical split with ratio `0.25`
     writes `split (layout: vertical, ratio: 0.25)` (two-decimal formatting).
   - **zoom prefix**: a zoomed leaf line is prefixed with `(zoomed) `; a zoomed
     **split** (root) also renders with the prefix (`(zoomed) split (…)`), since
     it is applied before the leaf/split switch.
   - **nested tree**: deeper indentation (four spaces at depth 2).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty terminal::split_tree
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/terminal/split_tree.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `format_text` reproduces upstream's indented dump (empty → `empty`; per-depth
  indentation; `(zoomed) ` prefix; `leaf: {index}`;
  `split (layout: {tag}, ratio: {n.nn})`; pre-order recursion) — faithful to
  `datastruct/split_tree.zig`;
- the tests pass (empty / single / horizontal / vertical / zoom / nested), and
  the existing tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the indentation, the leaf / split lines, the ratio
formatting, the zoom prefix, or the recursion order diverges from upstream, an
unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed the design and found **no Required findings**, with one Optional
— adopted:

- **Optional (adopted)**: add a zoom-prefix test for a **split** node (not only
  a leaf) — upstream applies `(zoomed) ` before the leaf/split switch, so a
  zoomed root split renders as `(zoomed) split (…)`. Added that case.

Codex confirmed everything else is faithful: `empty` writes exactly `empty`, the
recursion is pre-order, the indentation is two spaces per depth, the leaf output
uses the node-index path, the layout tags are lowercase `horizontal` /
`vertical`, and deferring the view-label trait, the diagram formatter, and the
combined formatter is a clean scope boundary; `ratio.to_f32()` is lossless for
`f16`, and Rust's `{:.2}` is the right two-decimal fixed-formatting analogue for
the ratios under test.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d585-prompt.md`
- Result: `logs/codex-review/20260604-d585-last-message.md`

## Result

**Result:** Pass

`terminal::split_tree` gained `SplitTree::format_text` and `format_text_inner`
(and a `use std::fmt::Write as _;`). `format_text` writes `empty` for an empty
tree; otherwise it recurses pre-order, indenting two spaces per depth, prefixing
`(zoomed) ` on the zoomed node (before the leaf/split switch), printing
`leaf: {index}` for a leaf and `split (layout: {tag}, ratio: {n.nn})` (lowercase
layout, two-decimal ratio) for a split. The module doc comment was updated to
mark `format_text` landed.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3232 passed, 0 failed (six new tests; no regressions,
  up from 3226).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + terminal/split_tree.rs +
  lib.rs/header/abi_harness.c) clean; `git diff --check` clean.

The six new tests (exact-string asserts): empty → `empty`, a single leaf
(`leaf: 0`), a horizontal split (`split (layout: horizontal, ratio: 0.50)` + two
indented leaves), a vertical split's two-decimal `0.25` ratio, the `(zoomed) `
prefix on both a leaf and a root split, and nested indentation (depth-2 leaves
get four spaces).

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no Required
or Optional findings** (one Nit: the `## Result` / `## Conclusion` sections were
not yet in the saved file — added here). Codex confirmed `format_text` matches
upstream's text formatter for the chosen index-label path (empty → `empty`,
two-spaces-per-depth indentation, zoom prefix applied before the leaf/split
rendering, leaves printing the node index, splits printing the lowercase layout
plus the two-decimal ratio, and pre-order left-then-right recursion) and that
the exact-string tests are sound, including nested indentation and zoom on both
a leaf and a split.

Review artifacts:

- Prompt: `logs/codex-review/20260604-r585-prompt.md` (result)
- Result: `logs/codex-review/20260604-r585-last-message.md` (result)

## Conclusion

This experiment ports `formatText` — the thirteenth split_tree slice — the
indented textual dump of a tree (empty, indentation, zoom prefix, leaf index,
split layout + two-decimal ratio, pre-order). The `splitTreeLabel` view-label
leaf path is deferred (it needs a view-label trait; roastty uses the index path,
upstream's `else` branch). The only remaining split_tree piece is the ASCII-art
`format_diagram` (and the combined `format`, which runs the diagram then the
text) — a large self-contained renderer that scales the spatial slots to integer
cells and draws box-drawing borders. The other remaining big-ticket subsystem is
the terminal **search subsystem** (coupled to `PageList` / `Pin` / `Screen` /
`Selection` / `PageFormatter`); the dependency-blocked helpers persist
(regex/oniguruma for `Link::oniRegex`, a URI parser for `os/uri`, the
config-directory naming decision for `file_load` / `edit` / `loadDefaultFiles`).
