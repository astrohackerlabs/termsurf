+++
status = "closed"
opened = "2026-02-23"
closed = "2026-02-23"
+++

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

### Experiment 2: Skip CALayerHost swap when context ID is unchanged

#### Purpose

Address the primary finding from Experiment 1 — smell #1: unnecessary
CALayerHost swap on same ID. When the GUI receives a `ca_context_id` via XPC
that matches the existing CALayerHost's `contextId`, it currently destroys the
old host and creates a new one pointing at the exact same CAContext. This
produces a blank frame for no reason.

The fix: read the existing host's `contextId` property and skip the replacement
when the IDs match. This is a GUI-only change — no Chromium modifications.

#### Hypothesis

Same-site navigations (clicking a link on the same origin) keep the same
`CALayerTreeCoordinator` and the same `ca_context_id`. The dedup reset (C3 from
Issue 630) forces Chromium to re-send the ID, but the ID hasn't changed. By
skipping the swap, the CALayerHost stays in the layer tree undisturbed and
Window Server continues compositing the remote content without interruption.

If this hypothesis is correct, same-site navigations will have zero flicker.
Cross-site navigations (where the `RenderViewHost` changes and a new CAContext
is created) will still flicker because the ID genuinely changes and a swap is
needed — but that is a separate problem for a future experiment.

#### Changes

**`gui/src/renderer/Metal.zig` — `setCALayerHostContextId()`**

In the existing-host branch (line 198), before the swap logic, add a contextId
comparison:

```zig
if (ca_layer_host_ptr.*) |existing| {
    // Skip swap when context ID hasn't changed (Issue 631, smell #1).
    // Same-site navigation keeps the same CAContext — the dedup reset
    // (C3) re-sends the same ID, but no swap is needed.
    const existing_obj = objc.Object.fromId(existing);
    const current_id = existing_obj.getProperty(u32, "contextId");
    if (current_id == context_id) {
        log.info("CALayerHost contextId={} unchanged, skipping swap", .{context_id});
        CATx.msgSend(void, objc.sel("commit"), .{});
        return;
    }

    // Replace existing CALayerHost with a new one (Issue 628).
    // ... existing swap logic unchanged ...
}
```

This is the only code change. No Chromium modifications. The Chromium side still
resets the dedup gate and re-sends the ID — but the GUI now ignores it when the
ID hasn't changed.

#### Verification

1. Build TermSurf: `cd gui && zig build`
2. Launch: `open gui/zig-out/TermSurf.app`
3. Open a web page:
   `cargo run -p web -- https://en.wikipedia.org/wiki/Main_Page`
4. **Same-site navigation test**: Click any link on Wikipedia (stays on
   `en.wikipedia.org`). Observe whether the overlay flickers.
   - **Pass**: No visible flicker. Log shows "contextId=N unchanged, skipping
     swap."
   - **Fail**: Flicker persists, or the overlay breaks (goes blank permanently,
     wrong content, etc.).
5. **Cross-site navigation test**: Navigate to a different origin (e.g., type a
   new URL in the address bar, or click a link to an external site). Observe
   that the overlay still works after the cross-site navigation.
   - **Pass**: Overlay appears with correct content after a brief transition.
   - **Fail**: Overlay goes blank or breaks after cross-site navigation.
6. Check the TermSurf log for "skipping swap" messages during same-site
   navigation and "replaced CALayerHost" messages during cross-site navigation.

**Result:** Fail

The exact same ~100ms flicker persists on every navigation. Skipping the
CALayerHost swap when the context ID is unchanged had no effect on the flicker.

#### Conclusion

The skip path never fired. Chromium server logs (`logs/chromium-server.log`)
show that every navigation produces a new `ca_context_id`:

```
Sent ca_context_id=912947617 ...
Sent ca_context_id=927833480 ...
Sent ca_context_id=3761875988 ...
```

Three navigations, three different IDs. The `CALayerTreeCoordinator` is
recreated on every navigation, even same-site. Because the ID always changes,
the early-return condition (`current_id == context_id`) was never true, and the
experiment tested nothing — the GUI still did a full host swap every time.

This eliminates smell #1 as a factor. The ID genuinely changes, so the
CALayerHost swap is necessary. The flicker comes from the gap between: (a) the
old host being removed and (b) the new host's CAContext having content to
display. The new CAContext exists but its content tree is empty until the
Chromium compositor submits the first frame on the new surface.

Relevant smells for the next experiment: #2 (CAContext content gap during
surface transition), #6 (old host removed before new host has content), #9 (no
fallback content during transition).

Code changes reverted.

### Experiment 3: Delay old CALayerHost removal

#### Purpose

The `ca_context_id` changes on every navigation (proven by Experiment 2 logs),
so a CALayerHost swap is unavoidable. Currently the old host is removed in the
same CATransaction as the new host addition (smell #6/#7). If the new CAContext
has no content yet, the user sees a blank frame.

The fix: keep the old CALayerHost behind the new one for 200ms. If the old
CAContext still has content during the transition, the old page shows through
the transparent new host until the new page's first frame arrives.

#### Hypothesis

The old `CALayerTreeCoordinator` (and its CAContext) is not destroyed instantly
when navigation creates a new one. There is a brief overlap where the old
CAContext still has the previous page's content. By keeping the old CALayerHost
in the layer tree behind the new one, the old content remains visible during the
gap, masking the flicker.

If the old CAContext is destroyed before the new one has content, the old host
also goes blank, and this experiment fails — the blank is just delayed, not
eliminated.

#### Changes

**`gui/src/renderer/Metal.zig` — `setCALayerHostContextId()`**

In the existing-host replacement branch, change the swap to keep the old host
for 200ms:

1. Add the new host to the positioning layer (same as now).
2. Do NOT remove the old host in the same transaction — leave it behind the new
   host.
3. Schedule a delayed removal of the old host via `dispatch_after` (200ms).

```zig
if (ca_layer_host_ptr.*) |existing| {
    // ... create new_host, set contextId, anchorPoint, autoresizingMask ...

    // Add new host to positioning_layer (on top of old host).
    if (ca_layer_positioning_ptr.*) |pos_ptr| {
        const pos = objc.Object.fromId(pos_ptr);
        pos.msgSend(void, objc.sel("addSublayer:"), .{new_host.value});
    }

    // Update pointer to new host immediately.
    ca_layer_host_ptr.* = new_host.value;

    // Schedule delayed removal of old host (200ms).
    // The old host's CAContext may still have content from the previous page,
    // visible behind the transparent new host until the new page renders.
    const Old = struct {
        host: objc.Object,
        fn remove(raw: ?*anyopaque) callconv(.c) void {
            const self: *@This() = @ptrCast(@alignCast(raw));
            const CATx2 = objc.getClass("CATransaction").?;
            CATx2.msgSend(void, objc.sel("begin"), .{});
            CATx2.msgSend(void, objc.sel("setDisableActions:"), .{true});
            self.host.msgSend(void, objc.sel("removeFromSuperlayer"), .{});
            self.host.release();
            CATx2.msgSend(void, objc.sel("commit"), .{});
            std.heap.c_allocator.destroy(self);
        }
    };
    const old_ctx = std.heap.c_allocator.create(Old) catch return;
    old_ctx.* = .{ .host = objc.Object.fromId(existing) };
    const delay = dispatch_time(DISPATCH_TIME_NOW, 200 * std.time.ns_per_ms);
    dispatch_after(delay, &_dispatch_main_q, old_ctx, &Old.remove);

    log.info("replaced CALayerHost contextId={}, old host removal delayed 200ms", .{context_id});
}
```

This requires adding `dispatch_after` and `dispatch_time` to the extern
declarations (they are standard GCD functions from `<dispatch/dispatch.h>`,
already available via Zig's C import).

#### Verification

1. Build TermSurf: `cd gui && zig build`
2. Launch: `open gui/zig-out/TermSurf.app`
3. Open a web page:
   `cargo run -p web -- https://en.wikipedia.org/wiki/Main_Page`
4. Click any Wikipedia link and observe the transition.
   - **Pass**: No visible blank frame. The old page content remains visible
     until the new page appears.
   - **Partial**: The old page lingers for ~200ms then there is a brief blank
     before the new page. This means the old CAContext is destroyed quickly but
     the new one takes longer than 200ms to have content.
   - **Fail**: Same flicker as before. The old CAContext is destroyed instantly
     when navigation begins, so the old host also goes blank immediately.
5. Test multiple navigations in sequence to confirm no layer tree corruption
   (leaked hosts, double-free, etc.).

**Result:** Fail

The flicker persists. The old page content was not visible during the transition
despite the old CALayerHost remaining in the layer tree.

Note: the initial implementation used `dispatch_after` (block-based) instead of
`dispatch_after_f` (function-pointer-based), which crashed the app. After fixing
to `dispatch_after_f`, the delayed removal worked mechanically but had no effect
on the flicker.

#### Conclusion

The approach is fundamentally flawed. Chromium destroys the old
`CALayerTreeCoordinator` (and its `CAContext`) when navigation creates the new
one. The old CALayerHost remains in the layer tree but points at a dead context
— it has nothing to display. Keeping it around longer cannot help because the
content it referenced is already gone.

This confirms smell #2: the content gap is Chromium-side. The old CAContext's
content tree is torn down before the new CAContext has content. No GUI-side
trick — skipping the swap (Experiment 2) or delaying old host removal
(Experiment 3) — can mask this gap because the source content is destroyed by
Chromium itself.

The fix must be Chromium-side. Either:

1. **Prevent Chromium from destroying the old CAContext before the new one has
   content** — delay the old coordinator's teardown until the first frame is
   submitted on the new surface.
2. **Capture a snapshot before navigation** — render the current page to a
   static image and hold it as a fallback layer while the new CAContext
   initializes. This would need to happen Chromium-side (e.g., via
   `CopyFromCompositingSurface`) before navigation commits.

Code changes reverted.

### Experiment 4: Research Chromium/Electron CALayerHost navigation transitions

#### Purpose

Experiments 2 and 3 proved the flicker is Chromium-side: the old CAContext is
destroyed before the new one has content, and no GUI-side fix can mask this. We
need to understand how Chromium and Electron handle this same problem.

This is a research-only experiment — no code modifications.

#### Questions to answer

1. **What destroys the old CAContext during navigation?** Trace the lifecycle:
   which class owns the `CALayerTreeCoordinator`, when is it torn down during
   navigation, and what triggers the teardown? Key classes to investigate:
   `BrowserCompositorMac`, `DelegatedFrameHost`, `CALayerTreeCoordinator`,
   `DisplayCALayerTree`.

2. **Does Chromium have a content transition mechanism?** When switching tabs,
   Chrome shows the old tab's content until the new tab renders. Does a similar
   mechanism exist for navigation within a tab? Look for snapshot/capture logic
   tied to navigation in `RenderWidgetHostViewMac` or `BrowserCompositorMac`.

3. **How does Electron handle this?** Electron uses the same out-of-process
   renderer with CALayerHost. Search the Electron source and patches for
   navigation transition handling, especially around `BrowserCompositorMac`,
   `DelegatedFrameHost`, or `RenderWidgetHostViewMac`.

4. **What is `ui::Compositor`'s role?** The `ui::Compositor` may hold a
   reference to the old surface's content. Does it have a "keep old content
   until new content arrives" pattern?

5. **Is there a "first frame after navigation" signal?** Something like
   `DidFirstVisuallyNonEmptyPaint` or `RenderFrameHost::DidCommitNavigation`
   that we could use to delay the old CAContext teardown.

#### Files to search

**Chromium (`chromium/src/`):**

- `ui/compositor/compositor.h` / `.cc` — compositor lifecycle
- `content/browser/renderer_host/render_widget_host_view_mac.mm` — macOS view,
  CALayerHost usage
- `content/browser/renderer_host/browser_compositor_view_mac.mm` / `.h` —
  `BrowserCompositorMac`, `DidNavigate()`, surface management
- `content/browser/renderer_host/delegated_frame_host.cc` / `.h` —
  `DelegatedFrameHost`, surface ID management, `DidNavigate()`
- `ui/accelerated_widget_mac/ca_layer_tree_coordinator.h` / `.cc` —
  `CALayerTreeCoordinator`, `CAContext` creation/destruction
- `ui/accelerated_widget_mac/display_ca_layer_tree.mm` —
  `DisplayCALayerTree::GotCALayerFrame()`, CALayerHost creation

**Electron (`vendor/electron/`):**

- Search for patches touching `browser_compositor_view_mac`,
  `render_widget_host_view_mac`, `delegated_frame_host`, or
  `ca_layer_tree_coordinator`
- Search for navigation transition or content snapshot logic

#### Steps

1. Read the Chromium files listed above, focusing on the navigation path and
   CAContext lifecycle.
2. Search Electron's patches for modifications to the same files.
3. Answer each of the 5 questions above with file paths and line numbers.
4. Propose concrete Chromium-side fixes based on findings.

#### Verification

All 5 questions have answers with specific file paths and line numbers from the
local source. At least one concrete fix is proposed with enough detail to design
as the next experiment.

#### Findings

##### Q1: What destroys the old CAContext during navigation?

**The old CAContext is NOT destroyed during normal navigation.** The
`CALayerTreeCoordinator` owns the `ca_context_` member variable
(`ca_layer_tree_coordinator.h:122`), and it lives for the entire lifetime of the
coordinator. The coordinator is created once per GPU compositor
(`ca_layer_tree_coordinator.mm:56`) and sends the same `ca_context_id` in every
`gfx::CALayerParams` frame (`ca_layer_tree_coordinator.mm:211`).

The `ca_context_id` only changes when the `CALayerTreeCoordinator` itself is
destroyed and a new one is created. This happens when:

- **Cross-site navigation** causes a renderer process swap — new renderer, new
  GPU compositor, new coordinator, new ID.
- **Compositor recycling** — when a view is hidden/occluded, Chromium may
  recycle the compositor. When the view becomes visible again, a new compositor
  is created with a new coordinator.

**Our Profile Server logs showed a new ID on every navigation** — even for
same-site Wikipedia link clicks that should keep the same renderer. This
strongly suggests compositor recycling is being triggered. Our hidden window
(`setAlphaValue:0` + `orderWindow:NSWindowBelow`, fix C1 from Issue 630) may
cause macOS to treat the view as occluded, triggering Chromium's compositor
recycling.

Key files:

- `ui/accelerated_widget_mac/ca_layer_tree_coordinator.mm:40–67` — CAContext
  creation
- `ui/accelerated_widget_mac/ca_layer_tree_coordinator.mm:206–228` — sends
  `ca_context_id` in params
- `ui/accelerated_widget_mac/display_ca_layer_tree.mm:123–153` — remote layer
  swap on ID change

##### Q2: Does Chromium have a content transition mechanism?

**Yes, extensively.** Chromium uses a fallback surface system in
`DelegatedFrameHost` to display old content while new content renders:

- **Pre-navigation caching**: `DidNavigateMainFramePreCommit()`
  (`delegated_frame_host.cc:586`) caches the current `local_surface_id_` as
  `pre_navigation_local_surface_id_` before navigation commits.

- **Fallback surface range**: The viz compositor uses `SurfaceRange` (old
  fallback surface → new surface). If the primary surface isn't ready, viz
  displays the fallback. Managed via `SetOldestAcceptableFallback()`
  (`delegated_frame_host.cc:421–425`).

- **Stale content layer**: When a frame is evicted, `DelegatedFrameHost` can
  capture a snapshot via `CopyFromCompositingSurface` and store it as a
  `stale_content_layer_` (`delegated_frame_host.cc:440–512`). This persists even
  after the frame is evicted.

- **Tab switching fallback**: `BrowserCompositorMac::TakeFallbackContentFrom()`
  (`browser_compositor_view_mac.mm:282`) captures the old tab's surface as
  fallback for the new tab.

**None of this helps us** because our Chromium Profile Server reads the
`CALayerParams` callback directly and creates our own `CALayerHost`. We bypass
the `DisplayCALayerTree` → `DelegatedFrameHost` → viz fallback stack entirely.

Key files:

- `content/browser/renderer_host/delegated_frame_host.cc:586–598` — pre-nav
  caching
- `content/browser/renderer_host/delegated_frame_host.cc:399–426` — fallback
  reset
- `content/browser/renderer_host/delegated_frame_host.cc:440–512` — stale
  content layer

##### Q3: How does Electron handle this?

Electron's key insight: **they disable compositor recycling.**

`disable_compositor_recycling.patch` modifies `render_widget_host_view_mac.mm`
to prevent the compositor from being destroyed when the view is hidden:

```cpp
// Consider the RWHV occluded only if it is not attached to a window
// (e.g. unattached BrowserView). Otherwise we treat it as visible to
// prevent unnecessary compositor recycling.
const bool unattached = ![GetInProcessNSView() window];
browser_compositor_->SetRenderWidgetHostIsHidden(unattached);
```

Instead of always marking the view as hidden when `WasHidden()` is called, they
only mark it hidden if it's truly unattached from a window. This prevents the
compositor from being recycled during state transitions, which would create a
visual gap (exactly our problem).

Other relevant Electron patches:

- **MAS build**: Disables `CAContext` entirely for Mac App Store builds, falling
  back to IOSurface-based rendering (`mas_avoid_private_macos_api_usage.patch`).
- **Resize performance**: Restores original `SynchronizeVisualProperties()` to
  avoid blocking during screen changes.
- **Occlusion handling**: Reverts stale occlusion code to prevent spurious
  notifications during fullscreen transitions.

Key file:

- `vendor/electron/patches/chromium/disable_compositor_recycling.patch`

##### Q4: What is `ui::Compositor`'s role?

`ui::Compositor` is the frame sink manager and compositor host. It does **not**
hold surface content directly. Instead:

- Owns the `LayerTreeHost` which communicates with viz
- Manages child frame sinks (`AddChildFrameSink`, `RemoveChildFrameSink`)
- In `BrowserCompositorMac`, wrapped in `RecyclableCompositorMac` (line 186)
- Attached/detached from `DelegatedFrameHost` in `TransitionToState()`
  (`browser_compositor_view_mac.mm:208–269`)

The "keep old content" pattern lives in viz (via `SurfaceRange` and
`GetOldestAcceptableFallback()` in `ui/compositor/layer.h:470`), not in
`ui::Compositor` itself.

##### Q5: Is there a "first frame after navigation" signal?

**Yes, multiple:**

- **`DelegatedFrameHost::DidNavigate()`** (`delegated_frame_host.cc:582`) —
  called after the new renderer's first frame. Sets
  `first_local_surface_id_after_navigation_`.

- **`OnFirstSurfaceActivation`** (`delegated_frame_host.cc:555`) — viz callback
  when a new surface from the frame sink is activated.

- **`DidNavigateMainFramePreCommit()`** (`delegated_frame_host.cc:586`) — called
  BEFORE the new renderer takes over. Caches old surface for fallback.

- **Timeout**: `ForceFirstFrameAfterNavigationTimeout()`
  (`render_widget_host_view_mac.mm:687`) — clears fallback surfaces if the new
  page doesn't send a frame within a timeout.

#### Analysis

The root cause is now clear: **compositor recycling.**

Our Chromium Profile Server uses a hidden window (`setAlphaValue:0` +
`orderWindow:NSWindowBelow`). When navigation happens, Chromium detects the view
as occluded and recycles the compositor. This destroys the
`CALayerTreeCoordinator` and its `CAContext`. When the new page starts
rendering, a new compositor is created with a new `CALayerTreeCoordinator` and a
new `ca_context_id`. During the gap between destruction and recreation, there is
no CAContext content to display.

In normal Chrome, same-site navigation keeps the same compositor and the same
`ca_context_id`. The `DisplayCALayerTree::GotCALayerFrame()` method has an
early-out when the context ID hasn't changed — no `CALayerHost` swap occurs, so
there's no flicker.

Electron solves this exact problem by disabling compositor recycling: they never
mark the view as hidden while it's attached to a window, so the compositor stays
alive during navigation.

#### Proposed fixes

**Fix A: Disable compositor recycling (Electron approach).** Modify the Chromium
Profile Server to never treat the view as hidden/occluded, preventing compositor
recycling. The `ca_context_id` would stay the same across same-site navigations,
and the GUI's existing dedup logic would skip the swap. This is the simplest fix
and addresses the root cause.

**Fix B: Keep old CAContext alive during transition.** Delay the old
`CALayerTreeCoordinator`'s destruction until the new one has submitted its first
frame. Requires hooking into the `DelegatedFrameHost` lifecycle signals
(`DidNavigate`, `OnFirstSurfaceActivation`).

**Fix C: Snapshot before navigation.** Use `CopyFromCompositingSurface()` to
capture a static image before navigation commits. Send it to the GUI as a
fallback texture. More complex, but works even for cross-site navigations where
the compositor must change.

Fix A is the recommended next experiment — it's a small change, matches
Electron's proven approach, and addresses the root cause rather than masking the
symptom.

**Result:** Pass

All 5 questions answered with file paths and line numbers. Three concrete fixes
proposed.

#### Conclusion

The flicker's root cause is compositor recycling. Our hidden window causes
Chromium to recycle the compositor on navigation, destroying the
`CALayerTreeCoordinator` and its `CAContext`. This creates a new `ca_context_id`
on every navigation — even same-site — forcing a `CALayerHost` swap in the GUI
with an empty content gap.

Electron solved this years ago by disabling compositor recycling when the view
is attached to a window. The next experiment should apply the same approach to
our Chromium Profile Server.

### Experiment 5: Disable compositor recycling (Electron patch)

#### Purpose

Apply Electron's proven fix for compositor recycling. When a
`RenderWidgetHostViewMac` receives `WasOccluded()`, it currently calls
`SetRenderWidgetHostIsHidden(true)` unconditionally, which triggers
`TransitionToState(HasNoCompositor)` and destroys the compositor. The Electron
patch makes this conditional: only mark hidden if the view's NSView is truly
unattached from any window.

Our Profile Server's window uses `setAlphaValue:0` + `orderWindow:NSWindowBelow`
(Issue 630, fix C1), which keeps the window in the window list but places it
behind everything. macOS may still report this as occluded, triggering
`WasOccluded()` and compositor recycling. The Electron patch would prevent
recycling because the view IS attached to a window — it's just transparent and
behind other windows.

#### Hypothesis

The `ca_context_id` changes on every navigation because the compositor is being
recycled. If we prevent recycling, the same `CALayerTreeCoordinator` and
`ca_context_id` persist across navigations. The GUI receives the same ID, the
dedup gate in the Chromium callback filters it out, and no `CALayerHost` swap
occurs. Zero flicker.

#### Changes

**New Chromium branch:** `146.0.7650.0-issue-631` forked from
`146.0.7650.0-issue-630`.

**`content/browser/renderer_host/render_widget_host_view_mac.mm` —
`WasOccluded()`**

Apply the Electron patch. Change `WasOccluded()` (around line 567) from:

```cpp
void RenderWidgetHostViewMac::WasOccluded() {
  if (host()->IsHidden()) {
    return;
  }

  host()->WasHidden();
  browser_compositor_->SetRenderWidgetHostIsHidden(true);
  // ...
}
```

To:

```cpp
void RenderWidgetHostViewMac::WasOccluded() {
  if (host()->IsHidden()) {
    return;
  }

  host()->WasHidden();
  // Only mark the compositor hidden if the view is truly unattached from a
  // window (Issue 631, Experiment 5). When the view has a window (even a
  // transparent one), keep the compositor alive to prevent recycling.
  // This matches Electron's disable_compositor_recycling.patch.
  const bool unattached = ![GetInProcessNSView() window];
  browser_compositor_->SetRenderWidgetHostIsHidden(unattached);
  // ...
}
```

This is the only code change. No GUI modifications.

#### Verification

1. Create Chromium branch and apply the change.
2. Build: `autoninja -C out/Default chromium_profile_server`
3. Launch TermSurf and open a web page.
4. Navigate to another page on the same site (e.g., click a Wikipedia link).
5. Check `logs/chromium-server.log` for `ca_context_id` values.
   - **Pass criterion 1**: Same-site navigations produce the **same**
     `ca_context_id` (compositor not recycled).
   - **Pass criterion 2**: No visible flicker during same-site navigation.
6. Navigate to a different origin (cross-site).
   - The ID may change (renderer swap creates a new compositor). Brief flicker
     is acceptable for cross-site navigation — that's a separate problem.

**Result:** Fail

Two variants were tested:

1. **Electron's original patch** (only skip `SetRenderWidgetHostIsHidden`, still
   call `host()->WasHidden()`): back navigation produced a white screen. The
   renderer was told it was hidden but the compositor wasn't — the show path
   never called `WasShown()` because the compositor appeared visible. Renderer
   never restarted producing frames.

2. **Skip `WasOccluded()` entirely** when the view has a window: same white
   screen on back navigation. The occlusion path is not the only thing
   controlling renderer frame production during navigation.

#### Conclusion

The compositor recycling hypothesis from Experiment 4 was wrong — or at least
incomplete. Preventing occlusion-triggered recycling does not keep the
`ca_context_id` stable across navigations, and both variants introduced a
regression (white screen on back navigation).

The `ca_context_id` likely changes on every navigation because of
**renderer/RenderViewHost swaps**, not window occlusion. Modern Chromium with
RenderDocument and BackForwardCache creates new `RenderFrameHost` instances even
for same-site navigations. Each new `RenderViewHost` has its own compositor and
`CALayerTreeCoordinator`, producing a new `ca_context_id`. This is a
renderer-level architectural behavior that window visibility cannot influence.

This means the flicker cannot be prevented by keeping the compositor alive — the
compositor is replaced as part of normal navigation, not as an artifact of our
hidden window. The fix must either:

1. Work within the constraint that `ca_context_id` changes on every navigation
   (e.g., snapshot the old content before navigation).
2. Prevent the renderer swap that causes the new compositor (e.g., disable
   RenderDocument or BackForwardCache for our use case).
3. Use Chromium's existing fallback surface mechanism (`DelegatedFrameHost`'s
   stale content layer or pre-navigation surface caching) to bridge the gap.

Code changes reverted. Chromium branch `146.0.7650.0-issue-631` contains the
revert commit.

## Conclusion

Five experiments. Zero fixes. The ~100ms navigation flicker remains unsolved.

### What we tried

1. **Skip CALayerHost swap when context ID unchanged** (Experiment 2, GUI-side).
   The ID changes on every navigation, so the skip never triggered.

2. **Delay old CALayerHost removal by 200ms** (Experiment 3, GUI-side). The old
   CAContext's content is already destroyed by the time we try to show it.
   Keeping the old host around longer just keeps a pointer to a dead context.

3. **Disable compositor recycling** (Experiment 5, Chromium-side, Electron's
   patch). Caused white screen on back navigation. The `ca_context_id` changes
   because of renderer swaps during navigation, not because of
   occlusion-triggered compositor recycling.

### What we learned

- **The `ca_context_id` changes on every navigation.** Three Wikipedia
  navigations produced three different IDs (Experiment 2 logs). This is not a
  bug — it's how Chromium works with RenderDocument and BackForwardCache.

- **The old CAContext is destroyed before the new one has content.** The old
  `CALayerTreeCoordinator` and its `CAContext` are torn down when the new
  renderer takes over. The new CAContext exists but has no submitted frame yet.
  No GUI-side trick can bridge this gap because there is no content to display
  on either side during the transition.

- **Chromium has fallback mechanisms we don't use.** `DelegatedFrameHost` caches
  pre-navigation surfaces, maintains stale content layers via
  `CopyFromCompositingSurface`, and manages fallback surface ranges through viz.
  Our `CALayerParams` callback bypasses this entire stack — we read the raw
  `ca_context_id` and create our own `CALayerHost`, so none of the built-in
  transition machinery applies.

- **Electron's compositor recycling patch doesn't apply.** Our hidden window
  (`setAlphaValue:0`) was suspected of triggering recycling, but the real cause
  of new IDs is renderer-level: new `RenderViewHost` → new compositor → new
  `CALayerTreeCoordinator` → new `ca_context_id`.

### Ideas for next steps

**Chromium-side approaches:**

- **Disable RenderDocument or BackForwardCache.** If same-site navigations keep
  the same `RenderViewHost`, the compositor and `ca_context_id` would persist.
  Trade-off: loses Chromium's navigation performance optimizations.

- **Hook into `DelegatedFrameHost`'s fallback mechanism.** Instead of reading
  `CALayerParams` directly, use the `DelegatedFrameHost`'s stale content layer
  or pre-navigation surface caching. The infrastructure exists — we just need to
  tap into it from our Profile Server.

- **Snapshot before navigation.** Call `CopyFromCompositingSurface()` before
  navigation commits and send the snapshot to the GUI as a static fallback
  texture. The GUI displays the snapshot while the new CAContext initializes.

**GUI-side approaches:**

- **Freeze the last good frame.** Before the `ca_context_id` changes, capture
  the current CALayerHost's rendered content (via `CALayer.render(in:)` or
  similar) and place it on a static `CALayer` behind the host. When the host
  swaps and the new CAContext is empty, the frozen frame shows through.

- **Hide the overlay during transition.** Accept the flicker but make it less
  jarring — fade to a loading state or show a placeholder instead of flashing
  the terminal background.

**Research approaches:**

- **Trace the exact Chromium navigation lifecycle.** Add microsecond-precision
  logging at every stage: `DidStartNavigation`, `RenderViewHostChanged`,
  `DidFinishNavigation`, CALayerParams callback, first frame on new surface.
  Determine the exact duration of the content gap.

- **Study how Chrome handles tab restore.** When restoring a tab from
  BackForwardCache, Chrome must face the same "new compositor, no content yet"
  gap. How does it avoid flicker? The answer may be in the `DelegatedFrameHost`
  stale content layer path.
