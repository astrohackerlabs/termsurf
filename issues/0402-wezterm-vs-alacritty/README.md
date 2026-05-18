+++
status = "closed"
opened = "2026-02-08"
closed = "2026-02-08"
+++

# Issue 402: WezTerm vs Alacritty

## The Question

Which terminal library should TermSurf 4.0 use, and should the terminal run
in-process with the window or as a separate process?

These two questions are interrelated. Alacritty favors in-process (it's a clean
library, but requires writing a wgpu text renderer from scratch). WezTerm favors
either configuration (it already has wgpu rendering and a richer font stack, but
is heavier and more coupled).

## The Two Terminal Libraries

### Alacritty

**Terminal core:** `alacritty_terminal` — a library crate explicitly designed
for embedding. "Library for writing terminal emulators."

| Aspect       | Details                                                               |
| ------------ | --------------------------------------------------------------------- |
| Crate        | `alacritty_terminal`                                                  |
| Dependencies | ~15 (vte, libc, log, parking_lot, etc.)                               |
| API          | `Term<T>` generic over `EventListener`, `Grid<Cell>`, PTY abstraction |
| Rendering    | None — pure state. Embedder reads grid and renders however they want  |
| GPU backend  | None in library. GUI uses OpenGL via glutin + crossfont               |
| Text shaping | `crossfont` in the GUI binary (not in the library)                    |
| PTY          | Built-in (`alacritty_terminal::tty`), Unix + Windows                  |
| Complexity   | Low. 4 crates total in workspace                                      |
| License      | Apache 2.0                                                            |

**Strengths:**

- Minimal, clean, embeddable
- No rendering opinions — we provide our own
- Small dependency tree
- Well-documented trait-based API (`EventListener`, `Dimensions`)
- Widely used and battle-tested

**Weaknesses:**

- No wgpu code anywhere — we must write GPU text rendering from scratch
- `crossfont` (its font rasterizer) is OpenGL-only
- No built-in font fallback chain (that's in the GUI binary)
- No multiplexer — one `Term<T>` per terminal instance

### WezTerm

**Terminal core:** `wezterm-term` — a library crate with richer features. "The
Virtual Terminal Emulator core from wezterm; helpful for implementing terminal
emulators."

| Aspect       | Details                                                                 |
| ------------ | ----------------------------------------------------------------------- |
| Crate        | `wezterm-term`                                                          |
| Dependencies | ~27 (termwiz, wezterm-cell, wezterm-escape-parser, etc.)                |
| API          | `Terminal` struct, `advance_bytes()` for input, screen/scrollback model |
| Rendering    | None in core. GUI uses wgpu (with OpenGL fallback)                      |
| GPU backend  | wgpu in `wezterm-gui` (proven, shipping code)                           |
| Text shaping | `wezterm-font` crate (FreeType + HarfBuzz + platform font discovery)    |
| PTY          | `portable-pty` separate crate (published on crates.io)                  |
| Complexity   | High. 29+ crates in workspace                                           |
| License      | MIT                                                                     |

**Additional reusable crates:**

| Crate                   | Purpose                                     | Separable?               |
| ----------------------- | ------------------------------------------- | ------------------------ |
| `portable-pty`          | Cross-platform PTY                          | Yes, fully independent   |
| `wezterm-font`          | Font loading, shaping (FreeType + HarfBuzz) | Yes, modular             |
| `termwiz`               | TUI primitives, cell types, escape parsing  | Yes, library-quality     |
| `wezterm-cell`          | Cell attributes and colors                  | Yes                      |
| `wezterm-escape-parser` | VTE escape sequence parser                  | Yes                      |
| `mux`                   | Tab/pane/window multiplexer                 | Partially (Lua coupling) |

**Strengths:**

- wgpu rendering already exists and ships to users
- `wezterm-font` is a complete font stack (FreeType + HarfBuzz + Core Text /
  FontConfig / GDI for discovery)
- Richer terminal emulation (sixel, iTerm2 images, bidi text, semantic zones)
- `portable-pty` is a clean, published crate
- Multiplexer (`mux`) handles multiple panes natively

**Weaknesses:**

- wgpu renderer is in `wezterm-gui`, tightly coupled to WezTerm's pane model
- Large dependency tree (~200+ transitive deps with Lua, SSH, config)
- `wezterm-term` depends on 6-7 WezTerm ecosystem crates
- Extracting the wgpu renderer requires significant refactoring
- Lua scripting deeply embedded (20+ crates)

## The Two Architectures

### Architecture A: Terminal In-Process

```
┌─────────────────────────────────────────────┐
│ Window Process                              │
│ ├── winit + wgpu (window, compositor)       │
│ ├── terminal library (in-process)           │
│ ├── GPU text renderer (in-process)          │
│ ├── termsurf-xpc (browser communication)    │
│ └── pane/tab manager                        │
│      │                                      │
│      │ XPC (IOSurface + input)              │
│      ▼                                      │
│ Browser Profile Process (C++)               │
└─────────────────────────────────────────────┘
```

The terminal library runs inside the window process. Terminal rendering is a
function call, not IPC. Input goes directly to the terminal; grid state is read
directly for rendering.

### Architecture B: Terminal Out-of-Process

```
┌─────────────────────────────────────────────┐
│ Window Process                              │
│ ├── winit + wgpu (window, compositor)       │
│ ├── termsurf-xpc (terminal + browser comm)  │
│ └── pane/tab manager                        │
│      │                    │                 │
│      │ XPC                │ XPC             │
│      ▼                    ▼                 │
│ Terminal Process    Browser Profile (C++)   │
│ ├── terminal lib    ├── Chromium            │
│ ├── GPU text render ├── OSR view            │
│ ├── IOSurface out   ├── IOSurface out       │
│ └── XPC (input in)  └── XPC (input in)      │
└─────────────────────────────────────────────┘
```

The terminal runs as a separate process, just like the browser. Both send GPU
textures (IOSurface) to the window via XPC. Both receive input (keyboard, mouse)
from the window via XPC. The window process is a pure compositor.

## Comparison Matrix

### Alacritty + In-Process (A1)

| Factor             | Assessment                                                         |
| ------------------ | ------------------------------------------------------------------ |
| Integration effort | Low — embed `alacritty_terminal`, call `term.lock()`               |
| GPU text rendering | **Must write from scratch** (~1500 lines wgpu)                     |
| Font stack         | Must use `cosmic-text` + `glyphon`, or write own FreeType bindings |
| Typing latency     | Optimal — no IPC for keystrokes                                    |
| Dependencies       | ~15 (lightest option)                                              |
| Window language    | Must be Rust (terminal is in-process)                              |
| Complexity         | Low overall, but text renderer is unknown work                     |
| Risk               | wgpu text rendering quality is unproven                            |

### Alacritty + Out-of-Process (A2)

| Factor             | Assessment                                          |
| ------------------ | --------------------------------------------------- |
| Integration effort | Medium — separate process with XPC                  |
| GPU text rendering | **Must write from scratch** in the terminal process |
| Font stack         | Same as A1 — must provide font rendering            |
| Typing latency     | +~2ms per frame (IOSurface Mach port transfer)      |
| Dependencies       | ~15 in terminal process, minimal in window          |
| Window language    | Any — window is just a compositor                   |
| Complexity         | Medium — same XPC protocol as browser               |
| Risk               | wgpu text rendering + process management            |

### WezTerm + In-Process (W1)

| Factor             | Assessment                                                            |
| ------------------ | --------------------------------------------------------------------- |
| Integration effort | Medium — embed `wezterm-term` + `wezterm-font` + adapt rendering      |
| GPU text rendering | **Reference code exists** in `wezterm-gui`, needs extraction          |
| Font stack         | `wezterm-font` is complete (FreeType + HarfBuzz + platform discovery) |
| Typing latency     | Optimal — no IPC                                                      |
| Dependencies       | ~50+ (wezterm-term + font + ecosystem crates)                         |
| Window language    | Must be Rust                                                          |
| Complexity         | Medium — more dependencies, but rendering is solved                   |
| Risk               | Extracting wgpu renderer from wezterm-gui coupling                    |

### WezTerm + Out-of-Process (W2)

| Factor             | Assessment                                                        |
| ------------------ | ----------------------------------------------------------------- |
| Integration effort | Medium — separate process with XPC                                |
| GPU text rendering | **Reference code exists**, can fork wezterm-gui as starting point |
| Font stack         | `wezterm-font` included                                           |
| Typing latency     | +~2ms per frame                                                   |
| Dependencies       | ~50+ in terminal process, minimal in window                       |
| Window language    | Any                                                               |
| Complexity         | Medium — same XPC protocol as browser                             |
| Risk               | Process management, but rendering is proven                       |

## Deep Analysis

### The GPU Text Rendering Question

This is the decisive factor. Terminal text rendering on the GPU requires:

1. **Font loading** — Find and load system fonts by family name
2. **Text shaping** — Convert Unicode codepoints to positioned glyphs (HarfBuzz)
3. **Glyph rasterization** — Render glyphs to bitmaps (FreeType or Core Text)
4. **Glyph atlas** — Pack glyph bitmaps into a GPU texture atlas
5. **Instanced rendering** — Draw a textured quad per glyph on the grid

**Alacritty's approach:** crossfont handles 1-3, the GUI handles 4-5 via OpenGL.
None of this is wgpu-compatible. We'd start from scratch for steps 4-5, and
either use crossfont (OpenGL-dependent) or find a wgpu-compatible alternative
for 1-3.

**WezTerm's approach:** `wezterm-font` handles 1-3 with FreeType + HarfBuzz (no
OpenGL dependency — the font crate is pure CPU). The GUI handles 4-5 via wgpu.
The glyph atlas (`glyphcache.rs`), vertex layout (`quad.rs`), and wgpu pipeline
(`webgpu.rs`) are working code. They're coupled to WezTerm's pane model, but the
GPU concepts (atlas packing, instanced quads, vertex format) transfer directly.

**The gap:**

| Step                | Alacritty               | WezTerm                                      |
| ------------------- | ----------------------- | -------------------------------------------- |
| Font loading        | crossfont (OpenGL-tied) | `wezterm-font` (CPU-only, reusable)          |
| Text shaping        | crossfont               | HarfBuzz via `wezterm-font`                  |
| Glyph rasterization | crossfont               | FreeType via `wezterm-font`                  |
| Glyph atlas (GPU)   | OpenGL (not reusable)   | wgpu (in wezterm-gui, coupled but reference) |
| Instanced rendering | OpenGL (not reusable)   | wgpu (in wezterm-gui, coupled but reference) |

With Alacritty, we write steps 1-5 from scratch (or find new libraries). With
WezTerm, we get steps 1-3 for free (`wezterm-font`) and have working reference
code for steps 4-5.

### The Typing Latency Question

The concern with out-of-process terminal is typing latency. Let's measure it:

**In-process path:**

```
Keystroke → winit event → term.input(bytes)     → PTY write
PTY read  → term.advance_bytes() → grid updated → render → present
Total additional: 0ms (all in-process)
```

**Out-of-process path:**

```
Keystroke → winit event → XPC send (key event)  → terminal process
                                                 → term.input(bytes) → PTY write
PTY read  → term.advance_bytes() → grid updated → render to IOSurface
IOSurface → Mach port → XPC send                → window process
                                                 → import → composite → present
Total additional: ~2-3ms (XPC round trip + IOSurface transfer)
```

The additional latency is ~2-3ms. For context:

- 60fps frame time: 16.7ms
- Human perception threshold for typing latency: ~30-50ms
- Typical terminal latency (SSH): 20-200ms

**2-3ms is imperceptible.** The out-of-process path is viable for terminal use.

And critically: **we must solve this exact problem for the browser anyway.** The
browser absolutely requires high-performance input forwarding and GPU texture
output via XPC. Whatever input/output protocol we build for the browser works
identically for the terminal. Building it once and using it twice is less total
work than building two different rendering paths (in-process terminal + IPC
browser).

### The Language Flexibility Question

With in-process terminal, the window process must be Rust (because
`alacritty_terminal` and `wezterm-term` are Rust libraries). This is fine for
now, but it locks a decision.

With out-of-process terminal, the window process can be any language:

- **Rust** with winit + wgpu (cross-platform)
- **Swift** with AppKit + Metal (native macOS)
- **C++** with SDL2 + Vulkan/Metal

The window becomes a pure compositor: receive textures, composite them, present.
Route input to the right process. Manage pane layout. No terminal emulation, no
font rendering, no VTE parsing.

This is a simpler, more focused process. And it means we can optimize the window
for each platform without touching terminal or browser code.

### The Protocol Symmetry Argument

The user's insight: the browser must have maximum-performance XPC for input and
GPU texture output. If we build that protocol, using it for terminal too is
near-zero additional work.

The protocol for both is identical:

```
Window → Pane Process:
  - key_event(keycode, modifiers, type)
  - mouse_event(x, y, type, button, modifiers)
  - scroll_event(dx, dy, phase)
  - resize(width, height, scale)
  - focus(gained: bool)

Pane Process → Window:
  - iosurface_port(mach_port, width, height)
  - title_changed(string)
  - cursor_changed(cursor_type)
```

Whether "Pane Process" is a terminal or a browser, the window doesn't care. It
receives an IOSurface and composites it. It sends input events and forgets about
them. This is clean, symmetric, and testable.

### WezTerm's Weight Problem

WezTerm's full dependency tree is heavy: Lua scripting (20+ crates), SSH,
multiplexer, configuration system, client-server protocol. If we embed
`wezterm-term` in-process, we drag in:

```
wezterm-term
├── termwiz
│   ├── wezterm-cell
│   ├── wezterm-escape-parser
│   ├── wezterm-surface
│   └── wezterm-input-types
├── wezterm-dynamic
├── wezterm-bidi
└── ... (27 direct deps total)
```

This is manageable but significantly heavier than `alacritty_terminal` (~15
deps). And if we also want `wezterm-font`, we add FreeType, HarfBuzz, Cairo, and
platform font discovery.

However, if the terminal is out-of-process, this weight lives in the terminal
process, not the window. The window stays lean.

### Alacritty's Simplicity Advantage

Alacritty's `alacritty_terminal` is beautifully minimal. The `Term<T>` generic
is elegant — implement `EventListener`, provide dimensions, and you have a
working terminal. The grid iteration API is clean. The dependency tree is tiny.

But this simplicity means we must solve font rendering ourselves. The options:

1. **cosmic-text + glyphon** — Rust text layout + wgpu text rendering. Modern,
   wgpu-native, but less battle-tested for terminal use (designed for UI text,
   not monospace grids).

2. **Write our own glyph atlas** — Use FreeType/HarfBuzz directly (via
   `freetype-rs` and `harfbuzz_rs` crates), build a glyph atlas, write wgpu
   shaders. ~1500 lines. Full control, but significant effort.

3. **Port Alacritty's renderer** — Translate Alacritty's OpenGL renderer to
   wgpu. The concepts are the same (glyph atlas, instanced quads). ~2000 lines
   of translation work.

4. **Port WezTerm's renderer** — Even if we use Alacritty for terminal
   emulation, we could port WezTerm's wgpu renderer concepts. This gives us
   proven wgpu patterns without WezTerm's terminal coupling.

### What "Fork" Really Means

Neither library needs to be forked in the git sense. Both are library crates:

- **Alacritty:** Add `alacritty_terminal` as a Cargo dependency. No fork needed.
- **WezTerm:** Add `wezterm-term`, `wezterm-font`, `portable-pty` as path or git
  dependencies. The crates are designed for external use.

"Forking" only applies if we need to modify the library internals. For initial
integration, both work as dependencies.

## Scoring

Each option scored 1-5 (5 = best) on factors relevant to TermSurf 4.0:

| Factor                      | Weight | A1     | A2      | W1     | W2      |
| --------------------------- | ------ | ------ | ------- | ------ | ------- |
| Integration simplicity      | 3      | 5      | 3       | 3      | 3       |
| GPU text rendering          | 5      | 1      | 1       | 4      | 4       |
| Font stack quality          | 4      | 2      | 2       | 5      | 5       |
| Typing latency              | 2      | 5      | 4       | 5      | 4       |
| Dependency weight           | 2      | 5      | 5       | 2      | 3       |
| Window language flexibility | 3      | 2      | 5       | 2      | 5       |
| Protocol symmetry (reuse)   | 4      | 1      | 5       | 1      | 5       |
| Cross-platform readiness    | 3      | 4      | 4       | 4      | 4       |
| Risk (unknowns)             | 4      | 2      | 3       | 4      | 4       |
| **Weighted total**          |        | **78** | **101** | **97** | **125** |

Breakdown:

- **A1** (Alacritty in-process): 3×5 + 5×1 + 4×2 + 2×5 + 2×5 + 3×2 + 4×1 + 3×4 +
  4×2 = **78**
- **A2** (Alacritty out-of-process): 3×3 + 5×1 + 4×2 + 2×4 + 2×5 + 3×5 + 4×5 +
  3×4 + 4×3 = **101**
- **W1** (WezTerm in-process): 3×3 + 5×4 + 4×5 + 2×5 + 2×2 + 3×2 + 4×1 + 3×4 +
  4×4 = **97**
- **W2** (WezTerm out-of-process): 3×3 + 5×4 + 4×5 + 2×4 + 2×3 + 3×5 + 4×5 +
  3×4 + 4×4 = **125**

## Conclusion

### Recommended: WezTerm out-of-process (W2)

**WezTerm as an out-of-process terminal** is the strongest option. Here's why:

**1. GPU text rendering is the hardest problem, and WezTerm has solved it.**

`wezterm-font` provides FreeType + HarfBuzz font loading and shaping as a
reusable Rust crate. WezTerm's `wezterm-gui` has a working wgpu renderer with
glyph atlas, instanced quad rendering, and sub-pixel antialiasing. We don't need
to extract it cleanly — the terminal process can start as a simplified fork of
`wezterm-gui` that strips out tabs, config UI, Lua, and SSH, keeping only the
terminal rendering core. Over time, we refactor it into something cleaner.

**2. Out-of-process gives protocol symmetry.**

The browser must use XPC + IOSurface. Building that protocol for terminal too
means we build one IPC layer, test it once, and use it for both pane types. The
window process becomes a pure compositor — it doesn't care whether a texture
comes from a terminal or a browser.

**3. Out-of-process gives language flexibility.**

The window process can be Rust, Swift, or anything else. It doesn't embed any
terminal library. If we later decide Swift + Metal is better for macOS, we
rewrite only the compositor — the terminal and browser processes don't change.

**4. The latency cost is negligible.**

~2-3ms additional latency for terminal keystrokes is imperceptible. We must
achieve this performance for the browser anyway (mouse/keyboard forwarding at
high rate). If our IPC is too slow for terminal, it's too slow for browser, and
the whole architecture fails. So the performance constraint is shared — solving
it for one solves it for both.

**5. WezTerm's weight is isolated.**

WezTerm's heavy dependencies (Lua, SSH, config) live in the terminal process.
The window process stays lean. If we later want to swap WezTerm for Alacritty,
we write a new terminal process — the window doesn't change.

### What This Looks Like

```
┌─────────────────────────────────────────────────────┐
│ Window Process (Rust or Swift)                      │
│ ├── Window framework (winit or AppKit)              │
│ ├── GPU compositor (wgpu or Metal)                  │
│ │   └── composites IOSurface textures from all panes│
│ ├── XPC client (termsurf-xpc)                       │
│ │   └── same protocol for terminal + browser panes  │
│ ├── Pane layout manager                             │
│ ├── Tab manager                                     │
│ └── Input router (keyboard/mouse → correct pane)    │
│      │                      │                       │
│      │ XPC                  │ XPC                   │
│      ▼                      ▼                       │
│ Terminal Process       Browser Profile (C++)        │
│ (Rust)                                              │
│ ├── wezterm-term       ├── Chromium Content API     │
│ ├── wezterm-font       ├── Custom OSR view          │
│ ├── portable-pty       ├── IOSurface output         │
│ ├── wgpu renderer      └── XPC (libxpc)             │
│ ├── IOSurface output                                │
│ └── XPC (termsurf-xpc)                              │
└─────────────────────────────────────────────────────┘
```

### Why Not Alacritty?

Alacritty is a better library in isolation — cleaner API, fewer dependencies,
more elegant design. But TermSurf 4.0 needs GPU text rendering on wgpu, and
Alacritty provides none of the GPU infrastructure. We'd spend weeks writing a
wgpu text renderer that WezTerm already has.

If we discover that WezTerm's wgpu renderer is too coupled to extract, we can
still fall back to Alacritty + cosmic-text/glyphon. The out-of-process
architecture makes this swap possible without changing the window.

### Why Not In-Process?

In-process terminal is simpler for a v0.1 prototype. But it locks us into Rust
for the window, creates an asymmetry between terminal (in-process, direct
rendering) and browser (out-of-process, IPC rendering), and means we build two
different rendering paths.

Out-of-process is slightly more work upfront but pays off immediately: one
protocol, one compositor, swappable components.

### Revision to Issue 401

This analysis changes the recommendation from Issue 401
(programming-language.md). The updated architecture:

| Process         | Language              | Change from 401              |
| --------------- | --------------------- | ---------------------------- |
| Window          | Rust (or Swift later) | Terminal removed from window |
| Terminal        | Rust (WezTerm crates) | **New: separate process**    |
| Browser profile | C++ (Chromium)        | Unchanged                    |
| Launcher        | Rust (or none)        | Unchanged                    |

The window process is now simpler (~3000 lines instead of ~5500) because it
doesn't embed terminal emulation or text rendering. The terminal process is new
(~4000 lines: wezterm-term + wezterm-font + wgpu renderer + XPC) but can start
as a stripped-down wezterm-gui.

### Starting Point

1. **Terminal process first.** Take `wezterm-gui`, strip it down to: open a
   window, render a terminal, no tabs, no config UI, no Lua. Verify it builds
   and runs as a standalone terminal.

2. **Replace the window.** Instead of rendering to a window, render to an
   IOSurface. Send the IOSurface via XPC to a test receiver.

3. **Build the window process.** winit + wgpu compositor that receives IOSurface
   textures and composites them. Start with one terminal pane.

4. **Add browser.** Follow Issue 401 (chromium-feasibility) to build the C++
   browser profile process. Same XPC protocol, same IOSurface delivery.

5. **Multi-pane.** Layout manager, input routing, tab management.
