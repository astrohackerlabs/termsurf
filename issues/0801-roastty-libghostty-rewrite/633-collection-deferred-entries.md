+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 633: Collection Deferred Entries

## Description

Wire the new CoreText `DeferredFace` into `Collection`.

Experiment 632 added the standalone deferred-face primitive and kept discovery's
public surface eager. The next missing upstream behavior is that collection
fallback slots can store deferred faces, search them for codepoint coverage
without loading, and load them only when glyph lookup/rasterization needs the
real `Face`.

This experiment should add deferred entries to `Collection` while keeping the
rest of the resolver behavior unchanged. It should not yet rewrite discovery to
return deferred iterators everywhere; the resolver fallback path can use a small
deferred discovery bridge for this slice.

## Upstream behavior

`vendor/ghostty/src/font/Collection.zig` stores each `Entry` as either
`loaded: Face` or `deferred: DeferredFace`. `getIndex` and `hasCodepoint` do not
load deferred entries; they call `DeferredFace.hasCodepoint`. `getFace` mutates
the collection, loads a deferred entry into a real face, computes any pending
size-adjustment scale, stores the loaded face, and returns it.

Roastty can mirror this with Rust ownership:

- `Entry` stores a local `AnyFace::{Loaded(Face), Deferred(DeferredFace)}`;
- eager `add`/`add_with_adjustment` keep storing `Loaded`;
- `add_deferred_with_adjustment` stores `Deferred` with a pending
  `SizeAdjustment`;
- immutable entry access remains non-loading for metadata, tests, aliases, and
  coverage checks;
- only render-facing face access becomes a mutable loading path;
- read-only coverage methods continue to avoid loading.

## Changes

1. Update `roastty/src/font/collection.rs`:
   - add `AnyFace` and `ScaleFactor` helpers inside the collection module;
   - let `Entry` hold `Loaded(Face)` or `Deferred(DeferredFace)`;
   - make `Entry::has_codepoint` dispatch to `DeferredFace::has_codepoint`
     without loading;
   - add `Entry::is_deferred` for tests;
   - add `Collection::add_deferred_with_adjustment`;
   - keep `get_entry` immutable and non-loading, and add a separate mutable
     entry lookup only for internals that need to load;
   - change render-facing `get_face` access to `&mut self`, loading a deferred
     entry in place, computing any pending scale factor, and returning the
     loaded `Face`;
   - implement deferred loading by temporarily taking/replacing the entry so the
     list borrow ends before scale-factor computation needs `&mut self`; then
     write the loaded entry back to the same slot, preserving the index;
   - keep `get_index` and `has_codepoint` read-only and unloaded.
2. Update `roastty/src/font/codepoint_resolver.rs` and
   `roastty/src/font/shared_grid.rs` for the mutable `get_face` path:
   - `glyph_index`, `get_presentation`, and `render_glyph` may need `&mut self`
     because they can trigger deferred loading;
   - shared-grid atlas rendering should borrow the atlas and resolver as
     disjoint mutable fields.
3. Add a narrow deferred fallback bridge:
   - add a discovery method returning deferred fallback candidates for the
     resolver fallback path;
   - use `Collection::add_deferred_with_adjustment` for discovery fallback
     additions;
   - leave codepoint overrides eager unless they naturally fit the same method,
     since override caching is not the main behavior under test.
4. Tests:
   - adding a deferred Menlo descriptor stores a deferred entry;
   - `get_index`/`has_codepoint` can find a deferred entry without loading it;
   - `get_face` loads the deferred entry in place and preserves the index;
   - a deferred adjusted entry keeps its pending adjustment while unloaded and
     has a finite eager-equivalent scale factor after `get_face` loads it;
   - discovered CJK or emoji fallback resolves to an index whose entry is
     deferred before glyph lookup/render-facing access and loaded after;
   - existing collection, resolver, shared-grid, run, and renderer tests still
     pass.

## Verification

- `cargo test -p roastty collection_deferred`
- `cargo test -p roastty discovery_fallback`
- `cargo test -p roastty codepoint_override`
- `cargo test -p roastty shared_grid`
- `cargo test -p roastty renderer::cell`
- `cargo test -p roastty`
- `cargo fmt -p roastty -- --check`
- `rg -n "Deferred-face loading.*later|deferred entries are deferred|get_face\\(&self" roastty/src/font`
- `git diff --check`

Pass = `Collection` can store deferred fallback entries, coverage checks remain
lazy, render-facing face access loads entries in place, and existing resolver
behavior stays green.

Fail = deferred entries require eager loading for coverage, render-facing access
cannot load safely, existing eager collection behavior regresses, or resolver
fallback behavior changes unexpectedly.

## Design Review

**Reviewer:** Codex (gpt-5.5, medium) · resumed session
`019e8f83-9029-7d43-8e82-f4c5754e14ba`

**Verdict:** APPROVED.

Initial review found three required plan fixes: clarify that `get_entry` remains
immutable and non-loading, specify the borrow-safe take/replace strategy for
loading deferred entries in place, and add a focused pending-scale-factor test.
The plan was updated for all three points. Follow-up review approved the revised
design with no findings.

## Result

**Result:** Pass

`Collection` now stores entries as either loaded `Face`s or deferred
`DeferredFace`s. Eager additions still store loaded faces, while
`add_deferred_with_adjustment` stores a deferred face with a pending
`SizeAdjustment`. `get_index` and `has_codepoint` remain read-only and check
deferred coverage without loading. Render-facing `get_face` now takes
`&mut self`, loads a deferred entry in place, computes the pending scale factor,
and preserves the entry's index.

Discovery now exposes deferred descriptor candidates for the resolver fallback
path. `CodepointResolver` adds those as deferred fallback entries when possible,
while preserving the eager CoreText `font_for_codepoint` path for CJK and for
candidates that CoreText only provides as loaded faces. Shaping and shared-grid
rendering now use mutable resolver/collection access where a deferred face may
need to load.

Verification:

- `cargo fmt -p roastty`
- `cargo test -p roastty collection_deferred` — 5 tests passed
- `cargo test -p roastty discovery_fallback` — 5 tests passed
- `cargo test -p roastty codepoint_override` — 3 tests passed
- `cargo test -p roastty shared_grid` — 5 tests passed
- `cargo test -p roastty renderer::cell` — 93 tests passed
- `cargo test -p roastty` — 3482 unit tests passed, plus the ABI harness
- `cargo fmt -p roastty -- --check`
- `rg -n "Deferred-face loading.*later|deferred entries are deferred|get_face\\(&self|Codepoint overrides are deferred|scoped to \\*\\*eagerly loaded\\*\\*" roastty/src/font`
  — no matches
- `git diff --check`

## Conclusion

This lands the upstream collection lazy-loading model in Roastty: fallback
entries can now remain descriptor-backed for coverage searches and become real
faces only when shaping, glyph lookup, or rendering needs them. The remaining
font checklist work can move outward to the shared grid/grid-set and lower-level
font support modules instead of treating `DeferredFace`/`Collection` as missing
building blocks.

## Completion Review

**Reviewer:** Codex (gpt-5.5, medium) · resumed session
`019e8f83-9029-7d43-8e82-f4c5754e14ba`

**Verdict:** APPROVED.

Initial completion review found that the deferred fallback path could bypass the
locale-aware CJK `font_for_codepoint` fallback. The resolver now routes CJK
codepoints through the loaded fallback path first, and
`discovery_fallback_cjk_uses_loaded_codepoint_search` covers the behavior.

Follow-up review found that loading a deferred primary entry could recurse while
computing its pending scale factor. `load_deferred_entry` now treats a deferred
primary load as scale `1.0`, caches its primary metrics, and
`collection_deferred_primary_loads_without_recursing` covers the behavior.

Second follow-up review approved the corrected staged result with no findings.
