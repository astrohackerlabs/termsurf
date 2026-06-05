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

# Experiment 587: search SlidingWindow skeleton (vocabulary + lifecycle)

## Description

This experiment opens the terminal **search** subsystem — upstream
`terminal/search/` (`sliding_window.zig` 1676 lines, plus `active` / `pagelist`
/ `screen` / `viewport` / `Thread`). The core is the `SlidingWindow`: it appends
page nodes, encodes their text into a growing byte buffer, and scans for a
needle across page boundaries, returning flattened highlights.

This first slice ports the **vocabulary and lifecycle** of `SlidingWindow` — the
pieces with no search algorithm yet: the search `Direction`, the per-page `Meta`
record, the `SlidingWindow` struct, its constructor (`init`), and its
`clearAndRetainCapacity`. The search algorithm itself (`next`, `append`,
`highlight`, the overlap/prune logic, integrity assertions, and buffer growth)
stays deferred to later slices. It creates a new `terminal::search` module with
`search/sliding_window.rs`.

Note: an earlier discovery during design — roastty already ports the
**formatter** (`PageFormatter`-equivalent page-string encoding) inline in
`page_list.rs` (`PageOutputFormat`, `CodepointMapEntry`, `CodepointReplacement`,
`PlainPageFormat` / `StyledPageFormat`, `PageStringWithPinMap`), so the search
subsystem's text-encoding dependency is already satisfied there; this slice does
not re-port it.

## Upstream behavior (`terminal/search/sliding_window.zig`)

```zig
pub const SlidingWindow = struct {
    alloc: Allocator,
    data: DataBuf,                 // CircBuf(u8, 0) — encoded page text
    meta: MetaBuf,                 // CircBuf(Meta, undefined) — per-page metadata
    chunk_buf: std.MultiArrayList(FlattenedHighlight.Chunk),
    data_offset: usize = 0,
    needle: []const u8,            // owned
    direction: Direction,
    overlap_buf: []u8,             // owned, needle.len * 2

    const Direction = enum { forward, reverse };
    const DataBuf = CircBuf(u8, 0);
    const MetaBuf = CircBuf(Meta, undefined);
    const Meta = struct {
        node: *PageList.List.Node,
        serial: u64,
        cell_map: std.ArrayList(point.Coordinate),
        pub fn deinit(self: *Meta, alloc) void { self.cell_map.deinit(alloc); }
    };

    pub fn init(alloc, direction, needle_unowned) !SlidingWindow {
        var data = try DataBuf.init(alloc, 0);
        var meta = try MetaBuf.init(alloc, 0);
        const needle = try alloc.dupe(u8, needle_unowned);
        switch (direction) { .forward => {}, .reverse => std.mem.reverse(u8, needle) }
        const overlap_buf = try alloc.alloc(u8, needle.len * 2);
        return .{ .alloc = alloc, .data = data, .meta = meta, .chunk_buf = .empty,
                  .needle = needle, .direction = direction, .overlap_buf = overlap_buf };
    }

    pub fn deinit(self) void {
        self.alloc.free(self.overlap_buf);
        self.alloc.free(self.needle);
        self.chunk_buf.deinit(self.alloc);
        self.data.deinit(self.alloc);
        var it = self.meta.iterator(.forward);
        while (it.next()) |meta| meta.deinit(self.alloc);
        self.meta.deinit(self.alloc);
    }

    pub fn clearAndRetainCapacity(self) void {
        var it = self.meta.iterator(.forward);
        while (it.next()) |meta| meta.deinit(self.alloc);
        self.meta.clear();
        self.data.clear();
        self.data_offset = 0;
    }
};
```

- `Direction` is `forward` / `reverse`; on a reverse search the needle is stored
  reversed (and pages are appended in reverse order — that ordering is the
  caller's job).
- `Meta` is the per-appended-page record: the page `node`, the page `serial` (to
  detect invalidation), and a `cell_map` mapping each encoded data byte back to
  a cell `Coordinate`.
- `init` dups the needle (reversing it for reverse search) and allocates an
  `overlap_buf` of `needle.len * 2` bytes (scratch for cross-page-boundary
  matches). The data and meta buffers start empty.
- `deinit` frees the owned buffers and each meta's `cell_map`.
- `clearAndRetainCapacity` deinits each meta, clears both buffers (retaining
  capacity), and resets `data_offset`.

## Rust mapping (`roastty/src/terminal/search/sliding_window.rs`, new module)

roastty's `CircBuf<T>` is **fixed-capacity and `T: Copy`** — it cannot serve
either upstream search buffer: `data` grows as pages are appended, and `meta`
holds a non-`Copy` `Meta` (it owns a `Vec` `cell_map`). The faithful adaptation
is `std::collections::VecDeque` for both — a growable ring buffer that holds
non-`Copy` elements, retains capacity on `clear`, iterates front-to-back, and
(critically, for the later `next` slice) exposes its contents as the **two
slices** `as_slices()` that upstream's `getPtrSlice` / overlap logic needs.
Rust's `Drop` subsumes upstream `deinit` (every field is an owned `Vec` /
`VecDeque` that drops its contents, including each `Meta`'s `cell_map`), so
there is no explicit `deinit` and the `init` allocations are infallible (no
`Result`).

```rust
//! The search sliding window (port of upstream `terminal/search/sliding_window.zig`). This first
//! slice lands the vocabulary and lifecycle: the search `Direction`, the per-page `Meta` record,
//! the `SlidingWindow` struct, its constructor, and `clear_and_retain_capacity`. The search
//! algorithm itself (`next` / `append` / `highlight`, the overlap/prune logic, the integrity
//! assertions, and buffer growth) is deferred to later slices.

use std::collections::VecDeque;
use std::ptr::NonNull;

use super::super::highlight::Chunk;
use super::super::page_list::Node;
use super::super::point::Coordinate;

/// The search direction (upstream `SlidingWindow.Direction`). For a reverse search the needle is
/// stored reversed and pages are appended in reverse order (the caller's responsibility).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Forward,
    Reverse,
}

/// Per-appended-page metadata (upstream `SlidingWindow.Meta`). `cell_map` maps each encoded data
/// byte back to a cell coordinate; `serial` detects page invalidation. Owns its `cell_map` (Rust's
/// `Drop` subsumes upstream `Meta.deinit`).
///
/// `Meta` and its `node` field are `pub(in crate::terminal)` — no more visible than `Node`
/// (`pub(super)` in `page_list`) — so exposing `node: NonNull<Node>` does not leak a more-private
/// type (which would trip the `private_interfaces` warning and the no-warnings gate).
#[derive(Debug)]
pub(in crate::terminal) struct Meta {
    pub(in crate::terminal) node: NonNull<Node>,
    pub(in crate::terminal) serial: u64,
    pub(in crate::terminal) cell_map: Vec<Coordinate>,
}

/// Searches page nodes via a sliding window over their encoded text (upstream `SlidingWindow`).
pub(crate) struct SlidingWindow {
    /// Encoded page text (upstream `data: CircBuf(u8, 0)`).
    data: VecDeque<u8>,
    /// Per-page metadata (upstream `meta: CircBuf(Meta, undefined)`).
    meta: VecDeque<Meta>,
    /// Scratch chunk buffer for flattened highlights (upstream `chunk_buf`).
    chunk_buf: Vec<Chunk>,
    /// Offset into `data` for the current search state (upstream `data_offset`).
    data_offset: usize,
    /// The needle, owned; stored reversed for a reverse search (upstream `needle`).
    needle: Vec<u8>,
    /// The search direction (upstream `direction`).
    direction: Direction,
    /// Cross-page-boundary scratch buffer, `needle.len() * 2` bytes (upstream `overlap_buf`).
    overlap_buf: Vec<u8>,
}

impl SlidingWindow {
    /// Create an empty sliding window for `needle` in `direction` (upstream `init`). The needle is
    /// copied (and reversed for a reverse search); the overlap buffer is `needle.len() * 2` bytes.
    pub(crate) fn new(direction: Direction, needle: &[u8]) -> SlidingWindow {
        let mut needle = needle.to_vec();
        if direction == Direction::Reverse {
            needle.reverse();
        }
        let overlap_buf = vec![0u8; needle.len() * 2];
        SlidingWindow {
            data: VecDeque::new(),
            meta: VecDeque::new(),
            chunk_buf: Vec::new(),
            data_offset: 0,
            needle,
            direction,
            overlap_buf,
        }
    }

    /// Clear all data but retain allocated capacity (upstream `clearAndRetainCapacity`). Clearing
    /// `meta` drops each `Meta` (and its `cell_map`), subsuming upstream's per-meta `deinit`.
    pub(crate) fn clear_and_retain_capacity(&mut self) {
        self.meta.clear();
        self.data.clear();
        self.data_offset = 0;
    }
}
```

The module tree: `roastty/src/terminal/search/mod.rs` (the `search.zig`
equivalent — for now just `#[allow(dead_code)] pub(crate) mod sliding_window;`;
the `Active` / `PageList` / `Screen` / `Viewport` re-exports are deferred) and
`roastty/src/terminal/search/sliding_window.rs`. Declared in
`roastty/src/terminal/mod.rs` as `#[allow(dead_code)] pub(crate) mod search;`.

## Scope / faithfulness notes

- **Ported**: `SlidingWindow.Direction` → `Direction`; `SlidingWindow.Meta` →
  `Meta`; the `SlidingWindow` struct; `init` → `SlidingWindow::new`;
  `clearAndRetainCapacity` → `clear_and_retain_capacity`.
- **Faithful**: the two directions; `Meta`'s `node` / `serial` / `cell_map`
  fields; the struct's `data` / `meta` / `chunk_buf` / `data_offset` / `needle`
  / `direction` / `overlap_buf` fields; `init`'s needle copy,
  reverse-on-reverse, and `needle.len() * 2` overlap buffer with both data/meta
  starting empty; `clearAndRetainCapacity`'s clear-both-buffers + reset
  `data_offset` with capacity retained and each meta released.
- **Faithful adaptation**: both upstream `CircBuf`s become `VecDeque` (roastty's
  `CircBuf<T: Copy>` is fixed-capacity and `Copy`-only — it cannot hold the
  growing data nor the non-`Copy` `Meta`; `VecDeque` grows, holds non-`Copy`
  elements, retains capacity on `clear`, and exposes the two-slice `as_slices()`
  the later overlap logic needs); `chunk_buf`'s `MultiArrayList` becomes
  `Vec<Chunk>` (roastty's `highlight::Chunk` is `Copy`); the owned `needle` /
  `overlap_buf` slices become `Vec<u8>`; `Meta.deinit` and
  `SlidingWindow.deinit` are subsumed by Rust `Drop` (no explicit `deinit`);
  `init`'s explicit allocation-error path (`Allocator.Error`) vanishes — `new`
  returns the value directly and, like any Rust collection, aborts on allocation
  failure rather than returning an error.
- **Deferred**: the search algorithm (`next`, `append`, `highlight`, the
  overlap/prune logic, `assertIntegrity`, buffer growth) and the rest of the
  search subsystem (`active` / `pagelist` / `screen` / `viewport` / `Thread`).
- No C ABI/header/ABI-inventory change (internal Rust). Creates the
  `terminal::search` module.

## Changes

1. `roastty/src/terminal/search/sliding_window.rs` (new): the module doc
   comment, `Direction`, `Meta`, the `SlidingWindow` struct,
   `SlidingWindow::new`, and `clear_and_retain_capacity`.
2. `roastty/src/terminal/search/mod.rs` (new):
   `#[allow(dead_code)] pub(crate) mod sliding_window;`.
3. `roastty/src/terminal/mod.rs`: declare
   `#[allow(dead_code)] pub(crate) mod search;`.
4. Tests (in `sliding_window.rs`):
   - **new forward**: `new(Forward, b"abc")` keeps the needle `b"abc"`,
     direction `Forward`, empty data/meta, `overlap_buf` length `6`,
     `data_offset` `0`.
   - **new reverse**: `new(Reverse, b"abc")` stores the needle reversed
     (`b"cba"`) and `overlap_buf` length `6`.
   - **new empty needle**: `new(Forward, b"")` has an empty needle and
     `overlap_buf` length `0`.
   - **clear_and_retain_capacity**: after pushing a `Meta` (with a
     `NonNull::dangling()` node — never dereferenced — and a non-empty
     `cell_map`) and some `data` bytes and setting `data_offset`,
     `clear_and_retain_capacity` empties `data` and `meta` and resets
     `data_offset` to `0`, while both buffers retain capacity (`> 0`).
   - **clear leaves chunk_buf alone**: after pushing a `Chunk` into the scratch
     `chunk_buf` (same-module test access), `clear_and_retain_capacity` leaves
     it untouched (upstream clears only `meta` / `data` / `data_offset`).
   - **Direction equality / Copy**.
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

- `Direction` / `Meta` / `SlidingWindow` / `new` / `clear_and_retain_capacity`
  reproduce upstream's vocabulary and lifecycle (the two directions; the meta
  fields; the struct fields; the needle copy + reverse-on-reverse + `len * 2`
  overlap buffer; the clear-and-retain semantics) — faithful to
  `terminal/search/sliding_window.zig`;
- the tests pass (new forward / reverse / empty / clear / Direction), and the
  existing tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the direction handling, the needle copy/reverse, the
overlap-buffer sizing, the struct fields, or the clear-and-retain semantics
diverge from upstream, an unrelated item changes, or any public C API/ABI
changes.

## Design Review

This experiment was first proposed as a formatter-vocabulary port; Codex's
review **correctly flagged (Required)** that roastty already ports the formatter
inline in `page_list.rs` (`PageOutputFormat`, `CodepointMapEntry`,
`CodepointReplacement`, `PlainPageFormat` / `StyledPageFormat`,
`PageStringWithPinMap`). That direction was abandoned and the experiment
re-scoped to the genuinely-unported **search** subsystem (this `SlidingWindow`
skeleton), whose text-encoding dependency is already satisfied by
`page_list.rs`.

Codex then reviewed the revised design and **approved the container decision**
(`VecDeque<u8>` / `VecDeque<Meta>` for the two upstream `CircBuf`s — covering
growability, non-`Copy` metadata, retained capacity on `clear`, front pruning,
reverse/front iteration, and the two-slice `as_slices()` the later overlap logic
needs; `Vec<Chunk>` is a reasonable scratch replacement for the
`MultiArrayList`; Rust `Drop` correctly subsumes `deinit`; the needle reversal
and `len * 2` overlap allocation are faithful; the dangling-node test is sound
as long as the pointer is never dereferenced), with one Required and two
Optionals and a Nit — all adopted:

- **Required (adopted)**: `Meta` exposing `pub(crate) node: NonNull<Node>` would
  leak the less-visible `Node` (`pub(super)` in `page_list`) and trip the
  `private_interfaces` warning, conflicting with the no-warnings gate. `Meta`
  and its fields are now `pub(in crate::terminal)` — no more visible than
  `Node`. (`SlidingWindow` stays `pub(crate)`; its `meta` field is private, so
  `Meta`'s visibility does not leak through it.)
- **Optional (adopted)**: state explicitly that upstream's `CircBuf` is growable
  while roastty's current `CircBuf` port is fixed-capacity and `Copy`-bound — so
  `VecDeque` is not just convenient but the better structural match for the
  future search-window operations. (Reflected in the Rust-mapping and
  faithfulness notes.)
- **Optional (adopted)**: add a test that `clear_and_retain_capacity` leaves
  `chunk_buf` untouched (upstream clears only `meta` / `data` / `data_offset`).
- **Nit (adopted)**: note that `new` aborts on allocation failure like any Rust
  collection, rather than returning upstream's `Allocator.Error`.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d587-prompt.md`
- Result: `logs/codex-review/20260604-d587-last-message.md`

## Result

**Result:** Pass

A new `terminal::search` module landed (`search/mod.rs` declaring
`sliding_window`; `search/sliding_window.rs`), declared
`#[allow(dead_code)] pub(crate) mod search;` in `terminal/mod.rs`. It ports the
`SlidingWindow` vocabulary and lifecycle: `Direction` (`Forward` / `Reverse`);
`Meta` (`node` / `serial` / `cell_map`, `pub(in crate::terminal)` so it does not
leak the `pub(super)` `Node`); the `SlidingWindow` struct (`data` / `meta` as
`VecDeque`, `chunk_buf` as `Vec<Chunk>`, `data_offset` / `needle` / `direction`
/ `overlap_buf`); `new` (copies the needle, reverses it for a reverse search,
sizes `overlap_buf` to `needle.len() * 2`, starts both buffers empty); and
`clear_and_retain_capacity` (clears `meta` and `data`, resetting `data_offset`,
retaining capacity; clearing `meta` drops each `Meta`'s `cell_map`, subsuming
upstream's per-meta `deinit`). The search algorithm is deferred.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3244 passed, 0 failed (six new tests; no regressions,
  up from 3238).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + terminal/search +
  lib.rs/header/abi_harness.c) clean; `git diff --check` clean.

The six new tests: `new` forward (needle `b"abc"`, `Forward`, empty data/meta,
`overlap_buf` len `6`, `data_offset` `0`), `new` reverse (needle `b"cba"`,
`overlap_buf` len `6`), `new` empty needle (empty needle, `overlap_buf` len
`0`), `clear_and_retain_capacity` (push data + a `Meta` with a
`NonNull::dangling()` node and a non-empty `cell_map` + `data_offset` = `1`,
then clear empties both buffers and resets `data_offset`, capacities retained),
`clear` leaves `chunk_buf` untouched, and `Direction` `Copy` / equality.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no Required
or Optional findings** (one Nit: the `## Result` / `## Conclusion` sections were
not yet in the saved file — added here). Codex confirmed the lifecycle slice is
faithful: `Direction` maps correctly, `new` copies and reverses the needle only
for a reverse search and sizes `overlap_buf` to `needle.len() * 2`, owned Rust
fields cover upstream `deinit` via `Drop`, and `clear_and_retain_capacity`
clears `meta` / `data` (dropping each `Meta.cell_map`), resets `data_offset`,
retains buffer capacity, and leaves `chunk_buf` untouched; the `VecDeque`
mapping remains sound for the future sliding-window operations, the `Meta`
visibility fix avoids leaking the terminal-private `Node`, and the
dangling-pointer tests are safe because nothing dereferences the node.

Review artifacts:

- Prompt: `logs/codex-review/20260604-r587-prompt.md` (result)
- Result: `logs/codex-review/20260604-r587-last-message.md` (result)

## Conclusion

This experiment opens the terminal **search** subsystem — the last major
unported terminal subsystem — with the `SlidingWindow` skeleton (the search
core's vocabulary and lifecycle). The key adaptation: both upstream search
`CircBuf`s become `VecDeque`, because roastty's `CircBuf` port is fixed-capacity
and `Copy`-bound and cannot hold the growing data buffer nor the non-`Copy`
`Meta`; `VecDeque` grows, holds non-`Copy` elements, retains capacity on
`clear`, and exposes the two-slice `as_slices()` the later overlap matcher
needs. A design-phase discovery: roastty already ports the **formatter** inline
in `page_list.rs`, so the search subsystem's text-encoding dependency is already
satisfied. The next slices build out the `SlidingWindow` search algorithm
(`append` to encode a page node's text into `data` with a `cell_map`, then
`next` / `highlight` with the cross-page overlap and prune logic and buffer
growth), followed by the higher-level searchers (`active` / `pagelist` /
`screen` / `viewport`) and the search `Thread`.
