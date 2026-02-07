# Issue 349: The Bimodal Pattern

## Background

Across Issues 346, 347, and 348, a persistent bimodal frame timing pattern has
emerged in ts3. The system randomly enters one of two modes per benchmark run:

- **Good mode:** p50 = 16.7ms, p95 = 33.3ms, ~50–55fps, 85%+ at 60fps
- **Bad mode:** p50 = 16.9–19.1ms, p95 = 66–83ms, ~33–38fps, <50% at 60fps

The modes are quantized to multiples of the 16.7ms vsync interval. The system
enters one mode or the other at the start of a run and tends to stay in it.
Which mode it enters appears random — the same code, same build, same machine
can produce either result on consecutive runs.

### What we've ruled out

| Hypothesis             | Issue | Result                                      |
| ---------------------- | ----- | ------------------------------------------- |
| Mouse input overhead   | 346   | No effect — variance identical with/without |
| Debug build overhead   | 347   | Release helps fps but doesn't fix bimodal   |
| Hot-path logging       | 346/8 | Removing logs doesn't fix bimodal           |
| 1ms message loop sleep | 348   | 0ms and 1ms both produce bimodal in ts3     |
| CEF OSR inherent limit | 347/8 | cef-test release is stable at ~51fps        |

### The critical clue

**cef-test release does NOT exhibit the bimodal pattern.** With 1ms sleeps,
cef-test release produced 50.3–51.6fps consistently across multiple runs, with a
stable p95 of 33.6ms. The bimodal pattern was explicitly noted as "gone" in
Issue 347 Experiment 1.

ts3, under identical conditions (release build, 1ms sleep, same machine), still
exhibits bimodal behavior. The difference must lie in something ts3 has that
cef-test doesn't.

### What ts3 has that cef-test doesn't

| Component               | cef-test            | ts3                                  |
| ----------------------- | ------------------- | ------------------------------------ |
| GUI event loop          | winit (simple)      | WezTerm (complex)                    |
| Terminal rendering      | None                | Active (pane rendering)              |
| Render scheduling       | Manual redraw       | WezTerm's frame scheduler            |
| XPC architecture        | Direct              | Via launcher relay                   |
| Number of panes         | 2 (left/right)      | Terminal + webview                   |
| wgpu surface management | Single window       | Per-window, multi-pane               |
| Process count           | 3 (gui, 2x profile) | 4+ (gui, launcher, profile, web CLI) |

## Lines of inquiry

### L1: WezTerm's render scheduling

WezTerm has its own frame scheduling logic for terminal rendering. It decides
when to redraw based on terminal output, cursor blink, animations, and
potentially vsync. If the webview frame arrives while WezTerm is in the middle
of its own render cycle, the webview frame may be delayed until the next
WezTerm-initiated render — adding up to one frame of latency.

In cef-test, the GUI calls `request_redraw()` immediately when a new IOSurface
arrives and renders on the next event loop iteration. WezTerm may batch or defer
redraws.

**Test:** Investigate how WezTerm schedules renders and whether webview
IOSurface arrivals trigger immediate redraws or wait for the next scheduled
frame.

### L2: Terminal pane rendering contention

ts3 renders both terminal panes and webview panes in the same wgpu render pass.
If the terminal pane has activity (cursor blink, output), it triggers renders on
its own schedule. The webview pane's IOSurface may arrive out of phase with the
terminal's render cycle, causing some frames to miss vsync.

In cef-test, the only thing driving renders is the incoming IOSurface — there's
no competing render source.

**Test:** Run the ts3 benchmark with a completely idle terminal (no cursor
blink, no output) and compare with an active terminal. If the bimodal pattern
disappears with an idle terminal, render scheduling contention is the cause.

### L3: XPC launcher relay latency

In cef-test, the profile server connects directly to the GUI via an XPC
endpoint. In ts3, the connection is established through the launcher as a relay
— the GUI sends its endpoint to the launcher, the profile server claims it. Once
established, the connection should be direct, but if there's any difference in
the XPC connection quality or routing, it could add variable latency.

**Test:** Verify that the ts3 XPC connection is truly direct after setup by
logging connection details. Compare XPC message delivery time between cef-test
and ts3.

### L4: WezTerm event loop interaction with vsync

WezTerm's event loop may have a different relationship with vsync than winit's
`pump_app_events`. If WezTerm's loop runs on a timer or is driven by a display
link, the phase relationship between CEF's frame production and WezTerm's
presentation could create a beat frequency — sometimes in phase (good mode),
sometimes out of phase (bad mode).

This would explain why the bimodal pattern is stable within a run but random
between runs: the phase relationship is set at startup and persists.

**Test:** Investigate WezTerm's main loop timing. Does it use a display link? A
timer? How does it decide when to present frames?

### L5: Process startup timing

The bimodal pattern may be determined by the relative startup timing of the
profile server and GUI. If the profile server's first frame happens to align
with the GUI's vsync cycle, subsequent frames stay aligned (good mode). If it
misaligns, frames consistently miss (bad mode).

This would explain the randomness between runs — process startup timing varies
by microseconds, which determines which vsync phase the pipeline locks into.

**Test:** Add timestamps to the first few frames in both the profile server and
GUI to see if good-mode runs have different phase alignment than bad-mode runs.

## Recommended experiment order

1. **L1 + L4:** Investigate WezTerm's render scheduling and event loop (code
   reading, no changes needed — highest information value)
2. **L2:** Test with idle terminal (quick behavioral test)
3. **L5:** Timestamp first frames to check phase alignment (diagnostic)
4. **L3:** Verify XPC connection is direct (quick check)
