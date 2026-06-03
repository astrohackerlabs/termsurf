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

# Experiment 272: CodepointResolver core — getIndex with style/regular fallback

## Description

The `CodepointResolver` sits on top of the `Collection` and resolves a codepoint
to a face index, layering in style-disabled fallback, presentation defaults,
sprite glyphs, codepoint overrides, and discovery-based fallback. This
experiment ports its **core resolution chain** (`font/CodepointResolver.zig`
`getIndex`, lines 120–228) over the now-complete `Collection`: the
style-disabled → regular fallback, the explicit/default presentation mode, the
collection lookup, the non-regular → regular retry, and the final regular/any
fallback. The sprite face, codepoint overrides, the UCD emoji-presentation
default, and discovery are **deferred** (each its own sub-area).

## Upstream behavior (`font/CodepointResolver.zig` `getIndex`)

1. If `style != regular` and that style is **disabled** (`self.styles`), recurse
   as `regular` (lines 127–130).
2. Codepoint overrides → sprite face check (lines 132–145) — **deferred**.
3. Build the presentation mode (lines 152–157): `explicit(p)` if a presentation
   is given, else `default(<emoji if UCD says so, else text>)` — the **UCD
   emoji-presentation default is deferred** (default to `text`).
4. `collection.getIndex(cp, style, p_mode)` — return it if found (line 160).
5. If `style != regular`, retry as `regular` (lines 165–167).
6. Discovery-based fallback (lines 170–219) — **deferred**.
7. `if style == regular and p_mode == any: return null` (line 224 — effectively
   unreachable since `p_mode ∈ {explicit, default}`, but ported faithfully).
8. Fall back to `collection.getIndex(cp, regular, any)` (line 227).

## Rust mapping (`roastty/src/font/codepoint_resolver.rs`, new)

- `struct CodepointResolver { collection: Collection, styles: [bool; 4] }` —
  owns the `Collection`; `styles[Style as usize]` is each style's enabled flag
  (upstream's `StyleStatus = EnumArray(Style, bool)`), default all `true`.
- `new(collection) -> CodepointResolver` (all styles enabled);
  `collection()`/`collection_mut()` accessors (so faces can be added and
  `complete_styles`/`update_metrics` run); `set_style_enabled(style, bool)`.
- `get_index(&self, cp: u32, style: Style, p: Option<Presentation>) -> Option<Index>`:
  1. `if style != Regular && !self.styles[style as usize] { return self.get_index(cp, Regular, p); }`
  2. `let p_mode = match p { Some(v) => Explicit(v), None => Default(Text) };`
     (the UCD emoji-presentation default is deferred — `None` defaults to text.)
  3. `if let Some(idx) = self.collection.get_index(cp, style, p_mode) { return Some(idx); }`
  4. `if style != Regular { if let Some(idx) = self.get_index(cp, Regular, p) { return Some(idx); } }`
  5. `if style == Regular && p_mode == Any { return None; }` (faithful port of
     the unreachable guard).
  6. `self.collection.get_index(cp, Regular, PresentationMode::Any)`.

`get_index` is `&self` here (no allocation): the deferred
sprite/override/discovery paths are what made upstream's `getIndex` take
`*self`.

## Scope / faithfulness notes

- **Deferred**: the sprite face check, codepoint overrides
  (`getIndexCodepointOverride` + `CodepointMap`), the UCD emoji-presentation
  default (`uucode`), and discovery-based fallback (`DeferredFace` +
  `discovery`). Each is a later experiment; the core resolution chain over an
  eager `Collection` is faithful without them.
- No C ABI/header/ABI-inventory change.

## Changes

1. `roastty/src/font/codepoint_resolver.rs` (new): `CodepointResolver`, `new`,
   the accessors, `set_style_enabled`, and `get_index`.
2. `roastty/src/font/mod.rs`: `pub(crate) mod codepoint_resolver;`.
3. Tests (live CoreText, macOS):
   - `resolve_basic`: a resolver over a Menlo-`Regular` collection;
     `get_index('M', Regular, Some(Text))` and `get_index('M', Regular, None)`
     are `Some({Regular, 0})`.
   - `resolve_missing`: `get_index(0xE000, Regular, Some(Text))` is `None`
     (Menlo lacks it and discovery is deferred).
   - `resolve_emoji_via_regular_any`: a collection with Menlo `Regular` (idx 0)
     and Apple Color Emoji `Regular` (idx 1);
     `get_index(😀, Regular, Some(Text))` finds the emoji at `{Regular, 1}` only
     via the final regular/any fallback (Menlo lacks it, and the emoji is color
     not text, so the explicit `Text` lookup misses but the any fallback hits).
   - `resolve_style_disabled_falls_back`: a collection with Menlo `Regular` then
     `complete_styles(NO_SYNTHESIS)` (so `Bold` is an alias to regular); disable
     `Bold`; `get_index('M', Bold, Some(Text))` recurses to `Regular` and
     returns `{Regular, 0}`.
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty codepoint_resolver
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `get_index` resolves via the style-disabled → regular fallback, the
  explicit/default presentation, the collection lookup, the non-regular →
  regular retry, and the final regular/any fallback, matching upstream's core
  chain;
- a text codepoint resolves to a text face, an emoji resolves via the
  regular/any fallback, a missing codepoint is `None`, and a disabled style
  falls back to regular;
- the sprite/override/UCD/discovery paths are cleanly deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the recursion/fallback ordering needs a
different shape than upstream.

The experiment **fails** if the resolution chain diverges from upstream's core
(beyond the documented deferred paths) or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-225625-792382-prompt.md`
- Result: `logs/codex-review/20260602-225625-792382-last-message.md`

## Result

**Result:** Pass

`roastty/src/font/codepoint_resolver.rs` (new) holds `CodepointResolver` (owns a
`Collection`, `styles: [bool; 4]`) with `new`, `collection`/
`collection_mut`/`set_style_enabled`, and `get_index` — the faithful core chain:
style-disabled → regular recursion, `Explicit`/`Default(Text)` presentation,
collection lookup, non-regular → regular retry, the (dead) `p_mode == Any`
guard, and the final regular/any fallback.

Tests (live CoreText):

- `resolve_basic` — `'M'` resolves to `{Regular, 0}` under `Some(Text)` and
  `None`.
- `resolve_missing` — a Private-Use codepoint → `None`.
- `resolve_emoji_via_regular_any` — `😀` under `Some(Text)` is found at
  `{Regular, 1}` only via the final regular/any fallback (the explicit-text
  lookup misses the color glyph).
- `resolve_style_disabled_falls_back` — a disabled `Bold` recurses to `Regular`
  → `{Regular, 0}`.

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty codepoint_resolver` → 4 passed, 0 failed.
- `cargo test -p roastty` → 2429 passed, 0 failed (no regressions; +4).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The resolver's core resolution chain is ported over the eager `Collection`. The
remaining `CodepointResolver` pieces are its deferred dependencies: the **sprite
face** check (which needs the sprite font — box-drawing/braille rendering, the
z2d-adjacent sub-area), **codepoint overrides** (`CodepointMap`), the **UCD
emoji-presentation default** (the `uucode` Unicode tables), and
**discovery-based fallback** (`DeferredFace` + `discovery`). Alongside these sit
the **shaper**, the **Nerd Font attribute table**, and **SVG color detection**.

## Completion Review

Codex reviewed the completed implementation and result and found **no required
changes**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-225838-871798-prompt.md`
- Result: `logs/codex-review/20260602-225838-871798-last-message.md`

Codex confirmed `get_index` follows the scoped upstream order (disabled
non-regular styles recurse to `Regular`, the presentation mode is built before
the collection lookup, the exact lookup precedes the regular retry, and the
final `Regular/Any` fallback is reached for both regular and non-regular
misses), that the `style != Regular` guard prevents disabled-style recursion
from looping, and that the tests are meaningful (the emoji test forces an
explicit-text miss before the `Any` fallback finds the color face; the
style-disabled test would fail if the disabled `Bold` used the alias path
instead of recursing). The UCD `None` default remains a documented deferred
deviation.

Codex confirmed the core chain matches upstream for the scoped eager path
(disabled non-regular styles recurse to regular first, the exact
style/presentation lookup precedes the regular retry, the non-regular retry
restarts through regular, and the final `regular/Any` fallback is correct for
both regular and non-regular callers). It confirmed `None => Default(Text)` is a
documented partial deviation from upstream's UCD-driven default (the
emoji-presentation default is deferred, but the final `Any` fallback still gives
best-effort resolution), that porting the `p_mode == Any` guard as effectively
dead is faithful, and that `&self` is appropriate while the
sprite/override/discovery mutation paths are deferred.
