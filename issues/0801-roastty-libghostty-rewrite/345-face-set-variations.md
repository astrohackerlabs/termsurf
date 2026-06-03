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

# Experiment 345: applying variations to a face

## Description

roastty already models a font-variation axis setting (`discovery::Variation`)
and carries a `Descriptor.variations` list, but **applying** those variations to
a constructed face is deferred (see `discover_faces`' doc comment: "Applying the
requested variations to the face is deferred"). This experiment ports upstream's
`Face.setVariations` — building a varied `CTFont` by copying the font descriptor
with each axis set — and wires it into discovery so a descriptor's requested
variations are applied to the faces it produces, matching upstream's
`DeferredFace` resolution (which calls `setVariations(variations, opts)`).

## Upstream behavior (`face/coretext.zig` `Face.setVariations`)

```zig
pub fn setVariations(self: *Face, vs: []const font.face.Variation, opts: …) !void {
    if (vs.len == 0) return;                       // nothing to do
    var desc = self.font.copyDescriptor();         // CTFontCopyFontDescriptor
    defer desc.release();
    for (vs) |v| {
        const id = try macos.foundation.Number.create(.int, @ptrCast(&v.id));
        defer id.release();
        const next = try desc.createCopyWithVariation(id, v.value);  // per axis
        desc.release();
        desc = next;
    }
    const ct_font = try self.font.copyWithAttributes(0, null, desc);  // rebuild
    const face = try initFont(ct_font, …);
    self.deinit();
    self.* = face;                                 // replace in place
}
```

Each variation's `id` (a four-character axis tag packed into a `u32`, e.g.
`wght` → `2003265652`) becomes a `CFNumber`;
`CTFontDescriptorCreateCopyWithVariation` folds it into a fresh descriptor; the
loop chains them. The font is then rebuilt from the final descriptor (`size 0`
preserves the current size), and the face is re-initialized so all derived state
is recomputed.

Variations are applied at face resolution time: `DeferredFace` calls
`face.setVariations(ct.variations, opts)` with the discovery descriptor's
requested variations.

## Rust mapping (`roastty/src/font/face/coretext.rs`, `discovery.rs`)

- `Face::set_variations(&mut self, vs: &[Variation])`:
  ```rust
  if vs.is_empty() {
      return;
  }
  // SAFETY: `self.font` is a live `CTFont`.
  let mut desc = unsafe { self.font.font_descriptor() };
  for v in vs {
      let id = CFNumber::new_i32(v.id as i32);   // u32 tag bits as the int id
      // SAFETY: `desc`/`id` are live; the call returns a retained descriptor.
      desc = unsafe { desc.copy_with_variation(&id, v.value) };
  }
  // SAFETY: `self.font`/`desc` are live; a null matrix is valid; size `0.0`
  // preserves the current size.
  let font = unsafe {
      self.font
          .copy_with_attributes(0.0, std::ptr::null(), Some(&desc))
  };
  let synthetic_bold = self.synthetic_bold;
  *self = Face::from_ct_font(font);
  self.synthetic_bold = synthetic_bold;
  ```
  (`v.id as i32` reinterprets the packed tag's bits as the signed int CoreText
  expects — matching upstream's `@ptrCast(&v.id)` to a C `int`. `from_ct_font`
  recomputes the color state, mirroring upstream's full re-init.)
- `discovery.rs`: apply the requested variations to **every** face discovery
  resolves — both `discover_faces` and `discover_fallback_faces` — matching
  upstream, which applies variations at deferred-face resolution (not just one
  iterator). Add a small helper and use it at each face-producing site:
  ```rust
  /// Apply this descriptor's requested variations to a resolved face.
  fn apply_variations(&self, mut face: Face) -> Face {
      face.set_variations(&self.variations);
      face
  }
  ```

  - `discover_faces` (keep it lazy; clone the variations so the returned
    iterator does not borrow `self`):
    ```rust
    let variations = self.variations.clone();
    self.discover_descriptors().into_iter().map(move |d| {
        let mut face = deferred_face(d);
        face.set_variations(&variations);
        face
    })
    ```
    and drop the "deferred" note from its doc comment.
  - `discover_fallback_faces` (a `Vec` context, so `self.apply_variations(..)`
    is used directly): apply at each of the three return paths — the CJK
    `font_for_codepoint` early return, the empty-descriptors
    `font_for_codepoint` fallback, and the general `deferred_face` map.

## Scope / faithfulness notes

- **Ported**: `Face.setVariations` — the per-axis descriptor copy chain and the
  font rebuild — and its application in discovery (`discover_faces`), matching
  upstream's `DeferredFace`-time variation application.
- **Faithful**: `size 0.0` preserves the source size (as upstream's
  `copyWithAttributes(0, …)`); `from_ct_font` re-derives the color state
  (upstream re-runs `initFont`); `synthetic_bold` is preserved across the
  rebuild (a no-op in the discovery path, where it is unset, but correct for any
  caller).
- **Deferred** (unchanged): the variation-axis **`score()`** refinement
  (deriving bold/italic from a variable font's `wght`/`slnt` axes during
  discovery scoring); the special-font fast path; the `Shaper` struct +
  `RunIterator`. The `score()` refinement is a separate, later experiment.
- No C ABI/header/ABI-inventory change (`set_variations` is internal
  `pub(crate)` Rust).

## Changes

1. `roastty/src/font/face/coretext.rs`: add `Face::set_variations`; import
   `CFNumber` and `Variation`.
2. `roastty/src/font/discovery.rs`: add `Descriptor::apply_variations`; apply
   the requested variations in **both** `discover_faces` and
   `discover_fallback_faces` (all three return paths); update the
   `discover_faces` doc comment.
3. Tests:
   - `set_variations_empty_noop` (in `coretext.rs`): `set_variations(&[])`
     leaves the face usable and unchanged — `glyph_index('A')` is still `Some`
     and equals its pre-call value. Deterministic.
   - `set_variations_runs_on_face` (in `coretext.rs`):
     `set_variations(&[wght = 700])` on Menlo (a non-variable font) does not
     crash and yields a usable face — `glyph_index('A')` is still `Some`.
     Exercises the descriptor-copy and font-rebuild path end to end. (CoreText
     returns a valid copy even when the font has no matching axis.)
     Deterministic — Menlo is always present.
   - `discover_faces_applies_variations` (in `discovery.rs`): a `Descriptor`
     with `family = "Menlo"` and `variations = [wght = 700]` yields faces from
     `discover_faces` that are usable (`glyph_index('M')` is `Some`), confirming
     the wiring calls `set_variations` without breaking face production. (A
     smoke test: since the host's Menlo match is non-variable, the variation
     produces no observable change, so this asserts only that face production
     survives the `set_variations` call — the axis semantics are covered by
     faithfulness to upstream's CoreText calls and the design review.)
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty variation
cargo test -p roastty set_variations
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Face::set_variations` builds a varied `CTFont` via the per-axis descriptor
  copy chain and rebuilds the face (preserving size and re-deriving state),
  faithful to upstream's `setVariations`;
- `discover_faces` applies the descriptor's requested variations to each face;
- the empty-no-op, runs-on-face, and discovery-wiring tests pass, and the
  existing tests still pass;
- the variation-axis `score()` refinement, the special-font path, and the
  `Shaper`/`RunIterator` stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if no reliably-present variable font is available
to prove an actual axis change at runtime (the code path is still exercised on a
non-variable font, and faithfulness to upstream's CoreText calls is verified).

The experiment **fails** if the descriptor-copy chain or the rebuild diverges
from upstream (wrong id encoding, not preserving size, dropping derived state),
the discovery wiring is incorrect, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **one Required
finding**, now fixed:

- **Required (fixed):** the integration was incomplete — applying variations
  only in `discover_faces` would leave `discover_fallback_faces`-produced faces
  unvaried, but upstream applies variations at deferred-face resolution
  generally (not one iterator). The design now applies the variations to
  **every** face both discovery functions resolve: `discover_fallback_faces`
  applies them at all three return paths (the CJK early return, the
  empty-descriptors fallback, and the general map), via a shared
  `Descriptor::apply_variations` helper.

Codex confirmed the rest is faithful: the per-axis descriptor copy chain plus
`copy_with_attributes(0.0, null, Some(&desc))` matches `setVariations` (`0.0`
preserves the size; `from_ct_font` re-derives color like upstream's `initFont`);
`v.id as i32` into `CFNumber::new_i32` is the correct id encoding (it preserves
the packed 32-bit tag pattern as CoreText's signed `int`, high-bit tags becoming
negative `i32` with the same bits — matching upstream's bitcast path); the CF
lifetimes are sound (the `CFRetained` reassignment drops the prior descriptor,
`id` lives through each copy, `Some(&desc)` is only borrowed during the
rebuild); and preserving `synthetic_bold` is a harmless local improvement (unset
in the discovery path). On testing: the deterministic tests are adequate smoke
tests but do not prove the integration call's effect on a non-variable font; the
limitation is documented (a real variable-axis test would need a stable system
variable font), and `set_variations_runs_on_face` still exercises the CoreText
path directly.

Review artifacts:

- Prompt: `logs/codex-review/20260603-135038-953972-prompt.md` (design)
- Result: `logs/codex-review/20260603-135038-953972-last-message.md` (design)
