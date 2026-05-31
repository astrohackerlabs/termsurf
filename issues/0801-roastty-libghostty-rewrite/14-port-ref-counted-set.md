# Experiment 14: Port Ref-Counted Set Storage

## Description

Port Ghostty's offset-backed `terminal/ref_counted_set.zig` foundation into
Roastty.

The next blocked Page behaviors are style storage and hyperlink storage:

- `Page clone styles`
- `Page verifyIntegrity styles ...`
- `Page exactRowCapacity styles ...`
- hyperlink clone/copy/exact-capacity tests

Both upstream systems use `RefCountedSet`:

- `style.Set = RefCountedSet(Style, style.Id, CellCountInt, Context)`
- `hyperlink.Set = RefCountedSet(PageEntry, hyperlink.Id, CellCountInt, Context)`

This experiment should port the reusable set before wiring it into `Page`. It
should not implement Page style operations or hyperlink operations yet.

## Changes

1. Inspect upstream source.
   - Use `vendor/ghostty/src/terminal/ref_counted_set.zig` as the source of
     truth.
   - Re-read the style/hyperlink set instantiations in:
     - `vendor/ghostty/src/terminal/style.zig`
     - `vendor/ghostty/src/terminal/hyperlink.zig`
   - Do not modify `vendor/ghostty/`.

2. Add a Roastty ref-counted set module.
   - Add `roastty/src/terminal/ref_counted_set.rs`.
   - Register it from `roastty/src/terminal/mod.rs`.
   - Preserve the upstream storage model:
     - caller-provided backing memory;
     - table stores IDs;
     - items live in a flat item array;
     - ID `0` is reserved;
     - open-addressed hash table;
     - linear probing;
     - Robin Hood insertion;
     - `max_psl` and `psl_stats` early-exit behavior;
     - dead items remain until reused or trimmed.

3. Keep the first Rust shape focused on Roastty's current needs.
   - It is acceptable for the initial Rust type to fix IDs and ref counts to the
     aliases used by both current upstream call sites:
     - `StyleCountInt == CellCountInt == u16`
     - `HyperlinkCountInt == CellCountInt == u16`
   - Use a generic value type `T` and a context trait/object suitable for the
     style and hyperlink contexts.
   - If a more general ID/ref-count abstraction is simple and readable, it may
     be used, but do not let generic machinery obscure the port.

4. Port layout behavior.
   - Port `base_align`, `load_factor`, `capacityForCount`, and `Layout.init`.
   - Replace the temporary layout helpers currently embedded in `page.rs` only
     if doing so stays mechanical and does not introduce Page behavior.
   - Keep existing Page layout tests green. Any replacement must preserve the
     numeric layout values already tested by Page.

5. Port core operations.
   - Port:
     - `init`
     - `add` / context-aware add equivalent
     - `addWithId` / context-aware add-with-id equivalent
     - `use`
     - `useMultiple`
     - `get`
     - `release`
     - `releaseMultiple`
     - `refCount`
     - `count`
     - `lookup`
   - Internal helpers should preserve upstream behavior:
     - `deleteItem`
     - `upsert`
     - `insert`
     - integrity assertion helper, disabled by default as upstream does
   - Preserve upstream `addWithId` return semantics explicitly:
     - requested ID used: return success with no alternate ID
     - alternate ID chosen: return success with that alternate ID
     - Rust shape should be `Result<Option<Id>, AddError>` or an equally clear
       enum where `None` means the requested ID was used

6. Define the context API carefully.
   - The context must support:
     - hashing a candidate value;
     - equality between the candidate and resident item;
     - an optional deletion callback for values that own auxiliary storage.
   - Style will not need deletion.
   - Hyperlinks will need deletion later to free string allocations.
   - This experiment should test deletion-callback behavior with a simple test
     context, but should not port hyperlink string ownership yet.
   - Deletion callbacks are non-reentrant for the same set. A callback must not
     call back into the same `RefCountedSet`.
   - Invoke deletion callbacks without holding long-lived mutable table/item
     slices or other aliases that would conflict with later hyperlink callbacks
     freeing page-backed string allocations.

7. Keep the unsafe boundary narrow.
   - Offset-to-slice conversion must stay inside this module's memory-view
     helpers.
   - Public/internal operations should remain safe once the set is initialized
     with a valid backing buffer.
   - Document safety invariants at the unsafe conversion sites:
     - backing memory must cover `layout.total_size`;
     - table and item offsets must be correctly aligned;
     - callers must not alias mutable set operations over the same backing
       memory.

8. Add tests.
   - Upstream `ref_counted_set.zig` has no dedicated tests, so create direct
     Roastty tests from the documented behavior and from Page's expected use.
   - Cover at least:
     - layout for zero capacity;
     - layout for normal capacity, including power-of-two table size and
       reserved ID capacity;
     - exact layout values for `base_align`, `table_start`, `items_start`,
       `total_size`, representative `Item` size/alignment, and metadata
       size/alignment;
     - `capacity_for_count(0) == 0`;
     - adding a new item returns a nonzero ID and count increments;
     - adding the same value reuses the same ID and increments ref count;
     - lookup returns only living items;
     - `use` and `useMultiple` increment ref count;
     - `release` and `releaseMultiple` decrement ref count and living count;
     - dead item can be resurrected or reused according to upstream ID rules;
     - `addWithId` uses the requested ID when valid;
     - `addWithId` returns an alternate ID when Robin Hood/dead-item reuse
       chooses another ID;
     - OutOfMemory when the set is full enough that no ID is available;
     - NeedsRehash when enough dead IDs exist below `next_id`;
     - deletion callback fires when an incoming duplicate value is discarded;
     - deletion callback fires when a dead resident value is finally overwritten
       or trimmed.
   - Add collision-heavy tests with a deterministic test context so Robin Hood
     insertion and lookup are exercised. Do not depend only on Rust's default
     hash behavior.

9. Preserve scope.
   - Do not implement `style.Set` integration in this experiment.
   - Do not implement `hyperlink.Set`, `PageEntry`, string allocation, or
     hyperlink deletion from Page memory.
   - Do not implement Page `setStyle`, `clone styles`, `verifyIntegrity`, or
     `exactRowCapacity`.
   - Do not rewrite existing style value formatting behavior except for imports
     needed to compile.

10. Verify.
    - Run:

      ```bash
      cargo fmt
      cargo test -p roastty terminal::ref_counted_set
      cargo test -p roastty terminal::page
      cargo test -p roastty
      ```

    - `cargo fmt` output must be accepted as-is.

11. Record the result.
    - Append `## Result` and `## Conclusion` to this file.
    - Include:
      - Rust API shape chosen;
      - whether IDs/ref counts are fixed to `u16` or generic;
      - layout compatibility notes;
      - unsafe boundary summary;
      - tests added;
      - any behavior deliberately deferred;
      - verification command output summary.
    - Update the Issue 801 README experiment index from `Designed` to `Pass`,
      `Partial`, or `Fail`.

## Verification

The experiment passes if:

- Roastty has a reusable offset-backed ref-counted set module;
- layout and capacity behavior matches Ghostty's documented behavior;
- ID `0` remains reserved and is never returned;
- lookup, add, add-with-id, use, release, ref-count, dead-item reuse, OOM, and
  NeedsRehash behavior are covered by tests;
- deletion callback behavior is covered without pulling in hyperlinks;
- existing Page layout tests remain green;
- no Page style/hyperlink behavior is introduced;
- `cargo fmt`, targeted ref-counted-set tests, targeted Page tests, and full
  `cargo test -p roastty` pass;
- Codex reviews the completed result and approves it or all real findings are
  fixed.

The experiment is partial if:

- layout and basic add/release behavior work, but a Robin Hood edge case or
  NeedsRehash rule is ambiguous enough that the next experiment must focus on
  parity diagnostics before style integration.

The experiment fails if:

- the implementation uses heap `HashMap`/`Vec` storage instead of the
  offset-backed page-memory model;
- ID `0` can be returned for a live item;
- dead items are removed immediately instead of preserving upstream resurrection
  behavior;
- deletion callbacks are ignored;
- Page layout numeric tests regress;
- the experiment drifts into Page style or hyperlink behavior.

## Codex Review

This experiment design must be reviewed by Codex before implementation. Any real
design issues must be fixed before committing the plan or implementing the
slice.

## Result

**Result:** Pass

Experiment 14 added Roastty's reusable offset-backed ref-counted set foundation.

The implementation added:

- `roastty/src/terminal/ref_counted_set.rs`
- `terminal::ref_counted_set` registration in `roastty/src/terminal/mod.rs`

### API Shape

The Rust port uses:

- `RefCountedSet<T>`
- `Layout`
- `Item<T>`
- `Metadata`
- `Context<T>`
- `AddError`

The value type `T` is generic and currently requires `Copy + Default` for the
offset-backed item array initialization model.

IDs and reference counts are fixed to the aliases used by both current upstream
call sites:

- `Id = CellCountInt = u16`
- `RefCount = CellCountInt = u16`

That keeps the first port focused on `style.Set` and `hyperlink.Set`, whose
upstream IDs/ref counts are also `CellCountInt` aliases.

`add_with_id` preserves Ghostty's nullable return semantics as
`Result<Option<Id>, AddError>`:

- `Ok(None)` means the requested ID was used;
- `Ok(Some(id))` means an alternate ID was chosen;
- `Err(...)` means the add failed.

### Storage and Layout

The set preserves Ghostty's storage model:

- caller-provided backing memory;
- table stores IDs;
- ID `0` is reserved;
- items live in a flat item array;
- open-addressed linear probing;
- Robin Hood insertion;
- `max_psl` and `psl_stats`;
- dead items remain until reused or trimmed.

The implementation does not replace Page's temporary style/hyperlink layout
helpers yet. That keeps this experiment limited to the reusable module. Existing
Page layout numeric tests remained green.

### Unsafe Boundary

Unsafe code is limited to converting offset-backed table/item regions into
slices. The conversion sites document the backing-memory and alignment
invariants.

Deletion callbacks are treated as non-reentrant for the same set. The
implementation avoids holding long-lived mutable table/item slices across
callback calls, which is important for future hyperlink deletion callbacks that
will free page-backed string allocations.

### Tests Added

The new `terminal::ref_counted_set` test suite covers:

- zero-capacity layout;
- exact normal layout values;
- `capacity_for_count`;
- new add;
- duplicate add and incoming deletion callback;
- lookup excluding dead items;
- `use`, `useMultiple`, `release`, and `releaseMultiple`;
- dead item reuse;
- `add_with_id` requested-ID and alternate-ID semantics;
- add-with-id duplicate reuse;
- OutOfMemory;
- NeedsRehash;
- resident deletion callback on overwrite;
- trailing deletion callback on trim;
- collision-heavy lookup.

Upstream `ref_counted_set.zig` has no dedicated tests, so these tests are direct
Rust equivalents derived from the documented behavior and from the Page
style/hyperlink requirements.

### Deferred

This experiment intentionally did not add:

- `style.Set` integration;
- `hyperlink.Set` / `PageEntry`;
- Page style operations;
- Page hyperlink operations;
- Page style clone;
- Page hyperlink clone/copy/exact-capacity behavior.

### Verification

The required verification passed:

```bash
cargo fmt
cargo test -p roastty terminal::ref_counted_set
cargo test -p roastty terminal::page
cargo test -p roastty
```

Observed results:

- `terminal::ref_counted_set`: 16 passed
- `terminal::page`: 50 passed
- full `roastty` suite: 152 Rust unit tests passed, C ABI harness passed, doc
  tests passed

## Conclusion

Roastty now has the ref-counted set foundation needed for Page styles and
hyperlinks. The next experiment can use this module to wire style storage into
`Page` and port the first style-backed Page behavior, most likely
`Page clone styles`.
