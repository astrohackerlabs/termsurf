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

# Experiment 605: search ViewportSearch

## Description

This experiment ports upstream `terminal/search/viewport.zig`'s `ViewportSearch`
into a new `roastty/src/terminal/search/viewport.rs`. `ViewportSearch` searches
only the pages the viewport covers, re-searching only when the viewport actually
moves (detected by a node "fingerprint") or when it overlaps the mutable active
area. It is a thin, forward-only wrapper over the already-ported `SlidingWindow`
matcher (587–591) plus a viewport fingerprint, so it is a self-contained slice
independent of `ScreenSearch`.

All matcher prerequisites are in place. The only new `PageList` surface needed
is two `pub(in crate::terminal)` accessors (the viewport's node list, and the
active area's bottom-right node — the top-left node is already exposed via
`active_area_top_left`).

## Upstream behavior (`viewport.zig`)

`ViewportSearch { window: SlidingWindow, fingerprint: ?Fingerprint, active_dirty: ?bool }`:

- `init(needle)` — a **forward** `SlidingWindow` (the viewport is small, so a
  forward search is instant and avoids reversing); `fingerprint = null`,
  `active_dirty = null`.
- `deinit` — frees the fingerprint and window. In Rust both own their data, so
  `Drop` handles this — no explicit method.
- `reset` — drop the fingerprint and `clearAndRetainCapacity` the window so the
  next `update` always re-searches.
- `needle` — `window.needle` (asserts forward).
- `update(list) -> bool` — build a fresh `Fingerprint` of the current viewport's
  nodes; if it equals the old one, optionally check active-area overlap (gated
  by `active_dirty`): if the old fingerprint contains the active top-left or
  bottom-right node, re-search; otherwise return `false` (no change). On a real
  change (or active overlap), store the new fingerprint, clear the window, then
  rebuild it: a **leading overlap** of soft-wrapped prior nodes (until
  `needle.len - 1` bytes are covered), the fingerprint nodes themselves, and a
  **trailing overlap** of following nodes (same rule). Returns `true`.
- `next() -> ?Flattened` — `window.next()`.
- `Fingerprint { nodes: []*Node }` — `init` collects the viewport's page nodes
  via `getTopLeft(.viewport) .. getBottomRight(.viewport)`; `eql` compares node
  pointers only (cached page contents are unsafe to read; only pointer identity
  is safe).

## New `PageList` accessors (`page_list.rs`, `pub(in crate::terminal)`)

```rust
/// The page nodes the viewport currently covers, front to back (upstream
/// `ViewportSearch.Fingerprint.init`: iterate `getTopLeft(.viewport) .. getBottomRight(.viewport)`).
pub(in crate::terminal) fn viewport_nodes(&self) -> Vec<NonNull<Node>> {
    let top = self.get_top_left(point::Tag::Viewport);
    // Upstream unwraps: the viewport bottom-right "can never fail".
    let bottom = self
        .get_bottom_right(point::Tag::Viewport)
        .expect("viewport bottom-right must exist");
    let mut it = PageIterator {
        list: self,
        row: Some(top),
        limit: Some(bottom),
        direction: Direction::RightDown,
    };
    let mut nodes = Vec::new();
    while let Some(chunk) = it.next() {
        nodes.push(chunk.node);
    }
    assert!(!nodes.is_empty(), "viewport must cover at least one node");
    nodes
}

/// The active area's bottom-right page node (upstream `getBottomRight(.active).?.node`).
pub(in crate::terminal) fn active_area_bottom_right_node(&self) -> Option<NonNull<Node>> {
    self.get_bottom_right(point::Tag::Active).map(|p| p.node())
}
```

(`active_area_top_left().node()` supplies the active top-left node; both already
exist as private `get_top_left` / `get_bottom_right` and the public PageIterator
machinery — these wrappers just expose what the search needs.)

## Rust mapping (`roastty/src/terminal/search/viewport.rs`)

```rust
/// Viewport fingerprint — the page nodes the viewport covers. Only pointer identity is compared
/// (cached page contents may be invalid; cf. upstream `Fingerprint`).
#[derive(Debug, PartialEq)]
struct Fingerprint {
    nodes: Vec<NonNull<Node>>,
}

impl Fingerprint {
    fn new(list: &PageList) -> Fingerprint {
        Fingerprint { nodes: list.viewport_nodes() }
    }
}

/// Searches the viewport of a `PageList`, re-searching only when the viewport moves or overlaps the
/// active area (upstream `ViewportSearch`).
pub(crate) struct ViewportSearch {
    window: SlidingWindow,
    fingerprint: Option<Fingerprint>,
    /// `None` disables active dirty-tracking (always re-search on active overlap); `Some(dirty)`
    /// re-searches the active area only when dirty. Dirty marking is the caller's responsibility.
    active_dirty: Option<bool>,
}

impl ViewportSearch {
    pub(in crate::terminal) fn new(needle: &[u8]) -> ViewportSearch {
        ViewportSearch { window: SlidingWindow::new(Direction::Forward, needle), fingerprint: None, active_dirty: None }
    }

    pub(in crate::terminal) fn reset(&mut self) {
        self.fingerprint = None;
        self.window.clear_and_retain_capacity();
    }

    pub(in crate::terminal) fn needle(&self) -> &[u8] {
        self.window.needle()
    }

    /// Set the active-area dirty-tracking state (upstream writes the `active_dirty` field directly
    /// from the search `Thread`). `None` disables tracking (always re-search on active overlap);
    /// `Some(true)` enables tracking and marks dirty; `Some(false)` enables and marks clean. Both
    /// upstream call sites are `active_dirty = true`, i.e. `set_active_dirty(Some(true))`.
    pub(in crate::terminal) fn set_active_dirty(&mut self, value: Option<bool>) {
        self.active_dirty = value;
    }

    /// Update the sliding window to reflect the current viewport; returns whether a re-search is
    /// needed. `# Safety`: `list` must be safe to read for the whole call.
    pub(in crate::terminal) unsafe fn update(&mut self, list: &PageList) -> bool {
        let fingerprint = Fingerprint::new(list);
        if let Some(old) = self.fingerprint.as_ref() {
            if *old == fingerprint {
                let check_active = match self.active_dirty {
                    None => true,
                    Some(false) => false,
                    Some(true) => { self.active_dirty = Some(false); true }
                };
                let mut overlaps = false;
                if check_active {
                    let tl = list.active_area_top_left().node();
                    let br = list.active_area_bottom_right_node()
                        .expect("active area always has a bottom-right node");
                    for &node in &old.nodes {
                        if node == tl || node == br { overlaps = true; break; }
                    }
                }
                if !overlaps {
                    return false; // unchanged
                }
            }
        }

        // Re-search: store the new fingerprint, unset dirty, rebuild the window.
        let nodes = fingerprint.nodes.clone(); // cheap pointer copies; `fingerprint` moves below
        self.fingerprint = Some(fingerprint);
        if let Some(v) = self.active_dirty.as_mut() { *v = false; }
        self.window.clear_and_retain_capacity();

        let overlap_target = self.window.needle_len().saturating_sub(1);

        // Leading overlap: soft-wrapped prior nodes, until `needle.len - 1` bytes are covered.
        let mut node_opt = list.prev_node_ptr(nodes[0]);
        let mut added = 0usize;
        while let Some(node) = node_opt {
            // SAFETY: `node` is a live page-list node (caller's read contract).
            if !unsafe { node.as_ref() }.last_row_wrapped() { break; }
            added += unsafe { self.window.append(node) };
            if added >= overlap_target { break; }
            node_opt = list.prev_node_ptr(node);
        }

        // The fingerprint nodes themselves.
        for &node in &nodes {
            // SAFETY: as above.
            unsafe { self.window.append(node) };
        }

        // Trailing overlap.
        let end = nodes[nodes.len() - 1];
        // SAFETY: as above.
        if unsafe { end.as_ref() }.last_row_wrapped() {
            let mut node_opt = list.next_node_ptr(end);
            let mut added = 0usize;
            while let Some(node) = node_opt {
                // SAFETY: as above.
                added += unsafe { self.window.append(node) };
                if added >= overlap_target { break; }
                if !unsafe { node.as_ref() }.last_row_wrapped() { break; }
                node_opt = list.next_node_ptr(node);
            }
        }

        true
    }

    pub(in crate::terminal) fn next(&mut self) -> Option<Flattened> {
        self.window.next()
    }
}
```

### Notes / deviations

- **No explicit `deinit`.** Upstream frees the fingerprint slice and the window;
  in Rust the `Vec`/`SlidingWindow` `Drop` impls handle that, so no method is
  needed (consistent with the other ported searchers).
- **`overlap_target = needle_len.saturating_sub(1)`** guards the upstream
  `needle.len - 1` underflow for an empty needle (a degenerate case; the matcher
  never produces matches then anyway). For `needle.len == 1` the target is `0`,
  so the first wrapped overlap node is appended then the loop breaks — faithful
  to upstream's `added >= 0`.
- **`active_br` `expect`** mirrors upstream's `getBottomRight(.active).?` unwrap
  (the active area always has a bottom-right node).
- **`update` is `unsafe`** (it dereferences page nodes for wrap checks); `next`
  / `needle` / `reset` / `new` are safe.
- Registered in `search/mod.rs` as
  `#[allow(dead_code)] pub(crate) mod viewport;`.

## Verification

- `cargo build -p roastty` — no warnings.
- `cargo test -p roastty` — no regressions; new tests:
  - `update_finds_viewport_matches` — a `PageList` with two `Fizz` rows:
    `new(b"Fizz")`, `update` returns `true`, and `next()` yields the matches.
  - `needle_returns_the_needle`.
  - `reset_forces_research` — after an `update`, `reset` then `update` returns
    `true` again (fingerprint cleared).
  - `update_twice_reresearches_when_viewport_covers_active` — a default viewport
    (which contains the active area) re-searches on every `update` (returns
    `true` both times).
  - `active_dirty_false_suppresses_reresearch` — with
    `set_active_dirty(Some(false))`, a second `update` on an unchanged
    viewport-covers-active screen returns `false` (the dirty gate skips the
    active-overlap check).
  - `active_dirty_true_reresearches_then_clears` — with
    `set_active_dirty(Some(true))`, the second `update` returns `true` and a
    third returns `false` (the gate re-searched once and reset to
    `Some(false)`).

  (An unchanged non-active viewport returning `false` needs a scrolled-up
  viewport over scrollback; the `Some(false)` test exercises the same
  `return false` path without that setup, so the scrolled-viewport case is left
  as a follow-up.)

- `cargo fmt -p roastty -- --check` — clean.
- no-ghostty grep on touched source — clean.
- `git diff --check` — clean.

Pass = `ViewportSearch::update` builds a window over the viewport (with wrap
overlap), `next` yields the matches, the fingerprint suppresses redundant
re-search except across the mutable active area, and `reset` forces a re-search.

## Design Review

Codex reviewed the design and raised **two Required** findings, both adopted:

- **Required (adopted)**: `active_dirty` needs a callable API — upstream's
  search `Thread` writes `vp.active_dirty = true` directly (Thread.zig:535,
  679), which the future Rust thread module can't do against a private field.
  Added
  `pub(in crate::terminal) fn set_active_dirty(&mut self, value: Option<bool>)`,
  preserving the tri-state; both upstream writes map to
  `set_active_dirty(Some(true))`.
- **Required (adopted)**: `viewport_nodes` must not return an empty `Vec` when
  the viewport bottom-right is `None` — upstream unwraps because it "can never
  fail", and `update` later indexes `nodes[0]`. Changed to
  `expect("viewport bottom-right must exist")` plus an
  `assert!(!nodes.is_empty(), ...)` before returning.
- **Optional (adopted)**: dirty-tracking tests — `Some(false)` suppresses the
  active-overlap re-search; `Some(true)` re-searches once then resets to
  `Some(false)`. Added.
- **Optional (deferred)**: an unchanged non-active viewport returning `false`
  needs a scrolled-up viewport over scrollback; the `Some(false)` test covers
  the same `return false` path, so the scrolled case is a follow-up.

Codex confirmed the rest is sound: forward window, fingerprint pointer
comparison, active-overlap fall-through, leading/trailing overlap asymmetry,
`needle_len().saturating_sub(1)`, owned `Drop` instead of explicit `deinit`, and
`unsafe update` under the PageList-read contract all map cleanly to upstream.

Review artifacts:

- Prompt: `logs/codex-review/20260605-d605-prompt.md`
- Result: `logs/codex-review/20260605-d605-last-message.md`

## Result

**Result:** Pass

Implemented `ViewportSearch` in the new
`roastty/src/terminal/search/viewport.rs`, plus the two `PageList` accessors
(`viewport_nodes`, `active_area_bottom_right_node`) and the `search/mod.rs`
registration. The port faithfully mirrors upstream `viewport.zig`: the forward
`SlidingWindow`, the `Fingerprint` (node-pointer identity only), the
`active_dirty` gate (`None` → always check; `Some(false)` → skip; `Some(true)` →
check and reset to `Some(false)`), the equal-fingerprint-but-overlaps-active
fall-through, the rebuild order (leading soft-wrap overlap → viewport nodes →
trailing soft-wrap overlap with the break-before/append-then asymmetry), and
`overlap_target = needle_len().saturating_sub(1)`. No explicit `deinit` — the
window and fingerprint `Vec` drop themselves. The Required design fixes are in:
the `set_active_dirty(Option<bool>)` API (so the future search `Thread` can mark
the active area dirty) and `viewport_nodes`'s `expect` + non-empty assert.

Seven tests cover the needle accessor, matching / no-match, the
viewport-covers-active re-search, `reset`, and both dirty-tracking states.
Gates: `cargo fmt --check` clean, `cargo build -p roastty` no warnings,
`cargo test -p roastty` **3317 passed / 0 failed** (3310 → 3317, +7), no-ghostty
grep clean, `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **APPROVED** it with **no Required
and no Optional findings**, confirming: `update` matches upstream (fingerprint
equality, active-dirty gate, active TL/BR overlap fall-through, `false` only for
unchanged/non-overlapping); the `set_active_dirty` API and the re-search
reset-to-clean match upstream's `if (active_dirty) |*v| v.* = false`; the
rebuild order and leading/trailing asymmetry are preserved; `viewport_nodes`
preserves the bottom-right invariant with `expect` + non-empty assert;
`unsafe fn update` is scoped appropriately; and the two accessors are minimal.
The lone Nit (record `## Result` / `## Conclusion`) is addressed here. The
deferred scrolled non-active viewport test is noted as a coverage follow-up.

Review artifacts:

- Prompt: `logs/codex-review/20260605-r605-prompt.md`
- Result: `logs/codex-review/20260605-r605-last-message.md`

## Conclusion

`ViewportSearch` is complete, leaving the search subsystem with a single
remaining piece: the search `Thread` (`terminal/search/Thread.zig`) — the
background driver that owns a `ScreenSearch` and a `ViewportSearch`, pumps
`tick` / `feed` off the render thread, and routes viewport vs. full-screen
results. That `Thread` is the natural next experiment; it depends on roastty's
threading/IPC conventions, so the next step is to study `Thread.zig` and map its
event loop / channel model onto roastty before slicing it.
