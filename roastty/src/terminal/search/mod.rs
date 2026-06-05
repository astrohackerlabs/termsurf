//! The terminal search subsystem (port of upstream `terminal/search`). So far it lands the
//! `SlidingWindow` skeleton (`sliding_window`); the search algorithm and the `active` / `pagelist`
//! / `screen` / `viewport` / `Thread` searchers are deferred to later slices.

#[allow(dead_code)]
pub(crate) mod sliding_window;
