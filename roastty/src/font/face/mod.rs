//! Font faces.
//!
//! Faithful (macOS) port of upstream `font/face/`. On macOS the face is backed
//! by CoreText (`CTFont`); the Linux/FreeType and WASM backends are out of
//! scope. This slice establishes the `CoreText`-backed `Face` and its raw
//! table access; `getMetrics` assembly and glyph rasterization land in later
//! experiments.

pub(crate) mod coretext;
