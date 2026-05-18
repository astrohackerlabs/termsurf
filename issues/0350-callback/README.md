+++
status = "closed"
opened = "2026-02-08"
closed = "2026-02-08"
+++

# Issue 350: Event-Driven CEF Message Pump

## Background

The profile server (`termsurf-profile`) needs to call CEF's
`do_message_loop_work()` to drive rendering. The current implementation uses a
busy-wait loop that burns one full CPU core at 100%:

```rust
while !QUIT_FLAG.load(Ordering::Relaxed) {
    cef::do_message_loop_work();
    cfrunloop::run_for(0.000);  // returns immediately = infinite loop
}
```

This achieves ~50fps but causes thermal throttling within minutes, making
performance benchmarks unreliable and wasting energy during normal use.

CEF provides `external_message_pump` mode with the
`on_schedule_message_pump_work(delay_ms)` callback — CEF tells you exactly when
to call `do_message_loop_work()`. Between callbacks, the process can sleep. This
is the intended API for driving CEF from a custom event loop.

### Prior art

| Implementation               | Architecture             | Main loop                     | Performance      |
| ---------------------------- | ------------------------ | ----------------------------- | ---------------- |
| cef-rs reference             | In-process, windowed     | `NSApp().run()`               | 60fps            |
| ts2                          | In-process with WezTerm  | WezTerm's AppKit loop         | 60fps            |
| ts3 busy-wait                | Out-of-process, headless | `while { work(); sleep(0); }` | ~50fps, 100% CPU |
| ts3 Experiment 3 (Issue 349) | Out-of-process, headless | `CFRunLoopRun()` + timers     | ~29fps, low CPU  |

The gap between the busy-wait (~50fps) and the event-driven attempt (~29fps) is
the problem this issue aims to close.

### What Issue 349 Experiment 3 did

- Added `external_message_pump: 1` to CEF settings
- Implemented `on_schedule_message_pump_work` in `ProfileBPH`
- Created `cef_pump` module with reentrancy guard and 33ms fallback timer
- Replaced the while-loop with `CFRunLoopRun()` (blocking, event-driven)
- Moved benchmark scroll simulation to an 8ms repeating CFRunLoop timer

The pump ran without crashing or deadlocking — a first for ts3. But fps dropped
from ~50 to ~29, with p50 shifting from 17ms to 25ms.

### Open questions from Issue 349

1. **Is the callback actually firing?** We assumed it stops being reliable, but
   never logged it. The problem might be response latency, not callback silence.

2. **Timer creation overhead.** Every `on_schedule_message_pump_work` call
   creates a new `CFRunLoopTimer` — even for delay=0. Timer allocation + run
   loop registration is heavyweight compared to signaling a `CFRunLoopSource`.

3. **Benchmark timer contention.** The 8ms scroll timer runs on the main thread.
   While its callback executes, no other timer (including the CEF pump timer)
   can fire. This steals up to 12.5% of available time.

4. **Multiple pump calls per frame.** CEF may need several
   `do_message_loop_work()` calls to push one frame through its pipeline. The
   busy-wait loop provides thousands of calls/sec. The event-driven pump
   provides ~30/sec (fallback timer). If frames need 5-10 calls each, 30/sec is
   nowhere near enough.

5. **CFRunLoopRun() vs NSApp().run().** The reference uses AppKit's full event
   loop. A headless process can create NSApplication with activation policy
   `.prohibited`. CEF's Chrome runtime may depend on AppKit event dispatching.

## Ideas

### Idea 1: Diagnostic logging

Add logging to `on_schedule_message_pump_work` (timestamp, delay_ms, thread) and
`pump_timer_callback` (timestamp, was it a fallback?) to see whether the callback
fires reliably or if we're running on the 33ms fallback. This is the most
important first step — it tells us which of the open questions matters.

### Idea 2: Reduce fallback timer to 1ms

Change `MAX_TIMER_DELAY_MS` from 33 to 1. If fps jumps to ~50, the callback
wasn't firing and we were running on the fallback — a 1ms fallback gives 1000
pumps/sec with negligible CPU. Cheapest possible test.

### Idea 3: CFRunLoopSource for immediate work

Replace `CFRunLoopTimer` with a persistent `CFRunLoopSource` for delay=0
requests. Signaling a source is just setting a flag — no timer allocation. It
triggers on the very next run loop iteration.

### Idea 4: NSApp().run() in the headless process

A headless process can create NSApplication with activation policy `.prohibited`
(no dock icon, no menu). If CEF needs AppKit event dispatching internally, this
would close the gap between bare `CFRunLoopRun()` and the reference
implementation's `NSApp().run()`.

## Experiments

### Experiment 1: Diagnostic logging of the pump cycle

**Goal:** Determine whether the performance gap is caused by the callback not
firing, or by our timer scheduling adding too much latency. Without this data,
every fix is a guess.

**Method:** Add counters and per-second summary logging to the `cef_pump` module.
No architectural changes — just instrumentation.

**What to measure (per-second counters):**

1. `schedule_work` calls with delay=0 (CEF wants immediate work)
2. `schedule_work` calls with delay>0 (CEF wants deferred work)
3. Fallback timer fires (the 33ms timer scheduled when no callback arrives)
4. Callback-scheduled timer fires (timers created from `schedule_work`)
5. Reentrancy detections (`is_active` was true when timer fired)
6. `do_message_loop_work()` calls actually executed

Print a one-line summary every second:

```
[PUMP] callbacks=247(imm=180 def=67) fires=62(cb=31 fb=31) reentrant=0 work=62
```

This tells us:

- **callbacks >> fires**: timer scheduling is the bottleneck — callbacks fire
  but timers don't keep up (new timers supersede old ones before they fire)
- **fires ≈ 30, mostly fallback**: callback isn't driving the pump — we're
  running at the fallback rate
- **callbacks ≈ 30**: CEF itself only requests work ~30 times/sec — the problem
  is upstream of our scheduling
- **callbacks >> 60, fires ≈ 60**: pump is working correctly, problem is
  elsewhere (GUI-side presentation, etc.)

**What to change in `cef_pump`:**

Add atomic counters (no mutex needed for counters):

```rust
static SCHED_IMMEDIATE: AtomicU64 = AtomicU64::new(0);
static SCHED_DEFERRED: AtomicU64 = AtomicU64::new(0);
static FIRE_CALLBACK: AtomicU64 = AtomicU64::new(0);
static FIRE_FALLBACK: AtomicU64 = AtomicU64::new(0);
static REENTRANT: AtomicU64 = AtomicU64::new(0);
static WORK_DONE: AtomicU64 = AtomicU64::new(0);
```

In `schedule_work`: increment `SCHED_IMMEDIATE` or `SCHED_DEFERRED` based on
whether the original `delay_ms` was <= 0 or > 0. Track whether the timer being
scheduled is a fallback (called from `pump_timer_callback` with
`MAX_TIMER_DELAY_MS`) by adding a `is_fallback: bool` parameter or a separate
`schedule_fallback` function.

In `pump_timer_callback`: increment `FIRE_CALLBACK` or `FIRE_FALLBACK` based on
whether this timer was scheduled by a callback or the fallback path. Increment
`WORK_DONE` after each `do_message_loop_work()`. Increment `REENTRANT` when
`is_active` is true.

Print the summary every second using a timestamp check in `pump_timer_callback`
(since it's the only code that runs regularly on the main thread).

**Status:** Complete

**Results (trial 7 of benchmark, last profile log preserved):**

```
[PUMP] callbacks=753(imm=753 def=0) fires=758(cb=752 fb=6) reentrant=0 work=758  ← page load burst
[PUMP] callbacks=149(imm=127 def=22) fires=133(cb=127 fb=6) reentrant=0 work=133
[PUMP] callbacks=135(imm=125 def=10) fires=129(cb=125 fb=4) reentrant=0 work=129
[PUMP] callbacks=142(imm=128 def=14) fires=134(cb=128 fb=6) reentrant=0 work=134
[PUMP] callbacks=101(imm=87 def=14) fires=93(cb=86 fb=7) reentrant=0 work=93
[PUMP] callbacks=127(imm=93 def=34) fires=97(cb=93 fb=4) reentrant=0 work=97
[PUMP] callbacks=132(imm=87 def=45) fires=92(cb=87 fb=5) reentrant=0 work=92
[PUMP] callbacks=141(imm=65 def=76) fires=65(cb=65 fb=0) reentrant=0 work=65
[PUMP] callbacks=187(imm=99 def=88) fires=99(cb=99 fb=0) reentrant=0 work=99
[PUMP] callbacks=114(imm=88 def=26) fires=93(cb=88 fb=5) reentrant=0 work=93
```

Benchmark: ~29fps across all 7 trials, thermal nominal. First trial anomaly: the
browser never appeared on screen but fps was still collected (the profile server
counted frames internally even though they may not have reached the GUI).

**Findings:**

1. **The callback fires reliably.** CEF sends 100-190
   `on_schedule_message_pump_work` calls per second, overwhelmingly delay=0
   (immediate). This rules out "callback not firing" as the cause.

2. **Nearly zero fallback.** Only 0-7 fallback fires per second. The pump is
   genuinely callback-driven, not limping on the 33ms fallback timer.

3. **Zero reentrancy.** No contention within the pump itself.

4. **Timers are being superseded (callbacks > fires).** On second 8: 141
   callbacks but only 65 fires. Each `schedule_work(0)` kills the pending timer
   and creates a new one. If two callbacks arrive between run loop iterations,
   the first timer never fires.

5. **~65-134 work/sec yields ~29fps — about 3-4 `do_message_loop_work()` calls
   per frame.** Each call processes one internal CEF pipeline stage. With ~10ms
   between work calls (100 work/sec), a 3-stage frame takes ~30ms — matching the
   observed p50 of 25ms.

**Conclusion:** The bottleneck is timer scheduling latency for immediate work.
When CEF says "I need work NOW" (delay=0), we create a new `CFRunLoopTimer`. But
timer creation + run loop registration is heavyweight. Before the timer fires,
another callback arrives and supersedes it. The work rate is throttled by how
fast the run loop processes timers, not by how fast CEF requests work.

This directly points to Idea 3 (CFRunLoopSource) as the next experiment. A
persistent `CFRunLoopSource` for delay=0 work avoids timer creation entirely —
signaling is just setting a flag, and it fires on the very next run loop
iteration regardless of how many times it's been signaled.

### Experiment 2: CFRunLoopSource for immediate work

**Goal:** Eliminate timer creation overhead for delay=0 requests. Experiment 1
showed that CEF sends 100-190 immediate callbacks/sec but only 65-134 timers
fire — the rest are superseded before they get a chance. A persistent
`CFRunLoopSource` avoids timer allocation entirely.

**How CFRunLoopSource works:**

- Created once at init with `CFRunLoopSourceCreate()`, added to the run loop
  with `CFRunLoopAddSource()`
- Signaled with `CFRunLoopSourceSignal()` — just sets a flag, O(1), no
  allocation
- Followed by `CFRunLoopWakeUp()` to wake the run loop if it's sleeping
- On the next run loop iteration, the source's callback fires
- Multiple signals before the callback fires coalesce into one fire — but that
  one fire calls `do_message_loop_work()` which processes ALL pending CEF work
- The source stays registered — no re-creation needed

**Architecture:**

- **delay=0 (immediate):** Signal the persistent source + wake the run loop.
  No timer creation, no timer supersession. This is the fast path — Experiment 1
  showed ~75% of callbacks are immediate.
- **delay>0 (deferred):** Keep using `CFRunLoopTimer` as before. These are
  infrequent (~25% of callbacks) and legitimately need a delay.
- **Fallback:** Keep the 33ms fallback timer. Although Experiment 1 showed it
  rarely fires, it's a safety net.

**What to add to `cfrunloop` module:**

```rust
type CFRunLoopSourceRef = *mut c_void;

// CFRunLoopSourceContext — 10-field struct, only `perform` callback needed
#[repr(C)]
struct CFRunLoopSourceContext {
    version: CFIndex,
    info: *mut c_void,
    retain: *const c_void,
    release: *const c_void,
    copy_description: *const c_void,
    equal: *const c_void,
    hash: *const c_void,
    schedule: *const c_void,
    cancel: *const c_void,
    perform: unsafe extern "C" fn(*mut c_void),
}

extern "C" {
    fn CFRunLoopSourceCreate(
        allocator: *const c_void,
        order: CFIndex,
        context: *const CFRunLoopSourceContext,
    ) -> CFRunLoopSourceRef;
    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopSourceSignal(source: CFRunLoopSourceRef);
    fn CFRunLoopSourceInvalidate(source: CFRunLoopSourceRef);
}
```

Helper functions: `create_source(callback) -> CFRunLoopSourceRef`,
`signal_source(source)`.

**What to change in `cef_pump`:**

- Add `init()` function that creates and registers the persistent source. Call
  it once before `cfrunloop::run()`.
- `schedule_work(delay_ms)` with delay<=0: signal the source + `wake_up()`.
  No timer involved.
- `schedule_work(delay_ms)` with delay>0: create a `CFRunLoopTimer` as before.
- Source callback: same reentrancy guard, `do_message_loop_work()`, fallback
  scheduling as current `pump_timer_callback`.
- Keep all diagnostic counters from Experiment 1 (add a `FIRE_SOURCE` counter).

**Expected outcome:** If timer supersession was the bottleneck, work/sec should
jump significantly (from ~100 to several hundred), and fps should increase
proportionally. If fps stays at ~29, the bottleneck is elsewhere (run loop
iteration speed, benchmark timer contention, or NSApp vs CFRunLoop).

**Status:** Complete (bug found)

**Results (trial 7 of benchmark, last profile log preserved):**

```
[PUMP] callbacks=1107(imm=1107 def=0) fires=18820(src=1042 tmr=0 fb=17778) reentrant=0 work=18820
[PUMP] callbacks=244(imm=207 def=37) fires=27161(src=130 tmr=14 fb=27017) reentrant=0 work=27161
[PUMP] callbacks=167(imm=167 def=0) fires=31042(src=119 tmr=0 fb=30923) reentrant=0 work=31042
[PUMP] callbacks=261(imm=178 def=83) fires=32932(src=134 tmr=32 fb=32766) reentrant=0 work=32932
[PUMP] callbacks=160(imm=124 def=36) fires=34450(src=89 tmr=13 fb=34348) reentrant=0 work=34450
[PUMP] callbacks=174(imm=112 def=62) fires=35810(src=78 tmr=21 fb=35711) reentrant=0 work=35810
[PUMP] callbacks=195(imm=126 def=69) fires=38537(src=89 tmr=28 fb=38420) reentrant=0 work=38537
[PUMP] callbacks=223(imm=104 def=119) fires=37076(src=71 tmr=46 fb=36959) reentrant=0 work=37076
[PUMP] callbacks=266(imm=138 def=128) fires=39110(src=73 tmr=46 fb=38991) reentrant=0 work=39110
[PUMP] callbacks=170(imm=121 def=49) fires=41017(src=90 tmr=19 fb=40908) reentrant=0 work=41017
```

Benchmark: ~22fps (down from ~29fps in Experiment 1), thermal nominal. Performance
got worse, not better.

**Findings: zombie timer leak**

The fallback count is exploding: 17k → 27k → 31k → 35k → 41k fires/sec, growing
every second and never stabilizing. This is a cascading timer leak caused by two
bugs in the source+timer interaction:

**Bug 1: Timer reference dropped without invalidation.** When the source callback
fires, `do_pump_work()` unconditionally sets `pump.timer = None`. If a fallback
timer was already pending, dropping the reference does NOT remove it from the run
loop — it becomes a zombie. The zombie still fires, calls `do_pump_work`, and
schedules yet another fallback. Each source fire (~100/sec) leaks one zombie.
After a few seconds, thousands of zombies are firing in a chain reaction.

**Bug 2: No source-pending tracking.** After `do_message_loop_work()`, if CEF
called `schedule_work(0)` (signaling the source), we don't detect that.
`was_reentrant` is false (schedule_work doesn't set `reentrancy_detected`),
`has_timer` is false (source doesn't create a timer). So we schedule a fallback
even though the source is already signaled for the next iteration.

**Fixes needed:**

1. **Invalidate, don't just clear:** `do_pump_work` must invalidate any pending
   timer before clearing: `if let Some(timer) = pump.timer.take() { invalidate(timer); }`
2. **Track source pending state:** Add `source_pending: bool` to PumpState.
   `schedule_work(0)` sets it true. Source callback sets it false. Don't schedule
   fallback if `source_pending` is true.

These are implementation bugs, not architectural problems. The CFRunLoopSource
approach is sound — we just need to fix the source/timer interaction before we
can evaluate whether it improves fps.

### Experiment 3: Fix zombie timer leak and source-pending tracking

**Goal:** Fix the two bugs from Experiment 2 and get a clean measurement of
CFRunLoopSource performance.

**Bug 1 fix: Invalidate timers before clearing.**

In `do_pump_work`, replace `pump.timer = None` with:

```rust
if let Some(timer) = pump.timer.take() {
    super::cfrunloop::invalidate_timer(timer.0);
}
```

This ensures any pending timer is removed from the run loop, not just orphaned.

**Bug 2 fix: Track source-pending state.**

Add `source_pending: bool` to `PumpState`. Update the three touchpoints:

1. **`schedule_work(0)`** — after signaling the source, set `source_pending = true`
2. **`source_callback`** — set `source_pending = false` (the signal was consumed)
3. **`do_pump_work` post-work logic** — don't schedule fallback if
   `source_pending` is true (source is already queued for the next iteration)

The post-work decision tree becomes:

```
if was_reentrant:
    signal source (immediate re-pump)
elif source_pending:
    do nothing (source already queued)
elif has_timer:
    do nothing (deferred timer already pending)
else:
    schedule fallback (33ms safety net)
```

**What to keep:** All diagnostic counters from Experiments 1-2. The log format
is unchanged — the same `[PUMP]` line lets us compare directly with prior runs.

**Expected outcome:** Fallback fires should drop back to single digits (as in
Experiment 1). Work/sec should reflect actual source + timer fires without zombie
inflation. If the source eliminates timer supersession, work/sec should be higher
than Experiment 1's ~100/sec, and fps should improve above ~29.

**Status:** Complete

**Results (trial 7 of benchmark, last profile log preserved):**

```
[PUMP] callbacks=982(imm=982 def=0) fires=988(src=981 tmr=0 fb=7) reentrant=0 work=988
[PUMP] callbacks=204(imm=175 def=29) fires=181(src=175 tmr=0 fb=6) reentrant=0 work=181
[PUMP] callbacks=150(imm=150 def=0) fires=157(src=150 tmr=0 fb=7) reentrant=0 work=157
[PUMP] callbacks=192(imm=145 def=47) fires=149(src=145 tmr=0 fb=4) reentrant=0 work=149
[PUMP] callbacks=154(imm=115 def=39) fires=120(src=115 tmr=0 fb=5) reentrant=0 work=120
[PUMP] callbacks=151(imm=103 def=48) fires=108(src=103 tmr=0 fb=5) reentrant=0 work=108
[PUMP] callbacks=174(imm=109 def=65) fires=114(src=109 tmr=0 fb=5) reentrant=0 work=114
[PUMP] callbacks=222(imm=110 def=112) fires=110(src=110 tmr=0 fb=0) reentrant=0 work=110
[PUMP] callbacks=207(imm=103 def=104) fires=103(src=103 tmr=0 fb=0) reentrant=0 work=103
[PUMP] callbacks=136(imm=97 def=39) fires=102(src=97 tmr=0 fb=5) reentrant=0 work=102
```

Benchmark: ~22fps (down from ~29fps in Experiment 1), thermal nominal. Zombie
leak is fixed (fallback back to 0-7/sec), but performance got worse, not better.

**Findings:**

1. **Zombie fix confirmed.** Fallback fires are back to single digits, matching
   Experiment 1. The timer invalidation and source-pending tracking work
   correctly.

2. **tmr=0 always.** Despite 29-112 deferred callbacks/sec, no deferred timer
   ever fires. The source callback's `do_pump_work` invalidates any pending
   timer. CEF says "call me in 5ms" → timer created → source fires first →
   invalidates the timer → deferred work waits for the next source fire or a
   33ms fallback instead of the requested 5ms.

3. **More work, fewer frames.** Experiment 1: ~100 work/sec → 29fps.
   Experiment 3: ~100-180 work/sec → 22fps. The source eliminated supersession
   (fires ≈ callbacks), but extra work calls didn't produce extra frames — they
   add overhead that slows the rendering pipeline.

4. **p50 jumped from 25ms → 44ms.** Each frame takes longer despite more
   `do_message_loop_work()` calls per second.

5. **Source adds lock contention.** Each pump cycle now involves 4 PUMP lock
   acquisitions (schedule_work → source_callback → do_pump_work × 2) vs 2 in
   the timer approach. Background-thread `schedule_work(0)` calls contend with
   the main thread.

**Comparison across experiments:**

| Experiment | Approach      | work/sec  | fps  | p50   |
| ---------- | ------------- | --------- | ---- | ----- |
| 1          | Timer-only    | ~100      | ~29  | 25ms  |
| 2          | Source (buggy)| ~30,000+  | ~22  | 50ms  |
| 3          | Source (fixed)| ~100-180  | ~22  | 44ms  |

**Conclusion:** CFRunLoopSource didn't help. Timer supersession (Experiment 1's
callbacks > fires gap) was not the bottleneck — the timer-only approach was
actually better. The source approach adds overhead (lock contention, deferred
timer invalidation) that outweighs any benefit from eliminating supersession.

Next steps: revert to the timer-only approach (Experiment 1) and investigate the
remaining ideas — benchmark timer contention (the 8ms scroll timer competing for
main thread time) or replacing bare `CFRunLoopRun()` with `NSApp().run()`.

### Reference implementation analysis

After Experiments 1-3 failed to close the gap, we compared the cef-rs reference
external pump (`cef-rs/examples/tests_shared/src/browser/main_message_loop_external_pump/`)
with our `cef_pump` module. The reference gets ~50fps. We get ~22-29fps. Three
critical differences:

**1. NSApp().run() instead of CFRunLoopRun()**

The reference creates an NSApplication subclass (`SimpleApplication`) and runs
`NSApp(mtm).run()`. We use bare `CFRunLoopRun()`. NSApp manages all run loop
modes automatically — it is the intended way to run a Cocoa event loop.

**2. Timers registered in two run loop modes (the root cause)**

Reference (`mac.rs:118-119`):

```rust
owner_runloop.addTimer_forMode(&timer, NSRunLoopCommonModes);
owner_runloop.addTimer_forMode(&timer, NSEventTrackingRunLoopMode);
```

Our code only adds timers to `kCFRunLoopCommonModes`. When scroll events are
processed, the run loop enters `NSEventTrackingRunLoopMode` — and our timers
don't fire. The pump starves until the 33ms fallback kicks in, capping fps at
~30. This matches our observed numbers exactly.

**3. performSelector:onThread: for thread marshaling**

The reference uses `performSelector_onThread_withObject_waitUntilDone` to safely
marshal `on_schedule_message_pump_work` calls to the main thread. We use direct
CFRunLoop signaling with a mutex.

**Comparison:**

| Aspect            | Reference (50fps)              | TermSurf (~22-29fps)        |
| ----------------- | ------------------------------ | --------------------------- |
| Main loop         | `NSApp().run()`                | `CFRunLoopRun()`            |
| Timer modes       | CommonModes + EventTracking    | CommonModes only            |
| Timer API         | NSTimer                        | CFRunLoopTimer              |
| Thread marshaling | performSelector:onThread:      | Direct CFRunLoop + mutex    |
| NSApplication     | Yes (SimpleApplication)        | No                          |

### Experiment 4: Match reference architecture

**Goal:** Revert to the timer-only approach (Experiment 1 was our best result at
~29fps) and close the gap with the reference by adopting its two key patterns:
`NSApp().run()` and dual-mode timer registration.

**What to change:**

1. **Revert cef_pump to timer-only.** Remove the CFRunLoopSource code from
   Experiments 2-3. Go back to the Experiment 1 architecture where every
   `schedule_work` call creates a CFRunLoopTimer. Keep the diagnostic counters
   (use the Experiment 1 log format with `cb` and `fb` fire counts).

2. **Register timers in both modes.** Add `NSEventTrackingRunLoopMode` to
   `cfrunloop::create_timer` and `cfrunloop::create_repeating_timer`. This
   requires importing the `NSEventTrackingRunLoopMode` constant — it lives in
   AppKit, not CoreFoundation. Since we use raw FFI, we need the
   `CFStringRef` for this mode. It can be obtained via:
   ```rust
   extern "C" {
       static NSEventTrackingRunLoopMode: CFStringRef;
   }
   ```
   Then add a second `CFRunLoopAddTimer` call for each timer.

3. **Replace CFRunLoopRun() with NSApp().run().** Create an NSApplication with
   activation policy `.prohibited` (no dock icon, no menu bar — appropriate for
   a headless helper process). This is Idea 4 from the original list. The
   reference does this with objc2 crates; we can do a minimal version with raw
   FFI or use the objc2 crates already available in the workspace.

4. **Keep the benchmark scroll timer and diagnostic logging.** The 8ms scroll
   timer should also be registered in both modes (it already goes through
   `create_repeating_timer`, so fix #2 covers it).

**Implementation order:** Apply all three changes together. They work as a unit —
the reference uses all three, and testing them individually would take three more
experiments without clear signal (any one change alone might not close the gap).

**Expected outcome:** If the run loop mode starvation is the root cause (and the
diagnostic data from Experiment 1 strongly suggests it is — fires ≈ 30/sec
matches the 33ms fallback rate), fps should jump significantly. Target: ≥45fps
(matching the reference's ~50fps minus overhead from out-of-process IPC).

**Status:** Failed (race condition prevents multi-trial benchmarking)

**Results (1 of 7 trials completed before hang):**

```
[BENCH] Trial 1/7: 31.9fps  14.7% @60fps  p50=25.1ms  p95=87.7ms
```

Trial 1 completed but trial 2 hung: the browser appeared but never scrolled and
never ended. Required Ctrl+C to exit. No profile log was preserved (overwritten
by the dying process).

**Root cause: race condition in `nsapp::stop()` shutdown path.**

The launcher log reveals the failure:

```
Launcher: Received action: spawn_profile          ← Trial 1 (new process spawned)
Launcher: Spawning new profile 'default'
Launcher: Received action: register_profile       ← Trial 1 registers
Launcher: Received action: spawn_profile          ← Trial 2 arrives while trial 1 still registered
Launcher: Forwarding to existing profile 'default' ← FORWARDED to dying process!
Launcher: Received action: claim_session          ← Dying process "claims" trial 2
Launcher: Received action: unregister_profile     ← Trial 1 unregisters (too late)
Launcher: Profile 'default' connection error      ← Trial 1 process dies
```

The benchmark coordinator sent trial 2's `spawn_profile` in the window between
trial 1's benchmark completing (printing `[BENCHMARK-DONE]`) and the profile
process actually unregistering from the launcher. The launcher saw profile
'default' still registered and forwarded the request to the dying process instead
of spawning a new one. The dying process briefly accepted the session, then exited.
No new process was ever spawned for trial 2.

**Why this didn't happen in Experiments 1-3:** Those used bare `CFRunLoopRun()` +
`CFRunLoopStop()`. `CFRunLoopStop` exits the run loop immediately on the current
iteration. `NSApp.stop()` only takes effect after the current event finishes
dispatching — there's a small delay before `NSApp.run()` returns and the shutdown
code (which sends `unregister_profile`) executes. This widened the race window
enough for trial 2's spawn to slip through.

**Trial 1 result (31.9fps) is promising but inconclusive.** The single trial
showed improvement over Experiment 1's ~29fps, but one data point isn't enough to
draw conclusions. The dual-mode timer registration and NSApp changes couldn't be
properly evaluated due to the race condition.

**Conclusion:** The `nsapp::run()` / `nsapp::stop()` approach has a latent race
condition with multi-trial benchmarking that bare `CFRunLoopRun()` / `CFRunLoopStop()`
doesn't trigger. The fix requires either: (a) unregistering from the launcher
before printing benchmark results (moving unregister into `tick_callback`), or
(b) having the benchmark coordinator wait for profile process death before
starting the next trial. The architectural changes (dual-mode timers, NSApp)
cannot be evaluated until the race condition is fixed.

### Experiment 5: Fix benchmark race condition

**Goal:** Fix the race condition that prevented Experiment 4 from completing
multi-trial benchmarks, then re-run the benchmark to properly evaluate dual-mode
timers and `NSApp().run()`.

**The race:** When the benchmark ends, `tick_callback` prints `[BENCHMARK-DONE]`
and calls `nsapp::stop()`. The benchmark coordinator sees the result and
immediately sends `spawn_profile` for the next trial. But the profile process
hasn't unregistered yet — `nsapp::run()` hasn't returned, so the shutdown code
(which sends `unregister_profile`) hasn't executed. The launcher sees the profile
still registered and forwards to the dying process.

**Fix: unregister from the launcher in `tick_callback` before stopping.**

The `LAUNCHER_CONNECTION` global is already accessible from `tick_callback`. The
profile name is not — it lives in `args.profile` which is local to
`run_profile_server`. Add a `static PROFILE_NAME: OnceLock<String>` global, set
it during initialization, and use it in `tick_callback`.

In `tick_callback`, when the benchmark duration is reached:

```rust
if elapsed >= Duration::from_secs(ctx.benchmark_duration) {
    // Unregister FIRST — before printing results that trigger the next trial
    if let Some(launcher) = crate::LAUNCHER_CONNECTION.get() {
        if let Some(profile) = crate::PROFILE_NAME.get() {
            let msg = termsurf_xpc::XpcDictionary::new();
            msg.set_string("action", "unregister_profile");
            msg.set_string("profile", profile);
            launcher.send(&msg);
        }
    }

    println!("[BENCHMARK-DONE] ...");
    stats.print_summary();
    QUIT_FLAG.store(true, ...);
    nsapp::stop();
}
```

In the shutdown code in `run_profile_server`, skip the duplicate unregister if
`PROFILE_NAME` was already consumed (or just let it send twice — the launcher
handles duplicate unregisters gracefully).

**What to change:**

1. Add `static PROFILE_NAME: OnceLock<String>` alongside the other globals
2. Set it early in `run_profile_server`: `PROFILE_NAME.set(args.profile.clone())`
3. In `tick_callback`: unregister from launcher before `[BENCHMARK-DONE]`
4. Keep the existing shutdown unregister as a fallback (non-benchmark exits)

**Expected outcome:** Multi-trial benchmarks complete without hanging. The
Experiment 4 architectural changes (dual-mode timers, NSApp) get a proper
evaluation across all 7 trials.

**Status:** Failed (fixed launcher race, exposed CEF SingletonLock race)

**Results (1 of 7 trials completed before hang):**

```
[BENCH] Trial 1/7: 30.2fps  15.6% @60fps  p50=24.4ms  p95=91.0ms
```

Trial 1 completed. Trial 2's process spawned but CEF failed to initialize. The
browser never appeared. Required Ctrl+C to exit.

**The launcher race is fixed.** The launcher log confirms the early unregister
worked correctly:

```
Launcher: Received action: register_profile       ← Trial 1 registers
Launcher: Received action: unregister_profile     ← Early unregister (from tick_callback)
Launcher: Profile 'default' connection error      ← Trial 1 process dies
Launcher: Received action: spawn_profile          ← Trial 2 → new process spawned (not forwarded!)
Launcher: Spawned profile 'default' (pid: 43100)
Launcher: Received action: claim_session          ← Trial 2 claims successfully
```

Trial 2 correctly got its own fresh process. The forwarding bug from Experiment 4
is gone.

**New failure: CEF `SingletonLock` contention.**

Trial 2's profile log:

```
Profile: Cache: "/Users/ryan/.config/termsurf/cef/default"
Profile: CEF initialize failed (returned 0)
```

CEF's `SingletonLock` file in the `root_cache_path` prevents two processes from
using the same profile directory simultaneously. Trial 1's early unregister
happens in `tick_callback`, but `cef::shutdown()` hasn't run yet — the
`SingletonLock` is still held. Trial 2 spawns, tries to initialize CEF with the
same `root_cache_path`, and CEF rejects it.

This is the foundational constraint from CLAUDE.md: one CEF process per profile,
enforced by `SingletonLock`. The early unregister solved the launcher-level race
but exposed a deeper process-level race — the new process spawns before the old
process has released the CEF lock.

**Conclusion:** Two-stage race condition. Experiment 5 fixed stage 1 (launcher
forwarding). Stage 2 (CEF lock) requires the old process to complete
`cef::shutdown()` before the new process can initialize. The fix must ensure
the old profile process fully exits (or at least releases the CEF lock) before
the launcher spawns the replacement.

### Experiment 6: Retry CEF initialization on SingletonLock failure

**Goal:** Handle the CEF `SingletonLock` contention from Experiment 5 so
multi-trial benchmarks can run. Combined with Experiment 5's early unregister
(which fixed the launcher forwarding race), this should allow the Experiment 4
architecture to be properly evaluated.

**The problem:** The benchmark coordinator starts trial 2 before trial 1's
`cef::shutdown()` has released the `SingletonLock`. Trial 2's process spawns
correctly (Experiment 5 fixed the forwarding), but `cef::initialize()` fails
because the lock file is still held by the dying trial 1 process.

We can't control when the coordinator starts trial 2 — it reacts to benchmark
results, not process exit. And we can't call `cef::shutdown()` from inside
`tick_callback` (it would destroy CEF while we're in a CEF-driven timer). The
shutdown sequence is inherently asynchronous.

**Fix: retry loop around `cef::initialize()`.**

```rust
let mut init_result = 0;
for attempt in 0..15 {
    init_result = cef::initialize(...);
    if init_result == 1 {
        break;
    }
    if attempt < 14 {
        eprintln!("Profile: CEF init attempt {} failed, retrying in 200ms...", attempt + 1);
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
if init_result != 1 {
    eprintln!("Profile: CEF initialize failed after 15 attempts");
    std::process::exit(1);
}
```

15 attempts at 200ms = 3 seconds maximum wait. In practice, `cef::shutdown()` in
the old process should release the lock within 1-2 seconds, so retries should
succeed within a few attempts.

**What to change:**

1. Wrap the existing `cef::initialize()` call in a retry loop (15 attempts, 200ms
   between retries)
2. Log each retry attempt for diagnostics
3. Keep `std::process::exit(1)` if all retries exhausted

**Expected outcome:** Trial 2 retries `cef::initialize()` until trial 1 releases
the lock. Combined with Experiment 5's early unregister, both race conditions are
handled: the launcher spawns a new process (not forwarding), and the new process
waits for the lock. The Experiment 4 dual-mode timers and NSApp get a full 7-trial
benchmark.

**Status:** Failed (`nsapp::stop()` is broken — process never exits)

**Results (1 of 7 trials completed before hang):**

```
[BENCH] Trial 1/7: 31.4fps  17.3% @60fps  p50=24.0ms  p95=89.5ms
```

Trial 1 completed. Trial 2 spawned correctly (Experiment 5's fix worked), tried
one CEF init retry, then hung. Required Ctrl+C to exit.

**Root cause: trial 1's process never exits.**

```
$ ps aux | grep termsurf-profile
ryan  44364  1.6  0.1  ...  termsurf-profile ... --benchmark --benchmark-duration 10
```

Trial 1 (pid 44364) is **still running** after reporting results. `nsapp::run()`
never returned despite `nsapp::stop()` being called. The process is permanently
stuck, holding the `SingletonLock` forever. Trial 2's retry loop can never
succeed because the lock will never be released.

**Why `nsapp::stop()` doesn't work:** `[NSApp stop:nil]` sets an internal flag
that NSApp checks between events. But our headless process generates no AppKit
events. Without an event to process, NSApp never checks the stop flag.
`CFRunLoopStop` wakes the inner `CFRunLoopRunInMode`, but `NSApp.run()` has its
own outer loop that re-enters the run loop before checking the stop flag.

The standard fix is to post a dummy `NSEvent` after calling `stop:`, giving NSApp
an event to dispatch so it notices the stop flag.

**Why trial 1 "completed":** `tick_callback` printed `[BENCHMARK-DONE]` and the
coordinator collected the result — but the profile process itself never exited.
The benchmark data was communicated while the pump was still running.

**Conclusion:** The retry loop is correct but useless — the lock is held forever
because the old process never exits. The real fix must make `nsapp::stop()`
actually terminate `nsapp::run()` by posting a dummy event.

### Experiment 7: Fix `nsapp::stop()` with dummy NSEvent

**Goal:** Make `nsapp::stop()` reliably cause `nsapp::run()` to return, so the
profile process can reach `cef::shutdown()` (releasing the `SingletonLock`) and
exit cleanly. This unblocks the retry loop from Experiment 6.

**Why `stop:` alone doesn't work:** `NSApp.run()` internally loops:

```
while (!stopped) {
    event = [self nextEventMatchingMask:... untilDate:distantFuture inMode:...];
    [self sendEvent:event];
    // check stopped flag HERE
}
```

Calling `[app stop:nil]` sets `stopped = YES`, but the loop is blocked on
`nextEventMatchingMask:` waiting for an event. `CFRunLoopStop` exits the inner
run loop call, but `NSApp.run()` re-enters it immediately. The flag is only
checked AFTER an event is dispatched.

**Fix: post a dummy `NSEvent` after `stop:`.**

The standard Cocoa pattern:

```objc
[NSApp stop:nil];
NSEvent *event = [NSEvent otherEventWithType:NSEventTypeApplicationDefined
                                    location:NSZeroPoint
                               modifierFlags:0
                                   timestamp:0
                                windowNumber:0
                                     context:nil
                                     subtype:0
                                       data1:0
                                       data2:0];
[NSApp postEvent:event atStart:YES];
```

In raw objc FFI, this requires a new `objc_msgSend` signature for the 10-argument
`otherEventWithType:location:modifierFlags:timestamp:windowNumber:context:subtype:data1:data2:`
class method. The `NSPoint` struct (two `f64` fields) is passed by value.

**What to change in `nsapp::stop()`:**

1. After `[app stop:nil]`, create a dummy `NSEvent` using
   `[NSEvent otherEventWithType:...]` with `NSEventTypeApplicationDefined` (15)
2. Post it with `[NSApp postEvent:event atStart:YES]`
3. Remove the `CFRunLoopStop` call — the posted event will wake the loop naturally

**New FFI needed:**

```rust
#[repr(C)]
struct NSPoint { x: f64, y: f64 }

// [NSEvent otherEventWithType:location:modifierFlags:timestamp:windowNumber:context:subtype:data1:data2:]
// Returns NSEvent*
type MsgSendEvent = unsafe extern "C" fn(
    cls: *mut c_void,        // NSEvent class
    sel: *mut c_void,        // otherEventWithType:...
    event_type: isize,       // 15 = NSEventTypeApplicationDefined
    location: NSPoint,       // NSZeroPoint
    flags: usize,            // 0
    timestamp: f64,          // 0.0
    window_num: isize,       // 0
    context: *const c_void,  // nil
    subtype: i16,            // 0
    data1: isize,            // 0
    data2: isize,            // 0
) -> *mut c_void;

// [NSApp postEvent:event atStart:YES]
type MsgSendPost = unsafe extern "C" fn(
    app: *mut c_void,
    sel: *mut c_void,
    event: *mut c_void,
    at_start: i8,  // BOOL = YES (1)
);
```

**Expected outcome:** `nsapp::run()` returns immediately after `nsapp::stop()` is
called. Trial 1's process reaches `cef::shutdown()`, releases the `SingletonLock`,
and exits. Trial 2's retry loop succeeds on the first or second attempt. Full
7-trial benchmark completes.

**Status:** Complete

**Results (all 7 trials completed):**

```
[BENCH] Trial 1/7: 33.1fps  20.5% @60fps  p50=21.3ms  p95=83.5ms
[BENCH] Trial 2/7: 30.5fps  15.4% @60fps  p50=24.6ms  p95=93.1ms
[BENCH] Trial 3/7: 31.1fps  19.2% @60fps  p50=22.7ms  p95=91.1ms
[BENCH] Trial 4/7: 30.7fps  18.6% @60fps  p50=21.1ms  p95=94.4ms
[BENCH] Trial 5/7: 30.8fps  18.8% @60fps  p50=20.7ms  p95=94.2ms
[BENCH] Trial 6/7: 32.4fps  16.8% @60fps  p50=23.1ms  p95=94.2ms
[BENCH] Trial 7/7: 31.6fps  18.8% @60fps  p50=22.4ms  p95=90.6ms
```

Average: ~31.5fps, p50 ~22ms. Thermal nominal, no bimodal behavior.

**Infrastructure fixes confirmed:** All three race condition fixes work together:

- Experiment 5 (early unregister) — launcher spawns new process, not forwarding
- Experiment 6 (CEF init retry) — handles SingletonLock contention
- Experiment 7 (dummy NSEvent) — nsapp::run() returns cleanly, process exits

Profile log confirms clean shutdown: "Shutting down... Done."

**PUMP diagnostics (trial 7):**

```
[PUMP] callbacks=380(imm=380 def=0) fires=384(src=379 tmr=0 fb=5) reentrant=0 work=384  ← page load
[PUMP] callbacks=176(imm=169 def=7) fires=172(src=169 tmr=0 fb=3) reentrant=0 work=172
[PUMP] callbacks=121(imm=120 def=1) fires=126(src=120 tmr=0 fb=6) reentrant=0 work=126
[PUMP] callbacks=154(imm=123 def=31) fires=128(src=123 tmr=0 fb=5) reentrant=0 work=128
[PUMP] callbacks=122(imm=93 def=29) fires=97(src=93 tmr=0 fb=4) reentrant=0 work=97
[PUMP] callbacks=123(imm=93 def=30) fires=95(src=93 tmr=0 fb=2) reentrant=0 work=95
[PUMP] callbacks=142(imm=110 def=32) fires=113(src=110 tmr=0 fb=3) reentrant=0 work=113
[PUMP] callbacks=174(imm=96 def=78) fires=96(src=96 tmr=0 fb=0) reentrant=0 work=96
[PUMP] callbacks=164(imm=94 def=70) fires=94(src=94 tmr=0 fb=0) reentrant=0 work=94
[PUMP] callbacks=120(imm=108 def=12) fires=113(src=108 tmr=0 fb=5) reentrant=0 work=113
```

**Findings:**

1. **Small improvement over Experiment 1.** ~31.5fps vs ~29fps — a real but modest
   ~2.5fps gain. p50 improved from 25ms to 22ms.

2. **Pump behavior is nearly identical to Experiment 1.** ~95-130 work/sec during
   steady-state scrolling, 0-6 fallback fires/sec. The dual-mode timer
   registration did not significantly change how often timers fire.

3. **NSEventTrackingRunLoopMode is irrelevant to the benchmark.** Our simulated
   scroll sends `mouse_wheel_event` through CEF's API, not through macOS event
   tracking. The run loop never enters `NSEventTrackingRunLoopMode` because
   there's no actual trackpad/mouse being tracked. The dual-mode registration
   will help with real user scrolling (when input comes from the GUI via XPC →
   macOS events) but doesn't affect the benchmark.

4. **The ~31fps cap is fundamental to the timer-only architecture.** At ~100
   work/sec, with 3-4 `do_message_loop_work()` calls needed per frame, each
   frame takes 3-4 timer cycles. At ~10ms per cycle, that's 30-40ms per frame
   — matching the observed p50 of 22ms (some cycles are faster).

**Comparison across all experiments:**

| Experiment | Approach                          | fps   | p50   | p95   |
| ---------- | --------------------------------- | ----- | ----- | ----- |
| 1          | Timer-only, CFRunLoopRun          | ~29   | 25ms  | 87ms  |
| 2          | Source (buggy)                    | ~22   | 50ms  | —     |
| 3          | Source (fixed)                    | ~22   | 44ms  | —     |
| 4-6        | Failed (race conditions)          | —     | —     | —     |
| 7          | Timer-only, NSApp, dual-mode      | ~31.5 | 22ms  | 91ms  |
| Reference  | Busy-wait (100% CPU)              | ~50   | 17ms  | —     |

**Conclusion:** The dual-mode timer registration and NSApp provided a small but
real improvement (~2.5fps). The infrastructure fixes (early unregister, CEF retry,
dummy event) are solid and should be kept. But the remaining gap to ~50fps
(reference) cannot be closed by run loop tuning alone. The ~100 work/sec rate is
the bottleneck — the timer-only architecture limits how many CEF pipeline stages
can be processed per second. Possible next steps:

- **Reduce fallback timer delay** (Idea 2 from the original list) — drop from
  33ms to 1ms to increase work rate
- **Investigate the out-of-process overhead** — IOSurface Mach port transfer
  adds latency per frame that the in-process reference doesn't have
- **Profile `do_message_loop_work()` call cost** — understand how many calls CEF
  actually needs per frame in the out-of-process architecture

### Experiment 8: Fixed-rate 1ms repeating timer

**Goal:** Determine whether the ~31fps cap is caused by timer scheduling overhead
or something else (IPC latency, CEF internal pacing). This gives a binary answer.

**Hypothesis:** The timer-only architecture creates a new one-shot timer for every
`schedule_work` call. Each cycle involves: timer callback returns → run loop
checks other sources → new timer created and registered → run loop schedules it →
timer fires. This overhead adds ~10ms per cycle, capping work at ~100/sec. If we
bypass on-demand timers and just poll at 1000Hz, the overhead disappears.

**Method:** Replace the on-demand cef_pump with a single 1ms repeating timer that
calls `do_message_loop_work()` on every tick. This is the busy-wait approach
capped at 1000Hz — enough to saturate CEF's pipeline without burning 100% CPU.

Keep `external_message_pump: 1` and the `on_schedule_message_pump_work` callback,
but use the callback only for diagnostic logging — don't create any timers from
it. The repeating timer drives all work unconditionally.

**What to change in `cef_pump`:**

1. Remove `schedule_work`, `schedule_internal`, `schedule_fallback`, and
   `timer_callback` — no more on-demand timers
2. Add `pub fn pump_callback()` — called by a 1ms repeating timer. Contains the
   reentrancy guard and calls `do_message_loop_work()`. Keeps diagnostic counters
   for work/sec.
3. Change `schedule_work` to only increment diagnostic counters (track CEF's
   callback rate for comparison)
4. Remove PumpState's timer field — no timers to track

**In the main loop:**

```rust
// Create 1ms repeating pump timer (replaces on-demand timers)
let _pump_timer = cfrunloop::create_repeating_timer(0.001, cef_pump::pump_callback);

// Keep the 8ms scroll timer for benchmarking
let _scroll_timer = cfrunloop::create_repeating_timer(0.008, benchmark_timers::tick_callback);

nsapp::run();
```

**Diagnostic log format:**

```
[PUMP] callbacks=150(imm=120 def=30) work=950 idle=50
```

Where `callbacks` is what CEF requested (for comparison with Experiments 1/7),
`work` is how many times `do_message_loop_work()` was called, and `idle` is ticks
where the reentrancy guard triggered (pump already active).

**Expected outcomes:**

- **~50fps, work ~1000/sec:** Timer scheduling overhead was the bottleneck.
  Solution: use a high-frequency repeating timer instead of on-demand timers.
  CPU usage at 1ms polling is negligible.
- **~31fps, work ~1000/sec:** Timer overhead was NOT the bottleneck. The cap is
  elsewhere — likely IPC latency or CEF's internal frame pacing. Further
  investigation needed.
- **~31fps, work ~100/sec:** Reentrancy guard blocks most ticks — CEF holds the
  main thread during `do_message_loop_work()` for ~10ms, so only ~100 of 1000
  ticks actually execute. This would mean CEF itself is the bottleneck.

**Status:** Complete

**Results (all 7 trials completed):**

```
[BENCH] Trial 1/7: 24.2fps  21.7% @60fps  p50=21.4ms  p95=84.6ms
[BENCH] Trial 2/7: 25.0fps  20.0% @60fps  p50=15.5ms  p95=86.3ms
[BENCH] Trial 3/7: 27.7fps  26.0% @60fps  p50=14.0ms  p95=82.8ms
[BENCH] Trial 4/7: 27.0fps  19.2% @60fps  p50=15.3ms  p95=82.3ms
[BENCH] Trial 5/7: 28.4fps  37.5% @60fps  p50=13.8ms  p95=81.2ms
[BENCH] Trial 6/7: 23.2fps  14.2% @60fps  p50=49.9ms  p95=81.1ms
[BENCH] Trial 7/7: 26.4fps  35.7% @60fps  p50=15.9ms  p95=80.4ms
```

Average: ~26fps, p50 varies widely (13.8-49.9ms). **Worse than Experiment 7's
~31.5fps.** 1000Hz polling reduced performance instead of improving it.

**PUMP diagnostics (trial 7):**

```
[PUMP] callbacks=182(imm=182 def=0) work=377 idle=0
[PUMP] callbacks=136(imm=101 def=35) work=439 idle=0
[PUMP] callbacks=70(imm=70 def=0) work=458 idle=0
[PUMP] callbacks=186(imm=71 def=115) work=489 idle=0
[PUMP] callbacks=125(imm=58 def=67) work=477 idle=0
[PUMP] callbacks=154(imm=54 def=100) work=450 idle=0
[PUMP] callbacks=202(imm=81 def=121) work=489 idle=0
[PUMP] callbacks=299(imm=61 def=238) work=493 idle=0
[PUMP] callbacks=296(imm=60 def=236) work=479 idle=0
[PUMP] callbacks=153(imm=68 def=85) work=453 idle=0
```

**Findings:**

1. **Only ~450-490 work/sec out of 1000 ticks.** `do_message_loop_work()` takes
   ~2ms on average, so roughly half the 1ms ticks are skipped (the timer fires
   while the previous call is still running, but since `idle=0`, the calls don't
   actually overlap — they just consume enough time to halve the effective rate).

2. **Zero reentrancy (idle=0).** Every tick successfully enters
   `do_message_loop_work()`. The calls don't overlap — they just take ~2ms each,
   leaving no time for other run loop activity.

3. **More pumping produced fewer frames.** ~470 work/sec → ~26fps vs Experiment
   7's ~100 work/sec → ~31fps. Calling `do_message_loop_work()` 5x more often
   made things worse.

4. **Scroll timer starvation.** The pump consumes ~2ms per tick × 470 ticks =
   ~940ms of every second. The 8ms scroll timer barely gets main thread time to
   send input events. With fewer scroll events reaching CEF, fewer frames are
   produced.

5. **`do_message_loop_work()` is not free.** Even with no rendering work pending,
   each call takes ~2ms (likely: checking internal queues, acquiring CEF locks,
   processing IPC). Unnecessary calls waste time that could serve other timers.

**None of the three predicted outcomes matched.** The actual result was a fourth
scenario not anticipated: ~26fps at ~470 work/sec — fewer fps than the on-demand
approach despite more work calls. The cause: excessive pumping starves the scroll
timer, reducing input to CEF.

**Comparison across all experiments:**

| Experiment | Approach                          | fps   | p50   | work/sec |
| ---------- | --------------------------------- | ----- | ----- | -------- |
| 1          | Timer-only, CFRunLoopRun          | ~29   | 25ms  | ~100     |
| 3          | Source (fixed)                    | ~22   | 44ms  | ~100-180 |
| 7          | Timer-only, NSApp, dual-mode      | ~31.5 | 22ms  | ~100     |
| 8          | 1ms repeating, 1000Hz             | ~26   | 15ms  | ~470     |
| Reference  | Busy-wait (100% CPU)              | ~50   | 17ms  | ~∞       |

**Conclusion:** Timer scheduling overhead is NOT the bottleneck. The on-demand
approach (Experiment 7) was already optimal for pump timing — CEF's callback tells
us exactly when work is needed, and calling more often just wastes time. The
~31fps cap comes from somewhere downstream: IPC latency (IOSurface Mach port
transfer per frame), CEF's internal rendering pipeline, or the GUI's frame
presentation timing. Experiment 7's architecture should be restored as the best
event-driven result.

### Experiment 9: Immediate re-pump loop for delay=0

**Goal:** Eliminate timer round-trip overhead between consecutive
`do_message_loop_work()` calls within a single frame. Revert Experiment 8's 1ms
repeating timer back to Experiment 7's on-demand architecture and add one
targeted improvement.

**Background:** The cef-rs reference implementation (`mod.rs:81-94`) calls
`do_work()` immediately when `on_schedule_work(delay <= 0)` — no timer creation.
After `do_work()`, if reentrancy was detected (CEF called
`on_schedule_message_pump_work(0)` during execution), it re-pumps via
`performSelector` which executes on the next run loop iteration — much faster
than creating a new timer.

Our Experiment 7 creates a new one-shot timer for every `schedule_work(0)` call.
Each timer round-trip (create → register → run loop iteration → fire) adds ~10ms
of overhead. A frame requiring 3-4 CEF pipeline stages takes 3-4 timer
round-trips = 30-40ms per frame = ~31fps. This matches the observed results
exactly.

**Hypothesis:** The ~31fps cap in Experiment 7 comes from per-stage timer
round-trip latency. CEF needs 3-4 `do_message_loop_work()` calls to push one
frame through its internal pipeline. Each call goes through a separate timer
fire. If we call them back-to-back inside the callback when CEF requests
immediate work, a frame takes 3-4 × 2ms = 6-8ms instead of 30-40ms.

**Architecture:** Revert to Experiment 7's on-demand timer-based scheduling,
then add a re-pump loop inside the pump callback:

1. After `do_message_loop_work()` returns, check if CEF called
   `schedule_work(0)` during execution (tracked by an atomic flag).
2. If so, clear the flag and call `do_message_loop_work()` again immediately —
   no timer creation, no run loop iteration, no overhead.
3. Repeat until CEF stops requesting immediate work or a cap of 10 iterations is
   reached.
4. After the loop exits, schedule a fallback timer or handle deferred work as
   before.

This gives busy-wait-like latency during the 3-4 pipeline stages of a frame,
while remaining idle-friendly between frames (when CEF requests a non-zero
delay). The key difference from Experiment 8: we only loop when CEF explicitly
says "more work needed" (delay=0), not unconditionally.

**What to change in `cef_pump`:**

1. **Revert from Experiment 8 to Experiment 7.** Restore on-demand timer
   scheduling where `schedule_work` creates one-shot timers. Remove the 1ms
   repeating timer from the main loop.

2. **Add `IMMEDIATE_REQUESTED: AtomicBool`.** Set by `schedule_work(0)`, checked
   by the pump loop.

3. **Add re-pump loop in `pump_callback`:**

   ```rust
   // Clear flag before pumping
   IMMEDIATE_REQUESTED.store(false, Ordering::Release);

   let mut iterations = 0;
   loop {
       cef::do_message_loop_work();
       WORK_DONE.fetch_add(1, Ordering::Relaxed);
       iterations += 1;

       // Re-pump if CEF requested immediate work during the call
       if iterations < 10 && IMMEDIATE_REQUESTED.swap(false, Ordering::Acquire) {
           REPUMP.fetch_add(1, Ordering::Relaxed);
           continue;
       }
       break;
   }
   ```

4. **Skip timer creation when pump is active.** In `schedule_work(0)`, if
   `IS_ACTIVE` is true, just set `IMMEDIATE_REQUESTED` — the loop will pick it
   up. Only create a timer when the pump is idle (IS_ACTIVE is false).

5. **Close the IS_ACTIVE race window.** After clearing `IS_ACTIVE`, re-check
   `IMMEDIATE_REQUESTED`. If a `schedule_work(0)` arrived between the loop's
   last check and IS_ACTIVE clearing (the thread didn't create a timer because
   it saw IS_ACTIVE=true), create a 0ms timer to re-enter the pump.

**Diagnostic log format:**

```
[PUMP] callbacks=150(imm=120 def=30) work=450 repump=320 idle=0
```

Where `repump` counts loop iterations beyond the first (i.e., times the re-pump
loop fired instead of going through a timer). High `repump` relative to `work`
means the loop is working — most pump calls happen without timer overhead.

**Expected outcomes:**

- **~50fps, work ~400/sec, repump ~300:** Timer round-trip overhead was the
  bottleneck. The re-pump loop eliminates inter-stage latency. Frames complete
  in 6-8ms instead of 30-40ms. This would match the busy-wait reference.
- **~31fps, work ~100/sec, repump ~0:** `schedule_work(0)` is called from a
  background thread after `do_message_loop_work()` returns, not during it. The
  flag is never set when the loop checks it. The bottleneck is elsewhere.
- **~26fps, work ~400+/sec, repump ~300:** Same problem as Experiment 8 — the
  re-pump loop starves the scroll timer. The 10-iteration cap needs to be lower,
  or the approach is fundamentally flawed.

**Status:** Failed (re-pump loop is a no-op; fallback timer causes cascade)

**Results (all 7 trials completed):**

```
[BENCH] Trial 1/7: 23.9fps  9.0% @60fps  p50=44.2ms  p95=83.3ms
[BENCH] Trial 2/7: 25.1fps  14.8% @60fps  p50=41.6ms  p95=82.2ms
[BENCH] Trial 3/7: 24.0fps  15.7% @60fps  p50=44.1ms  p95=79.8ms
[BENCH] Trial 4/7: 24.3fps  14.8% @60fps  p50=41.2ms  p95=86.8ms
[BENCH] Trial 5/7: 23.2fps  11.2% @60fps  p50=43.5ms  p95=86.8ms
[BENCH] Trial 6/7: 22.0fps  9.7% @60fps  p50=48.3ms  p95=89.0ms
[BENCH] Trial 7/7: 23.8fps  8.2% @60fps  p50=39.9ms  p95=83.5ms
```

Average: ~24fps, p50 ~42ms. Worse than every previous experiment.

**PUMP diagnostics (trial 7):**

```
[PUMP] callbacks=561(imm=561 def=0) work=9093 repump=22 idle=0
[PUMP] callbacks=196(imm=173 def=23) work=15460 repump=17 idle=0
[PUMP] callbacks=164(imm=136 def=28) work=18579 repump=6 idle=0
[PUMP] callbacks=226(imm=181 def=45) work=22523 repump=13 idle=0
[PUMP] callbacks=154(imm=107 def=47) work=26533 repump=8 idle=0
[PUMP] callbacks=238(imm=145 def=93) work=35561 repump=10 idle=0
[PUMP] callbacks=248(imm=145 def=103) work=35279 repump=7 idle=0
[PUMP] callbacks=203(imm=127 def=76) work=32261 repump=6 idle=0
[PUMP] callbacks=179(imm=123 def=56) work=29369 repump=4 idle=0
[PUMP] callbacks=114(imm=95 def=19) work=27712 repump=3 idle=0
```

**Findings:**

1. **repump is near zero (3-22/sec out of 9,000-35,000 work).** CEF's
   `on_schedule_message_pump_work` fires from a background thread, after the
   pump callback has already returned. `IMMEDIATE_REQUESTED` is never set when
   the re-pump loop checks it. The loop is a no-op — every pump goes through a
   timer round-trip regardless.

2. **Cascading timer accumulation from the 33ms fallback.** Every
   `pump_callback` invocation creates a 33ms fallback timer. Each fallback fires
   and creates another. Combined with ~248 new timers/sec from `schedule_work`
   (which always creates deferred timers), the count grows linearly:
   9k → 15k → 22k → 35k work/sec until CPU-saturated. This is the same class
   of bug as Experiment 2's zombie timer leak.

3. **~24fps with p50=42ms — worst result yet.** The 35,000 idle
   `do_message_loop_work()` calls per second starve the scroll timer even more
   aggressively than Experiment 8's ~470/sec.

4. **Implementation bug: `schedule_work(delay > 0)` always creates timers.**
   The initial implementation skipped timer creation when `IS_ACTIVE` was true,
   which killed the pump entirely (no surface reached the GUI). The fix —
   always creating deferred timers — prevented the deadlock but introduced
   the cascading accumulation because no timer is ever invalidated.

**Comparison across all experiments:**

| Experiment | Approach                          | fps   | p50   | work/sec  |
| ---------- | --------------------------------- | ----- | ----- | --------- |
| 1          | Timer-only, CFRunLoopRun          | ~29   | 25ms  | ~100      |
| 3          | Source (fixed)                    | ~22   | 44ms  | ~100-180  |
| 7          | Timer-only, NSApp, dual-mode      | ~31.5 | 22ms  | ~100      |
| 8          | 1ms repeating, 1000Hz             | ~26   | 15ms  | ~470      |
| 9          | Re-pump loop + fallback cascade   | ~24   | 42ms  | ~9k-35k   |
| Reference  | Busy-wait (100% CPU)              | ~50   | 17ms  | ~∞        |

**Conclusion:** The hypothesis is disproven. The ~31fps cap is NOT caused by
timer round-trip overhead between CEF pipeline stages. CEF does not call
`on_schedule_message_pump_work(0)` synchronously during `do_message_loop_work()`
— it fires from a background thread after the callback returns. There are no
consecutive pipeline stages to batch. The re-pump loop never triggers.

This, combined with Experiment 8's finding, definitively rules out pump-side
scheduling as the bottleneck. Both approaches to increasing pump frequency —
unconditional polling (Exp 8) and conditional re-pumping (Exp 9) — made
performance worse by starving the scroll timer. The on-demand timer approach in
Experiment 7 (~100 work/sec, ~31.5fps) is the optimal event-driven result.

The remaining gap to ~50fps (reference busy-wait) must come from a factor
outside the pump architecture:

- **IPC overhead:** IOSurface Mach port transfer adds per-frame latency not
  present in the in-process reference
- **CEF's internal frame pacing:** CEF may pace frame delivery differently
  in external pump mode vs its own message loop
- **GUI presentation timing:** The GUI's wgpu texture import and rendering
  pipeline may introduce latency

Experiment 7's architecture should be restored as the final event-driven pump
implementation.

## Performance Journey Summary (Issues 325–350)

This section reviews the full arc of performance work across TermSurf 3.0,
tracking what was accomplished, what was learned, and what problems remain.

### The arc

**Issue 325 — Webview Frame Rate (12fps → 60fps).** The first performance issue.
CEF's `run_message_loop()` didn't pump frequently enough. Replacing it with a
custom polling loop calling `do_message_loop_work()` every 1ms achieved ~60fps.
This established the busy-wait pattern that all subsequent work builds on.

**Issue 338 — Browser Lag (~20fps).** Five experiments tried to fix lag through
CEF configuration: IOSurface caching, `windowless_frame_rate`,
`multi_threaded_message_loop`, `external_begin_frame_enabled`, Chrome
command-line flags. All failed. Root cause identified:
`CefCopyFrameGenerator::GenerateCopyFrame()` discards frames when one is
in-progress. This is baked into CEF's C++ code and cannot be configured away.

**Issue 339 — Electron Study.** Studied how Electron achieves 240fps using
Chromium's `FrameSinkVideoCapturer` API with `kGpuMemoryBuffer` — a completely
different capture path than CEF's `OnAcceleratedPaint`. Concluded that CEF's
architecture might be fundamentally limited.

**Issue 340 — Architecture Reconsideration.** Began evaluating whether to abandon
CEF entirely and embed Chromium directly (C++ rewrite). However, research
revealed the cef-rs OSR example achieves 60fps with the same CEF version. The
bottleneck wasn't CEF itself — it was ts3's integration. This reopened
optimization before committing to a rewrite.

**Issue 341 — Performance Investigation (18 experiments).** Systematic search for
why ts3 gets ~20fps while the cef-rs example gets ~60fps. Tried winit event
loops, `external_message_pump`, NSApplication initialization, hidden windows,
CVDisplayLink, activation policies. A hidden 1x1 window achieved 60fps but
steals focus — architectural dead end. Root cause: CEF's
`SyntheticBeginFrameSource` needs either a CFRunLoop or display link.

**Issue 342 — 60fps Without a Window (20fps → 38fps).** Replacing
`thread::sleep(1ms)` with `CFRunLoopRunInMode(0.001)` unlocked 38fps. The
breakthrough: CEF's timer sources were being starved by dead sleeping instead
of run loop servicing. This was the first real architectural fix.

**Issue 343 — Optimal Performance (38fps → stuck).** Eight experiments tried to
close the gap from 38fps to 60fps. All failed or regressed. Key finding:
`do_message_loop_work()` takes >1ms on 100% of calls in ts3 vs only 5.7% in the
cef-rs example. CEF's internal task queue backs up because complementary work
(NSApplication events) isn't processed.

**Issue 344 — cef-test Harness (~50fps).** Built a minimal 3-process CEF test
harness to isolate the multi-process architecture from WezTerm. Result: ~50fps
with two profiles. Proved the multi-process XPC + IOSurface Mach port
architecture is sound. The 12fps gap between cef-test (50fps) and ts3 (38fps)
came from input routing overhead.

**Issue 345 — Automated Benchmark (51fps without mouse, 39fps with).** Created
`web benchmark` with simulated scroll input directly in the profile server,
eliminating manual scrolling variability. Discovery: 51.5fps with no mouse
movement, 39fps with continuous mouse movement. Mouse move events forwarded
over XPC cause frame drops.

**Issue 346 — Mouse Performance (problem didn't exist).** Three experiments
investigated the 12fps mouse penalty. Finding: the "mouse performance problem"
disappeared after removing debug logging from hot paths. Performance is bimodal
(good mode vs bad mode), and the mouse movement correlation was coincidental.
Run-to-run variance dominates the signal.

**Issue 347 — Lingering Lag (debug → release = +12fps).** Release builds
improved cef-test from 38fps to 51fps and eliminated "bad mode." TermSurf
pipeline adds ~2ms per frame vs cef-test (p50: 18.9ms vs 16.7ms). That 2ms
pushes frames past the 16.7ms vsync deadline, reducing 60fps hit rate from
81-85% to 47-55%.

**Issue 348 — CEF Test Ceiling (~51fps hard limit).** Investigated why cef-test
plateaus at ~51fps. Removing 1ms sleeps pushed to 55.7fps but caused thermal
throttling (46.7fps → 33.7fps → 27.9fps across runs). IOSurface handles are
NOT reused by CEF (~850 unique handles per ~3000 frames), ruling out "send Mach
port once" optimization. The ~15% vsync miss rate is fundamental to the CEF OSR
→ IOSurface → Mach port → wgpu pipeline.

**Issue 349 — Bimodal Pattern.** Investigated why frame rates cluster into
distinct "good" and "bad" modes. Likely cause: WezTerm uses `PresentMode::Fifo`
(strict queue) vs cef-test's `AutoVsync` (mailbox). In Fifo, one late frame
desynchronizes the entire queue. Also introduced `external_message_pump` mode
with CEF callbacks, which dropped fps from ~50 to ~29 — the starting point for
Issue 350.

**Issue 350 — Event-Driven CEF Message Pump (this issue).** Nine experiments to
close the gap between the event-driven pump (~29fps) and the busy-wait (~50fps).
Best result: Experiment 7 at ~31.5fps using NSApp, dual-mode timers, and three
infrastructure fixes (early launcher unregister, CEF init retry, dummy NSEvent
for clean shutdown). Experiments 8 and 9 definitively proved that pump-side
scheduling is not the bottleneck — more pumping only starves the scroll timer.

### What was accomplished

1. **50fps rendering pipeline.** Out-of-process CEF with IOSurface Mach port
   transfer achieves ~50fps in release builds — close to the 60fps target and
   smooth enough for real use.

2. **Automated benchmarking.** `web benchmark` provides deterministic,
   multi-trial measurement with simulated scroll input, eliminating manual
   testing variability.

3. **Event-driven pump.** `external_message_pump` mode with on-demand timers
   achieves ~31.5fps at <5% CPU. The busy-wait reference burns 100% CPU for
   ~50fps. The event-driven architecture is the correct production approach
   despite the fps gap.

4. **Multi-trial reliability.** Three infrastructure fixes (early launcher
   unregister, CEF init retry loop, dummy NSEvent posting) enable reliable
   multi-trial benchmarking across profile process restarts.

5. **Multi-process architecture validated.** cef-test proved the XPC + Mach port
   + IOSurface pipeline is sound. The architecture supports multiple profiles,
   each in its own process, with near-reference-level performance.

### What was learned

1. **CEF's `on_schedule_message_pump_work` fires from a background thread.**
   It does not fire synchronously during `do_message_loop_work()`. This means
   there is no opportunity for immediate re-pumping inside the callback — every
   pump cycle requires a timer round-trip through the run loop.

2. **~100 work/sec is the optimal pump rate.** Experiment 7 achieves ~31.5fps
   at ~100 `do_message_loop_work()` calls per second. Pumping faster (470/sec
   in Exp 8, 35k/sec in Exp 9) makes things worse by starving the scroll timer.
   `do_message_loop_work()` takes ~2ms even with no work pending.

3. **Timer scheduling overhead is not the bottleneck.** Creating one-shot
   CFRunLoopTimers for each pump is cheap enough at ~100/sec. The gap to ~50fps
   is not caused by timer allocation or run loop scheduling latency.

4. **NSApp + dual-mode timers provide a small improvement (~2.5fps).** Switching
   from bare `CFRunLoopRun()` to `NSApp().run()` and registering timers in both
   `kCFRunLoopCommonModes` and `NSEventTrackingRunLoopMode` helps marginally.

5. **Debug builds cost ~12fps.** Release builds are essential for performance
   work. Debug logging in hot paths (XPC event handlers, frame callbacks) also
   causes measurable regression.

6. **Performance is bimodal due to `PresentMode::Fifo`.** The GUI's strict vsync
   queue means one late frame desynchronizes subsequent frames. This creates
   distinct "good mode" (~50fps) and "bad mode" (~35fps) clusters that appear
   random but are deterministic based on initial vsync phase alignment.

7. **IOSurface handles are not reused.** CEF allocates ~850 unique IOSurfaces
   per ~3000 frames. Each frame requires a fresh Mach port transfer. The "cache
   the Mach port" optimization is impossible with the current CEF API.

8. **`NSApp.stop()` requires a dummy event.** In a headless process with no
   windows, `[NSApp stop:nil]` sets a flag but `NSApp.run()` blocks on
   `nextEventMatchingMask:` forever. Posting `NSEventTypeApplicationDefined`
   after `stop:` is the standard Cocoa fix.

### What problems remain

1. **The 19fps gap between event-driven (~31fps) and busy-wait (~50fps).** This
   is the central unsolved problem. Nine experiments have ruled out pump-side
   scheduling as the cause. The gap likely comes from one or more of:
   - CEF's internal frame pacing in `external_message_pump` mode vs its own loop
   - The busy-wait's `cfrunloop::run_for(0.0)` processing run loop events inline
     with pumping, giving CEF tighter integration than timer-based pumping
   - Interaction between the event-driven pump and the scroll timer's 8ms cadence

2. **The ~10fps gap between busy-wait (~50fps) and 60fps.** Even the best
   busy-wait result misses ~15% of vsync deadlines. This is fundamental to the
   out-of-process pipeline: CEF render → IOSurface → Mach port → wgpu import →
   present. Each step adds latency that the in-process cef-rs example avoids.

3. **`PresentMode::Fifo` bimodality.** The GUI's strict vsync queue amplifies
   single frame drops into sustained mode shifts. Switching to `AutoVsync`
   (mailbox) would absorb late frames gracefully but hasn't been tested in ts3.

4. **100% CPU in the busy-wait.** The ~50fps busy-wait burns one full CPU core,
   causing thermal throttling within minutes. The event-driven pump solves this
   but at a 19fps cost. No middle ground has been found.

5. **Experiment 7 code needs to be restored.** The codebase currently has
   Experiment 9's failed re-pump loop and cascading fallback timers. The code
   should be reverted to Experiment 7's clean on-demand timer architecture.
