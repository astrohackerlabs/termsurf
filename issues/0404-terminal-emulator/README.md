+++
status = "closed"
opened = "2026-02-08"
closed = "2026-02-08"
+++

# Issue 404: Terminal Emulator Selection

## Goal

Choose which terminal emulator to integrate into ts4's terminal process. The
language of the terminal process is not predetermined — it follows from
whichever emulator fits best. If the best emulator is written in Zig, the
terminal process is Zig. If Rust, Rust. If C, C.

The terminal process runs headless — no window, no event loop, no display
connection. It renders text to an IOSurface backed by Metal, creates a Mach
port, and sends it to the Swift compositor via XPC. The emulator must fit this
architecture.

## Candidates

1. Ghostty (Zig)
2. iTerm2 (Objective-C)
3. Alacritty (Rust)
4. Kitty (C/Python)
5. WezTerm (Rust)

We may support more than one eventually. But we start with one.

## Architectural Context

The terminal process in ts4 is not an application. It has no window. It receives
input events over XPC, drives a PTY, and renders the terminal grid to a GPU
texture that gets sent to another process. This is fundamentally different from
how every terminal emulator is designed — they all assume they own the window.

```
XPC input events ──▶ Terminal Process ──▶ IOSurface ──▶ Mach port ──▶ Swift window
                          │
                          ▼
                     PTY ──▶ shell
```

The emulator must be decomposable into parts we can use independently:

1. **Terminal state** — VTE parsing, cell grid, scrollback, selection
2. **Text rendering** — Font loading, shaping, glyph rasterization, atlas
3. **GPU rendering** — Drawing the cell grid to a texture
4. **Input handling** — Translating keyboard/mouse events to terminal sequences
5. **PTY management** — Spawning shells, reading/writing the PTY fd

We do NOT need:

- Window creation or management (Swift owns the window)
- Event loop or display server connection (no window)
- Menu bar, tabs, splits, or any UI chrome (Swift handles all UI)
- Configuration file parsing (we have our own config)
- Clipboard integration (Swift handles pasteboard)

## Evaluation Criteria

### 1. Library extractability

**Weight: Critical**

Can the terminal's core be used as a library without its application shell? This
is the single most important criterion. If the rendering pipeline is entangled
with window management, event loops, and platform UI, extracting it is a
rewrite.

Questions to answer:

- Is the codebase architecturally split into library and application?
- Is there a defined API boundary between the core and the platform shell?
- Can the renderer be instantiated without creating a window?
- Are there existing examples of third parties embedding the core?

### 2. Offscreen rendering capability

**Weight: Critical**

The terminal process has no window and no display connection. The renderer must
be able to target an offscreen texture — specifically, a Metal `MTLTexture`
backed by an `IOSurface`.

Questions to answer:

- Can the renderer target an offscreen texture instead of a window drawable?
- Does the renderer assume it draws to a `CAMetalLayer` or swap chain?
- How deeply is the rendering pipeline coupled to the window surface?
- What would it take to redirect rendering output to an IOSurface?

### 3. Rendering backend

**Weight: High**

On macOS, Metal is the only native GPU API. Everything else is a translation
layer on top of Metal:

- **wgpu** translates to Metal via its `wgpu-hal` Metal backend
- **OpenGL** translates to Metal via Apple's deprecated compatibility layer (no
  longer maintained, but still functional)
- **Vulkan** translates to Metal via MoltenVK

There is no way to talk to the GPU on macOS without going through Metal. Any of
these APIs can work — the IOSurface just needs to end up as a Metal texture at
some point. But using Metal directly is the shortest path: no translation layer
to configure, and `MTLTexture` has native IOSurface support via
`device.makeTexture(descriptor:iosurface:plane:)`. With wgpu or OpenGL, you must
drop into the underlying Metal layer to set up the IOSurface backing.

Questions to answer:

- What GPU API does the renderer use? (Metal, wgpu, OpenGL, custom)
- If not Metal, how much work is needed to access the underlying Metal texture
  for IOSurface backing?
- Does the renderer use any platform-specific GPU abstractions that complicate
  offscreen use?

### 4. Terminal state separation

**Weight: High**

The VTE parser and terminal state machine (cell grid, scrollback, cursor,
selection, etc.) should be separable from the renderer. Even if we can't use the
emulator's renderer directly (e.g., it's OpenGL), we might use its terminal
state library with our own Metal renderer.

Questions to answer:

- Is the terminal state (parser + grid) in a separate module/crate/library?
- Can the terminal state be driven without a renderer attached?
- What is the API for querying the cell grid? (iterate cells, get attributes,
  get cursor position)
- Is the state thread-safe? (our XPC event handler and render loop may be on
  different threads)

### 5. Text rendering pipeline

**Weight: High**

Terminal text rendering requires font discovery, text shaping (for ligatures and
complex scripts), glyph rasterization, and atlas management. This is the most
complex part of a terminal renderer and the hardest to build from scratch.

Questions to answer:

- What does the font pipeline use? (CoreText, HarfBuzz, FreeType, fontconfig)
- Is text shaping separated from GPU rendering?
- Is the glyph atlas reusable with a different GPU backend?
- Does it support ligatures, color emoji, bold/italic, and variable fonts?
- How are glyphs rasterized? (CPU via CoreText/FreeType, or GPU?)

### 6. Language and XPC compatibility

**Weight: Medium**

The terminal process communicates with the Swift window over XPC (C API). The
emulator's language determines the terminal process's language. Any language
that can call C functions works — the XPC and IOSurface APIs are all C.

| Language        | XPC / IOSurface integration                              |
| --------------- | -------------------------------------------------------- |
| Zig             | C interop is native — call XPC/IOSurface C APIs directly |
| Rust            | Via FFI — already proven in ts4 prototype                |
| C / Objective-C | Native — XPC and IOSurface are C APIs                    |
| C++             | Native — C APIs callable directly from C++               |
| Python          | Possible via ctypes but impractical for GPU rendering    |

Questions to answer:

- What language is the core library written in?
- Can that language call C APIs directly? (XPC, IOSurface, Mach ports)
- Does the emulator expose a C-ABI compatible library interface?
- How natural is Metal / IOSurface integration from that language?

### 7. Input injection

**Weight: Medium**

The terminal process receives keyboard and mouse events from the Swift window
over XPC as structured data (key code, modifiers, mouse position). It must
translate these into terminal sequences and write them to the PTY.

Questions to answer:

- Can input events be injected programmatically? (not from an OS event)
- Is the key-to-sequence mapping separable from the platform input system?
- Does the emulator support mouse reporting modes? (SGR, X10, etc.)
- How does it handle IME / dead keys / compose sequences?

### 8. PTY management

**Weight: Low**

PTY creation and management is straightforward (`forkpty`, `read`, `write`).
Most emulators handle this similarly. We could also use our own PTY code and
just feed bytes to the terminal state machine.

Questions to answer:

- Is PTY management separable from the rest of the emulator?
- Can we feed raw bytes to the parser instead of reading from a PTY?
- Does it support custom shell commands and environment variables?

### 9. Feature coverage

**Weight: Low (for initial selection)**

All five candidates are mature terminal emulators with extensive feature sets.
Feature gaps can be filled later. But some features are harder to add after the
fact.

Features to compare:

- Sixel / iTerm2 image protocol / Kitty image protocol
- Unicode width handling (EAW, grapheme clusters)
- URL detection and OSC 8 hyperlinks
- True color (24-bit) and 256-color support
- Scrollback buffer size and performance
- Shell integration (OSC 133, etc.)

### 10. License

**Weight: Medium**

TermSurf is not yet open source. The license must permit embedding the terminal
core as a library in a proprietary application (for now).

| License    | Embedding OK? | Notes                              |
| ---------- | ------------- | ---------------------------------- |
| MIT        | Yes           | No restrictions                    |
| Apache 2.0 | Yes           | Patent grant, notice required      |
| GPLv2      | No            | Must open-source the combined work |
| GPLv3      | No            | Must open-source the combined work |

Questions to answer:

- What license does the emulator use?
- Are there any dual-licensing options?
- Do dependencies introduce additional license constraints?

### 11. Build complexity

**Weight: Low**

Three build systems already coexist in ts4 (SwiftPM, Cargo, Make). Adding a
fourth is acceptable if necessary, but simpler is better.

Questions to answer:

- What build system does the emulator use?
- What are the native dependencies? (system libraries, frameworks)
- How long does a clean build take?
- Can it be built as a static library or standalone process?

## Research

### Ghostty (Zig)

**Library extractability:** Ghostty is architecturally split into **libghostty**
(a C-ABI library) and platform-specific application shells (Swift on macOS, GTK
on Linux). The C API boundary is extensive — `ghostty_surface_key()`,
`ghostty_surface_mouse_button()`, `ghostty_surface_draw()`,
`ghostty_surface_set_size()`, and dozens more exported functions. A smaller
**libghostty-vt** (terminal state + VTE parser only, zero dependencies) is
available in public alpha. The macOS app is pure Swift consuming libghostty via
its C API. Third-party embedding exists: gpui-ghostty embeds Ghostty's terminal
core into Zed's GPUI framework. The build system produces static libraries
(`libghostty-fat.a`).

**Offscreen rendering:** This is the standout finding. **Ghostty already renders
to IOSurface-backed textures internally.** The Metal render target
(`src/renderer/metal/Target.zig`) creates an IOSurface, then creates an
`MTLTexture` backed by it via `newTextureWithDescriptor:iosurface:plane:`. The
renderer draws into this texture, and the `present()` function simply sets the
IOSurface as the `contents` property of a CALayer. The coupling to the window
is minimal — to redirect to XPC Mach ports, you would: (1) skip the
IOSurfaceLayer/view attachment in `Metal.init()`, (2) replace `present()` with
`IOSurfaceCreateMachPort()` + XPC send, (3) provide an alternative to
`surfaceSize()`. The rendering pipeline itself is view-independent.

**Rendering backend:** Metal directly via Zig's Objective-C interop. No MetalKit,
no abstraction layer — talks directly to `MTLDevice`, `MTLCommandQueue`,
`MTLRenderPassDescriptor`. Uses triple buffering with IOSurfaces.

**Terminal state separation:** Fully separate. The terminal state lives in
`src/terminal/` — `Terminal.zig` (state machine), `Screen.zig` (page list,
cursor, selection), `page.Cell` (individual cells), `PageList.zig` (grid
storage). A `RenderState` struct converts terminal state into renderer-friendly
form. No rendering dependencies in any of these files.

**Text rendering:** Compile-time selectable backends. macOS default: CoreText for
discovery, rendering, and shaping. Alternatives: CoreText + HarfBuzz shaping,
CoreText + FreeType rendering. Glyph atlas uses rectangle bin-packing with
atomic dirty tracking. Supports ligatures (via HarfBuzz backend), color emoji,
bold/italic. Font grids can be shared across surfaces with matching configs.

**Language:** Zig with first-class C interop via `@cImport`. Already uses
IOSurface, Metal, and Mach APIs directly. XPC would be called the same way —
`extern` declarations for C functions.

**Input injection:** Full programmatic injection via C API:
`ghostty_surface_key()`, `ghostty_surface_mouse_button()`,
`ghostty_surface_mouse_scroll()`, `ghostty_surface_text()`,
`ghostty_surface_preedit()`. These accept structured event data — no OS event
objects required.

**PTY:** Separable. `src/pty.zig` provides platform-specific implementations.
Raw bytes can be fed directly to the `Parser`/`Stream` → `Terminal` pipeline,
bypassing the PTY entirely.

**Features:** Kitty graphics protocol (full), Unicode with grapheme clusters, OSC
8 hyperlinks, 24-bit color, shell integration (bash/zsh/fish/elvish). No Sixel.
No iTerm2 image protocol.

**License:** MIT.

**Build:** Zig build system. Produces static library on macOS via `libtool`.
Requires LLVM backend on macOS. Clean build 1–3 minutes. Dependencies: system
Metal/CoreText/IOSurface frameworks, optional FreeType/HarfBuzz.

### iTerm2 (Objective-C)

**Library extractability:** Monolithic macOS application with no library/API
boundary. All components — VT100 parsing, screen state, PTY, rendering, window
management, preferences — live in a single flat `sources/` directory. The author
acknowledged that library extraction "would not be hard to bring back" but it
was never done. No third party has embedded iTerm2's core.

**Offscreen rendering:** The Metal renderer (`iTermMetalDriver`) renders via
MTKView delegate callbacks. It uses intermediate offscreen textures for subpixel
antialiasing, but the final target is always the MTKView's framebuffer.
Redirecting to IOSurface would require replacing the MTKView-driven rendering
entry point.

**Rendering backend:** Metal 2 with a sophisticated multi-pass pipeline — 17+
renderer classes (background, text, cursor, images, marks, margins, etc.) each
handling one visual layer. Uses a transient state pattern: terminal state is
snapshot on the main thread and consumed on the GPU thread.

**Terminal state separation:** Conceptual separation exists — `VT100Terminal`
(parser), `VT100Screen` (grid/scrollback), `VT100Grid` (cells),
`screen_char_t` (cell type). But all are Objective-C classes with deep imports
of iTerm2-specific headers (preferences, profiles, notifications). Extracting
them requires stubbing dozens of dependencies.

**Text rendering:** CoreGraphics for default path (fast, no ligatures), CoreText
for ligature path (slow, disables GPU renderer entirely). Glyph atlas via
`iTermTextureArray` (Metal texture array). Handles emoji, Powerline glyphs
(rendered synthetically), wide characters.

**Language:** Objective-C — a strict superset of C. XPC, IOSurface, Metal, and
Mach APIs are all native. The language is not the problem; the monolithic
architecture is.

**Input injection:** `writeTask:` writes bytes to the PTY fd. Python scripting
API provides `Session.async_send_text()`. Works within the app, but no external
embedding API.

**PTY:** `PTYTask` handles forkpty/fork/exec. Moderately coupled to PTYSession
and the app's notification system.

**Features:** The most feature-rich terminal on macOS. Sixel (via libsixel),
iTerm2 inline image protocol (originator), tmux `-CC` integration, shell
integration, triggers, semantic history, annotations, Python scripting API.
No Kitty image protocol.

**License:** GPLv2-or-later. Embedding any iTerm2 code requires open-sourcing
the entire application under GPLv2. This is a hard blocker.

**Build:** Complex Xcode project + Makefile for vendored dependencies (libssh2,
libsixel, OpenSSL, etc.). Non-trivial build system.

### Alacritty (Rust)

**Library extractability:** Split into a Cargo workspace with four crates.
**`alacritty_terminal`** is a standalone library on crates.io designed for
embedding. It provides `Term<T>` (terminal state machine), `Grid` (cell grid),
`Cell` (content + attributes), PTY management, and VTE parsing. There is no
separate renderer crate — the OpenGL renderer lives inside the `alacritty`
application crate. **Zed editor uses `alacritty_terminal` as a library** with
its own Metal-based GPUI renderer, proving the embedding pattern works.

**Offscreen rendering:** Alacritty's own renderer cannot target offscreen
textures (it's coupled to glutin/winit/OpenGL). But this is irrelevant — you
would use `alacritty_terminal` for terminal state and write your own Metal
renderer, exactly as Zed does.

**Rendering backend:** OpenGL ES 2.0 only (GLSL 3.3 primary, GLES 2.0 fallback).
No Metal, no plans for Metal. The rendering approach: crossfont rasterizes
glyphs to CPU bitmaps, uploads to OpenGL texture atlas, two draw calls per
frame (backgrounds + text). For ts4, you would skip this entirely and build a
Metal renderer that reads from `alacritty_terminal`'s cell grid.

**Terminal state separation:** Genuinely separate. `Term<T>` is parameterized
over `EventListener` (a simple trait with one method). `renderable_content()`
returns an iterator over visible cells with cursor info, designed for renderer
consumption. Each cell has: character, foreground/background color, flags
(bold, italic, underline, etc.). Can be driven entirely without a renderer.

**Text rendering:** `crossfont` crate (separate repo). CoreText + CoreGraphics
on macOS, FreeType + Fontconfig on Linux. CPU-side glyph rasterization only.
**No ligatures** — the maintainers have stated it "most likely will never"
support them. HarfBuzz integration exists only as an unmerged experimental
feature.

**Language:** Rust. XPC/IOSurface integration proven in ts3/ts4 prototypes via
FFI.

**Input injection:** Via event loop channel: `sender.send(Msg::Input(...))` for
PTY input, `Msg::Resize(...)` for resize. Also via `EventListener` trait for
terminal-initiated writes.

**PTY:** Inside `alacritty_terminal` (not a separate crate). VTE parser (`vte`
crate, also Alacritty-maintained) accepts raw bytes independently.

**Features:** Basic VT102 emulation. 256-color and 24-bit true color, Unicode
(basic), scrollback, selection, URL detection, OSC 52 clipboard, bracketed
paste, mouse reporting. **Missing:** Sixel, all image protocols, ligatures,
complex text shaping (Arabic, Devanagari), right-to-left text, shell
integration.

**License:** Apache 2.0.

**Build:** Cargo. `alacritty_terminal` alone has minimal dependencies (vte,
bitflags, log, unicode-width, libc). No C compilation needed on macOS. Fast
build.

### Kitty (C/Python)

**Library extractability:** Not designed as a library. The C code is compiled as
Python C extensions (`.so` modules loaded by CPython), not a standalone C
library. No header files, no public C API, no embedding interface. The Python
layer (`boss.py`, `window.py`, `main.py`) drives everything — initialization,
configuration, lifecycle, event dispatch. The C core cannot be used without the
Python runtime.

**Offscreen rendering:** No offscreen capability. Requires a GLFW window with an
OpenGL 3.3 context. No FBO-based offscreen path. `--start-as=hidden` still
creates an OpenGL context internally.

**Rendering backend:** OpenGL 3.3 only via a custom GLFW fork. No Metal, no
plans for Metal. Apple deprecated OpenGL on macOS. The rendering architecture
(glyph atlas in 3D texture array, instanced cell rendering) is conceptually
clean but welded to OpenGL.

**Terminal state separation:** Separate in data structures — `Screen` holds cell
data, parser is in `parser.c`, CPU cells and GPU cells are parallel arrays. But
the execution flow is intertwined with Python: the `Boss` class orchestrates
everything, and C functions use `PyObject*` interfaces. Extracting parser +
screen requires rewriting all CPython interfaces as plain C APIs.

**Text rendering:** HarfBuzz for shaping (supports ligatures), CoreText on macOS
for font discovery and rasterization, FreeType + Fontconfig on Linux. Glyph
atlas via 3D OpenGL texture array. SIMD-accelerated parsing (AVX2/SSE/NEON).
Good pipeline, but outputs to OpenGL textures.

**Language:** C + Python + Go. The C core cannot run without Python. This is a
fundamental barrier — you would need to rewrite the orchestration layer.

**Input injection:** Via remote control protocol (`kitten @ send-text`,
`kitten @ send-key`). Requires the full app running. At the C level,
`write_to_child` exists but is buried inside the Python-driven architecture.

**PTY:** Custom `ChildMonitor` with dedicated I/O thread using `poll()`. Well
designed, but coupled to the Python lifecycle.

**Features:** Rich. Kitty graphics protocol (originator), Kitty keyboard protocol
(originator), ligatures, Unicode with grapheme clusters, styled underlines,
shell integration, remote control, SSH kitten, file transfer, OSC 8. No Sixel
(Kitty graphics is the intended replacement).

**License:** GPLv3. Embedding any Kitty code forces the entire application to
GPLv3. Hard blocker.

**Build:** Custom Python-based build system (`setup.py` + `dev.sh`). Requires
C compiler, Go >= 1.24, Python 3, HarfBuzz, zlib, libpng, OpenSSL. Heavyweight.

### WezTerm (Rust)

**Library extractability:** Large Cargo workspace (~66 directories, 29 workspace
members). Key crates: **`wezterm-term`** (terminal state, independent of GUI),
**`termwiz`** (escape sequences, surfaces, published on crates.io),
**`portable-pty`** (PTY, published on crates.io), **`wezterm-font`** (font
loading/shaping/rasterization). `wezterm-term` is explicitly designed for
headless use — its docs say "this crate does not provide any kind of gui."
**Tattoy** (text-based compositor) embeds a fork of `wezterm-term` headlessly,
proving extraction works. However, `wezterm-font` and `mux` depend on `config`,
which depends on `mlua` (embedded Lua 5.4) — a heavyweight dependency chain.

**Offscreen rendering:** The renderer in `wezterm-gui` is coupled to a window
surface via wgpu. But wgpu supports headless rendering (request adapter without
compatible surface, render to offscreen texture). The rendering logic
(`paint_impl()`, `paint_pass()`) operates on abstract quad allocators — the
surface binding happens only in `call_draw_webgpu()`. IOSurface integration
path: create IOSurface → create `MTLTexture` from it → import into wgpu via
`create_texture_from_hal()` → render → send Mach port.

**Rendering backend:** Dual backend — wgpu (Metal on macOS) and Glium (OpenGL).
wgpu is the primary path. Accessing the underlying Metal texture requires
`wgpu::hal` unsafe APIs. This works but adds a layer between you and the
IOSurface.

**Terminal state separation:** `wezterm-term::Terminal` is genuinely independent.
Created with a `TerminalSize`, a `TerminalConfiguration` trait impl, and a
`Box<dyn Write>` writer. Feed PTY bytes via `advance_bytes()`. Query state via
`cursor_pos()`, `screen()`. Input via `key_down()`, `mouse_event()`,
`send_paste()`. No GUI dependency.

**Text rendering:** HarfBuzz (vendored) for shaping, FreeType (vendored) for
rasterization, CoreText for font discovery on macOS. Full ligature support.
Glyph cache in `wezterm-gui` uploads to wgpu texture atlas. The font crate
produces CPU-side `RasterizedGlyph` bitmaps — GPU-independent. But
`wezterm-font` depends on `config` → `mlua`.

**Language:** Rust. XPC/IOSurface/Mach ports proven in ts3/ts4 prototypes.

**Input injection:** Direct API: `Terminal::key_down(key, mods)`,
`Terminal::mouse_event(event)`, `Terminal::send_paste(text)`. Encodes to
appropriate escape sequences respecting terminal state (application cursor
mode, mouse encoding mode, Kitty keyboard mode, bracketed paste).

**PTY:** `portable-pty` is a fully independent crate on crates.io. Provides
`PtySystem::openpty()`, `MasterPty`, `SlavePty`. No dependency on any other
WezTerm crate.

**Features:** The most comprehensive feature set. Sixel, iTerm2 image protocol,
Kitty image protocol, Kitty keyboard protocol, ligatures (via HarfBuzz), shell
integration (OSC 133), tmux control mode, SSH domains, serial ports, Unicode
with BiDi support, multiplexing (panes/tabs/windows/domains).

**License:** MIT.

**Build:** Cargo workspace. Vendored C/C++ libraries (FreeType, HarfBuzz, Cairo,
Lua). Clean build 5–15 minutes. `wezterm-term` alone builds faster with fewer
dependencies.

## Evaluation Matrix

Score each criterion 1–5 (5 = ideal fit for ts4 architecture).

| Criterion                 | Weight   | Ghostty | iTerm2 | Alacritty | Kitty | WezTerm |
| ------------------------- | -------- | ------- | ------ | --------- | ----- | ------- |
| Library extractability    | Critical | 5       | 1      | 4         | 1     | 3       |
| Offscreen rendering       | Critical | 5       | 2      | 3         | 1     | 3       |
| Rendering backend         | High     | 5       | 4      | 2         | 1     | 3       |
| Terminal state separation | High     | 5       | 2      | 5         | 2     | 5       |
| Text rendering pipeline   | High     | 5       | 4      | 2         | 4     | 5       |
| Language compatibility    | Medium   | 4       | 5      | 4         | 1     | 4       |
| Input injection           | Medium   | 5       | 3      | 4         | 3     | 5       |
| PTY management            | Low      | 4       | 3      | 4         | 4     | 5       |
| Feature coverage          | Low      | 3       | 5      | 2         | 4     | 5       |
| License                   | Medium   | 5       | 1      | 5         | 1     | 5       |
| Build complexity          | Low      | 3       | 2      | 5         | 2     | 3       |

## Analysis

### Eliminated: iTerm2 and Kitty

**iTerm2** is eliminated by its GPLv2 license and monolithic architecture. Even
if the license were permissive, the extraction effort would be comparable to
writing a terminal emulator from scratch. The sophisticated Metal renderer is
impressive but inseparable from the app.

**Kitty** is eliminated by its GPLv3 license and Python runtime dependency. The
C core cannot be used without Python. The OpenGL-only renderer would need a
complete rewrite for Metal.

### Remaining: Ghostty, Alacritty, WezTerm

All three have permissive licenses (MIT or Apache 2.0) and separable terminal
state. The choice comes down to how much of the rendering pipeline we get for
free versus how much we must build ourselves.

**Ghostty** is the strongest fit. It scores 5 on both critical criteria. Its
Metal renderer already renders to IOSurface-backed textures — the exact
mechanism ts4 needs. The modification to redirect from CALayer presentation to
XPC Mach port export is small: skip view attachment in `Metal.init()`, replace
`present()` with `IOSurfaceCreateMachPort()` + XPC send, and provide dimensions
from configuration instead of querying layer bounds. The C-ABI means the
terminal process would be written in Zig (Ghostty's language), which has native
C interop for XPC and IOSurface calls.

**WezTerm** is the second strongest. Its terminal state crate is mature and
proven for headless use (Tattoy embeds it). Feature coverage is the most
comprehensive of all five candidates. The challenge is rendering: no offscreen
renderer exists today, and the wgpu → Metal HAL → IOSurface integration path is
possible but non-trivial. The font crate's dependency on `config` → `mlua` adds
weight. TermSurf already has deep WezTerm experience from ts3, which reduces
risk.

**Alacritty** offers the cleanest library boundary (`alacritty_terminal` is
proven by Zed) but the least out of the box. No renderer, no ligatures, no
image protocols, only VT102-level emulation. You get a solid terminal state
machine and must build everything else — Metal renderer, glyph atlas, text
shaping pipeline. The features missing from Alacritty (ligatures, image
protocols, shell integration) are exactly the features that differentiate a
modern terminal from a basic one.

### Ghostty vs WezTerm: the real decision

| Factor                    | Ghostty                          | WezTerm                          |
| ------------------------- | -------------------------------- | -------------------------------- |
| Renderer reuse            | Metal + IOSurface — nearly free   | Must build offscreen renderer    |
| Terminal state maturity   | Mature, full featured            | Mature, most features            |
| Font pipeline             | CoreText/HarfBuzz, self-contained | HarfBuzz/FreeType, needs `config` |
| Feature coverage          | Good (no Sixel)                  | Best (Sixel + all image protocols) |
| Language                  | Zig (new for the project)        | Rust (proven in ts3/ts4)         |
| Existing codebase knowledge | ts1 is a Ghostty fork          | ts3 is a WezTerm fork            |
| IOSurface integration     | Already done internally          | Possible via wgpu HAL            |
| Modification surface      | ~3 functions in Metal.zig        | New renderer + HAL interop       |

Ghostty's Metal renderer already does what ts4 needs — render terminal text to
an IOSurface. The delta between what Ghostty does today and what ts4 requires
is small: redirect the IOSurface from a CALayer to an XPC Mach port. With
WezTerm, the delta is larger: build an offscreen renderer, integrate wgpu HAL
with IOSurface, and manage the font crate's dependency chain.

WezTerm's advantage is feature coverage (Sixel, iTerm2 images) and the Rust
ecosystem familiarity. But Ghostty's architectural alignment with ts4's
requirements is hard to beat — it's the only candidate whose renderer already
outputs IOSurface-backed Metal textures.

## Recommendation

**Start with Ghostty.** The terminal process would be written in Zig, using
libghostty with a modified Metal renderer that exports IOSurface Mach ports via
XPC instead of presenting to a CALayer. TermSurf already has a Ghostty fork
(ts1), so the codebase is familiar.

The modification path:

1. Use libghostty as a static library (`libghostty-fat.a`)
2. Modify `Metal.init()` to skip view/layer attachment (headless mode)
3. Modify `present()` to call `IOSurfaceCreateMachPort()` and send via XPC
4. Provide terminal dimensions from XPC resize messages instead of layer bounds
5. Use the existing C API for input injection (`ghostty_surface_key()`, etc.)
6. Use the existing PTY infrastructure or feed raw bytes to the parser

WezTerm remains a strong fallback. If Ghostty's renderer proves harder to
modify than expected, `wezterm-term` + a custom Metal renderer is a viable
alternative — more work, but in a more familiar language with broader feature
coverage.
