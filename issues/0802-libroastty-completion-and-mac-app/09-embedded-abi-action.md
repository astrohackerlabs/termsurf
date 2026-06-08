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

# Experiment 9: Embedded ABI — the action-dispatch type surface (tranche 2)

## Description

The biggest tranche of the 48-symbol gap: the **`action_*` family** — the tagged
union the app reads from the runtime-config `action` callback. This is 801's
"single largest item / the action dispatch surface."

**The Exp-6 divergence:** the app reads a **typed tagged union**
`ghostty_action_s { tag; ghostty_action_u action }` where `action_u` is a
37-member union of typed payloads (`set_title_s.title`,
`color_change_s.{kind,r,g,b}`, `open_url_s`, …). `libroastty` instead has
`roastty_action_s { int tag; uintptr_t storage[8] }` — an opaque hand-packed
array — and populates it at **20 `perform_action` call sites**. The app's
`action.action.set_title.title` won't compile (no `.action` union field) and
would read the wrong bytes.

**Scope (the 36 missing `action_*` types + the union + `action_s` + the firing
rewire):**

- **16 enums** (`action_split_direction_e`, `action_fullscreen_e`,
  `action_goto_tab_e` (negative discriminants!), `action_mouse_shape_e` (34
  values), `action_color_kind_e`, `action_open_url_kind_e`,
  `action_close_tab_mode_e`, `action_progress_report_state_e`, …).
- **20 structs** (`action_set_title_s`, `action_color_change_s`,
  `action_open_url_s`, `action_resize_split_s`, `action_move_tab_s`,
  `action_desktop_notification_s`, `action_key_sequence_s`, `action_key_table_s`
  (nested union), `action_command_finished_s`, `action_progress_report_s`,
  `action_scrollbar_s`, `action_initial_size_s`, `action_cell_size_s`,
  `action_mouse_over_link_s`, `surface_message_childexited_s`, …).
- **`roastty_action_u`** (the 37-member union) + change **`roastty_action_s`**
  to `{ roastty_action_tag_e tag; roastty_action_u action }` (byte-faithful to
  `ghostty_action_s`).
- **Rewire the 20 `perform_action` sites** to populate the typed union member
  for each tag instead of packing `storage[N]`.

(`command_s`, `quick_terminal_size_s`, `surface_message_childexited_s` from the
"misc" worklist are pulled in here — they're action/union dependencies.)

**Layout confirmed (design review, via clang static asserts):**
`ghostty_action_s` = **32 bytes / align 8**, `ghostty_action_u` = **24 bytes**
(largest members `scrollbar_s` / `open_url_s` / `key_table_s` = 24); the new
`roastty_action_s` matches and shrinks from the old oversized 72. All union
members are scalars/enums/raw pointers → a `#[repr(C)] union` is viable.

1. **Insert the type definitions** into `roastty.h` (extract `action_*` +
   dependency blocks from `ghostty.h`, rename). **But union members must
   reference the EXISTING roastty enum type names where they differ** — ~11
   already exist under other names (`roastty_inspector_mode_e` not
   `action_inspector_e`; `roastty_resize_split_e` not
   `action_resize_split_direction_e`; `roastty_close_tab_e` not
   `action_close_tab_mode_e`; …). A blind import re-emits enumerators
   (`ROASTTY_INSPECTOR_TOGGLE` …) → C "redefinition of enumerator". So:
   collision-check every constant; **reuse the existing enum type in the union
   member**, add a typedef alias for the upstream name only if the app
   references it, and define fresh only the genuinely-new types. Preserve
   **negative discriminants** (`GOTO_TAB_PREVIOUS=-1`,
   `COLOR_KIND_FOREGROUND=-1`) and signed fields (`move_tab_s.amount: ssize_t`,
   `command_finished_s.exit_code: int16_t`,
   `progress_report_s.progress: int8_t`).
2. **The `readonly` value-swap (a real divergence to fix).** Upstream
   `READONLY_OFF=0, READONLY_ON=1`; roastty's existing `roastty_readonly_e` is
   **swapped** (`ON=0, OFF=1`), and the firing site uses it. Define the union's
   `roastty_action_readonly_e` with the **upstream** values (OFF=0, ON=1); the
   `storage → union` conversion must map the internal (swapped) value correctly.
   Add a value-parity test for it specifically.
3. **Replace `roastty_action_s`** with
   `{roastty_action_tag_e tag; roastty_action_u action}`
   - add `roastty_action_u`.
4. **Rewire = ONE central conversion, not 20 sites.** The binding path is
   **type-erased** (`ParsedBindingAction::RuntimeAction(c_int, [usize;8])` →
   `perform_targeted_action_result`), so the 20 firing sites and the internal
   `(tag, storage)` carrier **stay unchanged**. Add a single
   `action_u_from_storage(tag, storage) -> RoasttyActionU` match at the one
   C-callback build point (`perform_targeted_action_result`, lib.rs ~2150, where
   `RoasttyAction { tag, storage }` is built today) — read `storage[N]` per the
   documented layout into the typed union member.
5. **Rust side:** `#[repr(C)]` payload structs/enums +
   `#[repr(C)] union RoasttyActionU` +
   `RoasttyAction { tag: c_int, action: RoasttyActionU }`; the
   `action_u_from_storage` match.
6. **Migrate the test harness + ~82 assertions.**
   `ActionRecord { … storage: [usize;8] … }` (lib.rs:14628) and the **82
   `.storage[N]` assertions** read the C callback, which now delivers the typed
   union — change the harness to capture `tag` + the typed union and the
   assertions to read `action.<member>`.
7. **Cross-check Rust↔header (not just hand numbers):** add C-side
   `_Static_assert`s (in a tiny test `.c` or the header) tying `roastty.h`
   `action_s`/`action_u`/key-payload sizes+offsets to the same constants the
   Rust `offset_of` test uses — so a Rust↔header padding/order drift is caught
   in the gated build, not at runtime.

This changes **no app source**; only `roastty.h` + `libroastty`.

## Changes / Deliverables

- `roastty/include/roastty.h` — the 36 `action_*` types + `roastty_action_u` +
  the new `roastty_action_s`.
- `roastty/src/lib.rs` — the `#[repr(C)]` payloads + union + `RoasttyAction`;
  the rewired firing; migrated tests; layout/value ABI tests.
- Result: the `action_*` symbols resolve in the app build;
  `cargo test -p roastty` green; gap 48 → ~9.

## Verification

1. **Header parses clean** (clang `-fsyntax-only`), **no duplicate enum
   constants** (the collision-check held).
2. **Layout parity, both sides:** Rust `size_of`/`offset_of` of `action_s`
   (32/align 8), `action_u` (24), and the non-trivial payloads
   (`color_change_s`, `open_url_s`, `key_table_s`, `command_finished_s` padding)
   match upstream **and** the C-side `_Static_assert`s in `roastty.h` agree with
   the same numbers (Rust↔header cross-check).
3. **Value parity:** negative discriminants (`GOTO_TAB_PREVIOUS == -1`,
   `COLOR_KIND_FOREGROUND == -1`), signed fields, and **`readonly` (OFF=0,
   ON=1)** — the storage→union conversion maps the internal swapped value
   correctly.
4. **`cargo test -p roastty --lib`** green after the `action_s` change + the
   central conversion + the harness/82-assertion migration.
5. **Static worklist check:** the `action_*` subset is empty in `roastty.h`; the
   app rebuild advances past the action symbols (next error = a config/misc
   symbol, Exp 10).

**Pass** = the action types + union + new `action_s` are byte-faithful
(layout-tested **on both sides** + value-tested incl. `readonly`), the central
`storage→union` conversion delivers the typed payload at the C callback,
`cargo test` green, the action subset resolved.

**Partial** = types resolve + tests green, but a payload layout/value mismatch
or an un-rewired firing site remains (documented as a follow-up).

**Fail** = the typed union can't be reconciled with roastty's internal action
data without a deeper rework (documented).

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: CHANGES REQUIRED → addressed.** It **verified the layout
via clang static asserts**: `ghostty_action_s` = 32 bytes/align 8,
`ghostty_action_u` = 24 (largest: `scrollbar_s`/`open_url_s`/`key_table_s`), all
members FFI-trivial → `#[repr(C)]` union viable; the new `action_s` matches and
shrinks from the old 72. Findings, addressed:

- **Required — `readonly` value-swap.** roastty's `roastty_readonly_e` is
  `ON=0,OFF=1`, inverted from upstream `OFF=0,ON=1`, and the value-check plan
  omitted it. **Fixed:** the union's `roastty_action_readonly_e` uses upstream
  values; the storage→union conversion maps the internal swapped value; a
  value-parity test for `readonly` added (step 2 / V3).
- **Required — rewire mischaracterized.** The binding path is type-erased
  (`ParsedBindingAction::RuntimeAction(c_int,[usize;8])` →
  `perform_targeted_action_result`), so "redirect at 20 sites" can't work.
  **Fixed:** the design is now **one central
  `action_u_from_storage(tag, storage)` conversion** at the single C-callback
  build point; the 20 sites + internal storage stay.
- **Optional — test scope under-counted.** **Fixed:** the `ActionRecord` harness
  (`storage:[usize;8]`) + the **82 `.storage` assertions** are called out for
  migration to the typed union.
- **Optional — no Rust↔header cross-check.** **Fixed:** added C-side
  `_Static_assert`s tying `roastty.h` sizes/offsets to the Rust `offset_of`
  numbers.
- **Nit — union members can't be a blind rename.** **Fixed:** ~11 union members
  reuse the existing roastty enum type names (`inspector_mode_e`,
  `resize_split_e`, `close_tab_e`, …); only genuinely-new types are defined
  fresh.

## Implementation notes (analysis done — ready to execute)

Pre-implementation analysis is complete, so a cold-resume starts here without
re-deriving:

- **Of the 36 missing `action_*` types: 25 are defined fresh, 11 collide →
  alias** to an existing roastty enum (the constants already exist, so
  re-defining = C "redefinition of enumerator"). The alias map (all verified): |
  new name | alias to existing | | --- | --- | | `action_float_window_e` |
  `roastty_float_window_e` | | `action_fullscreen_e` | `roastty_fullscreen_e` |
  | `action_goto_split_e` | `roastty_goto_split_e` | | `action_goto_tab_e` |
  `roastty_goto_tab_e` | | `action_goto_window_e` | `roastty_goto_window_e` | |
  `action_inspector_e` | `roastty_inspector_mode_e` | | `action_prompt_title_e`
  | `roastty_prompt_title_e` | | `action_readonly_e` | `roastty_readonly_e`
  (after the swap fix) | | `action_resize_split_direction_e` |
  `roastty_resize_split_e` | | `action_secure_input_e` |
  `roastty_secure_input_e` | | `action_split_direction_e` |
  `roastty_split_direction_e` | The 25 fresh blocks (structs + the genuinely-new
  enums like `mouse_shape_e`, `color_kind_e`, `open_url_kind_e`,
  `progress_report_state_e`, …) are extracted/renamed in `/tmp/action_defs.h`
  (regenerable from `ghostty.h`).
- **`readonly` swap fix (localized — 4 lines):** `lib.rs:213-214` has
  `ROASTTY_READONLY_ON=0, OFF=1` (inverted from upstream `OFF=0, ON=1`); flip to
  `ON=1, OFF=0` and update the two value asserts (`lib.rs:20635-36`). The firing
  site (`lib.rs:3590-3594`) uses the **named** constants (readonly→ON,
  not-readonly→OFF), so its logic is unchanged — only the integer values flip,
  becoming upstream-correct.
- **Central conversion point:** `perform_targeted_action_result`
  (`lib.rs:~2150`) builds `RoasttyAction { tag, storage }` today → replace with
  `RoasttyAction { tag, action: action_u_from_storage(tag, storage) }`. The
  storage→member layout for each tag is documented in `roastty.h`'s current
  `roastty_action_s` comment (e.g. `SET_TITLE`: storage[0]=title ptr;
  `OPEN_URL`: storage[0]=kind, [1]=ptr, [2]=len; …).
- **Test migration:** `ActionRecord` (`lib.rs:14628`) `storage:[usize;8]`
  field + **82 `.storage[N]` assertions** read the C callback → capture the
  typed union and read `action.<member>` (the harness's test `action_cb` records
  the new struct).
- **Union/struct layout (review-confirmed):** `action_s` 32 bytes/align 8,
  `action_u` 24. Add Rust `offset_of` tests + C `_Static_assert`s on both sides.

## Result

**Result:** Pass — the embedded **action** ABI is implemented byte-faithful on
both sides, the suite is green with **zero regression**, and the action subset
of the worklist is fully resolved (gap **48 → 11**, `action_*` = 0).

### What landed

- **`roastty.h`:** 31 fresh `action_*` types + 11 aliases (reusing the existing
  roastty enum names — the collision the review flagged) + the 37-member
  `roastty_action_u` union + `roastty_action_s` switched from
  `{int tag; uintptr_t storage[8]}` (72 bytes) to the typed
  `{roastty_action_tag_e tag; roastty_action_u action}` (32 bytes). Plus
  `#include <sys/types.h>` (`ssize_t`) and **C `_Static_assert`s** tying the
  layout (action_s=32, action_u=24, open_url offsets 0/8/16) to the Rust
  numbers. Header parses clean, no duplicate constants.
- **`lib.rs`:** `#[repr(C)] union RoasttyActionU` (24 B / align 8) + 5 payload
  structs (`set_title`/`start_search`/`open_url`/`move_tab`/`resize_split`);
  `RoasttyAction` switched to `{tag, action: union}`; the central
  **`action_u_from_storage(tag, storage)`** conversion at the single C-callback
  build point (`perform_targeted_action_result`); the `readonly` value-swap
  corrected (`ON=1, OFF=0` upstream).
- **The 82-assertion migration, avoided cleanly:** a test-only inverse
  **`action_u_to_storage`** reconstructs the storage carrier from the union in
  the one test cb, so all 82 `.storage[N]` assertions stay as-is yet now
  **round-trip the real `storage→union` conversion** — testing the production
  path without touching them.

### Verification

- **`cargo test -p roastty --lib`: 4396 passed, 0 failed** (4395 prior + the new
  `action_abi_layout_and_roundtrip`) — the `action_s` change + central
  conversion + union caused **no regression**.
- **Layout cross-checked both sides:** Rust `offset_of`/`size_of` (action_s
  32/align 8, action_u 24, open_url 0/8/16, resize_split.direction @4) **and**
  the C `_Static_assert`s agree — caught at compile time, not runtime. The
  round-trip test exercises READONLY, OPEN_URL, MOVE_TAB (negative),
  RESIZE_SPLIT, SET_TITLE.
- **Worklist:** `action_*` = 0 in the gap; **gap 48 → 11**. RoasttyKit + the app
  rebuild show **0 action errors** (next error = `roastty_config_color_s`, the
  Exp-10 config type).

## Conclusion

The single largest item is closed byte-faithfully with zero test churn. Two
moves made it tractable: (1) the **central `storage→union` conversion** at the
one C-callback boundary (the binding path is type-erased, so per-site rewrites
were neither needed nor possible), and (2) the **test-only reverse conversion**
that let 82 storage assertions keep working while now exercising the real path.
The remaining gap is **11**: 4 config value types
(`config_color_s`/`_color_list_s`/`_command_list_s`/`_quick_terminal_size_s`) +
`command_s`/`quick_terminal_size_s` + 4 functions (`cli_try_action`,
`inspector_metal_init`/`_render`, `set_window_background_blur`); `roastty_app`
is the Exp-7 Swift-var false positive.

**Next (Exp 10):** the config + misc/function tail — should take the app from
"types resolve" to **compiles + links**, opening Phase C (the live
`surface_draw` render path).

## Result Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: CHANGES REQUIRED → addressed.** It **independently
verified the load-bearing claims**: compiled `roastty.h`'s `_Static_assert`s
(action*s=32, action_u=24, action@8, open_url 24/8/16) **and** asserted the same
numbers against the vendored upstream header (both pass), plus
`resize_split`=8/direction@4, `move_tab`=`ssize_t`, `command_finished`=16;
enumerated **every firing site** and confirmed each data-carrying tag
(SET_TITLE/START_SEARCH/OPEN_URL/READONLY/…/RESIZE_SPLIT/MOVE_TAB/GOTO_TAB) maps
to the same storage slots the site packs (no fired payload falls to `* =>
{}`); confirmed signedness round-trips (GOTO_TAB −1/−2/−3, negative MOVE_TAB); confirmed the round-trip test is **not self-masking** (offsets pinned independently by Rust `offset_of!`and C`offsetof`
on both headers); confirmed the readonly swap is correct on both sides with no
stale assertion; and confirmed the negative discriminants + 11 aliases resolve
with upstream-equal values. **No payload-corruption bug found.**

- **Finding (addressed):** Exp 9 added 3 comments naming the literal upstream
  type — reworded to "upstream". (The cited "no literal ghostty in comments"
  rule isn't the actual Issue-801 rule, which forbids _exposing_ `ghostty_*` ABI
  names — satisfied, all types are `roastty_*` — and Exp 8 shipped equivalent
  comments; reworded anyway for the cleaner convention.)
- **Nit:** the result was uncommitted at review time (expected pre-result-commit
  state); this result commit captures all four files.
