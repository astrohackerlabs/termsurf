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

# Experiment 590: search SlidingWindow highlight (flattened match builder)

## Description

This experiment ports `SlidingWindow.highlight` from upstream
`terminal/search/sliding_window.zig` — the function that turns a match
(`start_offset`, `len`) within the window's `data` buffer into a
`FlattenedHighlight`: a list of per-page `Chunk`s (node, serial, row range) plus
the match's `top_x` / `bot_x`. It also prunes the consumed metas/data and
advances `data_offset` past the match. `highlight` is the half of the matcher
that builds results; `next` (the scan that finds matches and calls `highlight`)
is the next slice. It extends `terminal::search::sliding_window` and adds a
`Node::page_rows` accessor.

## Upstream behavior

`highlight(start_offset, len)` (see `sliding_window.zig`):

1. `start = start_offset + data_offset`; `end = start + len - 1`. Asserts both
   in bounds.
2. Clears `chunk_buf`; `result = .empty`.
3. **Top-left search** — iterate metas, tracking `meta_consumed`. For each meta,
   `meta_i = start - prior_meta_consumed`; if `meta_i >= cell_map.len`, the
   match isn't here (and everything up to here is prunable) → continue.
   Otherwise:
   - if `end_i = end - prior_meta_consumed < cell_map.len`, the whole match is
     in this meta: set `top_x`/`bot_x` from
     `cell_map[meta_i]`/`cell_map[end_i]`, push one chunk
     `{node, serial, start = start_map.y, end = end_map.y + 1}`, record
     `prune = {meta: idx - 1, data: prior_meta_consumed}`, done (no `br`).
   - else the match only starts here: set `top_x`, push a chunk
     `{node, serial, start = map.y, end = page.size.rows}` (to the page bottom),
     record `br = {it, consumed: meta_consumed}` and the same `prune`, done.
   - Falling off the loop is `unreachable` (the start must be in bounds).
4. **Bottom-right search** (only if `br`): continue from `br.it`. For each meta,
   `meta_i = end - meta_consumed`; if `meta_i >= cell_map.len`, this whole page
   is inside the match → push a full-page chunk
   `{start: 0, end: page.size.rows}` and continue; otherwise push
   `{start: 0, end: map.y + 1}`, set `bot_x = map.x`, stop. Falling off is
   `unreachable`.
5. `data_offset = start - prune.data + 1` (one past the match within the
   surviving data).
6. If `prune.meta > 0`, deinit + `deleteOldest(prune.meta)` metas and
   `deleteOldest(prune.data)` data bytes (everything before the start meta).
7. **Reverse fixup** (reverse direction only): the chunks were built in forward
   data order, but for a reverse search the pages are in reverse screen order,
   so the row ranges of the first/last chunk need inverting:
   - `> 1` chunk: reverse the chunk list; then
     `chunks[0].start = chunks[0].end - 1`, `chunks[0].end = page.size.rows`;
     `chunks[last].end = chunks[last].start + 1`, `chunks[last].start = 0`.
   - `1` chunk: `start' = chunks[0].end - 1`, `end' = chunks[0].start + 1`.
   - swap `top_x` / `bot_x`.
8. Move `chunk_buf` into `result.chunks`; return.

## Rust mapping (`roastty/src/terminal/search/sliding_window.rs`)

`MetaBuf.Iterator` + `.idx` becomes index iteration over the `meta` `VecDeque`
(`meta_it.idx - 1` for the current meta = the loop index `i`). `deleteOldest`
becomes `VecDeque::drain(..n)` (which drops the pruned metas, subsuming
`meta.deinit`). The `MultiArrayList` `chunk_buf` is a `Vec<Chunk>`; the reverse
fixup reverses the `Vec` (moving each chunk's fields together, equivalent to
reversing the three parallel arrays) and patches `chunks[0]` / `chunks[last]`.
`result.chunks = self.chunk_buf.clone()` (roastty's `Flattened` owns its chunks;
cloning keeps `chunk_buf`'s capacity for reuse). The stored `Meta.node` pointers
are dereferenced for `page_rows` under the window's invariant (append's
`# Safety` contract: stored nodes stay valid). `y` (`u32`) → `CellCountInt`
(`u16`) casts go through `try_into().expect(...)` (page rows always fit),
matching upstream's `@intCast`.

```rust
/// Build a flattened highlight for a match at `start_offset` (relative to `data_offset`) of length
/// `len` (upstream `highlight`). Sets `top_x` / `bot_x`, emits one `Chunk` per spanned page, prunes
/// consumed metas/data, and advances `data_offset` one past the match.
///
/// Dereferences stored `Meta.node` pointers for `page_rows`; sound under the window invariant that
/// every node remains valid while in the window (see `append`'s `# Safety`).
fn highlight(&mut self, start_offset: usize, len: usize) -> Flattened {
    let start = start_offset + self.data_offset;
    let end = start + len - 1;
    debug_assert!(start < self.data.len());
    debug_assert!(start + len <= self.data.len());

    self.chunk_buf.clear();
    let mut result = Flattened::empty();

    // Top-left (start) search. `prune` = (meta count, data length) before the start meta; `br` =
    // Some((next meta index, consumed)) when the end is in a later meta.
    let mut br: Option<(usize, usize)> = None;
    let mut prune_meta = 0usize;
    let mut prune_data = 0usize;
    let mut meta_consumed = 0usize;
    let mut found = false;
    for i in 0..self.meta.len() {
        let meta = &self.meta[i];
        let prior = meta_consumed;
        meta_consumed += meta.cell_map.len();
        let meta_i = start - prior;
        if meta_i >= meta.cell_map.len() {
            continue;
        }
        let end_i = end - prior;
        if end_i < meta.cell_map.len() {
            let start_map = meta.cell_map[meta_i];
            let end_map = meta.cell_map[end_i];
            result.top_x = start_map.x;
            result.bot_x = end_map.x;
            self.chunk_buf.push(Chunk {
                node: meta.node,
                serial: meta.serial,
                start: cell_row(start_map.y),
                end: cell_row(end_map.y + 1),
            });
        } else {
            let map = meta.cell_map[meta_i];
            result.top_x = map.x;
            // SAFETY: stored nodes stay valid while in the window (append's contract).
            let rows = unsafe { meta.node.as_ref() }.page_rows();
            self.chunk_buf.push(Chunk {
                node: meta.node,
                serial: meta.serial,
                start: cell_row(map.y),
                end: rows,
            });
            br = Some((i + 1, meta_consumed));
        }
        prune_meta = i;
        prune_data = prior;
        found = true;
        break;
    }
    assert!(found, "highlight start index must be within the data buffer");

    // Bottom-right (end) search.
    if let Some((mut idx, mut consumed)) = br {
        let mut end_found = false;
        while idx < self.meta.len() {
            let meta = &self.meta[idx];
            let meta_i = end - consumed;
            if meta_i >= meta.cell_map.len() {
                // SAFETY: see above.
                let rows = unsafe { meta.node.as_ref() }.page_rows();
                self.chunk_buf.push(Chunk {
                    node: meta.node,
                    serial: meta.serial,
                    start: 0,
                    end: rows,
                });
                consumed += meta.cell_map.len();
                idx += 1;
                continue;
            }
            let map = meta.cell_map[meta_i];
            result.bot_x = map.x;
            self.chunk_buf.push(Chunk {
                node: meta.node,
                serial: meta.serial,
                start: 0,
                end: cell_row(map.y + 1),
            });
            end_found = true;
            break;
        }
        assert!(end_found, "highlight end index must be within the data buffer");
    }

    // Advance one past the match, then prune everything before the start meta.
    self.data_offset = start - prune_data + 1;
    if prune_meta > 0 {
        self.meta.drain(..prune_meta);
        debug_assert!(prune_data > 0);
        self.data.drain(..prune_data);
        // The surviving front meta is the start meta — its node is the first chunk's node (upstream's
        // post-prune cross-check, before the reverse fixup reorders `chunk_buf`).
        debug_assert_eq!(
            self.meta.front().map(|m| m.node),
            self.chunk_buf.first().map(|c| c.node),
        );
    }

    // Reverse fixup: the chunks were built in forward data order. NOTE: reversing the `Vec<Chunk>`
    // reverses `serial` along with `node` / `start` / `end` — deliberately, so each chunk's `serial`
    // stays paired with its `node`. Upstream reverses only the node/start/end arrays (leaving the
    // serial array in place); this is a correctness-preserving deviation, not exact equivalence.
    if self.direction == Direction::Reverse {
        let n = self.chunk_buf.len();
        if n > 1 {
            self.chunk_buf.reverse();
            // SAFETY: see above.
            let first_rows = unsafe { self.chunk_buf[0].node.as_ref() }.page_rows();
            self.chunk_buf[0].start = self.chunk_buf[0].end - 1;
            self.chunk_buf[0].end = first_rows;
            self.chunk_buf[n - 1].end = self.chunk_buf[n - 1].start + 1;
            self.chunk_buf[n - 1].start = 0;
        } else {
            let start_y = self.chunk_buf[0].start;
            self.chunk_buf[0].start = self.chunk_buf[0].end - 1;
            self.chunk_buf[0].end = start_y + 1;
        }
        std::mem::swap(&mut result.top_x, &mut result.bot_x);
    }

    result.chunks = self.chunk_buf.clone();
    result
}
```

with a small file-local helper
`fn cell_row(y: u32) -> CellCountInt { y.try_into().expect("page row fits CellCountInt") }`.

Supporting accessor on `Node` (`page_list.rs`):

```rust
impl Node {
    /// The page's row count (upstream `node.data.size.rows`). Search uses it for full-page chunk
    /// bounds.
    pub(in crate::terminal) fn page_rows(&self) -> CellCountInt {
        self.page.size_rows()
    }
}
```

## Scope / faithfulness notes

- **Ported**: `highlight` → `SlidingWindow::highlight`; plus the
  `Node::page_rows` accessor it reads.
- **Faithful**: the `start` / `end` computation and bounds asserts; the top-left
  search (per-meta `meta_i`, the within-one-meta vs start-only cases, the chunk
  row ranges, `top_x` / `bot_x`, the `prune` record, the `unreachable` start
  precondition → `assert`); the bottom-right search (full-page middle chunks,
  the terminal chunk `{0, map.y + 1}`, `bot_x`); the
  `data_offset = start - prune.data + 1`; the prune of metas/data before the
  start meta; the reverse fixup (`> 1` and single-chunk cases, the `top_x` /
  `bot_x` swap); and moving the chunks into the result.
- **Correctness-preserving deviation (reverse `serial`)**: the reverse fixup
  reverses the `Vec<Chunk>`, which reverses each chunk's `serial` together with
  its `node` / `start` / `end`. Upstream reverses only the `node` / `start` /
  `end` parallel arrays and leaves the `serial` array in place — which would
  un-pair `serial` from `node` for a multi-page reverse highlight. Reversing the
  whole `Chunk` keeps `serial` paired with its `node` (the correct behavior,
  since the serial was stamped from that node). This is the one intentional
  divergence; a dedicated test (distinct nodes/serials) guards it.
- **Faithful adaptation**: `MetaBuf.Iterator` + `.idx` → `VecDeque` index
  iteration (`idx - 1` → loop index `i`); `deleteOldest` → `VecDeque::drain`
  (dropping pruned metas subsumes `meta.deinit`); the `MultiArrayList` chunk
  buffer → `Vec<Chunk>` (the reverse fixup reverses the `Vec`); the file-local
  `cell_row` helper narrows `y` (`u32`) → `CellCountInt`;
  `result.chunks = chunk_buf` (an alias upstream) → `chunk_buf.clone()`
  (roastty's `Flattened` owns its chunks; clone retains `chunk_buf`'s capacity);
  the runtime-safety asserts → `debug_assert*`; the `unreachable` preconditions
  → `assert!(found, ...)` (active, since a missed bound is a real bug, not just
  a debug check); the `@intCast` y→`CellCountInt` casts →
  `try_into().expect(...)`; stored `Meta.node` derefs for `page_rows` under the
  window invariant (append's `# Safety`).
- **Deferred**: `next` (the scan + overlap + no-match prune that calls
  `highlight`), and the higher-level searchers.
- The upstream debug-only post-prune cross-check (the first surviving meta's
  node equals the first chunk's node) is preserved as a `debug_assert_eq!` on
  `self.meta.front()` vs `self.chunk_buf.first()` (after the `drain`s, before
  the reverse fixup reorders `chunk_buf`).
- No C ABI/header/ABI-inventory change (internal Rust). Extends
  `terminal::search::sliding_window`; adds one `Node` accessor and widens
  `last_node_ptr`'s visibility.

## Changes

1. `roastty/src/terminal/search/sliding_window.rs`: add
   `SlidingWindow::highlight` and the file-local `cell_row` helper; `Chunk` is
   already imported, add `Flattened` and `CellCountInt` imports; update the
   module doc comment to note `highlight` landed (leaving `next` deferred).
2. `roastty/src/terminal/page_list.rs`: add `Node::page_rows`; change
   `last_node_ptr` to `pub(in crate::terminal)` (the reverse-serial test, and
   the reverse searchers later, need the last page's node pointer).
3. Tests (in `sliding_window.rs`). To exercise the multi-meta (`br`) and prune
   paths without multi-page plumbing, several tests **append the same node
   twice** (two metas over identical page text); the node stays valid, and the
   chunk index/prune math is what's under test:
   - **single-meta forward**: append `"abcdef"` once (data `"abcdef\n"`),
     `highlight(0, 3)` → one chunk `{start: 0, end: 1}`, `top_x == 0`,
     `bot_x == 2`, `data_offset == 1`, no prune (`meta.len() == 1`).
   - **two-meta forward (br path)**: append `"abcdef"` twice, `highlight(5, 4)`
     (spans the meta boundary) → two chunks: the first
     `{start: 0, end: page_rows}`, the second `{start: 0, end: 1}`;
     `top_x == 5`, `bot_x == 1`; `data_offset == 6`; no prune (start in meta 0).
   - **prune path**: append `"abcdef"` twice, `highlight(8, 3)` (match starts in
     the second meta) → one chunk in the surviving meta; `meta.len() == 1` after
     (the first meta pruned), `data` is the second page only, and `data_offset`
     points one past the match within it.
   - **single-chunk reverse fixup**: a reverse window with one appended page,
     `highlight` of an interior match → the single chunk's `start` / `end` are
     swapped per the reverse rule and `top_x` / `bot_x` are swapped.
   - **multi-meta reverse keeps serial paired with node**: build a **two-page**
     `PageList` (via `grow_rows`, so the two nodes have **distinct serials**),
     append them to a reverse window (last page first, per reverse order), and
     `highlight` a match spanning both metas → after the reverse fixup, every
     returned chunk's `serial` still equals its own `node`'s `serial` (the guard
     for the reverse-`serial` deviation). Uses `first_node_ptr` /
     `last_node_ptr`.
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

- `highlight` reproduces upstream's flattened-highlight construction (top-left /
  bottom-right search, the chunk row ranges, `top_x` / `bot_x`, the
  `data_offset` advance, the meta/data prune, and the reverse fixup) — faithful
  to `terminal/search/sliding_window.zig`;
- the tests pass (single-meta / two-meta br / prune / reverse), and the existing
  tests still pass;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the top-left / bottom-right search, the chunk
construction, the `data_offset` advance, the prune, or the reverse fixup
diverges from upstream, an unrelated item changes, or any public C API/ABI
changes.

## Design Review

Codex reviewed the design and **confirmed the hard parts sound**: the
`meta_i = start - prior` / `end - prior` / `end - consumed` subtractions never
underflow (every continued top-left meta proves `meta_consumed <= start`, so the
next `prior` cannot exceed `start`; entering `br` proves `end >= consumed`, and
every continued bottom-right meta preserves `consumed <= end`); the prune after
both loops with `prune_meta = i`, `drain(..prune_meta)`, `drain(..prune_data)`
matches upstream's "delete metas/data before the start meta"; and a safe,
private `highlight` doing internal documented `unsafe` derefs (under the window
invariant from `unsafe append`) is acceptable. It found one Required, one
Optional, and a Nit — all adopted:

- **Required (adopted)**: `Vec<Chunk>::reverse()` also reverses `serial`, which
  upstream's array reversal does **not** — but reversing `serial` with its
  `node` is the _correct_ behavior (the serial is stamped to that node; leaving
  serials in place would make multi-page reverse chunks internally
  inconsistent). The design now documents this explicitly as a deliberate
  correctness-preserving deviation (not exact equivalence), and adds a
  **multi-meta reverse test with distinct nodes/serials** asserting each
  returned chunk's `serial` still matches its `node`'s serial — a case the
  same-node-twice tests cannot catch.
- **Optional (adopted)**: the upstream post-prune debug check (first surviving
  meta's node equals the first chunk's node) is added as a `debug_assert_eq!` on
  `self.meta.front()` vs `self.chunk_buf.first()`, guarded by `prune_meta > 0`
  and placed after the drains (before the reverse fixup reorders `chunk_buf`).
- **Nit (adopted)**: the narrowing helper is named `cell_row` (not `row`) to
  make the `u32 → u16` cast intent clear.

Review artifacts:

- Prompt: `logs/codex-review/20260604-d590-prompt.md`
- Result: `logs/codex-review/20260604-d590-last-message.md`

## Result

**Result:** Pass

`terminal::search::sliding_window` gained `SlidingWindow::highlight` and the
file-local `cell_row` helper; `page_list.rs` gained `Node::page_rows`, promoted
`last_node_ptr` to `pub(in crate::terminal)`, and added a
`#[cfg(test)] grow_to_two_pages_for_tests` helper. `highlight` performs the
top-left search (within-one-meta vs start-only cases), the optional bottom-right
continuation (full-page middle chunks + the terminal chunk), sets `top_x` /
`bot_x`, advances `data_offset = start - prune_data + 1`, prunes the metas/data
before the start meta (with the post-prune `meta.front()` vs `chunk_buf.first()`
debug check), applies the reverse fixup (the `> 1` and single-chunk cases plus
the `top_x` / `bot_x` swap), and clones `chunk_buf` into `result.chunks`. The
reverse fixup reverses whole `Chunk`s, keeping each `serial` paired with its
`node` — the one deliberate correctness-preserving deviation from upstream.

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 3257 passed, 0 failed (five new tests; no
  regressions, up from 3252).
- `cargo build -p roastty`: no warnings (`highlight` is uncalled until `next`,
  but the search module carries `#[allow(dead_code)]`).
- no-`ghostty`-name greps (font/renderer/config + terminal/search + page_list.rs
  - lib.rs/header/abi_harness.c) clean; `git diff --check` clean.

The five new tests: single-meta forward (`highlight(0, 3)` → one chunk `{0, 1}`,
`top_x 0` / `bot_x 2`, `data_offset 1`); two-meta forward `br` path (same node
twice, `highlight(5, 4)` → two chunks `[{0, page_rows}, {0, 1}]`, `top_x 5` /
`bot_x 1`, `data_offset 6`); prune (same node twice, `highlight(8, 3)` → first
meta pruned, `meta.len() == 1`, `data == "abcdef\n"`, `data_offset 2`);
single-chunk reverse fixup (`["ab", "cd"]`, the chunk's `start` / `end` inverted
to `{0, 2}`); and the multi-meta reverse serial guard (a two-page list with
distinct serials, reverse-appended, where every returned chunk's `serial` still
equals its own node's serial).

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no Required
or Optional findings** (one Nit: the `## Result` / `## Conclusion` sections were
not yet saved — added here). Codex confirmed the implementation matches the
upstream `highlight` flow (top-left search, optional bottom-right continuation,
chunk ranges, `top_x` / `bot_x`, `data_offset = start - prune_data + 1`, and the
delayed meta/data pruning are all faithful; the post-prune debug check is in the
right place before the reverse reordering), and that the reverse fixup is sound
— reversing whole `Chunk`s deliberately keeps `serial` paired with `node`, and
the distinct-node reverse test covers the deviation that same-node tests cannot;
`cell_row` and `Node::page_rows` are the appropriate mappings for the row casts
and full-page chunk bounds.

Review artifacts:

- Prompt: `logs/codex-review/20260604-r590-prompt.md` (result)
- Result: `logs/codex-review/20260604-r590-last-message.md` (result)

## Conclusion

This experiment ports `highlight` — the half of the search matcher that turns a
match byte-range into a `Flattened` highlight (a `Chunk` per spanned page plus
`top_x` / `bot_x`), pruning consumed metas/data and advancing `data_offset`. The
`VecDeque` adaptation maps `MetaBuf.Iterator` + `.idx` to index iteration and
`deleteOldest` to `drain`; the one deliberate divergence is reversing whole
`Chunk`s in the reverse fixup so `serial` stays paired with `node` (upstream
reverses only the node/start/end arrays). The remaining matcher half is
`SlidingWindow::next`: scan the window's data — across the two `VecDeque` slices
and the `overlap_buf` for cross-page matches — for the needle (case-insensitive
ASCII), call `highlight` on a hit, special-case 1-length needles, and on a miss
prune everything except the trailing `needle.len() - 1` overlap bytes. After
`next`, the higher-level searchers (`active` / `pagelist` / `screen` /
`viewport`) and the search `Thread` remain.
