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
