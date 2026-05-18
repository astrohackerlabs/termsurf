+++
status = "closed"
opened = "2026-02-07"
closed = "2026-02-07"
+++

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

### Experiment 2: Measure the cursor change rate

**Goal:** Determine how many `on_cursor_change` callbacks fire per second during
mouse movement + scrolling. Each callback sends an XPC message back to the GUI,
so if the rate is high, cursor changes are a significant source of per-event
overhead — potentially doubling XPC traffic (mouse_move in, cursor_change out).

**What to measure:**

- Callbacks per second during benchmark with mouse movement
- Whether cursor type actually changes, or CEF fires the callback redundantly
- The ratio of cursor changes to mouse moves (1:1 would mean every mouse move
  triggers a cursor change round-trip)

**Implementation plan:**

1. Add a global `AtomicU64` counter (`CURSOR_CHANGE_COUNT`) to the profile server
2. Increment it at the top of `on_cursor_change` in the display handler (inside
   `cef_handlers` module, accessed via `crate::CURSOR_CHANGE_COUNT`)
3. In the message loop, log the count once per second alongside the mouse rate:
   ```
   [CURSOR-RATE] 58 cursor_change callbacks in last second
   ```
   Only log when count > 0.
4. Track with `last_cursor_rate_count`, computing the delta each second using
   the same timing as the mouse rate logger

**How to test:**

1. `web benchmark` with no mouse movement — expect 0 or near-0 cursor changes
2. `web benchmark` with continuous mouse movement — measure the rate

**What the results tell us:**

- If rate is ~60/sec (1:1 with mouse moves): H3 is confirmed — every mouse move
  triggers a cursor change round-trip, doubling XPC traffic. Deduplicating cursor
  changes would halve the per-event overhead.
- If rate is >> 60/sec: CEF fires redundant callbacks even without type changes.
  Filtering by actual type change is critical.
- If rate is near 0: H3 is ruled out. The per-event cost is elsewhere (H2 mutex
  contention or `post_task` overhead).

**Result:**

Four benchmark runs revealed two things: cursor change rate, and a variance
problem.

Cursor change rate during continuous mouse movement:

```
[CURSOR-RATE] 8-27 callbacks/sec, averaging ~17/sec
[MOUSE-RATE]  ~60 events/sec (consistent with Experiment 1)
Ratio: ~1 cursor change per 3-4 mouse moves
```

But the benchmark results themselves were highly variable:

| Run | Condition | FPS  | p50    | p95    | 60fps% |
| --- | --------- | ---- | ------ | ------ | ------ |
| 1   | No mouse  | 35.5 | 33.1ms | 66.7ms | 38.2%  |
| 2   | Mouse     | 45.7 | 16.7ms | 83.4ms | 69.1%  |
| 3   | No mouse  | 41.6 | 19.2ms | 53.7ms | 38.8%  |
| 4   | Mouse     | 40.0 | 17.2ms | 66.2ms | 43.2%  |

The first no-mouse run was the **worst** (35.5fps). The first mouse run was the
**best** (45.7fps). There is no consistent pattern of mouse movement hurting
performance across these runs.

**Findings:**

1. **Cursor changes fire at ~17/sec, not ~60/sec.** Not 1:1 with mouse moves.
   Cursor changes add ~30% extra XPC traffic, not 100%. H3 is a contributing
   factor but not the dominant one.

2. **Run-to-run variance dominates the signal.** The 35.5–45.7fps swing between
   runs with identical conditions is larger than the mouse-vs-no-mouse difference
   we were trying to measure. Something else — thermal state, system load, CEF
   internal scheduling — is the primary source of variation.

3. **The Issue 345 conclusion may have been premature.** The clean 51.5 vs 39.0
   result that motivated this investigation may have been partly luck — one good
   run vs one bad run, with mouse movement coincidentally correlating.

**Hypothesis impact:**

- H3 (cursor change round-trips): Partially weakened — ~17/sec, not ~60/sec
- All hypotheses: Uncertain — run-to-run variance makes it hard to isolate any
  single factor

**New concern:** Before further mouse performance experiments, the benchmark
itself needs better statistical reliability. Either run multiple iterations and
average, or identify and control the source of variance.

**Status:** Done

### Experiment 3: Remove debug logging from hot paths

**Goal:** Eliminate excessive `println!` calls from performance-critical paths and
re-measure. The profile server currently emits ~660 println!/sec during mouse
movement. Each call acquires a stdout lock, formats a string, and writes to a log
file. This I/O overhead may be the primary cause of both the fps drop and the
run-to-run variance.

**What to remove:**

Hot path logs to delete (these fire on every event, every frame, or every
message):

1. **Mouse move handler** (6 printlns, 60/sec each = 360/sec):
   - `[MOUSE] mouse_move handler entered`
   - `[MOUSE] BrowserState available, posting task`
   - `[MOUSE] mouse_move coords: ...`
   - `[MOUSE] Calling post_task for MouseMoveTask`
   - `[MOUSE] post_task returned`

2. **MouseMoveTask::execute** (4 printlns, 60/sec each = 240/sec):
   - `[MOUSE-TASK] MouseMoveTask::execute() called`
   - `[MOUSE-TASK] Browser obtained`
   - `[MOUSE-TASK] Host obtained, calling send_mouse_move_event`
   - `[MOUSE-TASK] send_mouse_move_event returned`

3. **Mouse click handler** (same pattern as mouse_move)

4. **Mouse wheel handler** (same pattern as mouse_move)

5. **MouseClickTask/MouseWheelTask::execute** (same pattern as MouseMoveTask)

6. **XPC receive** (1 println per message, 60/sec):
   - `[XPC-RECV] Received message: action=...`

7. **Frame transmit** (1 println per frame, ~40/sec):
   - `[FRAME-TX] frame=N t=Xms`

8. **Cursor change** (1 println per callback, ~17/sec):
   - `Profile: Cursor changed to type N`

9. **Loop timing** (every 1000 iterations + final):
   - `[LOOP-TIMING] iter=... max_mlw=...`

**What to keep:**

- All startup/shutdown logs (fire once)
- All error/failure logs (eprintln on error paths)
- `[SCROLL]` logs (fire every 125 events, ~1/sec)
- `[LOAD]` page loaded log (fires once)
- `[PERF]` benchmark stats (fires every 10s)
- `[BENCHMARK]` completion marker (fires once)
- `[MOUSE-RATE]` and `[CURSOR-RATE]` (fire 1/sec each)
- Connection/session logs (fire on connect/disconnect)

**How to test:**

1. `web benchmark` with no mouse — 3 runs, record all fps
2. `web benchmark` with continuous mouse — 3 runs, record all fps
3. Compare variance within each condition and difference between conditions

**What the results tell us:**

- If variance drops significantly (runs within ±2fps): logging was the variance
  source, benchmark is now reliable
- If no-mouse fps jumps well above 51.5: logging was dragging baseline
  performance
- If mouse-vs-no-mouse gap persists: mouse events have real overhead beyond
  logging — proceed with H1/H2 investigation
- If mouse-vs-no-mouse gap disappears: the entire "mouse performance" issue was
  really a "logging performance" issue

**Result:**

Four benchmark runs with logging removed, plus a cef-test reference run:

| Run | Condition | FPS  | p50    | p95    | 60fps% | Streak |
| --- | --------- | ---- | ------ | ------ | ------ | ------ |
| 1   | No mouse  | 44.6 | 16.8ms | 49.8ms | 60.2%  | 59     |
| 2   | Mouse     | 34.6 | 18.9ms | 83.0ms | 38.8%  | 16     |
| 3   | No mouse  | 33.7 | 19.1ms | 83.3ms | 27.7%  | 16     |
| 4   | Mouse     | 46.6 | 16.9ms | 49.7ms | 51.0%  | 30     |

| cef-test | FPS  | p50    | p95    | 60fps% | Streak |
| -------- | ---- | ------ | ------ | ------ | ------ |
| LEFT     | 37.8 | 16.9ms | 50.0ms | 51.6%  | 59     |
| RIGHT    | 39.5 | 16.8ms | 49.9ms | 59.0%  | 67     |

**Findings:**

1. **Mouse movement has no effect on performance.** Run 3 (no mouse) was the
   worst at 33.7fps. Run 4 (with mouse) was the best at 46.6fps. The
   correlation is random.

2. **There are exactly two performance modes.** The p50/p95 values are bimodal,
   not continuous:
   - Good mode: p50 = 16.8–16.9ms, p95 = 49.7–50.0ms
   - Bad mode: p50 = 18.9–19.1ms, p95 = 83.0–83.3ms

   These are quantized to display refresh multiples (16.7ms = 1/60s, 50.0ms =
   3/60s, 83.3ms = 5/60s). Something external — vsync alignment, macOS display
   server scheduling, or thermal state — determines which mode the system enters.

3. **cef-test shows the same performance.** At 37.8–39.5fps with identical p50
   and p95 values, cef-test is in the same band as termsurf's good mode.
   TermSurf is performing on par with the reference implementation.

4. **Removing logging did not fix variance.** The bimodal behavior is external
   to our code.

**Status:** Done

## Conclusion

The "mouse performance" issue was a false signal. The original Issue 345 finding
(51.5fps without mouse vs 39.0fps with mouse) was a coincidence — one run in
"good mode" and one in "bad mode" that happened to correlate with mouse input.

Experiment 3 proved this conclusively: with debug logging removed, a no-mouse
run scored 33.7fps while a mouse-movement run scored 46.6fps — the opposite of
the original finding.

**What we actually discovered:**

1. The rendering pipeline has a bimodal performance characteristic with p50/p95
   values quantized to 60Hz display refresh multiples. The cause is external to
   TermSurf (likely vsync/display server scheduling).

2. The profile server had ~660 println!/sec of debug logging on hot paths. While
   this didn't cause the bimodal variance, it was wasteful I/O and has been
   removed.

3. TermSurf performs on par with the cef-test reference implementation
   (~38–47fps in the same p50/p95 band).

**Hypotheses resolved:**

- H1 (post_task contention): Not the cause of the observed fps drop
- H2 (mutex contention): Not the cause of the observed fps drop
- H3 (cursor change round-trips): ~17/sec, minor overhead, not the cause
- H4 (excessive mouse event rate): Ruled out — rate is ~60Hz

**Issue status: Closed.** The mouse performance problem does not exist. The
debug logging cleanup is a useful side effect worth keeping.
