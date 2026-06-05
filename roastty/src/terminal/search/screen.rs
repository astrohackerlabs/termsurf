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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_is_complete() {
        assert!(State::Complete.is_complete());
        assert!(!State::Active.is_complete());
        assert!(!State::History.is_complete());
        assert!(!State::HistoryFeed.is_complete());
    }

    #[test]
    fn state_needs_feed() {
        assert!(State::HistoryFeed.needs_feed());
        assert!(State::Complete.needs_feed());
        assert!(!State::Active.needs_feed());
        assert!(!State::History.needs_feed());
    }

    #[test]
    fn enums_are_copy_and_eq() {
        let s = State::History;
        let copy = s;
        assert_eq!(s, copy);
        assert_ne!(State::Active, State::History);

        let d = Select::Next;
        let copy = d;
        assert_eq!(d, copy);
        assert_ne!(Select::Next, Select::Prev);
    }
}
