//! The search sliding window (port of upstream `terminal/search/sliding_window.zig`). So far it
//! lands the vocabulary and lifecycle (the search `Direction`, the per-page `Meta` record, the
//! `SlidingWindow` struct, its constructor, and `clear_and_retain_capacity`), plus `append` (encode
//! a page node's text into the window with its cell map) and the `assert_integrity` invariant. The
//! cross-page matcher (`next` / `highlight`, the overlap/prune logic) is deferred to later slices.

use std::collections::VecDeque;
use std::ptr::NonNull;

use super::super::highlight::Chunk;
use super::super::page_list::Node;
use super::super::point::Coordinate;

/// The search direction (upstream `SlidingWindow.Direction`). For a reverse search the needle is
/// stored reversed and pages are appended in reverse order (the caller's responsibility).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Forward,
    Reverse,
}

/// Per-appended-page metadata (upstream `SlidingWindow.Meta`). `cell_map` maps each encoded data
/// byte back to a cell coordinate; `serial` detects page invalidation. Owns its `cell_map` (Rust's
/// `Drop` subsumes upstream `Meta.deinit`).
///
/// `Meta` and its `node` field are `pub(in crate::terminal)` — no more visible than `Node`
/// (`pub(super)` in `page_list`) — so exposing `node: NonNull<Node>` does not leak a more-private
/// type (which would trip the `private_interfaces` warning and the no-warnings gate).
#[derive(Debug)]
pub(in crate::terminal) struct Meta {
    pub(in crate::terminal) node: NonNull<Node>,
    pub(in crate::terminal) serial: u64,
    pub(in crate::terminal) cell_map: Vec<Coordinate>,
}

/// Searches page nodes via a sliding window over their encoded text (upstream `SlidingWindow`).
pub(crate) struct SlidingWindow {
    /// Encoded page text (upstream `data: CircBuf(u8, 0)`).
    data: VecDeque<u8>,
    /// Per-page metadata (upstream `meta: CircBuf(Meta, undefined)`).
    meta: VecDeque<Meta>,
    /// Scratch chunk buffer for flattened highlights (upstream `chunk_buf`).
    chunk_buf: Vec<Chunk>,
    /// Offset into `data` for the current search state (upstream `data_offset`).
    data_offset: usize,
    /// The needle, owned; stored reversed for a reverse search (upstream `needle`).
    needle: Vec<u8>,
    /// The search direction (upstream `direction`).
    direction: Direction,
    /// Cross-page-boundary scratch buffer, `needle.len() * 2` bytes (upstream `overlap_buf`).
    overlap_buf: Vec<u8>,
}

impl SlidingWindow {
    /// Create an empty sliding window for `needle` in `direction` (upstream `init`). The needle is
    /// copied (and reversed for a reverse search); the overlap buffer is `needle.len() * 2` bytes.
    /// Unlike upstream's `Allocator.Error` return, this aborts on allocation failure like any Rust
    /// collection.
    pub(crate) fn new(direction: Direction, needle: &[u8]) -> SlidingWindow {
        let mut needle = needle.to_vec();
        if direction == Direction::Reverse {
            needle.reverse();
        }
        let overlap_buf = vec![0u8; needle.len() * 2];
        SlidingWindow {
            data: VecDeque::new(),
            meta: VecDeque::new(),
            chunk_buf: Vec::new(),
            data_offset: 0,
            needle,
            direction,
            overlap_buf,
        }
    }

    /// Clear all data but retain allocated capacity (upstream `clearAndRetainCapacity`). Clearing
    /// `meta` drops each `Meta` (and its `cell_map`), subsuming upstream's per-meta `deinit`.
    pub(crate) fn clear_and_retain_capacity(&mut self) {
        self.meta.clear();
        self.data.clear();
        self.data_offset = 0;
    }

    /// Encode `node`'s page text into the window, recording its `Meta` (upstream `append`). Returns
    /// the number of content bytes added (0 if the page contributes nothing).
    ///
    /// # Safety
    /// `node` must point to a live `Node`. The window dereferences it here and **stores the pointer
    /// in `meta`** for later use by the matcher (`next` / `highlight`), so the caller must keep the
    /// node valid for as long as it remains in the window — in particular, the caller must not
    /// mutate or drop the owning `PageList` in any way that reallocates or removes the node while
    /// the window may still reference it (clear the window first). The window does not own pages.
    pub(in crate::terminal) unsafe fn append(&mut self, node: NonNull<Node>) -> usize {
        let node_ref = unsafe { node.as_ref() };
        let (text, mut cell_map) = node_ref.search_encode();
        let mut bytes = text.into_bytes();

        // Trailing newline if the last row isn't soft-wrapped (added before the empty check, so an
        // unwrapped empty page still contributes one '\n').
        if !node_ref.last_row_wrapped() {
            let last = cell_map.last().copied().unwrap_or(Coordinate::new(0, 0));
            bytes.push(b'\n');
            cell_map.push(last);
        }

        if bytes.is_empty() {
            self.assert_integrity();
            return 0;
        }

        // Reverse the encoding for a reverse search.
        if self.direction == Direction::Reverse {
            bytes.reverse();
            cell_map.reverse();
        }

        let written_len = bytes.len();
        self.data.extend(bytes);
        self.meta.push_back(Meta {
            node,
            serial: node_ref.serial(),
            cell_map,
        });

        self.assert_integrity();
        written_len
    }

    /// Debug-only integrity check (upstream `assertIntegrity`): the `data` length equals the sum of
    /// every meta's `cell_map` length, and `data_offset` is in bounds.
    fn assert_integrity(&self) {
        debug_assert_eq!(
            self.meta.iter().map(|m| m.cell_map.len()).sum::<usize>(),
            self.data.len(),
        );
        debug_assert!(self.data.is_empty() || self.data_offset < self.data.len());
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::page_list::PageList;
    use super::*;

    #[test]
    fn new_forward_keeps_needle_and_sizes_overlap() {
        let w = SlidingWindow::new(Direction::Forward, b"abc");
        assert_eq!(w.needle, b"abc");
        assert_eq!(w.direction, Direction::Forward);
        assert!(w.data.is_empty());
        assert!(w.meta.is_empty());
        assert_eq!(w.overlap_buf.len(), 6);
        assert_eq!(w.data_offset, 0);
    }

    #[test]
    fn new_reverse_stores_needle_reversed() {
        let w = SlidingWindow::new(Direction::Reverse, b"abc");
        assert_eq!(w.needle, b"cba");
        assert_eq!(w.direction, Direction::Reverse);
        assert_eq!(w.overlap_buf.len(), 6);
    }

    #[test]
    fn new_empty_needle_has_empty_overlap() {
        let w = SlidingWindow::new(Direction::Forward, b"");
        assert!(w.needle.is_empty());
        assert_eq!(w.overlap_buf.len(), 0);
    }

    #[test]
    fn clear_and_retain_capacity_empties_buffers_and_resets_offset() {
        let mut w = SlidingWindow::new(Direction::Forward, b"abc");

        // Push some data and a Meta. The Meta's node is a dangling pointer that is never
        // dereferenced (clearing only drops the Meta, which drops its cell_map Vec).
        w.data.push_back(b'x');
        w.data.push_back(b'y');
        w.meta.push_back(Meta {
            node: NonNull::dangling(),
            serial: 7,
            cell_map: vec![Coordinate::new(0, 0)],
        });
        w.data_offset = 1;

        w.clear_and_retain_capacity();

        assert!(w.data.is_empty());
        assert!(w.meta.is_empty());
        assert_eq!(w.data_offset, 0);
        // Capacity is retained, not freed.
        assert!(w.data.capacity() > 0);
        assert!(w.meta.capacity() > 0);
    }

    #[test]
    fn clear_and_retain_capacity_leaves_chunk_buf() {
        let mut w = SlidingWindow::new(Direction::Forward, b"abc");
        w.chunk_buf.push(Chunk {
            node: NonNull::dangling(),
            serial: 0,
            start: 0,
            end: 0,
        });

        w.clear_and_retain_capacity();

        // Upstream clears only meta / data / data_offset; the chunk scratch buffer is untouched.
        assert_eq!(w.chunk_buf.len(), 1);
    }

    #[test]
    fn direction_is_copy_and_eq() {
        let d = Direction::Reverse;
        let copy = d;
        assert_eq!(d, copy);
        assert_ne!(Direction::Forward, Direction::Reverse);
    }

    #[test]
    fn append_forward_encodes_page_with_trailing_newline() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.set_screen_text_lines_for_tests(&["abc"]);
        let node = list.first_node_ptr();

        let mut w = SlidingWindow::new(Direction::Forward, b"abc");
        // SAFETY: `list` outlives `w`, and the node pointer is not invalidated below.
        let added = unsafe { w.append(node) };

        assert_eq!(added, 4); // "abc" + trailing '\n'
        assert_eq!(w.data.iter().copied().collect::<Vec<u8>>(), b"abc\n");
        assert_eq!(w.meta.len(), 1);
        assert_eq!(w.meta[0].cell_map.len(), 4);
        assert_eq!(w.meta[0].serial, unsafe { node.as_ref() }.serial());
        assert_eq!(w.data_offset, 0);
    }

    #[test]
    fn append_reverse_reverses_bytes_and_cell_map() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.set_screen_text_lines_for_tests(&["abc"]);
        let node = list.first_node_ptr();

        let mut w = SlidingWindow::new(Direction::Reverse, b"abc");
        // SAFETY: `list` outlives `w`, and the node pointer is not invalidated below.
        let added = unsafe { w.append(node) };

        assert_eq!(added, 4);
        // The forward encoding "abc\n" is reversed byte-wise.
        assert_eq!(w.data.iter().copied().collect::<Vec<u8>>(), b"\ncba");
        // The cell map is reversed in lockstep. Forward is a@(0,0), b@(1,0), c@(2,0), and the '\n'
        // maps to the previous coordinate (2,0); reversed, that is (2,0),(2,0),(1,0),(0,0).
        assert_eq!(
            w.meta[0].cell_map,
            vec![
                Coordinate::new(2, 0),
                Coordinate::new(2, 0),
                Coordinate::new(1, 0),
                Coordinate::new(0, 0),
            ]
        );
    }

    #[test]
    fn append_empty_page_adds_only_trailing_newline() {
        let list = PageList::init(80, 24, None).unwrap();
        // No text set: the page is blank, so the encoded text is empty, but the last row is not
        // soft-wrapped, so a single '\n' is still appended (before the empty check).
        let node = list.first_node_ptr();

        let mut w = SlidingWindow::new(Direction::Forward, b"x");
        // SAFETY: `list` outlives `w`, and the node pointer is not invalidated below.
        let added = unsafe { w.append(node) };

        assert_eq!(added, 1);
        assert_eq!(w.data.iter().copied().collect::<Vec<u8>>(), b"\n");
        assert_eq!(w.meta[0].cell_map, vec![Coordinate::new(0, 0)]);
    }

    #[test]
    fn append_maintains_data_meta_length_invariant() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.set_screen_text_lines_for_tests(&["hello"]);
        let node = list.first_node_ptr();

        let mut w = SlidingWindow::new(Direction::Forward, b"hello");
        // SAFETY: `list` outlives `w`, and the node pointer is not invalidated below.
        // (`append` also calls `assert_integrity` internally; this re-checks explicitly.)
        unsafe { w.append(node) };

        let summed: usize = w.meta.iter().map(|m| m.cell_map.len()).sum();
        assert_eq!(summed, w.data.len());
    }
}
