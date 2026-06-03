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
- Rust replacements for the third-party libraries Ghostty depends on (see
  **Dependency replacements** below) — Roastty does not vendor or link Ghostty's
  Zig or C packages; each is provided in Rust (a crate or a from-scratch port
  per the hybrid policy)

### Dependency replacements

Roastty does **not** vendor, link, or carry forward Ghostty's third-party Zig or
C packages. The capability each provides is **provided in Rust**, in scope for
this issue. How that capability is provided follows a **hybrid policy by
layer**:

- **Crate-eligible (commodity primitives).** Where a component is a well-solved,
  general-purpose primitive with **no byte-for-byte requirement** — compression,
  image decoding, UTF-8 validation, regex, fuzzy matching, an event loop — a
  mature, well-maintained Rust crate may be used. Behavioral faithfulness (it
  decodes the same image, matches the same URL) is the bar, not bit-identical
  output.
- **From-scratch (identity / fidelity).** Where a component is part of Ghostty's
  own behavior/identity, or is constrained by a **fidelity test** (a golden
  fixture that demands byte-exact output), it is reimplemented as first-class
  Roastty code. Examples: the sprite path rasterizer (its glyph PNGs are
  compared byte-for-byte) and the Unicode tables (must match Ghostty's exact
  property/grapheme semantics).

So **byte-for-byte equivalence with the original C/Zig is required only where a
test encodes it**; otherwise correct behavior suffices. Each dependency's
crate-vs-from-scratch choice (and, for a crate, which one) is decided and
recorded in that dependency's own experiment, under the standard process (design
→ review → implement → test → review).

The macOS-only constraint above still applies: dependencies that exist solely
for Linux/GTK/Wayland/X11/OpenGL or non-macOS build paths are out of scope, and
capabilities already supplied by macOS **system frameworks** (CoreText,
CoreGraphics, etc., reached via `objc2` bindings) are bound, not reimplemented.

Zig-origin libraries:

- **uucode** — _from-scratch._ Unicode property, grapheme-break, and width
  tables used across the terminal and font layers. Roastty Unicode tables
  generated from the UCD, matching Ghostty's exact property semantics (crates
  like `unicode-width`/`unicode-segmentation` may cross-check, not replace).
- **z2d** — _fidelity decision (open)._ 2D vector rasterization, used only for
  the sprite font's anti-aliased path glyphs and the CPU debug overlay. Because
  the glyph PNGs are compared byte-for-byte, this is the one rasterizer subject
  to the fidelity rule — resolved in its own experiment (byte-exact port vs. a
  Rust rasterizer with regenerated fixtures). The exact-fill sprite path needs
  no rasterizer and is already ported.
- **libxev** — _crate-eligible._ The async event loop. A Rust event-loop crate
  (e.g. `mio`/`polling` over kqueue, or `tokio`) may drive the PTY read/write
  loops and timers; no byte-exact requirement (macOS only — the
  epoll/io_uring/IOCP backends are not ported).
- **zf** — _crate-eligible._ Fuzzy matching for list/command filtering (e.g.
  `nucleo`/`fuzzy-matcher`).
- **zig-objc** — _done._ Objective-C runtime bindings — already satisfied by
  `objc2`.
- **vaxis** — TUI toolkit; used only by the `+list-*` CLI tools, not the
  library. Addressed only if/when those CLIs are ported.
- **zig-js** — WASM/JS interop; not part of the macOS library (out of scope).

C-origin libraries (mostly crate-eligible — none carries a byte-exact fixture):

- **wuffs / libpng / zlib** — _crate-eligible._ Image decoding (Kitty graphics
  PNG) and DEFLATE (e.g. `png`, `flate2`/`miniz_oxide`).
- **oniguruma** — _crate-eligible._ Regular expressions for link/URL detection
  (e.g. the `regex` crate; confirm the flavor covers Ghostty's link patterns).
- **simdutf** — _crate-eligible._ Fast UTF-8 validation/transcoding (e.g.
  `simdutf8`, or `std`).
- **highway** — _crate-eligible / subsumed._ SIMD primitives — mostly absorbed
  by the crates above or `std::simd`; reimplemented only where still needed.
- **sentry** — _crate-eligible._ Crash reporting (app-level, optional; e.g. the
  `sentry` crate) if retained.
- **dcimgui** — _crate-eligible._ Dear ImGui for the inspector UI (e.g.
  `imgui-rs`/`egui`) if the inspector is retained.
- **glslang / spirv-cross** — _crate-eligible / deferred._ GLSL→SPIR-V→MSL
  shader translation (e.g. `naga`). Needed only if Roastty translates shaders at
  runtime; the Metal path uses precompiled shaders, so deferred unless a runtime
  need appears.
- **harfbuzz / freetype / fontconfig** — _system framework._ Shaping,
  rasterization, font discovery — superseded on macOS by CoreText/CoreGraphics
  (bound via `objc2`), so not reimplemented; fontconfig is Linux-only.

Out of scope (non-macOS): gobject/GTK, gtk4-layer-shell, wayland (and
protocols), opengl, libintl, and the Android/SDK packages.

## Subsystem checklist

A living, file-verified progress tracker for the whole rewrite: every libghostty
subsystem and every reimplemented dependency. **As items are completed, check
them off** (`[ ]` → `[x]`). An item is only checked when it is a complete,
faithful, tested port; partial subsystems leave the finished sub-items checked
and the rest unchecked with a note. Keep this in sync as experiments land — the
per-experiment detail lives in the [Experiments](#experiments) index below.

Status verified file-by-file against `vendor/ghostty/src/` on 2026-06-02 (after
Experiment 246).

### Terminal core — largely complete

- [x] Page / datastruct layer (`bitmap_allocator`, `page`, `page_list`, `point`,
      `ref_counted_set`, `offset_hash_map`)
- [x] VT parser + stream
      (`Parser`/`parse_table`/`ansi`/`apc`/`csi`/`UTF8Decoder` folded into
      `stream.rs`)
- [x] `Screen`, `Terminal`, `cursor`, `style`, `color` (named/SGR/x11),
      `charsets`, `tabstops`, `modes`
- [x] `SGR`, `OSC`, `DCS`, device attributes & status
- [x] Selection (+ codepoints, gestures), mouse (+ encoding), focus, clipboard,
      context signal, size / size-report, semantic prompt
- [x] Kitty graphics + Kitty keyboard
- [ ] `highlight`, `hyperlink` — ported but untested (finish + add tests)
- [ ] `formatter` / terminal `render`, `ScreenSet`, `stream_terminal` — partial
      / folded into `screen.rs`/`terminal.rs` (confirm parity)
- [ ] Scrollback `search` + `StringMap` — missing (needs `oniguruma`)
- [ ] `tmux` control mode — missing
- [ ] `sys` (PNG-decode abstraction) — missing

### Renderer — data + Metal primitives only; no live render loop

- [x] Cell contents builder (`cell.rs`), cursor style (`cursor.rs`),
      size/padding types (`size.rs`), shader vertex/uniform types (`shader.rs`)
- [x] Metal primitives — `api`, `buffer`, `shaders` (MSL), `render_pass`,
      `texture`
- [ ] Render `state` — partial (only `Preedit`; full `State` + `Mouse` missing)
- [ ] Image state (`image.rs`) — partial (data only, no GPU upload)
- [ ] Metal `pipeline` (partial), `Sampler`, `Frame`, `Target`, `IOSurfaceLayer`
      — missing
- [ ] Main render loop (`generic.zig`: frame build, dirty tracking, glyph
      upload, draw calls, pacing) — missing (critical)
- [ ] z2d debug `Overlay`, link highlighting, render `Thread`, custom shaders —
      missing

### Font & text — foundations only

- [x] `Metrics` (Metrics.zig complete)
- [x] `Atlas` (Atlas.zig complete, minus WASM)
- [x] `Glyph` value type
- [ ] Sprite `Canvas` — partial (exact-pixel ops done; z2d path rendering
      missing)
- [ ] Sprite `draw/` glyph tables (box/block/braille/powerline/geometric/legacy)
      — missing
- [ ] CoreText `Face` (rasterization + face-metric extraction) — missing
- [ ] Shaper (CoreText shaping, run, cache, feature) — missing
- [ ] `Collection` / `CodepointResolver` / `CodepointMap` / `DeferredFace` /
      `discovery` / `library` / `backend` — missing
- [ ] `SharedGrid` / `SharedGridSet` — missing
- [ ] `opentype/` (SFNT table parsing), `embedded`, `nerd_font_attributes` —
      missing

### Input — encoding only

- [x] Key codes/events, key encoding (VT/Kitty), key mods, bracketed paste
- [ ] Keybinding system (`Binding`, `command`/action dispatch) — missing
- [ ] Keymaps (`keycodes`, `function_keys`, `KeymapDarwin`, layouts) — missing
- [ ] Kitty keyboard protocol details (`input/kitty`), `Link`, mouse input
      structs — missing (note: mouse SGR/x11 _encoding_ exists in
      `terminal/mouse_encode.rs`)

### Configuration — skeleton only

- [ ] `Config` struct (full field set) — only a `finalized` flag exists
- [ ] Option parsing, CLI args, file / default / recursive loading — stubbed
- [ ] Validation / finalization / diagnostics — stubbed
- [ ] Keybind parsing, theme loading, conditionals, key-remap, clipboard maps,
      `formatter`/export — missing

### C ABI (`libroastty` boundary)

- [x] Init / string / lifecycle; terminal cells / rows / styles / render-state /
      colors / modes / IO / grid / selection / formatting
- [x] Key-event + key-encoder ABI, mouse event/encoder ABI, OSC parser ABI,
      selection-gesture ABI, Kitty-graphics ABI
- [x] Config / app / surface lifecycle handles (new/free/clone/userdata/basic
      setters)
- [ ] `config_get` (12 defaults only) + keybind triggers — partial
- [ ] App/surface key dispatch, surface draw/refresh, IME/text/preedit, surface
      mouse dispatch, selection read, splits — missing
- [ ] Inspector ABI — missing

### App / Surface / IO — not started (stubs only)

- [ ] `App` lifecycle (init, tick, focus, quit-confirm, color-scheme, global
      keybinds, mailbox / events) — skeleton struct + stubbed fns
- [ ] `Surface` lifecycle (create, config-inherit, draw/refresh, sizing, scale,
      display-id, occlusion, quicklook, inspector, splits, selection + text
      reads) — skeleton + basic setters only
- [ ] `pty` + `termio` (shell spawn, read/write loops, resize, fg pid, tty name,
      exit) — missing (placeholder returns)
- [ ] `os/` utilities (tmpdir / file / env / hostname / locale) — ad hoc Rust
      stdlib; no dedicated module

### Supporting subsystems

- [ ] `unicode/` (grapheme break, width/wcwidth, properties) — missing (no
      tables; widths currently implicit)
- [x] Datastruct: `OffsetHashMap`, `PageList`, `BitmapAllocator`,
      `RefCountedSet` (in `terminal/`)
- [ ] Datastruct: `CircBuf`, `IntrusiveLinkedList`, other utilities — as needed
- [ ] `cli/` (+list-\* tools), `inspector/` (imgui), `crash/` (sentry),
      `terminfo/`, `synthetic/`
- [ ] Swift macOS frontend integration

Out of scope / tooling: `build/`, `benchmark/`, `extra/`, `simd/`, `stb/`,
`lib/`, and all non-macOS paths (Linux/GTK/Wayland/OpenGL).

### Dependencies (provided in Rust — crate or from-scratch per the hybrid policy)

- [x] `zig-objc` → satisfied by `objc2`
- [ ] `uucode` (Unicode tables) — not started (no Unicode tables exist yet)
- [ ] `z2d` (sprite path rasterizer) — in progress (exact-pixel Canvas done;
      path rendering pending)
- [ ] `libxev` (event loop) — not started
- [ ] `zf` (fuzzy match) — not started
- [ ] `wuffs` / `libpng` / `zlib` (image decode + inflate) — not started
- [ ] `oniguruma` (regex) — not started (also gates terminal `search`)
- [ ] `simdutf` (UTF-8 validation/transcoding) — not started
- [ ] `highway` (SIMD) — not started
- [ ] `sentry` (crash reporting) — not started
- [ ] `dcimgui` (inspector UI) — not started
- [ ] `glslang` / `spirv-cross` (shader translation) — not started

Resolved by decision (no reimplementation): `harfbuzz` / `freetype` /
`fontconfig` — superseded by macOS CoreText/CoreGraphics (bound via `objc2`).

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

## Agent Provenance

Every experiment records, in TOML frontmatter at the top of its file, which AI
agent performed each role: the **implementer** (designs, writes, and records the
experiment) and the two review gates (**design review** and **result review**).
Each role logs `agent`, `model`, and `reasoning` (effort level), so the record
is machine-parseable for later comparison. Design review and result review are
independent passes from implementation; from Experiment 223 the reviewer is also
a different agent than the implementer.

- **Experiments 1–222:** designed, reviewed, implemented, and re-reviewed
  entirely by **Codex (GPT-5.5, medium)** — all three roles.
- **Experiment 223 onward:** the implementer switches to **Claude (Opus 4.8,
  high)** as a controlled trial; both review gates remain **Codex (GPT-5.5,
  medium)**. This is the first time implementer and reviewer differ.

Each `## Experiments` line is also tagged after its status with the agents for
all three roles, in order — `implementer/design-review/result-review` (e.g.
`— **Pass** · Codex/Codex/Codex`, or `· Claude/Codex/Codex` from Experiment 223
on) — so the implementer and reviewers are scannable from the index. The tags
must match that experiment's `[implementer]`, `[review.design]`, and
`[review.result]` frontmatter.

Frontmatter schema:

```toml
+++
[implementer]
agent = "..."      # codex | claude-code
model = "..."      # gpt-5.5 | claude-opus-4-8
reasoning = "..."  # effort level: medium | high

[review.design]    # same three keys
[review.result]    # same three keys
+++
```

## Experiment Granularity

Early experiments may be small when they establish correctness-critical
foundation: page storage, managed metadata, pin tracking, parser state,
formatters, selection, and row/cell mutation primitives. These areas are allowed
to advance one primitive or one control sequence at a time because bugs are hard
to localize and can corrupt later layers.

Once the relevant foundation exists, experiments should grow to coherent
subsystem slices rather than one tiny behavior at a time. Prefer grouping
related features when they share the same implementation surface and can be
verified together, for example:

- remaining related CSI row/scroll mutations;
- SGR execution plus styled printing;
- OSC parsing and action dispatch;
- mouse modes plus mouse encoding;
- PTY spawn/read/write/resize;
- macOS input/key translation.

The goal is not to maximize experiment count. The goal is to preserve
reviewable, testable progress toward full `libroastty` parity. If an experiment
can cover a larger subsystem without blurring failure diagnosis or weakening
tests, choose the larger subsystem.

### Sizing each experiment

Size each experiment by **risk, not by count or line total**. Choose the largest
slice that still satisfies all of:

1. **One coherent implementation surface** — the change lives in one
   subsystem/file cluster, not spread across unrelated layers.
2. **Predictable tests** — you can write the experiment's tests and reasonably
   expect them to pass before implementing. If you cannot predict the outcome,
   the slice carries too much uncertainty.
3. **At most one novel mechanism** — only one genuinely new or uncertain
   technique per experiment (a new unsafe pattern, a new framework binding, a
   new ownership model). Two independent unknowns means two experiments.
4. **Localized failure** — if it fails, the cause narrows to a single behavior.

If a candidate breaks any of these, split it at the point of uncertainty. If it
satisfies all of them and a sibling behavior shares the same surface and tests
with no independent risk, fold the sibling in rather than spending a separate
design + review + result cycle on it.

When unsure, bias by risk: prefer the **larger** slice for mechanical, low-risk
work (leaf control-sequence handlers, value types, lookup tables, scalar ABI
getters), and the **smaller** slice for high-risk work (unsafe memory,
ownership/lifetime, reflow, framework lifecycle — anything whose bug corrupts
later layers).

The remaining subsystems — font/CoreText, the live Metal renderer, config
loading, PTY/termio, and the Swift frontend — involve macOS framework
integration that is harder to test one-to-one than the terminal control-sequence
tail. Treat those as higher-risk and keep their slices smaller even though they
are past the correctness-critical foundation.

## Experiments

- [Experiment 1: Audit Dependencies and Platform Readiness](01-dependency-platform-audit.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 2: Define Zig-to-Rust Porting Patterns](02-zig-rust-porting-patterns.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 3: Port Terminal Tabstops](03-port-tabstops.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 4: Port Terminal Size Offsets](04-port-terminal-size.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 5: Decompose Page Storage Port](05-decompose-page-storage.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 6: Port Bitmap Allocator](06-port-bitmap-allocator.md) — **Pass**
  · Codex/Codex/Codex
- [Experiment 7: Port Terminal Style Value Types](07-port-style-value-types.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 8: Port Packed Row and Cell Values](08-port-row-cell-values.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 9: Port Page Capacity and Layout Arithmetic](09-port-page-layout.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 10: Port Basic Page Allocation and Access](10-port-page-init-access.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 11: Port Offset Hash Map Storage](11-port-offset-hash-map.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 12: Port Page Grapheme Storage](12-port-page-graphemes.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 13: Port Page Clone for Text and Graphemes](13-port-page-clone.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 14: Port Ref-Counted Set Storage](14-port-ref-counted-set.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 15: Port Style Hashing and Set Storage](15-port-style-set.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 16: Port Page Style Storage and Clone](16-port-page-style-clone.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 17: Port Page CloneFrom Plain Rows](17-port-page-clone-from-plain.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 18: Port Page CloneFrom Graphemes](18-port-page-clone-from-graphemes.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 19: Port Page CloneFrom Styles](19-port-page-clone-from-styles.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 20: Port Page Hyperlink Storage](20-port-page-hyperlink-storage.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 21: Port Page CloneFrom Hyperlinks](21-port-page-hyperlink-row-copy.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 22: Port Page Exact Row Capacity](22-port-page-exact-row-capacity.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 23: Port Page Partial Row Clone](23-port-page-partial-row-clone.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 24: Port Page Move Cells](24-port-page-move-cells.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 25: Port Page Swap Cells](25-port-page-swap-cells.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 26: Port Page Clear Cells](26-port-page-clear-cells.md) — **Pass**
  · Codex/Codex/Codex
- [Experiment 27: Port Page Reinit](27-port-page-reinit.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 28: Port Page Integrity Checks](28-port-page-integrity-checks.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 29: Port Page Set Graphemes](29-port-page-set-graphemes.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 30: Port Page Move Grapheme](30-port-page-move-grapheme.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 31: Port Terminal Points](31-port-terminal-points.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 32: Port PageList Sizing](32-port-pagelist-sizing.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 33: Port PageList Init](33-port-pagelist-init.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 34: Port PageList Points](34-port-pagelist-points.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 35: Port PageList Tracked Pins](35-port-pagelist-tracked-pins.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 36: Port PageList Scrollbar State](36-port-pagelist-scrollbar.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 37: Port PageList Viewport Scrolling](37-port-pagelist-scroll.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 38: Port PageList Basic Growth](38-port-pagelist-grow.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 39: Port PageList Prune Growth](39-port-pagelist-prune.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 40: Port PageList Reset](40-port-pagelist-reset.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 41: Port PageList Page Iterator](41-port-pagelist-page-iterator.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 42: Port PageList Clone](42-port-pagelist-clone.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 43: Port PageList Dirty Helpers](43-port-pagelist-dirty-helpers.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 44: Port PageList Increase Capacity](44-port-pagelist-increase-capacity.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 45: Port PageList Compact](45-port-pagelist-compact.md) — **Pass**
  · Codex/Codex/Codex
- [Experiment 46: Port PageList Split](46-port-pagelist-split.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 47: Port PageList Viewport Fixup](47-port-pagelist-viewport-fixup.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 48: Port PageList Erase Row](48-port-pagelist-erase-row.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 49: Port PageList Erase Row Bounded](49-port-pagelist-erase-row-bounded.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 50: Port PageList Erase Page](50-port-pagelist-erase-page.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 51: Port PageList Erase Rows](51-port-pagelist-erase-rows.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 52: Port PageList Scroll Clear](52-port-pagelist-scroll-clear.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 53: Port PageList Cell Lookup](53-port-pagelist-cell-lookup.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 54: Port PageList Row Iterator](54-port-pagelist-row-iterator.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 55: Port PageList Cell Iterator](55-port-pagelist-cell-iterator.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 56: Port PageList Prompt Iterator](56-port-pagelist-prompt-iterator.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 57: Port Semantic Prompt Highlight](57-port-semantic-prompt-highlight.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 58: Port Semantic Input Highlight](58-port-semantic-input-highlight.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 59: Port Semantic Output Highlight](59-port-semantic-output-highlight.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 60: Port Semantic Highlight Dispatcher](60-port-semantic-highlight-dispatcher.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 61: Port Highlight Untracked Module](61-port-highlight-untracked-module.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 62: Port Highlight Flattened Shape](62-port-highlight-flattened-shape.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 63: Port Flattened Highlight Constructor](63-port-flattened-highlight-constructor.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 64: Port Tracked Highlight](64-port-tracked-highlight.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 65: Port Selection Codepoints](65-port-selection-codepoints.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 66: Port Selection Value Shape](66-port-selection-value-shape.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 67: Port Selection Ordering](67-port-selection-ordering.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 68: Port Selection Containment](68-port-selection-containment.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 69: Port Selection Contained Row](69-port-selection-contained-row.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 70: Port Selection Adjustment](70-port-selection-adjustment.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 71: Port Selection Tracking Ownership](71-port-selection-tracking-ownership.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 72: Port Selection Pin Navigation](72-port-selection-pin-navigation.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 73: Port Cell Drag Selection](73-port-cell-drag-selection.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 74: Port Word Selection](74-port-word-selection.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 75: Port Line Selection](75-port-line-selection.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 76: Port Select All](76-port-select-all.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 77: Port Select Output](77-port-select-output.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 78: Port Line Iterator](78-port-line-iterator.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 79: Port Plain Selection String](79-port-plain-selection-string.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 80: Port Prompt Click Movement](80-port-prompt-click-movement.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 81: Port Dump String Helpers](81-port-dump-string.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 82: Port Styled Page Formatter Core](82-port-styled-page-formatter-core.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 83: Port Formatter Codepoint Map](83-port-codepoint-map.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 84: Port Plain Formatter Point Map](84-port-plain-formatter-point-map.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 85: Port Plain Formatter Pin Map](85-port-plain-formatter-pin-map.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 86: Port VT Formatter Point Map](86-port-vt-formatter-point-map.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 87: Port HTML Formatter Point Map](87-port-html-formatter-point-map.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 88: Port Styled Formatter Pin Maps](88-port-styled-formatter-pin-maps.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 89: Port Screen Formatter Content](89-port-screen-formatter-content.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 90: Port Terminal Formatter Content](90-port-terminal-formatter-content.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 91: Port Screen Formatter Cursor and Style Extras](91-port-screen-formatter-cursor-style-extras.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 92: Port Screen Formatter Protection Extra](92-port-screen-formatter-protection-extra.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 93: Port Screen Formatter Charset Extra](93-port-screen-formatter-charset-extra.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 94: Port Screen Formatter Kitty Keyboard Extra](94-port-screen-formatter-kitty-keyboard-extra.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 95: Port Screen Formatter Hyperlink Extra](95-port-screen-formatter-hyperlink-extra.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 96: Port Terminal Formatter Screen Extra Forwarding](96-port-terminal-formatter-screen-extra-forwarding.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 97: Port Terminal Formatter Palette Extra](97-port-terminal-formatter-palette-extra.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 98: Port Terminal Modes and Formatter Extra](98-port-terminal-modes-extra.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 99: Port Terminal Scrolling Region Formatter Extra](99-port-terminal-scrolling-region-extra.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 100: Port Terminal Tabstops Formatter Extra](100-port-terminal-tabstops-extra.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 101: Port Terminal Keyboard and Pwd Formatter Extra](101-port-terminal-keyboard-pwd-extra.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 102: Port Stream UTF-8 Print Core](102-port-stream-utf8-print-core.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 103: Port Basic Stream Print Mutation](103-port-basic-stream-print-mutation.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 104: Port Basic Pending Wrap](104-port-basic-pending-wrap.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 105: Port Basic Wrap Scroll](105-port-basic-wrap-scroll.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 106: Port Basic LF and CR](106-port-basic-lf-cr.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 107: Port Basic Backspace](107-port-basic-backspace.md) — **Pass**
  · Codex/Codex/Codex
- [Experiment 108: Port Basic Horizontal Tab](108-port-basic-horizontal-tab.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 109: Port VT and FF Linefeed Aliases](109-port-vt-ff-linefeed-aliases.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 110: Port Escape Tab Set](110-port-escape-tab-set.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 111: Port CSI Tab Set](111-port-csi-tab-set.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 112: Port CSI Tab Clear and Reset](112-port-csi-tab-clear-reset.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 113: Port Escape Index and Next Line](113-port-escape-index-next-line.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 114: Port Basic CSI Cursor Movement](114-port-basic-csi-cursor-movement.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 115: Port CSI Next and Previous Line](115-port-csi-next-previous-line.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 116: Port CSI Horizontal Absolute](116-port-csi-horizontal-absolute.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 117: Port CSI Vertical Positioning](117-port-csi-vertical-positioning.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 118: Port CSI Cursor Position](118-port-csi-cursor-position.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 119: Port CSI Horizontal Tabulation](119-port-csi-horizontal-tabulation.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 120: Port CSI Erase Display](120-port-csi-erase-display.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 121: Port CSI Erase Line](121-port-csi-erase-line.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 122: Port CSI Delete Character](122-port-csi-delete-character.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 123: Port CSI Insert Lines](123-port-csi-insert-lines.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 124: Port CSI Delete Lines](124-port-csi-delete-lines.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 125: Port CSI Scroll Up and Down](125-port-csi-scroll-up-down.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 126: Port CSI Insert and Erase Characters](126-port-csi-insert-erase-characters.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 127: Port CSI Horizontal Tab Back](127-port-csi-horizontal-tab-back.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 128: Port CSI Mode Set and Reset](128-port-csi-mode-set-reset.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 129: Port Basic Print Mode Effects](129-port-basic-print-mode-effects.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 130: Port CSI Mode Save and Restore](130-port-csi-mode-save-restore.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 131: Port DECRQM Mode Reports](131-port-decrqm-mode-reports.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 132: Port SGR and Styled Printing](132-port-sgr-styled-printing.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 133: Port Basic OSC Runtime](133-port-basic-osc-runtime.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 134: Port OSC 8 Printed Cell Hyperlinks](134-port-osc8-cell-hyperlinks.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 135: Port OSC ANSI Palette Operations](135-port-osc-ansi-palette.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 136: Port OSC Dynamic Colors](136-port-osc-dynamic-colors.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 137: Port RGB Parser Parity](137-port-rgb-parser-parity.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 138: Port Kitty OSC 21 Colors](138-port-kitty-osc21-colors.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 139: Port OSC 22 Mouse Shape](139-port-osc22-mouse-shape.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 140: Port Kitty OSC 66 Text Sizing](140-port-kitty-osc66-text-sizing.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 141: Port OSC Notifications](141-port-osc-notifications.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 142: Port OSC Clipboard Protocols](142-port-osc-clipboard-protocols.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 143: Port OSC 3008 Context Signals](143-port-osc3008-context-signals.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 144: Port OSC 133 Semantic Prompts](144-port-osc133-semantic-prompts.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 145: Port iTerm2 OSC 1337](145-port-iterm2-osc1337.md) — **Pass**
  · Codex/Codex/Codex
- [Experiment 146: Port Terminal Query Responses](146-port-terminal-query-responses.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 147: Port DCS and APC Framing](147-port-dcs-apc-framing.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 148: Port DCS Command Handling](148-port-dcs-command-handling.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 149: Port Cursor Visual Style](149-port-cursor-visual-style.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 150: Port ESC Cursor State Controls](150-port-esc-cursor-state-controls.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 151: Port RIS Full Reset](151-port-ris-full-reset.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 152: Port Charset Escape Controls](152-port-charset-escape-controls.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 153: Port CSI Repeat Previous Character](153-port-csi-repeat-previous-character.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 154: Port Alternate-Screen Modes](154-port-alternate-screen-modes.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 155: Port Kitty Keyboard Protocol](155-port-kitty-keyboard-protocol.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 156: Port Mouse Event Encoding](156-port-mouse-event-encoding.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 157: Port Mouse Mode Runtime State](157-port-mouse-mode-runtime-state.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 158: Port Mouse Encoder C ABI](158-port-mouse-encoder-c-abi.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 159: Port Key Event Value Types](159-port-key-event-value-types.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 160: Port Key Encoder Core](160-port-key-encoder-core.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 161: Complete Key Encoder Tables](161-complete-key-encoder-tables.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 162: Port Legacy Ctrl/CSI-u/Alt Matrix](162-port-legacy-ctrl-csiu-alt-matrix.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 163: Port Key Encoder C ABI](163-port-key-encoder-c-abi.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 164: Port OSC Parser C ABI](164-port-osc-parser-c-abi.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 165: Port Terminal Stream C ABI](165-port-terminal-stream-c-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 166: Port Terminal Scalar Getters C ABI](166-port-terminal-scalar-getters-c-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 167: Port Terminal Mode Control C ABI](167-port-terminal-mode-control-c-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 168: Port Terminal Metadata Setters C ABI](168-port-terminal-metadata-setters-c-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 169: Port Terminal Color Set/Get C ABI](169-port-terminal-color-set-get-c-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 170: Port Terminal Basic Effects C ABI](170-port-terminal-basic-effects-c-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 171: Port Terminal Query Callback C ABI](171-port-terminal-query-callbacks-c-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 172: Port Terminal Grid Reference C ABI](172-port-terminal-grid-reference-c-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 173: Port Terminal Selection C ABI](173-port-terminal-selection-c-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 174: Port Selection Gesture C ABI](174-port-selection-gesture-c-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 175: Port Tracked Grid Reference C ABI](175-port-tracked-grid-reference-c-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 176: Port Row and Cell C ABI](176-port-row-cell-c-abi.md) —
  **Pass** · Codex/Codex/Codex
- [Experiment 177: Port Style C ABI](177-port-style-c-abi.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 178: Port Render State Scalar C ABI](178-port-render-state-scalar-c-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 179: Port Render State Row Iterator C ABI](179-port-render-state-row-iterator-c-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 180: Port Render State Row Cells Basic C ABI](180-port-render-state-row-cells-basic-c-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 181: Complete Render State Row Cells Selectors](181-complete-render-state-row-cells-selectors.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 182: Port Grid Ref Accessors C ABI](182-port-grid-ref-accessors-c-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 183: Port Terminal Formatter C ABI](183-port-terminal-formatter-c-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 184: Port Standalone Terminal Encoding C ABI](184-port-standalone-terminal-encoding-c-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 185: Port Support C ABI](185-port-support-c-abi.md) — **Pass** ·
  Codex/Codex/Codex
- [Experiment 186: Port Kitty Graphics Command Parser](186-port-kitty-graphics-command-parser.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 187: Port Direct Kitty Image Loading](187-port-direct-kitty-image-loading.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 188: Port Kitty Image Storage Foundation](188-port-kitty-image-storage-foundation.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 189: Port Kitty Transmit and Query Execution](189-port-kitty-transmit-query-execution.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 190: Port Kitty Placement Storage Foundation](190-port-kitty-placement-storage-foundation.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 191: Port Kitty Display Storage Execution](191-port-kitty-display-storage-execution.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 192: Port Kitty Graphics Terminal Dispatch](192-port-kitty-graphics-terminal-dispatch.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 193: Port Kitty Tracked Placement Ownership](193-port-kitty-tracked-placement-ownership.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 194: Port Kitty Delete Execution](194-port-kitty-delete-execution.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 195: Port Kitty Transmit-Display and Cursor-After](195-port-kitty-transmit-display-cursor-after.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 196: Port Kitty Graphics C ABI Handles](196-port-kitty-graphics-c-abi-handles.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 197: Port Kitty Placement Render Info ABI](197-port-kitty-placement-render-info-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 198: Port Kitty Graphics Terminal Options](198-port-kitty-graphics-terminal-options.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 199: Port Kitty Graphics File Media](199-port-kitty-graphics-file-media.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 200: Port Kitty Graphics Shared Memory](200-port-kitty-graphics-shared-memory.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 201: Port Kitty Graphics PNG Decode Hook](201-port-kitty-graphics-png-decode-hook.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 202: Port Kitty Virtual Placeholder Foundation](202-port-kitty-virtual-placeholder-foundation.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 203: Port Kitty Terminal Render Placement ABI](203-port-kitty-terminal-render-placement-abi.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 204: Attach Kitty Render Placements to Render State](204-attach-kitty-render-placements-to-render-state.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 205: Port Renderer Image State Foundation](205-port-renderer-image-state-foundation.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 206: Port Renderer Image Upload and Draw Contract](206-port-renderer-image-upload-draw-contract.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 207: Port Metal Image Texture Values](207-port-metal-image-texture-values.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 208: Port Metal Texture Upload Backend](208-port-metal-texture-upload-backend.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 209: Port Image Draw Command Contract](209-port-image-draw-command-contract.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 210: Port Metal Buffer Wrapper](210-port-metal-buffer-wrapper.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 211: Port Metal Vertex Descriptor Mapping](211-port-metal-vertex-descriptor-mapping.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 212: Port Metal Pipeline Attachment Values](212-port-metal-pipeline-attachment-values.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 213: Port Standard Metal Pipeline Descriptions](213-port-standard-metal-pipeline-descriptions.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 214: Port Metal Pipeline State Builder](214-port-metal-pipeline-state-builder.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 215: Port Standard Metal Shader Library](215-port-standard-metal-shader-library.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 216: Port Offscreen Metal Render Pass Readback](216-port-offscreen-metal-render-pass-readback.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 217: Port Offscreen Metal Cell Background Readback](217-port-offscreen-metal-cell-background-readback.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 218: Port Offscreen Metal Image Texture Readback](218-port-offscreen-metal-image-texture-readback.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 219: Port Offscreen Metal Background Image Readback](219-port-offscreen-metal-background-image-readback.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 220: Port Offscreen Metal Cell Text Grayscale Readback](220-port-offscreen-metal-cell-text-grayscale-readback.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 221: Port Offscreen Metal Cell Text Color Readback](221-port-offscreen-metal-cell-text-color-readback.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 222: Port Offscreen Metal Cell Text Cursor Readback](222-port-offscreen-metal-cell-text-cursor-readback.md)
  — **Pass** · Codex/Codex/Codex
- [Experiment 223: Port Renderer Cursor Style](223-port-renderer-cursor-style.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 224: Port Renderer Sizing Value Types](224-port-renderer-sizing-value-types.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 225: Port Renderer Size, PaddingBalance, and Coordinate](225-port-renderer-size-coordinate.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 226: Port Renderer Preedit](226-port-renderer-preedit.md) —
  **Pass** · Claude/Codex/Codex
- [Experiment 227: Port Renderer Cell Codepoint Classification](227-port-renderer-cell-codepoint-classification.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 228: Port Renderer `is_symbol` Predicate](228-port-renderer-is-symbol.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 229: Port Renderer `constraint_width`](229-port-renderer-constraint-width.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 230: Port Renderer `Contents` Storage and Lifecycle](230-port-renderer-contents-storage.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 231: Port `Contents::set_cursor` and `get_cursor_glyph`](231-port-contents-set-cursor.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 232: Port `Contents::add` / `clear` and `Key`](232-port-contents-add-clear.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 233: Establish the `font` Module and Port `Glyph`](233-establish-font-module-glyph.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 234: Port Font `Style` and `Presentation` Enums](234-port-font-style-presentation.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 235: Port the Font `Metrics` Struct](235-port-font-metrics-struct.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 236: Port Font `FaceMetrics` and Its Convenience Methods](236-port-font-facemetrics.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 237: Port the Remaining `FaceMetrics` Accessors](237-port-facemetrics-accessors.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 238: Port Font `Metrics::calc` and `clamp`](238-port-font-metrics-calc.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 239: Port Font `Modifier` and `Modifier::parse`](239-port-font-modifier-parse.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 240: Port `Modifier::apply` (u32 / i32 / f64)](240-port-modifier-apply.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 241: Port Font `Key` and `ModifierSet`](241-port-font-key-modifierset.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 242: Port `Metrics::apply` (modifier dispatch + cell-height re-centering)](242-port-metrics-apply.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 243: Port the Atlas core (skyline packing: `reserve`/`fit`/`merge`/`set`)](243-port-atlas-core.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 244: Port Atlas `grow`, `set_from_larger`, and `dump`](244-port-atlas-grow.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 245: Port the sprite canvas geometric primitives and `Color`](245-port-sprite-primitives.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 246: Port the Canvas exact-pixel operations (draw + export to atlas)](246-port-canvas-pixel-ops.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 247: Port the OpenType metric-table parsers (sfnt scalars + `head` + `hhea`)](247-port-opentype-head-hhea.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 248: Port the OpenType `post` table parser (+ `Version16Dot16`)](248-port-opentype-post.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 249: Port the OpenType `os2` table parser (version-gated)](249-port-opentype-os2.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 250: CoreText `Face` FFI spike — create a `CTFont` and copy a table](250-coretext-face-table-spike.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 251: CoreText `Face` scalar metric accessors](251-coretext-scalar-metrics.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 252: CoreText `Face` glyph-measurement accessors](252-coretext-glyph-measurement.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 253: `Face::get_metrics` — assemble `FaceMetrics` from a CoreText font](253-coretext-get-metrics.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 254: CoreText glyph rasterization spike — glyph → alpha bitmap](254-coretext-rasterize-glyph.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 255: CoreText render_glyph — rasterize into the atlas, return a Glyph](255-coretext-render-glyph.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 256: Glyph constraint math — the constrain geometry, fixture-exact](256-glyph-constraint.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 257: Wire the constraint into render_glyph — RenderOptions + scaled draw](257-render-glyph-constrained.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 258: Faithful drawing context + thicken (font smoothing)](258-render-glyph-thicken.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 259: Synthetic bold — rect growth + fill-stroke draw](259-render-glyph-synthetic-bold.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 260: Color glyph detection — ColorState (sbix)](260-color-glyph-detection.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 261: Colored glyph render — depth-4 P3 RGBA (sbix)](261-color-glyph-render.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 262: Collection Index — the packed font-index type](262-collection-index.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 263: Collection — eager faces, add / get_face](263-collection-add-getface.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 264: Collection codepoint resolution — get_index / has_codepoint](264-collection-codepoint-resolution.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 265: Collection EntryOrAlias — style aliasing storage](265-collection-entry-alias.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 266: Collection completeStyles — alias missing styles](266-collection-complete-styles.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 267: Synthetic face methods — Face::synthetic_bold / synthetic_italic](267-synthetic-face-methods.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 268: completeStyles synthesis — synthesize vs. alias](268-complete-styles-synthesis.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 269: Collection scale factor — match a face to the primary](269-collection-scale-factor.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 270: Collection scale-factor integration — add with size adjustment](270-collection-scale-integration.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 271: Collection updateMetrics — grid metrics from the primary face](271-collection-update-metrics.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 272: CodepointResolver core — getIndex with style/regular fallback](272-codepoint-resolver-core.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 273: Descriptor + CodepointMap — the font-search data layer](273-discovery-descriptor-codepoint-map.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 274: Resolver getPresentation + renderGlyph delegation](274-resolver-presentation-render.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 275: Sprite box-drawing — the lines_char primitive](275-sprite-box-lines.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 276: Complete the lines_char dispatch (all 109 line glyphs)](276-box-lines-full-dispatch.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 277: Box-drawing dash primitives](277-box-dashes.md) — **Pass** ·
  Claude/Codex/Codex
- [Experiment 278: The Fraction + fill cell-geometry primitive](278-fraction-fill.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 279: Block Elements (U+2580–U+259F)](279-block-elements.md) —
  **Pass** · Claude/Codex/Codex
- [Experiment 280: Braille Patterns (U+2800–U+28FF)](280-braille.md) — **Pass**
  · Claude/Codex/Codex
- [Experiment 281: Sextants (U+1FB00–U+1FB3B)](281-sextants.md) — **Pass** ·
  Claude/Codex/Codex
- [Experiment 282: Separated Block Quadrants (U+1CC21–U+1CC2F)](282-separated-block-quadrants.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 283: Octants (U+1CD00–U+1CDE5)](283-octants.md) — **Pass** ·
  Claude/Codex/Codex
- [Experiment 284: z2d port — the Polygon/Edge tessellation core](284-z2d-polygon-core.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 285: z2d port — the WorkingEdgeSet active-edge-table](285-z2d-working-edge-set.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 286: z2d port — the SparseCoverageBuffer](286-z2d-sparse-coverage.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 287: z2d port — the MSAA supersampled-span distributor](287-z2d-supersample-span.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 288: z2d port — the multisample rasterizer run](288-z2d-multisample-run.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 289: z2d port — the path-node representation](289-z2d-path-nodes.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 290: z2d port — the Spline cubic-Bézier flattener](290-z2d-spline.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 291: z2d port — the fill plotter](291-z2d-fill-plotter.md) —
  **Pass** · Claude/Codex/Codex
- [Experiment 292: z2d port — Slope](292-z2d-slope.md) — **Pass** ·
  Claude/Codex/Codex
- [Experiment 293: z2d port — Face (stroke offsets + butt cap)](293-z2d-face.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 294: z2d port — Polygon Contour](294-z2d-contour.md) — **Pass** ·
  Claude/Codex/Codex
- [Experiment 295: z2d port — the single-segment stroke](295-z2d-stroke-line.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 296: Canvas::line + the box-drawing diagonals](296-canvas-line-diagonals.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 297: z2d port — the multi-segment open-path stroke](297-z2d-stroke-path.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 298: z2d port — the Pen (round-join/cap vertex set)](298-z2d-pen.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 299: z2d port — round joins in the stroke plotter](299-z2d-round-joins.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 300: z2d port — the cubic-curve stroke (`runCurveTo`)](300-z2d-curve-stroke.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 301: the box-drawing arcs (`U+256D`–`U+2570`)](301-box-arcs.md) —
  **Pass** · Claude/Codex/Codex
- [Experiment 302: z2d port — round and square line caps](302-z2d-caps.md) —
  **Pass** · Claude/Codex/Codex
- [Experiment 303: the curly underline (the first round-cap glyph)](303-curly-underline.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 304: z2d port — the closed-path stroke (`plotClosedJoined`)](304-z2d-closed-stroke.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 305: Canvas::fill_path + the filled corner triangles (U+25E2–25E5)](305-corner-triangles.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 306: Canvas::inner_stroke_path + the outlined corner triangles (U+25F8–25FA, 25FF)](306-outlined-triangles.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 307: the rect-based special sprites (underline, double, strikethrough, overline)](307-special-underlines.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 308: the cursor sprites (block, hollow, bar, underline)](308-cursors.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 309: the dashed underline](309-dashed-underline.md) — **Pass** ·
  Claude/Codex/Codex
- [Experiment 310: the arc primitive + the dotted underline](310-dotted-underline.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 311: Canvas::triangle + the solid powerline triangles (E0B0–E0BE)](311-powerline-triangles.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 312: Canvas::flip_horizontal + the outlined powerline chevrons (E0B1/E0B3)](312-powerline-chevrons.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 313: the rounded powerline separators (E0B4–E0B7)](313-powerline-rounded.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 314: the powerline diagonal spacers (E0B9/E0BB/E0BD/E0BF)](314-powerline-diagonals.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 315: the powerline flame separators (E0D2/E0D4)](315-powerline-flames.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 316: the unifying codepoint sprite dispatch (draw_codepoint)](316-sprite-dispatch.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 317: the sprite has_codepoint predicate](317-sprite-has-codepoint.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 318: the sprite render-to-atlas (renderGlyph)](318-sprite-render-glyph.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 319: wiring the sprite font into the resolver](319-sprite-resolver-wiring.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 320: the wide-glyph cell-width factoring](320-wide-glyph-cell-width.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 321: the sprite-kind special glyph dispatch](321-sprite-kind-special-glyphs.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 322: the UCD emoji-presentation default](322-ucd-emoji-presentation-default.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 323: the Nerd Font constraint attribute table](323-nerd-font-attribute-table.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 324: SVG color detection](324-svg-color-detection.md) — **Pass** ·
  Claude/Codex/Codex
- [Experiment 325: font discovery — the CoreText descriptor](325-discovery-descriptor.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 326: font discovery — the collection match](326-discovery-collection-match.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 327: font discovery — the ranking score](327-discovery-score.md) —
  **Pass** · Claude/Codex/Codex
- [Experiment 328: font discovery — computing the score](328-discovery-score-compute.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 329: font discovery — bold/italic refinement](329-discovery-score-bold-italic.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 330: font discovery — the style match](330-discovery-score-style.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 331: font discovery — sorting the candidates](331-discovery-sort.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 332: font discovery — the iterator (deferred faces)](332-discovery-iterator.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 333: the resolver's discovery-based fallback](333-resolver-discovery-fallback.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 334: discover a font for a codepoint (CTFontCreateForString)](334-discover-codepoint.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 335: the discoverFallback orchestration](335-discover-fallback-orchestration.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 336: codepoint overrides](336-codepoint-overrides.md) — **Pass** ·
  Claude/Codex/Codex
- [Experiment 337: the shaper's output cell](337-shaper-cell.md) — **Pass** ·
  Claude/Codex/Codex
- [Experiment 338: the CoreText shaping core](338-shaper-coretext-core.md) —
  **Pass** · Claude/Codex/Codex
- [Experiment 339: the shaper's glyph offsets](339-shaper-glyph-offsets.md) —
  **Pass** · Claude/Codex/Codex
- [Experiment 340: the shaper's non-LTR run sort](340-shaper-nonltr-sort.md) —
  **Pass** · Claude/Codex/Codex
- [Experiment 341: the shaper's cluster→cell mapping](341-shaper-cluster-cell-map.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 342: the shaper's clustered input](342-shaper-clustered-input.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 343: the shaper's reorder guard](343-shaper-reorder-guard.md) —
  **Pass** · Claude/Codex/Codex
- [Experiment 344: the shaper's `is_first` term](344-shaper-is-first-term.md) —
  **Pass** · Claude/Codex/Codex
- [Experiment 345: applying variations to a face](345-face-set-variations.md) —
  **Pass** · Claude/Codex/Codex
- [Experiment 346: the score's variation-axis derivation](346-score-variation-axes.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 347: default font features in shaping](347-shaper-default-features.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 348: the feature-string parser](348-shaper-feature-parser.md) —
  **Pass** · Claude/Codex/Codex
- [Experiment 349: threading user features into shaping](349-shaper-user-features.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 350: the special-font fast path](350-shaper-special-font.md) —
  **Pass** · Claude/Codex/Codex
- [Experiment 351: the run iterator's grapheme font resolution](351-runiter-grapheme-index.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 352: the run iterator's break-and-style helpers](352-runiter-break-helpers.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 353: the run iterator's TextRun and style break](353-runiter-textrun-style.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 354: the run iterator's content hash](354-runiter-run-hash.md) —
  **Pass** · Claude/Codex/Codex
- [Experiment 355: the run iterator's input view](355-runiter-input-view.md) —
  **Pass** · Claude/Codex/Codex
- [Experiment 356: the run iterator's core grouping loop](356-runiter-next-core.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 357: the run iterator's spacer skip and selection/cursor breaks](357-runiter-breaks.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 358: decoding a terminal row into RunCells](358-render-runcell-extract.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 359: assembling RunOptions for the viewport](359-render-run-options.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 360: the Screen/Terminal-facing shape entry](360-render-screen-entry.md)
  — **Pass** · Claude/Codex/Codex
- [Experiment 361: the shape-a-row driver](361-shape-row-driver.md) — **Pass** ·
  Claude/Codex/Codex
- [Experiment 362: shaping the whole viewport](362-shape-viewport.md) — **Pass**
  · Claude/Codex/Codex
- [Experiment 363: the shared font grid (render path)](363-shared-grid.md) —
  **Pass** · Claude/Codex/Codex

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
