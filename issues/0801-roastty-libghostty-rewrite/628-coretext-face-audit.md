+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 628: CoreText Face audit

## Description

Verify and close the stale Issue 801 checklist item for CoreText `Face`
rasterization and face-metric extraction.

The README still says:

```markdown
- [ ] CoreText `Face` (rasterization + face-metric extraction) — missing
```

Current Roastty source and prior experiments already contain the relevant
implementation: CoreText `CTFont` wrapping, OpenType table access, scalar
metrics, glyph measurement, `Face::get_metrics`, glyph rasterization, and atlas
`render_glyph`. This experiment should not change behavior unless the
verification finds a real gap. It should make one doc-only source edit: update
the stale module comment in `roastty/src/font/face/coretext.rs` that still says
metric assembly and glyph rasterization land in later experiments. If the gates
pass, update the checklist line to:

```markdown
- [x] CoreText `Face` (rasterization + face-metric extraction)
```

This does not close the adjacent `Shaper` checklist item even though
`coretext.rs` also contains shaping methods. That is a separate line and should
get its own audit or implementation experiment.

## Current implementation surface

- `roastty/src/font/face/coretext.rs` — defines `Face`, wraps CoreText `CTFont`,
  copies font tables, exposes scalar metrics and glyph measurement, implements
  `get_metrics`, rasterizes glyphs into coverage bitmaps, and renders glyphs
  into `Atlas` entries. Its module comment is stale and should be updated to
  describe the current implemented surface.
- `roastty/src/font/opentype/` — contains the `head`, `hhea`, `os2`, `post`, and
  `sfnt` parsers used by `Face::get_metrics`.
- `roastty/src/font/atlas.rs` and `roastty/src/font/glyph.rs` — provide the
  atlas and returned glyph value used by `render_glyph`.
- Prior issue docs already mark the CoreText Face build-up as passing:
  Experiments 250-255 cover table copy, scalar metrics, glyph measurement,
  metrics assembly, rasterization, and atlas render-glyph.

## Verification

- `cargo test -p roastty face::coretext::tests::face_copies_and_parses_head` —
  proves `CTFont` creation and table-copy/parsing work.
- `cargo test -p roastty face::coretext::tests::glyph_measurement` — proves
  CoreText glyph lookup/advance/bounds measurement works.
- `cargo test -p roastty face::coretext::tests::get_metrics` — proves
  `Face::get_metrics` extracts sane face metrics and feeds `Metrics::calc`.
- `cargo test -p roastty face::coretext::tests::rasterize_glyph_has_ink` —
  proves glyph rasterization produces ink for a live glyph.
- `cargo test -p roastty face::coretext::tests::rasterize_space_is_empty_or_none`
  — proves an outline-less space glyph returns no ink.
- `cargo test -p roastty face::coretext::tests::render_glyph` — proves
  CoreText-rendered glyphs can be written into the atlas and represented as
  `Glyph` values.
- `cargo test -p roastty` — full Roastty test suite stays green.
- forbidden compatibility-name grep on `roastty/src/font/face/coretext.rs`,
  `roastty/src/font/opentype`, `roastty/src/font/atlas.rs`, and
  `roastty/src/font/glyph.rs` — clean for `ghostty_*` symbols.
- `git diff --check` — clean.

Pass = the current source and tests prove CoreText `Face` rasterization and
metric extraction are implemented for Issue 801, allowing that checklist item to
be checked without new code.

## Design Review

**Reviewer:** Codex (gpt-5.5, medium) · resumed session
`019e8f83-9029-7d43-8e82-f4c5754e14ba`

**Verdict:** APPROVED.

Initial review found two Required issues. First, the `rasterize_glyph` test
filter selected only `rasterize_glyph_has_ink` and missed
`rasterize_space_is_empty_or_none`, so the verification overclaimed coverage.
Second, `roastty/src/font/face/coretext.rs` still had a stale module comment
saying metric assembly and glyph rasterization would land in later experiments.

The design now includes a doc-only source edit for the stale module comment and
separate focused gates for both rasterization tests. Follow-up review confirmed
the filters select the intended tests and that the gates are broad enough for
the CoreText `Face` checklist item while leaving Shaper separate.
