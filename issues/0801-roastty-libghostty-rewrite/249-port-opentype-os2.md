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

# Experiment 249: Port the OpenType `os2` table parser (version-gated)

## Description

Complete the OpenType metric-table parsers that `Face::getMetrics` reads by
porting **`os2`** (OS/2 and Windows Metrics) — the largest and only
version-gated table. `getMetrics` reads its typographic metrics
(`sTypoAscender`/ `sTypoDescender`/`sTypoLineGap`, preferred when the
`fsSelection` USE_TYPO_METRICS bit is set), the Windows ascent/descent fallback
(`usWinAscent`/`usWinDescent`), and the cap/ex heights (`sCapHeight`/`sxHeight`,
present only in v2+). With `os2` in place, all four tables `getMetrics` parses
(`head`/`hhea`/`post`/`os2`) exist, unblocking the CoreText `Face` FFI (next
experiment).

Still no FFI, no rasterization — pure fixed-layout parsing with one novel
mechanism (version gating) and deterministic tests.

### Upstream shape (`font/opentype/os2.zig`)

Upstream defines four `extern struct`s (`OS2v0`, `OS2v1`, `OS2v4_3_2`, `OS2v5`)
and a unified `OS2` struct whose version-specific trailing fields are optional
(`?T = null`). `init(data)` reads the leading `version` `u16`, then reads the
struct for that version (`error{ EndOfStream, OS2VersionNotSupported }`),
filling the optionals. The supported versions are **0–5**; any other version is
`OS2VersionNotSupported`.

The fields are a fixed common block (present in **all** versions) followed by
version-gated trailing blocks:

- **v0 common block (78 bytes):** `version` (u16), `xAvgCharWidth` (i16),
  `usWeightClass`/`usWidthClass`/`fsType` (u16), the eight sub/superscript
  fields (i16), `yStrikeoutSize`/`yStrikeoutPosition` (i16), `sFamilyClass`
  (i16), `panose` (`[10]u8`), `ulUnicodeRange1..4` (u32), `achVendID` (`Tag` =
  `[4]u8`), `fsSelection` (`FSSelection`, a `u16` bitfield), `usFirstCharIndex`/
  `usLastCharIndex` (u16), `sTypoAscender`/`sTypoDescender`/`sTypoLineGap`
  (i16), `usWinAscent`/`usWinDescent` (u16).
- **v1+ adds:** `ulCodePageRange1`/`ulCodePageRange2` (u32).
- **v2/3/4 add:** `sxHeight`/`sCapHeight` (i16), `usDefaultChar`/`usBreakChar`/
  `usMaxContext` (u16).
- **v5 adds:** `usLowerOpticalPointSize`/`usUpperOpticalPointSize` (u16).

`FSSelection` is `packed struct(u16)` with ten named bool flags from the
least-significant bit — `italic` (bit 0), `underscore`, `negative`, `outlined`,
`strikeout`, `bold`, `regular`, `use_typo_metrics` (bit 7), `wws`, `oblique`
(bit 9) — plus 6 reserved bits.

### Rust mapping

- `opentype/sfnt.rs`:
  - add `OpenTypeError::UnsupportedVersion` (the analog of upstream
    `OS2VersionNotSupported`; named generically so it can be reused);
  - add `Reader::read_u8(&mut self) -> Result<u8, _>` and
    `Reader::read_bytes<const N: usize>(&mut self) -> Result<[u8; N], _>` (for
    `panose` `[10]` and `achVendID` `[4]`), both via the existing bounds-checked
    `take`.
- `opentype/os2.rs` (new):
  - `pub(crate) struct FsSelection(pub u16)`
    (`Debug, Clone, Copy, PartialEq, Eq`) with the ten named bit accessors
    (`italic()`, `underscore()`, `negative()`, `outlined()`, `strikeout()`,
    `bold()`, `regular()`, `use_typo_metrics()`, `wws()`, `oblique()`), each
    `self.0 & (1 << bit) != 0`.
  - `pub(crate) struct Os2 { … 30 common fields, then the version-gated optionals as `Option<…>` … }`
    (`Debug, Clone, Copy, PartialEq, Eq`). Field names are `snake_case` of the
    spec (`x_avg_char_width`, `s_typo_ascender`, `us_win_ascent`, `sx_height`,
    `s_cap_height`, `ul_code_page_range1`, etc.); `panose: [u8; 10]`,
    `ach_vend_id: [u8; 4]`, `fs_selection: FsSelection`.
  - `pub(crate) fn from_bytes(data: &[u8]) -> Result<Os2, OpenTypeError>`: a
    **single** version-gated reader (idiomatic Rust replacing upstream's four
    structs + normalize, with identical field values): read `version`; if
    `version > 5` → `Err(UnsupportedVersion)`; read the v0 common block in spec
    order; then `if version >= 1` read the code-page ranges (else `None`);
    `if version >= 2` read `sx_height`/`s_cap_height`/`us_default_char`/
    `us_break_char`/`us_max_context` (else `None`); `if version >= 5` read the
    two optical sizes (else `None`). Trailing optional reads use the `Reader`,
    so a truncated table of a claimed version returns `EndOfStream`.
- `opentype/mod.rs`: add `pub(crate) mod os2;`; update the module doc.

### Faithfulness and scope notes

- A single version-gated `from_bytes` is used instead of four separate
  `extern struct`s + a normalize step; it reads the same bytes in the same order
  and yields the same field values and the same `Option` presence per version
  (the issue permits idiomatic Rust where it is a clearer representation). The
  block boundaries (v0 → 78 bytes, +codepage v1, +sxHeight block v2, +optical
  v5) exactly match the upstream per-version struct sizes.
- `UnsupportedVersion` maps upstream `OS2VersionNotSupported`; versions 0–5 are
  supported, 6+ rejected.
- `FsSelection` reproduces the `packed struct(u16)` bit order (bit 0 = `italic`,
  bit 7 = `use_typo_metrics`).
- Upstream's `os2` test parses the embedded `julia_mono`; this slice uses
  programmatically-built fixtures (an equivalent Roastty test per Test Parity).
- No CoreText FFI, no rasterization.
- No C ABI, header, or ABI inventory changes; no new dependencies (std only).

## Changes

1. `roastty/src/font/opentype/sfnt.rs`: add `OpenTypeError::UnsupportedVersion`,
   `Reader::read_u8`, `Reader::read_bytes<N>`.
2. `roastty/src/font/opentype/os2.rs` (new): `FsSelection`, `Os2`, `from_bytes`.
3. `roastty/src/font/opentype/mod.rs`: add `pub(crate) mod os2;` and update the
   doc.
4. Tests in `os2.rs` (fixtures built programmatically by appending big-endian
   fields, to avoid hand-miscounting ~80–100 bytes):
   - `fs_selection_bits`: `FsSelection(1 << 7).use_typo_metrics()` is true and
     the others false; `FsSelection(1 << 0).italic()` true; a combined value.
   - `parse_os2_v4`: a v4 table → the common fields equal (incl.
     `s_typo_ascender`, `us_win_ascent`, `fs_selection` with USE_TYPO_METRICS),
     `ul_code_page_range*` and `sx_height`/`s_cap_height` are `Some(...)`, and
     the v5 optical sizes are `None`.
   - `parse_os2_v0`: a v0 table → common fields equal, **all** optionals `None`.
   - `parse_os2_v5`: a v5 table → optical sizes are `Some(...)`.
   - `os2_unsupported_version`: `version = 6` → `Err(UnsupportedVersion)`.
   - `os2_truncated`: a v4-claimed table cut short in the trailing block →
     `Err(EndOfStream)`.

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

- `Os2::from_bytes` reads the v0 common block then the version-gated trailing
  blocks in spec order, setting the right `Option` fields per version (0–5), and
  rejects version > 5 with `UnsupportedVersion`;
- `FsSelection` exposes the ten flags at the correct bit positions
  (`use_typo_metrics` = bit 7);
- the v0/v4/v5 fixtures parse to the expected fields and optional presence,
  unsupported-version and truncation error correctly;
- the CoreText FFI is cleanly deferred;
- no C ABI, header, or ABI inventory changes;
- `cargo fmt` accepted and `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the CoreText `getMetrics` port reveals an `os2`
field it needs typed differently.

The experiment **fails** if a field is read at the wrong
offset/width/endianness, if the version gating includes/excludes the wrong
trailing block, if `FsSelection` bits are misplaced, if version > 5 is not
rejected, or if any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no required
changes**.

Review artifacts:

- Prompt: `logs/codex-review/20260602-195307-324023-prompt.md`
- Result: `logs/codex-review/20260602-195307-324023-last-message.md`

Codex confirmed the v0 common block field order/widths match upstream `OS2v0`
(78 bytes), the version-gated tails match upstream sizes/thresholds (v1+
code-page ranges, v2+ `sxHeight`/`sCapHeight`/default/break/max-context, v5
optical sizes), and that versions 0–5 are supported with 6+ →
`UnsupportedVersion` (faithful to `OS2VersionNotSupported`). It confirmed the
`FsSelection` bit order (`italic` bit 0, `use_typo_metrics` bit 7, `oblique`
bit 9) and that `getMetrics` needs the typo/win metrics,
`fsSelection.use_typo_metrics`, the optional v2+ cap/ex heights, and the
strikeout size/position (in the v0 common block). `Reader::read_bytes<N>` /
`read_u8` were endorsed as the right additions.
