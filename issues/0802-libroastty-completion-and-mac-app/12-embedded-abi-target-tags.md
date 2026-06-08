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

**Result:** Pass — `target_u`/`target_s` byte-faithful, the 24 tags present at
correct values, `cargo test` green, and the app build **drops from 80 errors to
1** (both target clusters + the action-tag cluster cleared — and the cascading
`floating`/`normal`/`DispatchWorkItem` errors, which were downstream of the
broken action `switch`, cleared with them).

### What landed

- **`target_s` union:** `roastty_target_u { surface }` +
  `roastty_target_s { tag, target_u target }` (byte-identical 16 B); Rust
  `#[repr(C)] union RoasttyTargetU` + `RoasttyTarget { tag, target }`; the one
  firing site (`perform_targeted_action_result`) and the one harness reader
  (`unsafe { target.target.surface }`) updated. C `_Static_assert`s (size 16,
  `target` @8) + a Rust layout test.
- **`action_tag_e` completion:** the 24 missing constants added at their exact
  upstream values (21–62 gaps) to the C enum + Rust consts
  (`#[allow(dead_code)]`, reserved for Phase-C emission). A value-parity test
  asserts **all 24** against upstream positions.

### Verification

- **`cargo test -p roastty --lib`: 4400 passed, 0 failed** (4399 + the new
  `target_abi_layout_and_action_tags_match_upstream`) — no regression from the
  union change + the harness reader migration.
- **App rebuild: `target` errors 0, missing-`ROASTTY_ACTION_*` errors 0, total
  errors 1** — the two clusters (and their cascades) are gone.

## Conclusion

The action/target glue is reconciled, and the app build is **one error from
compiling**. The sole remaining error is `AppDelegate.swift:579`:
`roastty_config_key_is_binding(config, roasttyEvent)` passes a by-value
`roastty_input_key_s`, but the function still takes the opaque
`roastty_key_event_t` handle. This is the **same by-value-key pattern as Exp 8**
(`surface_key`/`app_key`/`surface_key_is_binding`), now applied to
`roastty_config_key_is_binding` (upstream `ghostty_config_key_is_binding` takes
`ghostty_input_key_s` by value).

**Next (Exp 13):** change `roastty_config_key_is_binding` to take
`roastty_input_key_s` by value (+ migrate its test call sites, as in Exp 8),
then re-attempt the build — which should take the renamed app to a **clean
compile + link** (Phase B exit), opening Phase C (the live `surface_draw` render
path).

## Result Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: APPROVED** (no findings). It re-derived all of
`ghostty_action_tag_e` and confirmed **every one of the 24 C constants AND its
hand-synced Rust `const` equals its upstream position (0 C↔Rust mismatch)**, the
enum has 0–64 each exactly once (no duplicate, no `NAVIGATE_SEARCH=1000` clash),
and the test asserts all 24; confirmed `target_s`/`target_u` are structurally
identical to upstream with the C `_Static_assert`s compiling and the Rust layout
test asserting size 16 / align 8 / `target`@8; confirmed a regex sweep finds
**exactly two `RoasttyTarget {` literals** (both the new shape — no stale
`{tag, surface}`) and the only field read is the migrated
`unsafe { target.target.surface }`; and confirmed the `#[allow(dead_code)]`
masks no production use (the new consts are referenced only under
`#[cfg(test)]`, reserved for Phase-C emission). "Pass" / "80→1" judged honest,
layout + value tested on both sides.
