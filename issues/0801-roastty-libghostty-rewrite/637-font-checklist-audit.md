+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 637: Font Checklist Audit

## Description

Audit the Issue 801 font checklist after the recent CoreText font foundation
experiments and correct stale status lines.

Experiments 632 through 636 completed deferred faces, deferred collection
entries, backend/library naming, collection point-size loading, and SharedGrid
codepoint caching. The README checklist still says the
`Collection`/`CodepointResolver`/`CodepointMap`/`DeferredFace`/`discovery` line
needs a final parity audit, says `SharedGrid` is missing, and says all of
`opentype`/`embedded`/`nerd_font_attributes` are missing. Those statements need
to be verified against the current Rust modules and the vendored Ghostty font
files before the next implementation slice.

This experiment should not change Rust code. Checklist/content changes should be
limited to the issue README. Experiment-process updates to this file (design
review, result, conclusion, and completion review) are still required.

## Audit Targets

1. `Collection` / `CodepointResolver` / `CodepointMap` / `DeferredFace` /
   `discovery`:
   - confirm the current CoreText/macOS scope has eager and deferred entries,
     aliases, style completion, size adjustment, point-size loading, codepoint
     overrides, presentation defaults, sprite resolution, discovery fallback,
     descriptor scoring, fallback discovery, and deferred descriptor loading;
   - if those pieces are covered and tested, mark the checklist item complete.
2. `SharedGrid` / `SharedGridSet`:
   - confirm `SharedGrid` now has the render path, glyph cache, codepoint cache,
     `get_index`, `has_codepoint`, and `cell_size`;
   - leave the checklist item open because `SharedGridSet` and the cross-surface
     ownership/refcount/locking model are still missing;
   - update the wording so it no longer claims `SharedGrid` itself is missing.
3. `opentype/`, `embedded`, `nerd_font_attributes`:
   - confirm which OpenType parsers exist (`sfnt`, `head`, `hhea`, `post`,
     `os2`, `svg`) and which upstream OpenType helpers remain absent;
   - explicitly audit upstream `opentype.zig` and `opentype/glyf.zig` so the
     checklist does not imply full OpenType parity when only the metric/color
     table readers are present;
   - confirm `nerd_font_attributes` exists as a generated Rust table;
   - leave the checklist item open if `embedded` fonts and remaining OpenType
     pieces are still absent, but update the wording to reflect completed
     sub-parts.

## Changes

1. Update `issues/0801-roastty-libghostty-rewrite/README.md`:
   - mark the collection/resolver/discovery line complete if the audit confirms
     parity for the current CoreText scope;
   - refine the SharedGrid line to distinguish completed `SharedGrid` work from
     missing `SharedGridSet`;
   - refine the OpenType/embedded/Nerd Font line to distinguish completed
     parsers/tables from missing embedded-font work and remaining OpenType
     helpers.
2. Update this experiment file only for required process records.

## Verification

- `cargo test -p roastty font::collection`
- `cargo test -p roastty font::codepoint_resolver`
- `cargo test -p roastty font::codepoint_map`
- `cargo test -p roastty font::deferred_face`
- `cargo test -p roastty font::discovery`
- `cargo test -p roastty shared_grid`
- `cargo test -p roastty font::opentype`
- `cargo test -p roastty nerd_font`
- compare/read the audited Rust files against:
  - `vendor/ghostty/src/font/Collection.zig`
  - `vendor/ghostty/src/font/CodepointResolver.zig`
  - `vendor/ghostty/src/font/CodepointMap.zig`
  - `vendor/ghostty/src/font/DeferredFace.zig`
  - `vendor/ghostty/src/font/discovery.zig`
  - `vendor/ghostty/src/font/SharedGrid.zig`
  - `vendor/ghostty/src/font/SharedGridSet.zig`
  - `vendor/ghostty/src/font/opentype.zig`
  - `vendor/ghostty/src/font/opentype/glyf.zig`
  - `vendor/ghostty/src/font/embedded.zig`
  - `vendor/ghostty/src/font/nerd_font_attributes.zig`
- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/637-font-checklist-audit.md`
- `git diff --name-only` shows only
  `issues/0801-roastty-libghostty-rewrite/README.md` and
  `issues/0801-roastty-libghostty-rewrite/637-font-checklist-audit.md`
- `git diff --check`

Pass = the README checklist reflects the audited font state accurately,
completed items are checked only when the current CoreText scope is complete and
tested, partial lines remain open with precise remaining gaps, and no Rust code
changes are made.

Fail = any checklist item is marked complete without tested parity evidence, a
partial subsystem is hidden behind vague wording, or the experiment discovers a
code gap that should become a dedicated implementation experiment instead of a
documentation update.

## Design Review

**Reviewer:** Codex (gpt-5.5) · session `019e9a8b-9245-7c02-951b-4f2982b46706`

**Verdict:** APPROVED after revision.

Initial review found four required fixes: distinguish checklist/content updates
from required experiment-file process updates; add explicit vendored Ghostty
file comparison steps; add a no-code-change check; and call out upstream
`opentype.zig` plus `opentype/glyf.zig` so the audit does not imply full
OpenType parity. The plan now includes those constraints and verification steps.
Follow-up review approved the revised design.

## Result

**Result:** Pass

The README checklist now marks the
`Collection`/`CodepointResolver`/`CodepointMap`/`DeferredFace`/`discovery` line
complete for the current CoreText/macOS scope. The audited Rust modules cover
eager and deferred collection entries, aliases, style completion, size
adjustment, point-size loading, codepoint overrides, presentation defaults,
sprite resolution, discovery fallback, descriptor scoring, fallback discovery,
and deferred descriptor loading.

The `SharedGrid` line remains open but no longer says `SharedGrid` is missing:
its render path and codepoint-cache path are present and tested, while
`SharedGridSet` ownership/refcount/locking remains absent. The OpenType line
also remains open: `sfnt`, `head`, `hhea`, `post`, `os2`, `svg`, and generated
Nerd Font attributes are present and tested, while upstream `glyf`, embedded
fonts, and full OpenType helper coverage are still missing.

Verification passed:

- `cargo test -p roastty font::collection` — 49 passed, 3450 filtered
- `cargo test -p roastty font::codepoint_resolver` — 28 passed, 3471 filtered
- `cargo test -p roastty font::codepoint_map` — 3 passed, 3496 filtered
- `cargo test -p roastty font::deferred_face` — 5 passed, 3494 filtered
- `cargo test -p roastty font::discovery` — 34 passed, 3465 filtered
- `cargo test -p roastty shared_grid` — 11 passed, 3488 filtered
- `cargo test -p roastty font::opentype` — 21 passed, 3478 filtered
- `cargo test -p roastty nerd_font` — 6 passed, 3493 filtered
- vendored file audit covered `Collection.zig`, `CodepointResolver.zig`,
  `CodepointMap.zig`, `DeferredFace.zig`, `discovery.zig`, `SharedGrid.zig`,
  `SharedGridSet.zig`, `opentype.zig`, `opentype/glyf.zig`, `embedded.zig`, and
  `nerd_font_attributes.zig`
- `git diff --name-only` was empty before the checklist/result docs edits,
  confirming no Rust code changed during the audit
- `prettier --write --prose-wrap always --print-width 80` — pass

## Conclusion

The font checklist is now more precise: the CoreText collection/resolver/
discovery foundation is complete, `SharedGrid` itself has crossed from missing
to partial-complete, and the remaining font work is explicitly `SharedGridSet`,
sprite draw/table coverage, OpenType `glyf`/full helpers, embedded fonts, and
other renderer-facing integration.

## Completion Review

**Reviewer:** Codex (gpt-5.5) · session `019e9a8f-0d37-70d0-8f87-76d4b33910d5`

**Verdict:** APPROVED.

The reviewer found no blocking issues. The staged diff was documentation-only,
kept the appropriate checklist items open, scoped the completed font foundations
to CoreText/macOS, and distinguished completed OpenType/Nerd Font pieces from
remaining `glyf`, embedded font, and helper work.
