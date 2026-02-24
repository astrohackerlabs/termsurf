# Issue 631: Continue Navigation CALayerHost

## Goal

Eliminate the ~100ms flicker that occurs on every page navigation. The browser
overlay should transition seamlessly — no visible blank frame between the old
page and the new page.

## Background

### CALayerHost issue history

This is the seventh issue in the CALayerHost series. Each addressed a different
regression from the migration away from `FrameSinkVideoCapturer`:

- [Issue 625](625-calayerhost.md) — **CALayerHost migration.** Replaced the
  `FrameSinkVideoCapturer` pipeline with `CALayerHost`. Instead of capturing
  IOSurface frames at 120fps and transferring Mach ports over XPC every frame,
  Chromium now sends a `ca_context_id` (uint32) once per tab. The GUI creates a
  `CALayerHost` sublayer, and Window Server composites the remote content
  directly from GPU VRAM. Zero per-frame IPC, zero texture copies.

- [Issue 626](626-x-y-calayerhost.md) — **X/Y positioning.** The CALayerHost
  overlay had a ~10px Y and ~3px X offset. Fixed by adding a positioning layer
  inside a geometry-flipped layer, matching Chromium's `maybe_flipped_layer_`
  pattern.

- [Issue 627](627-resize-calayerhost.md) — **Resize.** The overlay stopped
  resizing when the user resized the window or pane. Fixed by propagating resize
  events through XPC to the Chromium capturer and updating the positioning
  layer's frame.

- [Issue 628](628-navigation-calayerhost.md) — **Navigation (first attempt).**
  Ran 8 experiments targeting the Chromium-side pipeline. All failed. Key
  finding from diagnostic logging: the new `ca_context_id` arrives within 100ms
  and the GUI replaces the `CALayerHost` immediately, yet the new host shows
  nothing for ~10 seconds.

- [Issue 629](629-understand-nav-calayerhost.md) — **Navigation (diagnosis).**
  Research issue. Five experiments: compared Electron/Chromium CALayerHost
  usage, traced the CAContext lifecycle, tested `DisableDisplay()` (made things
  worse), audited all 10-second delays in Chromium, and performed a full code
  audit of both the GUI and Chromium Profile Server. Produced the primary
  hypothesis and confirmed two latent bugs.

- [Issue 630](630-nav-calayerhost-6.md) — **Navigation (fix).** Resolved the
  permanent overlay disappearance with seven coordinated fixes across GUI (Zig)
  and Chromium (C++): transparent hidden window instead of `orderOut:` (C1),
  callback re-registration on view swap (C2), dedup gate reset (C3),
  `ResizeWebContentForTests` for correct `dfh_size_dip_` (C4), main-thread
  dispatch with CATransaction wrapping (G1), atomic CALayerHost swap (G2), and
  zero context ID guard (G3). Navigation no longer causes permanent
  disappearance, but a brief ~100ms flicker remains on every navigation.

### What we know

1. **The permanent blank is fixed.** Issue 630's seven fixes resolved the
   overlay vanishing forever on navigation.
2. **A ~100ms flicker remains.** On every navigation, the overlay briefly
   disappears then reappears. Visible and annoying, but not app-breaking.
3. **The flicker is likely compositor-side.** The CAContext's content tree is
   torn down and rebuilt during navigation. Even though the CALayerHost stays
   pointed at the right CAContext, there is a brief moment where the CAContext
   has no rendered content.
4. **The `ca_context_id` may not change.** During same-site navigation, the same
   `CALayerTreeCoordinator` may keep the same ID. If so, the CALayerHost swap
   triggered by our dedup reset (C3) is unnecessary and may itself cause the
   flicker.

### Untested CALayerHost changes

The CALayerHost migration (Issues 625–630) replaced fundamental rendering
infrastructure. The following features have not been retested since the
migration and may have regressions:

- **Mouse input**: clicks, drag, scroll, cursor changes (Issue 606)
- **Keyboard input**: key forwarding, Cmd+key bypass, clipboard, Tab (Issues
  607–609)
- **Loading progress**: progress bar, pulse animation (Issue 616)
- **Browser navigation keybindings**: Cmd+L, Cmd+R, back/forward (Issue 616)
- **Multi-pane multi-profile**: server reuse, independent tabs (Issues 604–605)
- **Dynamic resize**: pane resize propagation through XPC (Issue 627)
- **Text selection**: drag-to-select, cursor changes (Issue 606)

A comprehensive retest should be performed as part of this issue or immediately
after.

### Chromium branch

Continue from `146.0.7650.0-issue-630`.

### Possible approaches

- **Don't swap when `ca_context_id` is unchanged.** If same-site navigation
  keeps the same ID, skipping the CALayerHost replacement eliminates the
  GUI-side gap entirely.
- **Snapshot before swap.** Capture the current CALayerHost content as a
  `CGImage` and place it on a static `CALayer` behind the host. When the host
  goes blank during transition, the snapshot shows through.
- **Delay old host removal.** Keep the old CALayerHost for ~200ms after adding
  the new one, so the old content remains visible until the new host composites.
- **Debug logging.** Add timestamps at every stage (XPC arrival, host swap,
  Chromium callback) to confirm whether the gap is GUI-side or Chromium-side.

## Experiments

### Experiment 1: Audit code for navigation flicker smells

#### Purpose

Identify what causes the ~100ms blank frame during every page navigation. The
permanent blank is fixed (Issue 630), but a brief flicker remains. This audit
searches both the GUI (Zig) and Chromium Profile Server (C++) code for patterns
that could cause a momentary gap in content during navigation.

This is a research-only audit — no code modifications.

#### Code smells

**Flicker-specific smells (1–10):**

1. **Unnecessary CALayerHost swap on same ID.** The dedup gate reset (C3) forces
   `*last_ca_context_id_ = 0` on every navigation. If the `ca_context_id`
   doesn't actually change during same-site navigation, this causes the callback
   to fire with the same ID, triggering a full CALayerHost destroy-and-recreate
   in the GUI — a swap that produces a blank frame for no reason.

2. **CAContext content gap during compositor surface transition.** When the
   Chromium compositor processes a navigation, it may invalidate the old
   `LocalSurfaceId` and allocate a new one. During the transition, the
   `CAContext` exists but has no submitted frame — the CALayerHost renders as
   transparent. This is a Chromium-side gap that no GUI-side fix can address.

3. **Async main-thread dispatch adds latency to host swap.** The `ca_context`
   XPC message arrives on the XPC queue, then `dispatch_async_f` to the main
   queue adds scheduling latency. If the old host was already torn down
   Chromium-side but the new host isn't created until the main queue drains,
   there is a visible gap.

4. **draw_mutex contention during swap.** `setCAContextId()` acquires
   `draw_mutex` on the main thread. If the renderer thread holds it (mid-frame),
   the main thread blocks. The CALayerHost replacement is delayed until the
   frame completes, extending the blank window.

5. **No content readiness signal.** The GUI swaps the CALayerHost as soon as the
   new `ca_context_id` arrives. But the new CAContext may not have a submitted
   frame yet. The new host is added to the layer tree pointing at an empty
   context. A "content ready" signal from Chromium (e.g., after the first
   `SubmitCompositorFrame`) would allow delaying the swap until content exists.

6. **Old host removed too early in atomic swap.** The atomic swap (G2) adds the
   new host before removing the old one, but both happen in the same
   CATransaction. If the new host's CAContext has no content yet, removing the
   old host (which may still have stale content from the old page) eliminates
   the only visible content. The old host should stay until the new one has
   rendered.

7. **CATransaction commit flushes both add and remove simultaneously.** The
   single CATransaction wrapping the entire swap means Window Server sees "add
   new + remove old" as one atomic operation. If the new host's context is
   empty, Window Server transitions from "old content" to "nothing" in one
   commit.

8. **Chromium `DidNavigate()` surface ID churn.** During navigation,
   `BrowserCompositorMac::DidNavigate()` calls
   `InvalidateLocalSurfaceIdAndAllocationGroup()` which invalidates the current
   surface, then allocates a new one via `GetRendererLocalSurfaceId()`. Between
   invalidation and the first frame on the new surface, the CAContext has
   nothing to display.

9. **No fallback content during transition.** Unlike the old
   `FrameSinkVideoCapturer` pipeline (which always had the last captured frame
   as a texture), the CALayerHost pipeline has no fallback. When the CAContext
   goes empty, there is nothing to show — just transparency.

10. **RenderViewHostChanged re-registration triggers redundant swap.** If
    `RenderViewHostChanged` fires AND the CALayerParams callback also fires with
    a new ID, two host swaps happen in quick succession. The first swap may
    create a host pointing at a stale context, immediately replaced by the
    second.

**Structural smells (11–15):**

11. **No timestamp logging at swap boundaries.** We have log lines for "replaced
    CALayerHost" and "Sent ca_context_id" but no microsecond timestamps showing
    the gap between: (a) Chromium navigation commit, (b) CALayerParams callback
    fire, (c) XPC message send, (d) XPC message receive, (e) main-thread
    dispatch, (f) CALayerHost swap, (g) first visible frame. Without these, we
    cannot distinguish GUI-side from Chromium-side flicker.

12. **Dedup reset timing vs callback timing.** The dedup gate is reset in
    `DidFinishNavigation()`, which fires when the navigation commits. The
    CALayerParams callback fires when the compositor produces new params. If the
    compositor fires BEFORE `DidFinishNavigation` resets the gate, the callback
    is still blocked by the old dedup value and the new context ID is missed.

13. **No distinction between same-site and cross-site navigation.** The code
    treats all navigations identically — full dedup reset, potential host swap.
    Same-site navigations (where the CAContext survives) and cross-site
    navigations (where the RenderViewHost changes) may need different handling.

14. **CALayerHost replacement vs contextId update.** The code always destroys
    and recreates the CALayerHost when the context ID changes. Chromium's
    `DisplayCALayerTree::GotCALayerFrame()` does the same, but an alternative is
    to update the `contextId` property on the existing host. This avoids the
    remove/add cycle entirely. Issue 628 noted this "may not rebind Window
    Server compositing" but this was never tested in the post-630 codebase.

15. **Overlay visibility during loading state.** The `DidStartLoading` /
    `DidStopLoading` XPC messages are sent to the GUI, but the GUI does not use
    them to manage CALayerHost visibility. If the GUI knew a navigation was in
    progress, it could hold the old content visible until the new page's first
    frame arrives.

#### Files to audit

**GUI (Zig):**

- `gui/src/renderer/Metal.zig` — `setCALayerHostContextId()` swap logic,
  CATransaction wrapping, layer creation
- `gui/src/Surface.zig` — `setCAContextId()`, `draw_mutex` acquisition
- `gui/src/apprt/xpc.zig` — `handleCAContext()`, main-thread dispatch,
  `handleLoadingState()`

**Chromium (C++):**

- `content/chromium_profile_server/browser/shell_browser_main_parts.cc` —
  CALayerParams callback, dedup gate, `CreateTab()`
- `content/chromium_profile_server/browser/shell_tab_observer.cc` —
  `DidFinishNavigation()` dedup reset, `RenderViewHostChanged()` re-registration
- `content/chromium_profile_server/browser/shell_tab_observer.h` — observer
  interface, stored state

#### Steps

For each of the 6 files above:

1. Read the file in full.
2. Check each of the 15 code smells.
3. Record a verdict: **clean** (not present), **suspect** (possible but
   unconfirmed), or **confirmed** (definitely present).
4. Add a one-line note explaining the verdict.

#### Output format

A findings table per file:

```
#### File: `path/to/file.zig`

| # | Smell | Verdict | Note |
|---|-------|---------|------|
| 1 | Unnecessary swap on same ID | confirmed | Dedup reset forces swap even when ID unchanged |
| … | … | … | … |
```

After all files, a summary section listing every confirmed and suspect finding
with file path and line number.

#### Verification

Every confirmed and suspect finding has a file path, line number, and one-line
explanation. No smell is left unchecked for any file.

#### Findings

##### File: `gui/src/renderer/Metal.zig`

**#1 — Unnecessary swap on same ID: confirmed.** `setCALayerHostContextId()`
(line 198) always takes the replace path when a host exists — destroys old,
creates new — regardless of whether `context_id` is the same as the current
host's `contextId`. No comparison is made. Every dedup-reset-triggered callback
causes a full host swap even when the ID hasn't changed.

**#2 — CAContext content gap: suspect.** Not directly visible in this file. The
new CALayerHost is created at line 208–212 and added at line 217. If the
CAContext's content tree is being rebuilt Chromium-side at this moment, the new
host renders as transparent. This file cannot prevent that.

**#3 — Async dispatch latency: clean.** Not applicable — this file is the
destination of the dispatch, not the source.

**#4 — draw_mutex contention: confirmed.** Called via `Surface.setCAContextId()`
which holds `draw_mutex` (Surface.zig:2525). The main-thread dispatch from
xpc.zig blocks on this mutex. If the renderer thread is mid-frame, the
CALayerHost swap is delayed until the frame completes — potentially one full
frame period (~16ms at 60fps).

**#5 — No content readiness signal: confirmed.** The host is added immediately
at line 217 with no check that the CAContext has a submitted frame. The new host
may point at an empty context.

**#6 — Old host removed too early: confirmed.** Old host is removed at line 222
in the same CATransaction as the new host addition (line 217). If the new host's
context is empty, removing the old host eliminates any visible content. The old
host may still have had the previous page's content composited by Window Server.

**#7 — CATransaction flushes add+remove together: confirmed.** Single
`CATransaction begin` (line 195) wraps both `addSublayer:` (line 217) and
`removeFromSuperlayer` (line 222), committed together at line 271. Window Server
sees the removal and addition as one atomic step.

**#8 — DidNavigate surface ID churn: clean.** Not applicable — GUI side.

**#9 — No fallback content: confirmed.** No snapshot or fallback layer. When the
CALayerHost is replaced, the only content is whatever the new host's CAContext
provides. If that's empty, the user sees the terminal background.

**#10 — Redundant swap from re-registration: clean.** `RenderViewHostChanged` is
Chromium-side; this file doesn't interact with it.

**#11 — No timestamp logging at swap boundaries: confirmed.**
`log.info("replaced CALayerHost contextId={}...")` at line 227 uses Zig's
default log format. No microsecond timestamps. Cannot measure the gap between
XPC arrival and host swap completion.

**#12 — Dedup reset vs callback timing: clean.** Not applicable — GUI side.

**#13 — No same-site vs cross-site distinction: confirmed.** The replacement
path (line 198) does the same full destroy-and-recreate for any context ID,
whether same-site (same ID) or cross-site (new ID).

**#14 — Replacement vs contextId update: confirmed.** Line 199–202 comment says
"updating contextId on an existing host may not rebind Window Server
compositing" — but this was never tested post-630. The code always destroys and
recreates. Setting `contextId` on the existing host would avoid the remove/add
gap entirely.

**#15 — Loading state not used for visibility: clean.** Not applicable —
renderer layer, doesn't handle XPC messages.

##### File: `gui/src/Surface.zig`

**#4 — draw_mutex contention: confirmed.** `setCAContextId()` (line 2525) locks
`draw_mutex` on the main thread. The renderer thread's `drawFrame()` also holds
this mutex. If the renderer is mid-frame, the main thread blocks until it
finishes. This adds up to ~16ms latency to the CALayerHost swap.

All other smells are clean — this file is a pass-through to Metal.zig.

##### File: `gui/src/apprt/xpc.zig`

**#1 — Unnecessary swap on same ID: confirmed.** `handleCAContext()` (line 410)
passes every non-zero `context_id` through to `surface.setCAContextId()` with no
comparison against the current host's ID. When the dedup gate is reset (C3), the
Chromium callback fires with the same ID it already sent, and the GUI does a
full host swap for nothing.

**#3 — Async dispatch latency: confirmed.** `dispatch_async_f` to main queue
(line 436) adds scheduling latency. The XPC message arrives on the serial XPC
queue, then waits for the main run loop to drain before the dispatch block runs.
If the main thread is busy (e.g., processing input, rendering), the swap is
delayed.

**#11 — No timestamp logging: suspect.**
`log.info("ca_context pane={s}
context_id={}")` at line 414 logs arrival, but no
microsecond timestamp. Cannot measure dispatch latency to main thread.

**#15 — Loading state not used for visibility: confirmed.**
`handleLoadingState()` (line 469) forwards the loading state to the TUI via XPC
but does not use it to manage CALayerHost visibility. The GUI has
`DidStartLoading` → `"loading"` and `DidStopLoading` → `"done"` signals
available. If the GUI knew navigation was starting, it could freeze the old
CALayerHost content until the new page's first frame arrives.

All other smells are clean.

##### File: `shell_browser_main_parts.cc`

**#1 — Unnecessary swap on same ID: confirmed.** The dedup gate (line 411)
filters `params.ca_context_id == *last_id`. But `DidFinishNavigation` resets
`*last_id = 0` (via observer, line 95–96 of shell_tab_observer.cc). After reset,
the next CALayerParams callback fires even if `ca_context_id` is unchanged. The
GUI receives the same ID it already has and does a full host
destroy-and-recreate. This is the root of the unnecessary swap chain.

**#2 — CAContext content gap: suspect.** Between `DidNavigate()` invalidating
the old surface and the compositor submitting the first frame on the new
surface, the CAContext's content tree may be empty. The CALayerParams callback
fires with the same or new ID, but the context has nothing to composite. This
gap is internal to `BrowserCompositorMac` and cannot be observed from this file.

**#5 — No content readiness signal: confirmed.** The CALayerParams callback
(line 408–428) fires whenever the compositor produces new params — including
during the surface transition when no frame has been submitted yet. There is no
distinction between "new context with content" and "new context, still empty."
The GUI cannot tell whether the new ID points at a ready context.

**#8 — DidNavigate surface ID churn: suspect.** Not directly in this file, but
`ResizeWebContentForTests` (line 349) and other Shell operations may trigger
`DidNavigate()` in `BrowserCompositorMac`. The surface invalidation and
re-allocation during navigation is the suspected source of the content gap.
Needs Chromium-internal logging to confirm.

**#10 — Redundant swap from re-registration: suspect.** `RenderViewHostChanged`
(observer line 58–78) re-registers the callback on the new view. If this fires
shortly after `DidFinishNavigation` resets the dedup gate, the re-registered
callback may fire with the same ID, causing a second XPC send + GUI swap on top
of the first.

**#11 — No timestamp logging at swap boundaries: confirmed.**
`LOG(INFO) <<
"Sent ca_context_id=..."` (line 425) uses Chromium's default
logging. No `base::TimeTicks::Now()` or microsecond precision. Cannot correlate
with GUI-side logs to measure end-to-end latency.

**#12 — Dedup reset timing vs callback timing: confirmed.**
`DidFinishNavigation` resets `*last_ca_context_id_ = 0` (observer line 95–96).
The CALayerParams callback may fire at any time — before, during, or after this
reset. If it fires after the reset, it re-sends an ID the GUI already has,
triggering an unnecessary swap. There is no synchronization between the
navigation commit and the callback timing.

**#13 — No same-site vs cross-site distinction: confirmed.** The dedup reset
(C3) fires unconditionally for every committed primary-frame navigation
(`DidFinishNavigation` line 85–90). Same-site navigations (where the
CAContext/ID survives) are treated identically to cross-site navigations (where
a new view and context are created). For same-site, the reset is unnecessary and
causes the redundant swap.

All other smells are clean.

##### File: `shell_tab_observer.cc`

**#1 — Unnecessary swap on same ID: confirmed.** `DidFinishNavigation()` (line
95–96) unconditionally resets `*last_ca_context_id_ = 0`. This forces the next
CALayerParams callback to fire regardless of whether the `ca_context_id`
actually changed. For same-site navigation where the ID stays the same, this
triggers a GUI-side destroy-and-recreate of the CALayerHost for no reason.

**#10 — Redundant swap from re-registration: suspect.** `RenderViewHostChanged`
(line 58–78) re-registers the CALayerParams callback on the new view. If this
fires around the same time as `DidFinishNavigation`, the newly registered
callback may immediately fire with params from the new view's compositor.
Combined with the dedup reset, this could trigger two rapid successive sends of
the same or different `ca_context_id`, causing two GUI-side swaps in quick
succession.

**#11 — No timestamp logging: confirmed.**
`LOG(INFO) <<
"Navigation committed:..."` (line 103) and
`"RenderViewHostChanged..."` (line 66) use default logging format. No
microsecond timestamps to correlate with the CALayerParams callback timing or
GUI-side swap.

**#12 — Dedup reset timing vs callback timing: confirmed.** The dedup reset at
line 95–96 happens inside `DidFinishNavigation`. The CALayerParams callback is
asynchronous — it fires when the compositor produces new params, which may be
before or after this method runs. There is no ordering guarantee. If the
callback fires immediately after reset, it re-sends an ID the GUI already has.

**#13 — No same-site vs cross-site distinction: confirmed.**
`DidFinishNavigation` (line 85–96) does the same dedup reset for all committed
navigations. `navigation_handle->IsSameDocument()` and
`navigation_handle->IsSameOrigin()` are available but not checked. Same-site
navigations likely keep the same CAContext and ID, making the reset unnecessary.

All other smells are clean.

##### File: `shell_tab_observer.h`

All smells are clean. Header declares interface only — all implementation
reviewed in the `.cc` file above.

#### Summary

##### Confirmed findings

**#1 — Unnecessary swap on same ID** — `shell_tab_observer.cc:95–96`,
`shell_browser_main_parts.cc:411`, `xpc.zig:410`, `Metal.zig:198`. **Most likely
cause of the flicker.** The dedup reset (C3) forces `*last_id = 0` on every
navigation. For same-site navigation, the `ca_context_id` doesn't change, but
the reset causes the callback to re-send the same ID. The GUI receives it,
enters the replacement path, destroys the old CALayerHost, and creates a new one
pointing at the same CAContext. During the swap, the positioning layer has no
host (or a host pointing at an empty-during-transition context), producing the
visible blank frame.

**#4 — draw_mutex contention** — `Surface.zig:2525`, `Metal.zig:172`. The
main-thread dispatch blocks on `draw_mutex` if the renderer is mid-frame. Adds
up to ~16ms to the swap latency. Combined with smell #1, this extends the blank
window.

**#5 — No content readiness signal** — `Metal.zig:208–217`,
`shell_browser_main_parts.cc:408–428`. The CALayerParams callback fires whenever
the compositor produces params, including during surface transitions when no
frame has been submitted. The GUI swaps the host immediately, potentially
pointing at an empty CAContext.

**#6 — Old host removed too early** — `Metal.zig:220–223`. The old host is
removed in the same CATransaction as the new host addition. If the new context
is empty, removing the old host eliminates the only visible content.

**#7 — CATransaction flushes add+remove** — `Metal.zig:195–271`. The single
CATransaction wrapping the entire swap means Window Server sees removal and
addition atomically. No way for the old content to persist while the new context
initializes.

**#9 — No fallback content** — `Metal.zig:198–227`. No snapshot or backup layer.
When the host is swapped, if the new CAContext is empty, there is nothing to
show.

**#11 — No timestamp logging** — `Metal.zig:227`, `xpc.zig:414`,
`shell_tab_observer.cc:103`, `shell_browser_main_parts.cc:425`. No microsecond
timestamps anywhere in the pipeline. Cannot measure where the ~100ms gap occurs
— Chromium compositor, XPC transit, main-thread dispatch, or mutex wait.

**#12 — Dedup reset vs callback timing** — `shell_tab_observer.cc:95–96`,
`shell_browser_main_parts.cc:408–411`. No synchronization between dedup reset
and callback firing. The reset is unconditional; the callback timing is
compositor-driven.

**#13 — No same-site vs cross-site distinction** —
`shell_tab_observer.cc:85–96`, `shell_browser_main_parts.cc:408–411`,
`Metal.zig:198`. All navigations treated identically. Same-site navigations
where the CAContext survives unchanged trigger the same full host replacement as
cross-site navigations.

**#14 — Replacement vs contextId update** — `Metal.zig:199–202`. The code always
destroys and recreates the CALayerHost. Updating `contextId` on the existing
host was dismissed as unreliable (Issue 628) but was never tested after the
Issue 630 fixes. Could eliminate the swap entirely.

**#15 — Loading state not used for visibility** — `xpc.zig:469–482`.
`handleLoadingState()` forwards to TUI only. The GUI has real-time `"loading"` /
`"done"` signals but doesn't use them to freeze the old CALayerHost content
during navigation.

##### Suspect findings

**#2 — CAContext content gap** — `Metal.zig:208–212`,
`shell_browser_main_parts.cc:408`. During navigation, the compositor may briefly
produce a CAContext with no submitted frame. Even if the GUI doesn't swap the
host, the existing host may show nothing during this gap. Needs
Chromium-internal logging to confirm.

**#3 — Async dispatch latency** — `xpc.zig:436`. `dispatch_async_f` to main
queue introduces scheduling delay. If the main thread is busy, the swap is
delayed. Contributes to overall latency but may not be the primary cause.

**#8 — DidNavigate surface ID churn** — `shell_browser_main_parts.cc`
(indirect). `BrowserCompositorMac::DidNavigate()` invalidates and re-allocates
surface IDs. Between invalidation and first frame submission, the CAContext
content tree may be empty.

**#10 — Redundant swap from re-registration** — `shell_tab_observer.cc:58–78`,
`shell_browser_main_parts.cc:408`. `RenderViewHostChanged` re-registers the
callback. If this fires near `DidFinishNavigation`, two rapid sends of the same
or different ID may cause two GUI-side swaps, producing a double-flicker.

#### Analysis: most likely cause of the flicker

The primary cause is **smell #1: unnecessary CALayerHost swap on same ID**.

The chain:

1. User clicks a link → same-site navigation.
2. `DidFinishNavigation` resets `*last_ca_context_id_ = 0` (smell #1).
3. The compositor fires the CALayerParams callback with the same `ca_context_id`
   it had before.
4. The dedup gate passes (because `*last_id` was reset to 0).
5. Chromium sends the same ID via XPC.
6. GUI receives it, dispatches to main thread (smell #3 adds latency).
7. Main thread acquires `draw_mutex` (smell #4 may add ~16ms).
8. `setCALayerHostContextId` enters the replacement path (smell #13 — no
   same-site distinction).
9. New CALayerHost created with the same `contextId`, added to layer.
10. Old CALayerHost removed in same transaction (smells #6, #7).
11. Window Server sees: remove old host + add new host atomically.
12. The new host points at the same CAContext, but Window Server may need one
    frame cycle to composite the new host's content — producing the blank frame.
13. No fallback content exists during the gap (smell #9).

**The simplest fix**: skip the CALayerHost swap when the incoming `context_id`
equals the current host's `contextId`. This eliminates steps 8–13 entirely for
same-site navigation. For cross-site navigation where the ID genuinely changes,
the swap is still needed, but delaying old host removal (deferred transaction or
`dispatch_after`) would eliminate the content gap.
