# TermSurf

TermSurf is a protocol for embedding web browsers inside terminal emulators. Any
terminal, any browser engine, any TUI — connected by a protobuf/Unix socket
protocol. Users type `web localhost:3000` and see their work without ever
leaving the terminal. No alt+tab, no context switch.

[Agent development guide](https://agents.md/).

## Rules

Do exactly what your user says. No more, no less. NEVER assume they want
something they didn't ask for. NEVER change code unless explicitly asked.

## Vision

TermSurf is a protocol, not just an app. It is a network of interchangeable
components — terminals, browser engines, and TUIs — all speaking the same
protobuf/Unix socket protocol (`termsurf.proto`).

### Cross-platform

TermSurf will work on macOS, Linux, and Windows. iOS and Android can be added
later.

### Every major browser engine

Each browser engine runs as a separate "profile server" process, communicating
with the terminal (board) via the TermSurf protocol. One process per profile.

| Engine   | C library              | Rust binary | Status     |
| -------- | ---------------------- | ----------- | ---------- |
| Chromium | `libtermsurf_chromium` | Roamium     | Done       |
| WebKit   | `libtermsurf_webkit`   | Surfari     | Planned    |
| Gecko    | `libtermsurf_gecko`    | Waterwolf   | Researched |
| Ladybird | `libtermsurf_ladybird` | Girlbat     | Researched |

Each engine follows the same pattern: a C shared library wrapping the engine's
embedding API (`ts_*` functions), linked by a Rust binary that handles Unix
socket IPC, protobuf parsing, and process lifecycle. The Rust binary (~400
lines) is almost entirely reusable across engines.

### Multiple terminals (boards)

Although TermSurf currently ships as a Ghostty fork (`gui/`), we will implement
forks of all major terminal emulators:

- **Ghostty** (gui/) — Current board. Active development.
- **WezTerm** (Wezboard) — Researched in Issue 709. Strong architectural match.
- **Kitty** — Planned.
- **Alacritty** — Planned.
- **iTerm2** — Planned.

Any terminal that implements the TermSurf protocol can host browser overlays. A
"board" is a terminal emulator that listens on a Unix socket, accepts
connections from TUIs and browser engines, and renders browser content as
overlays at pixel coordinates.

### Many TUIs

The first TUI, `web`, provides browser chrome (URL bar, navigation, modes) in
the terminal pane. But TermSurf is really a webview overlay protocol — many TUIs
can embed web browsers with any engine:

- `web` — General-purpose web browser TUI (current)
- Future TUIs could include: documentation viewers, API explorers, email
  clients, dashboard monitors, or any application that benefits from rendering
  web content inside a terminal.

### The protocol is the product

The TermSurf protocol (`termsurf.proto`) is the most important artifact. It
defines 30 message types covering tab lifecycle, navigation, input forwarding,
GPU compositing, state synchronization, and request/reply pairs. The protocol
will be extended to support:

- All common web browser features (bookmarks, history, downloads, etc.)
- Terminal-specific features (keyboard-based navigation, shrink/grow overlay,
  split management)
- New message types as needs arise

Care goes into the protocol first. Individual apps (boards, engines, TUIs) are
implementations of the protocol.

## Architectural Decisions

### Multi-process architecture

TermSurf is multi-process by necessity, not by choice. Each browser engine
process serves exactly one profile (one set of cookies, storage, and cache).
This is a hard constraint imposed by Chromium (one `BrowserContext` per
process), and Gecko and Ladybird have the same limitation. This constraint is
the defining architectural fact of TermSurf — it shaped every generation from
ts2 onward.

The multi-process design has a second benefit: it enables multi-engine support.
Because each browser process is an independent program speaking the TermSurf
protocol, the board doesn't care which engine is behind it. A user can have one
pane running Roamium (Chromium), another running Surfari (WebKit), and a third
running Girlbat (Ladybird) — all in the same terminal window, all speaking the
same protobuf messages.

WebKit is the exception — `WKWebsiteDataStore` supports multiple profiles in one
process — but the one-process-per-profile model still works for it, and keeping
the architecture uniform is more valuable than optimizing for one engine.

**Process topology:**

```
┌─────────┐  ┌─────────┐  ┌─────────┐
│  TUI 1  │  │  TUI 2  │  │  TUI N  │    N TUIs (e.g., `web`)
└────┬────┘  └────┬────┘  └────┬────┘
     │            │            │
     └────────────┼────────────┘
                  │  Unix socket
           ┌──────┴──────┐
           │    Board    │                1 board (terminal emulator)
           │  (Ghostty)  │
           └──┬───┬───┬──┘
              │   │   │
              │   │   │  Unix sockets
              │   │   │
     ┌────────┘   │   └──────┐
     │            │          │
┌────┴────┐ ┌─────┴───┐ ┌────┴────┐
│ Roamium │ │ Surfari │ │ Roamium │    M engines (one per profile)
│ profile │ │ profile │ │ profile │
│   "A"   │ │   "B"   │ │   "C"   │
└─────────┘ └─────────┘ └─────────┘
```

N TUIs connect to 1 board. The board manages M browser engine processes, each
serving one profile. Different profiles can use different engines. The board is
the hub — it routes messages between TUIs and engines, manages pane layout, and
composites browser overlays into the terminal window.

### Unix sockets + protobuf for all IPC

All inter-process communication uses Unix domain sockets with length-prefixed
protobuf messages. The board (terminal) listens on a PID-scoped socket
(`$TMPDIR/termsurf/gui-{pid}.sock`), and both TUIs and browser engines connect
to it as clients.

- **TUI → Board:** The TUI reads the `TERMSURF_SOCKET` env var (set by the
  board) to discover the socket path.
- **Board → Engine:** The board passes `--ipc-socket={path}` when launching
  browser engine processes.
- **Wire format:** 4-byte little-endian length prefix + serialized protobuf
  (`termsurf.proto`).
- **Serialization:** protobuf-c in Zig (board), prost in Rust (TUI and engines),
  C++ protobuf in Chromium.

Earlier generations (ts3–ts5) used XPC for IPC. Issues 698–701 replaced XPC with
sockets, and Issue 702 removed all dead XPC code. CALayerHost compositing
(zero-copy GPU rendering) does not require XPC — Window Server routes
`CAContext` layer IDs between processes natively.

### Every Chromium issue gets its own branch

When modifying the Chromium fork (`chromium/src/`), ALWAYS create a new branch
for the current issue. Never commit directly to an existing issue's branch.

1. Find the most relevant recent branch (usually the one with the latest
   TermSurf modifications).
2. Create a new branch from it: `{version}-issue-{N}` (e.g.,
   `146.0.7650.0-issue-625`).
3. Add the new branch to the Branches table in `chromium/README.md`.

This keeps every issue's Chromium changes isolated and traceable.

## Directory Structure

- `gui/` — The GUI (Ghostty fork, Zig-first). **Active development.**
- `webtui/` — The `web` TUI (Rust/ratatui). Browser chrome in the terminal pane.
- `chromium/` — Chromium fork build workspace (gitignored).
- `docs/issues/` — Documentation across all generations.
- `docs/early-prototypes.md` — Archived prototype documentation (ts1–ts5,
  cef-rs).

## GUI (gui/) — Active Development

### Architecture

The GUI forks Ghostty with all browser integration in Zig. Swift remains a thin
macOS wrapper — window creation, menu bar, application lifecycle — matching
Ghostty's own architecture. This is a clean break from ts5, where browser
integration lived in Swift (CompositorXPC.swift).

Key architectural decisions:

- **Socket IPC in Zig.** All IPC uses Unix domain sockets with length-prefixed
  protobuf. The GUI listens on `$TMPDIR/termsurf/gui-{pid}.sock`. TUI and
  Chromium connect as clients. The IPC module (`gui/src/apprt/xpc.zig`) handles
  accept, framing, dispatch, and per-connection lifecycle.
- **CALayerHost in Zig.** Browser panes render via `CALayerHost` — a CALayer
  subclass that displays a remote `CAContext` from Chromium's GPU process.
  Window Server composites directly from GPU VRAM. Zero per-frame IPC, zero
  texture copies. The Metal renderer sets up the CALayerHost layer tree in Zig.
- **Input routing in Zig.** Zig already receives all keyboard/mouse events
  through `Surface.keyCallback()` and `mouseButtonCallback()`. In browse mode,
  these route to Chromium via socket IPC instead of to the terminal.
- **Single source of truth.** Browse mode, focus state, pane profiles, overlay
  coordinates — all live in Zig's Surface struct.

### Current State

The GUI is a Ghostty fork with browser integration built in Zig. Current
additions: IPC gateway and connection management (Issues 601, 698–702), pink
texture proof-of-concept (Issue 602), live Chromium streaming at 60fps with
dynamic resize (Issue 603), multi-pane multi-profile server reuse (Issues
604–605), mouse input forwarding with cursor changes and text selection (Issue
606), keyboard input forwarding with Cmd+key bypass (Issues 607–609), branding
and app icon (Issues 611–612), directory rename from ghost/web to gui/tui (Issue
613), XDG directory compliance (Issue 615), loading progress indicator and
browser navigation keybindings (Issue 616), CALayerHost migration replacing
FrameSinkVideoCapturer with zero-copy Window Server compositing (Issues
624–632), reproducible rename script for upstream merges (Issue 656), purple
Edit mode border (Issue 657), vim-like editor modes and keybindings (Issue 658),
vim-style command mode (Issue 659), per-mode submode colors (Issue 660), tight
title spacing (Issue 661), clap CLI parser with subcommands (Issue 664),
context-sensitive Esc key navigation (Issue 665), Esc latency fix via unified
mpsc channel (Issue 666), active pane indicator with borders and desaturation
(Issues 667–669), click-to-focus without pass-through (Issue 670), app icon
update (Issue 671), inner border padding (Issue 672), script consolidation
(Issue 673), configurable homepage (Issue 674), hello message for live config
(Issue 675), URL normalization (Issue 676), website deps and linting (Issues
677–678), MIT license and trademark (Issue 679), dark mode with `:colorscheme`
command (Issue 680), `:quitall` and subsequence matching (Issue 681), Chrome
DevTools in split panes (Issues 684, 687, 690–691), multi-profile tracking fix
(Issue 685), tab lifecycle — close tabs when panes close (Issue 689), `web file`
subcommand (Issue 692), smart input resolution (Issue 693), replace pane_id with
tab_id in Chromium (Issue 694), activation drag suppression (Issue 695), double
click suppression fix (Issue 696), Unix socket + protobuf IPC replacing XPC
(Issues 698–702), click suppression removal (Issue 703), browser bindings C
library (Issues 704–706), Roamium Rust browser binary (Issue 707), clean
Chromium fork with renamed libtermsurf_chromium (Issue 708).

### Source Layout

- `gui/src/` — Shared Zig core (libtermsurf)
- `gui/src/Surface.zig` — Core surface (holds browser state)
- `gui/src/renderer/Metal.zig` — Metal renderer
- `gui/src/renderer/metal/` — Metal pipeline, shaders, IOSurface layer
- `gui/src/apprt/embedded.zig` — C API exports
- `gui/include/ghostty.h` — libghostty C API headers
- `gui/macos/` — macOS app (Swift, thin wrapper)
- `gui/build.zig` — Build system
- `gui/build.zig.zon` — Dependencies

### Build & Install

All build scripts live in `scripts/`. They handle GUI, Chromium, TUI, and
Roamium together.

| Script                                        | Purpose                                                                                                                    |
| --------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `scripts/build-debug.sh [--clean] [--open]`   | Build all components (GUI debug, Chromium, TUI, Roamium). `--clean` wipes build dirs first, `--open` launches.             |
| `scripts/build-release.sh [--clean] [--open]` | Same as above but ReleaseFast builds.                                                                                      |
| `scripts/install.sh`                          | Install release build to `/Applications/TermSurf.app`, bundle Chromium + TUI, codesign, symlink CLIs to `/usr/local/bin/`. |
| `scripts/build-roamium.sh [args]`             | Build Roamium only and copy binary to `chromium/src/out/Default/`. Pass `--release` for release build.                     |
| `scripts/clean-zig.sh`                        | Clean Zig build artifacts + Xcode DerivedData. Preserves Chromium cache.                                                   |
| `scripts/rename-ghostty.sh [dir]`             | Rename all Ghostty references to TermSurf in `gui/`. Re-runnable after upstream merges.                                    |
| `scripts/generate-icons.sh [image]`           | Generate app icon assets from a source image (default: `assets/termsurf-2-black-3.png`).                                   |
| `scripts/nerd-font-test.sh`                   | Print Nerd Font test glyphs for visual verification.                                                                       |

For GUI-only iteration, `cd gui && zig build` still works. The full build
scripts also auto-detect Chromium's `protoc` so you don't need a system install.

### Upstream Merges

Same approach as ts5 (Issue 418 Experiment 3):

```bash
git fetch upstream
git subtree pull --prefix=gui upstream main -m "Merge upstream Ghostty into gui"
```

## Documentation

All documentation is in `docs/` or in `README.md` files throughout the codebase.

### GUI (active)

Recent issues:

- `docs/issues/700-tui-gui-sockets.md` — Replace TUI↔GUI XPC with Unix sockets
- `docs/issues/701-chromium-sockets.md` — Replace GUI↔Chromium XPC with Unix
  sockets
- `docs/issues/702-socket-cleanup.md` — Dead XPC removal and unlimited client
  connections
- `docs/issues/703-remove-click-suppression.md` — Remove click-to-activate
  suppression
- `docs/issues/704-browser-bindings.md` — Browser bindings (libtermsurf_content)
- `docs/issues/705-browser-bindings.md` — Browser bindings continued (DevTools
  fix)
- `docs/issues/706-plusium-devtools.md` — Plusium DevTools crash fix
- `docs/issues/707-roamium.md` — Roamium (shared lib + Rust rewrite)
- `docs/issues/708-roamium-only.md` — Roamium-only (clean fork, renamed lib)
- `docs/issues/709-wezboard.md` — Wezboard (WezTerm fork research)
- `docs/issues/710-gecko-webkit-ladybird.md` — Gecko, WebKit & Ladybird engine
  research
- `docs/issues/711-rename-ghostboard-webtui.md` — Rename GUI to Ghostboard, TUI
  to webtui

### Early Prototypes (ts1–ts5)

Issue docs for all prototype generations are indexed in
[docs/early-prototypes.md](docs/early-prototypes.md).

### General

- `docs/issues/002-merge-upstream.md` — How to merge changes from upstream repos
- `docs/issues/001-competitors.md` — Terminal-browser hybrid comparison
- `docs/issues/003-website.md` — termsurf.com website
- `TODO.md` — Task checklist and future issues. Only one issue is active at a
  time (the highest-numbered issue doc without a `## Conclusion`). When a new
  problem is identified during work on the active issue, add it to the "Future
  issues" section of TODO.md instead of creating an issue doc.

## Remember

NEVER change code unless explicitly asked. NEVER make unrequested changes.
Always do EXACTLY what your user asks — no more, no less.
