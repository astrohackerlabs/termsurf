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

# Experiment 589: search SlidingWindow append (+ integrity)

## Description

This experiment ports `SlidingWindow.append` (and `assertIntegrity`) from
upstream `terminal/search/sliding_window.zig`. `append` encodes a page node's
text into the window's `data` buffer and records the per-page `Meta` (node,
serial, cell map). It builds directly on `Node::search_encode` (Experiment 588).
The cross-page search itself (`next` / `highlight`) stays deferred. It extends
`terminal::search::sliding_window` and adds the small `Node` accessors `append`
needs.

## Upstream behavior

```zig
pub fn append(self: *SlidingWindow, node: *PageList.List.Node) Allocator.Error!usize {
    var meta: Meta = .{ .node = node, .serial = node.serial, .cell_map = .empty };
    errdefer meta.deinit(self.alloc);

    var encoded: std.Io.Writer.Allocating = .init(self.alloc);
    defer encoded.deinit();

    // Encode the page (plain, unwrap) with a point_map into meta.cell_map.
    const formatter: PageFormatter = ...; // emit=plain, unwrap=true, point_map=&meta.cell_map
    formatter.format(&encoded.writer) catch { return error.OutOfMemory; };
    assert(meta.cell_map.items.len == encoded.written().len);

    // Trailing newline if the last row isn't soft-wrapped.
    const row = node.data.getRow(node.data.size.rows - 1);
    if (!row.wrap) {
        encoded.writer.writeByte('\n') catch return error.OutOfMemory;
        try meta.cell_map.append(self.alloc, meta.cell_map.getLastOrNull() orelse .{ .x = 0, .y = 0 });
    }

    const written = encoded.written();
    if (written.len == 0) { self.assertIntegrity(); return 0; }

    // Reverse the encoding for a reverse search.
    switch (self.direction) {
        .forward => {},
        .reverse => { std.mem.reverse(u8, written); std.mem.reverse(point.Coordinate, meta.cell_map.items); },
    }

    try self.data.ensureUnusedCapacity(self.alloc, written.len);
    try self.meta.ensureUnusedCapacity(self.alloc, 1);
    try self.chunk_buf.ensureTotalCapacity(self.alloc, self.meta.capacity());

    self.data.appendSliceAssumeCapacity(written);
    self.meta.appendAssumeCapacity(meta);
    self.assertIntegrity();
    return written.len;
}

fn assertIntegrity(self: *const SlidingWindow) void {
    if (comptime !std.debug.runtime_safety) return;
    // data length == sum of all meta cell_map lengths
    var data_len: usize = 0;
    var it = self.meta.iterator(.forward);
    while (it.next()) |m| data_len += m.cell_map.items.len;
    assert(data_len == self.data.len());
    // data_offset in bounds
    assert(self.data.len() == 0 or self.data_offset < self.data.len());
}
```

Order matters: the trailing newline is added **before** the empty check, so an
empty page whose last row is **not** wrapped still appends a single `\n` (its
cell-map entry is the last coordinate, or `(0, 0)` if the page produced
nothing). The empty (`return 0`) path is reached only when the encoded text is
empty **and** the last row is soft-wrapped. The integrity invariant: the `data`
length equals the sum of every meta's `cell_map` length, and `data_offset` is in
bounds.

## Rust mapping (`roastty/src/terminal/search/sliding_window.rs`)

`append` takes a `NonNull<Node>` (the search doesn't own pages — the caller
keeps them valid, as upstream documents). It reads the node via `node.as_ref()`
and uses `Node::search_encode` (Experiment 588) for the encoded text + cell map.
The two `VecDeque`s grow implicitly (no explicit `ensureUnusedCapacity`); the
`chunk_buf` pre-grow is a scratch optimization deferred to the `next` slice that
allocates it. `assertIntegrity` becomes `assert_integrity` using `debug_assert`
(compiled out in release, matching upstream's `runtime_safety` guard).

```rust
impl SlidingWindow {
    /// Encode `node`'s page text into the window, recording its `Meta`. Returns the number of
    /// content bytes added (0 if the page contributes nothing). Upstream `append`.
    ///
    /// # Safety
    /// `node` must point to a live `Node`. The window dereferences it here and **stores the pointer
    /// in `meta`** for later use by the matcher (`next` / `highlight`), so the caller must keep the
    /// node valid for as long as it remains in the window — in particular, the caller must not
    /// mutate or drop the owning `PageList` in any way that reallocates or removes the node while
    /// the window may still reference it (clear the window first). The window does not own pages.
    pub(in crate::terminal) unsafe fn append(&mut self, node: NonNull<Node>) -> usize {
        let node_ref = unsafe { node.as_ref() };
        let (text, mut cell_map) = node_ref.search_encode();
        let mut bytes = text.into_bytes();

        // Trailing newline if the last row isn't soft-wrapped (added before the empty check, so an
        // unwrapped empty page still contributes one '\n').
        if !node_ref.last_row_wrapped() {
            let last = cell_map.last().copied().unwrap_or(Coordinate::new(0, 0));
            bytes.push(b'\n');
            cell_map.push(last);
        }

        if bytes.is_empty() {
            self.assert_integrity();
            return 0;
        }

        // Reverse the encoding for a reverse search.
        if self.direction == Direction::Reverse {
            bytes.reverse();
            cell_map.reverse();
        }

        let written_len = bytes.len();
        self.data.extend(bytes);
        self.meta.push_back(Meta {
            node,
            serial: node_ref.serial(),
            cell_map,
        });

        self.assert_integrity();
        written_len
    }

    /// Debug-only integrity check (upstream `assertIntegrity`): the `data` length equals the sum of
    /// every meta's `cell_map` length, and `data_offset` is in bounds.
    fn assert_integrity(&self) {
        debug_assert_eq!(
            self.meta.iter().map(|m| m.cell_map.len()).sum::<usize>(),
            self.data.len(),
        );
        debug_assert!(self.data.is_empty() || self.data_offset < self.data.len());
    }
}
```

Supporting accessors on `Node` (`page_list.rs`):

```rust
impl Node {
    /// This page's serial (upstream `node.serial`).
    pub(in crate::terminal) fn serial(&self) -> u64 {
        self.serial
    }

    /// Whether the page's last row is soft-wrapped (upstream
    /// `node.data.getRow(size.rows - 1).wrap`). Search uses this to decide the trailing newline.
    pub(in crate::terminal) fn last_row_wrapped(&self) -> bool {
        let rows = self.page.size_rows();
        if rows == 0 {
            return false;
        }
        self.page.get_row(rows as usize - 1).wrap()
    }
}
```

`PageList::first_node_ptr` is promoted from private to `pub(in crate::terminal)`
so the search subsystem (and this experiment's tests) can obtain a node pointer
to append. (The full node-iteration API the higher-level searchers need is a
later slice.)

## Scope / faithfulness notes

- **Ported**: `append` → `SlidingWindow::append`; `assertIntegrity` →
  `assert_integrity`; plus the `Node::serial` / `Node::last_row_wrapped`
  accessors `append` reads.
- **Faithful**: the `Meta` construction (node, serial, cell map); the
  trailing-newline rule (added before the empty check; the appended cell-map
  entry is the last coordinate or `(0, 0)`); the empty → `0` path; the
  reverse-direction reversal of both bytes and cell map; appending the bytes to
  `data` and the meta to `meta`; the returned content-byte count; and the
  integrity invariant (data length == summed cell-map lengths; `data_offset` in
  bounds).
- **Faithful adaptation**: the node is a `NonNull<Node>` read via `as_ref()`
  (upstream's `*Node`), so `append` is an `unsafe fn` with a `# Safety` contract
  (the pointer must be valid now and for as long as it stays in the window's
  `meta`; the caller must not invalidate it via the owning `PageList`) — the
  most explicit faithful mapping of upstream's pointer-based model; the encode
  step is `Node::search_encode` (Experiment 588) rather than an inline
  `PageFormatter`; the `VecDeque`s grow implicitly, so the explicit
  `ensureUnusedCapacity` calls vanish; the `chunk_buf` pre-grow is deferred to
  the `next` slice (it is a scratch buffer for highlight chunks, only needed
  when `next` runs); `assertIntegrity`'s `runtime_safety` guard becomes
  `debug_assert*` (compiled out in release); upstream's `Allocator.Error` paths
  vanish (Rust collections are infallible here).
- **Deferred**: `next` / `highlight` (the cross-page overlap matcher and the
  flattened-highlight return), `clearAndRetainCapacity` already landed
  (Experiment 587), and the higher-level searchers.
- No C ABI/header/ABI-inventory change (internal Rust). Extends
  `terminal::search::sliding_window`; adds two `Node` accessors and widens
  `first_node_ptr`'s visibility.

## Changes

1. `roastty/src/terminal/search/sliding_window.rs`: add `SlidingWindow::append`
   and `assert_integrity`; update the module doc comment to note `append` landed
   (leaving `next` / `highlight` deferred).
2. `roastty/src/terminal/page_list.rs`: add `Node::serial` and
   `Node::last_row_wrapped`; change `first_node_ptr` to
   `pub(in crate::terminal)`.
3. Tests (in `sliding_window.rs`, each calling `unsafe { w.append(node_ptr) }`
   with a pointer from `PageList::first_node_ptr`, the `PageList` outliving the
   window):
   - **forward append**: a single-page `PageList` with row text `"abc"` (last
     row not wrapped) appended forward returns `4`, leaves `data == b"abc\n"`,
     one meta whose `cell_map.len() == 4` and whose `serial` matches the node's,
     and `data_offset == 0`.
   - **reverse append**: the same page appended to a reverse window returns `4`
     and leaves `data == b"\ncba"` (bytes reversed) with the cell map reversed.
   - **empty page trailing newline**: a blank `PageList` (no text) appended
     forward returns `1` and leaves `data == b"\n"` with a one-entry cell map
     (`(0, 0)`), exercising the `unwrap_or((0, 0))` default and the
     newline-before-empty-check order.
   - **integrity across appends**: after an append, `data.len()` equals the
     summed cell-map lengths (the internal `assert_integrity` not panicking,
     plus an explicit length check).
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty terminal::search
cargo test -p roastty terminal::page_list
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/terminal/search roastty/src/terminal/page_list.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `append` reproduces upstream's encode-and-record (the `Meta` build, the
  trailing-newline rule before the empty check, the empty → `0` path, the
  reverse reversal, the data/meta appends, the returned byte count) and
  `assert_integrity` reproduces the invariant — faithful to
  `terminal/search/sliding_window.zig`;
- the tests pass (forward / reverse / empty-newline / integrity), and the
  existing tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the meta construction, the trailing-newline
ordering, the empty path, the reverse handling, the returned count, or the
integrity invariant diverges from upstream, an unrelated item changes, or any
public C API/ABI changes.

## Design Review

Codex reviewed the design and **approved everything else as faithful** (the
newline-before-empty-check ordering matches upstream; the reverse is byte-wise
and reverses `cell_map` in lockstep; the return length includes the injected
newline; `assert_integrity` as `debug_assert*` is the right analogue for
upstream's runtime-safety-only check; and the planned tests cover the
ordering/reverse/integrity cases), with one Required, one Optional, and a Nit:

- **Required (adopted)**: `append` accepts `NonNull<Node>` and immediately
  dereferences it, but `NonNull::dangling()` is constructible in safe Rust — a
  safe `append` would allow UB through a safe API. Made `append` an `unsafe fn`
  with a real `# Safety` contract (the most explicit faithful mapping of
  upstream's `*Node` pointer model; the alternative — taking `&Node` and storing
  `NonNull::from` — defers the same hazard to the matcher slice). The tests call
  `unsafe { w.append(..) }`.
- **Optional (deferred, with rationale)**: reserve `chunk_buf` capacity in
  `append` (`self.chunk_buf.reserve(..)`) to mirror upstream's
  `chunk_buf.ensureTotalCapacity(meta.capacity())`. Codex confirmed deferring is
  acceptable since `next` / highlight are deferred and `Vec` grows later; the
  `chunk_buf` pre-grow lands with the `next` slice that actually uses it.
- **Nit (adopted)**: the `# Safety` docs now call out pointer invalidation
  specifically — callers must not mutate/drop the owning `PageList` in a way
  that reallocates or removes the node while the window may use stored
  `Meta.node` pointers.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d589-prompt.md`
- Result: `logs/codex-review/20260604-d589-last-message.md`

## Result

**Result:** Pass

`terminal::search::sliding_window` gained `SlidingWindow::append` and
`assert_integrity`; `page_list.rs` gained `Node::serial` and
`Node::last_row_wrapped`, and `PageList::first_node_ptr` was promoted to
`pub(in crate::terminal)`. `append` reads the node via `as_ref()`, encodes its
page with `search_encode`, appends a trailing `\n` (with a cell-map entry — the
last coordinate, or `(0, 0)`) when the last row isn't soft-wrapped, returns `0`
if the result is empty, reverses the bytes and cell map for a reverse search,
pushes the bytes to `data` and the `Meta` (node, serial, cell map) to `meta`,
and returns the content-byte count. `assert_integrity` debug-checks that
`data.len()` equals the summed cell-map lengths and that `data_offset` is in
bounds.

One deviation from the plan, validated by the result review: `append` is
`pub(in crate::terminal) unsafe fn`, not the `pub(crate)` the plan wrote —
because the signature names `Node` (`pub(in crate::terminal)`), a `pub(crate)`
fn would trip the `private_interfaces` warning (the no-warnings gate). Codex
confirmed `pub(in crate::terminal)` is the right call (it matches `Node`'s
visibility boundary without exposing search internals more broadly).

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3252 passed, 0 failed (four new tests; no
  regressions, up from 3248).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + terminal/search + page_list.rs
  - lib.rs/header/abi_harness.c) clean; `git diff --check` clean.

The four new tests (each `unsafe { w.append(node) }` with a `first_node_ptr`,
the `PageList` outliving the window): a forward append (`["abc"]` → `4`,
`data == b"abc\n"`, one meta, `cell_map.len() == 4`, matching serial,
`data_offset == 0`), a reverse append (`data == b"\ncba"` with the cell map
reversed to `(2,0),(2,0),(1,0),(0,0)`), an empty/blank page (`→ 1`,
`data == b"\n"`, `cell_map == [(0, 0)]` — the newline-before-empty-check order
plus the `unwrap_or` default), and the data/meta length invariant.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no Required
findings** (one Optional — strengthen the reverse test to assert the reversed
`cell_map` contents, not just its length — adopted; one Nit — the `## Result` /
`## Conclusion` sections were not yet saved — added here). Codex confirmed the
port is faithful: `append` builds `Meta` with the node pointer, serial, and
per-byte cell map; adds the trailing newline before the empty check; preserves
the empty-to-`0` path only for empty soft-wrapped pages; reverses raw bytes and
cell map together for a reverse search; returns the byte count including the
newline; and keeps the aggregate integrity check debug-only. Codex also
confirmed the `pub(in crate::terminal)` visibility deviation is the right call.

Review artifacts:

- Prompt: `logs/codex-review/20260604-r589-prompt.md` (result)
- Result: `logs/codex-review/20260604-r589-last-message.md` (result)

## Conclusion

This experiment ports `SlidingWindow::append` — the step that feeds the search
window. A page node's text is encoded (via `Node::search_encode`) into the
window's `data` with a per-byte `cell_map`, recorded in a `Meta` (node pointer,
serial, cell map); a trailing newline is appended for non-soft-wrapped last rows
(before the empty check, so an unwrapped blank page still contributes one `\n`),
and both bytes and cell map are reversed for a reverse search. `append` is an
`unsafe fn` (it dereferences and stores a `NonNull<Node>` the window does not
own), and `assert_integrity` debug-checks the data/cell-map invariant. The next
slice is the matcher: `SlidingWindow::next` (scan the window's data — across the
two `VecDeque` slices and the `overlap_buf` for cross-page matches — for the
needle, prune consumed pages while keeping `needle.len() - 1` bytes for future
overlaps) and `highlight` (turn a match's byte range into a `FlattenedHighlight`
via the `chunk_buf` and the cell map). After the matcher come the higher-level
searchers (`active` / `pagelist` / `screen` / `viewport`) and the search
`Thread`.
