+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 631: Shaper audit

## Description

Audit and close the stale Issue 801 checklist item for:

```markdown
- [ ] Shaper (CoreText shaping, run, cache, feature) — missing
```

The implementation has landed incrementally:

- Experiments 337-350 built the shaped-cell contract, CoreText shaping, offsets,
  non-LTR ordering, cluster mapping, reorder guards, default/user features, and
  the special-font helper.
- Experiments 351-357 built run grouping, font resolution, run hash, spacer
  skip, selection/cursor breaks, and `RunIterator`.
- Experiments 358-362 connected terminal rows to `RunOptions`, `shape_row`, and
  viewport shaping.
- Experiments 629-630 added and integrated the shaped-run cache.

This experiment should verify the current source against that checklist line,
update stale module/method comments that still say Shaper pieces are deferred,
and, if the gates pass, check off only the Shaper line. It must not check off
the adjacent Collection/CodepointResolver/SharedGrid/opentype/Sprite lines.

## Source comments to update

- `roastty/src/font/shape.rs`: the module comment still says the run iterator,
  shaping hook, and CoreText pipeline are later sub-areas. Update it to describe
  the current shaping value types, feature parsing/options, and special-font
  helper.
- `roastty/src/font/run.rs`: the module comment still says the renderer code
  that builds `RunCell`s is a later sub-area. Update it to mention
  terminal-row/viewport entry points and cached row shaping.
- `roastty/src/font/face/coretext.rs`: `shape_run_with_features` still says the
  special-font path is deferred to the full Shaper. Update it to describe the
  current CoreText path and note that special/sprite shaping is handled by
  `shape::shape_special`/the sprite renderer path, not by CoreText.

## Verification

- `cargo test -p roastty face::coretext::tests::shape`
- `cargo test -p roastty shape_special`
- `cargo test -p roastty shape_row`
- `cargo test -p roastty shape_viewport`
- `cargo test -p roastty shaper_cache`
- `cargo test -p roastty shape::tests::feature`
- `cargo test -p roastty shape::tests::merged_features`
- `cargo test -p roastty face::coretext::tests::feature_settings_descriptor`
- `cargo test -p roastty font_shaping_break`
- `cargo test -p roastty rebuild_viewport`
- `cargo test -p roastty`
- `cargo fmt -p roastty -- --check`
- `rg -n "Shaper .*missing|later sub-area|shaping cache.*later|special-font path.*deferred|renderer code that builds .*later" roastty/src/font/shape.rs roastty/src/font/run.rs roastty/src/font/face/coretext.rs`
  — no stale matches.
- `rg -n "\bghostty_[A-Za-z0-9_]*\b" roastty/src/font/shape.rs roastty/src/font/run.rs roastty/src/font/face/coretext.rs roastty/src/font/shaper_cache.rs roastty/src/renderer/cell.rs`
- `git diff --check`

Pass = the current code and tests prove the Shaper checklist scope (CoreText
shaping, run grouping, cache, and feature handling), stale deferral comments are
removed, the Shaper line can be checked, and no adjacent checklist line is
changed.

Fail = any part of CoreText shaping/run/cache/feature handling is unimplemented
or only indirectly proven, the source comments still contradict the checklist
status, or checking the Shaper line would overclaim sprite, collection,
SharedGrid, or opentype completion.

## Design Review

**Reviewer:** Codex (gpt-5.5, medium) · resumed session
`019e8f83-9029-7d43-8e82-f4c5754e14ba`

**Verdict:** APPROVED.

Initial review found one required fix: the original `font_shaping_break` gate
covered the run-break config policy but did not prove the OpenType feature
parsing/options path. The design now includes explicit gates for
`shape::tests::feature`, `shape::tests::merged_features`, and
`face::coretext::tests::feature_settings_descriptor`, covering
`Feature::from_str`/`parse_features`, `Options::merged_features`, and CoreText
feature descriptor construction.

Follow-up review approved the revised design.

## Result

**Result:** Pass.

The Shaper checklist item was stale. Current Roastty source and tests now prove
the requested scope: CoreText shaping, run grouping, shaped-run caching, and
OpenType feature handling.

Source cleanup:

- `roastty/src/font/shape.rs`: updated the module comment to describe the
  current shaped-cell contract, clustered input, feature parsing/options, and
  special-font helper.
- `roastty/src/font/run.rs`: updated the module comment to describe the current
  row/viewport shaping entry points and cached renderer-facing row shaping.
- `roastty/src/font/face/coretext.rs`: updated the CoreText shaping comment so
  it no longer says the special-font path is deferred to a full Shaper; the
  special/sprite path is separate from CoreText.

Checklist update:

- `README.md`: changed only the Shaper line to checked. Adjacent Sprite,
  Collection/CodepointResolver, SharedGrid, and opentype lines remain unchecked.

Verification:

- `cargo test -p roastty face::coretext::tests::shape` — passed, 16 tests.
- `cargo test -p roastty shape_special` — passed, 4 tests.
- `cargo test -p roastty shape_row` — passed, 3 tests.
- `cargo test -p roastty shape_viewport` — passed, 1 test.
- `cargo test -p roastty shaper_cache` — passed, 7 tests.
- `cargo test -p roastty shape::tests::feature` — passed, 6 tests.
- `cargo test -p roastty shape::tests::merged_features` — passed, 1 test.
- `cargo test -p roastty face::coretext::tests::feature_settings_descriptor` —
  passed, 2 tests.
- `cargo test -p roastty font_shaping_break` — passed, 2 tests.
- `cargo test -p roastty rebuild_viewport` — passed, 5 tests.
- `cargo test -p roastty` — passed, 3470 unit tests plus 1 ABI harness test.
- `cargo fmt -p roastty -- --check` — clean.
- `rg -n "Shaper .*missing|later sub-area|shaping cache.*later|special-font path.*deferred|renderer code that builds .*later" roastty/src/font/shape.rs roastty/src/font/run.rs roastty/src/font/face/coretext.rs`
  — no stale matches.
- `rg -n "\bghostty_[A-Za-z0-9_]*\b" roastty/src/font/shape.rs roastty/src/font/run.rs roastty/src/font/face/coretext.rs roastty/src/font/shaper_cache.rs roastty/src/renderer/cell.rs`
  — no matches.
- `git diff --check` — clean.

## Conclusion

The Shaper line is complete for Issue 801's stated scope: CoreText shaping, run
grouping, cache integration, and OpenType feature handling. The remaining
font/text checklist work is outside this line: sprite drawing, collection and
resolver audit, SharedGrid/SharedGridSet audit, and opentype/embedded/Nerd Font
attribute audit.

## Completion Review

**Reviewer:** Codex (gpt-5.5, medium) · resumed session
`019e8f83-9029-7d43-8e82-f4c5754e14ba`

**Verdict:** APPROVED.

The reviewer checked the staged diff for source-comment accuracy, whether the
evidence is sufficient to check only the Shaper checklist line, whether any
adjacent checklist line is overclaimed, and whether the result record satisfies
the Issue 801 process. No required fixes were found.
