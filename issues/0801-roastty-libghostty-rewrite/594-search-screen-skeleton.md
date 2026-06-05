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

# Experiment 594: search ScreenSearch skeleton (state machine vocabulary + struct)

## Description

This experiment opens `ScreenSearch` — upstream `terminal/search/screen.zig`
(~1552 lines), the largest searcher. It searches a whole `Screen`, combining
`ActiveSearch` (the mutable active area) and `PageListSearch` (history), caching
results so a background search survives screen changes (e.g. alt-screen
toggles). It is a state machine over those two lower-level searchers.

At its size, `ScreenSearch` lands across several slices. This first slice ports
the **state-machine vocabulary and the struct skeleton**: the `State` enum (with
`is_complete` / `needs_feed`), the `Select` enum (`next` / `prev`), and the
field definitions of `ScreenSearch`, `SelectedMatch`, and `HistorySearch`. The
construction (`init` / `reloadActive`) and the search/select/feed logic are
deferred to later slices. It creates a new `terminal::search::screen` module.

## Upstream behavior

```zig
pub const ScreenSearch = struct {
    screen: *Screen,
    active: ActiveSearch,
    history: ?HistorySearch,
    state: State,
    selected: ?SelectedMatch = null,
    history_results: std.ArrayList(FlattenedHighlight),
    active_results: std.ArrayList(FlattenedHighlight),
    rows: size.CellCountInt,
    cols: size.CellCountInt,

    pub const SelectedMatch = struct {
        idx: usize,                  // index from the end of the match list (0 = most recent)
        highlight: TrackedHighlight, // tracked so we can detect movement
        pub fn deinit(self, screen) void { self.highlight.deinit(screen); }
    };

    const HistorySearch = struct {
        searcher: PageListSearch,
        start_pin: *Pin,             // first node this searcher started from
        pub fn deinit(self, screen) void { self.searcher.deinit(); screen.pages.untrackPin(self.start_pin); }
    };

    const State = enum {
        active, history, history_feed, complete,
        pub fn isComplete(self) bool { return self == .complete; }
        pub fn needsFeed(self) bool { return self == .history_feed or self == .complete; }
    };

    pub const Select = enum { next, prev };
};
```

- `State` is the search's position in the active → history → complete pipeline:
  `active` (searching the active area), `history` (searching scrollback),
  `history_feed` (waiting for the next history page to be fed), `complete` (done
  given the current terminal state). `isComplete` is `complete`; `needsFeed` is
  `history_feed` **or** `complete` (a complete search still prunes stale history
  results on the next feed).
- `Select` is the direction for stepping the selected match: `next` (newest to
  oldest, non-wrapping) or `prev` (oldest to newest).
- `SelectedMatch` tracks the currently-selected result with a `TrackedHighlight`
  (tracked pins) so it follows the content as the screen changes; `idx` is its
  index from the end of the combined match list.
- `HistorySearch` bundles the `PageListSearch` with the `start_pin` it began
  from (used to detect whether the active area has grown over
  previously-searched history).

## Rust mapping (`roastty/src/terminal/search/screen.rs`, new module)

The two enums and the three struct field definitions. The `*Screen` / `*Pin`
become `NonNull` (the same pointer model as `PageListSearch`); the `ArrayList`s
become `Vec`; `TrackedHighlight` is roastty's `highlight::Tracked`. The `deinit`
methods and all behavior beyond the enum predicates are deferred to the
construction/teardown slice (Rust `Drop` cannot subsume them here because they
need the `Screen` to untrack pins — they will become explicit, like
`PageListSearch::deinit`).

```rust
//! Searches a whole `Screen` (port of upstream `terminal/search/screen.zig`). A state machine over
//! `ActiveSearch` (the mutable active area) and `PageListSearch` (history) that caches results so a
//! background search survives screen changes. This first slice lands the state-machine vocabulary
//! and the struct skeleton; construction and the search/select/feed logic are deferred.

use std::ptr::NonNull;

use super::super::highlight::{Flattened, Tracked};
use super::super::page_list::Pin;
use super::super::screen::Screen;
use super::super::size::CellCountInt;
use super::active::ActiveSearch;
use super::pagelist::PageListSearch;

/// The search state machine's position (upstream `ScreenSearch.State`). Module-private, like
/// upstream's `State` (private to `ScreenSearch`); same-module tests exercise its predicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Searching the active area.
    Active,
    /// Searching the history (scrollback) area.
    History,
    /// History search is waiting for more data to be fed before it can progress.
    HistoryFeed,
    /// Search is complete given the current terminal state.
    Complete,
}

impl State {
    /// Whether the search is complete (upstream `isComplete`).
    fn is_complete(self) -> bool {
        matches!(self, State::Complete)
    }

    /// Whether the search wants a `feed` (upstream `needsFeed`): `HistoryFeed`, or `Complete` (a
    /// complete search still prunes stale history results on the next feed).
    fn needs_feed(self) -> bool {
        matches!(self, State::HistoryFeed | State::Complete)
    }
}

/// The direction to step the selected match (upstream `ScreenSearch.Select`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::terminal) enum Select {
    /// Next selection, newest to oldest, non-wrapping.
    Next,
    /// Previous selection, oldest to newest, non-wrapping.
    Prev,
}

/// The currently-selected match (upstream `ScreenSearch.SelectedMatch`).
struct SelectedMatch {
    /// Index from the end of the match list (0 = most recent match).
    idx: usize,
    /// Tracked highlight so we can detect movement.
    highlight: Tracked,
}

/// History (scrollback) search state (upstream `ScreenSearch.HistorySearch`).
struct HistorySearch {
    /// The actual searcher.
    searcher: PageListSearch,
    /// The pin for the first node this searcher started from (to detect active-area growth over
    /// previously-searched history).
    start_pin: NonNull<Pin>,
}

/// Searches a needle within a whole `Screen`, caching results across screen changes (upstream
/// `ScreenSearch`).
pub(crate) struct ScreenSearch {
    /// The screen being searched (upstream `*Screen`).
    screen: NonNull<Screen>,
    /// The active-area search state.
    active: ActiveSearch,
    /// The history search state (`None` if there is no history yet).
    history: Option<HistorySearch>,
    /// The state machine's current position.
    state: State,
    /// The currently-selected match, if any.
    selected: Option<SelectedMatch>,
    /// History results (mostly immutable once found; reverse order, newest to oldest).
    history_results: Vec<Flattened>,
    /// Active-area results (may change on re-search; forward order).
    active_results: Vec<Flattened>,
    /// Screen dimensions; a change restarts the whole search.
    rows: CellCountInt,
    cols: CellCountInt,
}
```

## Scope / faithfulness notes

- **Ported**: `State` (+ `isComplete` / `needsFeed`) → `State` (+ `is_complete`
  / `needs_feed`); `Select` → `Select`; the `ScreenSearch` / `SelectedMatch` /
  `HistorySearch` field definitions.
- **Faithful**: the four states and the two predicates (`is_complete` =
  `Complete`; `needs_feed` = `HistoryFeed` or `Complete`); the two `Select`
  variants; and the struct fields (`screen` / `active` / `history` / `state` /
  `selected` / `history_results` / `active_results` / `rows` / `cols`;
  `SelectedMatch`'s `idx` / `highlight`; `HistorySearch`'s `searcher` /
  `start_pin`).
- **Faithful adaptation**: `*Screen` / `*Pin` → `NonNull` (the `PageListSearch`
  pointer model); `std.ArrayList(Flattened)` → `Vec<Flattened>`;
  `TrackedHighlight` → `highlight::Tracked`; the `SelectedMatch.deinit` /
  `HistorySearch.deinit` (which untrack pins via the `Screen`) are deferred to
  the construction/teardown slice and will be explicit (not `Drop`), like
  `PageListSearch::deinit`.
- **Deferred**: everything else in `ScreenSearch` — `init` / `reloadActive`, the
  `active` / `history` / `history_feed` / `complete` transitions, `matches` /
  `matchesLen` / `needle`, `select` / `selectNext` / `selectPrev`, `feed`,
  `pruneHistory`, and the `deinit`s — plus `ViewportSearch` and the search
  `Thread`.
- No C ABI/header/ABI-inventory change (internal Rust). Creates the
  `terminal::search::screen` module.

## Changes

1. `roastty/src/terminal/search/screen.rs` (new): the module doc comment, the
   `State` enum (+ `is_complete` / `needs_feed`), the `Select` enum, and the
   `ScreenSearch` / `SelectedMatch` / `HistorySearch` struct field definitions.
2. `roastty/src/terminal/search/mod.rs`: declare
   `#[allow(dead_code)] pub(crate) mod screen;`.
3. Tests (in `screen.rs`):
   - **`is_complete`**: `Complete` is complete; `Active` / `History` /
     `HistoryFeed` are not.
   - **`needs_feed`**: `HistoryFeed` and `Complete` need a feed; `Active` /
     `History` do not.
   - **enum traits**: `State` and `Select` are `Copy` and compare by value
     (distinct variants differ).
4. Format and test (`cargo fmt`, accept output).

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

- `State` / `Select` and the struct field definitions reproduce upstream's
  vocabulary (the four states + two predicates, the two select directions, and
  the `ScreenSearch` / `SelectedMatch` / `HistorySearch` fields) — faithful to
  `terminal/search/screen.zig`;
- the tests pass (`is_complete` / `needs_feed` / enum traits), and the existing
  tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the state predicates, the select directions, or the
struct fields diverge from upstream, an unrelated item changes, or any public C
API/ABI changes.

## Design Review

Codex reviewed the design and **approved it**, confirming the slice boundary is
sound: opening `ScreenSearch` with the state vocabulary + field skeleton matches
the incremental pattern used for the other subsystems; deferring
construction/`deinit` is right (those need the reload/selection/tracked-pin
mechanics); `NonNull<Screen>` is the right raw-pointer analogue for `*Screen`
(the unsafe lifecycle contract lands with construction);
`needs_feed == HistoryFeed || Complete` is faithful to upstream's stale-history
pruning; and the module-level `#[allow(dead_code)]` is acceptable for the
skeleton. One Optional, adopted (which moots the Nit):

- **Optional (adopted)**: keep `State` (and its predicates) **private to
  `screen.rs`** rather than `pub(in crate::terminal)` — upstream's `State` is
  private inside `ScreenSearch`, and same-module tests can still exercise
  `is_complete` / `needs_feed`. `Select` stays `pub(in crate::terminal)` because
  upstream exposes it.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d594-prompt.md`
- Result: `logs/codex-review/20260604-d594-last-message.md`

## Result

**Result:** Pass

A new `terminal::search::screen` module landed, declared
`pub(crate) mod screen;`. It holds the state-machine vocabulary and the struct
skeleton: the module-private `State` enum (`Active` / `History` / `HistoryFeed`
/ `Complete`) with `is_complete` (`Complete`) and `needs_feed` (`HistoryFeed` or
`Complete`); the `pub(in crate::terminal)` `Select` enum (`Next` / `Prev`); and
the field definitions of `ScreenSearch` (`screen` as `NonNull<Screen>`,
`active`, `history`, `state`, `selected`, the two `Vec<Flattened>` result lists,
`rows` / `cols`), `SelectedMatch` (`idx` / `highlight: Tracked`), and
`HistorySearch` (`searcher` / `start_pin`). Construction, teardown, and the
search/select/feed logic are deferred. The struct's currently-unused fields rely
on the module's `#[allow(dead_code)]`.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3278 passed, 0 failed (three new tests; no
  regressions, up from 3275).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + terminal/search +
  lib.rs/header/abi_harness.c) clean; `git diff --check` clean.

The three new tests: `is_complete` (`Complete` true, others false), `needs_feed`
(`HistoryFeed` / `Complete` true, `Active` / `History` false), and `Copy` /
equality for both enums.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no Required
or Optional findings** (one Nit: the `## Result` / `## Conclusion` sections were
not yet saved — added here). Codex confirmed the skeleton is faithful: the four
states and predicates match upstream, `Select` has the two exposed directions,
and the `ScreenSearch` / `SelectedMatch` / `HistorySearch` fields map correctly
to the upstream vocabulary; `State` being module-private is the right adopted
adjustment.

Review artifacts:

- Prompt: `logs/codex-review/20260604-r594-prompt.md` (result)
- Result: `logs/codex-review/20260604-r594-last-message.md` (result)

## Conclusion

This experiment opens `ScreenSearch` — the largest searcher (~1552 lines) — with
its state-machine vocabulary (`State` + `Select`) and the `ScreenSearch` /
`SelectedMatch` / `HistorySearch` struct skeleton, following the
vocabulary-first pattern used for the other subsystems. The next slices build
the behavior on this skeleton: `init` / `reloadActive` (construct the searcher
and load the active area, the trickiest piece — it diffs the new active top
against the previous history start to decide whether to re-search), then the
`active` → `history` → `history_feed` → `complete` state transitions and `feed`,
the result accessors (`matches` / `matchesLen` / `needle`), and `select` /
`selectNext` / `selectPrev` (stepping the tracked selected match). After
`ScreenSearch`, `ViewportSearch` (`search/viewport.zig`) and the search `Thread`
remain.
