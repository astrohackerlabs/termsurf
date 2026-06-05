//! Searches a whole `Screen` (port of upstream `terminal/search/screen.zig`). A state machine over
//! `ActiveSearch` (the mutable active area) and `PageListSearch` (history) that caches results so a
//! background search survives screen changes. So far it lands the state-machine vocabulary and
//! struct skeleton, the read-only result accessors (`needle` / `matches_len` / `matches`), and the
//! `tick` state machine (`tick` / `tick_active` / `tick_history`), `prune_history` (drop stale
//! cached history results), `selected_match` (read the selected result by index), the selection
//! stepping (`select_next` / `select_prev`), the construction/dispatch cluster (`new` /
//! `reload_active` / `select` / `deinit`), and the feed driver (`feed` / `search_all`). The
//! incremental, lock-aware search surface is complete.

use std::ptr::NonNull;

use super::super::highlight::{Flattened, Tracked};
use super::super::page_list::{Node, Pin};
use super::super::screen::Screen;
use super::super::size::CellCountInt;
use super::active::ActiveSearch;
use super::pagelist::PageListSearch;
use super::sliding_window::{Direction, SlidingWindow};

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

/// The currently-selected match (upstream `ScreenSearch.SelectedMatch`).
struct SelectedMatch {
    /// Index from the end of the match list (0 = most recent match).
    idx: usize,
    /// Tracked highlight so we can detect movement.
    highlight: Tracked,
}

impl SelectedMatch {
    /// Untrack the selection's pins (upstream `SelectedMatch.deinit`). Takes `self` by value (the
    /// lifecycle style of `Tracked::deinit`; the caller owns the previous selection via `take`).
    fn deinit(self, screen: &mut Screen) {
        self.highlight.deinit(screen);
    }
}

/// History (scrollback) search state (upstream `ScreenSearch.HistorySearch`).
struct HistorySearch {
    /// The actual searcher.
    searcher: PageListSearch,
    /// The pin for the first node this searcher started from (to detect active-area growth over
    /// previously-searched history).
    start_pin: NonNull<Pin>,
}

impl HistorySearch {
    /// Untrack the start pin and drop the page-list searcher (upstream `HistorySearch.deinit`).
    ///
    /// Takes `screen` by pointer (not `&mut Screen`): `PageListSearch::deinit` dereferences its own
    /// raw `NonNull<PageList>` into this same screen, so no `&mut Screen` may be live across it.
    ///
    /// # Safety
    /// `screen` must be the live screen this history searcher belongs to, exclusively accessed.
    unsafe fn deinit(self, mut screen: NonNull<Screen>) {
        let mut searcher = self.searcher;
        // SAFETY: the searcher's `list` is this screen's page list, which is alive; no `&mut Screen`
        // is held here, so this is the only access to the page list.
        unsafe { searcher.deinit() };
        // SAFETY: screen alive; the searcher access above has ended.
        unsafe { screen.as_mut() }.untrack_pin(self.start_pin);
    }
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

impl ScreenSearch {
    /// The needle this search is using (upstream `needle`). The active window is always forward, so
    /// its stored bytes are the original needle.
    pub(in crate::terminal) fn needle(&self) -> &[u8] {
        self.active.needle()
    }

    /// The total number of matches found so far (upstream `matchesLen`).
    pub(in crate::terminal) fn matches_len(&self) -> usize {
        self.active_results.len() + self.history_results.len()
    }

    /// Whether this screen's search state is complete (upstream `self.state.isComplete()`). Used by
    /// the search-thread aggregator to detect when all screens are done.
    pub(in crate::terminal) fn is_state_complete(&self) -> bool {
        self.state.is_complete()
    }

    /// Whether this search wants a feed (upstream `self.state.needsFeed()`). Used by the
    /// search-thread aggregator's `feed`.
    pub(in crate::terminal) fn needs_feed(&self) -> bool {
        self.state.needs_feed()
    }

    /// This search's backing screen pointer (upstream `screen_search.screen`). Used by the
    /// search-thread aggregator to detect when the terminal reinitialized a screen.
    pub(in crate::terminal) fn screen_ptr(&self) -> NonNull<Screen> {
        self.screen
    }

    /// The index of the currently-selected match, if any (upstream `screen_search.selected.?.idx`).
    /// The search-thread `notify` gates its selection event on this.
    pub(in crate::terminal) fn selected_index(&self) -> Option<usize> {
        self.selected.as_ref().map(|m| m.idx)
    }

    /// Force the selected match's index (test-only), used to exercise `notify`'s
    /// `selected_index() == Some` but `selected_match() == None` branch. Requires an existing
    /// selection (its tracked highlight is left intact).
    #[cfg(test)]
    pub(in crate::terminal) fn set_selected_idx_for_tests(&mut self, idx: usize) {
        self.selected.as_mut().expect("a selection must exist").idx = idx;
    }

    /// Drop the current selection without untracking its pins (test-only), used to exercise
    /// `notify`'s selection-cleared branch. The orphaned pins are released when the backing screen
    /// is dropped at the end of the test.
    #[cfg(test)]
    pub(in crate::terminal) fn clear_selection_for_tests(&mut self) {
        self.selected = None;
    }

    /// All matches, ordered newest-to-oldest (upstream `matches`): the active results (stored
    /// forward) reversed, then the history results (already newest-to-oldest) appended. Returns an
    /// owned `Vec` (Rust ownership replaces upstream's caller-frees slice).
    pub(in crate::terminal) fn matches(&self) -> Vec<Flattened> {
        let mut results =
            Vec::with_capacity(self.active_results.len() + self.history_results.len());
        results.extend(self.active_results.iter().rev().cloned());
        results.extend(self.history_results.iter().cloned());
        results
    }

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

    /// Drop cached history results whose pages have been pruned from the scrollback (upstream
    /// `pruneHistory`). History results are stored newest-to-oldest, so the first result whose first
    /// chunk's serial is below the screen's minimum live page serial marks the boundary — it and
    /// everything older are truncated.
    fn prune_history(&mut self) {
        // SAFETY: the screen is alive (the construction-time invariant).
        let min = unsafe { self.screen.as_ref() }.page_serial_min();
        for i in 0..self.history_results.len() {
            let first_chunk_serial = self.history_results[i].chunks[0].serial;
            if first_chunk_serial < min {
                self.history_results.truncate(i);
                self.history_results.shrink_to_fit(); // mirror upstream's `shrinkAndFree`
                return;
            }
        }
    }

    /// Return the currently-selected match, if any (upstream `selectedMatch`). `idx` counts from the
    /// end of the combined newest-to-oldest match list: `< active_len` indexes the (forward) active
    /// results reversed, then history results follow; out of range yields `None`. Does not access
    /// the screen.
    pub(in crate::terminal) fn selected_match(&self) -> Option<Flattened> {
        let sel = self.selected.as_ref()?;
        let active_len = self.active_results.len();
        if sel.idx < active_len {
            return Some(self.active_results[active_len - 1 - sel.idx].clone());
        }
        let history_len = self.history_results.len();
        if sel.idx < active_len + history_len {
            return Some(self.history_results[sel.idx - active_len].clone());
        }
        None
    }

    /// The cached result at `idx` (the `selected_match` indexing; the caller guarantees
    /// `idx < active_len + history_len`).
    fn result_at(&self, idx: usize) -> Flattened {
        let active_len = self.active_results.len();
        if idx < active_len {
            self.active_results[active_len - 1 - idx].clone()
        } else {
            self.history_results[idx - active_len].clone()
        }
    }

    /// Select the next match (newest→oldest, wrapping), upstream `selectNext`. `false` only if there
    /// are no matches.
    fn select_next(&mut self) -> bool {
        let total = self.active_results.len() + self.history_results.len();
        let next_idx = match &self.selected {
            None => {
                if total == 0 {
                    return false;
                }
                // The newest match is index 0.
                0
            }
            Some(m) => {
                if m.idx + 1 >= total {
                    0
                } else {
                    m.idx + 1
                }
            }
        };

        self.set_selection(next_idx);
        true
    }

    /// Select the previous match (oldest→newest, wrapping), upstream `selectPrev`. `false` only if
    /// there are no matches.
    fn select_prev(&mut self) -> bool {
        let total = self.active_results.len() + self.history_results.len();
        let next_idx = match &self.selected {
            None => {
                if total == 0 {
                    return false;
                }
                // The oldest match is the last index.
                total - 1
            }
            Some(m) => {
                if m.idx != 0 {
                    m.idx - 1
                } else {
                    total - 1
                }
            }
        };

        self.set_selection(next_idx);
        true
    }

    /// Track the result at `next_idx`, deinit any previous selection, and store the new one. Shared
    /// by `select_next` / `select_prev`; the caller guarantees `next_idx` is in range.
    fn set_selection(&mut self, next_idx: usize) {
        let hl = self.result_at(next_idx);
        // SAFETY: the screen is alive and exclusively accessed (the caller holds the screen lock —
        // upstream's `select` read/write contract).
        let screen = unsafe { self.screen.as_mut() };
        // Track first, so a (non-)failure leaves the previous selection intact. A `None` here is an
        // invariant violation (a valid cached match must have trackable pins), not a "no match".
        let tracked = hl
            .untracked()
            .track(screen)
            .expect("selected match pins must be trackable");
        if let Some(prev) = self.selected.take() {
            prev.deinit(screen);
        }
        self.selected = Some(SelectedMatch {
            idx: next_idx,
            highlight: tracked,
        });
    }

    /// Construct a screen search for `needle` and load the initial active area (upstream `init`).
    ///
    /// # Safety
    /// `screen` must be live and outlive the search; the caller holds the screen lock (no
    /// concurrent access). The search stores the pointer and dereferences it on `reload_active` /
    /// `select` / `deinit`.
    pub(in crate::terminal) unsafe fn new(screen: NonNull<Screen>, needle: &[u8]) -> ScreenSearch {
        // SAFETY: caller's contract — `screen` is live.
        let (rows, cols) = {
            let s = unsafe { screen.as_ref() };
            (s.rows(), s.cols())
        };
        let mut result = ScreenSearch {
            screen,
            active: ActiveSearch::new(needle),
            history: None,
            state: State::Active,
            selected: None,
            history_results: Vec::new(),
            active_results: Vec::new(),
            rows,
            cols,
        };
        // SAFETY: see above.
        unsafe { result.reload_active() };
        result
    }

    /// Untrack the selection and history pins and drop the searchers (upstream `deinit`). Explicit
    /// (not `Drop`) because it dereferences the `screen` pointer.
    ///
    /// # Safety
    /// As `new`. Call exactly once, before the backing `Screen` is dropped.
    pub(in crate::terminal) unsafe fn deinit(&mut self) {
        if let Some(m) = self.selected.take() {
            // SAFETY: caller's contract — `screen` is live.
            let screen = unsafe { self.screen.as_mut() };
            m.deinit(screen);
        }
        if let Some(h) = self.history.take() {
            // SAFETY: caller's contract — `screen` is live; no `&mut Screen` is held here.
            unsafe { h.deinit(self.screen) };
        }
    }

    /// Select the next/previous match after re-validating the active area and pruning stale history
    /// (upstream `select`).
    ///
    /// # Safety
    /// As `new`.
    pub(in crate::terminal) unsafe fn select(&mut self, to: Select) -> bool {
        // SAFETY: caller's contract.
        unsafe { self.reload_active() };
        self.prune_history();
        match to {
            Select::Next => self.select_next(),
            Select::Prev => self.select_prev(),
        }
    }

    /// Re-copy the active area, grow the history search, and keep the selection valid (upstream
    /// `reloadActive`). Decomposed into `reload_history` (B), `prune_no_scrollback_active` (D), and
    /// `fixup_selection` (E) — a faithful refactoring of upstream's labeled blocks.
    ///
    /// # Safety
    /// As `new`.
    pub(in crate::terminal) unsafe fn reload_active(&mut self) {
        // (A) Selection-garbage recovery: if either of the selection's tracked pins went garbage,
        // drop the selection now and re-select the last match after the body runs.
        let mut select_prev_recovery = false;
        if let Some(m) = self.selected.as_ref() {
            // SAFETY: screen alive.
            let screen = unsafe { self.screen.as_ref() };
            let garbage = screen
                .tracked_pin_value(m.highlight.start)
                .map_or(true, |p| p.is_garbage())
                || screen
                    .tracked_pin_value(m.highlight.end)
                    .map_or(true, |p| p.is_garbage());
            if garbage {
                let m = self.selected.take().unwrap();
                // SAFETY: screen alive.
                let screen = unsafe { self.screen.as_mut() };
                m.deinit(screen);
                select_prev_recovery = true;
            }
        }

        // (B) Active update + history growth.
        let history_node = {
            // SAFETY: nodes come from the live screen; `update` stores them under the search
            // contract (screen alive + lock held).
            let list = unsafe { self.screen.as_ref() }.pages();
            unsafe { self.active.update(list) }
        };
        // SAFETY: as `reload_active`.
        unsafe { self.reload_history(history_node) };

        // (C) Re-run the active-area search, preserving any non-active state.
        let old_active_len = self.active_results.len();
        let old_selection_idx = self.selected.as_ref().map(|m| m.idx);
        self.active_results.clear();
        if self.state == State::Active {
            self.tick_active();
        } else {
            let saved = self.state;
            self.tick_active();
            self.state = saved;
        }

        // (D) No-scrollback active pruning.
        self.prune_no_scrollback_active();

        // (E) Selection fixup.
        // SAFETY: as `reload_active`.
        unsafe { self.fixup_selection(old_active_len, old_selection_idx) };

        // (A, deferred) Re-select the last match after a garbage recovery.
        if select_prev_recovery {
            // SAFETY: as `reload_active`.
            unsafe { self.select(Select::Prev) };
        }
    }

    /// Grow (or create/reset) the history searcher to cover everything above the active area
    /// (upstream `reloadActive`'s history block).
    ///
    /// # Safety
    /// As `new`.
    unsafe fn reload_history(&mut self, history_node: Option<NonNull<Node>>) {
        let Some(history_node) = history_node else {
            // No history node → no history. Clear it and move a history-area selection to the
            // active end.
            if let Some(h) = self.history.take() {
                // SAFETY: screen alive; no `&mut Screen` is held here.
                unsafe { h.deinit(self.screen) };
            }
            self.history_results.clear();
            if let Some(m) = self.selected.as_ref() {
                if m.idx >= self.active_results.len() {
                    let m = self.selected.take().unwrap();
                    // SAFETY: screen alive.
                    let screen = unsafe { self.screen.as_mut() };
                    m.deinit(screen);
                    // SAFETY: as `reload_active`.
                    unsafe { self.select(Select::Prev) };
                }
            }
            return;
        };

        // SAFETY: screen alive.
        if unsafe { self.screen.as_ref() }.no_scrollback() {
            debug_assert!(self.history.is_none());
            return;
        }

        // Reset the history if its start pin went garbage.
        if let Some(h) = self.history.as_ref() {
            // SAFETY: screen alive.
            let garbage = unsafe { self.screen.as_ref() }
                .tracked_pin_value(h.start_pin)
                .map_or(true, |p| p.is_garbage());
            if garbage {
                let h = self.history.take().unwrap();
                // SAFETY: screen alive; no `&mut Screen` is held here.
                unsafe { h.deinit(self.screen) };
                self.history_results.clear();
            }
        }

        // No history yet → create one rooted at `history_node`.
        if self.history.is_none() {
            let needle = self.needle().to_vec();
            // SAFETY: `history_node` is a live node; the screen is alive.
            let searcher = unsafe {
                let list = self.screen.as_mut().pages_mut();
                PageListSearch::new(&needle, list, history_node)
            }
            .expect("history start node must be trackable");
            let start_pin = unsafe { self.screen.as_mut() }
                .track_pin(Pin::new(history_node, 0, 0))
                .expect("history start node must be trackable");
            self.history = Some(HistorySearch {
                searcher,
                start_pin,
            });
            return;
        }

        // Existing history: if the start node is unchanged, there is nothing to grow.
        let start_pin = self.history.as_ref().unwrap().start_pin;
        // SAFETY: the start pin is tracked in the live screen.
        if unsafe { start_pin.as_ref() }.node() == history_node {
            return;
        }

        // Grown: forward-search `[start_node ..= history_node]`, collect matches that do not start
        // on the active-covering node, reverse, and prepend to the existing history results.
        let needle = self.needle().to_vec();
        let mut window = SlidingWindow::new(Direction::Forward, &needle);
        loop {
            // SAFETY: the start pin is tracked in the live screen.
            let node = unsafe { start_pin.as_ref() }.node();
            // SAFETY: `node` is a live page-list node.
            unsafe { window.append(node) };
            if node == history_node {
                break;
            }
            // SAFETY: screen alive.
            let Some(next) = unsafe { self.screen.as_ref() }.pages().next_node_ptr(node) else {
                break;
            };
            let mut sp = start_pin;
            // SAFETY: the start pin is tracked in the live screen; advancing its node keeps it
            // valid (a real page-list node).
            unsafe { sp.as_mut() }.set_node(next);
        }
        // The walk must have reached the active-covering node (upstream asserts this); otherwise the
        // collected results and the advanced `start_pin` would be garbage. A hard assert (vs
        // upstream's debug-only one) keeps a contract violation from silently corrupting history.
        // SAFETY: the start pin is tracked in the live screen.
        assert!(
            unsafe { start_pin.as_ref() }.node() == history_node,
            "history walk must reach the active-covering node",
        );

        let mut results: Vec<Flattened> = Vec::new();
        while let Some(hl) = window.next() {
            // Skip matches that start on the active-covering node — those belong to the active area.
            if hl.chunks[0].node == history_node {
                continue;
            }
            results.push(hl);
        }

        // No new matches → nothing changes in the history (fast path).
        if results.is_empty() {
            return;
        }

        let added_len = results.len();
        results.reverse();
        results.extend(self.history_results.drain(..));
        self.history_results = results;

        // A history-area selection shifts by the number of newly prepended matches.
        if let Some(m) = self.selected.as_mut() {
            if m.idx >= self.active_results.len() {
                m.idx += added_len;
            }
        }
    }

    /// In a no-scrollback screen, drop active results that are not actually in the active area
    /// (upstream `reloadActive`'s no-scrollback block).
    fn prune_no_scrollback_active(&mut self) {
        // SAFETY: screen alive.
        if !unsafe { self.screen.as_ref() }.no_scrollback() || self.active_results.is_empty() {
            return;
        }
        // SAFETY: screen alive.
        let tl = unsafe { self.screen.as_ref() }.active_area_top_left();
        let mut boundary = None;
        for (i, hl) in self.active_results.iter().enumerate() {
            // A result is in the active area iff the active top-left is before its end pin.
            // SAFETY: screen alive.
            let in_active = unsafe { self.screen.as_ref() }
                .pin_before(tl, hl.end_pin())
                .unwrap_or(false);
            if in_active {
                boundary = Some(i);
                break;
            }
        }
        match boundary {
            Some(i) => {
                self.active_results.drain(..i);
            }
            None => self.active_results.clear(),
        }
    }

    /// Re-resolve the selection's index against the freshly-rebuilt active results (upstream
    /// `reloadActive`'s selection-fixup block).
    ///
    /// # Safety
    /// As `new`.
    unsafe fn fixup_selection(&mut self, old_active_len: usize, old_selection_idx: Option<usize>) {
        let Some(old_idx) = old_selection_idx else {
            return;
        };
        if self.selected.is_none() {
            return;
        }

        if old_idx >= old_active_len {
            // History-area selection: shift by the change in active-area length.
            let active_len = self.active_results.len();
            let m = self.selected.as_mut().unwrap();
            m.idx = m.idx - old_active_len + active_len;
            return;
        }

        // Active-area selection: re-find it by comparing the selection's CURRENT tracked pin values
        // against each candidate's untracked endpoints.
        let (start_val, end_val) = {
            // SAFETY: screen alive.
            let screen = unsafe { self.screen.as_ref() };
            let m = self.selected.as_ref().unwrap();
            (
                screen.tracked_pin_value(m.highlight.start),
                screen.tracked_pin_value(m.highlight.end),
            )
        };
        let (Some(start_val), Some(end_val)) = (start_val, end_val) else {
            // Missing/garbage tracked pin → treat as not found.
            // SAFETY: as `reload_active`.
            unsafe { self.drop_selection_and_select_next() };
            return;
        };

        let active_len = self.active_results.len();
        for i in 0..active_len {
            let untracked = self.active_results[i].untracked();
            if untracked.start == start_val && untracked.end == end_val {
                self.selected.as_mut().unwrap().idx = active_len - 1 - i;
                return;
            }
        }

        // Not found in the new active results.
        // SAFETY: as `reload_active`.
        unsafe { self.drop_selection_and_select_next() };
    }

    /// Drop the current selection and advance to the next match (upstream's not-found recovery).
    ///
    /// # Safety
    /// As `new`.
    unsafe fn drop_selection_and_select_next(&mut self) {
        let m = self.selected.take().unwrap();
        // SAFETY: screen alive.
        let screen = unsafe { self.screen.as_mut() };
        m.deinit(screen);
        // SAFETY: as `reload_active`.
        unsafe { self.select(Select::Next) };
    }

    /// Tick and feed until the search is complete (upstream `searchAll`). Drives the state machine
    /// to `Complete`, pulling more history whenever a `tick` requests a feed.
    ///
    /// # Safety
    /// As `feed` — the caller holds the screen lock and the screen outlives the search.
    pub(in crate::terminal) unsafe fn search_all(&mut self) {
        loop {
            match self.tick() {
                Tick::Progressed => {}
                // SAFETY: caller's contract.
                Tick::FeedRequired => unsafe { self.feed() },
                Tick::Complete => return,
            }
        }
    }

    /// Pull one page of history into the searcher, resetting the whole search on a resize (upstream
    /// `feed`). Accesses screen state.
    ///
    /// # Safety
    /// The caller holds the screen lock; the screen is live and outlives the search.
    pub(in crate::terminal) unsafe fn feed(&mut self) {
        // (1) A resize can't be reflowed into the cached results, so reset the whole search.
        let (cur_rows, cur_cols) = {
            // SAFETY: screen alive.
            let s = unsafe { self.screen.as_ref() };
            (s.rows(), s.cols())
        };
        if cur_rows != self.rows || cur_cols != self.cols {
            let screen = self.screen;
            let needle = self.needle().to_vec();
            // SAFETY: screen alive. `new` runs `reload_active`, creating fresh tracked pins; the old
            // pins (still tracked) are released by `deinit` immediately below — they briefly
            // coexist, exactly as upstream's `init` then `self.deinit()`.
            let new = unsafe { ScreenSearch::new(screen, &needle) };
            // SAFETY: screen alive; release the old pins before overwriting.
            unsafe { self.deinit() };
            *self = new;
            debug_assert!(self.rows == cur_rows && self.cols == cur_cols);
        }

        // (2) No history searcher → nothing left to feed.
        if self.history.is_none() {
            self.state = State::Complete;
            return;
        }

        // (3) Feed one page. No `&mut Screen` is held across this — `PageListSearch::feed`
        // dereferences its own `NonNull<PageList>` into the same screen (cf.
        // `HistorySearch::deinit`).
        // SAFETY: screen alive; the searcher's list is the live screen's page list.
        let fed = unsafe { self.history.as_mut().unwrap().searcher.feed() };
        if !fed {
            // No more data → complete; reclaim scrollback-pruned history results.
            self.state = State::Complete;
            self.prune_history();
            return;
        }

        // (4) A successful feed while waiting resumes the history search; active/history unchanged.
        match self.state {
            State::Active | State::History => {}
            State::HistoryFeed => self.state = State::History,
            State::Complete => unreachable!("a complete search's feed returns no data"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::highlight::Chunk;
    use super::super::super::page_list::{Node, PageList};
    use super::super::super::screen::Screen;
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

    /// A minimal `Flattened` distinguished by its `top_x`.
    fn flat(top_x: CellCountInt) -> Flattened {
        Flattened {
            chunks: Vec::new(),
            top_x,
            bot_x: 0,
        }
    }

    /// Build a `ScreenSearch` directly with populated result lists. The accessors never dereference
    /// `screen` (so it can be dangling) and there are no tracked pins (so dropping is safe).
    fn build(active: Vec<Flattened>, history: Vec<Flattened>) -> ScreenSearch {
        ScreenSearch {
            screen: NonNull::dangling(),
            active: ActiveSearch::new(b"foo"),
            history: None,
            state: State::Active,
            selected: None,
            history_results: history,
            active_results: active,
            rows: 10,
            cols: 10,
        }
    }

    #[test]
    fn needle_returns_the_active_needle() {
        let s = build(Vec::new(), Vec::new());
        assert_eq!(s.needle(), b"foo");
    }

    #[test]
    fn matches_len_sums_both_lists() {
        let s = build(vec![flat(1), flat(2)], vec![flat(10), flat(11)]);
        assert_eq!(s.matches_len(), 4);

        let empty = build(Vec::new(), Vec::new());
        assert_eq!(empty.matches_len(), 0);
    }

    #[test]
    fn matches_orders_newest_to_oldest() {
        // Active is stored forward (oldest-to-newest); history is newest-to-oldest.
        let s = build(vec![flat(1), flat(2)], vec![flat(10), flat(11)]);
        let order: Vec<CellCountInt> = s.matches().iter().map(|h| h.top_x).collect();
        // Reversed active, then history.
        assert_eq!(order, vec![2, 1, 10, 11]);

        let empty = build(Vec::new(), Vec::new());
        assert!(empty.matches().is_empty());
    }

    #[test]
    fn tick_drains_active_then_completes_with_no_history() {
        let mut list = PageList::init(10, 10, None).unwrap();
        list.set_screen_text_lines_for_tests(&["Fizz"]);
        let mut active = ActiveSearch::new(b"Fizz");
        // SAFETY: `list` outlives `s`; its pages are not mutated.
        unsafe { active.update(&list) };

        let mut s = ScreenSearch {
            screen: NonNull::dangling(),
            active,
            history: None,
            state: State::Active,
            selected: None,
            history_results: Vec::new(),
            active_results: Vec::new(),
            rows: 10,
            cols: 10,
        };

        // First tick drains the active area.
        assert_eq!(s.tick(), Tick::Progressed);
        assert_eq!(s.matches_len(), 1);
        // Second tick: no history -> complete.
        assert_eq!(s.tick(), Tick::Progressed);
        // Now complete.
        assert_eq!(s.tick(), Tick::Complete);
    }

    #[test]
    fn tick_in_history_feed_requests_a_feed() {
        let mut s = build(Vec::new(), Vec::new());
        s.state = State::HistoryFeed;
        assert_eq!(s.tick(), Tick::FeedRequired);
    }

    #[test]
    fn tick_when_complete_reports_complete() {
        let mut s = build(Vec::new(), Vec::new());
        s.state = State::Complete;
        assert_eq!(s.tick(), Tick::Complete);
    }

    /// A `Flattened` whose single chunk carries `serial` (its `node` is dangling — `prune_history`
    /// reads only the serial).
    fn flat_serial(serial: u64) -> Flattened {
        Flattened {
            chunks: vec![Chunk {
                node: NonNull::dangling(),
                serial,
                start: 0,
                end: 0,
            }],
            top_x: 0,
            bot_x: 0,
        }
    }

    /// Build a `ScreenSearch` over a real `screen` (so `prune_history` can read `page_serial_min`)
    /// with the given history results.
    fn build_over_screen(screen: &Screen, history: Vec<Flattened>) -> ScreenSearch {
        ScreenSearch {
            screen: NonNull::from(screen),
            active: ActiveSearch::new(b"x"),
            history: None,
            state: State::Active,
            selected: None,
            history_results: history,
            active_results: Vec::new(),
            rows: 10,
            cols: 10,
        }
    }

    fn history_serials(s: &ScreenSearch) -> Vec<u64> {
        s.history_results
            .iter()
            .map(|h| h.chunks[0].serial)
            .collect()
    }

    #[test]
    fn prune_history_drops_from_the_first_stale_result() {
        let mut screen = Screen::init(10, 10, None).unwrap();
        screen.set_page_serial_min_for_tests(4);

        // Newest-to-oldest serials [5, 4, 3, 2]; serial 3 < 4 marks the boundary.
        let mut s = build_over_screen(
            &screen,
            vec![
                flat_serial(5),
                flat_serial(4),
                flat_serial(3),
                flat_serial(2),
            ],
        );
        s.prune_history();
        assert_eq!(history_serials(&s), vec![5, 4]);
    }

    #[test]
    fn prune_history_keeps_all_live_results() {
        let mut screen = Screen::init(10, 10, None).unwrap();
        screen.set_page_serial_min_for_tests(0);

        let mut s = build_over_screen(&screen, vec![flat_serial(5), flat_serial(4)]);
        s.prune_history();
        assert_eq!(history_serials(&s), vec![5, 4]);
    }

    #[test]
    fn prune_history_on_empty_is_a_noop() {
        let screen = Screen::init(10, 10, None).unwrap();
        let mut s = build_over_screen(&screen, Vec::new());
        s.prune_history();
        assert!(s.history_results.is_empty());
    }

    /// Build a `ScreenSearch` with a selected match at `idx` (the tracked highlight uses dangling
    /// pins — `selected_match` never dereferences them).
    fn build_selected(idx: usize, active: Vec<Flattened>, history: Vec<Flattened>) -> ScreenSearch {
        let mut s = build(active, history);
        s.selected = Some(SelectedMatch {
            idx,
            highlight: Tracked::init_assume(NonNull::dangling(), NonNull::dangling()),
        });
        s
    }

    #[test]
    fn selected_match_indexes_the_reversed_active_area() {
        // Active is stored forward [a(1), b(2)]; idx 0 = most recent active = b.
        let s = build_selected(0, vec![flat(1), flat(2)], vec![flat(10)]);
        assert_eq!(s.selected_match().unwrap().top_x, 2);

        let s = build_selected(1, vec![flat(1), flat(2)], vec![flat(10)]);
        assert_eq!(s.selected_match().unwrap().top_x, 1);
    }

    #[test]
    fn selected_match_spills_into_history() {
        // active_len 2, so idx 2 = history[0].
        let s = build_selected(2, vec![flat(1), flat(2)], vec![flat(10)]);
        assert_eq!(s.selected_match().unwrap().top_x, 10);
    }

    #[test]
    fn selected_match_out_of_range_is_none() {
        let s = build_selected(3, vec![flat(1), flat(2)], vec![flat(10)]);
        assert!(s.selected_match().is_none());
    }

    #[test]
    fn selected_match_none_when_nothing_selected() {
        let s = build(vec![flat(1)], vec![flat(10)]);
        assert!(s.selected_match().is_none());
    }

    #[test]
    fn selected_match_with_empty_active_uses_history() {
        // active_len 0, so idx 0 = history[0].
        let s = build_selected(0, Vec::new(), vec![flat(10)]);
        assert_eq!(s.selected_match().unwrap().top_x, 10);
    }

    /// A `Flattened` whose single chunk is on `node` with a trackable in-bounds extent (so its
    /// `untracked()` pins are valid for a real screen).
    fn flat_on(node: NonNull<Node>, x: CellCountInt) -> Flattened {
        Flattened {
            chunks: vec![Chunk {
                node,
                serial: 0,
                start: 0,
                end: 1,
            }],
            top_x: x,
            bot_x: x,
        }
    }

    /// Build a `ScreenSearch` over `screen` (mutably, so `select_*` can track pins) with the given
    /// active results and no history.
    fn build_over_screen_mut(screen: &mut Screen, active: Vec<Flattened>) -> ScreenSearch {
        ScreenSearch {
            screen: NonNull::from(screen),
            active: ActiveSearch::new(b"x"),
            history: None,
            state: State::Active,
            selected: None,
            history_results: Vec::new(),
            active_results: active,
            rows: 10,
            cols: 10,
        }
    }

    fn selected_idx(s: &ScreenSearch) -> usize {
        s.selected.as_ref().unwrap().idx
    }

    #[test]
    fn select_next_picks_newest_then_steps_and_wraps() {
        let mut screen = Screen::init(10, 10, None).unwrap();
        let node = screen.first_node_ptr_for_tests();
        let baseline = screen.tracked_pin_count();
        let mut s = build_over_screen_mut(&mut screen, vec![flat_on(node, 1), flat_on(node, 2)]);

        assert!(s.select_next());
        assert_eq!(selected_idx(&s), 0); // newest
        assert!(s.select_next());
        assert_eq!(selected_idx(&s), 1);
        assert!(s.select_next());
        assert_eq!(selected_idx(&s), 0); // wrapped

        // Exactly one selection is tracked (2 pins past the baseline); the previous was deinited
        // each step.
        assert_eq!(screen.tracked_pin_count(), baseline + 2);
    }

    #[test]
    fn select_prev_picks_oldest_then_steps_and_wraps() {
        let mut screen = Screen::init(10, 10, None).unwrap();
        let node = screen.first_node_ptr_for_tests();
        let baseline = screen.tracked_pin_count();
        let mut s = build_over_screen_mut(&mut screen, vec![flat_on(node, 1), flat_on(node, 2)]);

        assert!(s.select_prev());
        assert_eq!(selected_idx(&s), 1); // oldest = total - 1
        assert!(s.select_prev());
        assert_eq!(selected_idx(&s), 0);
        assert!(s.select_prev());
        assert_eq!(selected_idx(&s), 1); // wrapped

        assert_eq!(screen.tracked_pin_count(), baseline + 2);
    }

    #[test]
    fn select_with_no_matches_returns_false() {
        let mut screen = Screen::init(10, 10, None).unwrap();
        let mut s = build_over_screen_mut(&mut screen, Vec::new());

        assert!(!s.select_next());
        assert!(s.selected.is_none());
        assert!(!s.select_prev());
        assert!(s.selected.is_none());
    }

    #[test]
    fn new_searches_the_active_area_and_releases_its_pins_on_deinit() {
        let mut screen = Screen::init(10, 10, None).unwrap();
        screen.set_text_lines_for_tests(&["Fizz buzz", "Fizz pop"]);
        let baseline = screen.tracked_pin_count();
        let ptr = NonNull::from(&mut screen);
        // SAFETY: `screen` outlives `s`; this thread holds it exclusively.
        let mut s = unsafe { ScreenSearch::new(ptr, b"Fizz") };

        assert_eq!(s.needle(), b"Fizz");
        // Two "Fizz" occurrences in the active area, both copied into the active results.
        assert_eq!(s.matches_len(), 2);
        // Construction stands up a history searcher rooted above the active area (its `start_pin`
        // plus the `PageListSearch` pin = two pins past the baseline); no selection yet.
        assert_eq!(screen.tracked_pin_count(), baseline + 2);

        // SAFETY: as `new`; called exactly once.
        unsafe { s.deinit() };
        assert_eq!(screen.tracked_pin_count(), baseline);
    }

    #[test]
    fn select_after_new_tracks_one_selection_then_deinit_releases_it() {
        let mut screen = Screen::init(10, 10, None).unwrap();
        screen.set_text_lines_for_tests(&["Fizz buzz", "Fizz pop"]);
        let baseline = screen.tracked_pin_count();
        let ptr = NonNull::from(&mut screen);
        // SAFETY: `screen` outlives `s`.
        let mut s = unsafe { ScreenSearch::new(ptr, b"Fizz") };

        // SAFETY: as `new`.
        assert!(unsafe { s.select(Select::Next) });
        assert!(s.selected.is_some());
        // History searcher (two pins) plus one selection (two pins) past the baseline.
        assert_eq!(screen.tracked_pin_count(), baseline + 4);

        // SAFETY: as `new`; called once.
        unsafe { s.deinit() };
        assert_eq!(screen.tracked_pin_count(), baseline);
    }

    #[test]
    fn new_with_no_matches_finds_nothing_and_selects_nothing() {
        let mut screen = Screen::init(10, 10, None).unwrap();
        screen.set_text_lines_for_tests(&["hello"]);
        let baseline = screen.tracked_pin_count();
        let ptr = NonNull::from(&mut screen);
        // SAFETY: `screen` outlives `s`.
        let mut s = unsafe { ScreenSearch::new(ptr, b"zzz") };

        assert_eq!(s.matches_len(), 0);
        // SAFETY: as `new`.
        assert!(!unsafe { s.select(Select::Next) });
        assert!(s.selected.is_none());

        // SAFETY: as `new`; called once.
        unsafe { s.deinit() };
        assert_eq!(screen.tracked_pin_count(), baseline);
    }

    #[test]
    fn search_all_reaches_complete_and_keeps_active_matches() {
        let mut screen = Screen::init(10, 10, None).unwrap();
        screen.set_text_lines_for_tests(&["Fizz buzz", "Fizz pop"]);
        let baseline = screen.tracked_pin_count();
        let ptr = NonNull::from(&mut screen);
        // SAFETY: `screen` outlives `s`.
        let mut s = unsafe { ScreenSearch::new(ptr, b"Fizz") };

        // SAFETY: as `new`.
        unsafe { s.search_all() };
        // Driven to completion, with the two active-area matches intact.
        assert_eq!(s.tick(), Tick::Complete);
        assert_eq!(s.matches_len(), 2);

        // SAFETY: as `new`; called once.
        unsafe { s.deinit() };
        assert_eq!(screen.tracked_pin_count(), baseline);
    }

    #[test]
    fn feed_repeatedly_reaches_complete() {
        let mut screen = Screen::init(10, 10, None).unwrap();
        screen.set_text_lines_for_tests(&["Fizz"]);
        let baseline = screen.tracked_pin_count();
        let ptr = NonNull::from(&mut screen);
        // SAFETY: `screen` outlives `s`.
        let mut s = unsafe { ScreenSearch::new(ptr, b"Fizz") };

        // A single-page screen has no history pages to feed, so feeding completes the search; the
        // call is idempotent once complete.
        for _ in 0..3 {
            // SAFETY: as `new`.
            unsafe { s.feed() };
        }
        assert_eq!(s.tick(), Tick::Complete);
        assert_eq!(s.matches_len(), 1);

        // SAFETY: as `new`; called once.
        unsafe { s.deinit() };
        assert_eq!(screen.tracked_pin_count(), baseline);
    }

    #[test]
    fn feed_from_history_feed_resumes_history_search() {
        // A two-page screen leaves a real history page above the active area, so the first feed
        // returns data and resumes (rather than completes) the history search.
        let mut screen = Screen::init(10, 3, Some(100)).unwrap();
        screen.pages_mut().grow_to_two_pages_for_tests();
        screen.pages_mut().set_page_row0_text_for_tests(1, "Fizz");
        screen.pages_mut().set_page_row0_text_for_tests(0, "Fizz");
        let baseline = screen.tracked_pin_count();
        let ptr = NonNull::from(&mut screen);
        // SAFETY: `screen` outlives `s`.
        let mut s = unsafe { ScreenSearch::new(ptr, b"Fizz") };

        // Tick until the state machine needs a feed.
        let mut guard = 0;
        while s.tick() != Tick::FeedRequired {
            guard += 1;
            assert!(guard < 100, "search should reach FeedRequired");
        }

        // Feeding the history page resumes the history search rather than completing it.
        // SAFETY: as `new`.
        unsafe { s.feed() };
        assert_eq!(s.tick(), Tick::Progressed);

        // SAFETY: as `new`; called once.
        unsafe { s.deinit() };
        assert_eq!(screen.tracked_pin_count(), baseline);
    }
}
