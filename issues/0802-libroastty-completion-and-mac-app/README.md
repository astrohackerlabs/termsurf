+++
status = "open"
opened = "2026-06-08"
+++

# Issue 802: Complete libroastty and prove it with a copied, renamed Ghostty macOS app

## Goal

Finish reimplementing `libghostty` (Zig) as `libroastty` (Rust), and **prove the
port is correct** by running a **copied, lightly-renamed** version of Ghostty's
macOS app on top of `libroastty` — fully, automatically tested. The end state: a
complete Zig→Rust reimplementation of `libroastty`, and a `ghostty`→`roastty`
find/replaced (otherwise **unaltered**) macOS app that runs, supports all
features, and is verified feature-by-feature against the real app's behavior via
automated UI testing (macOS automation APIs + screenshots).

The unaltered-except-for-rename app is the point: if Ghostty's own app runs
correctly on `libroastty`, the port is correct end to end. The app is the
**conformance test**.

## Background

**Predecessor:**
[Issue 801 — Reimplement libghostty as libroastty](../0801-roastty-libghostty-rewrite/README.md).
This issue continues directly from it.

**Vendored Ghostty pin (upstream version under test):** every `vendor/ghostty/…`
reference and ABI target in this issue is against **Ghostty commit
`2c62d182cec246764ff725096a70b9ef44996f7f`** (branch `main`; `git describe`:
`tip-1608-g2c62d182c`; dated 2026-05-29; `build.zig.zon`
`version = "1.3.2-dev"`; requires zig `0.15.2`). The copied app and the embedded
ABI must match this commit.

[Issue 801](../0801-roastty-libghostty-rewrite/README.md) ported the large
majority of `libghostty` to `libroastty` (terminal core largely complete; the
renderer's data + Metal/**offscreen** pipeline fully composed and driven from
`(terminal, config)`; font/text, input-encoding, and configuration foundations;
a growing `roastty_*` C ABI) under a 4394-test suite. It closed with two facts
that define this issue:

1. **The remaining subsystems are partial**, not started-from-scratch — they
   need an audit and completion in dependency order (see 801's
   [Subsystem checklist](../0801-roastty-libghostty-rewrite/README.md#subsystem-checklist)
   and [Conclusion](../0801-roastty-libghostty-rewrite/README.md#conclusion)).
2. **roastty's current C ABI diverges from libghostty's app-facing ("embedded")
   ABI.** libghostty exposes ~71 coarse `ghostty_*` exports — an `app` object, a
   `surface` object with `surface_draw` (the _library_ renders into an
   app-provided `NSView`), and a `ghostty_runtime_config_s` **callback/action**
   struct the app supplies (`set_title`, clipboard, bell, …). roastty instead
   exposes ~239 granular exports shaped around a `roastty_render_state_*` _pull_
   model (the _app_ draws) plus fine-grained terminal accessors. That
   render-state path is interim scaffolding; libghostty's embedded ABI is the
   faithful target.

**The strategic decision (this issue):** rather than write a new macOS app,
**copy Ghostty's** and rename it. This (a) gives a precise, externally-defined
ABI target (`apprt/embedded.zig` + `ghostty.h`) with no design ambiguity, (b)
turns the app into a conformance test, (c) provides the test host that the
renderer's live `surface_draw` path needs (the app supplies the `NSView`), and
(d) defers any Rust rewrite of the app — the renamed Swift app works as the
oracle, and a Rust rewrite can come later as its own issue if desired (the
project may yet want it for consistency, but it must not block this work).

## Scope

Three workstreams, in rough dependency order.

### 1. Audit the architecture and finish the port (Zig → Rust)

- **Audit** the completed `libghostty` architecture (the vendored
  `vendor/ghostty/src`) against `libroastty`, and decide which subsystems still
  need reimplementation. Produce a concrete, prioritized checklist (faithful /
  partial / missing), building on 801's Subsystem checklist.
- **Reimplement the remaining subsystems in a logical, dependency-driven
  order**, Zig → Rust, gated experiment by experiment. Known remaining work from
  801:
  - **The embedded app-runtime ABI** (`apprt/embedded.zig` / `ghostty.h`) — the
    single largest item: the `ghostty_app` object model + tick/event
    integration, `ghostty_surface_draw` rendering into the app-provided `NSView`
    (the live render wiring deferred in 801), the `ghostty_runtime_config_s`
    callback struct, and the action dispatch surface — with **byte-faithful**
    struct layouts, enums, and semantics. This supersedes the interim
    `roastty_render_state_*` path.
  - **Config** finalize/validation/diagnostics and the full upstream field set.
  - **Input** — the keybinding system (`Binding` / action dispatch) and keymaps.
  - **Font & text** — `SharedGridSet`, `opentype/` / embedded font tables.
  - **Terminal core** — `tmux` control mode; render/render-state parity items.
  - **Renderer** — remaining `state` / `image` / `pipeline` / main-loop items.

### 2. Copy (not translate) the Ghostty macOS app and rename it

- **Copy** Ghostty's macOS app into the roastty project **as-is** (pin a
  specific Ghostty version; the app and the ABI must match the same version).
- **Find/replace `ghostty` → `roastty`** (and `Ghostty` → `Roastty`,
  `GhosttyKit` → `RoasttyKit`, bundle identifiers, the linked library + header,
  etc.) and make any other strictly-mechanical `ghostty→roastty` changes
  required to build and link against `libroastty`.
- **Otherwise leave it unaltered.** No feature rewrites, no logic changes — the
  whole value is that an unmodified app is a true conformance test. Make it
  **build, run, and support all features** against `libroastty`; every gap is a
  bug in the port (workstream 1) to fix, not a reason to edit the app.

### 3. Automatically test the macOS app end to end

- **Automate** the app under macOS UI-automation APIs (e.g. XCUITest /
  Accessibility / `osascript` / `screencapture`), driving real interactions.
- **Take screenshots** and verify the app _looks right_ (golden/visual checks
  where appropriate).
- **Exercise every feature** in the UI on top of `libroastty` — typing,
  rendering, selection, clipboard, scrollback, search, splits/tabs, config, key
  bindings, resize, color schemes, etc. — and confirm each actually works.
- **Anything that doesn't work gets fixed** — in `libroastty` (the port), since
  the app is unaltered.

## Architecture / Analysis

- **The embedded ABI is the contract.**
  `vendor/ghostty/src/apprt/embedded.zig` + the generated `ghostty.h` define
  exactly what the app calls and what callbacks it provides. Matching this is
  unambiguous: match upstream, version-pinned. The faithful `roastty.h` must be
  a structural rename of `ghostty.h` (identical struct
  layouts/enums/signatures).
- **Conformance, not translation.** Because the app is copied unaltered (only
  renamed), a passing app _is_ the proof of a faithful port. This is a far
  stronger oracle than unit tests, and it makes the automated UI tests
  (workstream 3) the acceptance criteria for the whole reimplementation.
- **Layering of verification:** unit/integration tests (library), offscreen
  golden-image render tests (no app needed), then live app + automated UI tests
  (the app as host). The three reinforce each other.
- **Risks to confirm early:** (a) that the Ghostty app is a _pure_ embedded-ABI
  consumer (nothing reaches past the C ABI); (b) ABI struct-layout/alignment
  fidelity across the FFI boundary; (c) the macOS UI-automation tooling can
  drive and screenshot the app headlessly/CI-ably; (d) version pinning so the
  app and ABI never skew.

## Screenshots policy

**Screenshots are never committed to this repo.** Verification in this issue is
screenshot-heavy (the app as a visual conformance oracle), so the standing rule
is:

- The capture harness writes screenshots **outside the working tree** by default
  — `${TERMSURF_SHOT_DIR:-$HOME/.cache/termsurf/shots}`.
- As a safety net, a `__screenshots__/` directory name is **git-ignored**
  anywhere in the repo, so an explicit in-repo path still cannot be committed.
- We do **not** commit "golden" reference images. Visual checks are **live A/B**
  — capture the real Ghostty app and the roastty app in the **same run** under
  identical input, diff them, and record only the **verdict / diff metric**. Any
  retained reference lives outside the repo. (The diff metric and tolerance are
  defined by the later A/B-diff experiment; the pinned Ghostty version,
  1.3.2-dev `2c62d18`, substitutes for a committed historical baseline.)

Established in [Experiment 4](04-window-screenshot-capture.md); it supersedes
the earlier "commit a small baseline PNG set" wording in Exp 2.

## Operating notes & lessons learned

**Keep this current.** When an experiment yields a durable, reusable fact — a
toolchain incantation, a dead-end to avoid, or where an artifact lives — distill
it here (not only in the experiment file), one line with a pointer. This section
is the **cold-resume cheat-sheet**: if the working context is lost, start here
before re-reading experiments.

### Building & running the real Ghostty app (the conformance host)

- **Build it:** `scripts/ghostty-app/build-macos-app.sh [Debug|ReleaseLocal]` →
  `vendor/ghostty/macos/build/<config>/Ghostty.app`. It runs, shows a working
  terminal. (Exp 3.)
- **Pinned zig 0.15.2** lives at `vendor/toolchains/` (gitignored);
  `setup-zig.sh` fetches it. The vendored ghostty (also gitignored) is pinned at
  commit `2c62d18` (v1.3.2-dev), which requires **exactly** zig 0.15.x.
- The build is **macOS-only by necessity**: zig 0.15.2 can't link this machine's
  **Xcode 26.4 SDK** (`__availability_version_check`), so the lib + Metal are
  built under `DEVELOPER_DIR=CommandLineTools` (the 26.0 SDK links) with Xcode's
  `metal` on `PATH`; the iOS xcframework slice is patched out
  (`macos-only-xcframework.patch`); then `xcodebuild -create-xcframework` +
  `macos/build.nu` build the app under Xcode. (Exp 2/3.)

### Dead-ends — do NOT repeat these

- **Do NOT suggest downgrading Xcode.** ghostty _requires_ Xcode 26 (official
  docs); the machine has 26.4. The gap is the too-new SDK _point release_, not
  the major version. (Exp 2 made this wrong call; Exp 3 corrected it.)
- **Do NOT try to bump the zig version.** `requireZig` enforces an exact
  major.minor and the source targets 0.15.x; even ghostty `main` still pins
  0.15.2 — a higher zig fails to compile `build.zig`. (Exp 3.)
- **Clear the zig caches when switching zig versions**
  (`rm -rf vendor/ghostty/.zig-cache ~/.cache/zig`); mixing 0.16.0 and 0.15.2
  artifacts caused phantom `DarwinSdkNotFound` / missing-archive errors. (Exp
  2/3.)
- **A full-screen `screencapture` grabs the agent's Wezboard Space, not
  Ghostty's** — and a JXA `CGWindowListCopyWindowInfo` call mis-resolves its
  option constant. Capture a specific window by id instead (below). (Exp 3 → 4.)

### Screenshots

- **Capture a window:** `scripts/ghostty-app/screenshot.sh <owner|bundle|pid>` →
  `screencapture -l<id>` via `winid.swift`; Space/occlusion-independent. (Exp
  4.)
- **Never committed** — see the Screenshots policy above; written to
  `$TERMSURF_SHOT_DIR` (default `~/.cache/termsurf/shots`); `__screenshots__/`
  is gitignored.

### Input injection (Exp 5)

- **Drive the app:** keyboard via `osascript` System Events, mouse via
  `scripts/ghostty-app/inject.swift` (CGEvent); `byteprobe.py` is the raw-mode
  PTY byte-log oracle. **activate-first** + a **warmup keystroke** (the first
  key after activate drops); never truncate the byte log while the probe holds
  it open; bootstrap to `bash` (default shell is nushell).
- **What fails** (don't re-debug — known): **F11** (macOS-swallowed),
  **Ctrl-K/Ctrl-L** (app-consumed before PTY), **dead-key/IME compose**,
  **synthetic double-click word-select**. Everything else works — including
  **scroll** and full SGR mouse reporting.

### Process hygiene — kill what you spawn

- **End every experiment by killing everything you launched** (debug Ghostty,
  byte probe, background builds): `scripts/ghostty-app/stop-app.sh`. Leave
  nothing dangling.
- **`kill -9 <pid>` scoped to the build path — never `osascript … to quit`**
  (graceful quit pops an "are you sure?" dialog needing the user; SIGKILL can't
  be caught → no dialog). **Never** broad `pkill ghostty` / `killall` — only the
  exact `vendor/ghostty/macos/build/…` PID, so nothing you didn't spawn is
  touched.

### ABI / RoasttyKit (Exp 6)

- **The link artifact:** `scripts/roastty-app/build-roastty-kit.sh` → builds
  `libroastty.a` + assembles `roastty/macos/RoasttyKit.xcframework`
  (gitignored), a structural drop-in for GhosttyKit (module `RoasttyKit`,
  umbrella `roastty.h`).
- **The link surface spans 3 export modules** — `apprt/embedded.zig` +
  `config/CApi.zig`
  - `main_c.zig`; derive the worklist from what the app **calls**
    (`grep -roE 'ghostty_[a-z0-9_]+' macos/Sources`), not one file.
- **The gap is small: 78/84 app-called functions present; 6 missing**
  (`app_key`, `app_keyboard_changed`, `cli_try_action`, `inspector_metal_init`,
  `inspector_metal_render`, `set_window_background_blur`).
- **`roastty.h` is hand-written → name-presence ≠ ABI-presence.** Diff
  signatures + by-value struct layouts. Verified faithful: `surface_config_s`,
  `runtime_config_s` (callback table). **Divergent (the real work):** the **key
  event** — roastty uses an opaque `roastty_key_event_t` handle, but the app
  passes a **by-value `input_key_s` struct**; that embedded by-value ABI must be
  added (`surface_key`/`app_key`/…).
- **Rust `staticlib` native deps** (for the app link):
  `-framework AppKit QuartzCore Metal IOSurface Foundation CoreText CoreGraphics CoreFoundation -lobjc -liconv -lSystem -lc -lm`.
- **The real ABI gap is the TYPE surface, not functions (Exp 7).**
  `scripts/roastty-app/rename-app.sh` copies+renames the app into
  `roastty/macos/` (build via `build.nu`; build/ + RoasttyKit.xcframework
  gitignored). Building it revealed **56 missing `roastty_*` symbols** —
  dominated by **~36 `action_*` payload types/enums** (the `action_s`
  tagged-union members the app reads directly) + input enums + config types. The
  Exp-6 function-signature audit missed these (nested in the union); to scope
  the embedded ABI, diff **all `roastty_*` idents the app references**
  (`grep -rhoE 'roastty_[a-z0-9_]+' roastty/macos/Sources`) vs `roastty.h`.

### Where things live

- Harness + recipes: `scripts/ghostty-app/` (`build-macos-app.sh`,
  `setup-zig.sh`, `screenshot.sh`, `winid.swift`,
  `macos-only-xcframework.patch`, `README.md`).
- The architecture gap-list + finish order: [Exp 1](01-architecture-audit.md) +
  the Roadmap below.

## Roadmap

The ordered plan to 100%, derived from
[Experiment 1's audit](01-architecture-audit.md) (whose own phase lettering
predates inserting Phase A — this Roadmap is the authority). Phases run roughly
in order (later phases depend on earlier ones); the strategy front-loads a
**running, automatable app** — the real Ghostty in Phase A (baseline + reusable
harness) and the roastty-backed app drawing by Phase C — so the rest is finished
behind the running conformance oracle. This checklist is the big-picture
progress tracker — check each item off as it lands; the `## Experiments` index
below is the fine-grained record. (A subsystem is "done" only when it works in
the live app, verified by a Phase-D UI test.)

**Phase 0 — Audit**

- [x] Architecture audit: what's done / partial / missing + the order (Exp 1)

**Phase A — Baseline & feasibility: build/run/automate the real Ghostty app**

- [x] Resolve the zig toolchain (pinned **0.15.2** under `vendor/toolchains/`;
      compiles ghostty's zig under `DEVELOPER_DIR=CommandLineTools`)
- [x] Resolve the SDK blocker (Exp 2 → Exp 3): zig 0.15.2 can't link Xcode
      26.4's SDK, so build the **macOS-only** `GhosttyKit` under the
      CommandLineTools 26.0 SDK + a build-only iOS-slice patch — **no Xcode
      change, app unaltered**.
- [x] Build the real, unmodified Ghostty macOS app from `vendor/ghostty/macos`
      (`scripts/ghostty-app/build-macos-app.sh` → `BUILD SUCCEEDED`)
- [x] Launch it; confirm a working terminal window (user-confirmed in Exp 3)
- [x] Screenshot the window **in isolation** (Exp 4: `screencapture -l` +
      `winid.swift`; cross-Space, live pixels, written outside the repo)
- [x] Drive it programmatically (**input injection** — Exp 5: full
      keyboard+mouse matrix mapped; scroll works, 4 known failures) — keyboard
      via `osascript`, mouse via `inject.swift`, byte-log/pasteboard/screenshot
      oracles
- [ ] Live-A/B compare (real app vs roastty app, same run) — replaces committed
      "golden" images per the Screenshots policy; deferred to the diff
      experiment (Phase B+)

**Phase B — App shell + ABI link**

- [x] Pin the Ghostty version (app + ABI must match — 1.3.2-dev, `2c62d18`)
- [x] **Record the exact missing/mismatched ABI symbol worklist** (Exp 6): 78/84
      app-called fns present; 6 missing (`app_key`, `app_keyboard_changed`,
      `cli_try_action`, `inspector_metal_init`, `inspector_metal_render`,
      `set_window_background_blur`); `surface_config_s`/`runtime_config_s`
      layouts match; **key-event ABI diverges** (opaque handle vs by-value
      `input_key_s`)
- [x] Build `RoasttyKit.xcframework` — the link artifact (Exp 6)
- [x] Copy + rename the macOS app into `roastty/macos/`; point at
      `RoasttyKit.xcframework`; first build reaches Swift compile (Exp 7,
      `scripts/roastty-app/rename-app.sh`)
- [ ] **Make it compile/link — the embedded ABI type surface (Exp 8+):** the
      build exposed the real gap = **56 missing `roastty_*` symbols**, dominated
      by the **~36 `action_*` payload types/enums** (the `action_s` tagged-union
      members) + 6 input types/enums (`input_key_s`, `input_action_e`, …) + 4
      config types + the 6 functions. Implement byte-faithful in
      `libroastty`/`roastty.h`, drive the app's error list to zero. (Spans
      several gated experiments.)

**Phase C — Live render path (the crux)**

- [ ] `surface_draw` owns a Metal renderer bound to the app `NSView`/`CALayer`;
      attach the layer and present on-screen
- [ ] Render thread (frame pacing + cursor-blink timer)
- [ ] Renderer mailbox / `Options` (focus / visible / occlusion / change-config)
- [ ] Retire the interim `render_state` pull divergence
- [ ] **Milestone: the app launches and shows a working ASCII terminal**

**Phase D — Automated UI tests for the roastty-backed app**

- [ ] Point the Phase-A harness at the renamed roastty-backed app
- [ ] Golden-diff its screenshots/behavior against the Phase-A real-Ghostty
      baseline
- [ ] Repeatable in-session run, wired so every later phase is regression-tested
      (headless/CI automation is a separate, later concern — see Exp 2's caveat)

**Phase E — Terminal correctness**

- [ ] Port `unicode/` tables (grapheme-break, codepoint-width, symbol/Nerd-Font
      width)
- [ ] Rewrite `Terminal::print()` (width lookup + grapheme accumulation;
      mode 2027)

**Phase F — Config completeness**

- [ ] The remaining ~140 config options (font, palette, link, command,
      cursor/mouse, scrollback, `macos-*`, …)
- [ ] `finalize()` — cross-field validation / derivation / clamping
- [ ] Theme loading (themes-dir locator + file read + palette/option
      application)
- [ ] Conditional state wiring (`changeConditionalState` + conditional reload)
- [ ] `font-codepoint-map` + `clipboard-codepoint-map` as config fields
- [ ] `SharedGridSet` config→font assembly (`Key`/`DerivedConfig` → discovery →
      populated `Collection`), replacing the hardcoded-"Menlo" test path

**Phase G — Input / keybindings**

- [ ] Multi-key sequences / chords (the trie), leader keys, key tables
- [ ] Trigger-prefix flags (`global:` / `all:` / `unconsumed:` / `performable:`)
- [ ] The full action set + the default-bindings data table + reverse
      action→trigger mapping
- [ ] Command-palette catalog (`command.zig`)
- [ ] Native keymaps (`keycodes`, `KeymapDarwin`) + `RemapSet`/`Mask` — if the
      app expects roastty-side translation

**Phase H — Renderer feature-completion (in the live pass)**

- [ ] Invoke image draws (Kitty graphics + background image) in the live draw
      pass
- [ ] Custom-shader screen pass (ping-pong target + post-process apply)
- [ ] Link-highlight matcher (`renderer/link.zig` `renderCellMap`) + feed
      `link_ranges`
- [ ] Debug `Overlay` (optional)

**Phase I — Polish / remaining**

- [ ] Shell-integration injection (`termio/shell_integration.zig`)
- [ ] Sprite legacy-computing coverage (Smooth Mosaics U+1FB3C–1FBEF) + branch
      glyphs (U+F5D0–F5E3)
- [ ] Sentry crash capture (the init/capture half of `crash/`)
- [ ] SIMD fast paths (perf — base64 / VT / index-of / width)
- [ ] `os/cf_release_thread` (perf), terminfo resource

**Workstream 3 (continuous — the harness from Phase A, the roastty app from
Phase D):** every app feature gets an automated UI test — typing, rendering,
selection, clipboard, scrollback, search, splits/tabs, config, key bindings,
resize, color schemes, … — and anything broken is fixed in `libroastty` (the app
stays unaltered except for the rename).

## Experiments

- [Experiment 1: Architecture audit — what's done, what remains, and the order to finish](01-architecture-audit.md)
  — **Pass** · Claude (7 parallel subsystem audits)
- [Experiment 2: Baseline & feasibility — build, run, and automate the real Ghostty app](02-ghostty-app-baseline.md)
  — **Partial** (toolchain blocker found: zig 0.15.2 ↔ Xcode 26.4 SDK +
  iOS-slice; resolved by Exp 3's macOS-only build) · Claude/Claude
- [Experiment 3: macOS-only build — the real Ghostty app builds and runs on this machine](03-macos-only-build.md)
  — **Pass** (macOS-only `GhosttyKit` under CommandLineTools + build-only
  iOS-slice patch; app builds via Xcode 26.4 and runs — no Xcode change) ·
  Claude
- [Experiment 4: Window-isolated screenshot capture (+ no-screenshots-in-repo policy)](04-window-screenshot-capture.md)
  — **Pass** (`screencapture -l<id>` + `winid.swift` captures the Ghostty window
  cross-Space, 1600×1264 px, outside the repo; ScreenCaptureKit fallback
  unneeded) · Claude/Claude
- [Experiment 5: Comprehensive keyboard & mouse input matrix — drive everything, map what works](05-input-injection-matrix.md)
  — **Pass** (full matrix driven + classified; keyboard ~complete, mouse incl.
  **scroll** works; 4 known failures: F11, Ctrl-K/L, dead-key compose, synthetic
  double-click) · Claude/Claude
- [Experiment 6: Phase B — RoasttyKit.xcframework + the embedded-ABI link worklist](06-roastty-kit-and-abi-worklist.md)
  — **Pass** (RoasttyKit builds; 78/84 app-called fns present, 6 missing;
  configs + callback table layout-match; key event diverges — opaque vs
  by-value) · Claude/Claude
- [Experiment 7: Phase B — copy + rename the Ghostty macOS app; first build against RoasttyKit](07-copy-rename-app.md)
  — **Partial** (renamed app builds to Swift compile, links RoasttyKit; the real
  ABI gap is **56 missing symbols** — ~36 `action_*` payload types +
  input/config types — far larger than Exp 6's function audit) · Claude/Claude

## Process

Standard project process (see `CLAUDE.md`): one gated experiment at a time —
designed, AI-reviewed before implementation, plan-committed, implemented,
verified (tests / the bounded runner), result-recorded, AI-reviewed before the
next, and result-committed.

**Keep the issue current as you go (part of the result step, not optional).**
After each experiment, besides flipping its status in the index: (1) distill any
durable, reusable fact or dead-end into
[Operating notes & lessons learned](#operating-notes--lessons-learned), and (2)
update the [Roadmap](#roadmap) checkboxes. That lessons section is what makes
this issue survivable across context resets — if a fact would cost time to
rediscover, it belongs there.

**Kill every process you spawned — at the end of each experiment, leave nothing
dangling (mandatory).** Experiments here launch the debug Ghostty app, byte
probes, background builds, etc. When the experiment ends (pass _or_ fail),
terminate all of them so nothing is left running on the user's screen or
machine. Rules:

- **Kill by PID, scoped to what you spawned** — for the app,
  `scripts/ghostty-app/stop-app.sh` (kills the `vendor/ghostty/macos/build/…`
  process by PID). **Never** `osascript … to quit` (it's graceful and pops a
  confirmation dialog needing the user) — use **SIGKILL** (`kill -9 <pid>`),
  which can't be caught, so there is no dialog.
- **Never kill anything you didn't spawn.** No broad `pkill ghostty` /
  `pkill -f Ghostty` / `killall` — scope every match to the exact build-output
  path or the specific PID you launched, so an installed/stable Ghostty or any
  unrelated app is never touched.
- **Prefer launch → drive → stop in one flow** (`start-app.sh` → drive →
  `stop-app.sh`); don't leave the app running across turns "for the next step."

## Closure Criteria

This issue closes when `libroastty` faithfully implements libghostty's embedded
ABI and the remaining subsystems, **and** the copied, `ghostty→roastty`-renamed
macOS app builds, runs, and passes automated UI tests covering all features
against `libroastty` — i.e. a complete Zig→Rust reimplementation, proven by a
lightly modified real app that fully works.
