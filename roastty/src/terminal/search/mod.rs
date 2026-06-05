//! The terminal search subsystem (port of upstream `terminal/search`). So far it lands the complete
//! `SlidingWindow` matcher (`sliding_window`), the `ActiveSearch` active-area searcher (`active`),
//! and the `PageListSearch` history searcher (`pagelist`); the `screen` / `viewport` searchers and
//! the search `Thread` are deferred to later slices.

#[allow(dead_code)]
pub(crate) mod active;

#[allow(dead_code)]
pub(crate) mod pagelist;

#[allow(dead_code)]
pub(crate) mod sliding_window;
