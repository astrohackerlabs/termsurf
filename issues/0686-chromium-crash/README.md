+++
status = "closed"
opened = "2026-03-01"
closed = "2026-03-01"
+++

# Issue 686: Chromium Crashes After Multiple Sessions

Chromium becomes unresponsive after opening and closing several browser sessions
and/or DevTools panes. It stops responding to resize, keyboard, and mouse input.
Closing and reopening TermSurf fixes it.

## Symptoms

- Open a few browser tabs, close them, open more, eventually one becomes
  unresponsive
- The browser pane renders but won't resize or accept input
- DevTools sessions may trigger it faster
- No visible error — just a frozen pane
- Restarting TermSurf restores normal behavior

## Log Evidence

Analysis of `logs/gui.log` (Feb 23–26) found three types of errors that may be
related:

### 1. Breakpad crash (Feb 23, 19:13:47)

```
[sentry] INFO entering breakpad minidump callback
[sentry] INFO crash has been captured
[ProfileServer] Control connection interrupted
[ProfileServer] Closing tab for pane ..., 0 tab(s) remaining
[ProfileServer] No tabs remaining, exiting
```

A Chromium subprocess crashed hard enough to trigger a breakpad minidump. The
profile server detected the control connection was interrupted and shut down.
TermSurf spawned a new server and continued. The crash happened right after
sending a `ca_context_id`. No stack trace in the log — the minidump was sent to
sentry (but with an empty DSN, so it went nowhere).

### 2. Mach port rendezvous failure (Feb 26, 19:16:19)

```
bootstrap_look_up com.termsurf.chromium-profile-server.MachPortRendezvousServer.1: Permission denied (1100)
No rendezvous client, terminating process (parent died?)
```

A Chromium child process (PID 82160) couldn't connect to its parent's Mach port
rendezvous server and terminated itself. This appeared after several open/close
cycles in the same TermSurf session.

### 3. Orphaned surface references (Feb 23, multiple)

```
Old/orphaned temporary reference to SurfaceId(FrameSinkId[](5, 60), LocalSurfaceId(2, 1, 073D...))
```

The viz compositor reported orphaned surface references after navigations. These
appeared during active sessions with multiple navigations.

### 4. Reproduced crash: DevTools overlay paint DCHECK (Mar 1, 12:17:55)

Cleared all logs and reproduced the crash. Full stack trace captured.

**The crash:**

```
FATAL: third_party/blink/renderer/platform/graphics/paint/paint_controller.cc:662
DCHECK failed: !map.Contains(id.AsHashKey())
```

A duplicate `DisplayItem::Id` was added to the paint controller's index map
during a DevTools overlay paint cycle. The crash key
`"devtools_present" = "true"` confirms DevTools was active.

**Call chain:**

```
InspectorOverlayAgent::PageLayoutInvalidated
  → cc::ProxyMain::BeginMainFrame
  → LocalFrameView::PaintTree
  → WebDevToolsAgentImpl::PaintOverlays
  → InspectorOverlayAgent::PaintFrameOverlay
  → RecordForeignLayer
  → PaintController::ProcessNewItem
  → PaintController::AddToIdIndexMap
  → DCHECK: duplicate display item ID
```

**What was happening:**

- Two profile servers running (PIDs 98065 and 98380) — default and work profiles
- Both had DevTools open (5 tabs active on one server)
- Pane `C6D1640A` (DevTools) had focus, user was clicking in it
- Focus left `C6D1640A` at 12:17:52
- Pane `14FE0A14` (browser tab) was resized at 12:17:55
- Renderer PID 98074 crashed at 12:17:55.726 while painting the DevTools
  inspector overlay

**The crash is in the renderer process** (PID 98074), not the browser process.
The `PaintController` found a duplicate `DisplayItem::Id` when painting the
`InspectorOverlayAgent`'s frame overlay via `RecordForeignLayer`. This means the
DevTools overlay's foreign layer (the element highlighting layer) was registered
twice in the same paint cycle.

This is a DCHECK — it only fires in debug builds. In release builds the same
condition would silently corrupt the paint state, which could explain the
"unresponsive but still rendering" symptom.

## Reliable Repro

1. Open a browser tab: `web google.com`
2. Open two DevTools panes inspecting the same tab: `web devtools://1` twice
3. Resize the window

Crash occurs immediately on resize. Reproduced 4 times in a row — 100% hit rate.

**Why it crashes:** Both DevTools sessions attach an `InspectorOverlayAgent` to
the same inspected renderer. Each overlay paints via `RecordForeignLayer`, which
registers a `DisplayItem::Id` in the `PaintController`'s index map. When a
resize triggers a repaint, both overlays try to register the same display item
ID (same foreign layer on the same page). The second registration hits the
duplicate DCHECK.

All four crashes in the session log are identical — same file, same line, same
DCHECK, same call chain. Every one is preceded by resize events in the log.

## What We Know

- The crash is in Blink's paint system, specifically in the DevTools overlay
  paint path (`InspectorOverlayAgent::PaintFrameOverlay`)
- It happens in the renderer process, not the browser or GPU process
- It's triggered by a duplicate display item ID during overlay painting
- The specific trigger is: two DevTools sessions inspecting the same page +
  resize
- DevTools sessions are the common factor — the crash consistently involves
  DevTools
- The crash is in upstream Chromium code
  (`third_party/blink/renderer/platform/graphics/paint/paint_controller.cc`),
  not in our fork code
- Chromium normally prevents this by only allowing one DevTools frontend per
  inspected page — TermSurf bypasses that guard by creating separate
  `ShellDevToolsFrontend` instances for each pane

## What We Don't Know

- Whether the earlier errors (breakpad crash, Mach port failure, orphaned
  surfaces) are the same bug or separate issues
- Whether this is a known upstream Chromium bug or unique to our multi-frontend
  setup
- Whether switching from debug to release builds would mask the crash but leave
  the underlying paint corruption
- Whether preventing duplicate DevTools for the same tab (in the GUI or TUI) is
  sufficient, or whether other scenarios can also trigger the duplicate paint ID

## Relevant Code

- `third_party/blink/renderer/platform/graphics/paint/paint_controller.cc:662` —
  the DCHECK that fires
- `third_party/blink/renderer/core/inspector/inspector_overlay_agent.cc` —
  DevTools overlay painting
- `chromium/src/content/chromium_profile_server/browser/shell_browser_main_parts.cc`
  — profile server lifecycle, tab creation/destruction
- `gui/src/apprt/xpc.zig` — server spawning, pane cleanup, `cleanupPane`

## Log Locations

- `logs/gui.log` — combined GUI + Chromium output
- `logs/chromium-server.log` — Chromium profile server only
- `logs/termsurf.log` — TermSurf app output
- `logs/xpc-gateway.log` — XPC gateway (empty)

## Conclusion

The crash is caused by opening two DevTools sessions for the same inspected
page. Chromium's `InspectorOverlayAgent` paints a foreign layer overlay on the
inspected renderer. Two overlays on the same renderer produce duplicate
`DisplayItem::Id` entries in the `PaintController`'s index map, which triggers a
DCHECK on the next repaint (reliably caused by a resize).

This is not a bug in our fork code — it's an upstream Chromium invariant. Chrome
enforces one DevTools frontend per inspected page; TermSurf bypasses that by
creating independent `ShellDevToolsFrontend` instances for each pane. The fix is
to enforce the same one-DevTools-per-tab constraint.

The earlier log errors (breakpad crash, Mach port rendezvous failure, orphaned
surfaces) may or may not be the same root cause. They could be consequences of
renderer crashes caused by this same duplicate overlay issue, or they could be
independent. With the one-DevTools-per-tab fix in place, we can observe whether
those errors stop appearing.

### What was accomplished

- Identified the crash: `PaintController::AddToIdIndexMap` DCHECK in
  `paint_controller.cc:662`
- Traced the full call chain from `InspectorOverlayAgent::PageLayoutInvalidated`
  through to the duplicate display item registration
- Found a 100% reliable 3-step repro: open page, open two DevTools for it,
  resize
- Determined the root cause: two `InspectorOverlayAgent` instances painting
  overlays on the same renderer
- Identified the fix: enforce one DevTools session per inspected tab (Issue 687)
