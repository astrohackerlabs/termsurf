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

# Experiment 326: font discovery — the collection match

## Description

With `Descriptor::to_core_text_descriptor` in place (Experiment 325), the next
slice of discovery is the **collection match**: turn the descriptor into a
CoreText `CTFontCollection` and ask it for the **matching font descriptors** —
the candidate list a search yields. This experiment ports the core of upstream
`CoreText.discover` up to (but not including) the `Score` sort, the
`DiscoverIterator`, and `DeferredFace`, which are the next experiments.

## Upstream behavior (`discovery.zig` `CoreText.discover`)

```zig
const ct_desc = try desc.toCoreTextDescriptor();
var ct_desc_arr = [_]*const FontDescriptor{ct_desc};
const desc_arr = try Array.create(FontDescriptor, &ct_desc_arr);   // CFArray of 1
const set = try FontCollection.createWithFontDescriptors(desc_arr);
const list = set.createMatchingFontDescriptors();                  // CFArray of matches
const zig_list = try copyMatchingDescriptors(alloc, list);         // retained []*FontDescriptor
sortMatchingDescriptors(&desc, zig_list);                          // (deferred — next experiment)
return DiscoverIterator{ .list = zig_list, ... };                 // (deferred)
```

`copyMatchingDescriptors` copies the `CFArray` of matched descriptors into an
owned list, **retaining** each element (the array releases its members when
freed). This experiment ports through that copy — producing the owned candidate
list — and stops there.

## Rust mapping

- `roastty/Cargo.toml`: enable `CFArray` (`objc2-core-foundation`) and
  `CTFontCollection` (`objc2-core-text`).
- `roastty/src/font/discovery.rs`: add
  `pub(crate) fn discover_descriptors(&self) -> Vec<CFRetained<CTFontDescriptor>>`
  on `Descriptor`:
  - `let ct_desc = self.to_core_text_descriptor();`
  - Wrap it in a one-element `CFArray`:
    `CFArray::from_retained_objects(&[ct_desc])`.
  - `CTFontCollection::with_font_descriptors(array, None)` → the collection.
  - `collection.matching_font_descriptors()` → `Option<CFRetained<CFArray>>`;
    `None` (no matches) yields an empty `Vec`.
  - Copy each element into the `Vec`: iterate `0..array.len()`, `array.get(i)`
    downcast to `CFRetained<CTFontDescriptor>` (the `CFArray::get` already
    retains, the analog of upstream's explicit `retain()`).
  - `unsafe` CF/CT calls carry `// SAFETY:` notes.
- The candidate list is returned **unsorted** (the `Score` sort is the next
  experiment); upstream's order before the sort is CoreText's own.

## Scope / faithfulness notes

- **Ported**: `CoreText.discover` through `copyMatchingDescriptors` — the
  descriptor → `CFArray` → `CTFontCollection` → matching descriptors → owned,
  retained `Vec`.
- **Deferred**: `sortMatchingDescriptors`/`Score` (the next experiment), the
  `DiscoverIterator` and `DeferredFace`, `discoverFallback`/`discoverCodepoint`,
  and the resolver wiring.
- No C ABI/header/ABI-inventory change (`Descriptor`/`CTFontDescriptor` are
  internal Rust); the only build change is enabling already-present objc2
  binding features.

## Changes

1. `roastty/Cargo.toml`: enable `CFArray` and `CTFontCollection`.
2. `roastty/src/font/discovery.rs`: add `Descriptor::discover_descriptors`.
3. Tests (in `discovery.rs`):
   - `discover_descriptors_finds_menlo`: a
     `Descriptor { family: Some("Menlo"), .. }` returns a non-empty `Vec`, and
     at least one candidate's `kCTFontFamilyNameAttribute` reads back as
     `"Menlo"`.
   - `discover_descriptors_monospace`: a `Descriptor { monospace: true, .. }`
     returns a non-empty `Vec` (the system has monospace faces) — exercising the
     traits path through the collection.
   - `discover_descriptors_unknown_family`: a
     `Descriptor { family: Some("__no_such_font__"), .. }` returns an empty (or
     non-matching) `Vec` without panicking — proving the `None`/empty-match path
     is handled.
   - (The exact assertions are finalized against CoreText's matching behavior
     during implementation; CoreText may return a fallback for an unknown
     family, so the unknown-family test asserts no panic and that the result
     does not claim the requested family.)
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty discover
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `discover_descriptors` reproduces upstream's `discover` through
  `copyMatchingDescriptors` — the descriptor → collection → matching descriptors
  → owned retained `Vec`, with `None` handled as empty;
- the Menlo, monospace, and unknown-family tests pass;
- the `Score` sort, the iterator, `DeferredFace`, and the resolver wiring stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if CoreText's matching makes a candidate-list
assertion non-deterministic on the test host (the matching call is still
exercised and the `None`/empty path proven).

The experiment **fails** if the collection construction or the
matched-descriptor copy diverges from upstream, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no Required
findings**. It confirmed the planned pipeline matches upstream's
`CoreText.discover` through `copyMatchingDescriptors` (build the query
descriptor → wrap in a one-element `CFArray` → create the `CTFontCollection` →
`matching_font_descriptors` → copy into an owned retained `Vec`), that returning
the list **unsorted** is a clean slice (the `Score` sort is explicitly the next
step) so long as the method/docs say "candidate descriptors" rather than final
discovery order, that `CFArray::get` already returns a retained `CFRetained<T>`
(so do **not** retain again — the right analog of upstream's explicit
`retain()`), that `None → Vec::new()` is reasonable, and that the unknown-family
test caveat is handled sensibly (CoreText can be permissive, so "empty or no
candidate equals the impossible name" is better than requiring strictly empty).

One **implementation note** (not Required): `matching_font_descriptors()`
returns an **opaque** `CFArray`, so the implementation likely needs to view/cast
it as `CFArray<CTFontDescriptor>` before `get(i)` (the ownership rule is
unchanged — `get(i)` retains; push that retained descriptor directly). Folded
into the implementation plan.

Review artifacts:

- Prompt: `logs/codex-review/20260603-113258-462902-prompt.md`
- Result: `logs/codex-review/20260603-113258-462902-last-message.md`

## Result

**Result:** Pass

The collection match lands.

- `roastty/Cargo.toml`: enabled `CFArray` (`objc2-core-foundation`) and
  `CTFontCollection` (`objc2-core-text`).
- `roastty/src/font/discovery.rs`:
  `Descriptor::discover_descriptors(&self) -> Vec<CFRetained<CTFontDescriptor>>`
  builds the query descriptor, wraps it in a one-element `CFArray`
  (`CFArray::from_retained_objects`), creates a `CTFontCollection`
  (`with_font_descriptors`), asks for the matching descriptors
  (`matching_font_descriptors`, `None → empty Vec`), casts the opaque match
  array to `CFArray<CTFontDescriptor>` (`CFRetained::cast_unchecked`), and
  copies each retained element (`get(i)`) into an owned `Vec`. The list is
  returned **unsorted** (the `Score` sort is the next experiment).

Tests: `discover_descriptors_finds_menlo` (non-empty; a candidate reads back the
`"Menlo"` family), `discover_descriptors_monospace` (a monospace-trait search
yields faces), `discover_descriptors_unknown_family` (no candidate claims an
impossible family; no panic).

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2703 passed, 0 failed (+3, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

Discovery can now produce the candidate descriptor list for a `Descriptor` — the
descriptor → `CTFontCollection` → matching descriptors pipeline, faithful to
upstream through `copyMatchingDescriptors`.

The next discovery experiment is the **`Score` sort**
(`sortMatchingDescriptors`/ `Score` — loading each candidate font, reading its
glyph count, codepoint coverage, symbolic traits, and `head`-table bold/italic
to rank the candidates), then the `DiscoverIterator`/`DeferredFace`, then
`discoverFallback`/ `discoverCodepoint`, and finally the resolver's
discovery-based fallback and codepoint overrides in `get_index`.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no Required findings**. It confirmed the pipeline faithfully ports upstream
through `copyMatchingDescriptors` (query descriptor → one-element `CFArray` →
`CTFontCollection` → matching descriptor array → owned retained `Vec`), and that
deferring the `Score` sort is acceptable because the method documents the list
as **unsorted candidates**. It confirmed ownership is correct:
`CFArray::from_retained_objects` retains the query descriptor before the
temporary drops; `CFRetained::cast_unchecked` to `CFArray<CTFontDescriptor>` is
sound (CoreText documents that array as font descriptors); and `get(i)` already
retains each descriptor, so pushing it directly avoids both double-retain and
dangling references. It confirmed `Some(query.as_opaque())`/`None` match the
binding signature, `None → empty Vec` is reasonable, and the tests are valid and
non-flaky (family matching, trait-path matching, and unknown-family
non-panic/non-claim, none depending on sorted order). No Optional findings.

Review artifacts:

- Result review: `logs/codex-review/20260603-113613-763630-last-message.md`
