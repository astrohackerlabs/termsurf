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

The IOSurface must be created and rendered to via Metal (or wgpu targeting
Metal). The closer the renderer is to raw Metal, the simpler the IOSurface
integration.

Questions to answer:

- What GPU API does the renderer use? (Metal, wgpu, OpenGL, custom)
- If OpenGL, is there a Metal migration path or would it need rewriting?
- If wgpu, how hard is it to access the underlying Metal texture for IOSurface
  backing?
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

| Language        | XPC / IOSurface integration                             |
| --------------- | ------------------------------------------------------- |
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

## Evaluation Matrix

After researching each candidate, fill in this matrix. Score each criterion 1-5
(5 = ideal fit for ts4 architecture).

| Criterion                 | Weight   | Ghostty | iTerm2 | Alacritty | Kitty | WezTerm |
| ------------------------- | -------- | ------- | ------ | --------- | ----- | ------- |
| Library extractability    | Critical |         |        |           |       |         |
| Offscreen rendering       | Critical |         |        |           |       |         |
| Rendering backend         | High     |         |        |           |       |         |
| Terminal state separation | High     |         |        |           |       |         |
| Text rendering pipeline   | High     |         |        |           |       |         |
| Language compatibility    | Medium   |         |        |           |       |         |
| Input injection           | Medium   |         |        |           |       |         |
| PTY management            | Low      |         |        |           |       |         |
| Feature coverage          | Low      |         |        |           |       |         |
| License                   | Medium   |         |        |           |       |         |
| Build complexity          | Low      |         |        |           |       |         |

## Next Steps

1. Research each candidate against the criteria above.
2. Fill in the evaluation matrix.
3. Write a recommendation with rationale.
