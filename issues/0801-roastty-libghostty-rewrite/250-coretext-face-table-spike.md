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

# Experiment 250: CoreText `Face` FFI spike — create a `CTFont` and copy a table

## Description

The first FFI-heavy slice of the font face path, and the de-risking spike for
the whole CoreText `Face`/shaper line. It wires up the `objc2` CoreText bindings
and proves the one mechanism `Face::getMetrics` depends on: create a `CTFont`
from a system font, copy a raw OpenType table out of it via `CTFontCopyTable`,
and feed the bytes to the already-ported table parser (`Head`, Exp 247).

Deliberately minimal: this experiment does **not** assemble the full
`FaceMetrics` (the os2-typo-vs-hhea-vs-win fallback chain) or rasterize glyphs —
those are the next slices. It de-risks the FFI (crate wiring, `CTFont` creation,
`CTFontCopyTable` → `CFData` → `&[u8]`, parser integration) so the larger
`getMetrics` and rasterization slices land on a proven foundation.

### Upstream pattern (`font/face/coretext.zig`)

`getMetrics` reads each table with:

```zig
const tag = macos.text.FontTableTag.init("head");
const data = ct_font.copyTable(tag) orelse break :head null; // CFData
defer data.release();
break :head opentype.Head.init(data.getPointer()[0..data.getLength()]);
```

with a `bhed` fallback for bitmap-only fonts (`head` and `bhed` are
byte-identical). The table tag is a four-char code (`'h' 'e' 'a' 'd'`).

### The objc2 API (verified, `objc2-core-text` 0.3.2 / `objc2-core-foundation`

0.3.2)

- `CTFont::with_name(name: &CFString, size: CGFloat, matrix: *const CGAffineTransform) -> CFRetained<CTFont>`
  (`unsafe`; pass `std::ptr::null()` for `matrix`; always returns a font — falls
  back to a default if the name is unknown).
- `CTFont::table(&self, table: CTFontTableTag, options: CTFontTableOptions) -> Option<CFRetained<CFData>>`
  (`unsafe`; the `CTFontCopyTable` wrapper). `CTFontTableTag` is a `u32`
  four-char code; `CTFontTableOptions(0)` = no options.
- `CFString::from_str(&str)`; `CFData::to_vec(&self) -> Vec<u8>` (safe copy).
- `CFRetained<T>` manages the CoreFoundation retain/release, so the `Face` can
  own the font and each table copy frees itself.

### Rust mapping

- `roastty/Cargo.toml`: add
  `objc2-core-text = { version = "0.3", default-features = false, features = ["CTFont"] }`
  and extend the `objc2-core-foundation` features (or add it) with
  `["CFString", "CFData", "CFBase", "alloc"]` (exact feature set finalized
  against `cargo build`). These are macOS frameworks; the crate is already
  macOS-only via `objc2-metal`.
- `roastty/src/font/face/mod.rs` (new): `pub(crate) mod coretext;` + a `Face`
  re-export and module doc.
- `roastty/src/font/face/coretext.rs` (new):
  - `pub(crate) struct Face { font: CFRetained<CTFont> }`.
  - `pub(crate) fn new(name: &str, size: f64) -> Face`: build a `CFString` from
    `name`, call `CTFont::with_name(&name, size, std::ptr::null())` inside an
    `unsafe` block, store the `CFRetained<CTFont>`.
  - `pub(crate) fn copy_table(&self, tag: &[u8; 4]) -> Option<Vec<u8>>`: compute
    the `CTFontTableTag` as `u32::from_be_bytes(*tag)`, call
    `self.font.table(tag, CTFontTableOptions(0))` (`unsafe`), and `to_vec()` the
    `CFData`. Returns `None` when the table is absent.
  - A short `// SAFETY:` note on each `unsafe` call (the args are valid; the
    font is a live `CFRetained`).
- `roastty/src/font/mod.rs`: add `pub(crate) mod face;`.

### Faithfulness and scope notes

- This mirrors upstream's `copyTable` + parser pattern exactly, one table at a
  time. The `head`/`bhed` fallback and the full `getMetrics` assembly are the
  next slice (this spike copies `head` directly).
- `copy_table` is generic over the tag (the building block `getMetrics` will
  call for `head`/`hhea`/`OS/2`/`post`).
- `objc2-core-graphics` is **not** added yet — it is only needed for glyph
  rasterization (a later slice). `with_name`'s `matrix` is passed as a null
  pointer, so `CGAffineTransform` need not be constructed.
- No C ABI, header, or ABI inventory changes.

## Changes

1. `roastty/Cargo.toml`: add `objc2-core-text` (CTFont) and the
   `objc2-core-foundation` features.
2. `roastty/src/font/mod.rs`: `pub(crate) mod face;`.
3. `roastty/src/font/face/mod.rs` (new): module + `coretext` submodule.
4. `roastty/src/font/face/coretext.rs` (new): `Face` with `new` + `copy_table`.
5. Tests in `coretext.rs` (run on macOS; the crate is macOS-only):
   - `face_copies_and_parses_head`: `Face::new("Menlo", 12.0)`;
     `copy_table(b"head")` is `Some`; `Head::from_bytes(&bytes)` parses; assert
     `head.magic_number == 0x5F0F_3CF5` (the constant in **every** valid `head`
     table — a font-version-independent correctness check) and
     `(16..=16384).contains(&head.units_per_em)` (the spec's valid range).
   - `missing_table_is_none`: `copy_table(b"ZZZZ")` (a tag no font has) is
     `None`.

6. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo test -p roastty face
cargo test -p roastty
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- the `objc2-core-text`/`objc2-core-foundation` crates build and `Face::new`
  creates a `CTFont`;
- `copy_table` returns the raw table bytes for a present table and `None` for an
  absent one, and the copied `head` bytes parse with the ported `Head` parser
  (`magic_number == 0x5F0F3CF5`, valid `units_per_em`);
- the full `getMetrics` assembly and rasterization are cleanly deferred;
- no C ABI, header, or ABI inventory changes;
- `cargo fmt` accepted and `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the objc2 CoreText API needs a different call
shape than expected (e.g. an explicit `CFString` retain or a different table
options value).

The experiment **fails** if the FFI cannot create a font or copy a table, if the
copied bytes do not parse, or if any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-200120-628474-prompt.md`
- Result: `logs/codex-review/20260602-200120-628474-last-message.md`

Codex confirmed the approach matches upstream's `copyTable` pattern (hold a
`CTFont`, `CTFontCopyTable` → copy `CFData` → parse), that
`u32::from_be_bytes(*b"head")` is the correct `FourCharCode` (`0x68656164`), and
that the memory model is safe as scoped (`Face` owns the `CFRetained<CTFont>`,
the `CFString` lives through `with_name`, `to_vec` makes an owned copy, the
`unsafe` blocks wrap only the two CoreText calls with valid args). It judged the
tests robust for a spike (`magic_number == 0x5F0F3CF5` and `units_per_em` range
are font-version-independent; creating a `CTFont` needs no GUI/main thread, and
CoreText returns a fallback font with a valid `head` table if the name is
unavailable). Deferring `getMetrics`/`bhed`/rasterization is cleanly scoped.
