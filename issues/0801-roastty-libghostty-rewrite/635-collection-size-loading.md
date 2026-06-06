+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 635: Collection Size Loading

## Description

Port the collection-size/load-size behavior for Roastty's CoreText font
collection.

Experiment 633 added deferred collection entries, but intentionally left the
physical resize path for a later experiment. That gap is now the next blocker
before the `Collection`/resolver/discovery checklist line can be audited: eager
fallback faces record a scale factor but are not resized, and deferred faces
load at CoreText's probe size until `Face::set_size` is called.

This experiment should give `Collection` an explicit point-size setting and use
it to resize loaded faces and future deferred loads. It should stay within the
existing macOS/CoreText scope and avoid introducing upstream's full
`DesiredSize` DPI structure unless a later renderer slice needs it.

## Upstream behavior

`vendor/ghostty/src/font/Collection.zig` stores `load_options.size`, resizes
eager faces on add when load options are present, resizes deferred faces after
load, and exposes `setSize` to update loaded faces plus future deferred loads.
Fallback faces use their recorded scale factor so the adjusted metric stays
aligned with the primary face.

Roastty's narrow equivalent can be:

- `Collection::set_point_size(points: f64)`;
- an optional collection point size stored on `Collection`;
- invalid point sizes are rejected before mutating the collection;
- empty/no-primary collections store the future point size but have no metrics
  to recompute yet, matching the existing `update_metrics` behavior that only
  errors when a caller explicitly asks for metrics;
- eager `add`/`add_with_adjustment` resize loaded faces when a point size is
  set;
- `load_deferred_entry` resizes loaded deferred faces using the resolved scale;
- existing metrics are recomputed after size changes.

## Changes

1. Update `roastty/src/font/collection.rs`:
   - add `point_size: Option<f64>` to `Collection`;
   - add `Collection::set_point_size(points: f64)`;
   - reject non-positive or non-finite point sizes with a small error enum;
   - resize every direct loaded entry to `points * scale_factor`;
   - update the stored point size for future deferred loads;
   - clear/recompute metrics only when a primary face is available; on an empty
     collection, keep the point size for future additions without fabricating
     metrics;
   - resize eager additions when a point size is already set;
   - resize deferred loads after their pending scale factor is resolved.
2. Tests:
   - setting the size resizes the primary face and updates metrics;
   - setting the size on an empty collection stores the future size and does not
     fabricate metrics;
   - setting the size after an adjusted fallback already exists resizes that
     fallback with its recorded scale factor;
   - setting the size before an eager fallback add resizes the added face with
     its scale factor;
   - setting the size before a deferred fallback load resizes the loaded face
     with its scale factor;
   - setting size on a collection with aliases resizes only direct entries and
     aliases continue to resolve;
   - invalid point sizes are rejected.

## Verification

- `cargo test -p roastty collection_size`
- `cargo test -p roastty collection_deferred`
- `cargo test -p roastty discovery_fallback`
- `cargo test -p roastty shared_grid`
- `cargo test -p roastty`
- `cargo fmt -p roastty -- --check`
- `rg -n "physical resize to the collection size is deferred|collection-size / load-options path|Collection-size resize lands in a later experiment" roastty/src/font/collection.rs`
- `git diff --check`

Pass = loaded and deferred collection entries honor an explicit collection point
size, fallback scale factors are applied on resize/load, existing resolver and
shared-grid behavior stays green, and stale resize-deferred comments are gone.

Fail = resizing changes face priority/resolution, aliases duplicate resize work,
deferred loads ignore the stored point size, invalid sizes are accepted, or the
implementation grows beyond the macOS/CoreText scope.

## Design Review

**Reviewer:** Codex (gpt-5.5, medium) · resumed session
`019e8f83-9029-7d43-8e82-f4c5754e14ba`

**Verdict:** APPROVED.

Initial review found two required fixes: define `set_point_size` behavior for an
empty/no-primary collection, and add a test for resizing an already-loaded
adjusted fallback after setting the collection size. The plan now specifies that
empty collections keep the future point size without fabricating metrics and
adds the already-loaded adjusted fallback resize test. Follow-up review approved
the revised design.
