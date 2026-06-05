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

# Experiment 592: search ActiveSearch (active-area searcher)

## Description

This experiment ports `ActiveSearch` from upstream `terminal/search/active.zig`
— the first higher-level searcher, which drives a `SlidingWindow` over the
**active area** of a `PageList` (the only mutable region, re-searched as content
changes). It is the smallest of the four searchers. It creates a new
`terminal::search::active` module and adds the `PageList` / `SlidingWindow`
accessors it needs.

## Upstream behavior

```zig
pub const ActiveSearch = struct {
    window: SlidingWindow,

    pub fn init(alloc, needle) !ActiveSearch {
        // Forward search (the active area is small).
        return .{ .window = try .init(alloc, .forward, needle) };
    }
    pub fn deinit(self) void { self.window.deinit(); }

    /// Copy the active area (+ overlap) into the window; returns the oldest page covered.
    pub fn update(self, list: *const PageList) !?*PageList.List.Node {
        self.window.clearAndRetainCapacity();

        // 1. Append pages (back to front) until the active `rows` are covered.
        var rem = list.rows;
        var node_ = list.pages.last;
        var last_node = null;
        while (node_) |node| : (node_ = node.prev) {
            _ = try self.window.append(node);
            last_node = node;
            if (rem <= node.data.size.rows) { node_ = node.prev; break; }
            rem -= node.data.size.rows;
        }

        // 2. Append overlap pages (older) until `needle.len - 1` bytes are covered, stopping at the
        //    first page whose last row isn't soft-wrapped (no overlap possible).
        while (node_) |node| : (node_ = node.prev) {
            const row = node.data.getRow(node.data.size.rows - 1);
            if (!row.wrap) break;
            const added = try self.window.append(node);
            if (added >= self.window.needle.len - 1) break;
        }

        return last_node;
    }

    pub fn next(self) ?FlattenedHighlight { return self.window.next(); }
};
```

`update` does not search — it only copies the active area's text (and a small
overlap into history) into the window, so the caller can hold the `PageList`
lock briefly. It returns the **oldest page covering the active area** (for the
history searcher to dedup against), or `null` if the active area covers the
whole list. The overlap pass (step 2) walks older pages only while the boundary
is soft-wrapped, adding up to `needle.len - 1` bytes so a match straddling the
active/history boundary is still found.

## Rust mapping (`roastty/src/terminal/search/active.rs`, new module)

Upstream's intrusive `pages.last` / `node.prev` reverse walk becomes reverse
index iteration over `PageList::node_ptrs_front_to_back()` (a front-to-back
`Vec<NonNull<Node>>`). The two passes share the running index `i`, exactly as
upstream shares `node_` across its two `while` loops. `update` is `unsafe` — it
derives node pointers from the borrowed `list` and stores them in the window for
later use (the same contract as `SlidingWindow::append`). Rust's `Drop` subsumes
`deinit`.

```rust
//! The active-area searcher (port of upstream `terminal/search/active.zig`). Drives a
//! `SlidingWindow` over a `PageList`'s mutable active area, re-copied on each `update`.

use std::ptr::NonNull;

use super::super::highlight::Flattened;
use super::super::page_list::{Node, PageList};
use super::sliding_window::{Direction, SlidingWindow};

/// Searches for a substring within the active area of a `PageList` (upstream `ActiveSearch`).
pub(crate) struct ActiveSearch {
    window: SlidingWindow,
}

impl ActiveSearch {
    /// Create a searcher for `needle` (upstream `init`). A forward window — the active area is
    /// small, so reversing is not worth it.
    pub(crate) fn new(needle: &[u8]) -> ActiveSearch {
        ActiveSearch {
            window: SlidingWindow::new(Direction::Forward, needle),
        }
    }

    /// Copy the active area (plus a small history overlap) into the window (upstream `update`).
    /// Does not search. Returns the oldest page covering the active area (for the history searcher
    /// to dedup against), or `None` if the active area covers the whole list.
    ///
    /// # Safety
    /// The window stores node pointers derived from `list`; the caller must keep `list`'s pages
    /// valid (not reallocated/freed) until the search results are consumed or the window is cleared
    /// (the same contract as `SlidingWindow::append`).
    pub(in crate::terminal) unsafe fn update(&mut self, list: &PageList) -> Option<NonNull<Node>> {
        self.window.clear_and_retain_capacity();

        let nodes = list.node_ptrs_front_to_back();
        let mut rem = list.active_rows() as usize;
        let mut last_node: Option<NonNull<Node>> = None;
        let mut i = nodes.len();
        let mut into_overlap = false;

        // 1. Cover the active area, walking pages back to front.
        while i > 0 {
            i -= 1;
            let node = nodes[i];
            // SAFETY: `nodes` are valid for this call; the caller upholds `update`'s contract for
            // their later use in the window.
            let rows = unsafe { node.as_ref() }.page_rows() as usize;
            unsafe { self.window.append(node) };
            last_node = Some(node);
            if rem <= rows {
                into_overlap = true;
                break;
            }
            rem -= rows;
        }

        // 2. Add overlap pages until `needle.len - 1` bytes are covered or a non-wrapped boundary.
        if into_overlap {
            let needed = self.window.needle_len().saturating_sub(1);
            while i > 0 {
                i -= 1;
                let node = nodes[i];
                // SAFETY: see above.
                if !unsafe { node.as_ref() }.last_row_wrapped() {
                    break;
                }
                let added = unsafe { self.window.append(node) };
                if added >= needed {
                    break;
                }
            }
        }

        last_node
    }

    /// Find the next match in the active area (upstream `next`); `None` when exhausted.
    pub(in crate::terminal) fn next(&mut self) -> Option<Flattened> {
        self.window.next()
    }
}
```

Supporting accessors:

```rust
// page_list.rs
impl PageList {
    /// The active row count (upstream `list.rows`).
    pub(in crate::terminal) fn active_rows(&self) -> CellCountInt {
        self.rows
    }

    /// The page nodes front-to-back as pointers, for the search subsystem to walk (upstream
    /// `pages.first/last` + `node.next/prev`).
    pub(in crate::terminal) fn node_ptrs_front_to_back(&self) -> Vec<NonNull<Node>> {
        self.pages.iter().map(|p| NonNull::from(p.as_ref())).collect()
    }
}

// sliding_window.rs
impl SlidingWindow {
    /// The needle length (upstream `window.needle.len`).
    pub(in crate::terminal) fn needle_len(&self) -> usize {
        self.needle.len()
    }
}
```

## Scope / faithfulness notes

- **Ported**: `ActiveSearch` (`init` → `new`, `update`, `next`) and the
  `PageList::active_rows` / `node_ptrs_front_to_back` and
  `SlidingWindow::needle_len` accessors.
- **Faithful**: the forward window; `update`'s clear-then-cover-active-area
  (appending pages back-to-front until `rem <= node.rows`), then the overlap
  pass (older pages, while soft-wrapped, until `>= needle.len - 1` bytes); the
  returned oldest-covering page; `next` delegating to the window.
- **Faithful adaptation**: the intrusive `pages.last` / `node.prev` walk →
  reverse index iteration over `node_ptrs_front_to_back()`, with the running
  index `i` shared across both passes (as upstream shares `node_`);
  `node.data.size.rows` → `Node::page_rows`; `getRow(rows-1).wrap` →
  `Node::last_row_wrapped`; `needle.len - 1` → `needle_len().saturating_sub(1)`
  (hoisted out of the loop; the needle is non-empty in practice); `deinit`
  subsumed by `Drop`; the allocation-error returns vanish. `update` is `unsafe`
  (it stores list-derived node pointers in the window beyond the borrow — the
  `append` contract).
- **Deferred**: the other searchers (`PageListSearch` / `ScreenSearch` /
  `ViewportSearch`) and the search `Thread`.
- No C ABI/header/ABI-inventory change (internal Rust). Creates the
  `terminal::search::active` module; adds three accessors.

## Changes

1. `roastty/src/terminal/search/active.rs` (new): the module doc comment,
   `ActiveSearch`, `new`, `update`, `next`.
2. `roastty/src/terminal/search/mod.rs`: declare
   `#[allow(dead_code)] pub(crate) mod active;`.
3. `roastty/src/terminal/page_list.rs`: add `PageList::active_rows` and
   `node_ptrs_front_to_back`.
4. `roastty/src/terminal/search/sliding_window.rs`: add
   `SlidingWindow::needle_len`.
5. Tests (in `active.rs`):
   - **simple search**: a `PageList::init(10, 10)` with active rows
     `["Fizz", "Buzz", "Fizz", "Bang"]`; `ActiveSearch::new(b"Fizz")`,
     `update(&list)`; the first `next()` is the `Fizz` on row 0 (`top_x == 0`,
     `bot_x == 3`, chunk `start == 0`), the second is the `Fizz` on row 2
     (`top_x == 0`, `bot_x == 3`, chunk `start == 2`), the third is `None`.
   - **update clears the prior window**: after exhausting `next()`, a second
     `update(&list)` re-finds both matches (the window was cleared and
     refilled).
   - **no match**: `ActiveSearch::new(b"zzzz")` → `update` → `next()` is `None`.
   - **update returns the covering page**: for the single-page list, `update`
     returns `Some(node)` equal to the list's first node pointer.
   - **overlap pass appends a soft-wrapped older page**: a two-page list (via
     `grow_to_two_pages_for_tests`) whose **older** page is given a content cell
     and its last row marked soft-wrapped (a new `#[cfg(test)]`
     `set_first_page_content_and_wrap_for_tests(wrapped)` helper) → after
     `update`, the window holds **two** metas (the active page plus the
     overlapped older page); with `wrapped == false` it holds **one** (the
     overlap pass stops at the non-wrapped boundary). The content cell is
     essential: `grow_to_two_pages_for_tests` makes the older page **blank**,
     and `append` on a blank page (whose wrapped last row yields no trailing
     newline) returns `0` and adds no meta — so without content, `meta_len`
     could not distinguish wrap from no-wrap. Asserted via a `#[cfg(test)]`
     `meta_len` accessor on `SlidingWindow` exposed through a same-module
     `ActiveSearch` test helper. (Guards the shared-index handoff and the
     overlap stop condition — Codex's design-review Optional.)
6. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty terminal::search
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/terminal/search roastty/src/terminal/page_list.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `ActiveSearch` reproduces upstream's behavior (forward window; `update`'s
  active-area cover + overlap pass returning the oldest covering page; `next`
  delegation) — faithful to `terminal/search/active.zig`;
- the tests pass (simple search / update-clears / no-match / covering-page), and
  the existing tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the active-area cover, the overlap pass, the
returned page, or the `next` delegation diverges from upstream, an unrelated
item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed the design and **approved it**, confirming all six questions:
(Q1) the shared running index `i` is faithful — after the active loop appends
`nodes[i]` and breaks, the overlap loop's first `i -= 1` starts at the previous
page (upstream's `node_ = node.prev` handoff), and an exhausted active loop
skips the overlap (matching `node_ = null`); (Q2) `unsafe fn update` is
appropriate (it stores page-node pointers beyond the `PageList` borrow — tying
that lifetime safely would be a larger change than upstream's model); (Q3)
hoisting `needle.len() - 1` is faithful (invariant); (Q4) `last_node` as the
oldest active-covering node from the first loop is correct (the overlap pass
must not update it); (Q5) the visibility avoids `private_interfaces`; (Q6)
allocating `node_ptrs_front_to_back()` per `update` is acceptable. One Optional
and one Nit, both adopted:

- **Optional (adopted)**: add a test exercising the second (overlap) loop, not
  just the active-cover loop — a two-page list whose older page's last row is
  soft-wrapped, guarding the shared-index handoff and the
  `added >= needle.len() - 1` stop. Added (with a `#[cfg(test)]` wrap-setter and
  a `meta_len` accessor).
- **Nit (adopted)**: name the accessor `node_ptrs_front_to_back` to make the
  ordering the reverse walk depends on explicit.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d592-prompt.md`
- Result: `logs/codex-review/20260604-d592-last-message.md`

## Result

**Result:** Pass

A new `terminal::search::active` module landed (`ActiveSearch`), declared
`pub(crate) mod active;` in `search/mod.rs`. `new` builds a forward
`SlidingWindow`; `update` clears it, walks `node_ptrs_front_to_back()` in
reverse to cover the active `rows` (tracking the oldest covering page as
`last_node`), then runs the overlap pass (older pages while their last row is
soft-wrapped, until `>= needle.len() - 1` bytes), returning `last_node`; `next`
delegates to the window. Accessors added: `PageList::active_rows` /
`node_ptrs_front_to_back` and `SlidingWindow::needle_len` (plus `#[cfg(test)]`
`meta_len` and the page content/wrap test helper). `update` is `unsafe` (it
stores list-derived node pointers in the window past the borrow).

One test-construction deviation from the plan, validated by the result review:
the overlap test's helper became `set_first_page_content_and_wrap_for_tests`
(content cell + wrap flag) rather than a wrap-only setter — because
`grow_to_two_pages_for_tests` makes the older page blank, and `append` on a
blank wrapped page is a no-op (returns `0`, no meta), so `meta_len` could not
otherwise distinguish the wrap from the no-wrap case. Codex confirmed this is a
sound, test-only setup that better exercises the overlap pass and does not
change production behavior.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3270 passed, 0 failed (five new tests; no
  regressions, up from 3265).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + terminal/search + page_list.rs
  - lib.rs/header/abi_harness.c) clean; `git diff --check` clean.

The five new tests: a simple active search (two `Fizz` matches on rows 0 and 2,
then `None`); update clearing/refilling the window; a no-match `None`; the
returned covering page; and the overlap pass appending a content-bearing
soft-wrapped older page (`meta_len` 2 with wrap, 1 without).

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no Required
or Optional findings** (one Nit: the doc still named the planned wrap-only
helper and lacked `## Result` / `## Conclusion` — both fixed here). Codex
confirmed the implementation is faithful — a forward `SlidingWindow`,
clear/refill on `update`, the reverse `node_ptrs_front_to_back()` walk with the
shared-index handoff into the overlap pass, `last_node` preserved as the oldest
active-covering page, and `next` delegation — and that the content-cell
overlap-test deviation is sound (a test-only setup that makes `append` produce
data so the wrap-enabled overlap is distinguishable from the non-wrapped stop,
without changing production behavior).

Review artifacts:

- Prompt: `logs/codex-review/20260604-r592-prompt.md` (result)
- Result: `logs/codex-review/20260604-r592-last-message.md` (result)

## Conclusion

This experiment ports `ActiveSearch` — the first higher-level searcher — driving
a `SlidingWindow` over a `PageList`'s mutable active area: `update` copies the
active rows (plus a soft-wrap overlap into history) into the window and returns
the oldest covering page, and `next` delegates to the matcher. The intrusive
`pages.last` / `node.prev` walk became reverse index iteration over a
`node_ptrs_front_to_back()` snapshot with a shared index across the cover and
overlap passes. The remaining search work is the history-spanning searchers —
`PageListSearch` (`search/pagelist.zig`, which drives a reverse window over the
scrollback and dedups against the active area returned here), then
`ScreenSearch` / `ViewportSearch`, and finally the search `Thread`.
