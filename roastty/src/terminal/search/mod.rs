//! The terminal search subsystem (port of upstream `terminal/search`). So far it lands the complete
//! `SlidingWindow` matcher (`sliding_window`), the `ActiveSearch` active-area searcher (`active`),
//! the `PageListSearch` history searcher (`pagelist`), the complete `ScreenSearch` (`screen`), and
//! the `ViewportSearch` viewport searcher (`viewport`); the search `Thread` is deferred to a later
//! slice.

#[allow(dead_code)]
pub(crate) mod active;

#[allow(dead_code)]
pub(crate) mod pagelist;

#[allow(dead_code)]
pub(crate) mod screen;

#[allow(dead_code)]
pub(crate) mod sliding_window;

#[allow(dead_code)]
pub(crate) mod viewport;
