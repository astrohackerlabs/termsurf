//! The search sliding window (port of upstream `terminal/search/sliding_window.zig`). This first
//! slice lands the vocabulary and lifecycle: the search `Direction`, the per-page `Meta` record,
//! the `SlidingWindow` struct, its constructor, and `clear_and_retain_capacity`. The search
//! algorithm itself (`next` / `append` / `highlight`, the overlap/prune logic, the integrity
//! assertions, and buffer growth) is deferred to later slices.

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
}

#[cfg(test)]
mod tests {
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
}
