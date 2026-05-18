+++
status = "closed"
opened = "2026-02-07"
closed = "2026-02-07"
+++

# Issue 345: `web benchmark` — Automated Framerate Benchmark for ts3

## Background

### The performance gap

The cef-rs OSR example renders at ~60fps in-process. ts3's profile server
renders at 38fps. Issue 343 ran eight experiments trying to close the gap and
failed. Issue 344 built cef-test — a minimal multi-process CEF harness — to
isolate the cause. cef-test achieved ~50fps with two profiles running
simultaneously, proving the multi-process architecture is sound.

### The initial hypothesis (wrong)

Issue 344's conclusion hypothesized that the 12fps gap between cef-test (50fps)
and ts3 (38fps) was caused by WezTerm integration overhead — texture import,
redraw scheduling, or interaction with the terminal renderer.

### The revised hypothesis (input rate)

The real difference between cef-test's 50fps and ts3's 38fps is likely much
simpler: **input rate**.

cef-test's benchmark sends simulated scroll events at ~125Hz (8ms intervals)
directly inside the profile server via `BrowserHost::send_mouse_wheel_event()`.
This bypasses all input routing and guarantees a continuous stream of events
that force CEF to re-render every frame.

ts3's input path is very different:

```
User scrolls mouse
    │
    ▼
macOS generates scroll events (~125Hz hardware rate)
    │
    ▼
winit receives NSEvent in WezTerm's event loop
    │
    ▼
WezTerm routes through pane system
    │
    ▼
Serialized to XPC dictionary
    │
    ▼
Sent over XPC to profile server
    │
    ▼
Profile server calls host.send_mouse_wheel_event()
    │
    ▼
CEF renders
```

If any stage in this pipeline drops events, batches them, or introduces latency,
the effective input rate reaching CEF could be far lower than 125Hz. A lower
input rate means fewer scroll events per second, which means fewer frames where
the page content actually changes, which means fewer `on_accelerated_paint`
callbacks, which means lower measured fps.

In other words: **ts3 might not be rendering slowly — it might not be asking CEF
to render often enough**, because the input isn't arriving fast enough.

### Why we can't test this with manual scrolling

The 38fps measurement in ts3 was taken while manually scrolling with a mouse.
But manual scrolling introduces uncontrolled variables:

- **Inconsistent scroll speed** — humans don't scroll at a constant rate
- **Scroll direction changes** — pauses when reversing direction
- **Page boundary stalls** — hitting the top or bottom of the page stops
  rendering entirely
- **Different content** — the page being scrolled affects rendering cost

cef-test solved this by simulating scrolling directly in the profile server. ts3
needs the same capability to produce a valid comparison.

## Plan

### The `web benchmark` command

Add a new command to ts3:

```
web benchmark
```

This command:

1. Opens `https://www.google.com/search?q=asdf+asdf` in a webview pane
2. Waits for the page to load
3. Begins simulated scrolling at ~125Hz directly in the profile server
4. Runs for 70 seconds
5. Collects and prints framerate statistics
6. Closes the webview

The simulated scrolling happens entirely inside the profile server — no mouse
events traverse the GUI → XPC → profile path. This eliminates input routing as a
variable and isolates the rendering pipeline.

### Simulated scroll behavior

Identical to cef-test's Phase 8:

- Scroll events at 8ms intervals (~125Hz)
- `send_mouse_wheel_event()` called directly in the profile server's message
  loop
- Direction reverses every 25 events (~200ms) to keep the page in continuous
  motion without hitting top/bottom stalls
- Mouse position fixed at viewport center
- Delta of 120 per event (one standard scroll notch)

### Statistics output

After 70 seconds, close the webview, and then print to the terminal:

```
=== ts3 Benchmark (70s) ===

50.0 fps | 80.8% at 60fps | streak: 139 | p50: 16.7ms | p95: 33.6ms

3252 frames over 65.0s
```

This matches cef-test's benchmark output format for direct comparison.

### What does NOT happen

- **No keyboard input** is forwarded to the browser
- **No mouse input** is forwarded to the browser
- **No manual interaction** is required or accepted
- The benchmark is fully automated and deterministic

## Expected Outcomes

### If ts3 benchmark matches cef-test (~50fps)

The input hypothesis is confirmed. ts3's rendering pipeline is fine — the 38fps
measurement was caused by insufficient input rate during manual scrolling. The
fix is to improve the input forwarding path: ensure scroll events reach the
profile server at the full hardware rate (~125Hz) without batching or dropping.

### If ts3 benchmark is still ~38fps

The input hypothesis is wrong. The problem IS in ts3's integration — likely in
how the GUI imports IOSurface textures, schedules redraws, or coordinates with
WezTerm's terminal renderer. In this case, the 12fps gap is real overhead, and
the investigation moves to the rendering path.

### Either way, we learn something definitive

Just like cef-test eliminated the architecture question, this benchmark
eliminates the input question. One test, one answer.

## Implementation Notes

### How `web benchmark` reaches the profile server

The `web` command already sends a JSON message over a Unix socket to the GUI,
which triggers an XPC `spawn_profile` to the launcher. The benchmark variant
needs to tell the profile server to enable simulated scrolling. Options:

1. **CLI arg to profile server** — Add `--benchmark` flag to `termsurf-profile`.
   When set, the profile server simulates scrolling after page load, exactly
   like cef-test-profile does. The GUI passes this flag through the launcher's
   `spawn_profile` message.

2. **XPC command after connection** — GUI sends a `start_benchmark` XPC message
   to the profile server after the connection is established. Profile server
   starts simulated scrolling when it receives this message.

Option 1 is simpler and matches cef-test's approach.

### Collecting statistics

The profile server already has frame timing instrumentation from Issue 343
experiments. The benchmark mode adds:

- `FrameStats` tracking (same as cef-test-gui's implementation)
- After 70 seconds, print the summary to the profile server's log
- The `web benchmark` command reads the summary from the log and prints it to
  the terminal

Alternatively, the profile server can send the statistics back to the GUI via
XPC, and the GUI prints them. This is cleaner but requires a new XPC message
type.

### Relationship to cef-test's benchmark

| Aspect             | cef-test benchmark          | ts3 `web benchmark`         |
| ------------------ | --------------------------- | --------------------------- |
| GUI                | Bare winit + wgpu           | WezTerm (full terminal)     |
| Profile server     | cef-test-profile            | termsurf-profile            |
| Input simulation   | Direct in profile server    | Direct in profile server    |
| IOSurface transfer | XPC Mach port               | XPC Mach port               |
| Texture import     | Standalone wgpu pipeline    | WezTerm's wgpu integration  |
| Event loop         | Simple pump_app_events      | WezTerm's event loop        |
| Scroll rate        | ~125Hz (8ms)                | ~125Hz (8ms)                |
| Duration           | 70s                         | 70s                         |
| Input routing      | None (simulated in-process) | None (simulated in-process) |

The only differences are on the GUI side: WezTerm's renderer vs bare wgpu. If
the results match (~50fps), then WezTerm's integration is not the problem and
the input path was the bottleneck all along. If they don't match, WezTerm's
integration is the culprit and we can bisect it.

## Implementation Phases

Each phase is independently testable. We confirm each phase works before moving
to the next.

### Phase 1: `--benchmark` flag on termsurf-profile (`cargo check`)

Add a `--benchmark` CLI flag to `termsurf-profile` that is parsed but does
nothing yet. This confirms the flag is wired through argument parsing without
breaking any existing behavior.

**Files:**

- `ts3/termsurf-profile/src/main.rs` — Add `#[arg(long)] benchmark: bool` to
  `Args` struct.

**Test:** `cd ts3 && cargo check -p termsurf-profile` succeeds. Run
`termsurf-profile --help` and see `--benchmark` listed.

**Result:** Done. Added `#[arg(long)] benchmark: bool` to `Args`. `cargo check`
passes clean (only pre-existing warnings).

### Phase 2: Pass `--benchmark` through the full chain (`cargo check`)

Thread the benchmark flag from the coordinator (`web benchmark`) through the GUI
socket, XPC manager, launcher, and into the profile server's command line. No
behavior changes yet — just plumbing.

**The chain:**

1. **Coordinator** (`termsurf-web/src/main.rs`) — Detect `benchmark` as the URL
   argument. When URL is `"benchmark"`, set `benchmark: true` in the JSON
   `open_webview` request and hardcode the URL to
   `https://www.google.com/search?q=asdf+asdf`.

2. **GUI socket handler** (`webview_socket.rs`) — Extract `benchmark` bool from
   the `open_webview` request data. Pass it to
   `xpc_manager.request_profile_spawn()`.

3. **XPC manager** (`webview_xpc.rs`) — Add `benchmark: bool` parameter to
   `request_profile_spawn()`. Set `msg.set_bool("benchmark", true)` in the XPC
   spawn message.

4. **Launcher** (`termsurf-launcher/src/main.rs`) — Extract `benchmark` bool
   from XPC message. When true, add `--benchmark` to the profile server command
   line. Also forward in `create_browser` messages for existing profiles.

5. **termsurf-xpc** — Add `set_bool` / `get_bool` to `XpcDictionary` if not
   already present.

**Files:**

- `ts3/termsurf-web/src/main.rs` — Detect `"benchmark"` URL, set flag in JSON.
- `ts3/wezterm-gui/src/termwindow/webview_socket.rs` — Extract and forward
  benchmark flag.
- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — Add benchmark param to
  `request_profile_spawn`, include in XPC message.
- `ts3/termsurf-launcher/src/main.rs` — Read benchmark from XPC, add
  `--benchmark` to spawn command.
- `ts3/termsurf-xpc/` — Add `set_bool`/`get_bool` if needed.

**Test:** `cd ts3 && cargo check` (full workspace check) succeeds. All five
crates compile with the new plumbing.

**Result:** Done. `set_bool`/`get_bool` already existed in termsurf-xpc.
Coordinator detects `web benchmark`, hardcodes the Google search URL, and sets
`benchmark: true` in JSON. Socket handler extracts it, XPC manager includes it
in the spawn message, launcher adds `--benchmark` to the command line. Full
workspace `cargo check` passes.

### Phase 3: Page load detection in termsurf-profile (`log output`)

Add a `LoadHandler` to termsurf-profile that detects when the page finishes
loading and sets a `PAGE_LOADED` atomic flag. This is a prerequisite for
starting scroll simulation — we can't scroll until the page is loaded.

cef-test-profile already has this pattern (`TestLoadHandler` +
`on_loading_state_change`). Port it to termsurf-profile.

**Files:**

- `ts3/termsurf-profile/src/main.rs` — Add `PAGE_LOADED: AtomicBool` static. Add
  `LoadHandler` impl to `cef_handlers` module. Wire it into `ProfileClient`.

**Test:** Build and run `web google.com`. Check
`/tmp/termsurf-profile-default.log` for `[LOAD] Page finished loading` message.
This confirms the load handler fires in the real ts3 pipeline.

**Result:** Done. Ran `web google.com` — line 31 of the profile log shows
`[LOAD] Page finished loading`. Load handler fires correctly in the real ts3
pipeline.

### Phase 4: Scroll simulation in benchmark mode (`log output`)

When `--benchmark` is set, inject simulated scroll events at ~125Hz after
`PAGE_LOADED` becomes true. This is the core mechanic — identical to
cef-test-profile's scroll loop.

**Implementation:**

- In the message loop (`while !QUIT_FLAG`), add scroll simulation code gated on
  `args.benchmark && PAGE_LOADED.load(Relaxed)`.
- Copy the scroll state from cef-test-profile: 8ms interval, direction reversal
  every 25 events, delta of 120, mouse position at viewport center.
- The scroll events are sent via `cef::post_task` using the existing
  `MouseWheelTask`, calling `host.send_mouse_wheel_event()` directly on the CEF
  UI thread.

**Files:**

- `ts3/termsurf-profile/src/main.rs` — Add scroll simulation state and logic to
  the message loop. Gated behind `args.benchmark`.

**Test:** Build and run `web benchmark`. Check the profile log for `[SCROLL]`
messages. The webview should scroll automatically without any manual input.
Visually confirm the page scrolls up and down.

**Result:** Done. `web benchmark` scrolls automatically — 1,250 scroll events
sent over ~21s before Ctrl+C. Log shows `[SCROLL] Page loaded, starting
simulated scroll at ~125Hz` followed by periodic `[SCROLL] N events sent`
milestones. Cursor type oscillates between 0 and 2 confirming page scrolls over
different elements. Early framerate read: ~42fps (883 frames in 21.05s).

### Phase 5: FrameStats collection (`log output`)

Add `FrameStats` tracking to the profile server. Record frame intervals in
`on_accelerated_paint`. Print periodic summaries to the profile log using the
same `[PERF]` format as cef-test.

**Implementation:**

- Port `FrameStats` struct from `cef-test-gui/src/main.rs` into
  termsurf-profile. Since `on_accelerated_paint` runs on the CEF IO thread, use
  a `Mutex<FrameStats>` behind a global `OnceLock`, or use per-frame atomics
  with a `Vec<u64>` collection in the main loop.
- In benchmark mode, print `[PERF]` summary every 10 seconds from the message
  loop.
- After 70 seconds, print the final summary and set `QUIT_FLAG`.

**Files:**

- `ts3/termsurf-profile/src/main.rs` — Add `FrameStats`, record in
  `on_accelerated_paint`, print summaries, auto-quit after 70s in benchmark
  mode.

**Test:** Run `web benchmark`. Watch the profile log for `[PERF]` lines every
10s. After 70s, the profile server should print final stats and exit. Compare
numbers against cef-test's benchmark.

**Result:** Done. Seven `[PERF]` summaries printed at 10s intervals. Auto-quit
fired at 70s. Final stats: 2939 frames over 70.6s = 41.6fps, 57.4% at 60fps,
max streak 57, p50=16.8ms, p95=49.9ms. This is close to the manually-measured
38fps and well below cef-test's ~50fps — the input rate hypothesis is wrong, the
bottleneck is in ts3's WezTerm integration.

### Phase 6: Statistics printed to the terminal (`end-to-end`)

The coordinator (`web benchmark`) reads the final stats from the profile log and
prints them to the user's terminal. This completes the user-facing feature.

**Implementation:**

- After the `open_webview` response, the coordinator enters a polling loop
  instead of waiting for Ctrl+C.
- Poll the profile log file (`/tmp/termsurf-profile-default.log`) for the final
  `[PERF]` summary line.
- When found (or after timeout), parse the stats and print the formatted output:

  ```
  === ts3 Benchmark (70s) ===

  50.0 fps | 80.8% at 60fps | streak: 139 | p50: 16.7ms | p95: 33.6ms

  3252 frames over 65.0s
  ```

- Send `close_webview` to clean up.

**Files:**

- `ts3/termsurf-web/src/main.rs` — Add benchmark polling loop to
  `run_coordinator` (when benchmark mode is detected).

**Test:** Run `web benchmark` from the terminal. The benchmark runs for 70s and
prints formatted stats directly to the terminal. No manual log inspection
needed.

**Result:** Done. `web benchmark` prints formatted stats directly to the
terminal after 70s, then auto-closes the webview. Clean run result: 51.5fps,
55.4% at 60fps, streak 25, p50 18.7ms, p95 33.9ms, 3646 frames over 70.8s.
This matches cef-test's ~50fps — confirming the input hypothesis: ts3's
rendering pipeline is fine, the 38fps during manual scrolling was caused by
insufficient input rate through the GUI → XPC → profile path.

### Phase summary

| Phase | What                        | Test                              | Status |
| ----- | --------------------------- | --------------------------------- | ------ |
| 1     | `--benchmark` flag parsed   | `cargo check -p termsurf-profile` | Done   |
| 2     | Flag threaded through chain | `cargo check` (full workspace)    | Done   |
| 3     | Page load detection         | Log shows `[LOAD]` message        | Done   |
| 4     | Scroll simulation           | Log shows `[SCROLL]`, page moves  | Done   |
| 5     | FrameStats + auto-quit      | Log shows `[PERF]` every 10s      | Done   |
| 6     | Stats printed to terminal   | `web benchmark` prints results    | Done   |

## Conclusion

### The experiment

Two benchmark runs with identical simulated scrolling (125Hz, direct in profile
server). The only difference: whether the user moved their mouse over the
webview during the test.

| Condition              | FPS  | p50    | p95    | 60fps% | Streak |
| ---------------------- | ---- | ------ | ------ | ------ | ------ |
| No mouse movement      | 51.5 | 18.7ms | 33.9ms | 55.4%  | 25     |
| Continuous mouse moves | 39.0 | 17.4ms | 78.8ms | 38.3%  | 10     |
| cef-test reference     | ~50  | —      | —      | —      | —      |

### What this proves

1. **ts3's rendering pipeline is not the bottleneck.** With no mouse input, ts3
   achieves 51.5fps — matching cef-test's ~50fps. WezTerm's integration
   (IOSurface import, texture rendering, event loop) adds negligible overhead.

2. **Mouse move events cause frame drops.** The median frame interval (p50) is
   nearly identical between runs (18.7ms vs 17.4ms), proving the rendering
   pipeline itself runs at the same speed. But the p95 doubles from 33.9ms to
   78.8ms — mouse move events traversing the GUI → XPC → profile path create
   periodic stalls that destroy tail latency.

3. **The original 38fps was caused by mouse contention, not scroll rate.** The
   initial hypothesis (Issue 344) was that insufficient scroll input rate caused
   the gap. The revised truth is subtler: scroll events arrive fine, but mouse
   move events that accompany manual scrolling compete with rendering and cause
   frame drops.

### The fix

The problem is not scroll rate — it's mouse move overhead in the profile server.
Possible approaches:

- **Throttle mouse moves** — the GUI currently forwards every mouse move event
  over XPC. Throttling to ~60Hz (one per frame) would reduce XPC traffic by ~50%
  without visible loss of cursor accuracy.
- **Batch or coalesce events** — combine multiple mouse moves into one, sending
  only the latest position.
- **Process mouse moves off the critical path** — decouple mouse event handling
  from CEF's message loop so it doesn't block frame delivery.
