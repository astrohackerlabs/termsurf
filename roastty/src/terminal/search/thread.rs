//! The search-thread aggregator (port of the `Search` struct in upstream `terminal/search/Thread.zig`).
//!
//! This lands the lock-aware, multi-screen orchestration core: a `ViewportSearch` plus a
//! `ScreenSearch` per terminal screen, with `new` / `deinit` / `is_complete` / `tick`. The
//! aggregator's `feed` (terminal integration: the `search_viewport_dirty` flag and screen
//! reconciliation) and `notify` (the `Event` / `EventCallback` machinery), and the outer libxev
//! event-loop `Thread`, are deferred to later slices — roastty has no libxev port yet.

use std::ptr::NonNull;

use super::super::highlight::Untracked;
use super::super::terminal::{Terminal, TerminalScreenKey};
use super::screen::{ScreenSearch, Tick as ScreenTick};
use super::viewport::ViewportSearch;

/// The number of screen kinds (`TerminalScreenKey`: `Primary`, `Alternate`).
const SCREEN_KEY_COUNT: usize = 2;

fn key_index(key: TerminalScreenKey) -> usize {
    match key {
        TerminalScreenKey::Primary => 0,
        TerminalScreenKey::Alternate => 1,
    }
}

/// Per-screen searchers keyed by `TerminalScreenKey` (upstream's `EnumMap<Key, ScreenSearch>`,
/// modelled as a two-slot array since roastty has exactly two screen kinds and no `EnumMap`).
#[derive(Default)]
struct ScreenSearches {
    entries: [Option<ScreenSearch>; SCREEN_KEY_COUNT],
}

impl ScreenSearches {
    fn get(&self, key: TerminalScreenKey) -> Option<&ScreenSearch> {
        self.entries[key_index(key)].as_ref()
    }

    fn get_mut(&mut self, key: TerminalScreenKey) -> Option<&mut ScreenSearch> {
        self.entries[key_index(key)].as_mut()
    }

    /// Insert, returning any replaced searcher so the caller can `deinit` it (avoids leaking the
    /// replaced searcher's tracked pins once `feed`'s reconciliation lands).
    fn insert(&mut self, key: TerminalScreenKey, s: ScreenSearch) -> Option<ScreenSearch> {
        self.entries[key_index(key)].replace(s)
    }

    fn take(&mut self, key: TerminalScreenKey) -> Option<ScreenSearch> {
        self.entries[key_index(key)].take()
    }

    fn iter(&self) -> impl Iterator<Item = &ScreenSearch> {
        self.entries.iter().filter_map(|e| e.as_ref())
    }

    fn iter_mut(&mut self) -> impl Iterator<Item = &mut ScreenSearch> {
        self.entries.iter_mut().filter_map(|e| e.as_mut())
    }
}

/// State captured at the last screen switch (upstream `Search.ScreenState`). Initialized by `new`;
/// read by the deferred `feed` / `notify`.
struct ScreenState {
    key: TerminalScreenKey,
    total: Option<usize>,
    selected: Option<SelectedMatch>,
}

/// The last-notified selected match (upstream `Search.ScreenState.SelectedMatch`).
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
/// drives one `ScreenSearch` per terminal screen plus one `ViewportSearch` for the active screen.
pub(crate) struct Search {
    /// Viewport search for the active screen.
    viewport: ViewportSearch,
    /// The searchers for all the screens.
    screens: ScreenSearches,
    /// State collected at the last screen switch (so a switch invalidates it all at once).
    last_screen: ScreenState,
    /// Whether the "complete" notification has been sent.
    last_complete: bool,
    /// Whether the last viewport matches are stale and need recomputing.
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
            last_screen: ScreenState {
                key: TerminalScreenKey::Primary,
                total: None,
                selected: None,
            },
            last_complete: false,
            stale_viewport_matches: true,
        }
    }

    /// Tear down every screen searcher (upstream `deinit`). `ViewportSearch` frees itself on `Drop`.
    ///
    /// # Safety
    /// Each screen searcher's backing `Screen` must still be live (the `ScreenSearch::deinit`
    /// contract).
    pub(in crate::terminal) unsafe fn deinit(&mut self) {
        for s in self.screens.iter_mut() {
            // SAFETY: caller's contract — the backing screen is live.
            unsafe { s.deinit() };
        }
    }

    /// Whether all screen searches are complete (upstream `isComplete`). Vacuously true with no
    /// screens, matching upstream's empty-iterator behavior.
    pub(in crate::terminal) fn is_complete(&self) -> bool {
        self.screens.iter().all(|s| s.is_state_complete())
    }

    /// Tick every screen forward without taking the big lock (upstream `tick`). `Progress` dominates;
    /// `Blocked` only when every incomplete screen needs a feed and none progressed.
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

    /// Reconcile searchers with the terminal's screens, honor the viewport-dirty flag, update the
    /// viewport search, and feed each searcher that needs it (upstream `feed`). This is the
    /// lock-holding step (it reads/mutates terminal state).
    ///
    /// # Safety
    /// `t` must be live and outlive this `Search`, the `Terminal` must not be moved/reallocated, and
    /// the caller holds the screen lock (no concurrent access). The per-screen searchers cache
    /// `NonNull<Screen>` into `t`'s screens; reconciliation drops a searcher whose screen vanished
    /// or was replaced *without* dereferencing the stale pointer (see step B).
    pub(in crate::terminal) unsafe fn feed(&mut self, t: NonNull<Terminal>) {
        // (A) Active screen switch resets the per-screen notification state.
        // SAFETY: caller's contract — `t` is live.
        let active_key = unsafe { Terminal::search_active_screen_key(t) };
        if active_key != self.last_screen.key {
            self.last_screen = ScreenState {
                key: active_key,
                total: None,
                selected: None,
            };
        }

        // (B) Reconcile searchers with the terminal's screens. Collect the present screen pointers
        // up front; no terminal reference is retained afterwards.
        // SAFETY: caller's contract — `t` is live.
        let present = unsafe { Terminal::present_screen_ptrs(t) };
        for key in [TerminalScreenKey::Primary, TerminalScreenKey::Alternate] {
            let remove = match self.screens.get(key) {
                None => false,
                Some(ss) => match present.iter().find(|(k, _)| *k == key) {
                    None => true,                              // screen gone
                    Some((_, ptr)) => ss.screen_ptr() != *ptr, // screen replaced
                },
            };
            if remove {
                // The backing screen was dropped or replaced, so its pin storage is already gone.
                // Drop the searcher WITHOUT `deinit` (untracking against a freed screen would be
                // use-after-free): roastty's `Screen` owns its tracked pins, so a dropped/replaced
                // screen takes them with it. This is a deliberate divergence from upstream's
                // `entry.value.deinit()`.
                let _ = self.screens.take(key);
            }
        }
        let needle = self.viewport.needle().to_vec();
        for (key, ptr) in &present {
            if self.screens.get(*key).is_some() {
                continue;
            }
            // SAFETY: `ptr` is a live terminal screen (see `# Safety`); no terminal reference held.
            let ss = unsafe { ScreenSearch::new(*ptr, &needle) };
            self.screens.insert(*key, ss);
        }

        // (C) Viewport dirty → re-search the active area.
        // SAFETY: `t` live; raw-pointer flag read, no reference materialized.
        if unsafe { Terminal::search_viewport_dirty(t) } {
            // SAFETY: `t` live.
            unsafe { Terminal::clear_search_viewport_dirty(t) };
            self.viewport.set_active_dirty(Some(true));
            if let Some(ss) = self.screens.get_mut(active_key) {
                // SAFETY: active screen live; no terminal reference held here.
                unsafe { ss.reload_active() };
            }
        }

        // (D) Update the viewport search over the active screen's pages.
        if let Some((_, active_ptr)) = present.iter().find(|(k, _)| *k == active_key) {
            // SAFETY: `active_ptr` is the live active screen; the `&Screen` is used only to read its
            // `&PageList` for `update`, which dereferences no `ScreenSearch` pointer.
            let pages = unsafe { active_ptr.as_ref() }.pages();
            // SAFETY: `pages` is read-only for the call.
            let updated = unsafe { self.viewport.update(pages) };
            if updated {
                self.stale_viewport_matches = true;
            }
        }

        // (E) Feed each searcher that needs more data.
        for ss in self.screens.iter_mut() {
            if ss.needs_feed() {
                // SAFETY: screen live; no terminal reference held here.
                unsafe { ss.feed() };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::screen::Screen;
    use std::ptr::NonNull;

    /// Build a `ScreenSearch` over `screen` (which must outlive it) searching for `needle`.
    fn screen_search(screen: &mut Screen, needle: &[u8]) -> ScreenSearch {
        let ptr = NonNull::from(screen);
        // SAFETY: `screen` outlives the returned search; this thread holds it exclusively.
        unsafe { ScreenSearch::new(ptr, needle) }
    }

    #[test]
    fn new_starts_empty_and_complete() {
        let search = Search::new(b"Fizz");
        // No screens → vacuously complete; nothing to tick.
        assert!(search.is_complete());
        assert_eq!(search.viewport.needle(), b"Fizz");
    }

    #[test]
    fn tick_on_empty_is_complete() {
        let mut search = Search::new(b"Fizz");
        assert_eq!(search.tick(), Tick::Complete);
    }

    #[test]
    fn tick_reports_progress_then_blocked_then_complete() {
        let mut screen = Screen::init(10, 10, None).unwrap();
        screen.set_text_lines_for_tests(&["Fizz"]);
        let ss = screen_search(&mut screen, b"Fizz");

        let mut search = Search::new(b"Fizz");
        search.screens.insert(TerminalScreenKey::Primary, ss);

        // First tick drains the active area (state History → HistoryFeed): progress.
        assert_eq!(search.tick(), Tick::Progress);
        // Next tick has nothing to do but wait for a feed: blocked.
        assert_eq!(search.tick(), Tick::Blocked);

        // Drive the screen search to completion, then the aggregate is complete.
        // SAFETY: `screen` is alive.
        unsafe {
            search
                .screens
                .get_mut(TerminalScreenKey::Primary)
                .unwrap()
                .search_all()
        };
        assert_eq!(search.tick(), Tick::Complete);
        assert!(search.is_complete());

        // SAFETY: `screen` is alive; called once.
        unsafe { search.deinit() };
    }

    #[test]
    fn tick_progress_dominates_blocked() {
        // Screen A: advance one tick so its next tick reports FeedRequired (blocked).
        let mut screen_a = Screen::init(10, 10, None).unwrap();
        screen_a.set_text_lines_for_tests(&["Fizz"]);
        let mut ss_a = screen_search(&mut screen_a, b"Fizz");
        let _ = ss_a.tick(); // History → HistoryFeed

        // Screen B: fresh, so its first tick reports Progressed.
        let mut screen_b = Screen::init(10, 10, None).unwrap();
        screen_b.set_text_lines_for_tests(&["Fizz"]);
        let ss_b = screen_search(&mut screen_b, b"Fizz");

        let mut search = Search::new(b"Fizz");
        search.screens.insert(TerminalScreenKey::Primary, ss_a);
        search.screens.insert(TerminalScreenKey::Alternate, ss_b);

        // One screen is blocked, the other progresses → progress dominates regardless of slot order.
        assert_eq!(search.tick(), Tick::Progress);

        // SAFETY: both screens are alive; called once.
        unsafe { search.deinit() };
    }

    #[test]
    fn deinit_releases_screen_search_pins() {
        let mut screen = Screen::init(10, 10, None).unwrap();
        screen.set_text_lines_for_tests(&["Fizz"]);
        let baseline = screen.tracked_pin_count();

        let ss = screen_search(&mut screen, b"Fizz");
        let mut search = Search::new(b"Fizz");
        // Constructing the screen search stood up a history searcher (two tracked pins).
        assert_eq!(screen.tracked_pin_count(), baseline + 2);
        search.screens.insert(TerminalScreenKey::Primary, ss);

        // SAFETY: `screen` is alive; called once.
        unsafe { search.deinit() };
        assert_eq!(screen.tracked_pin_count(), baseline);
    }

    #[test]
    fn insert_returns_replaced_searcher() {
        let mut screen = Screen::init(10, 10, None).unwrap();
        screen.set_text_lines_for_tests(&["Fizz"]);

        let mut map = ScreenSearches::default();
        assert!(map
            .insert(
                TerminalScreenKey::Primary,
                screen_search(&mut screen, b"Fizz")
            )
            .is_none());
        // A second insert returns the prior searcher so the caller can deinit it.
        let replaced = map.insert(
            TerminalScreenKey::Primary,
            screen_search(&mut screen, b"Fizz"),
        );
        assert!(replaced.is_some());

        // Deinit both the replaced and the resident searcher to release their pins.
        let mut replaced = replaced.unwrap();
        // SAFETY: `screen` is alive.
        unsafe { replaced.deinit() };
        let mut resident = map.take(TerminalScreenKey::Primary).unwrap();
        // SAFETY: `screen` is alive.
        unsafe { resident.deinit() };
    }

    use crate::terminal::terminal::Terminal;

    /// The primary screen's current tracked-pin count, read through the terminal's raw screen
    /// pointer (the screens are private to `terminal.rs`).
    fn primary_pin_count(t: NonNull<Terminal>) -> usize {
        // SAFETY: `t` is live; the primary screen is always present.
        let ptr = unsafe { Terminal::present_screen_ptrs(t) }[0].1;
        // SAFETY: `ptr` points at the live primary screen.
        unsafe { ptr.as_ref() }.tracked_pin_count()
    }

    #[test]
    fn feed_adds_a_searcher_for_the_active_screen() {
        let mut terminal = Terminal::init(10, 10, None).unwrap();
        terminal.next_slice(b"Fizz").unwrap();
        let t = NonNull::new(core::ptr::addr_of_mut!(terminal)).unwrap();

        let mut search = Search::new(b"Fizz");
        // SAFETY: `terminal` outlives `search`; exclusive access.
        unsafe { search.feed(t) };

        // A searcher exists for the active (Primary) screen, pointing at the terminal's screen.
        // SAFETY: `t` live.
        let present = unsafe { Terminal::present_screen_ptrs(t) };
        let primary_ptr = present
            .iter()
            .find(|(k, _)| *k == TerminalScreenKey::Primary)
            .unwrap()
            .1;
        let ss = search.screens.get(TerminalScreenKey::Primary).unwrap();
        assert_eq!(ss.screen_ptr(), primary_ptr);

        // It finds the needle.
        // SAFETY: the screen is live.
        unsafe {
            search
                .screens
                .get_mut(TerminalScreenKey::Primary)
                .unwrap()
                .search_all()
        };
        assert!(
            search
                .screens
                .get(TerminalScreenKey::Primary)
                .unwrap()
                .matches_len()
                >= 1
        );

        // SAFETY: `terminal` live; called once.
        unsafe { search.deinit() };
    }

    #[test]
    fn feed_is_idempotent_for_unchanged_screens() {
        let mut terminal = Terminal::init(10, 10, None).unwrap();
        terminal.next_slice(b"Fizz").unwrap();
        let t = NonNull::new(core::ptr::addr_of_mut!(terminal)).unwrap();

        let mut search = Search::new(b"Fizz");
        // SAFETY: as above.
        unsafe { search.feed(t) };
        let after_first = primary_pin_count(t);
        // A second feed neither duplicates the searcher nor tracks new pins.
        // SAFETY: as above.
        unsafe { search.feed(t) };
        assert_eq!(primary_pin_count(t), after_first);
        assert!(search.screens.get(TerminalScreenKey::Primary).is_some());

        // SAFETY: `terminal` live; called once.
        unsafe { search.deinit() };
    }

    #[test]
    fn feed_clears_viewport_dirty_flag() {
        let mut terminal = Terminal::init(10, 10, None).unwrap();
        terminal.next_slice(b"Fizz").unwrap();
        let t = NonNull::new(core::ptr::addr_of_mut!(terminal)).unwrap();

        let mut search = Search::new(b"Fizz");
        // SAFETY: `t` live.
        unsafe { Terminal::mark_search_viewport_dirty(t) };
        // SAFETY: `t` live.
        assert!(unsafe { Terminal::search_viewport_dirty(t) });

        // SAFETY: as above.
        unsafe { search.feed(t) };
        // The dirty handler cleared the flag and marked the viewport matches stale.
        // SAFETY: `t` live.
        assert!(!unsafe { Terminal::search_viewport_dirty(t) });
        assert!(search.stale_viewport_matches);

        // SAFETY: `terminal` live; called once.
        unsafe { search.deinit() };
    }

    #[test]
    fn feed_then_deinit_releases_pins() {
        let mut terminal = Terminal::init(10, 10, None).unwrap();
        terminal.next_slice(b"Fizz").unwrap();
        let t = NonNull::new(core::ptr::addr_of_mut!(terminal)).unwrap();
        let baseline = primary_pin_count(t);

        let mut search = Search::new(b"Fizz");
        // SAFETY: as above.
        unsafe { search.feed(t) };
        assert!(primary_pin_count(t) > baseline);

        // SAFETY: `terminal` live; called once.
        unsafe { search.deinit() };
        assert_eq!(primary_pin_count(t), baseline);
    }

    #[test]
    fn feed_drops_searcher_when_alternate_screen_goes_away() {
        let mut terminal = Terminal::init(10, 10, None).unwrap();
        terminal.next_slice(b"Fizz").unwrap();
        // Enter the alternate screen (mode 1049): the terminal now has an alternate screen.
        terminal.next_slice(b"\x1b[?1049h").unwrap();
        terminal.next_slice(b"Fizz").unwrap();

        let mut search = Search::new(b"Fizz");
        let t = NonNull::new(core::ptr::addr_of_mut!(terminal)).unwrap();
        // SAFETY: `terminal` outlives `search`.
        unsafe { search.feed(t) };
        assert!(search.screens.get(TerminalScreenKey::Alternate).is_some());

        // A full reset (RIS) returns to the primary screen and DROPS the alternate, dangling the
        // alternate searcher's cached pointer.
        terminal.next_slice(b"\x1bc").unwrap();
        let t = NonNull::new(core::ptr::addr_of_mut!(terminal)).unwrap();
        // Reconciliation drops the now-stale alternate searcher WITHOUT deref (no use-after-free):
        // its backing screen — and pin storage — was freed by the reset.
        // SAFETY: `terminal` live.
        unsafe { search.feed(t) };
        assert!(search.screens.get(TerminalScreenKey::Alternate).is_none());
        // The active primary screen keeps (or is freshly given) a searcher.
        assert!(search.screens.get(TerminalScreenKey::Primary).is_some());

        // SAFETY: `terminal` live; called once.
        unsafe { search.deinit() };
    }
}
