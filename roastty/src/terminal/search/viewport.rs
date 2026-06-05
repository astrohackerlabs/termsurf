//! Searches the viewport of a `PageList` (port of upstream `terminal/search/viewport.zig`).
//!
//! The viewport is the part of the search the user actively sees, so re-searching only the viewport
//! (rather than caching every screen result) is cheap. A node "fingerprint" detects when the
//! viewport actually moves; an unchanged viewport that overlaps the mutable active area is
//! re-searched anyway (gated by optional dirty tracking). Like upstream, this searches all pages the
//! viewport covers, so it can surface matches just outside the viewport that live on the same page.

use std::ptr::NonNull;

use super::super::highlight::Flattened;
use super::super::page_list::{Node, PageList};
use super::sliding_window::{Direction, SlidingWindow};

/// Viewport fingerprint — the page nodes the viewport covers (upstream `ViewportSearch.Fingerprint`).
/// Only pointer identity is compared: cached page contents may be invalid after mutation, so just
/// the node pointers are safe to keep and compare.
#[derive(Debug, PartialEq)]
struct Fingerprint {
    nodes: Vec<NonNull<Node>>,
}

impl Fingerprint {
    fn new(list: &PageList) -> Fingerprint {
        Fingerprint {
            nodes: list.viewport_nodes(),
        }
    }
}

/// Searches the viewport of a `PageList`, re-searching only when the viewport moves or overlaps the
/// active area (upstream `ViewportSearch`). Forward-only: the viewport is small, so a forward search
/// is instant and avoids the work of reversing.
pub(crate) struct ViewportSearch {
    /// The sliding-window matcher over the viewport's pages.
    window: SlidingWindow,
    /// The last viewport fingerprint, or `None` if the next `update` should always re-search.
    fingerprint: Option<Fingerprint>,
    /// Active-area dirty tracking. `None` disables it (always re-search when the viewport overlaps
    /// the active area); `Some(dirty)` re-searches the active area only when dirty. Dirty marking is
    /// the caller's responsibility (upstream's search `Thread` sets it).
    active_dirty: Option<bool>,
}

impl ViewportSearch {
    /// Create a forward viewport search for `needle` (upstream `init`).
    pub(in crate::terminal) fn new(needle: &[u8]) -> ViewportSearch {
        ViewportSearch {
            window: SlidingWindow::new(Direction::Forward, needle),
            fingerprint: None,
            active_dirty: None,
        }
    }

    /// Drop the fingerprint and clear the window so the next `update` always re-searches (upstream
    /// `reset`).
    pub(in crate::terminal) fn reset(&mut self) {
        self.fingerprint = None;
        self.window.clear_and_retain_capacity();
    }

    /// The needle this search is using (upstream `needle`).
    pub(in crate::terminal) fn needle(&self) -> &[u8] {
        self.window.needle()
    }

    /// Set the active-area dirty-tracking state (upstream writes the `active_dirty` field directly
    /// from the search `Thread`). `None` disables tracking; `Some(true)` enables it and marks dirty;
    /// `Some(false)` enables it and marks clean. Both upstream writes are `active_dirty = true`,
    /// i.e. `set_active_dirty(Some(true))`.
    pub(in crate::terminal) fn set_active_dirty(&mut self, value: Option<bool>) {
        self.active_dirty = value;
    }

    /// Update the sliding window to reflect the current viewport (upstream `update`). Does nothing if
    /// the viewport is unchanged and does not overlap the (mutable) active area. Returns whether a
    /// re-search is needed.
    ///
    /// # Safety
    /// `list` must be safe to read for the whole call (the caller holds the necessary locks); the
    /// page nodes it yields are dereferenced for soft-wrap checks.
    pub(in crate::terminal) unsafe fn update(&mut self, list: &PageList) -> bool {
        let fingerprint = Fingerprint::new(list);
        if let Some(old) = self.fingerprint.as_ref() {
            if *old == fingerprint {
                // Decide whether to check active-area overlap. With dirty tracking on, consume the
                // dirty flag here.
                let check_active = match self.active_dirty {
                    None => true,
                    Some(false) => false,
                    Some(true) => {
                        self.active_dirty = Some(false);
                        true
                    }
                };

                let mut overlaps = false;
                if check_active {
                    // The active area is mutable, so a viewport containing either of its endpoints
                    // must always re-search. (Checked first because the viewport is larger.)
                    let tl = list.active_area_top_left().node();
                    let br = list
                        .active_area_bottom_right_node()
                        .expect("active area always has a bottom-right node");
                    for &node in &old.nodes {
                        if node == tl || node == br {
                            overlaps = true;
                            break;
                        }
                    }
                }

                if !overlaps {
                    return false; // unchanged
                }
            }
        }

        // Re-search. Keep the node pointers (cheap copies) before moving the fingerprint in.
        let nodes = fingerprint.nodes.clone();
        self.fingerprint = Some(fingerprint);
        // We're re-searching now, so the active area is no longer dirty.
        if let Some(v) = self.active_dirty.as_mut() {
            *v = false;
        }
        self.window.clear_and_retain_capacity();

        let overlap_target = self.window.needle_len().saturating_sub(1);

        // Leading overlap: cover up to `needle.len - 1` bytes of soft-wrapped prior pages so a
        // match that wraps into the viewport's first node is still found.
        let mut node_opt = list.prev_node_ptr(nodes[0]);
        let mut added = 0usize;
        while let Some(node) = node_opt {
            // SAFETY: `node` is a live page-list node (caller's read contract).
            if !unsafe { node.as_ref() }.last_row_wrapped() {
                break;
            }
            // SAFETY: as above.
            added += unsafe { self.window.append(node) };
            if added >= overlap_target {
                break;
            }
            node_opt = list.prev_node_ptr(node);
        }

        // The viewport's own nodes (already traversed once for the fingerprint).
        for &node in &nodes {
            // SAFETY: as above.
            unsafe { self.window.append(node) };
        }

        // Trailing overlap: same rule for the following soft-wrapped pages.
        let end = nodes[nodes.len() - 1];
        // SAFETY: as above.
        if unsafe { end.as_ref() }.last_row_wrapped() {
            let mut node_opt = list.next_node_ptr(end);
            let mut added = 0usize;
            while let Some(node) = node_opt {
                // SAFETY: as above.
                added += unsafe { self.window.append(node) };
                if added >= overlap_target {
                    break;
                }
                // SAFETY: as above.
                if !unsafe { node.as_ref() }.last_row_wrapped() {
                    break;
                }
                node_opt = list.next_node_ptr(node);
            }
        }

        true
    }

    /// Find the next match in the viewport (upstream `next`). `None` when there are no more.
    pub(in crate::terminal) fn next(&mut self) -> Option<Flattened> {
        self.window.next()
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::page_list::PageList;
    use super::*;

    fn list_with(lines: &[&str]) -> PageList {
        let mut list = PageList::init(10, 10, None).unwrap();
        list.set_screen_text_lines_for_tests(lines);
        list
    }

    fn count_matches(vp: &mut ViewportSearch) -> usize {
        let mut n = 0;
        while vp.next().is_some() {
            n += 1;
        }
        n
    }

    #[test]
    fn needle_returns_the_needle() {
        let vp = ViewportSearch::new(b"Fizz");
        assert_eq!(vp.needle(), b"Fizz");
    }

    #[test]
    fn update_finds_viewport_matches() {
        let list = list_with(&["Fizz", "Buzz", "Fizz"]);
        let mut vp = ViewportSearch::new(b"Fizz");
        // SAFETY: `list` is alive and read-only for the call.
        assert!(unsafe { vp.update(&list) });
        assert_eq!(count_matches(&mut vp), 2);
    }

    #[test]
    fn update_with_no_matches_yields_nothing() {
        let list = list_with(&["Buzz", "Bang"]);
        let mut vp = ViewportSearch::new(b"Fizz");
        // SAFETY: as above.
        assert!(unsafe { vp.update(&list) });
        assert_eq!(count_matches(&mut vp), 0);
    }

    #[test]
    fn update_twice_reresearches_when_viewport_covers_active() {
        // The default viewport contains the (mutable) active area, so it always re-searches.
        let list = list_with(&["Fizz"]);
        let mut vp = ViewportSearch::new(b"Fizz");
        // SAFETY: as above.
        assert!(unsafe { vp.update(&list) });
        // SAFETY: as above.
        assert!(unsafe { vp.update(&list) });
    }

    #[test]
    fn reset_forces_research() {
        let list = list_with(&["Fizz"]);
        let mut vp = ViewportSearch::new(b"Fizz");
        // With dirty tracking off-but-clean, an unchanged viewport doesn't re-search...
        vp.set_active_dirty(Some(false));
        // SAFETY: as above.
        assert!(unsafe { vp.update(&list) });
        // SAFETY: as above.
        assert!(!unsafe { vp.update(&list) });
        // ...until `reset` drops the fingerprint.
        vp.reset();
        // SAFETY: as above.
        assert!(unsafe { vp.update(&list) });
    }

    #[test]
    fn active_dirty_false_suppresses_reresearch() {
        let list = list_with(&["Fizz"]);
        let mut vp = ViewportSearch::new(b"Fizz");
        vp.set_active_dirty(Some(false));
        // SAFETY: as above.
        assert!(unsafe { vp.update(&list) });
        // Unchanged viewport, dirty gate clean → no re-search even though it covers the active area.
        // SAFETY: as above.
        assert!(!unsafe { vp.update(&list) });
    }

    #[test]
    fn active_dirty_true_reresearches_then_clears() {
        let list = list_with(&["Fizz"]);
        let mut vp = ViewportSearch::new(b"Fizz");
        vp.set_active_dirty(Some(false));
        // SAFETY: as above.
        assert!(unsafe { vp.update(&list) });
        // SAFETY: as above.
        assert!(!unsafe { vp.update(&list) });
        // Marking the active area dirty forces one re-search, which clears the flag again.
        vp.set_active_dirty(Some(true));
        // SAFETY: as above.
        assert!(unsafe { vp.update(&list) });
        // SAFETY: as above.
        assert!(!unsafe { vp.update(&list) });
    }
}
