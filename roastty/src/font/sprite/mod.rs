//! Sprite font subsystem.
//!
//! Faithful port of upstream `font/sprite/`, which procedurally draws
//! box-drawing, block, Powerline, Braille, and legacy-computing glyphs directly
//! into the atlas. This slice establishes the module and the geometric
//! primitives; the `Canvas` (a 2D rasterization surface) and the `draw/` glyph
//! tables land in later experiments.

pub(crate) mod canvas;
pub(crate) mod draw;
