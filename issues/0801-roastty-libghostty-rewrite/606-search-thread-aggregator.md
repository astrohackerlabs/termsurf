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

# Experiment 606: search Thread — part 1: the `Search` aggregator core

## Description

The search `Thread` (upstream `terminal/search/Thread.zig`, 905 lines) is the
background driver that owns the searchers and pumps them off the render thread.
It decomposes into two layers:

1. The **outer `Thread`** — a real OS thread running a **libxev** event loop
   with a mailbox, timers (`REFRESH_INTERVAL`, 40 FPS), and an `EventCallback`.
   This is **blocked**: roastty has no `xev`/libxev port (`rg xev roastty/src`
   is empty), so the event loop, `Options`, `Mailbox`, and `Message` cannot be
   ported faithfully yet.
2. The **inner `Search`** — the lock-aware multi-screen orchestration: it owns a
   `ViewportSearch` and a per-screen `ScreenSearch` map, and exposes `init` /
   `deinit` / `isComplete` / `tick` / `feed` / `notify`.

This experiment ports the **dependency-light core of the inner `Search`**: the
struct, the `Tick` enum, and `init` / `deinit` / `is_complete` / `tick`. These
depend only on the already-ported `ScreenSearch` (594–604) and `ViewportSearch`
(605). The two methods that touch unported surface are deferred to
clearly-scoped follow-ups:

- `feed` needs `Terminal` integration that roastty lacks: a
  `search_viewport_dirty` flag on `TerminalFlags`, and a uniform "all screens"
  enumeration (roastty models screens as `primary` + `Option<alternate>`, not an
  `EnumMap`). → **Exp 607** (after adding/adapting that surface).
- `notify` needs the `Event` / `EventCallback` machinery (the Thread's external
  interface) and an arena for cloned viewport results. → bundled with the
  `Event` types in a later slice.

This is a faithful incremental slice: it lands the multi-screen aggregator's
shape and its lock-free `tick`, fully testable with manually-inserted
`ScreenSearch`es, without fabricating xev or renderer hooks.

## Upstream behavior (`Thread.zig`, the `Search` struct)

```zig
const Search = struct {
    viewport: ViewportSearch,
    screens: std.EnumMap(ScreenSet.Key, ScreenSearch),
    last_screen: ScreenState,
    last_complete: bool,
    stale_viewport_matches: bool,

    pub fn init(alloc, needle) !Search {
        var vp = try ViewportSearch.init(alloc, needle);
        vp.active_dirty = true;  // start dirty so the first change re-searches
        return .{ .viewport = vp, .screens = .init(.{}),
            .last_screen = .{ .key = .primary }, .last_complete = false,
            .stale_viewport_matches = true };
    }

    pub fn deinit(self) void {
        self.viewport.deinit();
        var it = self.screens.iterator();
        while (it.next()) |entry| entry.value.deinit();
    }

    pub fn isComplete(self) bool {
        var it = self.screens.iterator();
        while (it.next()) |entry| if (!entry.value.state.isComplete()) return false;
        return true;
    }

    pub const Tick = enum { complete, progress, blocked };

    pub fn tick(self) Tick {
        var result: Tick = .complete;
        var it = self.screens.iterator();
        while (it.next()) |entry| {
            if (entry.value.tick()) { result = .progress; }
            else |err| switch (err) {
                error.OutOfMemory => log.warn(...),     // infallible in roastty
                error.SearchComplete => {},             // good, no change
                error.FeedRequired => switch (result) {
                    .complete => result = .blocked,     // blocked: nothing progressed
                    .progress => {}, .blocked => {},
                },
            }
        }
        return result;
    }
    // feed, notify: deferred (see Description).
};
```

## Rust mapping (`roastty/src/terminal/search/thread.rs`, new file)

`std.EnumMap(ScreenSet.Key, ScreenSearch)` → a small fixed map over the two
`TerminalScreenKey` variants (`Primary`, `Alternate`), since roastty has no
`EnumMap` and exactly two screen kinds. `ScreenSearch::tick` already returns the
`screen::Tick` enum (`Progressed` / `FeedRequired` / `Complete`) rather than a
Zig error union, so the aggregation matches on it directly.
`ScreenSearch::deinit` is `unsafe` (it dereferences the screen pointer), so
`Search::deinit` is `unsafe` too. `ViewportSearch` needs no `deinit` (its `Drop`
frees the window).

```rust
use super::super::terminal::TerminalScreenKey;
use super::screen::{ScreenSearch, Tick as ScreenTick};
use super::super::highlight::Untracked;
use super::viewport::ViewportSearch;

/// The number of screen kinds (`TerminalScreenKey`: Primary, Alternate).
const SCREEN_KEY_COUNT: usize = 2;

fn key_index(key: TerminalScreenKey) -> usize {
    match key { TerminalScreenKey::Primary => 0, TerminalScreenKey::Alternate => 1 }
}

/// Per-screen searchers, keyed by `TerminalScreenKey` (upstream `EnumMap<Key, ScreenSearch>`).
#[derive(Default)]
struct ScreenSearches {
    entries: [Option<ScreenSearch>; SCREEN_KEY_COUNT],
}

impl ScreenSearches {
    fn get(&self, key: TerminalScreenKey) -> Option<&ScreenSearch> { self.entries[key_index(key)].as_ref() }
    fn get_mut(&mut self, key: TerminalScreenKey) -> Option<&mut ScreenSearch> { self.entries[key_index(key)].as_mut() }
    /// Insert, returning any replaced searcher so the caller can `deinit` it (avoids leaking the
    /// replaced searcher's tracked pins when `feed`'s reconciliation lands).
    fn insert(&mut self, key: TerminalScreenKey, s: ScreenSearch) -> Option<ScreenSearch> { self.entries[key_index(key)].replace(s) }
    fn take(&mut self, key: TerminalScreenKey) -> Option<ScreenSearch> { self.entries[key_index(key)].take() }
    fn iter_mut(&mut self) -> impl Iterator<Item = &mut ScreenSearch> { self.entries.iter_mut().filter_map(|e| e.as_mut()) }
    fn iter(&self) -> impl Iterator<Item = &ScreenSearch> { self.entries.iter().filter_map(|e| e.as_ref()) }
}

/// State captured at the last screen switch (upstream `Search.ScreenState`). Read by the deferred
/// `feed` / `notify`; set here by `init`.
struct ScreenState {
    key: TerminalScreenKey,
    total: Option<usize>,
    selected: Option<SelectedMatch>,
}

struct SelectedMatch {
    idx: usize,
    highlight: Untracked,
}

/// The progress of one `tick` across all screens (upstream `Search.Tick`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::terminal) enum Tick {
    /// All searches are complete.
    Complete,
    /// At least one screen made progress.
    Progress,
    /// All incomplete searches are blocked on a feed.
    Blocked,
}

/// The multi-screen search aggregator owned by the search thread (upstream `Thread.Search`). It
/// drives a `ScreenSearch` per terminal screen plus one `ViewportSearch` for the active screen.
pub(crate) struct Search {
    viewport: ViewportSearch,
    screens: ScreenSearches,
    last_screen: ScreenState,
    last_complete: bool,
    stale_viewport_matches: bool,
}

impl Search {
    /// Construct an aggregator searching for `needle` (upstream `init`). Active dirty-tracking starts
    /// dirty so the first active-area change re-searches.
    pub(in crate::terminal) fn new(needle: &[u8]) -> Search {
        let mut viewport = ViewportSearch::new(needle);
        viewport.set_active_dirty(Some(true));
        Search {
            viewport,
            screens: ScreenSearches::default(),
            last_screen: ScreenState { key: TerminalScreenKey::Primary, total: None, selected: None },
            last_complete: false,
            stale_viewport_matches: true,
        }
    }

    /// Tear down every screen searcher (upstream `deinit`). `ViewportSearch` drops itself.
    ///
    /// # Safety
    /// Each screen searcher's backing `Screen` must still be live (as `ScreenSearch::deinit`).
    pub(in crate::terminal) unsafe fn deinit(&mut self) {
        for s in self.screens.iter_mut() {
            // SAFETY: caller's contract — the backing screen is live.
            unsafe { s.deinit() };
        }
    }

    /// Whether all screen searches are complete (upstream `isComplete`).
    pub(in crate::terminal) fn is_complete(&self) -> bool {
        self.screens.iter().all(|s| s.is_state_complete())
    }

    /// Tick every screen forward without taking the big lock (upstream `tick`).
    pub(in crate::terminal) fn tick(&mut self) -> Tick {
        let mut result = Tick::Complete;
        for s in self.screens.iter_mut() {
            match s.tick() {
                ScreenTick::Progressed => result = Tick::Progress,
                ScreenTick::Complete => {}
                ScreenTick::FeedRequired => {
                    if result == Tick::Complete {
                        result = Tick::Blocked;
                    }
                }
            }
        }
        result
    }
}
```

### New `ScreenSearch` accessor

`isComplete` reads `entry.value.state.isComplete()` — `ScreenSearch.state` is
private. Add a thin predicate (mirroring the existing `State::is_complete`):

```rust
/// Whether this screen's search state is complete (upstream `self.state.isComplete()`).
pub(in crate::terminal) fn is_state_complete(&self) -> bool {
    self.state.is_complete()
}
```

### Notes / deviations

- **`Search::tick` is safe** (upstream `tick` takes no lock and
  `ScreenSearch::tick` accesses no screen state). `Search::deinit` is `unsafe`
  (it calls `ScreenSearch::deinit`).
- **Deferred fields.** `last_screen` / `last_complete` /
  `stale_viewport_matches` are initialized here (faithful to the struct) but
  only read by the deferred `feed` / `notify`; the module's
  `#[allow(dead_code)]` covers them until then.
- **No `EnumMap`.** A two-slot array keyed by `TerminalScreenKey` is the minimal
  faithful stand-in for the two screen kinds.
- Registered in `search/mod.rs` as `#[allow(dead_code)] pub(crate) mod thread;`.

## Verification

- `cargo build -p roastty` — no warnings.
- `cargo test -p roastty` — no regressions; new tests:
  - `new_starts_empty_and_complete` — a fresh `Search` (no screens) is
    `is_complete()` and `tick()` returns `Complete`; the viewport needle
    round-trips.
  - `tick_reports_progress_then_blocked_then_complete` — insert a `ScreenSearch`
    over a test screen with matches; the first `tick` is `Progress` (drains
    active), a later `tick` is `Blocked` (waiting on a feed), and after driving
    the screen search to completion `tick` is `Complete` and `is_complete()`
    holds.
  - `deinit_releases_screen_search_pins` — inserting a screen search tracks
    pins; `Search::deinit` returns the screen's tracked-pin count to baseline.
  - `tick_progress_dominates_blocked` — two screens, one feed-blocked and one
    still progressing: `tick` returns `Progress` (progress dominates a blocked
    screen regardless of slot order).
- `cargo fmt -p roastty -- --check` — clean.
- no-ghostty grep on touched source — clean.
- `git diff --check` — clean.

Pass = the aggregator constructs, ticks across its screen map with the
`Complete`/`Progress`/`Blocked` precedence matching upstream, reports
completion, and tears down its screen searchers without leaking tracked pins.

## Design Review

Codex reviewed the design and **APPROVED** it with **no Required findings**,
confirming the slice boundary is sound (deferring `feed` / `notify` / the outer
event-loop `Thread` is the right decomposition given the missing libxev,
callback, viewport-dirty flag, and screen-enumeration surface), and that the
`tick` aggregation matches upstream (`Progressed` dominates everything,
`FeedRequired` only flips `Complete → Blocked`, `Complete` leaves the aggregate
unchanged; empty-map `is_complete() == true` is faithfully vacuous). Both
Optionals adopted:

- **Optional (adopted)**: `ScreenSearches::insert` now returns the replaced
  `Option<ScreenSearch>` (via `replace`), so when `feed`'s reconciliation lands
  it can `deinit` a replaced searcher rather than leaking its tracked pins.
- **Optional (adopted)**: added `tick_progress_dominates_blocked` — a two-screen
  test (one feed-blocked, one progressing) asserting `Progress` dominates
  regardless of slot order.

Codex also confirmed the two-slot array keyed by `TerminalScreenKey` is a
reasonable `EnumMap` stand-in, the `unsafe deinit` / safe `tick` boundary is
correct, and keeping the deferred fields under `#[allow(dead_code)]` is
appropriate (they are part of upstream `Search` state used by the next slices).

Review artifacts:

- Prompt: `logs/codex-review/20260605-d606-prompt.md`
- Result: `logs/codex-review/20260605-d606-last-message.md`
