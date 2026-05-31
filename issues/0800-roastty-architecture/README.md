+++
status = "open"
opened = "2026-05-31"
+++

# Issue 800: Roastty Architecture and ABI Skeleton

## Goal

Document the planned architecture for Roastty, implement the first Rust ABI
skeleton for `libroastty`, and set the stage for the remaining Ghostty-to-Rust
rewrite.

Roastty is a macOS-first Rust reimplementation of Ghostty. The initial target is
feature parity with Ghostty as a terminal emulator, before adding TermSurf's
browser-overlay features.

## Background

TermSurf currently uses Wezboard, a WezTerm fork, as the active GUI. Wezboard
works, but recent work exposed architectural friction in areas that matter to
TermSurf: grid-native borders, pane geometry, browser overlays, and split
presentation all required careful changes inside a terminal architecture that
was not designed around TermSurf's browser-overlay model.

Ghostty has a more promising shape for a future TermSurf GUI:

- a native macOS frontend written in Swift/AppKit/SwiftUI;
- a reusable terminal core exposed through `libghostty`;
- a C ABI boundary between the native app and the terminal engine;
- a surface model where the app runtime decides whether a terminal surface is a
  window, tab, split, preview pane, or something else;
- a Metal renderer on macOS;
- a clean separation between terminal state, PTY/IO, rendering, fonts, config,
  and native application UI.

The vendored Ghostty repository is tracked under `vendor/ghostty/`. As of the
start of this issue, it has been updated to upstream main at:

`2c62d182c gtk: fix context menu hiding quick-terminal (#12843)`

## Planned Architecture

Roastty should not begin as a from-scratch macOS application rewrite. The
practical path is to use Ghostty's Swift macOS app as the reference frontend,
rename/adapt it into the Roastty macOS app, and gradually replace the Zig
`libghostty` implementation with a Rust `libroastty` implementation behind a C
ABI.

The target architecture is:

```text
┌────────────────────────────────────────────────────────────┐
│ Roastty Swift macOS app                                    │
│ windows, tabs, splits, menus, settings, native events      │
└─────────────────────────────┬──────────────────────────────┘
                              │ C ABI
┌─────────────────────────────┴──────────────────────────────┐
│ libroastty                                                 │
│ Rust replacement for the parts of Ghostty used by macOS    │
├────────────────────────────────────────────────────────────┤
│ App / Surface handles                                      │
│ Config, actions, callbacks, lifecycle                      │
├────────────────────────────────────────────────────────────┤
│ Terminal core                                              │
│ parser, screen/grid, scrollback, selection, input encoding │
├────────────────────────────────────────────────────────────┤
│ PTY / terminal IO                                          │
│ shell spawn, read/write threads, resize, child lifecycle   │
├────────────────────────────────────────────────────────────┤
│ Render state and renderer                                  │
│ terminal-to-render rows, Metal layer, glyph atlas, cursor  │
├────────────────────────────────────────────────────────────┤
│ Font and text stack                                        │
│ CoreText discovery, fallback, shaping, metrics, emoji      │
└────────────────────────────────────────────────────────────┘
```

The first milestone is not a working terminal. It is an ABI skeleton: a Rust
library that exposes enough `roastty_*` C symbols for the renamed Swift app, or
a small compatibility harness, to load and exercise app/config/surface lifecycle
without linking Zig `libghostty`.

## Naming and Attribution Policy

Roastty is a faithful Rust adaptation of Ghostty where possible, but it is a
renamed project.

All new Roastty code, public C ABI symbols, Swift app calls, type names,
filenames, bundle identifiers, product names, user-visible text, tests, and
documentation for the new implementation must use `roastty` / `Roastty` naming.
For example, the Rust ABI should export `roastty_app_new`,
`roastty_surface_new`, and `roastty_config_new`, not compatibility `ghostty_*`
symbols.

The word `ghostty` should appear only when citing the original upstream project,
vendored reference paths, or credit/attribution text. Before release, Roastty
must include appropriate credit to the Ghostty project, but credit does not mean
keeping Ghostty names in Roastty's app or ABI.

This naming policy is a functional requirement, not cosmetic cleanup. An
experiment that preserves `ghostty_*` as the app-facing ABI has failed the
Roastty architecture goal, even if it compiles and passes lifecycle tests.

## Ghostty Reference Layers

The relevant Ghostty layers are:

- `vendor/ghostty/include/ghostty.h` — full C ABI used by the Swift app:
  `ghostty_app_t`, `ghostty_surface_t`, config handles, input, mouse, clipboard,
  actions, lifecycle, and rendering-related callbacks.
- `vendor/ghostty/include/ghostty/vt.h` — lower-level virtual terminal library
  API. This is the best reference for Roastty's terminal-core boundary.
- `vendor/ghostty/src/App.zig` — core app state, surface list, focus, config
  propagation, and mailbox tick loop.
- `vendor/ghostty/src/Surface.zig` — the central terminal surface abstraction:
  PTY, renderer, renderer state, terminal IO, input, mouse, search, inspector,
  focus, config, and child process lifecycle.
- `vendor/ghostty/src/apprt.zig` and `vendor/ghostty/src/apprt/embedded.zig` —
  application-runtime abstraction. On macOS, Ghostty builds as an embedded
  library and the Swift app supplies runtime callbacks.
- `vendor/ghostty/src/terminal/` — terminal emulator state: parser, screens,
  scrollback, selection, OSC/CSI/DCS handling, mouse/key encoding, Kitty
  graphics, and render-state extraction.
- `vendor/ghostty/src/termio/` — PTY/subprocess/IO threads.
- `vendor/ghostty/src/renderer/` — renderer abstraction and Metal backend.
- `vendor/ghostty/src/font/` — font discovery, CoreText backend, shaping,
  fallback, glyph atlas, metrics, emoji, and sprite handling.
- `vendor/ghostty/macos/Sources/` — Swift native macOS app, including tabs,
  splits, terminal views, settings, menus, quick terminal, AppleScript,
  AppIntents, native input, pasteboard, and platform integration.

## Rewrite Order

The rewrite should proceed in layers. Each layer should be verified before the
next layer is designed.

1. **ABI inventory and skeleton**

   Identify the minimum Roastty-renamed API surface the macOS app needs to
   create an app, create a surface, resize it, focus it, tick the event loop,
   and destroy it. Implement no-op or stubbed Rust versions behind a `roastty_*`
   C ABI.

2. **Config and lifecycle**

   Implement enough config loading, diagnostics, cloning, and cleanup for the
   Swift app to launch without depending on Zig config internals.

3. **Terminal core**

   Reimplement the virtual terminal state: parser, screen/grid, scrollback,
   cursor, style, SGR/OSC/CSI/DCS handling, selection, resize/reflow, and
   terminal input encoding.

4. **PTY and IO**

   Spawn the shell, manage the PTY, resize it, read output, write input, track
   foreground process information, and report child exit.

5. **Render state**

   Convert terminal state into a renderer-friendly row/cell/cursor/selection
   model. This should be validated independently before full Metal parity.

6. **Minimal macOS renderer**

   Render a basic text grid into the Roastty surface. The first renderer can be
   plain; the goal is correctness and ABI integration, not final performance.

7. **Metal renderer parity**

   Port the production renderer pieces: Metal layer setup, command queues,
   textures, glyph atlas, cursor rendering, selections, decorations, and redraw
   scheduling.

8. **Font and text parity**

   Implement CoreText discovery, font fallback, shaping, ligatures, emoji, Nerd
   Font glyphs, cell metrics, and glyph caching.

9. **Input, keybindings, mouse, and clipboard**

   Match Ghostty's keyboard layout handling, keybinding action model, mouse
   modes, scroll behavior, paste behavior, OSC 52, IME/preedit, and clipboard
   confirmation flow.

10. **Native app feature parity**

    Bring the Swift frontend back to parity with Ghostty behavior: tabs, splits,
    zoom, quick terminal, settings, menus, command palette, AppleScript,
    AppIntents, notifications, secure input, inspector, and update-adjacent
    behavior where relevant.

11. **TermSurf integration**

    Add TermSurf browser overlays, protocol routing, Roamium integration, and
    browser/terminal surface composition only after Roastty is a credible
    Ghostty-compatible terminal on macOS.

## Test Parity Requirement

Roastty must reimplement Ghostty behavior and Ghostty's relevant tests, not just
the library shape.

For every subsystem rewritten from Zig to Rust, the corresponding experiment
must identify the Ghostty tests, fixtures, examples, and reference behavior that
prove the subsystem. The experiment must then do one of the following:

- port the relevant Ghostty tests to Rust;
- run the existing Ghostty tests or fixtures against Roastty through a
  compatibility harness;
- create an equivalent Rust test when the original test cannot be reused
  directly;
- explicitly document why a Ghostty test is not applicable to Roastty.

A subsystem is not complete merely because it compiles, links, exposes matching
C ABI symbols, or appears to work manually. It is complete only when the Roastty
implementation is backed by equivalent automated tests for the behavior being
claimed.

This applies to terminal parsing, screen/grid state, scrollback, selection,
input encoding, PTY/IO behavior, render-state extraction, renderer behavior,
font/text handling, config parsing, keybindings, mouse handling, clipboard,
IME/preedit, and native app integration.

The long-term goal is an equivalent terminal implementation named Roastty, not a
thin ABI-compatible shell around unrelated behavior.

## Non-Goals

- Do not add TermSurf browser overlays before terminal parity work has a stable
  surface model.
- Do not rewrite the Swift app first. The Swift app is valuable reference UI and
  should initially be reused or lightly adapted.
- Do not attempt a big-bang full Ghostty rewrite. Each subsystem needs an
  experiment, verification, and result.
- Do not claim compatibility from symbol presence alone. A stub ABI only proves
  linking and lifecycle shape, not terminal behavior.
- Do not target Linux, Windows, or a full cross-platform story during the first
  Roastty phase. This issue is macOS-first.

## Experiment Review Gate

Every remaining experiment in this issue must pass review by another AI agent
before the work moves forward. Codex is the default reviewer unless the user
explicitly asks for a different reviewing agent.

For each experiment:

1. **Design review before implementation**
   - After the experiment file is written, run AI review on the experiment
     design.
   - Fix all real issues found by the review.
   - Do not implement the experiment until the reviewing agent agrees the design
     is good.
   - Record the review result in the experiment file.
   - Commit the reviewed and approved experiment plan as its own commit before
     implementation begins.

2. **Completion review after implementation**
   - After implementation, verification, and result recording, run AI review on
     the completed experiment.
   - Fix all real issues found by the review.
   - Do not design or implement the next experiment until the reviewing agent
     agrees the completed output is good.
   - Record the completion-review result in the experiment file.
   - Commit the reviewed and approved experiment result as its own commit before
     designing the next experiment.

The design commit and result commit must be separate. Do not combine an
experiment plan and its result in one commit.

The review gate applies starting with Experiment 2. Experiment 1 was designed,
implemented, verified, and committed before this gate was added. A retrospective
review can be added later if needed, but it is not required before proceeding to
Experiment 2.

## Expected First Experiment

The first experiment should create the top-level Rust workspace that will host
Roastty and TermSurf-owned shared Rust crates while keeping Wezboard separate.

## Experiments

- [Experiment 1: Create the TermSurf Rust workspace](01-termsurf-rust-workspace.md)
  — **Pass**
- [Experiment 2: Implement the Roastty ABI skeleton](02-roastty-abi-skeleton.md)
  — **Fail** (used `ghostty_*` compatibility symbols instead of the required
  `roastty_*` renamed ABI)
- [Experiment 3: Replace the compatibility ABI with Roastty naming](03-roastty-renamed-abi.md)
  — **Pass**
- [Experiment 4: Add Roastty config lookup defaults](04-config-get-defaults.md)
  — **Designed**

## Original Expected ABI Experiment

After the workspace exists, the next experiment should implement the ABI
skeleton.

It should:

- create the initial `roastty/` Rust crate layout;
- choose the C ABI naming and compatibility strategy;
- inventory the Ghostty symbols used by the reference macOS Swift app, then map
  them to the corresponding `roastty_*` ABI names;
- implement opaque Rust handles for app, config, and surface lifecycle;
- expose a minimal set of C symbols with stable ownership rules;
- add a tiny C or Rust integration test that loads the library, creates config,
  app, and surface handles, calls the tick/focus/resize lifecycle, and frees
  everything cleanly;
- avoid implementing real terminal emulation, PTY, rendering, or font logic.

The result should leave the repo ready for the next experiment: either wiring
the Swift app to the stub library, or expanding the skeleton to cover the next
required API group discovered during symbol inventory.
