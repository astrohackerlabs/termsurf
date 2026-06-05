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

# Experiment 596: search ScreenSearch tick (the state machine step)

## Description

This experiment continues `ScreenSearch` (upstream `terminal/search/screen.zig`)
with the **`tick` state machine**: `tick` (the dispatcher), `tickActive`
(consume the active-area matches), and `tickHistory` (consume the loaded history
matches, deduping against the active area). `tick` is the incremental, lock-free
progress step. It extends `terminal::search::screen` and adds the `Tick` outcome
enum.

## Upstream behavior

```zig
pub const TickError = Allocator.Error || error{ FeedRequired, SearchComplete };

/// Make incremental progress without accessing screen state (no lock required).
pub fn tick(self: *ScreenSearch) TickError!void {
    switch (self.state) {
        .active => try self.tickActive(),
        .history => try self.tickHistory(),
        .history_feed => return error.FeedRequired,
        .complete => return error.SearchComplete,
    }
}

fn tickActive(self) !void {
    // Consume the entire active area in one go (it's small).
    while (self.active.next()) |hl| {
        var hl_cloned = try hl.clone(alloc);
        try self.active_results.append(alloc, hl_cloned);
    }
    self.state = .history;
}

fn tickHistory(self) !void {
    const history = if (self.history) |*h| h else { self.state = .complete; return; };
    while (history.searcher.next()) |hl| {
        // Skip matches in the start node — those are covered by the active area search.
        if (hl.chunks.items(.node)[0] == history.start_pin.node) continue;
        var hl_cloned = try hl.clone(alloc);
        try self.history_results.append(alloc, hl_cloned);
    }
    self.state = .history_feed;
}
```

- `tick` dispatches on the state: `active` → `tickActive`, `history` →
  `tickHistory`, `history_feed` → `FeedRequired` (caller must `feed`),
  `complete` → `SearchComplete`. A non-error return means progress was made.
- `tickActive` drains `active.next()` into `active_results`, then moves to
  `history`.
- `tickHistory`: if there is no history, the search is `complete`. Otherwise it
  drains `history.searcher.next()` into `history_results`, **skipping** any
  match whose first chunk is in the `start_pin`'s node (that node overlaps the
  active area, already searched), then moves to `history_feed` (the window is
  exhausted; the caller must `feed` the next page).

## Rust mapping (`roastty/src/terminal/search/screen.rs`)

`tick` returns a `Tick` outcome enum instead of an error union (Rust has no OOM
error here): `Progressed` (the `Ok(void)` "made progress" case), `FeedRequired`,
or `Complete`. roastty's `active.next()` / `searcher.next()` already return an
**owned** `Flattened` (the matcher deep-clones into it), so the upstream
`hl.clone(alloc)` is unnecessary — the result is pushed directly. The
`start_pin`-node dedup dereferences the tracked pin under the screen-alive
invariant (the same model as `highlight` dereferencing stored node pointers):
`tick` / `tickHistory` stay safe fns with internal `unsafe` blocks justified by
that invariant.

```rust
/// The outcome of a `tick` (upstream's `TickError` set, minus the OOM case which is infallible in
/// Rust). `Progressed` is upstream's `Ok(void)` "made progress".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::terminal) enum Tick {
    /// Progress was made; call `tick` again.
    Progressed,
    /// The search needs more data; the caller must `feed` (upstream `error.FeedRequired`).
    FeedRequired,
    /// The search is complete given the current screen state (upstream `error.SearchComplete`).
    Complete,
}

impl ScreenSearch {
    /// Make incremental progress on the search without accessing screen state (upstream `tick`).
    /// Returns whether progress was made, a feed is required, or the search is complete.
    pub(in crate::terminal) fn tick(&mut self) -> Tick {
        match self.state {
            State::Active => {
                self.tick_active();
                Tick::Progressed
            }
            State::History => {
                self.tick_history();
                Tick::Progressed
            }
            State::HistoryFeed => Tick::FeedRequired,
            State::Complete => Tick::Complete,
        }
    }

    /// Consume the entire active area into `active_results`, then move to history (upstream
    /// `tickActive`). The active area is small, so this drains it in one go.
    fn tick_active(&mut self) {
        while let Some(hl) = self.active.next() {
            self.active_results.push(hl);
        }
        self.state = State::History;
    }

    /// Consume the loaded history matches into `history_results` (deduping against the active area),
    /// then request a feed (upstream `tickHistory`). No history → complete.
    fn tick_history(&mut self) {
        let history = match &mut self.history {
            Some(h) => h,
            None => {
                self.state = State::Complete;
                return;
            }
        };

        while let Some(hl) = history.searcher.next() {
            // Skip matches whose first chunk is in the start node — that node overlaps the active
            // area, which is searched separately.
            // SAFETY: `start_pin` is a tracked pin in the (alive) screen's storage; the screen
            // outlives the search (the construction-time invariant).
            let start_node = unsafe { history.start_pin.as_ref() }.node();
            if hl.chunks[0].node == start_node {
                continue;
            }
            self.history_results.push(hl);
        }

        self.state = State::HistoryFeed;
    }
}
```

## Scope / faithfulness notes

- **Ported**: `tick` / `tickActive` / `tickHistory` → `tick` / `tick_active` /
  `tick_history`; the `TickError` outcomes → the `Tick` enum.
- **Faithful**: `tick`'s state dispatch (`Active`/`History` make progress;
  `HistoryFeed` → `FeedRequired`; `Complete` → `Complete`); `tickActive`
  draining the active area then moving to `History`; `tickHistory`'s no-history
  → `Complete`, the `start_pin`-node dedup skip, draining into
  `history_results`, and the move to `HistoryFeed`.
- **Faithful adaptation**: `TickError!void` → the `Tick` outcome enum (the OOM
  error vanishes; `Ok(void)` becomes `Progressed`); the upstream
  `hl.clone(alloc)` is dropped because roastty's `next()` returns an owned
  `Flattened` already (deep-cloned by the matcher); `ArrayList.append` →
  `Vec::push`; `chunks.items(.node)[0]` → `hl.chunks[0].node`; the
  `start_pin.node` read is an `unsafe` deref of the tracked pin under the
  screen-alive invariant (`tick` / `tick_history` stay safe fns, like
  `highlight`).
- **Deferred**: `init` / `reloadActive` (construction), `feed` / `pruneHistory`,
  `searchAll`, and `select` / `selectNext` / `selectPrev`; plus `ViewportSearch`
  and the search `Thread`.
- No C ABI/header/ABI-inventory change (internal Rust). Extends
  `terminal::search::screen`.

## Changes

1. `roastty/src/terminal/search/screen.rs`: add the `Tick` enum and
   `ScreenSearch::tick` / `tick_active` / `tick_history`; update the module doc
   comment.
2. Tests (in `screen.rs`):
   - **active drain then complete (no history)**: build a `ScreenSearch` whose
     `active` was `update`d over a `PageList` containing `"Fizz"` (a forward
     match), `history: None`. The first `tick()` is `Progressed` and drains the
     active area (`active_results` becomes non-empty, `matches_len() == 1`); the
     second `tick()` is `Progressed` and (history `None`) completes; the third
     `tick()` is `Complete`.
   - **feed-required state**: a `ScreenSearch` manually set to `HistoryFeed`
     returns `Tick::FeedRequired` from `tick()` (no state change).
   - **already complete**: a `ScreenSearch` set to `Complete` returns
     `Tick::Complete`.

   (The `tick_history` dedup path — matches in the start node skipped — depends
   on a constructed `HistorySearch` with a tracked `start_pin`, which lands with
   the construction slice; it is documented here and exercised then.)

3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty terminal::search
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/terminal/search && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `tick` / `tick_active` / `tick_history` reproduce upstream's state machine
  (the dispatch, the active drain → history, the history dedup + drain → feed,
  the no-history → complete) — faithful to `terminal/search/screen.zig`;
- the tests pass (active drain / feed-required / already-complete), and the
  existing tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the dispatch, the active drain, the history dedup,
or the state transitions diverge from upstream, an unrelated item changes, or
any public C API/ABI changes.

## Design Review

Codex reviewed the design and **approved it with no Required, Optional, or Nit
findings**, confirming the key decisions: (Q2) dropping upstream's `hl.clone` is
sound — roastty's `ActiveSearch::next` / `PageListSearch::next` already return
owned `Flattened` values with owned `Vec<Chunk>`, so pushing them directly is
the faithful Rust ownership adaptation (re-cloning would only add work); (Q3)
`tick` / `tick_history` can remain safe fns — the `unsafe` `start_pin`
dereference is covered by the `ScreenSearch` construction invariant (the same
model as the lower-level searchers' safe methods over raw pointers established
by unsafe setup), and since the fields are private, outside callers cannot
construct an invalid `HistorySearch` without going through the future
constructor. Codex confirmed the state transitions match upstream (Active drains
then → History; History with no searcher → Complete; History with loaded results
dedups against `start_pin` then → HistoryFeed; HistoryFeed / Complete return
explicit outcomes) and that the direct push preserves result ordering and
ownership.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d596-prompt.md`
- Result: `logs/codex-review/20260604-d596-last-message.md`

## Result

**Result:** Pass

`ScreenSearch` gained the `Tick` outcome enum (`Progressed` / `FeedRequired` /
`Complete`) and the `tick` / `tick_active` / `tick_history` methods. `tick`
dispatches on `state`: `Active` runs `tick_active` (drains `active.next()` into
`active_results`, then → `History`), `History` runs `tick_history` (no history →
`Complete`; otherwise drains `searcher.next()` into `history_results`, skipping
matches whose first chunk is in the `start_pin`'s node, then → `HistoryFeed`),
`HistoryFeed` → `FeedRequired`, `Complete` → `Complete`. roastty's `next()`
returns an owned `Flattened`, so the results are pushed directly (no re-clone);
`tick` / `tick_history` are safe with an internal `unsafe` `start_pin` deref
under the screen-alive invariant.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3284 passed, 0 failed (three new tests; no
  regressions, up from 3281).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + terminal/search +
  lib.rs/header/abi_harness.c) clean; `git diff --check` clean.

The three new tests: `tick` draining the active area then completing with no
history (`tick()` → `Progressed`, `matches_len() == 1`, then `Progressed`, then
`Complete`); `tick` in `HistoryFeed` returning `FeedRequired`; and `tick` when
`Complete` returning `Complete`. The `tick_history` dedup-with-history path is
deferred to the construction slice (which provides a tracked `start_pin`).

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no Required
or Optional findings** (one Nit: the `## Result` / `## Conclusion` sections were
not yet saved — added here). Codex confirmed the implementation is faithful:
`tick` dispatches by state, the active area drains into the owned
`active_results` then moves to history, history with no searcher completes,
history with loaded results dedups against the tracked start node then requests
a feed, and `HistoryFeed` / `Complete` return explicit outcomes; dropping the
extra clone is correct for roastty's owned `Flattened`.

Review artifacts:

- Prompt: `logs/codex-review/20260604-r596-prompt.md` (result)
- Result: `logs/codex-review/20260604-r596-last-message.md` (result)

## Conclusion

This experiment ports the `ScreenSearch` `tick` state machine — the lock-free
incremental progress step that drains the active area then the loaded history
(deduping the start-node overlap), driving the `Active` → `History` →
`HistoryFeed` → `Complete` transitions. The remaining `ScreenSearch` work is the
construction (`init` / `reload_active`, the trickiest piece — it loads the
active area, sets up the `HistorySearch` with its tracked `start_pin`, and
handles active-area growth into history), `feed` / `prune_history` (advance the
history searcher and prune stale results), and `select` / `select_next` /
`select_prev` (step the tracked selected match). After `ScreenSearch`,
`ViewportSearch` (`search/viewport.zig`) and the search `Thread` remain.
