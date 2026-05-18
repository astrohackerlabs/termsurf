+++
status = "closed"
opened = "2026-02-14"
closed = "2026-02-14"
+++

# Issue 417: Ghostty vs WezTerm

**Goal:** Choose between Ghostty and WezTerm as the terminal emulator for
TermSurf 4.0.

**Context:** TermSurf is a cross-platform terminal browser — an advanced
terminal emulator multiplexer that also has a `web` command for opening a
Chromium-based browser inside terminal panes. Users can run multiple browser
profiles side by side in the same window. The browser runs out-of-process and
communicates via IPC (XPC on macOS), sending IOSurface frames to the terminal
for compositing.

We must pick exactly one terminal to fork. The browser side is decoupled via IPC
and is not locked to any specific terminal. But the terminal fork is deep — we
need custom pane types, rendering pipeline integration, and long-term
maintainability.

**Prior experience:**

- ts1: Ghostty fork + WKWebView (macOS only, abandoned due to WKWebView limits)
- ts2: WezTerm fork + in-process CEF (abandoned due to one-profile-per-process)
- ts3: WezTerm fork + out-of-process CEF via XPC (superseded by ts4 due to CEF
  fps cap)
- ts4: Active development. Terminal TBD + in-process Chromium via Content API.

**Prior research:**

- Issue 402: WezTerm vs Alacritty comparison (WezTerm selected over Alacritty)
- Issue 403: Multi-process IOSurface compositing PoC (validated 60fps
  cross-process compositing with Swift + Rust + C++)
- Issue 404: Terminal emulator evaluation (Ghostty selected for ts4 due to
  native Metal IOSurface rendering)
- Issue 405: Architecture comparison (Option B selected: Ghostty fork with
  browser out-of-process, inheriting Ghostty's entire app infrastructure)
- Issue 416: Rust IOSurface receiver (proved C++, Swift, and Rust all achieve
  identical 60fps compositing; language is not the bottleneck)

## Research

### 1. Dealbreakers

#### 1.1 Windows Support

**Ghostty:** macOS + Linux only. Windows support is tracked in Discussion #2563,
estimated for Ghostty 1.4 or 1.5. Current progress: shared dependencies compile
(FreeType, HarfBuzz, GLFW, libpng, zlib, utf8proc, pixman), `zig build test`
works, and CI is established. Remaining work includes DirectWrite font
integration, DirectX graphics backend, native Windows UI framework, release
infrastructure, and shell integrations. The approach is incremental: GLFW/OpenGL
first, then platform-native APIs.

**WezTerm:** macOS + Linux + Windows + FreeBSD + NetBSD. Windows 10+ is a
first-class platform, already shipping.

**Assessment:** TermSurf targets web developers. Many web developers use macOS
or Linux, but a significant number use Windows (especially for corporate
environments). Ghostty's Windows support is actively being worked on but is
months away at best, with no release date.

#### 1.2 Custom Pane Feasibility

**Ghostty (ts1 experience):** Custom panes are straightforward. ts1 added a
WebView pane by:

1. Adding `web.zig` (1,291 LOC) as a new CLI command in `src/cli/`
2. Adding `web` as a new `Action` enum variant in `src/cli/ghostty.zig` (1 line)
3. Creating `termsurf-macos/Sources/Features/WebView/` (4 files, 2,058 LOC) and
   `Features/Socket/` (5 files, 864 LOC) in the Swift macOS app

Total: ~3,150 LOC of new code + 1 line changed in Ghostty core. The WebView was
a native NSView added as a subview to the terminal pane's SurfaceView. No
modifications to Ghostty's pane management, rendering pipeline, terminal
emulation, keybinding system, or build system.

**WezTerm (ts2/ts3 experience):** The `Pane` trait is terminal-specific and
cannot be extended for webviews. It requires returning `Vec<Line>` (terminal
character data), cursor positions as cell coordinates, and terminal color
palettes. ts3 worked around this by treating webviews as an **overlay system**
that bypasses the Pane trait entirely:

1. Added `webview_socket.rs` (882 LOC) and `webview_xpc.rs` (719 LOC) in
   `wezterm-gui/src/termwindow/`
2. Modified `render/pane.rs` (~100 LOC) to check `has_webview_overlay(pane_id)`
   before rendering
3. Added `paint_webview_control_bars()` call in `render/paint.rs`
4. Created 4 new crates: termsurf-launcher (338 LOC), termsurf-profile (~1,500
   LOC), termsurf-xpc (~600 LOC), termsurf-web (~300 LOC)

Total: ~5,000 LOC of new code + ~100 LOC modified in WezTerm core rendering. The
overlay approach works but is a workaround — webview panes remain terminal
LocalPane instances underneath, with overlay state stored separately.

**Assessment:** Both are feasible, but with different integration patterns.
Ghostty's approach is cleaner — the Swift macOS app has native NSView
composition, so a webview is just another view. WezTerm's Pane trait is
terminal-only, forcing an overlay workaround that bypasses the pane abstraction.

### 2. Integration Depth

#### 2.1 Language Alignment

**Ghostty:** Zig (80.8%) + Swift (11%) + C++/C (5.7%). The core engine
(libghostty) is Zig with a C ABI. The macOS GUI is Swift (AppKit + SwiftUI). The
Linux GUI is Zig calling the GTK4 C API.

All TermSurf browser integration code is Rust: termsurf-xpc, two-profiles-rust,
termsurf-launcher, termsurf-profile. Integrating Rust with Ghostty means:

- Calling libghostty's C ABI from Rust (well-established pattern)
- Or modifying Zig code directly (learning curve for Rust developers)
- The macOS app is Swift, so browser pane UI code must be Swift
- ts1 demonstrated this works: Zig core + Swift UI + socket IPC to web commands

**WezTerm:** Rust (98.9%). Same language as all TermSurf browser code. No FFI
boundary. Browser integration code can directly implement WezTerm traits, access
internal state, and share crates.

**Assessment:** WezTerm has a clear advantage in language alignment. With
Ghostty, the browser integration must cross a Zig/Swift ↔ Rust boundary, adding
complexity. With WezTerm, everything is Rust.

#### 2.2 Rendering Pipeline Compatibility

**Ghostty:** Metal on macOS, OpenGL (4.3+) on Linux. The Metal renderer runs on
a dedicated thread. In v1.2.0, the renderer architecture was restructured to
share core logic between OpenGL and Metal backends.

Ghostty already renders terminal text to IOSurface-backed Metal textures
internally. Issue 404 identified this as a decisive advantage: "the delta
between what Ghostty does today and what ts4 requires is small: redirect the
IOSurface from a CALayer to an XPC Mach port." This means browser IOSurface
compositing aligns with Ghostty's existing rendering model on macOS. On Linux, a
different compositing path would be needed.

**WezTerm:** wgpu (cross-platform abstraction over Metal/Vulkan/DX12/OpenGL).
Default is OpenGL; WebGpu mode was briefly default in Jan 2024 but reverted. The
Rust IOSurface receiver (Issue 416) already uses wgpu for compositing, meaning
the same abstraction layer is shared.

ts3 added a separate webview render pipeline alongside the terminal pipeline:

- Terminal pass: glyph atlas textures → quad rendering
- Webview pass: IOSurface textures composited on top via dedicated
  `webview_render_pipeline`
- Two completely separate render passes, no modification to terminal glyph
  pipeline

**Assessment:** Ghostty's Metal renderer is closer to what we need on macOS
(IOSurface-native). WezTerm's wgpu is cross-platform and matches our existing
Rust compositing code. Both work, but Ghostty has a macOS-specific advantage
while WezTerm has a cross-platform advantage.

#### 2.3 Upstream Merge Difficulty

**Ghostty:**

- 14,334 commits, 484 contributors
- 6-month major/minor release cycle (adopted in v1.2.0)
- Active development with renderer restructuring (v1.2.0 rewrote renderer
  architecture)
- ts1 modifications were extremely isolated (1 line changed in core + 1 new
  file), suggesting merges would be easy
- Risk: Zig is pre-1.0 and still evolving; Ghostty must track Zig compiler
  changes, and so would our fork

**WezTerm:**

- 8,564 commits, 389 contributors
- Last stable release was February 2024 (2+ years ago)
- 19+ interconnected crates with feature flag cascading
- ts3 modifications were mostly isolated (new files + ~100 LOC in
  render/pane.rs) but touched the rendering pipeline
- Risk: a community-contributed Wayland rewrite is in progress, which may cause
  significant churn in window/rendering code

**Assessment:** Ghostty's more modular architecture and our proven ability to
keep modifications isolated (ts1) suggest easier upstream merges. WezTerm's
interconnected crate structure and the pending Wayland rewrite add merge risk.
However, WezTerm's slower development pace (fewer breaking changes) somewhat
offsets this.

#### 2.4 Prior Integration Experience

**ts1 (Ghostty):**

- Easy: Adding CLI command (just a new Zig file). Creating Swift UI for webview
  (native NSView composition). Socket-based IPC between CLI and app.
- Hard: Nothing significant — modifications were surgically isolated.
- Depth: 1 line changed in Ghostty core. All TermSurf code in separate
  directories.
- Architecture: CLI command → Unix socket → Swift app → NSView webview. Clean
  separation.

**ts2 (WezTerm, in-process CEF):**

- More invasive: `cef_integration.rs` (173 LOC) for CFRunLoop integration,
  `cef_browser/mod.rs` (~1,000 LOC) for browser management, custom shader for
  compositing
- Had to integrate CEF's message loop with WezTerm's event handling
- Total: ~3,000 LOC, more scattered modifications

**ts3 (WezTerm, out-of-process CEF):**

- Cleaner than ts2: webview overlay system bypasses pane internals
- Two new files in wezterm-gui (~1,600 LOC), plus 4 new crates (~2,800 LOC)
- Modified render/pane.rs for overlay detection (~100 LOC)
- Hard: IOSurface Mach port transfer required FFI bindings to native macOS APIs.
  Dynamic scaling (CEF logical pixels vs physical pixels vs terminal cells).
  Process coordination (launcher + profile servers).
- Architecture: CEF process → XPC → Mach port → IOSurface → wgpu texture

**Assessment:** Ghostty was significantly easier to integrate with. ts1's
modifications were trivially isolated. ts3's WezTerm modifications were more
complex, partly due to the overlay workaround and partly due to the more complex
IPC architecture (though some of that complexity came from CEF, not WezTerm).

### 3. Practical Factors

#### 3.1 Codebase Size & Complexity

| Metric             | Ghostty                          | WezTerm                       |
| ------------------ | -------------------------------- | ----------------------------- |
| Total LOC (approx) | ~282,000 (867 files)             | ~150,000–250,000+ (estimated) |
| Primary language   | Zig (80.8%)                      | Rust (98.9%)                  |
| Crate/module count | Monolithic + platform dirs       | 19+ workspace crates          |
| Workspace deps     | Zig build system (build.zig.zon) | ~200 Cargo dependencies       |
| Config fields      | 200+                             | ~500                          |

Ghostty is a monolith with platform-specific directories (macos/, pkg/gtk/).
WezTerm is a multi-crate workspace with feature flag cascading across crate
boundaries.

#### 3.2 Community & Governance

| Metric                | Ghostty                                  | WezTerm             |
| --------------------- | ---------------------------------------- | ------------------- |
| Stars                 | ~43,700                                  | ~24,100             |
| Contributors          | 484                                      | 389                 |
| Total commits         | 14,334                                   | 8,564               |
| Open issues           | 138                                      | ~1,400              |
| Maintainer            | Mitchell Hashimoto (HashiCorp founder)   | Wez Furlong         |
| Governance            | Non-profit (Hack Club 501(c)(3))         | Personal project    |
| Paid contributors     | Yes ($60/hr contracts)                   | No                  |
| Subsystem maintainers | 8 appointed                              | None                |
| Bus factor            | Improving (paid + subsystem maintainers) | 1 (sole maintainer) |

Ghostty's governance is stronger. It transitioned to non-profit status in
December 2025 with public finances, paid contributor contracts, and 8 subsystem
maintainers. Mitchell Hashimoto donated $150,000 to Hack Club directly.

WezTerm is Wez Furlong's spare-time project. In December 2025 (Issue #7451), he
described moving countries, hospitalization, and insufficient sponsorship
income. He expressed interest in building a larger maintainer pool in 2026. The
repo moved from `wez/wezterm` to the `wezterm` organization, suggesting
preparation for shared maintainership, but no additional maintainers have been
announced.

#### 3.3 Build Complexity

**Ghostty:** Two-stage build. `zig build` produces `GhosttyKit.xcframework`,
then Xcode builds the macOS app linking that framework. Dependencies declared in
`build.zig.zon` and mirrored at `deps.files.ghostty.org`. Linux builds use GTK4.
Zig's pre-1.0 status means tracking compiler changes. Ghostty limits Linux
builds to 32 cores to work around a known Zig memory corruption bug.

**WezTerm:** Cargo workspace. `cargo build` builds everything. Native
dependencies vary by platform. Incremental compilation works well (Rust
ecosystem is mature). `split-debuginfo` optimization disabled on macOS due to
Windows compatibility. Two codegen crates excluded from workspace to prevent
build conflicts.

#### 3.4 Performance Baseline

**Ghostty:** Metal renderer on dedicated thread. No widely reported performance
issues. Native IOSurface rendering is zero-copy on macOS. Memory usage not
publicly benchmarked but expected to be lean (Zig is low-level).

**WezTerm:** Memory ~170 MB resident (vs ~80 MB for Alacritty), ~320 MB with
WebGpu backend. Performance lag reported on some Linux compositors (Wayland,
Hyprland). macOS font rendering described as "weirdly out of place." Resize
causes text to "fly around."

#### 3.5 Release Cadence

**Ghostty:**

| Version | Date                             |
| ------- | -------------------------------- |
| 1.0.0   | December 26, 2024                |
| 1.1.0   | January 30, 2025                 |
| 1.2.0   | September 15, 2025               |
| 1.2.3   | October 23, 2025 (latest stable) |

6-month major/minor cycle with patch releases as needed. Continuous "tip"
(nightly) builds on every commit.

**WezTerm:**

| Version  | Date                             |
| -------- | -------------------------------- |
| 20230712 | July 12, 2023                    |
| 20240128 | January 29, 2024                 |
| 20240203 | February 3, 2024 (latest stable) |

Irregular releases. Last stable release was February 2024 — over two years ago.
Nightly builds continue from main.

### 4. Nice-to-Haves

#### 4.1 Configuration & Extensibility

**Ghostty:** Key-value config file (~200 options). No scripting language. Lua
has been discussed (Discussion #4914) but rejected — the maintainer wants to
avoid the complexity of embedding a scripting runtime. Runtime config reloading
supported.

**WezTerm:** Lua 5.4 scripting (~500 config fields, ~50 API functions). Full
language: conditionals, loops, functions, modules. Event system with callbacks
(`gui-startup`, `update-status`, custom events). Hot reloading on file changes.
Can split config across multiple Lua files. Some users find Lua too simplistic
and have requested WASM plugin support.

#### 4.2 Image/Graphics Protocol Support

| Protocol       | Ghostty                 | WezTerm                     |
| -------------- | ----------------------- | --------------------------- |
| Kitty Graphics | Supported               | Supported (default on)      |
| iTerm2 Images  | Not confirmed           | Supported (includes imgcat) |
| Sixel          | Rejected (will not add) | Experimental                |

Ghostty's maintainer rejected Sixel due to underspecification and poor reference
implementations. WezTerm supports all three major protocols.

#### 4.3 Multiplexer Features

**Ghostty:** Built-in tabs, splits (horizontal/vertical), multiple windows.
Native UI components. Keyboard shortcuts for creating/navigating/resizing
splits. Tab overview with thumbnails (macOS). Quick Terminal (macOS). No remote
multiplexing.

**WezTerm:** Built-in multiplexer with three remote domain types:

| Domain | Transport       | Description                                      |
| ------ | --------------- | ------------------------------------------------ |
| Unix   | AF_UNIX sockets | Local or WSL; `wezterm-mux-server` runs headless |
| SSH    | SSH             | Connect to remote `wezterm-mux-server`           |
| TLS    | TLS over TCP    | SSH-bootstrapped TLS connection                  |

Supports session persistence (like tmux), multiple GUI clients connecting to the
same mux server, native mouse/clipboard/scrollback in remote sessions.

#### 4.4 License

Both are MIT licensed with no CLA requirements. Ghostty's maintainer has noted
awareness that MIT allows unrestricted forking. WezTerm bundles fonts under OFL
1.1.

## Summary Table

| Factor                           | Ghostty                                                | WezTerm                                         | Edge    |
| -------------------------------- | ------------------------------------------------------ | ----------------------------------------------- | ------- |
| **Windows support**              | Not yet (est. 1.4/1.5)                                 | Shipping                                        | WezTerm |
| **Custom pane feasibility**      | Native NSView composition; 1 line core change          | Overlay workaround; Pane trait is terminal-only | Ghostty |
| **Language alignment**           | Zig + Swift (Rust needs FFI)                           | Rust (same as all browser code)                 | WezTerm |
| **Rendering pipeline**           | Metal (IOSurface-native on macOS)                      | wgpu (cross-platform, matches our Rust code)    | Tie     |
| **Upstream merge difficulty**    | Modular; ts1 was trivially isolated                    | 19+ crates; Wayland rewrite in progress         | Ghostty |
| **Prior integration experience** | ts1: easy, 1 line core change                          | ts2/ts3: harder, overlay workaround             | Ghostty |
| **Codebase complexity**          | Monolith, fewer moving parts                           | Multi-crate workspace, 200 deps                 | Ghostty |
| **Community & governance**       | Non-profit, 8 subsystem maintainers, paid contributors | Sole maintainer, spare-time project             | Ghostty |
| **Build complexity**             | Zig (pre-1.0) + Xcode                                  | Cargo (mature ecosystem)                        | WezTerm |
| **Performance**                  | Metal, lean                                            | Higher memory, reported macOS/Wayland issues    | Ghostty |
| **Release cadence**              | 6-month cycle, regular patches                         | 2+ years since last stable                      | Ghostty |
| **Configuration**                | Key-value, no scripting                                | Lua scripting, 500 options                      | WezTerm |
| **Graphics protocols**           | Kitty only                                             | Kitty + iTerm2 + Sixel                          | WezTerm |
| **Multiplexer**                  | Local only (tabs/splits)                               | Local + remote (SSH, TLS, Unix)                 | WezTerm |
| **License**                      | MIT                                                    | MIT                                             | Tie     |

## Conclusion

**Decision: Ghostty.**

Ghostty wins on the factors that matter most for TermSurf — custom pane
integration, upstream merge difficulty, community health, and prior integration
experience. The scorecard shows 8 factors favoring Ghostty, 5 favoring WezTerm,
and 2 ties, but the raw count understates the gap: Ghostty's advantages are in
the high-impact categories while WezTerm's are in secondary ones.

### Why Ghostty

**Custom pane integration is the deciding factor.** TermSurf's core feature is
rendering a browser inside a terminal pane. ts1 proved this requires exactly 1
line changed in Ghostty's core — everything else lives in isolated new files.
WezTerm's Pane trait is terminal-only (returns `Vec<Line>`, cursor cell
positions, terminal palettes), forcing an overlay workaround that bypasses the
pane abstraction entirely. This isn't a minor inconvenience; it's a fundamental
architectural mismatch. Every future feature (input routing, resize sync, focus
management) must work around the fact that webview panes aren't really panes.

**Upstream mergeability compounds over time.** ts1's modifications were
trivially isolated — a monolithic Zig core with clean platform boundaries means
our changes don't collide with upstream. WezTerm's 19+ interconnected crates
with feature flag cascading, plus a community-contributed Wayland rewrite in
progress, means more merge conflicts on every upstream sync. Over the life of
the project, this difference adds up.

**Community health is a long-term bet.** Ghostty transitioned to non-profit
status (Hack Club 501(c)(3)) with public finances, $60/hr paid contributor
contracts, and 8 subsystem maintainers. WezTerm is a spare-time project whose
sole maintainer described moving countries, hospitalization, and insufficient
sponsorship income in December 2025. The last stable WezTerm release was
February 2024 — over two years ago. Forking WezTerm is a bet that either the
upstream recovers or we're prepared to maintain 150,000+ lines of Rust
ourselves. Ghostty's trajectory is the opposite.

### Why not WezTerm's advantages

**Windows support** is WezTerm's strongest card, but it's not a dealbreaker.
Ghostty's Windows support is actively in progress (Discussion #2563) with shared
dependencies compiling, tests passing, and CI established. The remaining work
(DirectWrite, DirectX, native UI) is estimated for Ghostty 1.4 or 1.5. TermSurf
is not shipping tomorrow — the Chromium integration alone (Issue 407) has months
of work ahead. By the time TermSurf is ready for users, Ghostty will likely have
Windows support. And if it doesn't, we can still ship macOS + Linux first (where
most web developers work) and add Windows later.

**Language alignment** sounds important but is overstated. The browser runs
out-of-process via IPC. The language boundary is at the protocol level (XPC
dictionaries, Mach ports), not at function calls. ts1 proved this works: Zig
core + Swift UI + Unix socket IPC to Rust-based web commands. Issue 416 proved
that C++, Swift, and Rust all achieve identical 60fps IOSurface compositing —
the language doesn't matter for the hot path.

**Lua scripting, remote multiplexing, and graphics protocols** are nice-to-haves
that don't affect TermSurf's core value proposition. TermSurf's differentiator
is the browser-in-terminal experience, not configuration extensibility or remote
session management. These features can be added later if needed — and Ghostty's
200+ config options and built-in tabs/splits/windows cover the baseline.

### Risks to monitor

1. **Ghostty Windows timeline.** If Ghostty's Windows support slips past 1.5, we
   may need to contribute to it directly or accept a macOS+Linux-only launch.
2. **Zig compiler stability.** Zig is pre-1.0. Ghostty must track compiler
   changes, and so must our fork. This adds build maintenance overhead that
   Cargo/Rust doesn't have.
3. **libghostty API stability.** The C API is public alpha and "not yet released
   as a standalone library." Breaking changes are possible. Our fork should
   track Ghostty's internal APIs rather than relying on the public C ABI.

### What this means for ts4

ts4 returns to ts1's approach: fork Ghostty as the application. The critical fix
is replacing WKWebView (which was too limited) with Chromium embedded via the
Content API (not CEF, which cannot sustain 60fps). This was already the plan
from Issue 405 (Option B: Ghostty fork with browser out-of-process). This
research confirms that decision with concrete data from both codebases.
