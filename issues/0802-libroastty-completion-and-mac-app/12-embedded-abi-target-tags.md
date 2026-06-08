+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"
+++

# Experiment 12: Embedded ABI — the target union + the action-tag completion

## Description

After Exp 11 the app compiles past selection and surfaces **80 errors**,
dominated by two contained divergences (the rest are a misc tail for a later
experiment):

1. **`roastty_target_s` lacks the `target` union member (51 errors).** The app
   reads `target.target.surface`, but roastty's `target_s` is flat
   `{tag, surface}`. Upstream is `target_u { surface }` +
   `target_s { tag, target_u target }`. Because the union has a single pointer
   member, this is **byte-identical** (16 B either way) — only the C/Swift shape
   changes.
2. **`roastty_action_tag_e` is missing 24 of upstream's 65 tags**
   (`MOUSE_SHAPE`, `RENDERER_HEALTH`, `SIZE_LIMIT`, `PRESENT_TERMINAL`,
   `KEY_TABLE`, `COLOR_CHANGE`, `CONFIG_CHANGE`, `PROGRESS_REPORT`,
   `COMMAND_FINISHED`, `SEARCH_TOTAL`/`_SELECTED`, `RING_BELL`,
   `SHOW_CHILD_EXITED`, …). Exp 9 added the `action_u` payload _types_ but the
   tag enum kept gaps (the Exp-9 type-gap check counted `roastty_action_*`
   types, not `ROASTTY_ACTION_*` constants). All existing roastty tags **already
   match upstream positions** (verified, 0 mismatches), so the 24 missing map to
   exact values (21, 22, 24, 25, 26, 27, 30, 31, 35–39, 41, 44–46, 48, 50, 55,
   56, 58, 61, 62).

## Approach

1. **`target_s` union:** add `roastty_target_u { roastty_surface_t surface; }`
   to `roastty.h` and change `roastty_target_s` to
   `{ roastty_target_tag_e tag; roastty_target_u target; }`. Rust:
   `RoasttyTarget { tag: c_int, target: RoasttyTargetU }` with
   `#[repr(C)] union RoasttyTargetU { surface: RoasttySurface }`. Update the one
   firing site (`perform_targeted_action_result` builds
   `RoasttyTarget { tag, surface }`) and any reader (`ActionRecord` test harness
   reads `target.surface` → `unsafe { target.target.surface }`). C
   `_Static_assert(sizeof(target_s) == 16)` **and
   `offsetof(target_s, target) == 8`** (matching the `action_s` precedent); Rust
   layout test. Move the `/* NULL when tag is ROASTTY_TARGET_APP */` comment
   onto the union member so the contract isn't lost.
2. **`action_tag_e` completion:** add the 24 missing constants at their exact
   upstream values to `roastty_action_tag_e` (C enum) + the matching Rust
   `const ROASTTY_ACTION_*: c_int = N`. No new firing logic — roastty doesn't
   yet _emit_ these actions (that's Phase-C feature work); the constants exist
   so the app's `switch (action.tag)` compiles. (A value-parity test asserts a
   few against upstream positions.)

This changes **no app source**; only `roastty.h` + `libroastty`. It does **not**
make the app fully compile — it clears the two big clusters; the misc tail (the
`input_key_s`→`key_event_t` inspector path, a couple of app enums, a
`DispatchWorkItem`) is recorded for Exp 13.

## Verification

1. **Header parses clean**, no duplicate constants, `_Static_assert`s pass
   (`target_s` 16 B).
2. **Layout/value tests (Rust):** `RoasttyTarget`/`RoasttyTargetU`
   byte-identical to the old flat layout (size 16, surface at offset 8); the 24
   new tag constants equal their upstream positions.
3. **`cargo test -p roastty --lib`** green (the `target_s` union change + the
   harness reader migration + the new constants don't regress).
4. **App rebuild:** the `target.target` (51) + the missing-`ROASTTY_ACTION_*`
   errors are gone; the remaining errors are the documented misc tail (→ Exp
   13).

**Pass** = `target_u`/`target_s` byte-faithful (layout-tested), the 24 tags
present at correct values, `cargo test` green, and the app build clears the
target + action-tag clusters.

**Partial** = the two clusters resolve + tests green, but an unexpected
dependency surfaces (documented).

**Fail** = the `target_s` union can't be reconciled without broader rework
(documented).

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: APPROVED** (no Required findings). It independently
verified the highest-risk claims: the **24 tag values are exactly correct and
non-colliding** (computed as position−1 from `ghostty.h:886-950`, fill the gaps
precisely, no clash with existing constants or the `NAVIGATE_SEARCH = 1000`
extension; spot-checked far beyond 3 that every existing roastty tag sits at its
upstream position); **`target_s` is byte-identical** (current `{tag, surface}`
and the proposed `{tag, target_u{surface}}` are both 16 B, surface at offset 8,
union derives Copy); the **reader migration is exhaustive — exactly 2 sites**
(`perform_targeted_action_result` write @lib.rs:2330, the one harness read
@15085 → `unsafe { target.target.surface }` in the already-`unsafe`
`split_action_cb`; the `record.surface` assertions read `ActionRecord`, not
`RoasttyTarget`); and the **new tags need no firing** (both `action_u_*storage`
have `_ => {}` defaults; the action path is outbound, so the app switching on a
new `.tag` never requires roastty to handle an inbound tag). Two minor findings,
folded in: add `offsetof(target_s, target) == 8`; carry the NULL comment to the
union member.

## Result

_(to be added after the run.)_

## Conclusion

_(to be added after the run.)_
