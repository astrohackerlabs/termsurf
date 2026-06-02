//! A texture atlas (<https://en.wikipedia.org/wiki/Texture_atlas>).
//!
//! Faithful port of upstream `font/Atlas.zig`. The packer is the skyline /
//! shelf-next-fit variant from Jukka Jylänki's "A Thousand Ways to Pack the
//! Bin" (as used by freetype-gl): the atlas hands out sub-rectangles of a square
//! texture for glyph sprites.
//!
//! Limitations carried over from upstream: written data must be packed (no
//! custom strides), and the texture is always square (regions written into it
//! need not be).
//!
//! The full public surface is ported (`new`/`clear`, `reserve` with `fit` and
//! `merge`, `set`, `set_from_larger`, `grow`, and `dump`); only the WASM
//! bindings are out of scope (macOS-only).

use std::sync::atomic::{AtomicUsize, Ordering};

/// The pixel format of the texture data written into the atlas. This is uniform
/// for all textures in one atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Format {
    /// 1 byte per pixel grayscale.
    Grayscale = 0,
    /// 3 bytes per pixel BGR.
    Bgr = 1,
    /// 4 bytes per pixel BGRA.
    Bgra = 2,
}

impl Format {
    /// Bytes per pixel for this format. Returned as `u32` so it composes with the
    /// offset arithmetic without casts.
    pub(crate) fn depth(self) -> u32 {
        match self {
            Format::Grayscale => 1,
            Format::Bgr => 3,
            Format::Bgra => 4,
        }
    }
}

/// A node (rectangle) of available space on the skyline frontier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Node {
    x: u32,
    y: u32,
    width: u32,
}

/// A reserved region within the texture atlas, acquired from [`Atlas::reserve`].
/// A region reservation is required to write data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// An error reserving space in the atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AtlasError {
    /// The atlas cannot fit the desired region. The atlas must be enlarged.
    AtlasFull,
}

/// Number of nodes to preallocate in the node list on construction.
pub(crate) const NODE_PREALLOC: usize = 64;

/// A square texture atlas.
pub(crate) struct Atlas {
    /// The raw texture data.
    data: Vec<u8>,
    /// Width and height of the (always square) atlas texture.
    size: u32,
    /// The nodes (rectangles) of available space.
    nodes: Vec<Node>,
    /// The format of the texture data being written into the atlas.
    format: Format,
    /// Incremented on every data change, so a GPU-upload consumer can observe
    /// that the texture data changed since it was last sent. Read atomically.
    modified: AtomicUsize,
    /// Incremented on every resize, so a consumer can tell whether a GPU texture
    /// can be updated in-place or needs reallocation. Read atomically.
    resized: AtomicUsize,
}

impl Atlas {
    /// Create an atlas of `size` × `size` pixels in the given format. All data is
    /// zeroed and a single full-texture node (inside a 1px border) is seeded.
    ///
    /// Allocation is infallible in Rust (it aborts on OOM rather than returning
    /// an error), so unlike upstream `init` this returns an `Atlas` directly.
    pub(crate) fn new(size: u32, format: Format) -> Atlas {
        let depth = format.depth() as usize;
        let mut atlas = Atlas {
            data: vec![0u8; size as usize * size as usize * depth],
            size,
            nodes: Vec::with_capacity(NODE_PREALLOC),
            format,
            modified: AtomicUsize::new(0),
            resized: AtomicUsize::new(0),
        };

        // Sets up the initial node state.
        atlas.clear();

        atlas
    }

    /// The current value of the `modified` counter.
    pub(crate) fn modified(&self) -> usize {
        self.modified.load(Ordering::Relaxed)
    }

    /// The current value of the `resized` counter.
    pub(crate) fn resized(&self) -> usize {
        self.resized.load(Ordering::Relaxed)
    }

    /// The pixel format of this atlas.
    pub(crate) fn format(&self) -> Format {
        self.format
    }

    /// The raw texture data (read-only).
    pub(crate) fn data(&self) -> &[u8] {
        &self.data
    }

    /// Reserve a region of `width` × `height` within the atlas.
    ///
    /// May grow the internal node list. This does not enlarge the texture if it
    /// is full; it returns [`AtlasError::AtlasFull`] instead.
    pub(crate) fn reserve(&mut self, width: u32, height: u32) -> Result<Region, AtlasError> {
        // x, y are populated within the best-index search below.
        let mut region = Region {
            x: 0,
            y: 0,
            width,
            height,
        };

        // A zero-size region is returned as-is. This simplifies callers that
        // might write empty data.
        if width == 0 && height == 0 {
            return Ok(region);
        }

        // Find the node to insert the new region's node at.
        let best_idx = {
            let mut best_height: u32 = u32::MAX;
            let mut best_width: u32 = best_height;
            let mut chosen: Option<usize> = None;

            let mut i: usize = 0;
            while i < self.nodes.len() {
                // Check if our region fits within this node.
                if let Some(y) = self.fit(i, width, height) {
                    let node = self.nodes[i];
                    if (y + height) < best_height
                        || ((y + height) == best_height
                            && node.width > 0
                            && node.width < best_width)
                    {
                        chosen = Some(i);
                        best_width = node.width;
                        best_height = y + height;
                        region.x = node.x;
                        region.y = y;
                    }
                }
                i += 1;
            }

            // If we never found a chosen index, the atlas cannot fit our region.
            match chosen {
                Some(idx) => idx,
                None => return Err(AtlasError::AtlasFull),
            }
        };

        // Insert the new node for this rectangle at the exact best index.
        self.nodes.insert(
            best_idx,
            Node {
                x: region.x,
                y: region.y + height,
                width,
            },
        );

        // Optimize our rectangles: trim/remove nodes the new node overlaps.
        // `i` stays fixed: on removal the next node shifts into index `i` and is
        // reprocessed (upstream's `i -= 1; continue` over a `+= 1` loop step);
        // any surviving node breaks the loop.
        let i = best_idx + 1;
        while i < self.nodes.len() {
            let prev = self.nodes[i - 1];
            if self.nodes[i].x < prev.x + prev.width {
                let shrink = prev.x + prev.width - self.nodes[i].x;
                self.nodes[i].x += shrink;
                self.nodes[i].width = self.nodes[i].width.saturating_sub(shrink);
                if self.nodes[i].width == 0 {
                    self.nodes.remove(i);
                    // Reprocess the node that shifted into index `i`.
                    continue;
                }
            }

            break;
        }
        self.merge();

        Ok(region)
    }

    /// Attempt to fit a `width` × `height` rectangle into the node at `idx`.
    ///
    /// Returns the `y` within the texture where the rectangle can be placed (its
    /// `x` is the node's `x`), or `None` if it would cross the right/bottom 1px
    /// border.
    fn fit(&self, idx: usize, width: u32, height: u32) -> Option<u32> {
        // If the added width exceeds our texture size, it doesn't fit.
        let node = self.nodes[idx];
        if (node.x + width) > (self.size - 1) {
            return None;
        }

        // Go node by node looking for space that can fit our width.
        let mut y = node.y;
        let mut i = idx;
        let mut width_left = width;
        while width_left > 0 {
            let n = self.nodes[i];
            if n.y > y {
                y = n.y;
            }

            // If the added height exceeds our texture size, it doesn't fit.
            if (y + height) > (self.size - 1) {
                return None;
            }

            width_left = width_left.saturating_sub(n.width);
            i += 1;
        }

        Some(y)
    }

    /// Merge adjacent nodes with the same `y` value.
    fn merge(&mut self) {
        let mut i: usize = 0;
        while i + 1 < self.nodes.len() {
            let next = self.nodes[i + 1];
            if self.nodes[i].y == next.y {
                self.nodes[i].width += next.width;
                self.nodes.remove(i + 1);
                continue;
            }

            i += 1;
        }
    }

    /// Set the data for a reserved region. The data must fit the region exactly
    /// and be packed in the atlas's format.
    pub(crate) fn set(&mut self, reg: Region, data: &[u8]) {
        debug_assert!(reg.x < (self.size - 1));
        debug_assert!((reg.x + reg.width) <= (self.size - 1));
        debug_assert!(reg.y < (self.size - 1));
        debug_assert!((reg.y + reg.height) <= (self.size - 1));

        let depth = self.format.depth() as usize;
        let size = self.size as usize;
        let rx = reg.x as usize;
        let ry = reg.y as usize;
        let row = reg.width as usize * depth;
        for i in 0..reg.height as usize {
            let tex_offset = ((ry + i) * size + rx) * depth;
            let data_offset = i * row;
            self.data[tex_offset..tex_offset + row]
                .copy_from_slice(&data[data_offset..data_offset + row]);
        }

        self.modified.fetch_add(1, Ordering::Relaxed);
    }

    /// Like [`set`](Self::set), but the source has its own row stride
    /// (`src_width`) and an `(src_x, src_y)` offset, so a sub-rectangle of a
    /// larger buffer can be copied into the region.
    pub(crate) fn set_from_larger(
        &mut self,
        reg: Region,
        src: &[u8],
        src_width: u32,
        src_x: u32,
        src_y: u32,
    ) {
        debug_assert!(reg.x < (self.size - 1));
        debug_assert!((reg.x + reg.width) <= (self.size - 1));
        debug_assert!(reg.y < (self.size - 1));
        debug_assert!((reg.y + reg.height) <= (self.size - 1));

        let depth = self.format.depth() as usize;
        let size = self.size as usize;
        let rx = reg.x as usize;
        let ry = reg.y as usize;
        let sw = src_width as usize;
        let sx = src_x as usize;
        let sy = src_y as usize;
        let row = reg.width as usize * depth;
        for i in 0..reg.height as usize {
            let tex_offset = ((ry + i) * size + rx) * depth;
            let src_offset = ((sy + i) * sw + sx) * depth;
            self.data[tex_offset..tex_offset + row]
                .copy_from_slice(&src[src_offset..src_offset + row]);
        }

        self.modified.fetch_add(1, Ordering::Relaxed);
    }

    /// Grow the texture to `size_new` × `size_new`, preserving all previously
    /// written data. `size_new` must not be smaller than the current size.
    ///
    /// Infallible in Rust (`Vec` allocation aborts on OOM), so unlike upstream
    /// `grow` this returns nothing.
    pub(crate) fn grow(&mut self, size_new: u32) {
        assert!(size_new >= self.size);
        if size_new == self.size {
            return;
        }

        let depth = self.format.depth() as usize;
        let size_old = self.size;

        // Swap in the new (already-zeroed) buffer and keep the old one to copy
        // from. `data_old` is a separate binding, so it does not alias `self`.
        let data_old = std::mem::replace(
            &mut self.data,
            vec![0u8; size_new as usize * size_new as usize * depth],
        );
        self.size = size_new;

        // Copy the old data over. We take the full old width starting at x = 0
        // (no border skip) so we can avoid strides, skipping the first and last
        // border rows.
        self.set(
            Region {
                x: 0,
                y: 1,
                width: size_old,
                height: size_old - 2,
            },
            &data_old[size_old as usize * depth..],
        );

        // Add the new rectangle for the added right-hand space.
        self.nodes.push(Node {
            x: size_old - 1,
            y: 1,
            width: size_new - size_old,
        });

        // We are both modified and resized.
        self.modified.fetch_add(1, Ordering::Relaxed);
        self.resized.fetch_add(1, Ordering::Relaxed);
    }

    /// Dump the atlas as a PPM to a writer, for debugging. Only grayscale and
    /// BGR are supported (BGR is written as-is, so red/blue are swapped versus
    /// true RGB — a debug-only wart). Panics on any other format.
    pub(crate) fn dump<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<()> {
        let magic = match self.format {
            Format::Grayscale => '5',
            Format::Bgr => '6',
            Format::Bgra => panic!("cannot dump this atlas format: {:?}", self.format),
        };
        write!(w, "P{}\n{} {}\n255\n", magic, self.size, self.size)?;
        w.write_all(&self.data)
    }

    /// Reset the atlas: zero the data and re-seed the single full-texture node
    /// inside the 1px border.
    pub(crate) fn clear(&mut self) {
        self.modified.fetch_add(1, Ordering::Relaxed);
        self.data.fill(0);
        self.nodes.clear();

        // The initial rectangle is the full texture inside a 1px border, which
        // avoids artifacting when sampling the texture.
        self.nodes.push(Node {
            x: 1,
            y: 1,
            width: self.size - 2,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_depth() {
        assert_eq!(Format::Grayscale.depth(), 1);
        assert_eq!(Format::Bgr.depth(), 3);
        assert_eq!(Format::Bgra.depth(), 4);
    }

    #[test]
    fn exact_fit() {
        let mut atlas = Atlas::new(34, Format::Grayscale); // +2 for 1px border
        let modified = atlas.modified();
        atlas.reserve(32, 32).unwrap();
        // reserve does not change the texture data.
        assert_eq!(modified, atlas.modified());
        assert_eq!(atlas.reserve(1, 1), Err(AtlasError::AtlasFull));
    }

    #[test]
    fn doesnt_fit() {
        let mut atlas = Atlas::new(32, Format::Grayscale);
        // Doesn't fit due to the border (only 30×30 usable).
        assert_eq!(atlas.reserve(32, 32), Err(AtlasError::AtlasFull));
    }

    #[test]
    fn fit_multiple() {
        let mut atlas = Atlas::new(32, Format::Grayscale);
        atlas.reserve(15, 30).unwrap();
        atlas.reserve(15, 30).unwrap();
        assert_eq!(atlas.reserve(1, 1), Err(AtlasError::AtlasFull));
    }

    #[test]
    fn writing_data() {
        let mut atlas = Atlas::new(32, Format::Grayscale);
        let reg = atlas.reserve(2, 2).unwrap();
        let old = atlas.modified();
        atlas.set(reg, &[1, 2, 3, 4]);
        assert!(atlas.modified() > old);

        // 33 because of the 1px border and so on.
        assert_eq!(atlas.data[33], 1);
        assert_eq!(atlas.data[34], 2);
        assert_eq!(atlas.data[65], 3);
        assert_eq!(atlas.data[66], 4);
    }

    #[test]
    fn writing_bgr_data() {
        let mut atlas = Atlas::new(32, Format::Bgr);
        // BGR is 3 bytes per pixel.
        let reg = atlas.reserve(1, 2).unwrap();
        atlas.set(
            reg,
            &[
                1, 2, 3, //
                4, 5, 6,
            ],
        );

        let depth = atlas.format.depth() as usize;
        assert_eq!(atlas.data[33 * depth], 1);
        assert_eq!(atlas.data[33 * depth + 1], 2);
        assert_eq!(atlas.data[33 * depth + 2], 3);
        assert_eq!(atlas.data[65 * depth], 4);
        assert_eq!(atlas.data[65 * depth + 1], 5);
        assert_eq!(atlas.data[65 * depth + 2], 6);
    }

    #[test]
    fn writing_data_from_larger_source() {
        let mut atlas = Atlas::new(32, Format::Grayscale);
        let reg = atlas.reserve(2, 2).unwrap();
        let old = atlas.modified();
        #[rustfmt::skip]
        atlas.set_from_larger(
            reg,
            &[
                8, 8, 8, 8, 8,
                8, 8, 1, 2, 8,
                8, 8, 3, 4, 8,
                8, 8, 8, 8, 8,
            ],
            5,
            2,
            1,
        );
        assert!(atlas.modified() > old);

        // 33 because of the 1px border and so on.
        assert_eq!(atlas.data[33], 1);
        assert_eq!(atlas.data[34], 2);
        assert_eq!(atlas.data[65], 3);
        assert_eq!(atlas.data[66], 4);

        // None of the `8`s outside the specified region should reach the atlas.
        assert!(!atlas.data.contains(&8));
    }

    #[test]
    fn grow_preserves_data() {
        let mut atlas = Atlas::new(4, Format::Grayscale); // +2 for 1px border
        let reg = atlas.reserve(2, 2).unwrap();
        assert_eq!(atlas.reserve(1, 1), Err(AtlasError::AtlasFull));

        // Write data so we can verify that growing doesn't mess it up.
        atlas.set(reg, &[1, 2, 3, 4]);
        assert_eq!(atlas.data[5], 1);
        assert_eq!(atlas.data[6], 2);
        assert_eq!(atlas.data[9], 3);
        assert_eq!(atlas.data[10], 4);

        // Expanding by exactly 1 should fit our new 1x1 block.
        let old_modified = atlas.modified();
        let old_resized = atlas.resized();
        atlas.grow(atlas.size + 1);
        assert!(atlas.modified() > old_modified);
        assert!(atlas.resized() > old_resized);
        atlas.reserve(1, 1).unwrap();

        // Data should still be set; offsets change due to the new size.
        let size = atlas.size as usize;
        assert_eq!(atlas.data[size + 1], 1);
        assert_eq!(atlas.data[size + 2], 2);
        assert_eq!(atlas.data[size * 2 + 1], 3);
        assert_eq!(atlas.data[size * 2 + 2], 4);
    }

    #[test]
    fn grow_bgr() {
        // 4x4 with a 1px border leaves only 2x2 usable.
        let mut atlas = Atlas::new(4, Format::Bgr);

        // Get our 2x2, which is ALL the usable space.
        let reg = atlas.reserve(2, 2).unwrap();
        assert_eq!(atlas.reserve(1, 1), Err(AtlasError::AtlasFull));

        // BGR is 3 bytes per pixel.
        #[rustfmt::skip]
        atlas.set(reg, &[
            10, 11, 12, // (0, 0) from top-left
            13, 14, 15, // (1, 0)
            20, 21, 22, // (0, 1)
            23, 24, 25, // (1, 1)
        ]);

        // Top-left skips the first row (size * depth) and first column (depth).
        let depth = atlas.format.depth() as usize;
        let mut tl = (atlas.size as usize * depth) + depth;
        assert_eq!(atlas.data[tl], 10);
        assert_eq!(atlas.data[tl + 1], 11);
        assert_eq!(atlas.data[tl + 2], 12);
        assert_eq!(atlas.data[tl + 3], 13);
        assert_eq!(atlas.data[tl + 4], 14);
        assert_eq!(atlas.data[tl + 5], 15);
        assert_eq!(atlas.data[tl + 6], 0); // border

        tl += atlas.size as usize * depth; // next row
        assert_eq!(atlas.data[tl], 20);
        assert_eq!(atlas.data[tl + 1], 21);
        assert_eq!(atlas.data[tl + 2], 22);
        assert_eq!(atlas.data[tl + 3], 23);
        assert_eq!(atlas.data[tl + 4], 24);
        assert_eq!(atlas.data[tl + 5], 25);
        assert_eq!(atlas.data[tl + 6], 0); // border

        // Expanding by exactly 1 should fit the new 1x1 block.
        atlas.grow(atlas.size + 1);

        // Data should be in the same place accounting for the new size.
        tl = (atlas.size as usize * depth) + depth;
        assert_eq!(atlas.data[tl], 10);
        assert_eq!(atlas.data[tl + 1], 11);
        assert_eq!(atlas.data[tl + 2], 12);
        assert_eq!(atlas.data[tl + 3], 13);
        assert_eq!(atlas.data[tl + 4], 14);
        assert_eq!(atlas.data[tl + 5], 15);
        assert_eq!(atlas.data[tl + 6], 0); // border

        tl += atlas.size as usize * depth; // next row
        assert_eq!(atlas.data[tl], 20);
        assert_eq!(atlas.data[tl + 1], 21);
        assert_eq!(atlas.data[tl + 2], 22);
        assert_eq!(atlas.data[tl + 3], 23);
        assert_eq!(atlas.data[tl + 4], 24);
        assert_eq!(atlas.data[tl + 5], 25);
        assert_eq!(atlas.data[tl + 6], 0); // border

        // Should fit the new blocks around the edges.
        atlas.reserve(1, 3).unwrap();
        atlas.reserve(2, 1).unwrap();
        assert_eq!(atlas.reserve(1, 1), Err(AtlasError::AtlasFull));
    }

    #[test]
    fn dump_grayscale_header() {
        let atlas = Atlas::new(4, Format::Grayscale);
        let mut buf: Vec<u8> = Vec::new();
        atlas.dump(&mut buf).unwrap();

        let header = b"P5\n4 4\n255\n";
        assert_eq!(&buf[..header.len()], header);
        // The remainder is the raw texture data (4*4*1 bytes).
        assert_eq!(&buf[header.len()..], &atlas.data[..]);
        assert_eq!(buf.len(), header.len() + 4 * 4);
    }
}
