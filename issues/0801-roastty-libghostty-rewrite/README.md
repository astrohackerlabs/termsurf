+++
status = "open"
opened = "2026-05-31"
+++

# Issue 801: Reimplement libghostty as libroastty

## Goal

Reimplement the remaining `libghostty` library surface and behavior as
`libroastty`, a Rust library with Roastty naming throughout. The end state is a
macOS-only terminal implementation with Ghostty feature parity before TermSurf
browser-overlay features are added.

## Background

Issue 800 created the Roastty foundation:

- a top-level Cargo workspace containing `webtui`, `roamium`, and `roastty`
- a new `roastty/` Rust crate
- the first `roastty_*` C ABI skeleton
- `roastty/include/roastty.h`
- a small configuration subset exposed through `roastty_config_get`
- an ABI inventory comparing upstream Ghostty's C surface to Roastty's current
  skeleton

That issue also corrected an important naming mistake: Roastty is a faithful
Rust adaptation of Ghostty, but it is not allowed to expose `ghostty_*` app or
ABI names. Public functions, app-facing symbols, crate names, comments, and
documentation inside the implementation should use `roastty` / `Roastty`. The
word `ghostty` should appear only when citing the upstream project, vendored
source paths, research notes, or attribution.

This issue continues from that foundation and owns the rest of the library
rewrite.

## Scope

This issue is intentionally broad: it tracks the remaining work needed to turn
`roastty/` from an ABI skeleton into a real terminal library. The work must be
implemented incrementally through reviewed experiments, but the target is the
entire remaining `libghostty` behavior needed by the macOS app.

Roastty is macOS-only. The rewrite should not preserve Ghostty's Linux, FreeBSD,
Windows, GTK, Wayland, X11, OpenGL, or other non-macOS gates as live
implementation paths. When upstream Ghostty has code shaped like "if Linux do
this, else if macOS do that," the Roastty port should keep the macOS behavior
and omit the other branches unless an experiment documents a specific reason to
keep a stub for testability or source comparison.

The rewrite includes, at minimum:

- complete configuration loading, parsing, validation, finalization,
  diagnostics, key lookup, keybindings, and runtime config updates
- app/runtime lifecycle behavior: init, tick, focus, quit confirmation, color
  scheme changes, global keybinds, mailbox/event delivery, and surface
  coordination
- surface lifecycle behavior: creation, inherited config, draw/refresh hooks,
  sizing, content scale, display ID, focus, occlusion, quicklook, inspector
  integration, split coordination, selection reads, and text reads
- terminal core behavior: parser, screen/grid, scrollback, cursor state, style
  state, CSI/OSC/DCS handling, SGR, mouse modes, selection, reflow, input
  encoding, Kitty keyboard behavior, and Kitty graphics behavior
- PTY and IO behavior: macOS shell/process spawn, read/write loops, resize,
  foreground process ID, TTY name, process exit, and platform-specific macOS
  behavior
- rendering state and renderer boundary: enough structure to preserve Ghostty's
  rendering model while adapting it to Rust, with Metal renderer parity as a
  later subsystem slice
- font and text stack: CoreText integration, shaping, fallback, metrics, glyph
  atlas behavior, emoji behavior, and Nerd Font handling
- clipboard, keyboard translation, mouse input, IME/preedit, and platform input
  edge cases
- the renamed macOS Swift frontend integration once the Rust library has enough
  behavior to host it

## Architecture

Roastty should be a faithful macOS adaptation of Ghostty's architecture, not a
new terminal emulator invented beside it. The implementation may use idiomatic
Rust where Rust gives a clearer or safer representation, but the port should
preserve Ghostty's subsystem boundaries and macOS behavior unless an experiment
explicitly records why a different design is required.

The public library boundary is `libroastty`:

- exported C functions use `roastty_*`
- public headers use Roastty names
- Rust crates/modules use Roastty names where product identity is visible
- compatibility shims named `ghostty_*` are not allowed
- upstream Ghostty names are allowed only when describing the source material,
  vendored paths, test provenance, or attribution

The target is macOS-only, not macOS-first. Cross-platform terminal parity is not
part of this issue and should not shape the internal architecture.

## Test Parity

The rewrite must include tests, not only implementation code. Each subsystem
experiment must identify the relevant upstream Ghostty tests, fixtures, or
behavioral checks and either:

- port those tests into the Roastty Rust test suite,
- create equivalent Roastty tests when a direct port is not appropriate, or
- explicitly document why a specific upstream test cannot be automated yet and
  what manual or future infrastructure would be required.

Feature parity is not considered achieved for a subsystem until both the
implementation and its corresponding tests or documented test-equivalent checks
exist.

## Process

This is a long parent issue. Do not attempt a big-bang rewrite. Each experiment
must choose one coherent subsystem slice, implement it, test it, record the
result, and let that result determine the next experiment.

Every experiment must follow the current project process:

- design exactly one experiment at a time
- have the experiment design reviewed by another AI agent before implementation
- fix all real design issues found by that review
- commit the reviewed experiment design separately
- implement and verify the experiment
- have the completed result reviewed by another AI agent before proceeding
- fix all real result issues found by that review
- commit the experiment result separately

No experiment may proceed to the next stage until the required review passes.

## Experiments

- [Experiment 1: Audit Dependencies and Platform Readiness](01-dependency-platform-audit.md)
  — **Pass**
- [Experiment 2: Define Zig-to-Rust Porting Patterns](02-zig-rust-porting-patterns.md)
  — **Pass**
- [Experiment 3: Port Terminal Tabstops](03-port-tabstops.md) — **Pass**
- [Experiment 4: Port Terminal Size Offsets](04-port-terminal-size.md) —
  **Pass**
- [Experiment 5: Decompose Page Storage Port](05-decompose-page-storage.md) —
  **Pass**
- [Experiment 6: Port Bitmap Allocator](06-port-bitmap-allocator.md) — **Pass**
- [Experiment 7: Port Terminal Style Value Types](07-port-style-value-types.md)
  — **Pass**
- [Experiment 8: Port Packed Row and Cell Values](08-port-row-cell-values.md) —
  **Pass**
- [Experiment 9: Port Page Capacity and Layout Arithmetic](09-port-page-layout.md)
  — **Pass**
- [Experiment 10: Port Basic Page Allocation and Access](10-port-page-init-access.md)
  — **Pass**
- [Experiment 11: Port Offset Hash Map Storage](11-port-offset-hash-map.md) —
  **Pass**
- [Experiment 12: Port Page Grapheme Storage](12-port-page-graphemes.md) —
  **Pass**
- [Experiment 13: Port Page Clone for Text and Graphemes](13-port-page-clone.md)
  — **Pass**
- [Experiment 14: Port Ref-Counted Set Storage](14-port-ref-counted-set.md) —
  **Pass**
- [Experiment 15: Port Style Hashing and Set Storage](15-port-style-set.md) —
  **Pass**
- [Experiment 16: Port Page Style Storage and Clone](16-port-page-style-clone.md)
  — **Pass**
- [Experiment 17: Port Page CloneFrom Plain Rows](17-port-page-clone-from-plain.md)
  — **Pass**

## Non-Goals

- Do not add TermSurf browser overlay behavior in this issue. Roastty must first
  become a real terminal foundation.
- Do not preserve or expose `ghostty_*` compatibility ABI names.
- Do not rewrite the vendored Ghostty source. The vendor copy is source material
  and attribution context.
- Do not retrofit older issue documents to the one-file-per-experiment
  structure.
- Do not implement Linux, FreeBSD, Windows, GTK, Wayland, X11, OpenGL, or other
  non-macOS support paths.
- Do not keep platform abstraction layers merely because upstream Ghostty has
  them. Keep an abstraction only when it is useful for Roastty's macOS design or
  for tests.

## Closure Criteria

This issue can close when `libroastty` implements the remaining macOS library
behavior required by the renamed app with test coverage or documented
test-equivalent checks for each ported subsystem. At that point Roastty should
be ready for a follow-up issue focused on the Swift app integration and then,
later, TermSurf browser overlay integration.
