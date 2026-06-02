//! Primitives to draw 2D graphics and export the result to a font atlas.
//!
//! Faithful port of upstream `font/sprite/canvas.zig`. This slice ports the
//! dependency-free geometric primitives and `Color`; the `Canvas` itself (which
//! upstream backs with the `z2d` vector-graphics library) and the `draw/` glyph
//! tables land in later experiments.

use std::ops::Sub;

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
}
