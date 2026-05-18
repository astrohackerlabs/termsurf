+++
status = "closed"
opened = "2026-02-08"
closed = "2026-02-08"
+++

# Issue 405: Architecture Comparison — Terminal In-Process vs Out-of-Process

## Goal

Choose between two architectures for ts4, now that Issue 404 has identified
Ghostty as the terminal emulator. Both architectures use out-of-process Chromium
for the browser. They differ in where the terminal lives.

## Context

Issue 403 proved that three processes (Swift + Rust + C++) can composite GPU
textures into a single window at 60fps via IOSurface Mach port transfer over
XPC. Issue 404 evaluated five terminal emulators and recommended Ghostty for its
Metal renderer that already outputs IOSurface-backed textures.

Ghostty is more than a renderer. It is a complete terminal application with pane
management, tab management, window management, keybindings, configuration, font
rendering, input handling, selection, scrollback, and more. Using Ghostty as
just a headless rendering library (as Issue 404 proposed) discards all of that.
The question is whether we should keep it.

TermSurf has prior art with both approaches:

- **ts1** forked Ghostty and added WKWebView browser panes alongside terminal
  panes. Terminal was in-process. WKWebView was in-process. Abandoned because
  WKWebView's API was too limited, not because the architecture was wrong.

- **ts3** forked WezTerm and added out-of-process CEF browser panes via XPC.
  Terminal was in-process (WezTerm's native rendering). Browser was
  out-of-process (CEF in a separate `termsurf-profile` process). This worked but
  WezTerm's rendering pipeline made IOSurface compositing difficult.

Both ts1 and ts3 kept the terminal in-process. Only the ts4 prototype (Issue
403) moved the terminal out-of-process — and that was to prove the IPC
mechanism, not because out-of-process terminal was a goal.

## Option A: Everything Out-of-Process (ts4 current plan)

The window is a Swift compositor. Both the terminal and browser are separate
processes that render to IOSurface and send Mach ports via XPC.

```
┌───────────────────────────────────────────┐
│  Swift Window Process (compositor only)   │
│  ├── NSWindow + CAMetalLayer              │
│  ├── Metal render pass (composite panes)  │
│  ├── Pane/tab/split management            │
│  ├── Keybindings + input routing          │
│  └── Configuration                        │
│       │ XPC              │ XPC            │
│       ▼                  ▼                │
│  Ghostty Process    Chromium Process      │
│  (Zig)              (C++)                 │
│  ├── Terminal state ├── CEF off-screen    │
│  ├── Metal renderer ├── IOSurface render  │
│  ├── Font pipeline  ├── Mach port send    │
│  ├── IOSurface out  └── XPC listener      │
│  ├── Mach port send                       │
│  └── XPC listener                         │
└───────────────────────────────────────────┘
```

### What we build from scratch

- Swift window application (NSWindow, CAMetalLayer, CVDisplayLink)
- Pane layout engine (splits, resize, focus tracking)
- Tab management (create, close, reorder, switch)
- Window management (multiple windows, fullscreen, split view)
- Keybinding system (configurable, modal, chord support)
- Configuration system (file format, hot reload)
- Input routing (keyboard/mouse → focused pane → XPC → child process)
- IOSurface compositor (Metal render pass, multiple textures)
- Clipboard integration (copy from terminal, paste to terminal)
- Selection rendering (highlight overlay, shipped from child process)
- Scrollback management (scroll commands → XPC → child, render updates)
- Font configuration UI (picker, preview, per-pane settings)
- Menu bar, context menus, toolbar
- Accessibility (VoiceOver, screen reader support)
- Notifications (bell, OSC alerts)
- Shell integration UI (marks, navigation, command history)
- URL detection and click handling
- Drag and drop
- Spotlight / services integration
- Auto-update mechanism

### What we get from Ghostty (via libghostty)

- Terminal state (VTE parser, cell grid, scrollback)
- Text rendering (CoreText, glyph atlas, Metal shaders)
- IOSurface rendering (already built)
- Input encoding (key → escape sequence)
- PTY management

### What we get from Chromium (via CEF)

- Browser rendering to IOSurface (proven in ts3)
- Cookie/session isolation per profile

## Option B: Ghostty Fork with Browser Out-of-Process

Fork Ghostty as the application. Terminal panes are native (in-process). Browser
panes are a new pane type that displays an IOSurface received from an
out-of-process Chromium instance via XPC.

```
┌───────────────────────────────────────────┐
│  Ghostty Fork (Zig + Swift macOS shell)   │
│  ├── NSWindow + CAMetalLayer              │
│  ├── Metal renderer (native IOSurface)    │
│  ├── Terminal panes (in-process, native)  │
│  ├── Browser panes (IOSurface from XPC)   │
│  ├── Pane/tab/split management ✓          │
│  ├── Keybindings ✓                        │
│  ├── Configuration ✓                      │
│  ├── Font pipeline ✓                      │
│  ├── Selection, scrollback ✓              │
│  ├── Shell integration ✓                  │
│  └── Everything else Ghostty provides ✓   │
│                          │ XPC            │
│                          ▼                │
│                    Chromium Process        │
│                    (C++)                   │
│                    ├── CEF off-screen      │
│                    ├── IOSurface render    │
│                    ├── Mach port send      │
│                    └── XPC listener        │
└───────────────────────────────────────────┘
```

### What we build from scratch

- Browser pane type in Ghostty (imports IOSurface, composites in render pass)
- XPC client in Ghostty (connects to Chromium process, receives Mach ports)
- Input forwarding for browser panes (keyboard/mouse → XPC → Chromium)
- Browser pane resize (send dimensions to Chromium on pane resize)
- `web` command (CLI or keybinding to open a URL in a browser pane)
- Chromium process management (spawn per profile, lifecycle, crash recovery)
- Profile management (which Chromium process handles which profile)
- Browser-specific UI (URL bar, navigation, loading indicator — if desired)

### What we get from Ghostty (for free)

- Pane layout engine (splits, resize, focus tracking)
- Tab management (create, close, reorder, switch)
- Window management (multiple windows, fullscreen, split view)
- Keybinding system (configurable, modal, chord support)
- Configuration system (file format, hot reload)
- Terminal rendering (Metal + IOSurface + CoreText + glyph atlas)
- Input handling (keyboard → escape sequence, mouse reporting)
- Clipboard integration (copy, paste, OSC 52)
- Selection rendering (character, line, block selection)
- Scrollback with search
- Font management (discovery, fallback, rendering)
- Menu bar, context menus
- Accessibility (VoiceOver support)
- Notifications (bell, OSC alerts)
- Shell integration (marks, navigation, semantic zones)
- URL detection and OSC 8 hyperlinks
- Drag and drop
- Auto-update mechanism (Sparkle on macOS)
- The entire macOS app shell (Swift)
- The entire Linux app shell (GTK) — future cross-platform path

### What we get from Chromium (via CEF)

- Browser rendering to IOSurface (proven in ts3)
- Cookie/session isolation per profile

## Comparison

### Scope of work

| Task                        | Option A (all out-of-process)  | Option B (Ghostty fork)  |
| --------------------------- | ------------------------------ | ------------------------ |
| Terminal rendering          | Modify libghostty for headless | Already done             |
| Browser rendering           | XPC + IOSurface (proven)       | XPC + IOSurface (same)   |
| Pane/tab/split management   | Build from scratch             | Inherited                |
| Window management           | Build from scratch             | Inherited                |
| Keybindings                 | Build from scratch             | Inherited                |
| Configuration               | Build from scratch             | Inherited                |
| Font management             | Configure libghostty           | Inherited                |
| Input handling (terminal)   | XPC forwarding                 | Native (in-process)      |
| Input handling (browser)    | XPC forwarding                 | XPC forwarding (same)    |
| Selection / scrollback      | Build from scratch             | Inherited                |
| Clipboard                   | Build from scratch             | Inherited                |
| Shell integration UI        | Build from scratch             | Inherited                |
| Accessibility               | Build from scratch             | Inherited                |
| macOS app shell             | Build from scratch             | Inherited                |
| Browser pane compositor     | Metal compositor (proven)      | Add to existing renderer |
| Browser pane type           | N/A (all panes are same)       | New pane variant         |
| XPC to Chromium             | Build (same either way)        | Build (same either way)  |
| Chromium process management | Build (same either way)        | Build (same either way)  |

Option A requires building ~15 major subsystems from scratch. Option B requires
building ~3 (browser pane type, XPC to Chromium, Chromium process management).

### Terminal rendering performance

| Metric                          | Option A                                                  | Option B                                      |
| ------------------------------- | --------------------------------------------------------- | --------------------------------------------- |
| Terminal frame path             | Render → IOSurface → Mach port → XPC → import → composite | Render → IOSurface → composite (same process) |
| IPC overhead per terminal frame | ~0.04ms (measured)                                        | Zero                                          |
| Memory copies                   | Zero (IOSurface)                                          | Zero (IOSurface)                              |
| Latency (keystroke → pixel)     | +XPC round trip                                           | Native                                        |

The IPC overhead is small (0.04ms measured in Issue 403), but it exists for
every terminal frame in Option A. In Option B, terminal rendering is in-process
with zero IPC overhead. Browser rendering has the same IPC overhead in both
options.

### Input latency

In Option A, every keystroke follows this path:

```
OS event → Swift window → XPC send → Ghostty process → PTY → shell response →
VTE parse → render → IOSurface → Mach port → XPC send → Swift window → composite
```

In Option B, terminal keystrokes stay in-process:

```
OS event → Ghostty → PTY → shell response → VTE parse → render → composite
```

Browser keystrokes follow the XPC path in both options.

### Architectural complexity

**Option A** is architecturally clean but operationally complex. Every
interaction with the terminal crosses a process boundary. Resize requires an XPC
round trip. Selection requires synchronizing state across processes. Scrollback
search requires an XPC query/response protocol. Each of these is solvable, but
each adds latency, error handling, and protocol surface area.

**Option B** is architecturally asymmetric (terminal native, browser foreign)
but operationally simple for terminal interactions. Only browser interactions
cross a process boundary. The asymmetry reflects a real difference: the terminal
IS the application, the browser is a guest.

### Merge upstream maintenance

**Option A** uses libghostty as a static library. Upstream Ghostty updates are
consumed by rebuilding the library. No merge conflicts. But we also don't
benefit from upstream improvements to the app shell, pane management,
keybindings, etc. — because we don't use them.

**Option B** forks Ghostty and must merge upstream changes. This is the same
workflow as ts1 (Ghostty fork) and ts3 (WezTerm fork). Merge conflicts happen
when our modifications touch the same files as upstream changes. The merge
burden depends on how invasive our modifications are.

Our modifications for Option B are:

1. **New browser pane type** — new files, unlikely to conflict with upstream.
2. **XPC client for Chromium** — new files, no conflicts.
3. **Metal renderer modification** — add IOSurface import and composite for
   browser panes alongside native terminal rendering. This touches `Metal.zig`
   and related files, which upstream also modifies. Merge conflicts are likely
   here.
4. **`web` command** — new command, minimal conflict surface.
5. **macOS app shell changes** — add XPC connection management. Touches Swift
   files that upstream also modifies. Some merge conflicts expected.

Items 1, 2, and 4 are additive (new files/commands). Items 3 and 5 modify
existing files and will occasionally conflict during upstream merges. This is
manageable — ts1 and ts3 both handled similar merge workflows.

### Cross-platform future

**Option A** is more portable in theory. The Swift window is macOS-only, but
could be rewritten for Linux (Rust + winit + wgpu) or Windows. Both child
processes are platform-agnostic in their IPC abstraction (XPC on macOS, D-Bus or
Unix sockets on Linux).

**Option B** inherits Ghostty's cross-platform support. Ghostty has both a macOS
app shell (Swift) and a Linux app shell (GTK). If we add browser pane support to
both shells, we get cross-platform for free. The IPC mechanism would need to
change on Linux (XPC is macOS-only), but the browser pane abstraction would be
the same.

In practice, Option B is more portable because Ghostty has already solved the
platform abstraction problem for the terminal side. We only need to solve it for
the browser pane IPC.

### Risk assessment

| Risk                           | Option A | Option B |
| ------------------------------ | -------- | -------- |
| Scope creep (building the app) | High     | Low      |
| Merge conflicts with upstream  | None     | Moderate |
| Terminal rendering bugs        | Medium   | Low      |
| Browser integration complexity | Same     | Same     |
| Ghostty API instability        | High     | Low      |

**Option A's biggest risk** is scope. Building a terminal application from
scratch — even with libghostty for rendering — is a multi-month effort. Pane
management alone (splits, resize, focus, drag to reorder) is hundreds of hours.
And libghostty's embedding API is explicitly "not yet supported as a general
purpose embedding API" — it may have rough edges and breaking changes.

**Option B's biggest risk** is merge conflicts with upstream Ghostty. But the
modifications are relatively contained (new pane type + renderer extension + app
shell XPC), and the merge-upstream workflow is proven from ts1 and ts3.

## Recommendation

**Option B: Fork Ghostty with browser out-of-process.**

The decision comes down to build vs. buy. Option A builds the application from
scratch and buys only the rendering engine. Option B buys the entire application
and builds only the browser integration.

The browser integration (XPC + IOSurface + Chromium process management) is the
same work in both options. The difference is everything else — and "everything
else" is the majority of a terminal application.

This is a return to the ts1 approach with the critical fix: replace WKWebView
(which was too limited) with out-of-process Chromium (which is not). The
architecture is:

- **ts1's window model** — Ghostty fork, terminal panes are native
- **ts3's browser model** — out-of-process CEF, IOSurface via XPC Mach ports
- **ts4's IPC proof** — the IOSurface/XPC mechanism is validated

### What changes from ts1

| Aspect             | ts1                       | ts4 (proposed)                |
| ------------------ | ------------------------- | ----------------------------- |
| Terminal           | Ghostty (in-process)      | Ghostty (in-process, same)    |
| Browser engine     | WKWebView (in-process)    | Chromium/CEF (out-of-process) |
| Browser IPC        | None (in-process)         | XPC + IOSurface Mach ports    |
| Browser limitation | WKWebView API too limited | CEF has full Chromium API     |
| Profile isolation  | WKWebsiteDataStore        | One CEF process per profile   |
| Pane management    | Ghostty native            | Ghostty native (same)         |

### Implementation path

1. Start from ts1 (existing Ghostty fork) or a fresh Ghostty fork.
2. Add a browser pane type that displays an imported IOSurface.
3. Add XPC client code to connect to Chromium profile processes.
4. Port the `termsurf-profile` CEF process from ts3 (already working).
5. Port the `termsurf-launcher` from ts3 (or simplify if one profile suffices
   initially).
6. Add the `web` command to open a URL in a browser pane.
7. Add input forwarding from browser panes to Chromium via XPC.
8. Add resize forwarding from browser panes to Chromium via XPC.

Steps 3–5 are largely done (ts3 has working code). Steps 2, 7, and 8 are the new
work — adding browser pane support to Ghostty's renderer and input system.
