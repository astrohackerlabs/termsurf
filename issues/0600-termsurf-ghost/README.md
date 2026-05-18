+++
status = "closed"
opened = "2026-02-19"
closed = "2026-02-19"
+++

# Issue 600: TermSurf Ghost

## Goal

Fork Ghostty into `ghost/` and verify it builds and runs. This is the foundation
for all subsequent work.

## Vision

TermSurf Ghost is the production application. The ts1–ts5 series were
experiments and proofs-of-concept that validated the architecture. Ghost takes
the lessons learned and builds them properly from the start.

The core principle: **Zig is the primary language.** Swift is a thin macOS
wrapper — window creation, menu bar, application lifecycle — nothing more. All
logic lives in Zig, matching Ghostty's own architecture.

### What moves from Swift to Zig

| Concern                  | ts5 (Swift)                  | Ghost (Zig)                      |
| ------------------------ | ---------------------------- | -------------------------------- |
| XPC communication        | CompositorXPC.swift (~500 L) | Zig via `@cImport("xpc/xpc.h")`  |
| IOSurface texture intake | Swift → C API → Zig renderer | Zig receives Mach port directly  |
| Keyboard forwarding      | NSEvent monitors in Swift    | Zig input handlers route to XPC  |
| Mouse forwarding         | NSEvent monitors in Swift    | Zig input handlers route to XPC  |
| Browse mode state        | Swift dictionaries           | Zig Surface state                |
| Focus lifecycle          | Swift NSNotification         | Zig focus callbacks              |
| Process spawning         | Swift `Process()`            | Zig `std.process.Child` or POSIX |

### Why this is better

1. **No middleman.** In ts5, Swift receives IOSurface Mach ports via XPC and
   immediately passes them to Zig via `ghostty_surface_set_overlay_iosurface`.
   In Ghost, Zig receives the Mach port directly and creates the Metal texture.
   One less boundary crossing at 120fps.

2. **Input routing in one place.** Zig already receives every keyboard and mouse
   event through `Surface.keyCallback()` and `mouseButtonCallback()`. In ts5,
   Swift intercepts events via `NSEvent.addLocalMonitorForEvents` before they
   reach Zig, requiring `suppressMouseForOverlay` flags and dual interception.
   In Ghost, Zig checks browse mode in its existing input handlers and routes to
   Chromium via XPC. No monitors, no flags, no fighting the natural flow.

3. **Single source of truth.** Browse mode, focus state, pane profiles, overlay
   coordinates — all live in Zig's Surface struct alongside the existing
   terminal state. No synchronization between Swift dictionaries and Zig state.

4. **Matches Ghostty's architecture.** Ghostty puts platform-specific code in
   Zig (Metal renderer, input encoding, keybindings). Swift is a thin shell.
   CompositorXPC broke that pattern. Ghost restores it.

### XPC in Zig

XPC is a pure C API (`<xpc/xpc.h>`). Zig calls C natively via `@cImport`. The
functions we use — `xpc_connection_create`, `xpc_dictionary_set_string`,
`xpc_dictionary_copy_mach_send` — are all plain C. No Objective-C or Swift
required. IOSurface is also a C API (`<IOSurface/IOSurface.h>`).

### Vsync

The ts5 vsync solution (120fps oversampling) works visually but wastes GPU. With
XPC and the CVDisplayLink both in Zig, demand-driven frame pulling becomes
natural — the vsync callback can request exactly one frame from Chromium per
display refresh. The vsync architecture will be revisited in a future issue.

### Compatibility

The `web` TUI continues to work unmodified. It communicates via the xpc-gateway
using XPC dictionary messages. It doesn't care whether the app-side handler is
in Swift or Zig — the protocol is the same.

The Chromium Profile Server also continues to work unmodified. It sends
IOSurface Mach ports and receives input events via XPC. The protocol is
unchanged.

### Scope

Ghost will be built incrementally across multiple issues:

| Issue | Scope                                  |
| ----- | -------------------------------------- |
| 600   | Fork Ghostty, build, run (this issue)  |
| 601+  | XPC gateway connection in Zig          |
|       | IOSurface overlay pipeline in Zig      |
|       | Chromium server lifecycle in Zig       |
|       | Keyboard and mouse forwarding in Zig   |
|       | Browse mode and focus lifecycle in Zig |
|       | Vsync rearchitecture                   |

Once Ghost replicates ts5's full feature set, ts1–ts5 will be archived.

## Experiment 1: Fork Ghostty into `ghost/`

### Goal

Import the latest Ghostty into `ghost/`, build it, and run it. No TermSurf
modifications — just a clean Ghostty under a new prefix.

### Changes

#### 1. Import Ghostty

Use `git subtree add` (proven in Issue 418 Experiment 3):

```bash
git fetch upstream
git subtree add --prefix=ghost upstream main
```

This creates a merge commit with the full Ghostty source tree under `ghost/`.

#### 2. Update `.gitignore`

Add build output patterns for `ghost/` (same patterns as ts5):

```gitignore
# TermSurf Ghost (ghost/)
ghost/zig-cache/
ghost/.zig-cache/
ghost/zig-out/
ghost/build/
ghost/.flatpak-builder/
ghost/flatpak/builddir/
ghost/flatpak/repo/
ghost/result*
ghost/.nixos-test-history
ghost/example/*.wasm
ghost/test/ghostty
ghost/test/cases/**/*.actual.png
ghost/glad.zip
ghost/Box_test.ppm
ghost/Box_test_diff.ppm
ghost/ghostty.qcow2
ghost/vgcore.*
```

#### 3. Build

```bash
cd ghost && zig build
```

Verify the app bundle is created at `ghost/zig-out/Ghostty.app`.

#### 4. Run

```bash
open ghost/zig-out/Ghostty.app
```

Verify Ghostty opens normally as a terminal emulator.

### Result

Pass. `ghost/zig-out/Ghostty.app` builds and runs as a standard Ghostty
terminal. No crashes, no missing resources, terminal input/output works.

## Conclusion

Ghost is forked and building. The foundation is in place for a Zig-first rewrite
of all browser integration logic that ts5 proved out in Swift.

Five generations of experiments (ts1–ts5, Issues 100–515) established every
piece of the architecture: IOSurface overlay compositing, XPC communication
protocols, Chromium Profile Server lifecycle, mouse and keyboard forwarding,
focus management, text selection, and vsync. Ghost takes these validated designs
and implements them where they belong — in Zig, alongside Ghostty's existing
input handling, Metal renderer, and surface management.

### Files changed

| File                                 | Change                             |
| ------------------------------------ | ---------------------------------- |
| `ghost/`                             | New directory (Ghostty subtree)    |
| `.gitignore`                         | Added ghost/ build output patterns |
| `CLAUDE.md`                          | Added Ghost section, updated ts5   |
| `docs/issues/0600-termsurf-ghost.md` | This issue                         |

### What's next

Issue 601+ will build browser integration in Zig, starting with the XPC gateway
connection and working up through IOSurface textures, Chromium server lifecycle,
input forwarding, and vsync.
