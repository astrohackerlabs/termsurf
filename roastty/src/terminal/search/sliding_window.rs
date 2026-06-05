//! The search sliding window (port of upstream `terminal/search/sliding_window.zig`). So far it
//! lands the vocabulary and lifecycle (the search `Direction`, the per-page `Meta` record, the
//! `SlidingWindow` struct, its constructor, and `clear_and_retain_capacity`), `append` (encode a
//! page node's text into the window with its cell map), the `assert_integrity` invariant,
//! `highlight` (turn a match byte-range into a `Flattened` highlight, pruning consumed pages), and
//! `next` (the scan: search the data across the two ring slices and the cross-page overlap for the
//! needle, pruning on a miss). The `SlidingWindow` matcher is complete; the higher-level searchers
//! (`active` / `pagelist` / `screen` / `viewport`) and the search `Thread` remain.

use std::collections::VecDeque;
use std::ptr::NonNull;

use super::super::highlight::{Chunk, Flattened};
use super::super::page_list::Node;
use super::super::point::Coordinate;
use super::super::size::CellCountInt;

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

    /// The needle length (upstream `window.needle.len`).
    pub(in crate::terminal) fn needle_len(&self) -> usize {
        self.needle.len()
    }

    /// The number of metas (pages) currently in the window (test helper).
    #[cfg(test)]
    pub(in crate::terminal) fn meta_len(&self) -> usize {
        self.meta.len()
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

    /// Build a flattened highlight for a match at `start_offset` (relative to `data_offset`) of
    /// length `len` (upstream `highlight`). Sets `top_x` / `bot_x`, emits one `Chunk` per spanned
    /// page, prunes consumed metas/data, and advances `data_offset` one past the match.
    ///
    /// Dereferences stored `Meta.node` pointers for `page_rows`; sound under the window invariant
    /// that every node remains valid while in the window (see `append`'s `# Safety`).
    fn highlight(&mut self, start_offset: usize, len: usize) -> Flattened {
        let start = start_offset + self.data_offset;
        let end = start + len - 1;
        debug_assert!(start < self.data.len());
        debug_assert!(start + len <= self.data.len());

        self.chunk_buf.clear();
        let mut result = Flattened::empty();

        // Top-left (start) search. `prune_*` = (meta count, data length) before the start meta; `br`
        // = Some((next meta index, consumed)) when the end is in a later meta.
        let mut br: Option<(usize, usize)> = None;
        let mut prune_meta = 0usize;
        let mut prune_data = 0usize;
        let mut meta_consumed = 0usize;
        let mut found = false;
        for i in 0..self.meta.len() {
            let meta = &self.meta[i];
            let prior = meta_consumed;
            meta_consumed += meta.cell_map.len();
            let meta_i = start - prior;
            if meta_i >= meta.cell_map.len() {
                continue;
            }
            let end_i = end - prior;
            if end_i < meta.cell_map.len() {
                let start_map = meta.cell_map[meta_i];
                let end_map = meta.cell_map[end_i];
                result.top_x = start_map.x;
                result.bot_x = end_map.x;
                self.chunk_buf.push(Chunk {
                    node: meta.node,
                    serial: meta.serial,
                    start: cell_row(start_map.y),
                    end: cell_row(end_map.y + 1),
                });
            } else {
                let map = meta.cell_map[meta_i];
                result.top_x = map.x;
                // SAFETY: stored nodes stay valid while in the window (append's contract).
                let rows = unsafe { meta.node.as_ref() }.page_rows();
                self.chunk_buf.push(Chunk {
                    node: meta.node,
                    serial: meta.serial,
                    start: cell_row(map.y),
                    end: rows,
                });
                br = Some((i + 1, meta_consumed));
            }
            prune_meta = i;
            prune_data = prior;
            found = true;
            break;
        }
        assert!(
            found,
            "highlight start index must be within the data buffer"
        );

        // Bottom-right (end) search.
        if let Some((mut idx, mut consumed)) = br {
            let mut end_found = false;
            while idx < self.meta.len() {
                let meta = &self.meta[idx];
                let meta_i = end - consumed;
                if meta_i >= meta.cell_map.len() {
                    // SAFETY: see above.
                    let rows = unsafe { meta.node.as_ref() }.page_rows();
                    self.chunk_buf.push(Chunk {
                        node: meta.node,
                        serial: meta.serial,
                        start: 0,
                        end: rows,
                    });
                    consumed += meta.cell_map.len();
                    idx += 1;
                    continue;
                }
                let map = meta.cell_map[meta_i];
                result.bot_x = map.x;
                self.chunk_buf.push(Chunk {
                    node: meta.node,
                    serial: meta.serial,
                    start: 0,
                    end: cell_row(map.y + 1),
                });
                end_found = true;
                break;
            }
            assert!(
                end_found,
                "highlight end index must be within the data buffer"
            );
        }

        // Advance one past the match, then prune everything before the start meta.
        self.data_offset = start - prune_data + 1;
        if prune_meta > 0 {
            self.meta.drain(..prune_meta);
            debug_assert!(prune_data > 0);
            self.data.drain(..prune_data);
            // The surviving front meta is the start meta — its node is the first chunk's node
            // (upstream's post-prune cross-check, before the reverse fixup reorders `chunk_buf`).
            debug_assert_eq!(
                self.meta.front().map(|m| m.node),
                self.chunk_buf.first().map(|c| c.node),
            );
        }

        // Reverse fixup: the chunks were built in forward data order. NOTE: reversing the
        // `Vec<Chunk>` reverses `serial` along with `node` / `start` / `end` — deliberately, so each
        // chunk's `serial` stays paired with its `node`. Upstream reverses only the node/start/end
        // arrays (leaving the serial array in place); this is a correctness-preserving deviation.
        if self.direction == Direction::Reverse {
            let n = self.chunk_buf.len();
            if n > 1 {
                self.chunk_buf.reverse();
                // SAFETY: see above.
                let first_rows = unsafe { self.chunk_buf[0].node.as_ref() }.page_rows();
                self.chunk_buf[0].start = self.chunk_buf[0].end - 1;
                self.chunk_buf[0].end = first_rows;
                self.chunk_buf[n - 1].end = self.chunk_buf[n - 1].start + 1;
                self.chunk_buf[n - 1].start = 0;
            } else {
                let start_y = self.chunk_buf[0].start;
                self.chunk_buf[0].start = self.chunk_buf[0].end - 1;
                self.chunk_buf[0].end = start_y + 1;
            }
            std::mem::swap(&mut result.top_x, &mut result.bot_x);
        }

        result.chunks = self.chunk_buf.clone();
        result
    }

    /// Search the window for the next needle occurrence (upstream `next`). Returns a flattened
    /// highlight on a match (pruning consumed pages); on a miss, prunes to the trailing overlap and
    /// returns `None`.
    pub(in crate::terminal) fn next(&mut self) -> Option<Flattened> {
        // The needle must be non-empty: an empty needle would make `highlight(0, 0)` underflow
        // `end = start + len - 1` (upstream assumes this too; the searchers always pass a non-empty
        // needle). `new` accepts an empty needle, so encode the precondition with an active assert.
        assert!(!self.needle.is_empty(), "search needle must be non-empty");

        let data_len = self.data.len();
        if data_len < self.needle.len() {
            return None;
        }

        // Search the two ring slices and the cross-boundary overlap, in upstream order. Yields the
        // match's start offset (relative to `data_offset`) or `None`.
        let match_offset: Option<usize> = 'search: {
            let needle = self.needle.as_slice();
            let nlen = needle.len();
            let (a, b) = self.data.as_slices();
            let (s0, s1): (&[u8], &[u8]) = if self.data_offset <= a.len() {
                (&a[self.data_offset..], b)
            } else {
                (&b[self.data_offset - a.len()..], &[][..])
            };

            if let Some(idx) = index_of_ignore_case(s0, needle) {
                break 'search Some(idx);
            }
            if !s0.is_empty() && !s1.is_empty() {
                let nlen1 = nlen - 1;
                let plen = s0.len().min(nlen1);
                let slen = s1.len().min(nlen1);
                let overlap_len = plen + slen;
                debug_assert!(overlap_len <= self.overlap_buf.len());
                self.overlap_buf[..plen].copy_from_slice(&s0[s0.len() - plen..]);
                self.overlap_buf[plen..overlap_len].copy_from_slice(&s1[..slen]);
                if let Some(idx) = index_of_ignore_case(&self.overlap_buf[..overlap_len], needle) {
                    break 'search Some(s0.len() - plen + idx);
                }
            }
            if let Some(idx) = index_of_ignore_case(s1, needle) {
                break 'search Some(s0.len() + idx);
            }
            None
        };

        if let Some(off) = match_offset {
            return Some(self.highlight(off, self.needle.len()));
        }

        // 1-length needles: clear the whole window.
        if self.needle.len() == 1 {
            self.clear_and_retain_capacity();
            self.assert_integrity();
            return None;
        }

        // No match: keep the trailing `needle.len() - 1` bytes for the future overlap; prune the
        // rest. Reverse-iterate the metas to find the oldest meta that still covers the remaining
        // overlap; every older meta (and its data) is pruned.
        let nlen1 = self.needle.len() - 1;
        let mut saved = 0usize;
        let mut keep: Option<usize> = None;
        for i in (0..self.meta.len()).rev() {
            let cmlen = self.meta[i].cell_map.len();
            if cmlen >= nlen1 - saved {
                keep = Some(i);
                break;
            }
            saved += cmlen;
        }
        match keep {
            Some(j) if j > 0 => {
                let prune_data_len: usize = (0..j).map(|k| self.meta[k].cell_map.len()).sum();
                self.meta.drain(..j);
                self.data.drain(..prune_data_len);
            }
            Some(_) => {} // keep the first meta — nothing older to prune
            None => debug_assert!(saved < nlen1),
        }

        // The inner upstream `data_offset = cell_map.len - needed` is vestigial (overwritten here
        // with no intervening read), so it is dropped. Move the offset to `needle.len - 1` from the
        // end so a future `append` can complete a cross-page match.
        self.data_offset = self.data.len() - self.needle.len() + 1;
        self.assert_integrity();
        None
    }
}

/// Narrow a page-relative row coordinate (`u32`) to `CellCountInt` (upstream `@intCast`). Page rows
/// always fit.
fn cell_row(y: u32) -> CellCountInt {
    y.try_into().expect("page row fits CellCountInt")
}

/// Case-insensitive ASCII substring search (upstream `std.ascii.indexOfIgnoreCase`). An empty
/// needle matches at 0.
fn index_of_ignore_case(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| {
        haystack[i..i + needle.len()]
            .iter()
            .zip(needle)
            .all(|(h, n)| h.eq_ignore_ascii_case(n))
    })
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
    fn highlight_single_meta_forward() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.set_screen_text_lines_for_tests(&["abcdef"]);
        let node = list.first_node_ptr();

        let mut w = SlidingWindow::new(Direction::Forward, b"abc");
        // SAFETY: `list` outlives `w`; the node pointer is not invalidated below.
        unsafe { w.append(node) };

        let h = w.highlight(0, 3);
        assert_eq!(h.chunks.len(), 1);
        assert_eq!(h.chunks[0].start, 0);
        assert_eq!(h.chunks[0].end, 1);
        assert_eq!(h.top_x, 0);
        assert_eq!(h.bot_x, 2);
        assert_eq!(w.data_offset, 1);
        assert_eq!(w.meta.len(), 1);
    }

    #[test]
    fn highlight_two_meta_forward_br_path() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.set_screen_text_lines_for_tests(&["abcdef"]);
        let node = list.first_node_ptr();
        let page_rows = unsafe { node.as_ref() }.page_rows();

        let mut w = SlidingWindow::new(Direction::Forward, b"abc");
        // Append the same node twice to get two metas without multi-page plumbing.
        // SAFETY: `list` outlives `w`; the node pointer is not invalidated below.
        unsafe {
            w.append(node);
            w.append(node);
        }

        // data is "abcdef\nabcdef\n"; offsets 5..=8 span the meta boundary.
        let h = w.highlight(5, 4);
        assert_eq!(h.chunks.len(), 2);
        // First chunk: start of match to the page bottom.
        assert_eq!(h.chunks[0].start, 0);
        assert_eq!(h.chunks[0].end, page_rows);
        // Second chunk: page top to the end of match.
        assert_eq!(h.chunks[1].start, 0);
        assert_eq!(h.chunks[1].end, 1);
        assert_eq!(h.top_x, 5);
        assert_eq!(h.bot_x, 1);
        assert_eq!(w.data_offset, 6);
        assert_eq!(w.meta.len(), 2);
    }

    #[test]
    fn highlight_prunes_metas_before_the_match() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.set_screen_text_lines_for_tests(&["abcdef"]);
        let node = list.first_node_ptr();

        let mut w = SlidingWindow::new(Direction::Forward, b"abc");
        // SAFETY: `list` outlives `w`; the node pointer is not invalidated below.
        unsafe {
            w.append(node);
            w.append(node);
        }

        // Match starts in the second meta (offset 8), so the first meta is pruned.
        let h = w.highlight(8, 3);
        assert_eq!(h.chunks.len(), 1);
        assert_eq!(h.top_x, 1);
        assert_eq!(h.bot_x, 3);
        assert_eq!(w.meta.len(), 1);
        assert_eq!(w.data.iter().copied().collect::<Vec<u8>>(), b"abcdef\n");
        assert_eq!(w.data_offset, 2);
    }

    #[test]
    fn highlight_single_chunk_reverse_fixup() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.set_screen_text_lines_for_tests(&["ab", "cd"]);
        let node = list.first_node_ptr();

        let mut w = SlidingWindow::new(Direction::Reverse, b"xyz");
        // SAFETY: `list` outlives `w`; the node pointer is not invalidated below.
        unsafe { w.append(node) };

        // Reverse data is "\ndc\nba"; an interior match exercises the single-chunk reverse fixup.
        let h = w.highlight(1, 3);
        assert_eq!(h.chunks.len(), 1);
        // start/end are inverted by the reverse rule (forward {start:1,end:1} -> {0,2}).
        assert_eq!(h.chunks[0].start, 0);
        assert_eq!(h.chunks[0].end, 2);
    }

    #[test]
    fn highlight_multi_meta_reverse_keeps_serial_paired_with_node() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.grow_to_two_pages_for_tests();
        let first = list.first_node_ptr();
        let last = list.last_node_ptr();
        // The two pages have distinct serials.
        let first_serial = unsafe { first.as_ref() }.serial();
        let last_serial = unsafe { last.as_ref() }.serial();
        assert_ne!(first_serial, last_serial);

        // Reverse traversal appends pages last-to-first.
        let mut w = SlidingWindow::new(Direction::Reverse, b"x");
        // SAFETY: `list` outlives `w`; the node pointers are not invalidated below.
        unsafe {
            w.append(last);
            w.append(first);
        }

        // Span both metas (each blank page contributes one '\n').
        let h = w.highlight(0, 2);
        assert_eq!(h.chunks.len(), 2);
        // After the reverse fixup, every chunk's serial still matches its own node's serial — the
        // guard for the reverse-`serial` deviation.
        for chunk in &h.chunks {
            assert_eq!(chunk.serial, unsafe { chunk.node.as_ref() }.serial());
        }
        // And the two chunks reference the two distinct nodes.
        assert_ne!(h.chunks[0].serial, h.chunks[1].serial);
    }

    #[test]
    fn index_of_ignore_case_matches() {
        assert_eq!(index_of_ignore_case(b"hello world", b"world"), Some(6));
        assert_eq!(index_of_ignore_case(b"hello world", b"WORLD"), Some(6));
        assert_eq!(index_of_ignore_case(b"abHELlo", b"hell"), Some(2));
        assert_eq!(index_of_ignore_case(b"hello", b"xyz"), None);
        assert_eq!(index_of_ignore_case(b"hi", b"hello"), None);
        assert_eq!(index_of_ignore_case(b"anything", b""), Some(0));
    }

    #[test]
    fn next_forward_match() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.set_screen_text_lines_for_tests(&["hello world"]);
        let node = list.first_node_ptr();

        let mut w = SlidingWindow::new(Direction::Forward, b"world");
        // SAFETY: `list` outlives `w`; the node pointer is not invalidated below.
        unsafe { w.append(node) };

        let h = w.next().expect("forward match");
        assert_eq!(h.chunks.len(), 1);
        assert_eq!(h.top_x, 6); // 'w'
        assert_eq!(h.bot_x, 10); // 'd'
    }

    #[test]
    fn next_match_is_case_insensitive() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.set_screen_text_lines_for_tests(&["hello world"]);
        let node = list.first_node_ptr();

        let mut w = SlidingWindow::new(Direction::Forward, b"WORLD");
        // SAFETY: `list` outlives `w`; the node pointer is not invalidated below.
        unsafe { w.append(node) };

        let h = w.next().expect("case-insensitive match");
        assert_eq!(h.top_x, 6);
        assert_eq!(h.bot_x, 10);
    }

    #[test]
    fn next_no_match_prunes_to_overlap_tail() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.set_screen_text_lines_for_tests(&["hello world"]);
        let node = list.first_node_ptr();

        let mut w = SlidingWindow::new(Direction::Forward, b"zzzz");
        // SAFETY: `list` outlives `w`; the node pointer is not invalidated below.
        unsafe { w.append(node) };

        assert!(w.next().is_none());
        // data is "hello world\n" (12 bytes), one meta (the keep meta — nothing older to prune).
        assert_eq!(w.meta.len(), 1);
        assert_eq!(w.data_offset, 12 - 4 + 1);
    }

    #[test]
    fn next_one_length_needle_clears_the_window() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.set_screen_text_lines_for_tests(&["hello"]);
        let node = list.first_node_ptr();

        let mut w = SlidingWindow::new(Direction::Forward, b"z");
        // SAFETY: `list` outlives `w`; the node pointer is not invalidated below.
        unsafe { w.append(node) };

        assert!(w.next().is_none());
        assert!(w.data.is_empty());
        assert!(w.meta.is_empty());
    }

    #[test]
    fn next_returns_successive_matches() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.set_screen_text_lines_for_tests(&["ababab"]);
        let node = list.first_node_ptr();

        let mut w = SlidingWindow::new(Direction::Forward, b"ab");
        // SAFETY: `list` outlives `w`; the node pointer is not invalidated below.
        unsafe { w.append(node) };

        // "ababab" has "ab" at columns 0, 2, 4.
        assert!(w.next().is_some());
        assert!(w.next().is_some());
        assert!(w.next().is_some());
        assert!(w.next().is_none());
    }

    #[test]
    #[should_panic(expected = "non-empty")]
    fn next_empty_needle_panics() {
        let mut w = SlidingWindow::new(Direction::Forward, b"");
        let _ = w.next();
    }

    #[test]
    fn next_finds_overlap_across_ring_boundary() {
        // Build a wrapped `data` VecDeque whose `as_slices()` splits "hello" across the boundary.
        let mut data: VecDeque<u8> = VecDeque::with_capacity(16);
        for _ in 0..11 {
            data.push_back(0);
        }
        for _ in 0..11 {
            data.pop_front();
        }
        for &byte in b"abhellocd" {
            data.push_back(byte);
        }
        let (a, b) = data.as_slices();
        assert_eq!(a, b"abhel");
        assert_eq!(b, b"locd");

        let mut w = SlidingWindow::new(Direction::Forward, b"hello");
        w.data = data;
        // A single meta covering all nine bytes; the match stays within it, so `highlight` takes the
        // within-one-meta path and never dereferences the (dangling) node.
        w.meta.push_back(Meta {
            node: NonNull::dangling(),
            serial: 0,
            cell_map: (0..9).map(|i| Coordinate::new(i as u16, 0)).collect(),
        });

        let h = w.next().expect("overlap match");
        assert_eq!(h.chunks.len(), 1);
        // "hello" spans logical columns 2..=6.
        assert_eq!(h.top_x, 2);
        assert_eq!(h.bot_x, 6);
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
