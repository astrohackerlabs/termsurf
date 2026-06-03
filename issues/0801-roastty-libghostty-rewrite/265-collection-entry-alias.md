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

# Experiment 265: Collection EntryOrAlias — style aliasing storage

## Description

Before `completeStyles` can fill missing styles by **aliasing** them to an
existing face (e.g. an absent bold-italic falling back to the regular face), the
Collection's per-style lists must hold `EntryOrAlias`, not bare `Entry`. This
experiment ports `EntryOrAlias` (`font/Collection.zig` lines 837–860) and
threads alias resolution through `get_entry`/`get_index`/`has_codepoint`. The
`completeStyles` logic that _creates_ aliases (and synthetic faces) is the next
experiment.

### Upstream behavior (`font/Collection.zig`)

- `EntryOrAlias = union(enum) { entry: Entry, alias: *Entry }` (lines 837–860).
  `getEntry`/`getConstEntry` return the underlying `*Entry` — for an `entry`,
  itself; for an `alias`, the pointed-to entry. Aliases always point to a real
  `entry` (never another alias).
- The per-style lists are `ArrayList(EntryOrAlias)`. `add` appends
  `.{ .entry = … }`; `completeStyles` appends `.{ .alias = regular_entry }`.
- `getEntry`/`getIndex`/`hasCodepoint` all go through `…getEntry()` /
  `…getConstEntry()`, so they transparently follow aliases.

### Rust adaptation (alias by `Index`, not pointer)

Upstream's `alias: *Entry` is a raw pointer into another style's list — a
self-referential shape Rust's borrow checker disallows. The faithful Rust
equivalent stores the alias **target as an `Index`** (which already names a face
within the collection); resolution follows that index. Since aliases never point
to aliases upstream, the target always resolves to a direct `Entry` in one step.

### Rust mapping (`roastty/src/font/collection.rs`)

- `enum EntryOrAlias { Entry(Entry), Alias(Index) }`.
- `Collection.faces: [Vec<EntryOrAlias>; 4]` (was `[Vec<Entry>; 4]`).
- a private resolver `fn entry_of(&self, eoa: &EntryOrAlias) -> &Entry`: an
  `Entry` returns itself; an `Alias(target)` returns the `Entry` at
  `faces[target.style()][target.idx()]` (asserting it's a direct `Entry`, never
  an alias — the upstream invariant).
- `add` now pushes `EntryOrAlias::Entry(Entry { face, fallback })` (unchanged
  signature/return).
- `AddError` gains an `InvalidAliasTarget` variant.
- `add_alias(&mut self, style: Style, target: Index) -> Result<Index, AddError>`:
  validate the target is a **direct `Entry`** by inspecting
  `faces[target.style()][target.idx()]` **directly** (not via `get_entry`, which
  follows aliases) — a special / out-of-bounds / `Alias` target →
  `InvalidAliasTarget`. This preserves upstream's invariant that an alias points
  only to a real entry, so the one-step `entry_of` can never hit an alias. Then
  the `CollectionFull` guard, push `EntryOrAlias::Alias(target)`, and return the
  new `Index`.
- `get_entry`: unchanged guards (special → `SpecialHasNoFace`, bounds →
  `IndexOutOfBounds`), then `Ok(self.entry_of(&list[i]))`.
- `get_index` / `has_codepoint`: resolve each list element through `entry_of`
  before calling `has_codepoint`, so aliases participate transparently.

### Scope / faithfulness notes

- This is the **storage + resolution** for aliases. `completeStyles` (which
  decides _when_ to alias vs. synthesize) and synthetic italic are the next
  experiments.
- Aliasing by `Index` is a documented Rust adaptation of upstream's `*Entry`
  (behaviorally identical: an alias resolves to the same target entry).
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/collection.rs`:
   - Add `EntryOrAlias`; change `faces` to `[Vec<EntryOrAlias>; 4]`.
   - Add `entry_of`; update `add`, `get_entry`, `get_index`, `has_codepoint`;
     add `add_alias`.
2. Tests in `collection.rs` (live CoreText, macOS):
   - `alias_resolves_to_target`: add Menlo `Regular` (idx 0);
     `add_alias(Italic, {Regular,0})` returns `{Italic,0}`;
     `get_face({Italic,0})` is the Menlo face (`!has_color()`);
     `get_entry({Italic,0})` returns the regular entry (its `fallback` flag),
     and `has_codepoint({Italic,0}, 'M', Any)` is true.
   - `get_index_follows_alias`: with the alias above,
     `get_index('M', Italic, Any)` is `Some({Italic,0})` (the alias position),
     and `get_index('M', Bold, Any)` is `None` (no bold entry/alias).
   - `add_alias_rejects_bad_target`: `add_alias(Italic, {Regular,0})` on an
     empty collection → `Err(InvalidAliasTarget)` (no such entry); and an alias
     whose target is itself an alias is also rejected (the target must be a
     direct `Entry`), pinning the invariant that protects `entry_of`.
   - the existing `add`/`get`/resolution tests still pass (the `Entry` arm is
     unchanged behavior).
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

- the per-style lists hold `EntryOrAlias`, `add` pushes an `Entry` and
  `add_alias` pushes an `Alias(Index)` with the `CollectionFull` guard;
- `entry_of`/`get_entry`/`get_index`/`has_codepoint` transparently follow
  aliases to the target entry;
- a live alias resolves to its target face and answers codepoint queries through
  it; a bad alias target is rejected;
- `completeStyles` and synthetic italic are cleanly deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the alias-by-`Index` adaptation needs a
different shape (e.g. a borrow issue in `entry_of`).

The experiment **fails** if alias resolution diverges from upstream's `*Entry`
behavior or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and raised a **Medium**
finding: validating `add_alias`'s target via `get_entry` would be insufficient
because `get_entry` _follows_ aliases, so an alias-to-alias target would
validate yet break the one-step `entry_of` (panic). The design was revised so
`add_alias` inspects `faces[target.style()][target.idx()]` **directly** and
accepts only a direct `EntryOrAlias::Entry(_)`, rejecting special /
out-of-bounds / alias targets with a new `AddError::InvalidAliasTarget`. Codex's
re-review confirmed this fully resolves the finding — no alias-to-alias can be
constructed, so the one-step `entry_of` is safe — and approved the design.

Review artifacts:

- Prompts: `logs/codex-review/20260602-220501-861624-prompt.md`,
  `…-220554-984710-prompt.md`
- Results: `logs/codex-review/20260602-220501-861624-last-message.md`,
  `…-220554-984710-last-message.md`

## Result

**Result:** Pass

`collection.rs` now stores `[Vec<EntryOrAlias>; 4]` (`EntryOrAlias::Entry` |
`Alias(Index)`). The private `entry_of` resolver follows an alias one step to
its target `Entry`; `get_entry`/`get_index`/`has_codepoint` route through it so
aliases participate transparently. `add` pushes an `Entry`; `add_alias`
validates the target is a **direct** entry by inspecting `faces[target][...]`
directly (rejecting special / out-of-bounds / alias targets with the new
`AddError::InvalidAliasTarget`), then pushes the `Alias` with the
`CollectionFull` guard. (`entry_of` needed an explicit shared lifetime `'a`
since the alias arm returns from `self`.)

Tests (live CoreText):

- `alias_resolves_to_target` — an `Italic` alias to `{Regular,0}` resolves to
  the Menlo face/entry and answers `has_codepoint('M', Any)`.
- `get_index_follows_alias` — `get_index('M', Italic, Any)` returns the alias
  position `{Italic,0}`; `Bold` (no entry/alias) is `None`.
- `add_alias_rejects_bad_target` — a nonexistent target, an alias target, and a
  special target are all rejected with `InvalidAliasTarget`.
- The existing `add`/`get`/resolution tests still pass unchanged.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty collection` → 19 passed, 0 failed.
- `cargo test -p roastty` → 2402 passed, 0 failed (no regressions; +3).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The Collection's per-style lists now hold entries or aliases, with transparent
alias resolution through the whole query path. This is the storage
`completeStyles` needs. The next experiment ports `completeStyles` itself: when
a style has no entry, alias it to the first regular text face (or, where
supported, synthesize — synthetic bold is already ported as
`Face::new_synthetic_bold`; synthetic italic needs a CoreText oblique-matrix
face, a small new FFI piece). Above that remain the per-entry `scale_factor` +
`load_options`/`setSize` normalization, the `DeferredFace` + `discovery`
lazy-loading sub-area, the `CodepointResolver`, the shaper, and the Nerd Font
attribute table.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-220915-750152-prompt.md`
- Result: `logs/codex-review/20260602-220915-750152-last-message.md`

Codex confirmed `add_alias` enforces the direct-entry invariant (inspecting
`faces[target][...]` directly and rejecting special / out-of-bounds / alias
targets), so `entry_of`'s one-step resolution can't recurse into an alias; that
`get_entry`/`get_index`/`has_codepoint` resolve aliases consistently with
`get_index` returning the alias slot's `Index` (not the target's); that the
shared lifetime on `entry_of` is sound for the immutable borrows (including
during `get_index` iteration); and that the tests cover live resolution,
alias-position lookup, and the nonexistent / alias / special target rejections.
