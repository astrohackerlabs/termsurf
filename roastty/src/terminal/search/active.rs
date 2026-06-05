//! The active-area searcher (port of upstream `terminal/search/active.zig`). Drives a
//! `SlidingWindow` over a `PageList`'s mutable active area, re-copied on each `update`.

use std::ptr::NonNull;

use super::super::highlight::Flattened;
use super::super::page_list::{Node, PageList};
use super::sliding_window::{Direction, SlidingWindow};

/// Searches for a substring within the active area of a `PageList` (upstream `ActiveSearch`).
pub(crate) struct ActiveSearch {
    window: SlidingWindow,
}

impl ActiveSearch {
    /// Create a searcher for `needle` (upstream `init`). A forward window — the active area is
    /// small, so reversing is not worth it.
    pub(crate) fn new(needle: &[u8]) -> ActiveSearch {
        ActiveSearch {
            window: SlidingWindow::new(Direction::Forward, needle),
        }
    }

    /// Copy the active area (plus a small history overlap) into the window (upstream `update`).
    /// Does not search. Returns the oldest page covering the active area (for the history searcher
    /// to dedup against), or `None` if the active area covers the whole list.
    ///
    /// # Safety
    /// The window stores node pointers derived from `list`; the caller must keep `list`'s pages
    /// valid (not reallocated/freed) until the search results are consumed or the window is cleared
    /// (the same contract as `SlidingWindow::append`).
    pub(in crate::terminal) unsafe fn update(&mut self, list: &PageList) -> Option<NonNull<Node>> {
        self.window.clear_and_retain_capacity();

        let nodes = list.node_ptrs_front_to_back();
        let mut rem = list.active_rows() as usize;
        let mut last_node: Option<NonNull<Node>> = None;
        let mut i = nodes.len();
        let mut into_overlap = false;

        // 1. Cover the active area, walking pages back to front.
        while i > 0 {
            i -= 1;
            let node = nodes[i];
            // SAFETY: `nodes` are valid for this call; the caller upholds `update`'s contract for
            // their later use in the window.
            let rows = unsafe { node.as_ref() }.page_rows() as usize;
            unsafe { self.window.append(node) };
            last_node = Some(node);
            if rem <= rows {
                into_overlap = true;
                break;
            }
            rem -= rows;
        }

        // 2. Add overlap pages until `needle.len - 1` bytes are covered or a non-wrapped boundary.
        if into_overlap {
            let needed = self.window.needle_len().saturating_sub(1);
            while i > 0 {
                i -= 1;
                let node = nodes[i];
                // SAFETY: see above.
                if !unsafe { node.as_ref() }.last_row_wrapped() {
                    break;
                }
                let added = unsafe { self.window.append(node) };
                if added >= needed {
                    break;
                }
            }
        }

        last_node
    }

    /// Find the next match in the active area (upstream `next`); `None` when exhausted.
    pub(in crate::terminal) fn next(&mut self) -> Option<Flattened> {
        self.window.next()
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::page_list::PageList;
    use super::*;

    #[test]
    fn simple_search_finds_active_matches() {
        let mut list = PageList::init(10, 10, None).unwrap();
        list.set_screen_text_lines_for_tests(&["Fizz", "Buzz", "Fizz", "Bang"]);

        let mut search = ActiveSearch::new(b"Fizz");
        // SAFETY: `list` outlives `search`; its pages are not mutated before the results are used.
        unsafe { search.update(&list) };

        let h1 = search.next().expect("first Fizz");
        assert_eq!(h1.top_x, 0);
        assert_eq!(h1.bot_x, 3);
        assert_eq!(h1.chunks[0].start, 0); // row 0

        let h2 = search.next().expect("second Fizz");
        assert_eq!(h2.top_x, 0);
        assert_eq!(h2.bot_x, 3);
        assert_eq!(h2.chunks[0].start, 2); // row 2

        assert!(search.next().is_none());
    }

    #[test]
    fn update_clears_the_prior_window() {
        let mut list = PageList::init(10, 10, None).unwrap();
        list.set_screen_text_lines_for_tests(&["Fizz", "Buzz", "Fizz", "Bang"]);

        let mut search = ActiveSearch::new(b"Fizz");
        // SAFETY: see above.
        unsafe { search.update(&list) };
        while search.next().is_some() {}

        // A fresh update refills the window and re-finds both matches.
        // SAFETY: see above.
        unsafe { search.update(&list) };
        assert!(search.next().is_some());
        assert!(search.next().is_some());
        assert!(search.next().is_none());
    }

    #[test]
    fn no_match_returns_none() {
        let mut list = PageList::init(10, 10, None).unwrap();
        list.set_screen_text_lines_for_tests(&["Fizz", "Buzz"]);

        let mut search = ActiveSearch::new(b"zzzz");
        // SAFETY: see above.
        unsafe { search.update(&list) };
        assert!(search.next().is_none());
    }

    #[test]
    fn update_returns_the_covering_page() {
        let mut list = PageList::init(10, 10, None).unwrap();
        list.set_screen_text_lines_for_tests(&["Fizz"]);

        let mut search = ActiveSearch::new(b"Fizz");
        // SAFETY: see above.
        let covering = unsafe { search.update(&list) };
        assert_eq!(covering, Some(list.first_node_ptr()));
    }

    /// Reach the `SlidingWindow::meta_len` through a same-module helper.
    impl ActiveSearch {
        #[cfg(test)]
        fn window_meta_len(&self) -> usize {
            self.window.meta_len()
        }
    }

    #[test]
    fn overlap_pass_appends_soft_wrapped_older_page() {
        // Two pages; the older (scrollback) page has content and its last row is soft-wrapped, so the
        // overlap pass appends it (a blank older page would encode to nothing and add no meta).
        let mut list = PageList::init(10, 10, None).unwrap();
        list.grow_to_two_pages_for_tests();
        list.set_first_page_content_and_wrap_for_tests(true);

        // needle longer than 1 so the overlap pass is meaningful.
        let mut search = ActiveSearch::new(b"abcdef");
        // SAFETY: `list` outlives `search`; its pages are not mutated before the results are used.
        unsafe { search.update(&list) };
        assert_eq!(search.window_meta_len(), 2); // active page + overlapped older page

        // Without the wrap, the overlap pass stops at the non-wrapped boundary (older page skipped).
        let mut plain = PageList::init(10, 10, None).unwrap();
        plain.grow_to_two_pages_for_tests();
        plain.set_first_page_content_and_wrap_for_tests(false);
        let mut search2 = ActiveSearch::new(b"abcdef");
        // SAFETY: see above.
        unsafe { search2.update(&plain) };
        assert_eq!(search2.window_meta_len(), 1);
    }
}
