//! The history searcher (port of upstream `terminal/search/pagelist.zig`). Drives a reverse
//! `SlidingWindow` backward through a `PageList`'s pages (newest to oldest), feeding pages on
//! demand and tracking its position with a tracked pin so it survives page pruning.
//!
//! This is a faithful pointer-based port of upstream's `*PageList` / `*Pin` design: the
//! list-touching methods are `unsafe` and assume the caller holds the `PageList` lock (no concurrent
//! access) and keeps the list alive, exactly as upstream documents.

use std::ptr::NonNull;

use super::super::highlight::Flattened;
use super::super::page_list::{Node, PageList, Pin};
use super::sliding_window::{Direction, SlidingWindow};

/// Searches for a substring in a `PageList`, in reverse (newest to oldest), feeding pages on demand
/// (upstream `PageListSearch`).
pub(crate) struct PageListSearch {
    /// The list we're searching (upstream `*PageList`).
    list: NonNull<PageList>,
    /// The reverse sliding window of page contents (upstream `window`).
    window: SlidingWindow,
    /// Tracked pin marking the current search position (upstream `*Pin`); the list moves/garbages it
    /// on pruning.
    pin: NonNull<Pin>,
}

impl PageListSearch {
    /// Initialize a reverse search from `start`, tracking a pin at its last cell and feeding the
    /// start page (upstream `init`). `None` if the start pin is invalid.
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
        let pin = Pin::new(start, start_ref.page_rows() - 1, start_ref.page_cols() - 1);
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
    /// dereferences the `list` pointer.
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

    /// The next match in the loaded pages (upstream `next`). `None` means "feed more". Does not touch
    /// the `PageList`, so no lock is needed; but returned pins must be validated before final use.
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

        // Walk older pages from the page before the pin's current node. Each iteration reads the list
        // (`prev_node_ptr`) in a scope that ends before the `&mut Pin` write, so no `&PageList`
        // derived from `self.list` is ever live when `&mut Pin` is created.
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

#[cfg(test)]
mod tests {
    use super::super::super::page_list::PageList;
    use super::*;

    #[test]
    fn single_page_search() {
        let mut list = PageList::init(10, 10, None).unwrap();
        list.set_screen_text_lines_for_tests(&["Fizz", "Buzz"]);
        let start = list.last_node_ptr();

        // SAFETY: `list` outlives `search`; pages are not mutated; `deinit` is called before drop.
        let mut search = unsafe { PageListSearch::new(b"Fizz", &mut list, start).unwrap() };
        assert!(search.next().is_some());
        assert!(search.next().is_none());
        // Only one page, so nothing older to feed.
        assert!(!unsafe { search.feed() });
        unsafe { search.deinit() };
    }

    #[test]
    fn feed_loads_older_page_with_match() {
        let mut list = PageList::init(10, 10, None).unwrap();
        list.grow_to_two_pages_for_tests();
        // `pages[1]` (= `last_node_ptr`) is the start page; `pages[0]` is the older page reached by
        // `feed`. Put a match on each (directly, since the viewport-relative text helper cannot
        // target an arbitrary page).
        list.set_page_row0_text_for_tests(1, "Fizz");
        list.set_page_row0_text_for_tests(0, "Fizz");
        let start = list.last_node_ptr();

        // SAFETY: `list` outlives `search`; pages are not mutated; `deinit` is called before drop.
        let mut search = unsafe { PageListSearch::new(b"Fizz", &mut list, start).unwrap() };

        // Match on the start (newest) page.
        assert!(search.next().is_some());
        assert!(search.next().is_none());

        // Feed loads the older page; its match is then found.
        assert!(unsafe { search.feed() });
        assert!(search.next().is_some());
        assert!(search.next().is_none());

        // No more pages.
        assert!(!unsafe { search.feed() });
        unsafe { search.deinit() };
    }

    #[test]
    fn feed_with_no_matches() {
        let mut list = PageList::init(10, 10, None).unwrap();
        list.grow_to_two_pages_for_tests();
        list.set_page_row0_text_for_tests(1, "Buzz");
        list.set_page_row0_text_for_tests(0, "Hello");
        let start = list.last_node_ptr();

        // SAFETY: see above.
        let mut search = unsafe { PageListSearch::new(b"Nope", &mut list, start).unwrap() };

        assert!(search.next().is_none());
        // The older page has content, so feed succeeds...
        assert!(unsafe { search.feed() });
        // ...but still no match.
        assert!(search.next().is_none());
        assert!(!unsafe { search.feed() });
        unsafe { search.deinit() };
    }

    #[test]
    fn garbage_pin_ends_feed() {
        let mut list = PageList::init(10, 10, None).unwrap();
        list.grow_to_two_pages_for_tests();
        list.set_page_row0_text_for_tests(0, "Fizz");
        let start = list.last_node_ptr();

        // SAFETY: see above.
        let mut search = unsafe { PageListSearch::new(b"Fizz", &mut list, start).unwrap() };

        // Mark the tracked pin garbage; feed must bail out immediately.
        // SAFETY: the pin is live tracked storage; the list is alive.
        unsafe { search.pin.as_mut() }.mark_garbage_for_tests();
        assert!(!unsafe { search.feed() });
        unsafe { search.deinit() };
    }

    #[test]
    fn deinit_untracks_the_pin() {
        let mut list = PageList::init(10, 10, None).unwrap();
        let baseline = list.tracked_pin_count();
        let start = list.last_node_ptr();

        // SAFETY: see above.
        let mut search = unsafe { PageListSearch::new(b"Fizz", &mut list, start).unwrap() };
        assert_eq!(list.tracked_pin_count(), baseline + 1);

        unsafe { search.deinit() };
        assert_eq!(list.tracked_pin_count(), baseline);
    }
}
