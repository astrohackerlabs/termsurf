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

# Experiment 614: search Thread — part 5: the `select` message handler

## Description

Slice 2 of 3 of the outer search `Thread` (after the Exp 613 foundation): the
`select` message handler (upstream `Thread.select`) and the `Message::Select`
variant. `select` makes/moves the selection on the active screen searcher,
resets the notify cache (so the new selection re-emits), and — if the match is
not already visible in the viewport — scrolls the screen so the match comes into
view. The `thread_main` event loop + spawn remain slice 3 (Exp 615).

roastty already has the scroll machinery: `PageList` has `Scroll::Pin(Pin)` +
`scroll_to_pin`, and `PageChunk::overlaps`. This slice only exposes thin
accessors for them; no new scroll algorithm is written.

## Upstream behavior (`Thread.zig` `select`)

```zig
fn select(self, sel) !void {
    const s = if (self.search) |*s| s else return;
    const screen_search = s.screens.getPtr(s.last_screen.key) orelse return;

    self.opts.mutex.lock(); defer self.opts.mutex.unlock();

    _ = try screen_search.select(sel);                       // make/move the selection
    const flattened = screen_search.selectedMatch() orelse return;
    s.last_screen.selected = null;                           // reset cache → re-notify

    const screen = self.opts.terminal.screens.get(s.last_screen.key) orelse return;
    var it = screen.pages.pageIterator(.right_down, .{ .viewport = .{} }, null);
    const hl_chunks = flattened.chunks.slice();
    while (it.next()) |chunk| for (0..hl_chunks.len) |i| {
        const c = hl_chunks.get(i);
        if (chunk.overlaps(.{ .node = c.node, .start = c.start, .end = c.end })) return;  // already visible
    };

    screen.scroll(.{ .pin = flattened.startPin() });         // scroll the match into view
}
```

## New surface

### `PageList` (`page_list.rs`, `pub(in crate::terminal)`)

```rust
/// Whether any viewport page-chunk overlaps the `[start, end)` rows of `node` (upstream
/// `select`'s viewport `pageIterator` + `chunk.overlaps`). Used to decide if a match is already
/// visible.
pub(in crate::terminal) fn viewport_overlaps(&self, node: NonNull<Node>, start: CellCountInt, end: CellCountInt) -> bool {
    let probe = PageChunk { node, start, end };
    let top = self.get_top_left(point::Tag::Viewport);
    // Upstream's viewport iterator assumes a valid extent; match `viewport_nodes` and `expect`.
    let bottom = self
        .get_bottom_right(point::Tag::Viewport)
        .expect("viewport bottom-right must exist");
    let mut it = PageIterator { list: self, row: Some(top), limit: Some(bottom), direction: Direction::RightDown };
    while let Some(chunk) = it.next() {
        if chunk.overlaps(&probe) { return true; }
    }
    false
}

/// Scroll the viewport to `pin` via the normal `Scroll::Pin` behavior (upstream
/// `screen.scroll(.{ .pin })`) — the existing scroll path may clamp, so `pin` is not necessarily
/// placed exactly at the top. Integrity-checked.
pub(in crate::terminal) fn scroll_to_pin_for_search(&mut self, pin: Pin) {
    self.scroll(Scroll::Pin(pin));
}
```

### `Screen` (`screen.rs`, `pub(in crate::terminal)`)

```rust
/// Whether any viewport chunk overlaps the given match chunk (delegates to `PageList`).
pub(in crate::terminal) fn viewport_overlaps_chunk(&self, node: NonNull<Node>, start: CellCountInt, end: CellCountInt) -> bool {
    self.pages.viewport_overlaps(node, start, end)
}

/// Scroll so `pin` is at the viewport top (upstream `screen.scroll(.{ .pin })`).
pub(in crate::terminal) fn scroll_to_pin(&mut self, pin: Pin) {
    self.pages.scroll_to_pin_for_search(pin);
}
```

`Chunk` (the match's chunks, `Flattened.chunks`) already exposes `node` /
`start` / `end` (`pub(in crate::terminal)`), and `Flattened::start_pin()`
exists.

## Rust mapping (`thread.rs`)

```rust
// Message gains the Select variant:
pub(in crate::terminal) enum Message {
    ChangeNeedle(MessageData<'static, u8, 255>),
    Select(Select),               // Select re-exported from super::screen
}

// drain_mailbox gains the arm:
Message::Select(sel) => unsafe { self.select(sel) },

impl Thread {
    /// Make/move the selection and scroll it into view if needed (upstream `select`).
    ///
    /// # Safety
    /// `opts.terminal` / `opts.lock` live; the terminal outlives the search.
    pub(in crate::terminal) unsafe fn select(&mut self, sel: Select) {
        let Some(s) = self.search.as_mut() else { return; };
        let key = s.last_screen.key;
        if s.screens.get(key).is_none() { return; }

        // SAFETY: lock live, guards terminal.
        let _guard = unsafe { self.opts.lock.as_ref() }.lock().unwrap();

        // Make the selection, then snapshot the flattened match (drops the searcher borrow).
        let flattened = {
            let screen_search = s.screens.get_mut(key).unwrap();
            // SAFETY: screen live; lock held.
            unsafe { screen_search.select(sel) };
            screen_search.selected_match()
        };
        let Some(flattened) = flattened else { return; };
        s.reset_last_selected(); // s.last_screen.selected = None → re-notify

        // Resolve the active screen pointer.
        // SAFETY: terminal live.
        let present = unsafe { Terminal::present_screen_ptrs(self.opts.terminal) };
        let Some((_, screen_ptr)) = present.iter().find(|(k, _)| *k == key) else { return; };

        // If the match is already visible in the viewport, do nothing. The shared `&Screen` borrow
        // is scoped to this block so it ends before the `&mut Screen` scroll below.
        let already_visible = {
            // SAFETY: screen live; lock held.
            let screen = unsafe { screen_ptr.as_ref() };
            flattened
                .chunks()
                .iter()
                .any(|c| screen.viewport_overlaps_chunk(c.node(), c.start(), c.end()))
        };
        if already_visible {
            return;
        }

        // Scroll the match's start pin into view.
        // SAFETY: screen live; lock held; this is the only access to it here.
        let mut sp = *screen_ptr;
        unsafe { sp.as_mut() }.scroll_to_pin(flattened.start_pin());
    }
}
```

### New `Search` / `Flattened` accessors

- `Search::reset_last_selected(&mut self)` — sets
  `self.last_screen.selected = None` (upstream `s.last_screen.selected = null`),
  so `notify` re-emits the selection. (A small method since `last_screen` is
  private.)
- `Flattened::chunks()` / `Chunk::{node, start, end}` accessors — exposed so the
  search-thread `select` can read the match's chunks for the overlap check (the
  `Chunk` fields are `pub(in crate::terminal)`, so this is just a `chunks()`
  iterator accessor if not already present).

### Notes / deviations

- No new scroll algorithm: roastty's `PageList::scroll(Scroll::Pin)` /
  `scroll_to_pin` and `PageChunk::overlaps` already exist; this slice only
  exposes `pub(in crate::terminal)` wrappers.
- `select` holds `opts.mutex` for the whole body (upstream `defer unlock`),
  matching the lock-during-selection contract; the selection + the screen
  derefs + the scroll all happen under it.
- The nested `pageIterator × hl_chunks` double loop becomes
  `for c in flattened.chunks() { if viewport_overlaps_chunk(c) return }` — the
  inner viewport iteration is encapsulated in `PageList::viewport_overlaps`.
- `Message::Select` is added now (deferred from Exp 613); `drain_mailbox` gains
  its arm.

## Verification

- `cargo build -p roastty` — no warnings.
- `cargo test -p roastty` — no regressions; new tests (a real `Terminal` with
  the needle, a `Mutex<()>`, a search started via `change_needle`):
  - `select_makes_a_selection` — after `change_needle` + driving the active
    searcher, `select(Next)` sets a selection (the active searcher's
    `selected_index()` becomes `Some`).
  - `select_resets_the_notify_cache` — after a `notify` records a selection,
    `select` resets `last_screen.selected` so the next `notify` re-emits the
    `SelectedMatch`.
  - `select_with_no_search_is_a_noop` — `select` before any `change_needle`
    returns without panicking.
  - `drain_mailbox_dispatches_select` — push a `Select` message,
    `drain_mailbox`, and a selection is made.
  - (Scroll-into-view is exercised indirectly; a dedicated scrolled-viewport
    assertion is a follow-up if the setup is tractable.)
- `cargo fmt -p roastty -- --check` — clean.
- no-ghostty grep on touched source — clean.
- `git diff --check` — clean.

Pass = `select` makes/moves the selection under the lock, resets the notify
cache, and scrolls the match into view when it isn't already visible, with the
new `Message::Select` dispatched by `drain_mailbox`.

## Design Review

Codex reviewed the design and raised **one Required** finding, adopted:

- **Required (adopted)**: `PageList::viewport_overlaps` must `expect` the
  viewport bottom-right (matching `viewport_nodes`), not treat a missing one as
  "not visible" — the latter would silently scroll on an invariant violation.
  Changed to `.expect("viewport bottom-right must exist")`.
- **Optional (adopted)**: scope the shared `&Screen` overlap check in its own
  block (`let already_visible = { … }`) before the `&mut Screen` scroll, making
  the raw-pointer aliasing story explicit.
- **Optional (noted)**: a dedicated scroll-into-view test — attempted if the
  scrolled-viewport setup is tractable; otherwise a follow-up (the
  selection/notify tests cover the rest).
- **Nit (adopted)**: `scroll_to_pin_for_search`'s doc now says it goes through
  the normal `Scroll::Pin` behavior (which may clamp), not that the pin lands
  exactly at the top.

Codex confirmed the rest is faithful: `Message::Select` is the right slice
addition; `select` ignores the boolean from `ScreenSearch::select`, resets the
notify cache only after a selected match exists, checks viewport overlap by
chunk, scrolls with `start_pin`, and holds the mutex for the whole body.

Review artifacts:

- Prompt: `logs/codex-review/20260605-d614-prompt.md`
- Result: `logs/codex-review/20260605-d614-last-message.md`
