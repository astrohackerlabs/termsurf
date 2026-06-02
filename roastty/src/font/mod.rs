#![allow(dead_code)]
// The font subsystem is consumed by later font and renderer slices.

//! Font subsystem.
//!
//! Faithful port of upstream `font/`. This slice establishes the module and the
//! `Glyph` value type; rasterization, atlas, faces, metrics, and shaping land in
//! later experiments.

pub(crate) mod glyph;
