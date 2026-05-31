# Experiment 20: Port Page Hyperlink Storage

## Description

Port the Page hyperlink storage substrate needed before `cloneFrom` can support
hyperlinks.

Experiments 17-19 brought Page row copy through plain cells, graphemes, and
styles. The remaining row-copy guard is hyperlinks. Unlike styles and graphemes,
Roastty does not yet have Page hyperlink storage initialized at all: the layout
constants exist, but `Page` has no `string_alloc`, `hyperlink_map`, or
`hyperlink_set` fields and no `insertHyperlink`, `lookupHyperlink`,
`setHyperlink`, or `clearHyperlink` behavior.

This experiment should add that substrate without wiring hyperlink row-copy yet.
The next experiment can then replace the final `cloneFrom` hyperlink guard.

## Changes

1. Inspect upstream source.
   - Use `vendor/ghostty/src/terminal/hyperlink.zig` and
     `vendor/ghostty/src/terminal/page.zig` as the source of truth.
   - Re-read:
     - `hyperlink.Hyperlink`;
     - `hyperlink.PageEntry`;
     - `hyperlink.Set`;
     - `Page.insertHyperlink`;
     - `Page.lookupHyperlink`;
     - `Page.setHyperlink`;
     - `Page.clearHyperlink`;
     - `Page.updateRowHyperlinkFlag`;
     - `Page.hyperlinkCount`;
     - `Page.hyperlinkCapacity`.
   - Do not implement hyperlink `cloneFrom` migration in this experiment.
   - Do not modify `vendor/ghostty/`.

2. Add a Roastty hyperlink module or Page-local equivalent.
   - Prefer a new `roastty/src/terminal/hyperlink.rs` module if it keeps Page
     readable.
   - Add value types equivalent to upstream:
     - external `Hyperlink` input with URI plus explicit/implicit ID;
     - Page-backed `PageEntry`;
     - Page-backed ID enum for explicit string offsets or implicit integer.
   - Use Roastty naming and Rust types. Do not expose `ghostty` names.
   - Keep strings as Page-backed `OffsetSlice<u8>` values, not heap `String`s
     inside Page entries.

3. Add hyperlink fields to Page.
   - Add and initialize:
     - `string_alloc: StringAlloc`;
     - `hyperlink_map: Option<OffsetHashMap<Offset<Cell>, HyperlinkId>>`;
     - `hyperlink_set: hyperlink::Set` or a Page-local
       `RefCountedSet<PageEntry>`.
   - Initialize these from the existing layout regions:
     - `layout.string_alloc_start`;
     - `layout.string_alloc_layout`;
     - `layout.hyperlink_map_start`;
     - `layout.hyperlink_map_layout`;
     - `layout.hyperlink_set_start`;
     - `layout.hyperlink_set_layout`.
   - Follow the style-set lesson from Experiment 17: if a structure expects its
     base argument to be the start of its own region, pass the region pointer,
     not an `OffsetBuf` whose `BaseAddress` resolves to the whole Page base.
   - Include the new fields in `Page::clone_page` by value. Whole-page clone is
     still byte-copy plus copied offset metadata.

4. Add Page hyperlink operations.
   - Add internal Page APIs matching upstream behavior:
     - `insert_hyperlink`;
     - `lookup_hyperlink_at` / offset-keyed helper;
     - `set_hyperlink`;
     - `clear_hyperlink`;
     - `update_row_hyperlink_flag`;
     - `hyperlink_count`;
     - `hyperlink_capacity`.
   - `insert_hyperlink` must allocate URI and explicit ID strings inside
     `string_alloc`, then insert the `PageEntry` into the ref-counted hyperlink
     set.
   - `insert_hyperlink` must faithfully roll back partial work on failure:
     - if URI allocation succeeds but explicit ID allocation fails, free the URI
       allocation;
     - if URI and explicit ID allocation succeed but hyperlink set insertion
       fails, free both string allocations;
     - leave no set entry, map entry, or ref-counted resident behind on failure.
   - `set_hyperlink` must:
     - create/update the cell-to-hyperlink map entry;
     - release an old hyperlink ID when replacing it;
     - set `cell.hyperlink` and `row.hyperlink`;
     - not increment the new hyperlink ref count itself, matching upstream
       caller responsibility.
     - release the old ID even when replacing a cell with the same ID, because
       the caller is expected to have already acquired the replacement ref; the
       final ref count should be unchanged in the same-ID replacement case.
   - `clear_hyperlink` must:
     - remove the cell map entry;
     - release the hyperlink set ref count;
     - clear the cell hyperlink bit.
   - `update_row_hyperlink_flag` must set the row flag from a whole-row scan.
   - Error types should distinguish:
     - string allocation out of memory;
     - hyperlink set out of memory;
     - hyperlink set needs rehash;
     - hyperlink map out of memory.

5. Add hyperlink set equality and cleanup.
   - Hash/equality must compare the actual Page-backed URI and explicit ID
     string bytes, not just offsets.
   - When a hyperlink set entry is deleted, free its URI and explicit ID string
     slices from `string_alloc`.
   - If the current `RefCountedSet` context API is insufficient for Page-backed
     source/destination comparison and deletion, extend it in the smallest
     reusable way rather than adding a heap map.
   - Do not use Rust's randomized `HashMap`/`DefaultHasher` for Page hyperlink
     storage.

6. Add focused tests.
   - Initialization:
     - Page initializes string allocator, hyperlink map, and hyperlink set from
       layout;
     - zero hyperlink/string capacity pages fail insertion cleanly.
   - Insert/lookup:
     - implicit hyperlink insert stores URI and implicit ID;
     - explicit hyperlink insert stores URI and explicit ID;
     - lookup by cell returns the expected hyperlink set ID.
   - Set/clear:
     - setting a hyperlink marks cell and row flags;
     - clearing removes the map entry, releases the ref, clears cell flag, and
       row flag clears after update when no hyperlink cells remain.
   - Replacement:
     - setting a different hyperlink on the same cell releases the old ref and
       maps the cell to the new ID;
     - setting the same hyperlink on a cell follows upstream behavior and does
       not double-count through `set_hyperlink`;
     - add the exact same-ID case where the caller pre-increments the ID, calls
       `set_hyperlink` on an already-linked cell, and the final ref count is
       unchanged with cell/row flags still set.
   - Deduplication:
     - inserting the same implicit hyperlink and URI reuses the same set entry;
     - inserting different implicit IDs with the same URI creates distinct set
       entries;
     - inserting the same explicit ID and URI reuses the same set entry;
     - different URI or different explicit ID produces a distinct entry.
   - Count/capacity semantics:
     - set one hyperlink ID on multiple cells;
     - verify `hyperlink_count()` reports the number of linked cells in the map,
       not the number of unique hyperlink set entries;
     - verify the hyperlink set has one unique entry with a ref count matching
       the linked-cell uses.
   - Failure rollback:
     - force URI-success / explicit-ID-failure and verify used string bytes are
       restored and no set entry exists;
     - force string-success / set-insertion-failure and verify used string bytes
       are restored and no set entry exists.
   - Whole-page clone:
     - byte-copy `Page::clone_page` preserves hyperlink map/set/string data;
     - source and clone are independent after releasing source refs.

7. Preserve scope.
   - Do not implement:
     - hyperlink `cloneFrom` / row-copy migration;
     - `exactRowCapacity`;
     - `clonePartialRowFrom`;
     - terminal OSC 8 parsing;
     - URI validation;
     - public ABI or app-facing APIs.
   - Keep the row-copy method's hyperlink guard in place.

8. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal::page
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

9. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - module/files added;
     - Page fields added;
     - hyperlink operation APIs added;
     - tests added;
     - any `RefCountedSet` changes;
     - deferred row-copy/exact-capacity work;
     - verification command output summary.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- Page initializes string allocator, hyperlink map, and hyperlink set inside
  Page backing memory;
- implicit and explicit hyperlink entries store strings in Page memory;
- hyperlink set equality/hashing compares string contents, not offset values;
- insertion deduplicates matching hyperlinks and distinguishes different ones;
- set/lookup/clear/update-row-flag behavior matches upstream;
- replacement releases old refs and maps cells to the new ID;
- same-ID replacement releases the old ref while consuming the caller's
  pre-acquired replacement ref, leaving the final ref count unchanged;
- hyperlink count reports linked cells, not unique hyperlink entries;
- one hyperlink ID used by multiple cells has one set entry and multiple refs;
- different implicit IDs with the same URI are distinct hyperlinks;
- partial `insert_hyperlink` failures roll back string allocations and set
  state;
- whole-page byte-copy clone preserves hyperlink data without source/clone
  backing-memory aliasing;
- zero-capacity insertion fails cleanly without corrupting Page state;
- row-copy hyperlink migration remains out of scope and guarded;
- no heap `HashMap`/`Vec` side storage is used as the Page hyperlink storage
  source of truth;
- `cargo fmt`, targeted Page tests, and full `cargo test -p roastty` pass;
- Codex reviews the completed result and approves it or all real findings are
  fixed.

The experiment is partial if:

- Page can initialize and use hyperlink map/set storage, but `RefCountedSet`
  needs a focused context/deletion extension before explicit-ID/URI cleanup can
  be faithful.

The experiment fails if:

- hyperlink strings are stored outside Page backing memory as the source of
  truth;
- hyperlink set equality compares only offsets;
- clearing/replacing links leaks refs or leaves stale cell/row flags;
- whole-page clone aliases mutable hyperlink backing storage with the source;
- row-copy hyperlink migration is implemented prematurely;
- existing Page layout, style, grapheme, or row-copy tests regress.

## Codex Review

This experiment design must be reviewed by Codex before implementation. Any real
design issues must be fixed before committing the plan or implementing the
slice.
