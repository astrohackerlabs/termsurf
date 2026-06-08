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

- [ ] Resolve the toolchain (zig 0.15.x for ghostty 1.3.2-dev; nushell for
      `macos/build.nu`)
- [ ] Build the real, unmodified Ghostty macOS app from `vendor/ghostty/macos`
- [ ] Launch it; confirm a working terminal (screenshot)
- [ ] Drive it programmatically (input + screenshot) from this environment;
      document the required permissions (Accessibility / Screen Recording)
- [ ] Capture the golden baseline (reference screenshots) + a reusable
      build/run/automate harness (Exp 2)

**Phase B — App shell + ABI link**

- [ ] Pin the Ghostty version (app + ABI must match — 1.3.2-dev)
- [ ] Copy the macOS app into the project as-is
- [ ] Find/replace `ghostty`→`roastty` (+ `GhosttyKit`→`RoasttyKit`, bundle IDs,
      linked library + header)
- [ ] Make it link — add the missing embedded exports (`app_key`,
      `app_keyboard_changed`, `app_open_config`,
      `inspector_metal_init/render/shutdown`, `set_window_background_blur`,
      `ghostty_translate`, `cli_try_action`), match struct layouts to the pinned
      version
- [ ] Record the exact missing/mismatched ABI symbol worklist

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
  — **Designed** · Claude/Claude/Claude

## Process

Standard project process (see `CLAUDE.md`): one gated experiment at a time —
designed, AI-reviewed before implementation, plan-committed, implemented,
verified (tests / the bounded runner), result-recorded, AI-reviewed before the
next, and result-committed. The first experiment should be the **ABI-gap
audit**: diff `roastty`'s exports/`roastty.h` against `ghostty.h` /
`embedded.zig`, classify every symbol as faithful / divergent / missing, and
produce the ordered reimplementation checklist that drives the rest of the
issue. No experiments are listed yet.

## Closure Criteria

This issue closes when `libroastty` faithfully implements libghostty's embedded
ABI and the remaining subsystems, **and** the copied, `ghostty→roastty`-renamed
macOS app builds, runs, and passes automated UI tests covering all features
against `libroastty` — i.e. a complete Zig→Rust reimplementation, proven by a
lightly modified real app that fully works.
