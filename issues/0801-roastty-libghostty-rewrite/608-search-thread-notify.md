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

# Experiment 608: search Thread — part 3: `Search::notify` + `Event` types

## Description

This experiment ports `Search.notify` (upstream `terminal/search/Thread.zig`
~719-810) and the `Event` / `EventCallback` types it drives. `notify` emits
state-change events to a caller-supplied callback: total-match-count changes,
fresh viewport matches, selected-match changes, and search completion. It is the
third slice of the search `Thread` (Exp 606 aggregator core, Exp 607 `feed`).

Upstream's comment: `notify` "doesn't require any locking as it only reads
internal state." In roastty this is literally true — every value it reads
(`matches_len`, `selected_match`, the drained `viewport.next()` results) comes
from already-accumulated, in-memory searcher state. `SlidingWindow::next`
matches over the bytes copied during `append` and only _copies_ node pointers
into the `Flattened` result (it never dereferences a page node), and
`selected_match` / `matches_len` read cached owned results. So **`notify` is a
safe function** — no `Terminal`, no screen-pointer deref.

With `notify`, only the outer libxev `Thread` (the OS thread + event loop +
mailbox + timers) remains, and it is blocked on a libxev port. The `Options`,
`Mailbox`, and `Message` types are part of that outer `Thread` and are deferred
with it.

## Upstream behavior (`Thread.zig`)

```zig
pub const EventCallback = *const fn (event: Event, userdata: ?*anyopaque) void;

pub const Event = union(enum) {
    quit,
    complete,
    total_matches: usize,
    selected_match: ?SelectedMatch,
    viewport_matches: []const FlattenedHighlight,   // owned by thread, valid only in the callback
    pub const SelectedMatch = struct { idx: usize, highlight: FlattenedHighlight };
};

pub fn notify(self: *Search, alloc, cb: EventCallback, ud) void {
    const screen_search = self.screens.get(self.last_screen.key) orelse return;

    // total
    const total = screen_search.matchesLen();
    if (total != self.last_screen.total) { self.last_screen.total = total; cb(.{ .total_matches = total }, ud); }

    // viewport (stale → drain viewport.next into an arena, emit)
    if (self.stale_viewport_matches) {
        self.stale_viewport_matches = false;
        var results = ...; while (self.viewport.next()) |hl| results.append(hl.clone());
        cb(.{ .viewport_matches = results.items }, ud);
    }

    // selection
    if (screen_search.selected) |m| match: {
        const flattened = screen_search.selectedMatch() orelse break :match;
        const untracked = flattened.untracked();
        if (self.last_screen.selected) |prev|
            if (prev.idx == m.idx and prev.highlight.eql(untracked)) break :match;  // unchanged
        self.last_screen.selected = .{ .idx = m.idx, .highlight = untracked };
        cb(.{ .selected_match = .{ .idx = m.idx, .highlight = flattened } }, ud);
    } else if (self.last_screen.selected != null) {
        self.last_screen.selected = null;
        cb(.{ .selected_match = null }, ud);
    }

    // complete (once)
    if (!self.last_complete and self.isComplete()) { self.last_complete = true; cb(.complete, ud); }
}
```

## Rust mapping (`thread.rs`)

The `EventCallback` C function-pointer + `userdata` becomes an idiomatic
`&mut dyn FnMut(Event<'_>)` (the closure captures whatever userdata it needs).
The `viewport_matches` "valid only during the callback" contract is expressed by
a borrow: `Event<'a>` carries `ViewportMatches(&'a [Flattened])` into a single
`cb` call over a `notify`-local `Vec`. `quit` is part of the enum (emitted by
the deferred outer `Thread`), unused here.

```rust
/// Events emitted by the search thread (upstream `Thread.Event`). `ViewportMatches` borrows a
/// `notify`-local buffer valid only for that callback invocation.
pub(in crate::terminal) enum Event<'a> {
    /// The search thread is exiting (emitted by the outer `Thread`; unused until it lands).
    Quit,
    /// Search is complete for the needle on all screens.
    Complete,
    /// The active screen's total match count changed.
    TotalMatches(usize),
    /// The selected match changed (or was cleared).
    SelectedMatch(Option<EventSelectedMatch>),
    /// The viewport matches changed (owned by `notify`, valid only during the callback).
    ViewportMatches(&'a [Flattened]),
}

/// A selected match reported to the callback (upstream `Event.SelectedMatch`).
pub(in crate::terminal) struct EventSelectedMatch {
    pub(in crate::terminal) idx: usize,
    pub(in crate::terminal) highlight: Flattened,
}

impl Search {
    /// Emit state-change events to `cb` (upstream `notify`). Reads only internal searcher state, so
    /// it needs no lock and no screen access.
    pub(in crate::terminal) fn notify(&mut self, cb: &mut dyn FnMut(Event<'_>)) {
        let key = self.last_screen.key;
        // Snapshot everything from the active screen searcher up front, releasing the borrow before
        // the mutations / callbacks below.
        let (total, sel_idx, sel_flattened) = match self.screens.get(key) {
            None => return,
            Some(ss) => (ss.matches_len(), ss.selected_index(), ss.selected_match()),
        };

        // Total matches.
        if Some(total) != self.last_screen.total {
            self.last_screen.total = Some(total);
            cb(Event::TotalMatches(total));
        }

        // Viewport matches. Always clear the stale flag first (a failed/empty drain still requires a
        // re-feed to re-search; the feed makes it stale again).
        if self.stale_viewport_matches {
            self.stale_viewport_matches = false;
            let mut results = Vec::new();
            while let Some(hl) = self.viewport.next() {
                results.push(hl);
            }
            cb(Event::ViewportMatches(&results));
        }

        // Selected match.
        match sel_idx {
            Some(idx) => {
                if let Some(flattened) = sel_flattened {
                    let untracked = flattened.untracked();
                    let unchanged = matches!(
                        &self.last_screen.selected,
                        Some(prev) if prev.idx == idx && prev.highlight == untracked
                    );
                    if !unchanged {
                        self.last_screen.selected = Some(SelectedMatch { idx, highlight: untracked });
                        cb(Event::SelectedMatch(Some(EventSelectedMatch { idx, highlight: flattened })));
                    }
                }
            }
            None => {
                if self.last_screen.selected.is_some() {
                    self.last_screen.selected = None;
                    cb(Event::SelectedMatch(None));
                }
            }
        }

        // Completion (emitted once).
        if !self.last_complete && self.is_complete() {
            self.last_complete = true;
            cb(Event::Complete);
        }
    }
}
```

### New `ScreenSearch` accessor

`notify` gates the selection branch on `screen_search.selected` (the field) and
reads its `idx`. `selected_match()` (Exp 600) already yields the `Flattened`;
add the index:

```rust
/// The index of the currently-selected match, if any (upstream `screen_search.selected.?.idx`).
pub(in crate::terminal) fn selected_index(&self) -> Option<usize> {
    self.selected.as_ref().map(|m| m.idx)
}
```

### Notes / deviations

- **`notify` is safe** — it reads only accumulated in-memory state. The `alloc`
  arena and OOM handling drop (roastty allocation is infallible; the
  reset-on-OOM viewport path is unreachable).
- **`EventCallback` → `&mut dyn FnMut(Event<'_>)`**: the idiomatic Rust port of
  the C function-pointer + opaque `userdata` pair; the borrow on
  `ViewportMatches` encodes upstream's "valid only during the callback".
- `Untracked` already derives `PartialEq`, so `prev.highlight == untracked` maps
  upstream's `prev.highlight.eql(untracked)`.
- The snapshot-up-front restructuring (vs. holding `screen_search` across the
  body) avoids borrowing `self.screens` while `self.is_complete()` borrows
  `&self` — a faithful, behavior-preserving refactor.

## Verification

- `cargo build -p roastty` — no warnings.
- `cargo test -p roastty` — no regressions; new tests (real `Terminal`, `feed`
  then `notify` with an event-collecting closure):
  - `notify_emits_total_and_viewport_matches` — after `feed`, `notify` emits a
    `TotalMatches` and a `ViewportMatches` (both non-empty for a screen with the
    needle); a second `notify` (nothing stale) emits neither.
  - `notify_emits_complete_once` — drive the active searcher to completion via
    `search_all`, then `notify` emits `Complete`; a further `notify` does not.
  - `notify_emits_and_dedups_selected_match` — select a match on the active
    searcher, `notify` emits `SelectedMatch(Some)`; an unchanged `notify` does
    not re-emit.
  - `notify_clears_selection` — after a selection, drop it, `notify` emits
    `SelectedMatch(None)`.
  - `notify_with_out_of_range_selection_does_not_clear` (Optional, adopted) —
    with a selection whose `idx` is out of range (`selected_index()` is `Some`
    but `selected_match()` is `None`, forced via a `#[cfg(test)]`
    `set_selected_idx_for_tests` on `ScreenSearch`), `notify` emits no
    `SelectedMatch` and does not clear a prior `last_screen.selected`.
  - `notify_with_no_active_screen_searcher_is_a_noop` (Optional, adopted) — a
    `Search` whose active key has no searcher emits nothing (upstream's early
    `screens.get(key) orelse return`).
- `cargo fmt -p roastty -- --check` — clean.
- no-ghostty grep on touched source — clean.
- `git diff --check` — clean.

Pass = `notify` emits each event exactly when the corresponding state changes
(and `complete` only once), reading only internal state, with the
`ViewportMatches` borrow scoped to its callback.

## Design Review

Codex reviewed the design and **APPROVED** it with **no Required findings**,
confirming: `EventCallback` → `&mut dyn FnMut(Event<'_>)` is the right mapping
and `ViewportMatches(&[Flattened])` faithfully encodes "owned by notify, valid
only during the callback"; the event order (total → viewport → selected →
complete) and the clear-stale-before-drain ordering match upstream; the
selection branch preserves the "selected `Some` but `selected_match()` `None` →
do nothing (don't clear)" case; `Complete` stays once-only; the
snapshot-up-front restructuring is behavior-preserving (viewport draining is
independent of `ScreenSearch` state); and `notify` being safe is correct (it
copies un-dereferenced node pointers — any later interpretation is the
callback's responsibility). Both Optionals adopted:

- **Optional (adopted)**: a focused test for the `selected_index() == Some` but
  `selected_match() == None` branch (emits nothing, doesn't clear) — added via a
  `#[cfg(test)]` `ScreenSearch::set_selected_idx_for_tests` that bumps the
  selected idx out of range while keeping the (tracked) highlight.
- **Optional (adopted)**: a no-active-searcher test (`notify` returns early,
  emits nothing).

Review artifacts:

- Prompt: `logs/codex-review/20260605-d608-prompt.md`
- Result: `logs/codex-review/20260605-d608-last-message.md`

## Result

**Result:** Pass

Implemented `Search::notify` and the `Event` / `EventSelectedMatch` types in
`thread.rs`, plus `ScreenSearch::selected_index` (and two `#[cfg(test)]`
helpers, `set_selected_idx_for_tests` / `clear_selection_for_tests`) in
`screen.rs`. The port faithfully mirrors upstream `notify`: snapshot the active
screen searcher's `(matches_len, selected_index, selected_match)` up front
(early return if absent), then emit in upstream order — total (on change) →
viewport (clear stale flag, drain `viewport.next()` into a local `Vec`, emit
`ViewportMatches(&buf)`) → selection (emit on change; the out-of-range
`selected_index Some` / `selected_match None` case does nothing and does not
clear; `None` clears + emits only if a prior selection existed) → complete
(once, via `last_complete`). `EventCallback` + opaque userdata becomes
`&mut dyn FnMut(Event<'_>)`, and the `viewport_matches` "valid only during the
callback" contract is the borrow on `Event::ViewportMatches`. `notify` is safe —
it reads only accumulated in-memory state and copies (never dereferences) node
pointers.

Six tests cover total + viewport emission, complete-once, selected-match emit +
dedup, the cleared-selection path, the out-of-range no-clear edge case, and the
no-active-searcher noop. Gates: `cargo fmt --check` clean,
`cargo build -p roastty` no warnings, `cargo test -p roastty` **3334 passed / 0
failed** (3328 → 3334, +6), no-ghostty grep clean, `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **APPROVED** it with **no Required
and no Optional findings**, confirming: the `Event` types map cleanly and
`ViewportMatches(&[Flattened])` encodes callback-scoped validity; `notify`
preserves upstream order, clears the stale flag before draining, matches the
selection branch (including the out-of-range no-clear case), and emits
`Complete` once; the snapshot-up-front restructuring is behavior-preserving
(viewport draining doesn't mutate `ScreenSearch`); `notify` is correctly safe;
and the `#[cfg(test)]` helpers are acceptable. The lone Nit (record `## Result`
/ `## Conclusion`) is addressed here.

Review artifacts:

- Prompt: `logs/codex-review/20260605-r608-prompt.md`
- Result: `logs/codex-review/20260605-r608-last-message.md`

## Conclusion

The inner `Search` of the search thread is now complete: `new` / `deinit` /
`is_complete` / `tick` / `feed` / `notify`, plus the `Tick` and `Event` types.
The _only_ remaining piece of the entire search subsystem is the **outer
`Thread`** — the OS thread running a libxev event loop with a mailbox
(`BlockingQueue<Message>`), `REFRESH_INTERVAL` timers, and the `Options` /
`Message` types — which is **blocked on a libxev port** roastty does not have.
That block is a genuine dependency boundary, shared with the regex/oniguruma and
URI-parser work elsewhere in Issue 801; the next tractable slices lie outside
the search subsystem.
