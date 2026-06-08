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

# Experiment 6: Phase B — RoasttyKit.xcframework + the embedded-ABI link worklist

## Description

Phase A proved we can build, run, drive, and capture the real Ghostty app. Phase
B is **copy + rename the app and make it link against `libroastty`.** Two things
gate that link, and this experiment delivers both:

1. **The link artifact.** The Ghostty app links `GhosttyKit.xcframework` (a
   static lib + `ghostty.h` + a `GhosttyKit` module map). The renamed app needs
   the analogous **`RoasttyKit.xcframework`** built from `libroastty`. None
   exists yet.
2. **The precise ABI worklist.** Exactly which embedded symbols the app calls
   that `libroastty` does not yet provide (or provides with a different
   signature/struct).

**Recon already established (this is good news):**

- `libroastty` **compiles cleanly** (`cargo build -p roastty` → 0 errors, 0
  warnings, `libroastty.a` 73 MB; `crate-type = ["rlib","cdylib","staticlib"]`).
- **The link surface is what the app _calls_, not one export file.** The app's
  `ghostty_*` call sites (`grep -roE 'ghostty_[a-z0-9_]+' macos/Sources`)
  resolve across **three** export modules: `apprt/embedded.zig` (the app/surface
  ABI), `config/CApi.zig` (the `ghostty_config_*` family), and `main_c.zig`
  (`init`/`info`/`string_free`/ `cli_try_action`). The worklist is derived from
  that union — diffing `embedded.zig` alone would miss the config/main_c
  surfaces (they happen to be present in roastty, but by luck, not
  construction).
- Cross-referencing **app-called** symbols against roastty's exports, the
  **needed-and-missing set is 6**: `app_key`, `app_keyboard_changed`,
  `inspector_metal_init`, `inspector_metal_render`,
  `set_window_background_blur`, `cli_try_action`. (`app_open_config`,
  `inspector_metal_shutdown`, and `translate` are exported by upstream but have
  **no call site** in this app — recorded, not a link blocker.)
- `roastty.h` is **hand-written** (~2130 lines, 239 fn decls — _not_ cbindgen),
  so **name-presence ≠ ABI-presence**: C linkage resolves by symbol name only,
  so a wrong arg type/count or a divergent by-value struct layout links fine and
  corrupts at runtime. Signatures (all present symbols) and by-value struct
  layouts must be **diffed**, not assumed.

## Approach

**Mirror the GhosttyKit xcframework exactly** (verified structure):
`GhosttyKit.xcframework/{Info.plist, macos-arm64/{Headers/{ghostty.h, module.modulemap}, libghostty-internal-fat.a}}`,
with a module map `module GhosttyKit { umbrella header "ghostty.h"; export * }`
and the app doing `import GhosttyKit`.

1. **Build the lib:** `cargo build -p roastty` (host target =
   `aarch64-apple-darwin` = macos-arm64) → `target/.../libroastty.a`.
2. **Module map:** add `roastty/include/module.modulemap` →
   `module RoasttyKit { umbrella header "roastty.h"; export * }`.
3. **Assemble:**
   `xcodebuild -create-xcframework -library libroastty.a -headers <dir with roastty.h + module.modulemap> -output roastty/macos/RoasttyKit.xcframework`
   (same mechanism that worked for GhosttyKit in Exp 3). The `.xcframework` is a
   **build artifact → gitignored** (like the toolchain/screenshots); the app
   _source_ will be committed in Exp 7.
4. **The ABI audit (the real deliverable) — done completely, not sampled:**
   - **Worklist by app reference:** enumerate every `ghostty_*` the app calls
     (`grep -roE 'ghostty_[a-z0-9_]+' macos/Sources` → unique), classify each as
     present-in-roastty / missing; the missing set is the link worklist
     (expected: the 6 above). Symbols upstream-exported but uncalled are listed
     separately (not blockers).
   - **Full signature diff (all present symbols):** for every app-called symbol
     present in roastty, mechanically compare the `roastty.h` declaration to the
     `ghostty.h` one (arg count/types, return) modulo the `ghostty_`→`roastty_`
     / `Ghostty`→`Roastty` rename. Record every drift — each is a
     silent-ABI-corruption bug to fix.
   - **By-value struct/enum layout diff:** for the structs passed by value
     across the ABI — `input_key_s`, `surface_config_s`, and especially
     `runtime_config_s` (the callback function-pointer table) — diff field
     order/types/sizes `ghostty.h` ↔ `roastty.h`. Linking does **not** validate
     these; a divergence corrupts at call time.
   - **Native link deps:** record
     `cargo rustc -p roastty -- --print native-static-libs` so Exp 8's app-link
     step knows which system libs/frameworks the Rust `staticlib` drags in
     (libSystem, libiconv, Security, CoreFoundation, libunwind, …) — the app's
     Zig-tuned link won't supply them by default.

This experiment changes **no app source** and adds only a module map + a build
script + the worklist; it does not yet implement the 6 missing symbols or fix
drifts (that's the copy/rename in Exp 7 and the link/fix in Exp 8).

## Changes / Deliverables

- `roastty/include/module.modulemap` — the `RoasttyKit` module (umbrella
  `roastty.h`).
- `scripts/roastty-app/build-roastty-kit.sh` — build `libroastty.a` + assemble
  `roastty/macos/RoasttyKit.xcframework`.
- `.gitignore` — ignore `roastty/macos/RoasttyKit.xcframework` (build artifact).
- The **ABI worklist** (this doc's Result): the missing set (derived from app
  references), the **full signature diff** of all present app-called symbols,
  the **by-value struct layout diff**, and the **`native-static-libs`** list.
- Lessons update (the ABI gap is small; the RoasttyKit recipe; the link-surface
  spans embedded + config + main_c).

## Verification

1. `cargo build -p roastty` → 0 errors, `libroastty.a` present.
2. `build-roastty-kit.sh` → `roastty/macos/RoasttyKit.xcframework` assembles
   (`xcodebuild` rc=0) with
   `macos-arm64/{Headers/{roastty.h, module.modulemap}, libroastty.a}` +
   `Info.plist`, mirroring GhosttyKit's structure.
3. **Missing set by app reference:** enumerate app-called `ghostty_*` (across
   `embedded.zig` + `config/CApi.zig` + `main_c.zig`), confirm the
   missing-in-roastty set (expected: the 6); list upstream-but-uncalled
   separately.
4. **Full signature diff of all present app-called symbols** (`roastty.h` ↔
   `ghostty.h`, modulo rename) — every drift recorded.
5. **By-value struct/enum layout diff** for `input_key_s`, `surface_config_s`,
   `runtime_config_s` (+ any other by-value ABI struct the app passes) — drifts
   recorded.
6. **`native-static-libs`** captured for Exp 8.

**Pass** = `RoasttyKit.xcframework` builds and is structurally a drop-in for
GhosttyKit, **and** the ABI worklist is _complete_: missing set derived from
actual app references, **all** present app-called signatures diffed, the
by-value struct layouts diffed, and the native link-deps recorded.

**Partial** = the xcframework builds but the full diff finds drifts/struct
mismatches that widen the worklist (still a go — just more Exp-7/8 work; the
value is the precise map).

**Fail** = `libroastty` can't be packaged into a usable xcframework (e.g. a
static-lib / module-map problem with no reasonable fix) — documented precisely.

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: CHANGES REQUIRED → addressed.** It independently
re-derived the symbol diff (confirmed 71 embedded exports, 64 present by
prefix-swap, 7 embedded absent) but showed the _worklist_ was imprecise in both
directions:

- **Required — worklist must come from app references, not `embedded.zig`.** 3
  of the original 9 entries (`app_open_config`, `inspector_metal_shutdown`,
  `translate`) have **zero** call sites under `macos/Sources`, and the
  "translate is used" claim was false. The needed-and-missing set is **6**
  (`app_key`, `app_keyboard_changed`, `inspector_metal_init/render`,
  `set_window_background_blur`, `cli_try_action`). **Fixed:** worklist now
  derived from app-called symbols; the 3 uncalled are recorded separately, the
  translate claim corrected.
- **Required — link surface spans 3 modules.** The app also links 14 `config_*`
  (`config/CApi.zig`) + 4 `main_c.zig` symbols; `embedded.zig`-only diff would
  miss them (roastty has them, but by luck). **Fixed:** the audit enumerates
  across all three.
- **Required — diff all 64 signatures, not 8.** Hand-written `roastty.h` +
  name-only C linkage ⇒ a wrong arg type anywhere is silent ABI corruption.
  **Fixed:** full signature diff of every present app-called symbol.
- **Optional — by-value struct layout doesn't fail at link.** **Fixed:** added a
  layout diff for `input_key_s` / `surface_config_s` / `runtime_config_s`
  (callback table) here, not deferred.
- **Optional — Rust `staticlib` native deps.** **Fixed:** capture
  `cargo rustc -- --print native-static-libs` for Exp 8's link.
- **Nit — Pass bar too low.** **Fixed:** Pass now requires the complete audit,
  not just "xcframework builds."

## Result

**Result:** Partial — `RoasttyKit.xcframework` builds as a structural drop-in
for GhosttyKit, and the audit produced the complete, app-reference-derived ABI
worklist below. Partial (not Pass) per this experiment's own rubric: the full
diff **found divergences that widen the worklist** — most notably **three
by-value ABI-shape divergences** (key event, selection, action), caught only
after a first pass under-counted to one. The core app/config/callback ABI is
faithful; the by-value structs are the real Phase-B/C work.

### RoasttyKit.xcframework

Built via `scripts/roastty-app/build-roastty-kit.sh` →
`roastty/macos/RoasttyKit.xcframework/{Info.plist, macos-arm64/{Headers/{roastty.h, module.modulemap}, libroastty.a}}`
— mirrors GhosttyKit exactly. Gitignored (build artifact). Module name
`RoasttyKit` (app will `import RoasttyKit`).

### The link worklist

The app calls **84** `ghostty_*` functions (across `embedded.zig` +
`config/CApi.zig` + `main_c.zig`); **78 are present** in `libroastty`, **6 are
missing**.

**A. Missing functions (6) — implement in `libroastty`:**

| Symbol                       | Signature (from `embedded.zig`/`main_c.zig`)           |
| ---------------------------- | ------------------------------------------------------ |
| `app_key`                    | `(app, ghostty_input_key_s) bool` — by-value key event |
| `app_keyboard_changed`       | `(app) void`                                           |
| `cli_try_action`             | `() void` (`main_c.zig`)                               |
| `inspector_metal_init`       | `(inspector, objc id device) bool`                     |
| `inspector_metal_render`     | `(inspector, …) ` Metal draw into the inspector view   |
| `set_window_background_blur` | `(surface, …) ` window blur                            |

(`app_open_config`, `inspector_metal_shutdown`, `translate` are
upstream-exported but have **no call site** in this app — recorded, not link
blockers.)

**B. By-value ABI-shape divergences (the headline work — a complete diff of all
29 structs crossing the app-called ABI found _three_).** `libroastty`
represented these as interim opaque/handle/grid shapes; the unaltered app passes
the embedded **by-value** structs, so each links-or-compiles wrong and corrupts
at the boundary. All three must be reconciled to the embedded by-value layout:

1. **`input_key_s` (missing).** Upstream
   `surface_key`/`surface_key_is_binding`/`app_key`/ `inspector_key` take a
   by-value 7-field struct
   (`input_action_e action; input_mods_e mods; input_mods_e consumed_mods; uint32 keycode; const char* text; uint32 unshifted_codepoint; bool composing`).
   `roastty.h` has **no `roastty_input_key_s`** — it uses an opaque handle
   `roastty_key_event_t` (`= void*`) + a builder API
   (`key_event_new`/`set_action`/…).
2. **`selection_s` (different layout).** Upstream:
   `point_s top_left; point_s bottom_right; bool rectangle` (point-based; the
   app builds it from coords and passes it by value to `surface_read_text`).
   roastty: `size_t size; grid_ref_s start; grid_ref_s end; bool rectangle` —
   and `grid_ref_s` holds a `void* node` the app cannot supply.
3. **`action_s` (different representation).** Upstream: a tagged union
   (`action_tag_e tag; action_u value`) the app reads in its runtime-config
   `action` callback. roastty: `int tag; uintptr_t storage[8]` (opaque) — the
   app can't interpret the payload.

Adding the embedded by-value `input_key_s` / `selection_s` (point-based) /
`action_s` (tagged union, with the full `action_tag_e` + `action_u`) — plus the
`input_action_e` / `input_mods_e` enums — is the core embedded-ABI work
(consistent with 801's "the embedded ABI is the faithful target; the
render-state path is interim scaffolding").

**C. Enum drifts (compile-time, must align):**

- `surface_key_is_binding`: flags out-param is `roastty_keybind_flags_t*`
  (`= uint8_t`, **1 byte**) vs upstream `binding_flags_e*` (a C `enum`, **4
  bytes**) — a **size** divergence (the lib would write 1 byte where the app
  allocates 4), not just a name. No `roastty_binding_flags_e` exists; align name
  **and** width (a 4-value enum).
- Enum-stem name drifts in `inspector_key` / `input_trigger_s`: upstream
  `input_action_e` / `input_key_e` are named `key_action_e` / `key_e` in
  `roastty.h` (no `roastty_input_action_e` / `roastty_input_key_e`), so the
  renamed app won't resolve under a pure `ghostty_`→`roastty_` rename. Values
  match (`RELEASE=0`/`PRESS=1`), so it's a compile-time, not runtime, item.
- `config_new`, `surface_config_new`: `()` vs `(void)` — **benign** (identical
  ABI).

**D. Verified faithful (no action):** of the 29 structs crossing the app-called
ABI, **26 match** field-for-field modulo the rename — notably `surface_config_s`
(12 fields) and `runtime_config_s` (the 8-entry callback table: `set_title`,
clipboard, …), so the app↔lib callback contract is correct. The diverging three
are §B. At the function level, the 78 present signatures match except the §C
enum-name drifts.

**E. Native link deps (for Exp 8's app link)** — the Rust `staticlib` pulls in:
`-framework AppKit -framework QuartzCore -framework Metal -framework IOSurface -framework Foundation -framework CoreText -framework CoreGraphics -framework CoreFoundation -lobjc -liconv -lSystem -lc -lm`.

## Conclusion

The Phase-B link gap is now precisely mapped, and it's smaller than 801 feared
on the function-count axis (78/84 present, only 6 missing) but has **three
by-value shape divergences** — `input_key_s`, `selection_s`, `action_s` — that
must be reconciled to the embedded by-value ABI (roastty used interim
opaque/handle/grid shapes). The faithful core (the
`surface_config_s`/`runtime_config_s` callback contract + 26/29 structs + 78/84
functions) already matches, which de-risks the app shell.
**RoasttyKit.xcframework** exists as the link artifact.

**Next (Exp 7):** copy the Ghostty macOS app into `roastty/macos/`, find/replace
`ghostty→roastty` / `GhosttyKit→RoasttyKit`, point it at
`RoasttyKit.xcframework`, and attempt the first build — surfacing the real
Swift-compile + linker errors (expected: the 6 missing symbols + the
`input_key_s` by-value type the app constructs + the `binding_flags_e` enum
name). **Exp 8** then closes them in `libroastty`.

## Result Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: CHANGES REQUIRED → addressed.** It independently
confirmed the 6-missing set, the RoasttyKit artifact (structure, gitignore,
module name), the key-event divergence, and that
`surface_config_s`/`runtime_config_s` match field-for-field — but showed the
worklist was **incomplete**, which is why this is now **Partial**:

- **Required — missed `selection_s`** (a second by-value divergence): the app
  builds a point-based `selection_s` and passes it by value to
  `surface_read_text`, but roastty's is grid-ref-based with a `void* node` the
  app can't supply. **Fixed:** a complete diff of **all 29** structs crossing
  the app-called ABI was run — it found **three** divergences (`input_key_s`,
  `selection_s`, **`action_s`** — the latter also missed by the first pass:
  tagged union vs opaque `int + uintptr_t[8]`). §B rewritten; the "one
  divergence / 76 match" claims corrected to "three / 26 of 29 structs match."
- **Required — enum-stem name drifts** (`input_action_e`→`key_action_e`,
  `input_key_e`→`key_e` in `inspector_key`/`input_trigger_s`): compile-time
  worklist items omitted. **Fixed:** added to §C (values match, so
  runtime-safe).
- **Optional — `binding_flags` is a size divergence** (4-byte enum vs 1-byte
  `uint8_t`), not just a name. **Fixed:** §C now notes the width.
- **Nit — decl count** (237→239). **Fixed.**

The lesson: a by-value ABI audit must enumerate **every** struct in the call
surface, not a hand-picked few — the first pass's 3-struct sample is exactly how
`selection_s`/ `action_s` slipped through.
