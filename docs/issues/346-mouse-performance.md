# Issue 346: Mouse Performance

## Background

Issue 345 built `web benchmark` — an automated framerate benchmark that
simulates scrolling at ~125Hz directly in the profile server. The benchmark
isolated the rendering pipeline from the input path, producing a controlled
measurement.

Two runs revealed the problem:

| Condition              | FPS  | p50    | p95    | 60fps% | Streak |
| ---------------------- | ---- | ------ | ------ | ------ | ------ |
| No mouse movement      | 51.5 | 18.7ms | 33.9ms | 55.4%  | 25     |
| Continuous mouse moves | 39.0 | 17.4ms | 78.8ms | 38.3%  | 10     |
| cef-test reference     | ~50  | —      | —      | —      | —      |

The rendering pipeline is fine. Mouse move events cause the 12fps drop.

## The mouse event path

```
User moves mouse
    │
    ▼
macOS generates NSEvent (~125Hz+ hardware rate)
    │
    ▼
winit receives event in WezTerm's event loop
    │
    ▼
WezTerm converts to logical coordinates, routes through pane system
    │
    ▼
Serialized to XPC dictionary (action: "mouse_move", x, y, modifiers)
    │
    ▼
Sent over XPC to profile server
    │
    ▼
Profile server XPC handler (background thread) receives message
    │
    ▼
Creates MouseMoveTask, calls cef::post_task(UI thread)
    │
    ▼
CEF UI thread executes task: browser.lock() → host.send_mouse_move_event()
    │
    ▼
CEF processes mouse move internally, may trigger:
    - Cursor change → on_cursor_change → XPC message back to GUI
    - Hover effects → re-render → on_accelerated_paint
```

Every mouse move traverses this full pipeline. At ~125Hz hardware rate, that's
~125 XPC messages per second in each direction, plus ~125 `post_task` calls to
the CEF UI thread, plus any cursor change messages flowing back.

## What the data tells us

The p50 (median frame interval) is nearly identical between runs: 18.7ms vs
17.4ms. The rendering pipeline itself runs at the same speed regardless of mouse
input. The problem is entirely in the **tail latency** — the p95 doubles from
33.9ms to 78.8ms.

This pattern (identical median, exploding tail) means mouse events cause
**periodic stalls**, not constant overhead. Something about processing mouse
events occasionally blocks the rendering path for 50-80ms.

## Hypotheses

### H1: post_task contention on the CEF UI thread

Every mouse move creates a `MouseMoveTask` and posts it to the CEF UI thread via
`cef::post_task()`. The message loop calls `cef::do_message_loop_work()` which
processes both CEF's internal rendering tasks and our posted mouse tasks.

At ~125 mouse moves per second, that's ~125 extra tasks competing with CEF's
rendering pipeline on the same thread. If `do_message_loop_work()` processes
mouse tasks before rendering tasks, or if the task queue introduces scheduling
delays, frames get delayed.

The fact that the stalls are periodic (not constant) suggests task queue
contention rather than per-event overhead — rendering proceeds normally until a
burst of mouse tasks backs up the queue.

### H2: Mutex contention on browser state

`MouseMoveTask::execute()` locks `self.state.browser.lock().unwrap()` to get the
browser host. The render handler's `on_accelerated_paint` accesses the same
`BrowserState` (for width/height, focus state, and URL). While these locks are
brief, at 125Hz the probability of contention per frame is non-trivial.

If `on_accelerated_paint` has to wait for a mouse task to release the browser
lock (or vice versa), that wait shows up as a frame delay.

### H3: Cursor change round-trips

The profile log from the mouse-movement run shows frequent cursor type changes
(type 0 ↔ type 2) as the page scrolls under the mouse. Each cursor change fires
`on_cursor_change` in the display handler, which sends an XPC message back to
the GUI:

```rust
let msg = XpcDictionary::new();
msg.set_string("action", "cursor_change");
msg.set_i64("cursor_type", cursor_type);
self.inner.state.gui.send(&msg);
```

This creates a round-trip: mouse_move goes GUI→profile, cursor_change goes
profile→GUI. At scrolling speed with the mouse hovering over links, this could
fire dozens of times per second, doubling the XPC traffic.

### H4: Excessive mouse event rate

Modern mice poll at 125Hz–1000Hz. macOS may deliver events even faster when the
system is under low load. If WezTerm forwards every single event without
throttling, the profile server could be receiving far more than 125 events per
second — especially with high-polling-rate gaming mice.

We haven't measured the actual event rate reaching the profile server. The true
rate could be much higher than assumed.

## Ideas for fixing

### Throttle mouse moves in the GUI (most likely fix)

The GUI (`webview_xpc.rs`) currently forwards every mouse move event to the
profile server over XPC. Throttling to ~60Hz would:

- Cut XPC traffic by 50% or more
- Reduce post_task calls to the CEF UI thread by the same factor
- Have no visible effect on cursor accuracy (60 updates/sec is smooth)

Implementation: track `last_mouse_move_time` in the XPC manager. Only send a
mouse_move message if ≥16ms have elapsed since the last one. Always send the
latest position (not a queued one).

### Coalesce mouse moves in the profile server

If multiple `mouse_move` XPC messages arrive between `do_message_loop_work()`
calls, only process the latest one. Instead of posting a `MouseMoveTask` for
every message, store the latest position in an atomic and process it once per
loop iteration.

This is complementary to GUI-side throttling — even with throttling, network
jitter could deliver bursts.

### Throttle cursor change messages

Only send `cursor_change` XPC messages when the cursor type actually differs
from the last sent value. The current code sends on every `on_cursor_change`
callback, but CEF may fire the callback even when the type hasn't changed (e.g.,
during scroll repaints).

### Measure the actual event rate

Before fixing, instrument the profile server to count mouse_move events per
second. Add a counter and log it every second during benchmark mode. This tells
us the actual load and validates whether throttling would help.

```
[MOUSE-RATE] 127 mouse_move events in last second
```

### Move mouse processing out of post_task

Instead of posting `MouseMoveTask` to the CEF UI thread for every event, buffer
the latest mouse position in an atomic and apply it once per message loop
iteration. The message loop already runs on the CEF UI thread, so
`host.send_mouse_move_event()` can be called directly — just like the benchmark
does with scroll events.

This eliminates task queue overhead entirely for mouse moves.

## Experiments

### Experiment 1: Measure the actual mouse event rate

**Goal:** Determine how many `mouse_move` XPC messages per second actually reach
the profile server. We've been assuming ~125Hz based on standard Apple mouse
polling, but we haven't measured it. The true rate determines how much throttling
would help and which hypotheses are most likely.

**What to measure:**

- Events per second reaching the profile server's XPC handler
- Whether the rate is constant or bursty
- How the rate changes with different mouse movement speeds (slow vs fast)

**Implementation plan:**

1. Add a global `AtomicU64` counter (`MOUSE_MOVE_COUNT`) to the profile server
2. Increment it at the top of the `"mouse_move"` XPC handler (before any locking
   or `post_task` calls — we want to measure arrival rate, not processing rate)
3. In the message loop, log the count once per second:
   ```
   [MOUSE-RATE] 127 mouse_move events in last second
   ```
   Only log lines where the count is > 0 to avoid noise when the mouse is idle.
4. Track with two variables: `last_mouse_rate_time` and `last_mouse_rate_count`,
   computing the delta each second

**How to test:**

1. `web benchmark` with no mouse movement — expect 0 events logged
2. `web benchmark` with slow mouse movement — expect moderate rate
3. `web benchmark` with fast continuous mouse movement — expect peak rate

**What the results tell us:**

- If rate is ~125Hz: standard Apple mouse, throttling to 60Hz cuts traffic ~50%
- If rate is >> 125Hz: high-polling mouse or macOS event coalescing behavior,
  throttling is even more important
- If rate is << 125Hz: WezTerm or XPC is already dropping events, the problem is
  not volume but per-event cost
- If rate is bursty (varying widely second to second): supports H1 (task queue
  contention from bursts)

**Result:**

The rate is ~60Hz — far below the assumed ~125Hz hardware polling rate:

```
[MOUSE-RATE] 59-62 events/sec (steady state, continuous movement)
[MOUSE-RATE] 48-53 events/sec (occasional dips, likely brief pauses)
[MOUSE-RATE] 33 events/sec (ramp-up in first second)
```

Benchmark with continuous mouse movement: 40.4fps, p50=16.9ms, p95=50.0ms.

**Findings:**

1. **Something upstream already throttles to ~60Hz.** Either macOS, winit, or
   WezTerm's event loop caps mouse events before they reach the profile server.
   We are not dealing with a 125Hz+ firehose.

2. **The rate is very stable, not bursty.** Nearly every second is 59–62 events.
   This weakens H1 (task queue contention from bursts) — the events arrive at a
   steady cadence, not in bursts that would back up the queue.

3. **60 events/sec still causes a 12fps drop.** Even at this modest rate, we go
   from 51.5fps to 40.4fps — roughly 1fps lost per 5 mouse events/sec. The
   per-event cost is the problem, not the volume.

4. **GUI-side throttling alone won't fix this.** Halving from 60Hz to 30Hz would
   reduce load but not eliminate the drop, because the per-event cost is so high.

**Hypothesis impact:**

- H1 (post_task contention): Weakened — steady rate, not bursty
- H2 (mutex contention): Strengthened — per-event cost dominates
- H3 (cursor change round-trips): Strengthened — per-event overhead matters most
- H4 (excessive rate): **Ruled out** — rate is only ~60Hz

The investigation should now focus on per-event cost: mutex contention (H2),
cursor change round-trips (H3), and eliminating `post_task` overhead.

**Status:** Done
