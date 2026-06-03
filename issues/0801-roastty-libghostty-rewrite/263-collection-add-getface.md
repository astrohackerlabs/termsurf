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

# Experiment 263: Collection — eager faces, add / get_face

## Description

Building on the `Index` (Experiment 262), this experiment ports the core of the
`Collection` itself (`font/Collection.zig`): the per-style storage of faces, the
`Entry` that owns a face, and `add` / `get_entry` / `get_face` for **eagerly
loaded** faces. This is the minimal store-and-retrieve spine the
`CodepointResolver` and `SharedGrid` build on. Deferred (lazy) faces and the
`discovery` subsystem, the per-entry scale-factor / `load_options` size
normalization, style aliasing / `completeStyles`, and codepoint resolution
(`getIndex`/`hasCodepoint`) are deferred to later experiments.

### Upstream behavior (`font/Collection.zig`)

- `faces: StyleArray` — an `EnumArray(Style, …)` mapping each style to a list of
  `EntryOrAlias`.
- `add(face, opts) -> Index` (lines 112–150): append the face to the
  `opts.style` list (as a `loaded` entry), returning
  `Index{ style, idx = list.count() }`. Guards `idx >= Index.Special.start - 1`
  → `error.CollectionFull` (special indices must never be produced).
- `Entry` (lines 751–836): owns `face: union { loaded: Face, deferred }`,
  `fallback: bool`, and a `scale_factor`.
- `getEntry(index)` (lines 210–215): `index.special() != null` →
  `error.SpecialHasNoFace`; `index.idx >= list.len` → `error.IndexOutOfBounds`;
  else the entry.
- `getFace(index)` → `getFaceFromEntry`: for a `loaded` entry returns the face;
  a `deferred` entry triggers loading (deferred here).

### Rust mapping (`roastty/src/font/collection.rs`)

This slice extends the existing `collection.rs` (which holds `Index`):

- `struct Entry { face: Face, fallback: bool }` — an eagerly-loaded face plus
  the fallback flag. (`face` is the union's `loaded` arm; the `deferred` arm and
  the per-entry `scale_factor` are deferred.) `Face` is
  `crate::font::face::coretext::Face`.
- `struct Collection { faces: [Vec<Entry>; 4] }` — the per-style lists, indexed
  by `Style as usize` (`Regular..BoldItalic` = `0..3`). `Collection::new()` →
  empty lists. (The list element is `Entry` directly; the `EntryOrAlias`
  indirection arrives with `completeStyles`.)
- `enum AddError { CollectionFull }` (upstream's `SetSizeFailed` belongs to the
  deferred `load_options`/`setSize` path).
- a small testable guard helper
  `fn list_is_full(len: usize) -> bool { len >= (Special::START - 1) as usize }`
  (`START - 1 = 8190`), so the off-by-one boundary can be checked without
  allocating thousands of faces.
- `add(&mut self, face: Face, style: Style, fallback: bool) -> Result<Index, AddError>`:
  `let idx = list.len();` guard `list_is_full(idx)` → `CollectionFull`; push
  `Entry { face, fallback }`; return `Index::new(style, idx as u16)`.
- `enum EntryError { SpecialHasNoFace, IndexOutOfBounds }`.
- `get_entry(&self, index: Index) -> Result<&Entry, EntryError>`:
  `index.special_kind().is_some()` → `SpecialHasNoFace`;
  `index.idx() as usize >= list.len()` → `IndexOutOfBounds`; else `&list[idx]`.
- `get_face(&self, index: Index) -> Result<&Face, EntryError>`: the loaded face
  of `get_entry(index)?` (deferred loading deferred).
- `Entry::fallback`/the face are exposed via accessors as needed.

### Scope / faithfulness notes

- **Deferred**: `DeferredFace` + `discovery` (lazy loading), the per-entry
  `scale_factor` and `load_options`/`setSize` normalization, `EntryOrAlias` /
  `completeStyles` aliasing, `getIndex`/`hasCodepoint` codepoint resolution,
  `setSize`/`updateMetrics`, and `metric_modifiers`/`metrics`.
- The collection owns its faces by value (Rust ownership replaces the manual
  `deinit` of loaded faces).
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/collection.rs`: add `Entry`, `Collection`, `AddError`,
   `EntryError`, and `new`/`add`/`get_entry`/`get_face`.
2. Tests in `collection.rs` (live CoreText, macOS):
   - `add_and_get_face`: add Menlo as `Regular` (non-fallback) and Apple Color
     Emoji as a `Regular` fallback; the returned indices are `{ Regular, 0 }` /
     `{ Regular, 1 }`; `get_face` returns the right faces (Menlo `!has_color()`,
     the emoji `has_color()`); the entries' `fallback` flags match.
   - `add_to_distinct_styles`: a face added under `Bold` gets index
     `{ Bold, 0 }`, independent of the `Regular` list.
   - `get_entry_special_has_no_face`: `get_entry(Index::special(Sprite))` →
     `Err(SpecialHasNoFace)`.
   - `get_entry_out_of_bounds`: `get_entry(Index::new(Regular, 0))` on an empty
     collection → `Err(IndexOutOfBounds)`.
   - `collection_full_boundary` (no live faces): `list_is_full(8189) == false`
     (count 8189 may still add, producing idx 8189) and
     `list_is_full(8190) == true` (count 8190 → `CollectionFull`), pinning the
     off-by-one guard.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty collection
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Collection` stores faces per style, `add` returns the right `Index` and
  guards `CollectionFull`, and `get_entry`/`get_face` return the face or the
  faithful `SpecialHasNoFace`/`IndexOutOfBounds` errors;
- a live face added and retrieved is the same face (verified via `has_color`),
  with the fallback flag preserved;
- deferred loading, scale-factor, aliasing, and codepoint resolution are cleanly
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if storing `Face` by value in `Entry` needs a
different ownership shape than expected.

The experiment **fails** if `add`/`get_entry` indexing diverges from upstream
(the `CollectionFull`/`special`/bounds guards) or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and raised one **Low** finding:
the test plan didn't directly cover the `CollectionFull` off-by-one boundary (a
key fidelity point). The design was updated to factor the guard into a testable
`list_is_full(len)` helper and add a `collection_full_boundary` test asserting
`8189` is accepted and `8190` is rejected — without allocating thousands of real
faces. Codex found no other issues: the storage shape, `get_entry` guards, the
eager-ownership model, and the deferral of scale-factor / load-options
normalization are sound for this scoped experiment.

Review artifacts:

- Prompt: `logs/codex-review/20260602-215137-506515-prompt.md`
- Result: `logs/codex-review/20260602-215137-506515-last-message.md`

## Result

**Result:** Pass

`collection.rs` gained `Entry` (a loaded `Face` + `fallback`, with `face()` /
`fallback()` accessors), `Collection { faces: [Vec<Entry>; 4] }`, `AddError`,
`EntryError`, the `list_is_full` guard helper, and `new`/`add`/`get_entry`/
`get_face`. `add` appends to the per-style list and returns
`Index::new(style, idx)` with the `CollectionFull` guard; `get_entry` returns
`SpecialHasNoFace` / `IndexOutOfBounds` faithfully.

Tests (live CoreText):

- `add_and_get_face` — Menlo (`Regular`, non-fallback) and Apple Color Emoji
  (`Regular`, fallback) get indices `{Regular,0}`/`{Regular,1}`; `get_face`
  returns the right faces (distinguished by `has_color()`); the `fallback` flags
  round-trip.
- `add_to_distinct_styles` — a `Bold` face is index `{Bold,0}`, independent of
  the `Regular` list.
- `get_entry_special_has_no_face` / `get_entry_out_of_bounds` — the special and
  bounds errors.
- `collection_full_boundary` — `list_is_full(8189) == false`,
  `list_is_full(8190) == true` (the off-by-one guard).

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty collection` → 12 passed, 0 failed.
- `cargo test -p roastty` → 2394 passed, 0 failed (no regressions; +5).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The Collection's eager store-and-retrieve spine is in place. The next
experiments build outward: `getIndex`/`hasCodepoint` (codepoint → face
resolution, the heart of the resolver — needs a `Face::has_codepoint` with
presentation handling), then `EntryOrAlias`/`completeStyles` (style aliasing),
and the per-entry `scale_factor` + `load_options`/`setSize` size normalization.
The deferred-face half (`DeferredFace` + `discovery`) introduces the CoreText
font-matching FFI sub-area. Above the Collection sit the `CodepointResolver`,
the shaper, and the Nerd Font attribute table.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-215428-787960-prompt.md`
- Result: `logs/codex-review/20260602-215428-787960-last-message.md`

Codex confirmed the implementation matches the scoped upstream behavior: `add`
uses `idx = list.len()` and rejects at `idx >= 8190`, `get_entry` checks special
indices before bounds, the per-style `[Vec<Entry>; 4]` indexing is valid for the
current `Style` discriminants, owning `Face` by value with `&Face` access is
sound, and the `collection_full_boundary` test covers the off-by-one guard.
