+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"
+++

# Experiment 595: search ScreenSearch result accessors

## Description

This experiment continues `ScreenSearch` (upstream
`terminal/search/screen.zig`), building on the skeleton (Experiment 594) with
the **read-only result accessors**: `needle`, `matchesLen`, and `matches`. These
are self-contained (they only read the cached result lists and the active
needle), so they can be ported and tested before the construction (`init` /
`reloadActive`) and the search/feed logic. It extends `terminal::search::screen`
and adds a needle accessor on `SlidingWindow` / `ActiveSearch`.

## Upstream behavior

```zig
/// The needle that this search is using.
pub fn needle(self: *const ScreenSearch) []const u8 {
    assert(self.active.window.direction == .forward);
    return self.active.window.needle;
}

/// Returns the total number of matches found so far.
pub fn matchesLen(self: *const ScreenSearch) usize {
    return self.active_results.items.len + self.history_results.items.len;
}

/// Returns all matches as an owned slice (caller must free). The matches are
/// ordered from most recent to oldest (e.g. bottom of the screen to top).
pub fn matches(self: *ScreenSearch, alloc) ![]FlattenedHighlight {
    const active_results = self.active_results.items;
    const history_results = self.history_results.items;
    const results = try alloc.alloc(FlattenedHighlight, active_results.len + history_results.len);

    // Active is a forward search, so copy then reverse it.
    assert(self.active.window.direction == .forward);
    @memcpy(results[0..active_results.len], active_results);
    std.mem.reverse(FlattenedHighlight, results[0..active_results.len]);

    // History is a backward search, so append after.
    @memcpy(results[active_results.len..], history_results);
    return results;
}
```

- `needle` returns the active window's needle (the active window is always a
  forward search, so the stored bytes are the original needle, not reversed).
- `matchesLen` is the sum of the two result-list lengths.
- `matches` returns all results ordered **newest to oldest**: the active results
  (stored forward, oldest-to-newest) are reversed, then the history results
  (already stored newest-to-oldest by the reverse history search) are appended.

## Rust mapping (`roastty/src/terminal/search/screen.rs`)

`needle` returns `&[u8]` (a borrow, not an alloc); `matches_len` is the length
sum; `matches` returns an owned `Vec<Flattened>` (Rust ownership replaces
upstream's caller-frees slice) built by reversing the active results and
appending the history results. The active window's "always forward" invariant
holds by construction (`ActiveSearch::new` always builds a forward window), so
the needle bytes are the original; this replaces upstream's
`assert(direction == forward)`.

```rust
impl ScreenSearch {
    /// The needle this search is using (upstream `needle`). The active window is always forward, so
    /// its stored bytes are the original needle.
    pub(in crate::terminal) fn needle(&self) -> &[u8] {
        self.active.needle()
    }

    /// The total number of matches found so far (upstream `matchesLen`).
    pub(in crate::terminal) fn matches_len(&self) -> usize {
        self.active_results.len() + self.history_results.len()
    }

    /// All matches, ordered newest-to-oldest (upstream `matches`): the active results (stored
    /// forward) reversed, then the history results (already newest-to-oldest) appended. Returns an
    /// owned `Vec` (Rust ownership replaces upstream's caller-frees slice).
    pub(in crate::terminal) fn matches(&self) -> Vec<Flattened> {
        let mut results = Vec::with_capacity(self.active_results.len() + self.history_results.len());
        results.extend(self.active_results.iter().rev().cloned());
        results.extend(self.history_results.iter().cloned());
        results
    }
}
```

Supporting accessors:

```rust
// sliding_window.rs
impl SlidingWindow {
    /// The stored needle bytes (upstream `window.needle`). For a reverse window these are reversed;
    /// callers that need the original (e.g. the screen search) use a forward window.
    pub(in crate::terminal) fn needle(&self) -> &[u8] {
        &self.needle
    }
}

// active.rs
impl ActiveSearch {
    /// The (forward) needle this searcher is using.
    pub(in crate::terminal) fn needle(&self) -> &[u8] {
        self.window.needle()
    }
}
```

## Scope / faithfulness notes

- **Ported**: `needle` / `matchesLen` / `matches` → `ScreenSearch::needle` /
  `matches_len` / `matches`; plus the `SlidingWindow::needle` /
  `ActiveSearch::needle` accessors.
- **Faithful**: `needle` returns the active needle; `matches_len` is the sum of
  the two list lengths; `matches` orders newest-to-oldest (reversed active ++
  history).
- **Faithful adaptation**: `needle` returns a `&[u8]` borrow instead of
  upstream's slice; `matches` returns an owned `Vec<Flattened>` (Rust ownership)
  instead of a caller-frees allocation, cloning each `Flattened` (roastty's
  `Flattened` owns its `Vec<Chunk>`, so this is a deep copy — upstream's
  `memcpy` is a shallow copy of the chunk-buffer-aliasing struct, but the
  resulting order and contents are the same); the active-window
  `assert(direction == forward)` is replaced by the by-construction invariant
  (`ActiveSearch::new` always builds a forward window).
- **Deferred**: `init` / `reloadActive`, the state transitions (`tick` /
  `tickActive` / `tickHistory`), `feed` / `pruneHistory`, `searchAll`, and
  `select` / `selectNext` / `selectPrev`; plus `ViewportSearch` and the search
  `Thread`.
- No C ABI/header/ABI-inventory change (internal Rust). Extends
  `terminal::search::screen`; adds two needle accessors.

## Changes

1. `roastty/src/terminal/search/screen.rs`: add `ScreenSearch::needle` /
   `matches_len` / `matches`; update the module doc comment.
2. `roastty/src/terminal/search/sliding_window.rs`: add `SlidingWindow::needle`.
3. `roastty/src/terminal/search/active.rs`: add `ActiveSearch::needle`.
4. Tests (in `screen.rs`) — these manually build a `ScreenSearch` with populated
   result lists (the accessors do not dereference `screen`, so it can be
   `NonNull::dangling()`, and no tracked pins exist so dropping it is safe):
   - **`needle`**: a search built with `ActiveSearch::new(b"foo")` returns
     `b"foo"`.
   - **`matches_len`**: with 2 active + 2 history results, returns `4`; with
     empty lists, `0`.
   - **`matches` ordering**: active `[A, B]` (forward, `top_x` 1, 2) and history
     `[H1, H2]` (newest-to-oldest, `top_x` 10, 11) → `matches()` yields `top_x`
     order `[2, 1, 10, 11]` (reversed active, then history); empty lists →
     empty.
5. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty terminal::search
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/terminal/search && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `needle` / `matches_len` / `matches` reproduce upstream (the active needle,
  the length sum, and the newest-to-oldest reversed-active-then-history
  ordering) — faithful to `terminal/search/screen.zig`;
- the tests pass (`needle` / `matches_len` / `matches` ordering), and the
  existing tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the needle, the length sum, or the match ordering
diverges from upstream, an unrelated item changes, or any public C API/ABI
changes.

## Design Review

Codex reviewed the design and **approved it with no Required, Optional, or Nit
findings**. It confirmed the accessor slice is clean and faithful: `matches_len`
is direct; `matches()` preserves upstream's `reverse(active) ++ history`
ordering, and returning an owned `Vec<Flattened>` with deep-cloned chunks is the
right Rust ownership adaptation (the same observable match contents without
sharing internal result storage); using `ActiveSearch::needle()` without an
explicit direction assert is acceptable because `ActiveSearch::new` constructs
only a forward `SlidingWindow` (adding a `direction()` accessor solely for the
assert would expose more than this slice needs); and the dangling-screen /
manual-construction tests are sound because these accessors never dereference
`screen` and the test data avoids tracked-pin ownership.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d595-prompt.md`
- Result: `logs/codex-review/20260604-d595-last-message.md`
