//! Primitives to draw 2D graphics and export the result to a font atlas.
//!
//! Faithful port of upstream `font/sprite/canvas.zig`. This slice ports the
//! geometric primitives, `Color`, and the `Canvas`'s exact-pixel operations
//! (drawing and export to the atlas). Upstream backs the canvas with the `z2d`
//! vector-graphics library, but its non-path methods operate directly on the
//! raw alpha8 buffer, so the surface here is a plain `Vec<u8>`. The `z2d`-backed
//! **path-rendering** methods (`fill_path`/`stroke_path`/`triangle`/`quad`/
//! `line`/`transformation`/`get_context`) and the `draw/` glyph tables land in
//! later experiments that select a Rust path-rasterization backend.

use std::ops::Sub;

use crate::font::atlas::{Atlas, AtlasError, Format, Region};
use crate::font::sprite::raster;

/// A 2D point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Point<T> {
    pub x: T,
    pub y: T,
}

/// A line segment between two points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Line<T> {
    pub p0: Point<T>,
    pub p1: Point<T>,
}

/// A box given by two opposite corners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Box<T> {
    pub p0: Point<T>,
    pub p1: Point<T>,
}

impl<T: PartialOrd + Sub<Output = T> + Copy> Box<T> {
    /// Normalize the box (given by any two opposite corners) into a top-left
    /// `Rect` with non-negative `width`/`height`.
    pub(crate) fn rect(self) -> Rect<T> {
        // Manual `PartialOrd` min/max (rather than `Ord`) so this one impl also
        // covers the `f64` instantiation; faithful to upstream `@min`/`@max` for
        // the non-NaN coordinates the sprite code produces.
        let tl_x = if self.p0.x < self.p1.x {
            self.p0.x
        } else {
            self.p1.x
        };
        let tl_y = if self.p0.y < self.p1.y {
            self.p0.y
        } else {
            self.p1.y
        };
        let br_x = if self.p0.x > self.p1.x {
            self.p0.x
        } else {
            self.p1.x
        };
        let br_y = if self.p0.y > self.p1.y {
            self.p0.y
        } else {
            self.p1.y
        };

        Rect {
            x: tl_x,
            y: tl_y,
            width: br_x - tl_x,
            height: br_y - tl_y,
        }
    }
}

/// An axis-aligned rectangle by top-left origin and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rect<T> {
    pub x: T,
    pub y: T,
    pub width: T,
    pub height: T,
}

/// A triangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Triangle<T> {
    pub p0: Point<T>,
    pub p1: Point<T>,
    pub p2: Point<T>,
}

/// A quadrilateral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Quad<T> {
    pub p0: Point<T>,
    pub p1: Point<T>,
    pub p2: Point<T>,
    pub p3: Point<T>,
}

/// A pixel color. Only the alpha channel is used, so a pixel is "on", "off", or
/// any intermediate alpha.
///
/// Upstream is `enum(u8) { on = 255, off = 0, _ }` — a `u8` with two named
/// endpoints and an open tag for arbitrary alpha (rounded shade values, etc.).
/// In Rust that is a newtype over the alpha byte: read it as `color.0` (the
/// analog of `@intFromEnum`) and build any alpha as `Color(byte)` (the analog of
/// `@enumFromInt`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Color(pub u8);

impl Color {
    /// Fully opaque ("on").
    pub(crate) const ON: Color = Color(255);
    /// Fully transparent ("off").
    pub(crate) const OFF: Color = Color(0);
}

/// A drawing surface that exact-fills an alpha8 buffer and exports it to a font
/// atlas.
///
/// This is the `z2d`-free half of upstream's `Canvas`: the surface is a plain
/// row-major alpha8 `Vec<u8>` (which is what upstream's non-path methods operate
/// on directly), padded on all sides. The clip margins are excluded when writing
/// to the atlas. The anti-aliased path-rendering methods are a later slice.
pub(crate) struct Canvas {
    /// Row-major alpha8 buffer, `width * height` bytes.
    buf: Vec<u8>,
    /// Surface (padded) width in pixels.
    width: u32,
    /// Surface (padded) height in pixels.
    height: u32,
    padding_x: u32,
    padding_y: u32,
    clip_top: u32,
    clip_left: u32,
    clip_right: u32,
    clip_bottom: u32,
}

impl Canvas {
    /// Create a canvas for a `width` × `height` cell, padded on all sides by
    /// `padding_x`/`padding_y`. All pixels start transparent.
    pub(crate) fn new(width: u32, height: u32, padding_x: u32, padding_y: u32) -> Canvas {
        let w = width + 2 * padding_x;
        let h = height + 2 * padding_y;
        Canvas {
            buf: vec![0u8; (w * h) as usize],
            width: w,
            height: h,
            padding_x,
            padding_y,
            clip_top: 0,
            clip_left: 0,
            clip_right: 0,
            clip_bottom: 0,
        }
    }

    /// Draw and fill a single pixel, offset by the padding. Writes outside the
    /// surface are silently dropped (matching z2d `putPixel`).
    pub(crate) fn pixel(&mut self, x: i32, y: i32, color: Color) {
        let px = x + self.padding_x as i32;
        let py = y + self.padding_y as i32;
        if px < 0 || py < 0 || px >= self.width as i32 || py >= self.height as i32 {
            return;
        }
        self.buf[(self.width as i32 * py + px) as usize] = color.0;
    }

    /// Read back the alpha at a cell coordinate, applying the padding offset
    /// (the inverse of [`pixel`](Self::pixel)). Out-of-surface reads return `0`.
    /// Test-only helper so sibling modules can inspect drawn ink without
    /// touching the private buffer.
    #[cfg(test)]
    pub(crate) fn get(&self, x: i32, y: i32) -> u8 {
        let px = x + self.padding_x as i32;
        let py = y + self.padding_y as i32;
        if px < 0 || py < 0 || px >= self.width as i32 || py >= self.height as i32 {
            return 0;
        }
        self.buf[(self.width as i32 * py + px) as usize]
    }

    /// Draw and fill a rectangle. This is also the main primitive for lines
    /// (which are just skinny rectangles).
    pub(crate) fn rect(&mut self, v: Rect<i32>, color: Color) {
        let mut y = v.y;
        while y < v.y + v.height {
            let mut x = v.x;
            while x < v.x + v.width {
                self.pixel(x, y, color);
                x += 1;
            }
            y += 1;
        }
    }

    /// Convenience wrapper for [`rect`](Self::rect) taking two opposite corners.
    pub(crate) fn r#box(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Color) {
        let r = Box {
            p0: Point { x: x0, y: y0 },
            p1: Point { x: x1, y: y1 },
        }
        .rect();
        self.rect(r, color);
    }

    /// Stroke an anti-aliased line from `p0` to `p1` (in unpadded cell
    /// coordinates) with the given `thickness`, painting the opaque (`.on`)
    /// source. Faithful port of upstream `Canvas.line`: it strokes a 2-node
    /// butt-cap path. The padding translation (upstream's CTM) is applied here;
    /// the stroke is rasterized with 4× multisample anti-aliasing into the
    /// padded surface.
    pub(crate) fn line(&mut self, p0: raster::Point, p1: raster::Point, thickness: f64) {
        let p0t = raster::Point::new(p0.x + self.padding_x as f64, p0.y + self.padding_y as f64);
        let p1t = raster::Point::new(p1.x + self.padding_x as f64, p1.y + self.padding_y as f64);
        let poly = raster::stroke_line(p0t, p1t, thickness, raster::MSAA_SCALE as f64);
        raster::fill_polygon(
            &mut self.buf,
            self.width as i32,
            self.height as i32,
            &poly,
            raster::FillRule::NonZero,
        );
    }

    /// Stroke an anti-aliased open path (in unpadded cell coordinates) with the
    /// given `thickness`, painting the opaque (`.on`) source. Faithful port of
    /// upstream `Canvas.strokePath` as used by the box arcs: butt caps, miter
    /// joins (`miter_limit` 10, `tolerance` 0.1) — z2d's `StrokeOptions`
    /// defaults. The padding translation (upstream's CTM) is applied to every
    /// node here; the stroke is rasterized with 4× multisample anti-aliasing
    /// into the padded surface.
    pub(crate) fn stroke_path(&mut self, nodes: &[raster::PathNode], thickness: f64) {
        let translated: Vec<raster::PathNode> =
            nodes.iter().map(|n| self.translate_node(*n)).collect();
        let poly = raster::stroke_path(
            &translated,
            thickness,
            raster::MSAA_SCALE as f64,
            10.0,
            0.1,
            raster::JoinMode::Miter,
        );
        raster::fill_polygon(
            &mut self.buf,
            self.width as i32,
            self.height as i32,
            &poly,
            raster::FillRule::NonZero,
        );
    }

    /// Offset a path node's point(s) by the surface padding (the upstream
    /// translation-only CTM).
    fn translate_node(&self, node: raster::PathNode) -> raster::PathNode {
        let dx = self.padding_x as f64;
        let dy = self.padding_y as f64;
        let t = |p: raster::Point| raster::Point::new(p.x + dx, p.y + dy);
        match node {
            raster::PathNode::MoveTo(p) => raster::PathNode::MoveTo(t(p)),
            raster::PathNode::LineTo(p) => raster::PathNode::LineTo(t(p)),
            raster::PathNode::CurveTo { p1, p2, p3 } => raster::PathNode::CurveTo {
                p1: t(p1),
                p2: t(p2),
                p3: t(p3),
            },
            raster::PathNode::ClosePath => raster::PathNode::ClosePath,
        }
    }

    /// Adjust the clip boundaries to trim off any fully transparent rows or
    /// columns. (Bypasses any drawing abstraction for performance.)
    fn trim(&mut self) {
        let width = self.width;
        let height = self.height;

        while self.clip_top < height - self.clip_bottom {
            let y = self.clip_top;
            let x0 = self.clip_left;
            let x1 = width - self.clip_right;
            let row = &self.buf[(y * width + x0) as usize..(y * width + x1) as usize];
            if row.iter().any(|&v| v != 0) {
                break;
            }
            self.clip_top += 1;
        }

        while self.clip_bottom < height - self.clip_top {
            let y = (height - self.clip_bottom).saturating_sub(1);
            let x0 = self.clip_left;
            let x1 = width - self.clip_right;
            let row = &self.buf[(y * width + x0) as usize..(y * width + x1) as usize];
            if row.iter().any(|&v| v != 0) {
                break;
            }
            self.clip_bottom += 1;
        }

        while self.clip_left < width - self.clip_right {
            let x = self.clip_left;
            let y0 = self.clip_top;
            let y1 = height - self.clip_bottom;
            if (y0..y1).any(|y| self.buf[(y * width + x) as usize] != 0) {
                break;
            }
            self.clip_left += 1;
        }

        while self.clip_right < width - self.clip_left {
            let x = (width - self.clip_right).saturating_sub(1);
            let y0 = self.clip_top;
            let y1 = height - self.clip_bottom;
            if (y0..y1).any(|y| self.buf[(y * width + x) as usize] != 0) {
                break;
            }
            self.clip_right += 1;
        }
    }

    /// Write the drawn glyph into the atlas, excluding the (trimmed) clip
    /// margins. The atlas must be grayscale.
    pub(crate) fn write_atlas(&mut self, atlas: &mut Atlas) -> Result<Region, AtlasError> {
        assert!(atlas.format() == Format::Grayscale);

        self.trim();

        let sfc_width = self.width;
        let sfc_height = self.height;

        // Subtract the clip margins to get the region size.
        let region_width = sfc_width
            .saturating_sub(self.clip_left)
            .saturating_sub(self.clip_right);
        let region_height = sfc_height
            .saturating_sub(self.clip_top)
            .saturating_sub(self.clip_bottom);

        let region = atlas.reserve(region_width, region_height)?;

        if region.width > 0 && region.height > 0 {
            debug_assert!(region.width == region_width);
            debug_assert!(region.height == region_height);
            atlas.set_from_larger(region, &self.buf, sfc_width, self.clip_left, self.clip_top);
        }

        Ok(region)
    }

    /// Zero the clip margins of the buffer.
    ///
    /// Only really useful for tests, since the clip region is automatically
    /// excluded when writing to an atlas with [`write_atlas`](Self::write_atlas).
    pub(crate) fn clear_clipping_regions(&mut self) {
        let width = self.width;
        let height = self.height;

        for y in 0..height {
            for x in 0..self.clip_left {
                self.buf[(y * width + x) as usize] = 0;
            }
        }
        for y in 0..height {
            for x in (width - self.clip_right)..width {
                self.buf[(y * width + x) as usize] = 0;
            }
        }
        for y in 0..self.clip_top {
            for x in 0..width {
                self.buf[(y * width + x) as usize] = 0;
            }
        }
        for y in (height - self.clip_bottom)..height {
            for x in 0..width {
                self.buf[(y * width + x) as usize] = 0;
            }
        }
    }

    /// Invert the alpha of every pixel.
    pub(crate) fn invert(&mut self) {
        for v in &mut self.buf {
            *v = 255 - *v;
        }
    }

    /// Mirror the canvas horizontally.
    pub(crate) fn flip_horizontal(&mut self) {
        let clone = self.buf.clone();
        let width = self.width;
        let height = self.height;
        for y in 0..height {
            for x in 0..width {
                self.buf[(y * width + x) as usize] = clone[(y * width + width - x - 1) as usize];
            }
        }
        std::mem::swap(&mut self.clip_left, &mut self.clip_right);
    }

    /// Mirror the canvas vertically.
    pub(crate) fn flip_vertical(&mut self) {
        let clone = self.buf.clone();
        let width = self.width;
        let height = self.height;
        for y in 0..height {
            for x in 0..width {
                self.buf[(y * width + x) as usize] = clone[((height - y - 1) * width + x) as usize];
            }
        }
        std::mem::swap(&mut self.clip_top, &mut self.clip_bottom);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_rect_normalizes() {
        // Corners given out of order (bottom-left, top-right style).
        let b = Box {
            p0: Point { x: 3, y: 5 },
            p1: Point { x: 1, y: 9 },
        };
        assert_eq!(
            b.rect(),
            Rect {
                x: 1,
                y: 5,
                width: 2,
                height: 4,
            }
        );
    }

    #[test]
    fn box_rect_already_ordered() {
        let b: Box<u32> = Box {
            p0: Point { x: 2, y: 4 },
            p1: Point { x: 10, y: 7 },
        };
        assert_eq!(
            b.rect(),
            Rect {
                x: 2,
                y: 4,
                width: 8,
                height: 3,
            }
        );
    }

    #[test]
    fn box_rect_float() {
        let b: Box<f64> = Box {
            p0: Point { x: 3.5, y: 1.0 },
            p1: Point { x: 1.5, y: 4.0 },
        };
        assert_eq!(
            b.rect(),
            Rect {
                x: 1.5,
                y: 1.0,
                width: 2.0,
                height: 3.0,
            }
        );
    }

    #[test]
    fn color_alpha() {
        assert_eq!(Color::ON.0, 255);
        assert_eq!(Color::OFF.0, 0);
        assert_eq!(Color(128).0, 128);
    }

    #[test]
    fn primitive_construction() {
        let line = Line {
            p0: Point { x: 0, y: 0 },
            p1: Point { x: 4, y: 5 },
        };
        assert_eq!(line.p1.x, 4);
        assert_eq!(line.p1.y, 5);

        let tri = Triangle {
            p0: Point { x: 0.0, y: 0.0 },
            p1: Point { x: 1.0, y: 0.0 },
            p2: Point { x: 0.0, y: 1.0 },
        };
        assert_eq!(tri.p2.y, 1.0);

        let quad = Quad {
            p0: Point { x: 0, y: 0 },
            p1: Point { x: 1, y: 0 },
            p2: Point { x: 1, y: 1 },
            p3: Point { x: 0, y: 1 },
        };
        assert_eq!(quad.p2, Point { x: 1, y: 1 });
    }

    #[test]
    fn pixel_padding_and_bounds() {
        let mut c = Canvas::new(2, 2, 1, 1); // surface 4x4
        c.pixel(0, 0, Color::ON); // -> padded (1,1) -> buf[5]
        c.pixel(-2, 0, Color::ON); // -> padded (-1,1), px < 0, dropped
        c.pixel(3, 0, Color::ON); // -> padded (4,1), px >= width, dropped
        assert_eq!(c.buf[5], 255);
        assert_eq!(c.buf.iter().filter(|&&v| v != 0).count(), 1);
    }

    #[test]
    fn rect_and_box_fill() {
        let mut c = Canvas::new(4, 4, 0, 0);
        c.rect(
            Rect {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            Color::ON,
        );
        // (0,0),(1,0),(0,1),(1,1) lit; nothing else.
        for &(x, y) in &[(0u32, 0u32), (1, 0), (0, 1), (1, 1)] {
            assert_eq!(c.buf[(y * 4 + x) as usize], 255);
        }
        assert_eq!(c.buf.iter().filter(|&&v| v != 0).count(), 4);

        // `box` with reversed corners fills the same four pixels.
        let mut c2 = Canvas::new(4, 4, 0, 0);
        c2.r#box(2, 2, 0, 0, Color::ON);
        assert_eq!(c.buf, c2.buf);
    }

    #[test]
    fn trim_clips_transparent_margins() {
        let mut c = Canvas::new(2, 2, 1, 1); // surface 4x4
        c.pixel(0, 0, Color::ON); // lit at surface (1,1)
        c.trim();
        assert_eq!(c.clip_top, 1);
        assert_eq!(c.clip_bottom, 2);
        assert_eq!(c.clip_left, 1);
        assert_eq!(c.clip_right, 2);
    }

    #[test]
    fn write_atlas_exports_trimmed() {
        let mut c = Canvas::new(2, 2, 1, 1); // surface 4x4
        c.pixel(0, 0, Color::ON); // lit at surface (1,1)

        let mut atlas = Atlas::new(8, Format::Grayscale);
        let region = c.write_atlas(&mut atlas).unwrap();
        assert_eq!(region.width, 1);
        assert_eq!(region.height, 1);

        // The single 255 byte is at the region's offset in the atlas.
        let off = (region.y * 8 + region.x) as usize;
        assert_eq!(atlas.data()[off], 255);
    }

    #[test]
    fn clear_clipping_regions_zeros_margins() {
        let mut c = Canvas::new(2, 2, 1, 1); // surface 4x4
        for v in &mut c.buf {
            *v = 255;
        }
        c.clip_left = 1;
        c.clip_right = 1;
        c.clip_top = 1;
        c.clip_bottom = 1;
        c.clear_clipping_regions();

        // Only the inner 2x2 (surface rows/cols 1..3) stays 255.
        for y in 0..4u32 {
            for x in 0..4u32 {
                let inner = (1..3).contains(&x) && (1..3).contains(&y);
                let expected = if inner { 255 } else { 0 };
                assert_eq!(c.buf[(y * 4 + x) as usize], expected, "at ({x},{y})");
            }
        }
    }

    #[test]
    fn invert_and_flips() {
        // invert: 0 -> 255.
        let mut c = Canvas::new(2, 2, 0, 0);
        c.invert();
        assert!(c.buf.iter().all(|&v| v == 255));

        // flip_horizontal mirrors columns and swaps left/right clips.
        let mut c = Canvas::new(2, 2, 0, 0);
        c.pixel(0, 0, Color(10)); // buf[0]
        c.clip_left = 1;
        c.flip_horizontal();
        // (0,0) moves to (1,0) = buf[1].
        assert_eq!(c.buf[1], 10);
        assert_eq!(c.buf[0], 0);
        assert_eq!(c.clip_right, 1);
        assert_eq!(c.clip_left, 0);

        // flip_vertical mirrors rows and swaps top/bottom clips.
        let mut c = Canvas::new(2, 2, 0, 0);
        c.pixel(0, 0, Color(20)); // buf[0]
        c.clip_top = 1;
        c.flip_vertical();
        // (0,0) moves to (0,1) = buf[2] in a 2-wide surface.
        assert_eq!(c.buf[2], 20);
        assert_eq!(c.buf[0], 0);
        assert_eq!(c.clip_bottom, 1);
        assert_eq!(c.clip_top, 0);
    }
}
