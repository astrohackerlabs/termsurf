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

# Experiment 266: Collection completeStyles — alias missing styles

## Description

`completeStyles` ensures every style (regular, bold, italic, bold-italic) has at
least one face, so the terminal can always render bold/italic text. Upstream
either **synthesizes** a face (synthetic bold/italic) or, when synthesis is
disabled or unavailable, **aliases** the missing style to the regular face. This
experiment ports the **aliasing** path (`font/Collection.zig` lines 320–466) on
top of the `EntryOrAlias` storage from Experiment 265; the synthesis path
(synthetic italic needs a new CoreText oblique-matrix face) is the next
experiment, after which the synthetic-config branches are wired in.

### Upstream behavior (`font/Collection.zig` `completeStyles`)

1. If **every** style already has ≥ 1 entry, return — the common case (lines
   327–334).
2. Find the first **regular** face with text glyphs (`regular_entry`, lines
   339–372): if the regular list is empty, return (nothing to do); iterate
   regular entries and pick the first whose face has text — the heuristic
   `!face.hasColor() or face.glyphIndex('A') != null` (accept a normal text
   font, or a mixed font that at least has ASCII). If none qualifies →
   `error.DefaultUnavailable`.
3. For **italic**, **bold**, **bold-italic**: if the style's list is empty,
   append an entry. In the **synthesis-disabled / unavailable** path this is
   `.{ .alias = regular_entry }` (lines 382, 388, 406, 412, 430, 464). (When
   synthesis is enabled and succeeds, a synthetic `Entry` is appended instead —
   deferred here.)

### Rust mapping (`roastty/src/font/collection.rs`)

- `enum CompleteError { DefaultUnavailable }` (upstream's `Allocator.Error` has
  no Rust analog — `Vec` growth is infallible here).
- `complete_styles(&mut self) -> Result<(), CompleteError>`:
  1. if all four `faces[*]` are non-empty → `Ok(())`.
  2. find the regular text face index: if `faces[Regular]` is empty → `Ok(())`
     (nothing to alias to); else iterate `0..faces[Regular].len()` and, for each
     slot, compute its **canonical direct-entry `Index`** — for an
     `EntryOrAlias::Entry` it is `Index::new(Regular, i)`; for an
     `Alias(target)` it is `target` (guaranteed direct by `add_alias`'s
     invariant). This mirrors upstream's `entry_or_alias.getEntry()` resolving
     to the underlying `*Entry`, and ensures the captured index always names a
     **direct** entry (so the later `add_alias` accepts it). Resolve the face
     via `get_face(canonical)` and pick the first slot whose face has text —
     `!face.has_color() || face.glyph_index('A' as u32).is_some()`. If none
     qualifies → `Err(DefaultUnavailable)`. Capture `regular = canonical` (a
     `Copy` value, so the immutable borrow ends before the mutations).
  3. for each of `Italic`, `Bold`, `BoldItalic`: if `faces[style]` is empty,
     `add_alias(style, regular)` (synthesis deferred → always alias).

### Scope / faithfulness notes

- **Deferred**: the synthesis branches (synthetic bold/italic creation and the
  `FontSyntheticStyle` config that selects them, plus the bold-italic
  synthesize-from-bold/italic preference). With synthesis unavailable,
  upstream's own fallback is exactly this aliasing path, so the port is a
  faithful subset.
- The `regular`-face heuristic (`!has_color() || glyph_index('A')`) is ported
  exactly.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/collection.rs`: add `CompleteError` and `complete_styles`.
2. Tests in `collection.rs` (live CoreText, macOS):
   - `complete_styles_aliases_missing`: a collection with only Menlo `Regular`;
     after `complete_styles()` (→ `Ok`), `Italic`/`Bold`/`BoldItalic` each
     resolve `{style,0}` to the Menlo face, and
     `has_codepoint({Italic,0}, 'M', Any)` is true.
   - `complete_styles_noop_when_full`: Menlo added under all four styles;
     `complete_styles()` is `Ok` and adds nothing (each list stays length 1,
     verified via `get_index`/face identity).
   - `complete_styles_empty_is_ok`: an empty collection → `complete_styles()` is
     `Ok(())` and stays empty (no regular face to alias to).
   - `complete_styles_default_unavailable`: a collection whose only `Regular`
     face is Apple Color Emoji (color, and — asserted as a precondition —
     lacking a text `'A'`) → `Err(DefaultUnavailable)`.
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

- `complete_styles` no-ops when all styles are present, aliases the missing
  italic/bold/bold-italic to the first regular text face, returns `Ok` (doing
  nothing) when there's no regular face, and `DefaultUnavailable` when no
  regular face has text;
- the regular-text heuristic matches upstream
  (`!has_color() || glyph_index('A')`);
- the synthesis branches are cleanly deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the regular-face search needs a borrow shape
different than the capture-then-mutate plan.

The experiment **fails** if the completion/aliasing logic diverges from upstream
or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and raised one **Low** finding:
the regular-face search could capture a `Regular` **alias** slot's position,
which `add_alias` would then reject (it accepts only direct-entry targets),
whereas upstream's `entry_or_alias.getEntry()` resolves an alias to its
underlying entry. Although `complete_styles` itself never creates regular
aliases, the `add_alias` API can represent that state. The design was updated to
**canonicalize** each regular slot to its direct-entry `Index` during the search
(an `Entry` → its own index; an `Alias(target)` → `target`), matching upstream's
resolved-pointer behavior and guaranteeing the captured index names a direct
entry. No other findings.

Review artifacts:

- Prompt: `logs/codex-review/20260602-221209-626886-prompt.md`
- Result: `logs/codex-review/20260602-221209-626886-last-message.md`

## Result

**Result:** Pass

`collection.rs` gained `CompleteError::DefaultUnavailable` and
`complete_styles`. It no-ops when all four styles are populated; otherwise finds
the first regular slot whose canonical direct-entry face has text
(`!has_color() || glyph_index('A')`) and aliases the missing
`Italic`/`Bold`/`BoldItalic` to it. Empty regular → `Ok` (nothing to do); no
regular text face → `DefaultUnavailable`. The regular slot is canonicalized to
its direct-entry `Index` before aliasing.

Tests (live CoreText):

- `complete_styles_aliases_missing` — Menlo-only `Regular`; after completion
  each of `Italic`/`Bold`/`BoldItalic` resolves `{style,0}` to the Menlo face
  and answers `has_codepoint('M', Any)`.
- `complete_styles_noop_when_full` — all four styles populated → no list grows.
- `complete_styles_empty_is_ok` — empty collection stays empty, returns `Ok`.
- `complete_styles_default_unavailable` — an emoji-only `Regular` (asserted to
  lack a text `'A'`) → `DefaultUnavailable`.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty collection` → 23 passed, 0 failed.
- `cargo test -p roastty` → 2406 passed, 0 failed (no regressions; +4).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

`complete_styles` ensures every style is renderable via aliasing — the full
Collection style-completion behavior in the synthesis-disabled/unavailable case.
The next experiment adds the **synthesis** path: a `Face::synthetic_italic`
(CoreText oblique transform matrix) and an instance-method
`Face::synthetic_bold` that copy an existing face, plus the `FontSyntheticStyle`
config that selects synthesize-vs-alias (and the bold-italic
synthesize-from-bold/italic preference). After that, the Collection's remaining
work is the per-entry `scale_factor` + `load_options`/`setSize` size
normalization and `setSize`/`updateMetrics`; then the `DeferredFace` +
`discovery` lazy-loading sub-area, the `CodepointResolver`, the shaper, and the
Nerd Font attribute table.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-221509-580102-prompt.md`
- Result: `logs/codex-review/20260602-221509-580102-last-message.md`

Codex confirmed `complete_styles` matches upstream's aliasing path (all-present
no-op, empty-regular → `Ok`, no-text-face → `DefaultUnavailable`, the exact
`!has_color() || glyph_index('A')` heuristic), that the canonicalization fixes
the design-review risk (a regular alias slot is converted to its direct target
so `add_alias` gets a validated direct entry — `CollectionFull` can't occur for
empty lists and `InvalidAliasTarget` is ruled out, justifying the `expect`),
that the borrow handling is sound (immutable search before mutation), and that
the tests cover the meaningful branches.
