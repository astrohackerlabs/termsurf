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

# Experiment 248: Port the OpenType `post` table parser (+ `Version16Dot16`)

## Description

Continue the OpenType metric-table parsers (Exp 247 ported `head`/`hhea`) with
**`post`** (the PostScript table), which gives `Face::getMetrics` its underline
position and thickness. `post` also introduces the shared **`Version16Dot16`**
scalar type. It is a small, fixed-layout slice like `head`/`hhea` — no FFI, no
macOS-version sensitivity, deterministic hand-built tests. The larger,
version-gated **`os2`** table is the next slice; the CoreText `Face` FFI that
feeds all four parsers comes after that.

### Upstream shape (`font/opentype/post.zig`, 32 bytes)

`post` is an `extern struct`, `align(1)`, read big-endian via
`readStructEndian(Post, .big)` (`init(data) → error{EndOfStream}!Post`).
Upstream's port deliberately stops at the v1.0 header and does **not** parse the
v2.0/v2.5 extra glyph-name fields:

| field                | type             | bytes |
| -------------------- | ---------------- | ----: |
| `version`            | `Version16Dot16` |     4 |
| `italicAngle`        | `Fixed` (16.16)  |     4 |
| `underlinePosition`  | `FWORD` (i16)    |     2 |
| `underlineThickness` | `FWORD` (i16)    |     2 |
| `isFixedPitch`       | `uint32`         |     4 |
| `minMemType42`       | `uint32`         |     4 |
| `maxMemType42`       | `uint32`         |     4 |
| `minMemType1`        | `uint32`         |     4 |
| `maxMemType1`        | `uint32`         |     4 |

`getMetrics` reads only `underlinePosition` and `underlineThickness` (with a
broken-underline check on `underlineThickness == 0`), but the whole 32-byte
header is parsed for fidelity.

### `Version16Dot16` (`sfnt.zig`)

Upstream: `Version16Dot16 = packed struct(u32) { minor: u16, major: u16 }`. In a
Zig `packed struct(u32)`, fields fill from the least-significant bit, so `minor`
is the low 16 bits and `major` the high 16. Read big-endian, the four bytes form
the `u32`; e.g. version 2.0 is `0x0002_0000` → `major = 2`, `minor = 0`.

### Rust mapping (in `roastty/src/font/opentype/`)

- `opentype/sfnt.rs`: add
  `pub(crate) struct Version16Dot16 { pub major: u16, pub minor: u16 }`
  (`Debug, Clone, Copy, PartialEq, Eq`) with
  `pub(crate) fn from_u32(raw: u32) -> Version16Dot16`
  (`major = (raw >> 16) as u16`, `minor = (raw & 0xFFFF) as u16`) — matching the
  packed-struct bit layout. (A `Reader::read_version16dot16` convenience is
  **not** added; `post` reads a `u32` and calls `from_u32`, keeping `Reader`
  minimal.)
- `opentype/post.rs` (new): `pub(crate) struct Post { … 9 fields … }`
  (`Debug, Clone, Copy, PartialEq, Eq`; `version: Version16Dot16`,
  `italic_angle: Fixed`, `underline_position`/`underline_thickness: i16`, the
  four `mem*` + `is_fixed_pitch` as `u32`) and
  `pub(crate) fn from_bytes(data: &[u8]) -> Result<Post, OpenTypeError>` reading
  the 32-byte header in spec order (`version` via `read_u32` →
  `Version16Dot16::from_u32`).
- `opentype/mod.rs`: add `pub(crate) mod post;`.

### Faithfulness and scope notes

- Like `head`/`hhea`, reading is field-by-field big-endian via the existing
  `Reader`; only the v1.0 32-byte header is parsed (matching upstream's
  documented decision to skip the v2.0/v2.5 glyph-name arrays).
- `Version16Dot16::from_u32` reproduces the packed-struct `{ minor, major }` bit
  layout exactly (`major` = high 16 bits).
- Upstream's `post` test parses the embedded `julia_mono`; this slice uses a
  hand-built 32-byte fixture (equivalent Roastty test per Test Parity).
- No `os2`, no CoreText FFI, no rasterization.
- No C ABI, header, or ABI inventory changes; no new dependencies (std only).

## Changes

1. `roastty/src/font/opentype/sfnt.rs`: add `Version16Dot16` (+`from_u32`).
2. `roastty/src/font/opentype/post.rs` (new): `Post` + `from_bytes`.
3. `roastty/src/font/opentype/mod.rs`: add `pub(crate) mod post;`.
4. Tests:
   - `sfnt`: `version16dot16_layout` — `from_u32(0x0002_0000)` is
     `{ major: 2, minor: 0 }`; `from_u32(0x0001_0005)` is
     `{ major: 1, minor: 5 }`.
   - `post`: `parse_post` — a hand-built 32-byte big-endian `post`
     (`version 2.0`, `italic_angle = 0.0`, `underline_position = -200`,
     `underline_thickness = 100`, `is_fixed_pitch = 1`, the four `mem*` = 0) →
     all fields equal; `post_truncated` (31 bytes → `Err(EndOfStream)`).

5. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo test -p roastty opentype
cargo test -p roastty
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Version16Dot16::from_u32` splits the `u32` into `major` (high 16) / `minor`
  (low 16) matching the packed-struct layout;
- `Post` parses its 32-byte v1.0 header field-by-field big-endian in spec order,
  matching the fixture, and errors on truncation;
- `os2` and the CoreText FFI are cleanly deferred;
- no C ABI, header, or ABI inventory changes;
- `cargo fmt` accepted and `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if `os2` or the CoreText path forces a
representation change on `Version16Dot16` or `Post`.

The experiment **fails** if `Version16Dot16` swaps `major`/`minor`, if a `post`
field is read at the wrong offset/width/endianness, if truncation does not
return `EndOfStream`, or if any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-194638-559251-prompt.md`
- Result: `logs/codex-review/20260602-194638-559251-last-message.md`

Codex confirmed the `post` layout (9 fields, correct order, 32 bytes total:
`Version16Dot16` 4, `Fixed` 4, two `i16` underline fields, five `u32`), that
parsing only the fixed v1.0 header is faithful (upstream skips the v2.0/v2.5
glyph-name payloads), that the `Version16Dot16` mapping is correct (Zig's
`packed struct(u32) { minor, major }` puts `minor` in the low 16 bits, so
`major = raw >> 16`, `minor = raw & 0xFFFF`; bytes `00 02 00 00` → version 2.0),
and that the fixture bytes are correctly encoded (`italic_angle 0.0`,
`underline_position -200 = 0xFF38`, `underline_thickness 100 = 0x0064`, version
`0x00020000`). Deferring `os2`/CoreText is cleanly scoped.
