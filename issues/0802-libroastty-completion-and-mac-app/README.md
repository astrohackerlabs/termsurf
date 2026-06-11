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

- **Verify ABI work with the FULL `cargo test -p roastty`, never just `--lib`.**
  `--lib` skips `tests/abi_harness.rs`, the integration test that compiles
  `abi_harness.c` against `roastty.h` and links the cdylib. Exp 8–15 were
  validated with `--lib` only, so the harness silently failed to compile from
  Exp 8 onward (141 errors) — not caught until Exp 16. The harness is the C-side
  ABI conformance oracle; keep it compiling every experiment.
- **Cursor blinking is default-on at the terminal mode layer.** Exp 60 changed
  DEC mode 12 (`cursor_blinking`) to default true so formatter/mode reports do
  not emit `?12h` for a fresh terminal; configured `cursor-style-blink = true`
  or `false` still gates later in-band DEC mode 12 mutations.
- **Window padding config is now a parser/formatter surface only.** Exp 70 wires
  `window-padding-x` and `window-padding-y` through `Config` with upstream
  `WindowPadding` defaults and diagnostics; Exp 71 adds `window-padding-balance`
  as a config enum. Renderer geometry remains later work.
- **Window inheritance/title scalar config is parser/formatter-only.** Exp 72
  wires `window-vsync`, window/tab/split working-directory inheritance,
  `window-inherit-font-size`, and `window-title-font-family`; runtime behavior
  remains later work.
- **Window size config is parser/formatter/finalize-only.** Exp 73 wires
  `window-height`, `window-width`, and `window-step-resize`; nonzero sizes clamp
  in `Config::finalize`, but runtime window sizing remains later work.
- **Quit-delay config is parser/formatter-only.** Exp 78 wires
  `quit-after-last-window-closed-delay` as an optional `Duration`; delayed app
  shutdown, CLI `-e` side effects, and short-duration warning logs remain later
  work.
- **Undo-timeout config is parser/formatter-only.** Exp 79 wires `undo-timeout`
  as a `Duration` with upstream default `5s`; runtime undo stack expiration and
  binding behavior remain later work.
- **Quick-terminal position config is parser/formatter-only.** Exp 80 wires
  `quick-terminal-position` as an enum with upstream default `top`; quick
  terminal sizing, window behavior, and toggle actions remain later work.
- **Quick-terminal size config is parser/formatter/calculation-only.** Exp 81
  wires `quick-terminal-size` as the upstream percentage/pixel struct; runtime
  window sizing and the app C ABI accessor remain later work.
- **GTK quick-terminal config is parser/formatter-only.** Exp 82 wires
  `gtk-quick-terminal-layer` and `gtk-quick-terminal-namespace`; empty values
  reset to upstream defaults before enum/string parsing, and GTK layer-shell
  runtime behavior remains later work.
- **Quick-terminal screen/animation config is parser/formatter-only.** Exp 83
  wires `quick-terminal-screen`, `quick-terminal-animation-duration`, and
  `quick-terminal-autohide` with macOS upstream defaults; runtime screen
  selection, animation timing, and focus-loss autohide remain later work.
- **Quick-terminal space/keyboard config is parser/formatter-only.** Exp 84
  wires `quick-terminal-space-behavior` and
  `quick-terminal-keyboard-interactivity`; runtime macOS Spaces behavior and
  Wayland keyboard interactivity remain later work.
- **Command-palette entry config is parser/formatter-only.** Exp 85 wires
  `command-palette-entry` with the pinned upstream 88-entry default list,
  repeatable append, `clear`, empty/missing-value default restore, duplicate
  last-wins fields, quoted-string decoding, and canonical action validation /
  formatting through Roastty's keybinding action parser; runtime command-palette
  UI and app C ABI exposure remain later work.
- **VT KAM is now a config-backed surface key gate.** Exp 86 wires
  `vt-kam-allowed` through config parsing/formatting and into the embedded
  surface key path: keybindings run first, then ANSI mode 2 (`disable_keyboard`)
  consumes normal key input only when the config allows it; config updates also
  refresh existing surfaces.
- **Custom shader config is parser/formatter/path-expansion-only.** Exp 87 wires
  `custom-shader` as an upstream `RepeatablePath`: repeated entries append, raw
  empty clears, parsed-empty paths are ignored, formatting emits one line per
  shader, and load-file / CLI-base expansion preserves required/optional path
  status; shader loading, compilation, and renderer integration remain later
  work.
- **Bell features config is parser/formatter-only.** Exp 88 wires
  `bell-features` as upstream's packed bool flags (`system`, `audio`,
  `attention`, `title`, `border`) with default `attention,title`, standalone
  bool-all parsing, `[no-]flag` lists, empty reset, missing-value diagnostics,
  and canonical formatter output; runtime bell delivery and app attention/title/
  border/audio effects remain later work.
- **App notifications config is parser/formatter-only.** Exp 89 wires
  `app-notifications` as upstream's packed bool flags (`clipboard-copy`,
  `config-reload`) with both enabled by default, standalone bool-all parsing,
  `[no-]flag` lists, empty reset, missing-value diagnostics, and canonical
  formatter output; runtime toast delivery and app C ABI exposure remain later
  work.
- **macOS icon config is parser/formatter-only.** Exp 90 wires `macos-icon`,
  `macos-custom-icon`, `macos-icon-frame`, `macos-icon-ghost-color`, and
  `macos-icon-screen-color` with upstream defaults, enum keywords, optional
  string/color/color-list parsing, empty reset, diagnostics, and canonical
  formatter output; runtime dock icon selection, custom icon loading,
  custom-style validation/rendering, app C ABI exposure, and macOS app
  integration remain later work.
- **macOS Shortcuts config is parser/formatter-only.** Exp 91 wires
  `macos-shortcuts` with upstream default `ask`, enum keywords `allow`/`deny`/
  `ask`, empty reset, missing/invalid diagnostics, and canonical formatter
  output; runtime Shortcuts authorization, action dispatch, app C ABI exposure,
  and macOS app integration remain later work.
- **Linux cgroup config is parser/formatter-only.** Exp 92 wires `linux-cgroup`,
  `linux-cgroup-memory-limit`, `linux-cgroup-processes-limit`, and
  `linux-cgroup-hard-fail` with upstream defaults, base-0 optional `u64`
  parsing, empty reset, diagnostics, and canonical formatter output; runtime
  transient `systemd` scope creation, resource-limit application, app C ABI
  exposure, and Linux app integration remain later work.
- **GTK chrome config is parser/formatter-only.** Exp 93 wires
  `gtk-opengl-debug`, `gtk-single-instance`, `gtk-titlebar`,
  `gtk-tabs-location`, `gtk-titlebar-hide-when-maximized`, `gtk-toolbar-style`,
  `gtk-titlebar-style`, and `gtk-wide-tabs` with upstream defaults, enum
  keywords, empty reset, compatibility shims for `gtk-single-instance = desktop`
  and `gtk-tabs-location = hidden`, diagnostics, and canonical formatter output;
  runtime GTK chrome behavior, app C ABI exposure, and GTK app integration
  remain later work.
- **GTK CSS / notification / progress config is parser/formatter-only.** Exp 94
  wires `gtk-custom-css`, `desktop-notifications`, and `progress-style` with
  upstream defaults, repeatable path syntax and base expansion for GTK CSS, bool
  reset/diagnostics, and canonical formatter output; runtime GTK CSS loading,
  terminal desktop-notification OSC behavior, progress-style OSC behavior, app C
  ABI exposure, and GTK app integration remain later work.
- **TERM / enquiry-response config is parser/formatter-only.** Exp 95 wires
  `term` and `enquiry-response` with upstream defaults, string reset and NUL
  diagnostics, and canonical formatter output; runtime child-process `TERM`
  propagation, ENQ response behavior, and app C ABI exposure remain later work.
- **Async backend / auto-update config is parser/formatter-only.** Exp 96 wires
  `async-backend`, `auto-update`, and `auto-update-channel` with exact upstream
  enum keywords and raw defaults; runtime async backend selection, Sparkle
  update behavior, and `auto-update-channel` finalization remain later work.
- **Config finalize scalar tail is wired.** Exp 97 restores empty `term`, clamps
  `minimum-contrast` and `faint-opacity`, and fills unset `auto-update-channel`
  from the pinned `1.3.2-dev` build channel (`tip`); heavier finalize behavior
  such as theme loading, conditional reload, working-directory/default-shell
  resolution, GTK runtime defaults, link matcher mutation, and key-remap
  finalization remains later work.
- **Absolute-path theme loading is wired.** Exp 99 loads existing absolute theme
  files during config finalization, then replays user file/CLI entries on top so
  explicit user config wins; named theme lookup, user/resource theme
  directories, full diagnostic parity, and conditional reload remain later work.
- **Named theme lookup is wired.** Exp 100 resolves non-absolute theme names
  from user `roastty/themes` first, then app resource `themes`, rejects
  path-separator names, reports tried paths, and preserves Exp99 replay
  priority; conditional reload, diagnostic text parity, resource packaging
  validation, and app ABI exposure remain later work.
- **Conditional theme reload foundation is wired.** Exp 101 marks different
  light/dark themes as dependent on the theme conditional state and can rebuild
  a fresh config from recorded file/CLI replay entries when that state changes;
  app ABI exposure, runtime OS-theme notifications, general conditional syntax,
  conditionalized theme-file replay steps, and live surface/app propagation
  remain later work.
- **Working-directory finalize is wired.** Exp 102 computes the upstream
  probable-CLI heuristic during config finalization, defaults an unset
  `working-directory` to `inherit` for probable CLI launches and `home`
  otherwise, and expands explicit `~/...` paths after theme replay; default
  shell resolution, passwd-home conversion for `home`, GTK runtime defaults,
  link matcher mutation, and key-remap finalization remain later work.
- **Command/home finalize is wired for UTF-8 values.** Exp 103 extends config
  finalization to resolve an unset `command` from present `$SHELL` in probable
  CLI contexts or passwd shell otherwise, and converts
  `working-directory = home` to a passwd-home path or `inherit` when no UTF-8
  home is present; byte-faithful non-UTF-8 config storage, runtime launch
  fallback cleanup, GTK runtime defaults, link matcher mutation, and key-remap
  finalization remain later work.
- **GTK single-instance detect finalization is wired.** Exp 104 adds the
  upstream GTK-only default for `gtk-single-instance = detect`: GTK probable CLI
  resolves to `false`, GTK non-CLI resolves to `true`, and the current non-GTK
  embedded/mac production runtime remains unchanged; GTK runtime behavior, link
  matcher mutation, quit-delay warning logging, key-remap finalization, and
  byte-faithful config string storage remain later work.
- **Quit-delay finalize warning is recorded in core config reports.** Exp 105
  adds a typed `ConfigFinalizeReport` warning for
  `quit-after-last-window-closed-delay` values shorter than five seconds while
  preserving the configured duration; app-facing/log plumbing, delayed shutdown
  behavior, link matcher mutation, key-remap finalization, and byte-faithful
  config string storage remain later work.
- **Default link-url matcher finalization is wired.** Exp 106 adds config-owned
  storage for upstream's pinned default URL/path matcher and removes that
  first/default matcher during finalization when `link-url = false`; user
  `link = ...` parsing, regex compilation/matching, renderer link ranges, link
  preview UI, open-url dispatch, app C ABI exposure, key-remap finalization, and
  byte-faithful config string storage remain later work.
- **Key-remap set foundation is wired.** Exp 107 ports the input-side
  `RemapSet`/mask/parser/finalize/apply/formatter foundation; the `key-remap`
  config field, config finalization call, app ABI exposure, runtime surface
  key-event remapping, and byte-faithful config string storage remain later
  work.
- **Key-remap config is wired.** Exp 108 adds `key-remap` to `Config`
  parsing/formatting/finalization using Exp 107's `RemapSet`; app ABI exposure,
  surface cloning, runtime key-event application before keybind/input encoding,
  native keymaps, and byte-faithful config string storage remain later work.
- **Key-remap runtime application is wired for surfaces.** Exp 109 clones the
  finalized `key-remap` set into each `Surface`, refreshes it on app/surface
  config updates, and applies it before configured/default binding lookup and
  terminal key encoding; native keymaps, app-scoped `roastty_app_key`, full
  upstream keybinding tables, app ABI config-string exposure, and keyboard
  layout-change handling remain later work.
- **Keybind trigger-prefix flags are parser/storage/query metadata.** Exp 110
  adds upstream-compatible `global:`, `all:`, `unconsumed:`, and `performable:`
  prefix parsing for configured keybinds, stores the C-facing flag byte, derives
  `roastty_app_has_global_keybinds` from `global:`, and returns configured flags
  from surface binding queries. Runtime global shortcut registration,
  all-surface routing, unconsumed pass-through, performable configured-action
  gating, sequences/chords, native keymaps, and `roastty_app_key` dispatch
  remain later work.
- **Configured binding consumption is wired for surfaces.** Exp 111 applies the
  configured-keybind flag byte in `Surface::key`: `unconsumed:` actions now
  perform and still encode the key, `performable:` actions only consume when
  performed, and global/all bindings remain consumed in the current surface
  path. Runtime global shortcut registration, all-surface action routing,
  app-scoped `roastty_app_key`, sequences/chords, key tables, native keymaps,
  and the full upstream binding table remain later work.
- **Default binding table foundation is wired for macOS single-key defaults.**
  Exp 112 moves Roastty's existing default runtime key lookup and reverse
  action-to-trigger lookup onto one ordered table. Reverse lookup skips
  performable rows for menu labels, but actions with separate non-performable
  defaults still reverse-map to those rows. Multi-key sequences/chords, key
  tables, non-macOS defaults, global/all app routing, `roastty_app_key`, native
  keymaps, command-palette catalog data, and the remaining upstream default
  bindings remain later work.
- **App-level global key dispatch is wired for configured single-key bindings.**
  Exp 113 makes `roastty_app_key` match configured `global:` keybinds, consume
  matched captures, dispatch app-scoped actions once, and fan out surface-scoped
  actions to live app surfaces. Plain `all:` remains surface-key-path behavior;
  native keymaps, keyboard-layout reload, sequences/chords, key tables, default
  global bindings, and full app action coverage remain later work.
- **Focused app-key app actions are wired for configured single-key bindings.**
  Exp 114 extends `roastty_app_key` to match upstream's focused app-key split:
  `global:` bindings still work regardless of focus, focused non-global
  app-scoped bindings dispatch once to the app target, and focused non-global
  surface-scoped bindings return false for the app-key path. Native keymaps,
  keyboard-layout reload, sequences/chords, key tables, default global bindings,
  and full action catalog coverage remain later work.
- **Configured `catch_all` triggers are wired for single-key bindings.** Exp 115
  parses `catch_all` and modifier-specific forms, returns
  `ROASTTY_TRIGGER_CATCH_ALL` through reverse trigger lookup, and applies
  upstream fallback order for config/app/surface binding queries. Because
  Roastty stores built-in defaults separately from configured bindings, surface
  dispatch explicitly checks configured exact bindings, built-in exact defaults,
  then configured `catch_all`; this keeps `catch_all` from shadowing exact
  default bindings, including unperformed `performable:` defaults. Sequences,
  key tables, native keymaps, native global shortcuts, broader global/all
  routing, and the full upstream binding catalog remain later work.
- **Configured key-table runtime activation is wired for surface single-key
  bindings.** Exp 117 adds per-surface active table stacks capped at upstream's
  depth 8, `activate_key_table`, `activate_key_table_once`,
  `deactivate_key_table`, and `deactivate_all_key_tables`, plus table-local
  exact/catch-all lookup before root/default lookup and
  `ROASTTY_ACTION_KEY_TABLE` app notifications. Sequences/chords, `chain=`,
  `ignore`, native keymaps, native global shortcuts, app-key table handling, and
  the full upstream binding catalog remain later work.
- **Configured key sequences are wired for root and active-table surface
  bindings.** Exp 118 stores root/table sequence tries, Exp 119 activates root
  sequences, and Exp 120 activates table-local sequences from active key tables,
  including nested leaders, `catch_all` sequence leaders, one-shot table
  popping, invalid-prefix flushes, and `surface_key_is_binding` leader/leaf
  flags. Exp 120's final full `cargo test -p roastty -- --test-threads=1` run
  passed after earlier transient PTY foreground-pid / mouse-readonly failures.
  Remaining Phase G keybinding gaps include `ignore`, `end_key_sequence`,
  `chain=`, native keymaps/global shortcuts, app-key sequence/table handling,
  broader global/all routing, and the full upstream binding catalog.
  keymaps/global shortcuts, app-key sequence/table handling, broader global/all
  routing, and the full upstream binding catalog.

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
- **Diff two captures:**
  `swift scripts/roastty-app/pngdiff.swift <expected.png> <actual.png>` emits
  one JSON metrics object on stdout and writes no artifacts (threshold flags:
  `--max-mismatch-ratio`, `--max-mean-channel-delta`). Use it for Phase-D live
  A/B verdicts; keep images outside the repo. (Exp 38.)
- **Run the first live A/B smoke:**
  `scripts/roastty-app/live-ab-smoke.sh --max-mismatch-ratio 1 --max-mean-channel-delta 255`
  launches debug Ghostty + Roastty, drives the same ASCII marker, captures
  Ghostty by window id, captures Roastty through the IOSurface-safe full-screen
  crop path, diffs the captures, prints one JSON summary, and traps exact
  launched-PID-tree cleanup. Strict thresholds currently fail with a useful
  metric rather than parity. (Exp 39.)
- **Choose a live A/B recipe:**
  `scripts/roastty-app/live-ab-smoke.sh --list-recipes`, then
  `scripts/roastty-app/live-ab-smoke.sh --recipe ascii-grid --max-mismatch-ratio 1 --max-mean-channel-delta 255`.
  The JSON summary includes `recipe`; `smoke` remains the default for Exp-39
  compatibility. (Exp 40.)
- **Color live A/B recipe:**
  `scripts/roastty-app/live-ab-smoke.sh --recipe color-grid --max-mismatch-ratio 1 --max-mean-channel-delta 255`
  prints deterministic ANSI palette, background, bold/bright, and truecolor
  rows. Strict thresholds currently fail with recorded metrics, as expected.
  (Exp 41.)
- **Clear-screen live A/B recipe:**
  `scripts/roastty-app/live-ab-smoke.sh --recipe clear-after --max-mismatch-ratio 1 --max-mean-channel-delta 255`
  prints pre-clear rows, emits `3J,H,2J`, then captures fixed post-clear rows.
  Strict thresholds currently fail with recorded metrics, as expected. (Exp 42.)
- **Alt-screen live A/B recipe:**
  `scripts/roastty-app/live-ab-smoke.sh --recipe alt-screen --max-mismatch-ratio 1 --max-mean-channel-delta 255`
  enters alternate screen mode, draws fixed cursor-addressed text, and captures
  while the alt screen is active. Strict thresholds currently fail with recorded
  metrics, as expected. (Exp 43.)
- **Scroll-output live A/B recipe:**
  `scripts/roastty-app/live-ab-smoke.sh --recipe scroll-output --max-mismatch-ratio 1 --max-mean-channel-delta 255`
  prints 80 deterministic rows and captures the settled bottom-of-output
  viewport. Strict thresholds currently fail with recorded metrics, as expected.
  (Exp 44.)
- **Run a live A/B recipe matrix:**
  `scripts/roastty-app/live-ab-matrix.sh --recipe ascii-grid --recipe clear-after`
  runs selected recipes with permissive thresholds by default, emits one JSON
  Lines summary per recipe, continues after failures, and exits nonzero if any
  selected recipe fails under the supplied thresholds. (Exp 45.)
- **Live A/B input delivery is not solved yet:** Exp 46 found that Command-V
  paste terminates the current Roastty app, while AppleScript and CGEvent
  keyboard text injection can leave the recipe unexecuted even though the
  permissive screenshot diff exits `0`. The harness now verifies the frontmost
  app before input and the recipes avoid `printf` format-string hazards, but the
  next Phase-D input step must make command delivery itself observable.
- **Live A/B recipe delivery is now launch-time bootstrap, not UI typing:** Exp
  47 launches each app binary directly with per-run `ZDOTDIR` and
  `XDG_CONFIG_HOME` temp dirs. Generated zsh/Nushell startup files run the
  selected recipe script, so every matrix recipe visibly executes in both apps
  without paste or synthetic keyboard input. Ghostty capture now uses the same
  full-screen crop path as Roastty.
- **Live A/B recipes hold their final frame through capture:** Exp 48 uses
  `${TERMSURF_AB_HOLD_SECONDS:-20}` so recipes do not return to different
  Ghostty/Roastty shell prompts before screenshot capture. Activation is
  verified against the exact launched target process's own `frontmost` property
  by Unix PID so full-screen crop captures fail instead of silently accepting
  occluded pixels.
- **Live A/B verdicts now default to terminal content-region diffs:** Exp 49
  keeps full-window metrics in JSON as `full_window_diff`, but gates on
  `content_region.diff` by default (`--comparison-region content`) using the
  configurable crop `${TERMSURF_AB_CONTENT_CROP_X:-0}`,
  `${TERMSURF_AB_CONTENT_CROP_Y:-132}`, `${TERMSURF_AB_CONTENT_CROP_W:-1600}`,
  `${TERMSURF_AB_CONTENT_CROP_H:-900}`. Use `--comparison-region full` for the
  legacy titlebar/debug-banner-inclusive verdict.
- **Unicode-width live A/B recipe:** Exp 50 adds `unicode-width`, a
  content-region recipe for guide columns, combining marks, CJK wide text,
  emoji/variation selectors, box/symbol glyphs, and cursor-addressed alignment.
  Current content metric: `mean_channel_delta=3.8124979166666666`,
  `mismatch_ratio=0.04077708333333333`; visual inspection shows the expected
  Roastty width/fallback differences, so the next Phase-E step should port the
  Unicode width/grapheme behavior behind this oracle.
- **Unicode property facade:** Exp 51 adds `roastty/src/unicode/` with a
  Ghostty-shaped `Properties` lookup (`width`, `width_zero_in_grapheme`,
  `grapheme_break`, `emoji_vs_base`) and representative width/grapheme tests.
  The full generated table and `unicode.graphemeBreak` state machine are still
  Phase-E work; the next slice should rewrite `Terminal::print()` against this
  API.
- **Terminal Unicode print widths:** Exp 52 rewires `Terminal::print()` to use
  the Rust Unicode facade for representative wide CJK/emoji cells, spacer tails,
  right-edge spacer heads, legacy zero-width attachments, and mode 2027 grapheme
  accumulation/variation selectors. The live `unicode-width` content metric
  remains passing (`mean_channel_delta=3.8088447916666666`,
  `mismatch_ratio=0.04076041666666667`); the remaining Phase-E gap is the full
  generated Unicode table/state-machine parity.
- **Generated Unicode table parity:** Exp 53 replaces the representative Unicode
  facade with committed Rust tables generated from Ghostty's pinned Unicode LUT
  and a generated no-control grapheme transition table. Regenerate and verify
  with `scripts/roastty-app/generate-unicode-tables.py --generate` and
  `scripts/roastty-app/generate-unicode-tables.py --check`; the property path
  requires Ghostty's generated `props.zig` cache, and the grapheme path runs the
  vendored uucode transition function through Zig 0.15. Normal
  `cargo test -p roastty` uses only committed Rust artifacts and does not
  require `vendor/ghostty`. Ghostty's table intentionally reports width `0` for
  combining marks and Hangul V/T codepoints, replacing the temporary Exp51
  standalone-width facade. The live `unicode-width` content metric remains
  passing (`mean_channel_delta=3.8097902777777777`,
  `mismatch_ratio=0.04077847222222222`).
- **Config-derived font-grid assembly:** Exp 55 moves live renderer font-grid
  construction through `font::shared_grid_set::build_grid_from_config`, using a
  `DerivedConfig`/`Key` built from the represented font-family, font-style,
  font-codepoint-map, and font-synthetic-style fields. The old hardcoded Menlo
  renderer path is gone, but Menlo remains the temporary default-primary
  fallback until embedded fallback fonts and the full font subsystem are ported.
- **Clipboard codepoint map is now app-facing:** Exp 56 represents
  `clipboard-codepoint-map` on `Config` and applies it only to copy-to-clipboard
  formatting, including styled HTML payloads. URL copying remains a direct URI
  write and intentionally bypasses the map.
- **Clipboard behavior config is app-facing:** Exp 57 represents
  `clipboard-trim-trailing-spaces`, `clipboard-paste-protection`,
  `clipboard-paste-bracketed-safe`, and `selection-clear-on-copy` on `Config`.
  They affect only app copy/paste behavior: trim and clear-on-copy apply to
  `copy_to_clipboard`, URL copying stays untouched, and paste confirmation now
  follows Ghostty's bracketed/unsafe-paste rules.
- **Selection behavior config is app-facing:** Exp 58 represents
  `selection-clear-on-typing` and `selection-word-chars` on `Config`. Mouse word
  selection and quicklook word lookup use configured word boundaries; text,
  raw-text, key, and preedit paths honor clear-on-typing, while Escape still
  clears selection even when clear-on-typing is disabled.

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
- **Embedded-ABI implementation pattern (Exp 8):** roastty's internals already
  match upstream value-for-value, so each tranche is mostly (a) expose the
  enums/structs in `roastty.h` byte-faithful (rename existing enums to ghostty's
  exact member names — e.g. `KEY_A`/`DIGIT_0` — values unchanged; alias when an
  equivalent enum exists), (b) thin by-value `#[repr(C)]` + `extern "C"` entries
  that build the internal type and call the existing path, (c) **the real cost:
  migrate roastty's tests off the interim opaque/handle API** (rename the old
  export to `*_handle`, sed the test call sites). Add a `size_of`/`offset_of`
  layout test per struct. `cargo build` only checks the lib — run
  `cargo test --lib` to compile+check the migrated tests.
- **Typed-union ABI without test churn (Exp 9):** when a struct switches from an
  opaque carrier (`int tag; uintptr_t storage[8]`) to a typed tagged union, do
  the `storage→union` conversion at the ONE C-callback boundary (the binding
  path is type-erased, so per-site rewrites are impossible anyway), and add a
  **test-only reverse `union→storage`** so existing `storage[N]` assertions
  round-trip the real conversion untouched. Reuse existing roastty enum type
  names in union members (a blind `ghostty_→roastty_` import re-emits
  enumerators → C redefinition). Pin layout with BOTH Rust `offset_of!` and C
  `_Static_assert` so a Rust↔header drift fails at compile time. `ssize_t` needs
  `#include <sys/types.h>`.
- **Mouse behavior config (Exp 59):** `mouse-reporting`,
  `mouse-scroll-multiplier`, and `click-repeat-interval` now finalize before
  they reach `App`/`Surface`. `click-repeat-interval = 0` becomes `500` ms, and
  both scroll multipliers clamp to `[0.01, 10000.0]`. Surfaces cache
  `click_repeat_interval_ns`, so runtime gesture timing should use the cached
  nanoseconds value rather than re-reading config milliseconds.
- **Link URL / maximize config (Exp 67):** `link-url` and `maximize` now
  parse/format as faithful bool surfaces. Runtime URL matching and startup
  window maximization remain later app/link wiring work.
- **Class config (Exp 68):** `class` and `x11-instance-name` now parse/format as
  faithful optional string surfaces. Runtime GTK/X11/Wayland identity behavior
  remains later app-runtime wiring work.
- **Working-directory config (Exp 69):** `working-directory` now parse/formats
  as a faithful optional `WorkingDirectory` surface. Config-file
  `working-directory =` is the empty-reset path; a missing value line
  `working-directory` reports `ValueRequired`. Finalize-time probable-CLI/home
  defaulting remains later config-finalize work.

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
- [x] Live-A/B compare (real app vs roastty app, same run) — replaces committed
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
- [x] **Make it compile/link — the embedded ABI type surface (Exp 8-13): DONE.**
      The renamed Roastty app **compiles + links** against `libroastty`
      (`** BUILD SUCCEEDED **`); the entire embedded ABI is byte-faithful.
      Original notes: Exp 8 (input) + Exp 9 (action: 36 types + typed `action_u`
      union) + Exp 10 (config/fn tail + mouse/action/init ABI fixes) done —
      **all 56 missing symbols resolved**, 4396 tests green. The app build now
      reaches **past every missing-symbol + enum + init issue** and is blocked
      on the **`selection_s`/`point_s` layout divergence** (Exp-6 #3 → Exp 11).
      The build exposed the real gap = **56 missing `roastty_*` symbols**,
      dominated by the **~36 `action_*` payload types/enums** (the `action_s`
      tagged-union members) + 6 input types/enums + 4 config types + 6 functions
      — plus the `selection_s`/`point_s` subsystem divergence. Implement
      byte-faithful in `libroastty`/`roastty.h`, drive the app's error list to
      zero. (Spans several gated experiments.)

**Phase C — Live render path (the crux)**

- [ ] `surface_draw` owns a Metal renderer bound to the app `NSView`/`CALayer`;
      attach the layer and present on-screen
- [ ] Render thread (frame pacing + cursor-blink timer)
- [ ] Renderer mailbox / `Options` (focus / visible / occlusion / change-config)
- [ ] Retire the interim `render_state` pull divergence
- [ ] **Milestone: the app launches and shows a working ASCII terminal**

**Phase D — Automated UI tests for the roastty-backed app**

- [x] Point the Phase-A harness at the renamed roastty-backed app
- [x] Golden-diff its screenshots/behavior against the Phase-A real-Ghostty
      baseline
- [x] Repeatable in-session run, wired so every later phase is regression-tested
      (headless/CI automation is a separate, later concern — see Exp 2's caveat)

**Phase E — Terminal correctness**

- [x] Port `unicode/` tables (grapheme-break, codepoint-width, symbol/Nerd-Font
      width)
- [x] Rewrite `Terminal::print()` (width lookup + grapheme accumulation;
      mode 2027)

**Phase F — Config completeness**

- [ ] The remaining ~140 config options (font, palette, link, command,
      cursor/mouse, scrollback, `macos-*`, …)
- [ ] `finalize()` — cross-field validation / derivation / clamping
- [ ] Theme loading (themes-dir locator + file read + palette/option
      application)
- [ ] Conditional state wiring (`changeConditionalState` + conditional reload)
- [x] `font-codepoint-map` + `clipboard-codepoint-map` as config fields
- [x] `SharedGridSet` config→font assembly (`Key`/`DerivedConfig` → discovery →
      populated `Collection`), replacing the hardcoded-"Menlo" test path

**Phase G — Input / keybindings**

- [ ] Multi-key sequences / chords (the trie), leader keys, key tables —
      configured root and active-table surface sequences are wired (Exp
      118–120), but `ignore`, `end_key_sequence`, `chain=`, native
      keymaps/global shortcuts, app-key sequence/table handling, broader
      global/all routing, and the full upstream binding catalog remain later
      work
- [ ] Trigger-prefix flags (`global:` / `all:` / `unconsumed:` / `performable:`)
      — parser/storage/query metadata and surface unconsumed/performable
      consumption are wired (Exp 110–111), and configured `global:` app-key
      dispatch plus focused app-scoped app-key dispatch are wired (Exp 113–114),
      but native global shortcut registration and broader global/all routing
      remain later work
- [x] `catch_all` trigger parsing and fallback lookup for configured single-key
      bindings (Exp 115)
- [ ] The full action set + the default-bindings data table + reverse
      action→trigger mapping — the macOS single-key table foundation and reverse
      lookup are wired for currently supported actions (Exp 112), but the full
      upstream table/actions remain later work
- [ ] Command-palette catalog (`command.zig`)
- [ ] Native keymaps (`keycodes`, `KeymapDarwin`) + app-level key handling —
      `RemapSet`/`Mask`, the `key-remap` config field, and surface runtime
      key-remap application are wired (Exp 107–109), and configured `global:`
      plus focused app-scoped `roastty_app_key` dispatch is wired (Exp 113–114),
      but native keymaps, keyboard-layout reload, and full upstream keybinding
      tables remain later work

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
- [Experiment 8: Embedded ABI — the input type surface (tranche 1)](08-embedded-abi-input.md)
  — **Pass** (input enums byte-faithful + by-value
  `input_key_s`/`surface_key`/`app_key`; 4395 tests green; gap 56→48) ·
  Claude/Claude
- [Experiment 9: Embedded ABI — the action-dispatch type surface (tranche 2)](09-embedded-abi-action.md)
  — **Pass** (36 action types + typed `action_u` union byte-faithful, central
  storage→union conversion, readonly swap fixed; 4396 tests green; gap 48→11) ·
  Claude/Claude
- [Experiment 10: Embedded ABI — the config + function tail (tranche 3)](10-embedded-abi-config-tail.md)
  — **Partial** (6 config types + 4 fn stubs + mouse/action/init ABI fixes; all
  11 symbols resolved, 4396 tests green; app build now reaches the
  `selection_s`/`point_s` divergence → Exp 11) · Claude/Claude
- [Experiment 11: Embedded ABI — the selection/point layout divergence (Exp-6 #3)](11-embedded-abi-selection.md)
  — **Pass** (embedded `point_s`/`selection_s`/`point_coord_e` byte-faithful +
  the `(tag,coord)`→pin resolver in `read_text`; 4399 tests green; app compiles
  past selection → `target_s`/`action_tag_e` next) · Claude/Claude
- [Experiment 12: Embedded ABI — the target union + the action-tag completion](12-embedded-abi-target-tags.md)
  — **Pass** (`target_s` `target_u` union + 24 `ROASTTY_ACTION_*` tags
  byte-faithful; 4400 tests green; app build 80→1 errors →
  `config_key_is_binding` by-value next) · Claude/Claude
- [Experiment 13: Embedded ABI — `config_key_is_binding` by-value (the last compile error)](13-embedded-abi-config-key.md)
  — **Pass** (`config_key_is_binding` by-value; **the app COMPILES + LINKS** —
  `** BUILD SUCCEEDED **`, Roastty.app produced, Phase B exit; 4401 tests green)
  · Claude/Claude
- [Experiment 14: Phase C — launch Roastty.app and capture what it renders](14-launch-roastty-app.md)
  — **Pass** (the app **launches cleanly** — no crash/panic — but renders blank;
  root cause: `surface_draw` is a stub, the live NSView present path (801 crux)
  is unwired → Exp 15; spawned app killed, 0 dangling) · Claude/Claude
- [Experiment 15: Phase C — the live present path (the 801 crux), slice 1](15-live-present-path.md)
  — **Partial** (live present path wired + the Metal IOSurface layer ATTACHES to
  the app NSView — window white→black, build -> Some(1600x1136); but no frame
  yet: surface_new doesn't auto-start the shell → Exp 16; 4401 tests green) ·
  Claude/Claude
- [Experiment 16: Phase C — `surface_new` auto-starts the IO (the shell-start divergence)](16-surface-new-autostart.md)
  — **Pass** (`surface_new` auto-starts the IO gated on `platform_tag == MACOS`
  — launched app spawns a live `/bin/zsh`; ALSO restored `abi_harness`, silently
  broken since Exp 8 by `--lib`-only testing: 141 compile errors + the readonly
  assert; full `cargo test` green, 0 shell leaks) · Claude
- [Experiment 17: Phase C — atlas coherence (sample the grid's glyph atlas)](17-atlas-coherence.md)
  — **Partial** (present now samples the grid's rasterized atlas — proven by a
  discriminating GPU-readback test; but live text ALSO needs the
  projection/screen-size uniforms, never wired → Exp 18) · Claude
- [Experiment 18: Phase C — wire the projection/screen-size uniforms (live text)](18-projection-uniforms.md)
  — **Pass** (drives the projection/screen-size uniforms from the surface,
  Retina-correct — the launched app renders the live **shell prompt as text**;
  first real terminal frame from libroastty) · Claude
- [Experiment 19: Phase C — a continuous present driver (live updates)](19-present-driver.md)
  — **Pass** (main-thread ~60fps driver drains tick_termio + presents on dirty —
  the terminal is LIVE: typed `echo TERMSURF_LIVE` + its output render live;
  suite 4403+1 green, idle-efficient, clean shutdown) · Claude
- [Experiment 20: Phase C — conformance smoke test (map the feature landscape)](20-conformance-smoke.md)
  — **Pass** (6 probes via ZDOTDIR drive: scroll / colors+truecolor / alt-screen
  / cursor-addressing / resize all WORK; gaps — `clear` drops post-clear content
  (→Exp 22), CJK+emoji tofu / no font fallback (→Exp 21); selection+scrollback
  deferred) · Claude
- [Experiment 21: Phase C — enable font-fallback discovery (CJK + emoji)](21-font-fallback.md)
  — **Pass** (enabled the resolver's discovery fallback in `build_live_renderer`
  — CJK `日本語` renders + `🎉` in COLOR vs `?` before; 4403+1 green; CJK
  wide-pitch fine-tune a noted follow-up) · Claude
- [Experiment 22: Phase C — diagnose + fix the `clear` gap](22-clear-screen.md)
  — **Pass** (root cause: `\033[3J` erase-scrollback errored `InvalidPoint` with
  no history → aborted the slice → post-clear content dropped; fixed to no-op
  (upstream-faithful) + regression test, 4404 green; live re-probe CONFIRMS
  post-clear content renders) · Claude
- [Experiment 23: Phase C — scrollback navigation (deferred Exp-20 probe)](23-scrollback.md)
  — **Pass** (wheel scrollback works live — fixed 3 bugs: mouse_scroll never
  scrolled the viewport, the reporting-gate used a coarse always-true flag, and
  the render read-path read the active bottom not the viewport
  (`Point::active`→`viewport`); 4405 green + CGEvent scroll driver) · Claude
- [Experiment 24: Phase C — suppress the cursor when scrolled into scrollback](24-cursor-in-scrollback.md)
  — **Pass** (a stray cursor block rendered on scrollback history rows; fixed
  with a pin-based `Terminal::cursor_viewport_position()` feeding both
  cursor-block-draw sites — `None` when scrolled off-viewport, faithful to
  upstream `cursor.viewport`; 4406 green + live-confirmed) · Claude
- [Experiment 25: Phase C — mouse-drag text selection (deferred Exp-20 probe)](25-mouse-selection.md)
  — **Pass** (mouse-drag selection was unwired; wired the `SelectionGesture`
  into the core `mouse_button`/`mouse_pos`, viewport-pin-anchored so it works in
  scrollback; headless 2-case test + 4408 green + live highlight) · Claude
- [Experiment 26: Phase C — clipboard copy of a selection (deferred Exp-20 probe)](26-clipboard-copy.md)
  — **Pass** (copy was already wired + unit-tested; added the missing
  drag-gesture→copy integration test + live proof — drag-select then Edit▸Copy
  lands the text on NSPasteboard, `pbpaste` confirms; 4409 green) · Claude
- [Experiment 27: Phase C — double/triple-click word & line selection](27-word-line-selection.md)
  — **Pass** (Exp-25 passed `time_ns: None` so click-count was stuck at 1/Cell;
  gave the Surface a monotonic clock + an injectable test clock → double-click
  word, triple-click line; 4410 green deterministic + live word/line highlight)
  · Claude
- [Experiment 28: Phase C — drag-selection autoscroll past the edge](28-drag-autoscroll.md)
  — **Pass** (gesture set `autoscroll` but nothing called `autoscroll_tick`;
  wired a tick into the present loop + clamped `selection_drag` past-edge
  positions so a held drag-above-edge scrolls into history + extends; 4411
  green + live 78→55 scroll w/ highlight) · Claude
- [Experiment 29: Phase C — CJK ideographic wide-pitch (`set_point_size`)](29-cjk-wide-pitch.md)
  — **Partial** (wired set_point_size in build_live_renderer so discovered CJK
  faces get the IcWidth ideographic resize; 4411 green + design-review-confirmed
  load-bearing/no-regression; live CJK width comparison pending — screen locked)
  · Claude
- [Experiment 30: Phase C — shift-click extends the selection](30-shift-click-extend.md)
  — **Partial** (shift-click extends the selection from its anchor when >500ms
  since the last press; not-reporting press branches to selection_drag; 2
  headless tests + 4413 green deterministic (also fixed a latent Exp-27
  double_click flaky test); live shift-click pending — screen locked) · Claude
- [Experiment 31: Phase C — viewport-gate the cursor run-shaping hint](31-cursor-hint-viewport.md)
  — **Pass** (Exp-24 loose end: the `shape_run_options` cursor run-shaping hint
  used active `cy==y`, breaking a ligature on a scrolled history row; gated on
  the viewport via `cursor_viewport_row`; 4414 green, fully headless) · Claude
- [Experiment 32: Phase C — widen the reporting-mode selection clear+reset](32-reporting-clear-widen.md)
  — **Pass** (hoisted the reporting-mode selection clear+reset out of the
  Left-only branch → any button + press/release clears while reporting, faithful
  to upstream; 4415 green, fully headless) · Claude
- [Experiment 33: Phase C — shift overrides mouse-reporting for selection](33-shift-while-reporting.md)
  — **Partial** (shift overrides mouse-reporting for selection — shift-drag
  selects in a mouse-mode TUI + suppresses the report (button-gated so bare
  motion still reports); flag-first shiftCapture (config deferred); 4-case
  headless test + 4416 green; live shift-drag pending — screen locked) · Claude
- [Experiment 34: Phase C — plumb the `mouse-shift-capture` config into `shiftCapture`](34-shift-capture-config.md)
  — **Pass** (plumbed `mouse-shift-capture` config (parsed → App →
  `capture_shift`) into the full `mouseShiftCapture` logic;
  Never/Always/flag/default all honored; 4417 green, fully headless — closes
  Exp-33's config sub-deferral) · Claude
- [Experiment 35: Phase C — rebuild the renderer on a DPI (content-scale) change](35-dpi-change-rebuild.md)
  — **Partial** (`set_content_scale` now drops the renderer on a DPI change so
  present_live rebuilds at the new scale — no more stale-DPI blur after a
  monitor move; headless change-detection test + 4418 green; live re-sharpen
  pending — screen locked) · Claude
- [Experiment 36: Phase C — report color-scheme changes live (DECSET 2031)](36-color-scheme-change-report.md)
  — **Pass** (mode 2031 now reports OS theme changes live — `set_color_scheme`
  emits `997;1n`/`997;2n` on a change via the new
  `Terminal::report_color_scheme_change`, gated on 2031, change-only;
  deterministic terminal-level test + 4419 green, fully headless) · Claude
- [Experiment 37: Phase C — in-band size reports (DECSET 2048)](37-in-band-size-reports.md)
  — **Pass** (mode 2048 in-band size reports now emit on enable
  (`set_mode_basic`) + on resize (`set_size`→`report_in_band_size`) — was
  registered+encoded but never emitted; deterministic terminal-level test + 4420
  green, fully headless) · Claude
- [Experiment 38: Phase D — screenshot diff metric for live A/B checks](38-screenshot-diff-metric.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 39: Phase D — live A/B smoke harness](39-live-ab-smoke-harness.md)
  — **Pass**
- [Experiment 40: Phase D — named live A/B recipes](40-live-ab-recipes.md) —
  **Pass**
- [Experiment 41: Phase D — color live A/B recipe](41-live-ab-color-recipe.md) —
  **Pass**
- [Experiment 42: Phase D — clear-screen live A/B recipe](42-live-ab-clear-recipe.md)
  — **Pass**
- [Experiment 43: Phase D — alt-screen live A/B recipe](43-live-ab-alt-screen-recipe.md)
  — **Pass**
- [Experiment 44: Phase D — scroll-output live A/B recipe](44-live-ab-scroll-output-recipe.md)
  — **Pass**
- [Experiment 45: Phase D — live A/B recipe matrix runner](45-live-ab-recipe-matrix.md)
  — **Pass**
- [Experiment 46: Phase D — paste-safe live A/B recipe input](46-live-ab-paste-safe-recipes.md)
  — **Partial**
- [Experiment 47: Phase D — launch-time live A/B recipe bootstrap](47-live-ab-launch-bootstrap.md)
  — **Pass**
- [Experiment 48: Phase D — hold live A/B recipe frames through capture](48-live-ab-held-recipe-frame.md)
  — **Pass**
- [Experiment 49: Phase D — content-region live A/B diffs](49-live-ab-content-region-diff.md)
  — **Pass**
- [Experiment 50: Phase E — Unicode-width live A/B recipe](50-live-ab-unicode-width-recipe.md)
  — **Pass**
- [Experiment 51: Phase E — Unicode width properties](51-unicode-width-properties.md)
  — **Pass**
- [Experiment 52: Phase E — Terminal print Unicode width](52-terminal-print-unicode-width.md)
  — **Pass**
- [Experiment 53: Phase E — Unicode table and grapheme parity](53-unicode-table-parity.md)
  — **Pass**
- [Experiment 54: Phase F — font config surface](54-font-config-surface.md) —
  **Pass**
- [Experiment 55: Phase F — SharedGridSet config assembly](55-shared-grid-config-assembly.md)
  — **Pass**
- [Experiment 56: Phase F — clipboard codepoint map](56-clipboard-codepoint-map.md)
  — **Pass**
- [Experiment 57: Phase F — clipboard behavior config](57-clipboard-behavior-config.md)
  — **Pass**
- [Experiment 58: Phase F — selection behavior config](58-selection-behavior-config.md)
  — **Pass**
- [Experiment 59: Phase F — mouse behavior config](59-mouse-behavior-config.md)
  — **Pass**
- [Experiment 60: Phase F — cursor default config](60-cursor-default-config.md)
  — **Pass**
- [Experiment 61: Phase F — split visual config surface](61-split-visual-config-surface.md)
  — **Pass**
- [Experiment 62: Phase F — search color config](62-search-color-config.md) —
  **Pass**
- [Experiment 63: Phase F — command config surface](63-command-config-surface.md)
  — **Pass**
- [Experiment 64: Phase F — env config surface](64-env-config-surface.md) —
  **Pass**
- [Experiment 65: Phase F — scalar launch config](65-scalar-launch-config.md) —
  **Pass**
- [Experiment 66: Phase F — scrollbar config](66-scrollbar-config.md) — **Pass**
- [Experiment 67: Phase F — link URL and maximize config](67-link-url-maximize-config.md)
  — **Pass**
- [Experiment 68: Phase F — class config](68-class-config.md) — **Pass**
- [Experiment 69: Phase F — working-directory config](69-working-directory-config.md)
  — **Pass**
- [Experiment 70: Phase F — window padding config](70-window-padding-config.md)
  — **Pass**
- [Experiment 71: Phase F — window padding balance config](71-window-padding-balance-config.md)
  — **Pass**
- [Experiment 72: Phase F — window scalar config](72-window-scalar-config.md) —
  **Pass**
- [Experiment 73: Phase F — window size and step resize config](73-window-size-step-config.md)
  — **Pass**
- [Experiment 74: Phase F — window tab and titlebar config](74-window-tab-titlebar-config.md)
  — **Pass**
- [Experiment 75: Phase F — resize overlay config](75-resize-overlay-config.md)
  — **Pass**
- [Experiment 76: Phase F — focus follows mouse config](76-focus-follows-mouse-config.md)
  — **Pass**
- [Experiment 77: Phase F — title report and image limit config](77-title-report-image-limit-config.md)
  — **Pass**
- [Experiment 78: Phase F — quit delay config](78-quit-delay-config.md) —
  **Pass**
- [Experiment 79: Phase F — undo timeout config](79-undo-timeout-config.md) —
  **Pass**
- [Experiment 80: Phase F — quick terminal position config](80-quick-terminal-position-config.md)
  — **Pass**
- [Experiment 81: Phase F — quick terminal size config](81-quick-terminal-size-config.md)
  — **Pass**
- [Experiment 82: Phase F — GTK quick terminal config](82-gtk-quick-terminal-config.md)
  — **Pass**
- [Experiment 83: Phase F — quick terminal screen and animation config](83-quick-terminal-screen-animation-config.md)
  — **Pass**
- [Experiment 84: Phase F — quick terminal space and keyboard config](84-quick-terminal-space-keyboard-config.md)
  — **Pass**
- [Experiment 85: Phase F — command palette entry config](85-command-palette-entry-config.md)
  — **Pass**
- [Experiment 86: Phase F — VT KAM config and key gate](86-vt-kam-config-key-gate.md)
  — **Pass**
- [Experiment 87: Phase F — custom shader config](87-custom-shader-config.md) —
  **Pass**
- [Experiment 88: Phase F — bell features config](88-bell-features-config.md) —
  **Pass**
- [Experiment 89: Phase F — app notifications config](89-app-notifications-config.md)
  — **Pass**
- [Experiment 90: Phase F — macOS icon config](90-macos-icon-config.md) —
  **Pass**
- [Experiment 91: Phase F — macOS Shortcuts config](91-macos-shortcuts-config.md)
  — **Pass**
- [Experiment 92: Phase F — Linux cgroup config](92-linux-cgroup-config.md) —
  **Pass**
- [Experiment 93: Phase F — GTK chrome config](93-gtk-chrome-config.md) —
  **Pass**
- [Experiment 94: Phase F — GTK CSS, notifications, and progress config](94-gtk-css-notifications-progress-config.md)
  — **Pass**
- [Experiment 95: Phase F — TERM and enquiry-response config](95-term-enquiry-config.md)
  — **Pass**
- [Experiment 96: Phase F — async backend and auto-update config](96-async-update-config.md)
  — **Pass**
- [Experiment 97: Phase F — config finalize scalar tail](97-config-finalize-scalar-tail.md)
  — **Pass**
- [Experiment 98: Phase F — config replay foundation](98-config-replay-foundation.md)
  — **Pass**
- [Experiment 99: Phase F — absolute theme loading](99-absolute-theme-loading.md)
  — **Pass**
- [Experiment 100: Phase F — named theme lookup](100-named-theme-lookup.md) —
  **Pass**
- [Experiment 101: Phase F — conditional theme reload](101-conditional-theme-reload.md)
  — **Pass**
- [Experiment 102: Phase F — working-directory finalize](102-working-directory-finalize.md)
  — **Pass**
- [Experiment 103: Phase F — command and home finalize](103-command-home-finalize.md)
  — **Pass**
- [Experiment 104: Phase F — GTK single-instance finalize](104-gtk-single-instance-finalize.md)
  — **Pass**
- [Experiment 105: Phase F — quit-delay finalize warning](105-quit-delay-finalize-warning.md)
  — **Pass**
- [Experiment 106: Phase F — link-url finalize](106-link-url-finalize.md) —
  **Pass**
- [Experiment 107: Phase G — key-remap set foundation](107-key-remap-set-foundation.md)
  — **Pass**
- [Experiment 108: Phase G — key-remap config field](108-key-remap-config-field.md)
  — **Pass**
- [Experiment 109: Phase G — key-remap runtime application](109-key-remap-runtime-application.md)
  — **Pass**
- [Experiment 110: Phase G — keybind trigger prefix flags](110-keybind-trigger-prefix-flags.md)
  — **Pass**
- [Experiment 111: Phase G — configured binding consumption](111-configured-binding-consumption.md)
  — **Pass**
- [Experiment 112: Phase G — default binding table foundation](112-default-binding-table-foundation.md)
  — **Pass**
- [Experiment 113: Phase G — app key global binding dispatch](113-app-key-global-binding-dispatch.md)
  — **Pass**
- [Experiment 114: Phase G — focused app key app actions](114-focused-app-key-app-actions.md)
  — **Pass**
- [Experiment 115: Phase G — catch-all keybind triggers](115-catch-all-keybind-triggers.md)
  — **Pass**
- [Experiment 116: Phase G — key-table syntax storage](116-key-table-syntax-storage.md)
  — **Pass**
- [Experiment 117: Phase G — key-table runtime activation](117-key-table-runtime-activation.md)
  — **Pass**
- [Experiment 118: Phase G — sequence syntax storage](118-sequence-syntax-storage.md)
  — **Pass**
- [Experiment 119: Phase G — sequence runtime activation](119-sequence-runtime-activation.md)
  — **Pass**
- [Experiment 120: Phase G — key-table sequence runtime](120-key-table-sequence-runtime.md)
  — **Pass**

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
