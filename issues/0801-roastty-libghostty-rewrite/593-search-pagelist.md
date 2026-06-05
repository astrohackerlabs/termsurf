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

# Experiment 593: search PageListSearch (history/scrollback searcher)

## Description

This experiment ports `PageListSearch` from upstream
`terminal/search/pagelist.zig` — the history searcher. It drives a **reverse**
`SlidingWindow` backward through a `PageList`'s pages (newest to oldest),
feeding pages on demand. Unlike `ActiveSearch` (which re-copies the mutable
active area each `update`), `PageListSearch` assumes the pages it searches do
not change, and uses a **tracked pin** to keep its position safe across page
pruning. It extends `terminal::search`. It is the most
pointer-and-lifetime-coupled search slice.

## Upstream behavior

```zig
pub const PageListSearch = struct {
    list: *PageList,
    window: SlidingWindow,
    pin: *Pin,             // tracked: moves/garbages if the list prunes pages

    pub fn init(alloc, needle, list: *PageList, start: *Node) !PageListSearch {
        const pin = try list.trackPin(.{ .node = start, .y = start.size.rows-1, .x = start.size.cols-1 });
        var window = try SlidingWindow.init(alloc, .reverse, needle);
        _ = try window.append(start);                    // always feed the start page
        return .{ .list = list, .window = window, .pin = pin };
    }
    pub fn deinit(self) void { self.window.deinit(); self.list.untrackPin(self.pin); }

    pub fn next(self) ?Flattened { return self.window.next(); }   // no lock needed

    pub fn feed(self) !bool {                            // needs the lock
        if (self.pin.garbage) return false;              // position was reused -> end
        var rem = self.window.needle.len;
        var node_ = self.pin.node.prev;                  // walk older pages
        while (node_) |node| : (node_ = node.prev) {
            rem -|= try self.window.append(node);         // saturating subtract
            self.pin.node = node;                         // advance the tracked pin
            if (rem == 0) break;
        }
        return rem < self.window.needle.len;             // true if any data fed
    }
};
```

- `init` tracks a pin at the **last cell** of the start page, builds a reverse
  window, and feeds the start page (the caller already holds the lock).
- `next` delegates to the window (no lock — but the returned pins reference page
  nodes that the caller must validate before final use).
- `feed` is the distinctive part: if the pin went `garbage` (its node was reused
  by pruning), the search is over. Otherwise it walks older pages from
  `pin.node.prev`, appending each (advancing the tracked pin to it) until it has
  fed at least `needle.len` bytes (`rem` saturating-subtracts each append's byte
  count). Returns whether any data was fed.

The tracked pin is the safety mechanism: it lives in the `PageList`'s
tracked-pin storage, so when the list prunes/reuses pages the pin is moved to a
safe location or marked `garbage` — letting `feed` detect that the search
position is gone.

## Rust mapping (`roastty/src/terminal/search/pagelist.rs`, new module)

This is a faithful **pointer-based, unsafe** port — upstream's design is built
on raw `*PageList` / `*Node` / `*Pin` and an explicit "caller holds the lock, no
concurrent access, validate pins before final use" contract, which roastty
mirrors with `NonNull` and `unsafe` methods rather than inventing a borrow-based
lifetime scheme (which would diverge from the upstream model and the search
`Thread`'s usage).

```rust
pub(crate) struct PageListSearch {
    /// The list we're searching (upstream `*PageList`).
    list: NonNull<PageList>,
    /// The reverse sliding window of page contents (upstream `window`).
    window: SlidingWindow,
    /// Tracked pin marking the current search position (upstream `*Pin`); the list moves/garbages
    /// it on pruning.
    pin: NonNull<Pin>,
}

impl PageListSearch {
    /// Initialize a reverse search from `start`, tracking a pin at its last cell and feeding the
    /// start page. `None` if the start pin is invalid. Upstream `init`.
    ///
    /// # Safety
    /// `start` must be a live node of `list`; `list` (and its pages) must outlive this search and
    /// must not be accessed concurrently. The caller holds the `PageList` lock.
    pub(in crate::terminal) unsafe fn new(
        needle: &[u8],
        list: &mut PageList,
        start: NonNull<Node>,
    ) -> Option<PageListSearch> {
        // SAFETY: `start` is a live node (caller's contract).
        let start_ref = unsafe { start.as_ref() };
        let pin = Pin::new(
            start,
            start_ref.page_rows() - 1,
            start_ref.page_cols() - 1,
        );
        let pin = list.track_pin(pin)?;

        let mut window = SlidingWindow::new(Direction::Reverse, needle);
        // SAFETY: `start` is live; the window's stored pointer is valid under this search's contract.
        unsafe { window.append(start) };

        Some(PageListSearch {
            list: NonNull::from(list),
            window,
            pin,
        })
    }

    /// Untrack the pin and drop the window (upstream `deinit`). Explicit (not `Drop`) because it
    /// dereferences the `list` pointer — the caller must call it while `list` is still alive.
    ///
    /// # Safety
    /// `list` (the one passed to `new`) must still be alive and not accessed concurrently. Call this
    /// exactly once, **before** the backing `PageList` is dropped (untracking a pin from a dropped
    /// list is undefined). Do not call it twice as a normal lifecycle operation, even though
    /// `untrack_pin` currently tolerates a missing pin.
    pub(in crate::terminal) unsafe fn deinit(&mut self) {
        // SAFETY: caller's contract — `list` is alive.
        unsafe { self.list.as_mut() }.untrack_pin(self.pin);
    }

    /// The next match in the loaded pages (upstream `next`). `None` means "feed more". Does not
    /// touch the `PageList`, so no lock is needed; but returned pins must be validated before final
    /// use.
    pub(in crate::terminal) fn next(&mut self) -> Option<Flattened> {
        self.window.next()
    }

    /// Feed older pages into the window until at least `needle.len` bytes are added (upstream
    /// `feed`). Returns `false` when there is nothing left to feed (the whole list searched, or the
    /// pin went garbage).
    ///
    /// # Safety
    /// `list` must be alive and locked (no concurrent access); accesses page nodes.
    pub(in crate::terminal) unsafe fn feed(&mut self) -> bool {
        // SAFETY: pin lives in the list's tracked storage; the list is alive (caller's contract).
        let garbage = unsafe { self.pin.as_ref() }.is_garbage();
        if garbage {
            return false;
        }

        let needle_len = self.window.needle_len();
        let mut rem = needle_len;

        // Walk older pages from the page before the pin's current node. Each loop iteration reads
        // the list (`prev_node_ptr`) in a scope that ENDS before the `&mut Pin` write, so no
        // `&PageList` derived from `self.list` is ever live when `&mut Pin` is created.
        // SAFETY: `list` is alive; `pin` is valid (not garbage).
        let current = unsafe { self.pin.as_ref() }.node();
        let mut node = unsafe { self.list.as_ref() }.prev_node_ptr(current);
        while let Some(n) = node {
            // SAFETY: `n` is a live node of `list`.
            let added = unsafe { self.window.append(n) };
            rem = rem.saturating_sub(added);
            // Compute the next page (a `&PageList` read) BEFORE mutating the pin, so the borrows
            // never overlap.
            // SAFETY: `list` is alive.
            let prev = unsafe { self.list.as_ref() }.prev_node_ptr(n);
            // Advance the tracked pin to this node.
            // SAFETY: `pin` points to live tracked storage; no `&PageList` is held here.
            unsafe { self.pin.as_mut() }.set_node(n);
            if rem == 0 {
                break;
            }
            node = prev;
        }

        rem < needle_len
    }
}
```

Supporting accessors:

```rust
// page_list.rs — Pin
impl Pin {
    pub(in crate::terminal) fn node(&self) -> NonNull<Node> { self.node }
    pub(in crate::terminal) fn is_garbage(&self) -> bool { self.garbage }
    pub(in crate::terminal) fn set_node(&mut self, node: NonNull<Node>) { self.node = node; }
}

// page_list.rs — Node
impl Node {
    /// The page's column count (upstream `node.data.size.cols`).
    pub(in crate::terminal) fn page_cols(&self) -> CellCountInt { self.page.size_cols() }
}

// page_list.rs — PageList
impl PageList {
    /// The page node immediately older than `node` (upstream `node.prev`); `None` if `node` is the
    /// oldest or not in this list.
    pub(in crate::terminal) fn prev_node_ptr(&self, node: NonNull<Node>) -> Option<NonNull<Node>> {
        let idx = self.node_index(node)?;
        if idx == 0 {
            return None;
        }
        Some(NonNull::from(self.pages[idx - 1].as_ref()))
    }
}
```

`track_pin` / `untrack_pin` already exist (`pub(super)`, visible to the search
module).

## Key design questions (for review)

1. **Pointer model**: holding `NonNull<PageList>` + `NonNull<Pin>` and making
   the list/pin-touching methods `unsafe` — the faithful mapping of upstream's
   `*PageList` / `*Pin` and its documented lock contract. Is this the right
   roastty model, or should it hold `&mut PageList` (which conflicts with the
   pin aliasing the list's storage) or pass the list to each method?
2. **Pin aliasing**: `feed` mutates the tracked pin via
   `unsafe { self.pin.as_mut() }` while NOT holding any `&PageList`
   simultaneously (the `prev_node_ptr` lookups use short-lived
   `unsafe { self.list.as_ref() }` borrows that end before the `&mut Pin`). Is
   sequencing the `&PageList` reads and the `&mut Pin` writes so they never
   overlap sufficient for soundness under the documented single-lock-holder
   contract?
3. **`deinit` vs `Drop`**: an explicit `unsafe fn deinit` (not `Drop`) because
   untracking dereferences the `list` pointer, and a `Drop` that did so would
   use-after-free if the `list` were dropped first. Acceptable (matches
   upstream's explicit `deinit`), or should `Drop` be attempted?

## Scope / faithfulness notes

- **Ported**: `PageListSearch` (`init` → `new`, `deinit`, `next`, `feed`) and
  the `Pin::node` / `is_garbage` / `set_node`, `Node::page_cols`, and
  `PageList::prev_node_ptr` accessors.
- **Faithful**: the reverse window; the tracked pin at the start page's last
  cell; feeding the start page in `init`; `next` delegating to the window;
  `feed`'s garbage check, the `rem = needle.len` budget with saturating
  subtraction, the `node.prev` walk advancing the tracked pin, and the
  `rem < needle.len` return.
- **Faithful adaptation**: `*PageList` / `*Node` / `*Pin` → `NonNull` with
  `unsafe` methods and the documented lock contract; `node.prev` →
  `prev_node_ptr` (index lookup); `rem -|= x` → `rem.saturating_sub(x)`;
  `deinit` is explicit (not `Drop`) to avoid a use-after-free of the `list`
  pointer; the allocation-error returns vanish (Rust collections are infallible
  here).
- **Deferred**: `ScreenSearch` / `ViewportSearch` and the search `Thread`.
- No C ABI/header/ABI-inventory change (internal Rust). Creates the
  `terminal::search::pagelist` module; adds five accessors.

## Changes

1. `roastty/src/terminal/search/pagelist.rs` (new): `PageListSearch` (`new`,
   `deinit`, `next`, `feed`).
2. `roastty/src/terminal/search/mod.rs`: declare
   `#[allow(dead_code)] pub(crate) mod pagelist;`.
3. `roastty/src/terminal/page_list.rs`: add `Pin::node` / `is_garbage` /
   `set_node`, `Node::page_cols`, `PageList::prev_node_ptr`, and a
   `#[cfg(test)]` `PageList::tracked_pin_count` (for the lifecycle test).
4. Tests (in `pagelist.rs`) — these need content on **multiple** pages, so they
   build a two-page list and write content into both pages via the existing /
   extended `#[cfg(test)]` page-cell helpers:
   - **single page**: a one-page list with a `Fizz` match; `next` finds it, then
     `None`; `feed` returns `false` (nothing older).
   - **feed loads an older page with a match**: a two-page list with a match on
     each page; the start page's match is found first, `feed` loads the older
     page, its match is then found, and a final `feed` returns `false`.
   - **feed with no matches**: a two-page list with no needle occurrences;
     `next` is always `None` and `feed` eventually returns `false`.
   - **pin garbage ends feed**: after marking the tracked pin garbage
     (`mark_garbage_for_tests`), `feed` returns `false` immediately.
   - **deinit untracks the pin** (lifecycle): the list's tracked-pin count rises
     by one after `new` and returns to baseline after `unsafe { deinit() }`
     (asserted via a `#[cfg(test)]` `PageList::tracked_pin_count` accessor) —
     the one easily-regressed behavior not covered by the search/feed tests.

   (The cross-page-boundary spanning-match cases from upstream depend on precise
   VT layout via a `Terminal`/stream that roastty's test harness does not
   provide here; that coverage is documented as a boundary and the spanning
   logic is already exercised by the `SlidingWindow` overlap tests in
   Experiments 590-591.)

5. Format and test (`cargo fmt`, accept output).

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

- `PageListSearch` reproduces upstream's behavior (reverse window; tracked-pin
  position; `init` feeding the start page; `next` delegation; `feed`'s garbage
  check, byte budget, `node.prev` walk, and return) — faithful to
  `terminal/search/pagelist.zig`;
- the tests pass (single / feed-with-match / feed-no-match / pin-garbage), and
  the existing tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the reverse window, the tracked-pin handling, the
feed loop / byte budget, or the garbage check diverges from upstream, an
unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed the design and **approved it**, confirming the three key
questions: (Q1) the raw `NonNull<PageList>` + `NonNull<Pin>` model is the right
faithful mapping (holding `&mut PageList` is too restrictive and creates
aliasing problems with the stored pin); (Q2) the sequential `&PageList` reads
and `&mut Pin` writes are sound under the unsafe contract as long as the
references do not overlap; (Q3) the explicit `unsafe fn deinit` is preferable to
`Drop` (which would hide a raw dereference that can UAF if the list is gone);
(Q4) `Option<PageListSearch>` for an invalid tracked pin is acceptable; (Q5) the
direct page-cell multi-page test approach is sound (the exact spanning matcher
is already covered in `SlidingWindow`; this slice proves the driver feeds pages
and respects pin garbage / previous-page traversal). Two Optionals and one Nit,
all adopted:

- **Optional (adopted)**: add a lifecycle test that `deinit` untracks the pin
  (tracked-pin count rises after `new`, returns to baseline after `deinit`) —
  the one easily-regressed behavior not covered by the search/feed tests. Added
  (with a `#[cfg(test)]` `tracked_pin_count` accessor).
- **Optional (adopted)**: write `feed`'s raw-pointer accesses in small scopes so
  the non-overlap is obvious — read the next page via `prev_node_ptr` **before**
  the `&mut Pin` `set_node` write, with no `&PageList` live across it.
  Restructured accordingly.
- **Nit (adopted)**: `deinit` now documents that it must be called once, before
  the backing `PageList` is dropped, and not twice as a normal operation.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d593-prompt.md`
- Result: `logs/codex-review/20260604-d593-last-message.md`

## Result

**Result:** Pass

A new `terminal::search::pagelist` module landed (`PageListSearch`), declared
`pub(crate) mod pagelist;`. It holds `NonNull<PageList>` + a reverse
`SlidingWindow` + `NonNull<Pin>` (a tracked pin). `new` tracks a pin at the
start page's last cell, builds the reverse window, appends the start page, and
stores the list pointer; `deinit` untracks the pin (explicit, not `Drop`);
`next` delegates to the window; `feed` short-circuits on a garbage pin, then
walks older pages from `prev_node_ptr(pin.node)` — appending each (saturating
the `needle.len` byte budget), reading the next `prev` before the `&mut Pin`
`set_node` (so the borrows never overlap), advancing the tracked pin, breaking
when the budget is met — and returns whether any data was fed. Accessors added:
`Pin::node` / `is_garbage` / `set_node`, `Node::page_cols`,
`PageList::prev_node_ptr` (plus `#[cfg(test)]` `tracked_pin_count` and
`set_page_row0_text_for_tests`).

One test-helper deviation from the plan, validated by the result review: the
planned `set_first_page_text_for_tests` became
`set_page_row0_text_for_tests(page_index, text)` — because
`grow_to_two_pages_for_tests` leaves the active bottom in `pages.last` (blank)
while `set_screen_text_lines_for_tests` routes through the viewport (which after
growing maps to `pages[0]`), so neither could place a match on a chosen page for
the feed tests. Writing directly into a chosen page's row 0 lets the feed tests
put a match on the start page (`pages[1]`) and the older page (`pages[0]`).
Codex confirmed this is a sound, faithful test setup (roastty's `prev_node_ptr`
returns `idx - 1`, so `pages[0]` is the oldest — consistent with the start being
`pages.last` and `feed` walking toward index 0).

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3275 passed, 0 failed (five new tests; no
  regressions, up from 3270).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + terminal/search + page_list.rs
  - lib.rs/header/abi_harness.c) clean; `git diff --check` clean.

The five new tests: a single-page search; `feed` loading an older page with a
match; `feed` with content but no matches (feeds data, finds nothing); a garbage
pin ending `feed` immediately; and `deinit` untracking the pin (tracked-pin
count returns to baseline).

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no Required
or Optional findings** (one Nit: the `## Result` / `## Conclusion` sections were
not yet saved — added here). Codex confirmed the implementation is faithful
(reverse `SlidingWindow`, tracked pin at the start page's last cell, the initial
start-page append, `next` delegation, `feed`'s garbage short-circuit, saturating
byte budget, previous-page walk, pin advancement, and `rem < needle_len` return
all match upstream) and that the raw-pointer model is handled correctly
(`deinit` explicit; `feed` computes `prev` before mutating the tracked pin; the
safety docs cover the list lifetime/lock contract); the page-index test helper
is sound.

Review artifacts:

- Prompt: `logs/codex-review/20260604-r593-prompt.md` (result)
- Result: `logs/codex-review/20260604-r593-last-message.md` (result)

## Conclusion

This experiment ports `PageListSearch` — the history/scrollback searcher and the
most pointer-coupled search slice. It drives a reverse `SlidingWindow` backward
through a `PageList`'s pages, feeding older pages on demand and tracking its
position with a tracked pin that survives page pruning (the `garbage` flag ends
the search when the position is reused). The faithful mapping keeps upstream's
raw-pointer model — `NonNull<PageList>` / `NonNull<Pin>` with `unsafe` methods
and the documented lock/lifetime contract — rather than inventing a borrow-based
scheme. The remaining search work is the two screen-oriented searchers that
combine `ActiveSearch` and `PageListSearch` — `ScreenSearch`
(`search/screen.zig`, the largest at ~1552 lines: searches a whole `Screen`,
deduping the active area against history) and `ViewportSearch`
(`search/viewport.zig`) — and finally the search `Thread`.
