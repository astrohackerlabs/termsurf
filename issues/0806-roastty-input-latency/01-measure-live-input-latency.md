# Experiment 1: Measure live input latency

## Description

This experiment will reproduce the current Roastty interactive input delay and
localize it with timestamped evidence before any behavioral fix is attempted.
The user observed roughly 50 seconds between typing and visible terminal
response in `ReleaseLocal`; this experiment treats that as the primary bug to
measure.

The experiment is measurement-only. It may add temporary or gated
instrumentation and a reusable latency harness, but it must not change terminal
behavior, render policy, event scheduling, PTY semantics, or configuration
defaults. If the measurement itself reveals an obvious fix, that fix belongs in
a later experiment after this one records the root-cause evidence.

The main hypothesis to test is that input reaches AppKit/Rust quickly, then
stalls downstream in termio event draining, display-link/app tick scheduling,
render/present work, or lock contention. The experiment must also be prepared to
disprove that hypothesis if the delay occurs earlier.

## Changes

- Add low-overhead, opt-in timestamp instrumentation for the input-to-render
  pipeline. Reuse `ROASTTY_UI_KEY_TRACE_PATH` where practical, but include
  monotonic timestamps so adjacent events can be compared.
- Instrument these milestones:
  - Swift `keyDown` entry and return from `roastty_surface_key`;
  - Rust `roastty_surface_key`, key encoding, and worker queueing;
  - termio worker receipt of queued writes;
  - termio pump events reporting bytes written and bytes read;
  - `roastty_app_tick` / `tick_termio` event draining;
  - `present_live` begin and end, including duration;
  - present driver tick cadence and whether CoreVideo display-link or fallback
    thread is active.
- Add a focused script under `scripts/roastty-app/` that:
  - builds the current `ReleaseLocal` Roastty app with
    `roastty/macos/build.nu --configuration ReleaseLocal`;
  - launches the exact `ReleaseLocal` app bundle path by setting `ROASTTY_APP`
    or by using an explicit ReleaseLocal-aware launch path, not the default
    debug app selected by `scripts/roastty-app/start-app.sh`;
  - launches it with a temporary shell/config environment and trace path;
  - focuses the exact Roastty process/window;
  - types a marker command using the known-good System Events path from Issue
    804;
  - waits for a marker file, captures timestamped screenshots or another
    visible-output oracle, and captures trace/log artifacts under
    `logs/issue806-exp1-*`;
  - captures samples with `sample` while the app is waiting or visibly delayed;
  - cleans up the launched app process.
- Record the exact reproduction command, log paths, trace excerpts, sample
  paths, and the measured largest gap in this experiment file.
- Update the Issue 806 README experiment status to `Pass`, `Partial`, or `Fail`
  after running the experiment.

## Verification

Run the latency harness from a clean shell with no pre-existing Roastty process
owned by the experiment.

Pass criteria:

- The harness launches the current `ReleaseLocal` app and proves the target PID
  is the focused/frontmost Roastty process before typing.
- The harness records a timestamped path from synthetic keyboard injection to
  marker-file creation and to visible terminal output/presentation.
- The experiment either reproduces a delay greater than 30 seconds or explains,
  with trace evidence, why the current run no longer reproduces the user's
  observation.
- If the delay is reproduced, the trace localizes the largest gap to a named
  stage such as AppKit input, Rust key handling, termio queueing, PTY
  write/read, app tick/event drain, render/present, or lock contention.
- A marker file alone is not sufficient. The experiment must also prove the same
  marker reached the rendered terminal path, either with a timestamped
  screenshot/text oracle showing the marker output or with trace milestones that
  tie the marker's PTY read through `apply_termio_event`, dirty presentation,
  and `present_live` completion.
- At least one profiler artifact (`sample`, `spindump`, or Instruments export)
  is captured while the delay is in progress when the delay lasts long enough to
  sample.
- The experiment makes no product behavior fix.
- Hygiene checks run and are recorded:
  - `cargo fmt -- roastty/src/lib.rs` after Rust edits;
  - `cargo fmt --check --manifest-path roastty/Cargo.toml`;
  - a relevant Rust test or build command for the edited Roastty crate;
  - `cd roastty/macos && ./build.nu --configuration ReleaseLocal`;
  - `bash -n` for any new shell harness;
  - `git diff --check`.

Fail criteria:

- The harness cannot target the exact Roastty process/window.
- The harness types into the wrong app or cannot prove focus before typing.
- The trace lacks timestamps around the input-to-render milestones.
- The run proves shell execution only through a marker file but does not prove
  visible terminal output or a completed render/present path for that output.
- The experiment changes product behavior instead of measuring it.

## Design Review

Fresh-context adversarial review returned `CHANGES REQUIRED`.

- Required: the original pass criteria allowed marker-file creation as an
  alternative to visible terminal output. Fixed by requiring visible-output or
  render/present evidence for the same marker; marker creation alone is now a
  fail criterion.
- Required: the original design omitted hygiene checks for planned Rust, Swift,
  and shell edits. Fixed by adding explicit formatting, build/test, shell
  syntax, ReleaseLocal build, and `git diff --check` verification.
- Optional: the original design did not force the ReleaseLocal app path even
  though existing helpers default to Debug. Fixed by requiring the harness to
  build and launch the exact `ReleaseLocal` bundle.

Fresh-context adversarial re-review returned `APPROVED`.

- The reviewer confirmed the marker-only oracle gap is resolved by requiring
  visible-output or render/present evidence.
- The reviewer confirmed the hygiene checks are now explicit.
- The reviewer confirmed the ReleaseLocal build and launch path is now explicit.
- No new required findings were reported.

## Result

**Result:** Pass

The latency reproduced in the current `ReleaseLocal` app.

Command:

```bash
scripts/roastty-app/issue806-exp1-latency.sh
```

Artifacts:

- Harness log: `logs/issue806-exp1-20260615-204522.harness.log`
- Trace: `logs/issue806-exp1-20260615-204522.trace`
- Summary: `logs/issue806-exp1-20260615-204522-summary.txt`
- Sample: `logs/issue806-exp1-20260615-204522.sample.txt`
- Build log: `logs/issue806-exp1-20260615-204522-build.log`
- Before screenshot:
  `/Users/astrohacker/.cache/termsurf/shots/issue806-exp1-20260615-204522-before-20260615-204547.png`
- After screenshot:
  `/Users/astrohacker/.cache/termsurf/shots/issue806-exp1-20260615-204522-after-20260615-204628.png`

Measured timings:

- Synthetic typing call returned in `163.382ms`.
- Accessibility-visible terminal output containing the marker appeared after
  `34411.614ms`.
- Marker file creation was observed after `37039.313ms`.

The focus oracle proved the harness targeted Roastty before typing:

```text
frontmost-name=roastty
frontmost-pid=68413
roast-frontmost=true
focused-role=AXTextArea
focused-description=text entry area
```

The profiler identified the root cause more clearly than the trace summary.
During the stall, the main thread was inside AppKit key handling and blocked on
the shared `TermioWorker` mutex:

```text
Roastty.SurfaceView.keyDown(with:)
  Roastty.SurfaceView.keyAction(...)
    roastty_surface_key
      roastty::Surface::write_encoded_key_event
        roastty::termio::TermioWorker::with_termio
          _pthread_mutex_firstfit_lock_wait
            __psynch_mutexwait
```

The largest sampled stack count was:

```text
2659 samples:
  Surface::write_encoded_key_event
    TermioWorker::with_termio
      Mutex::lock
        __psynch_mutexwait
```

Another `232` samples were blocked in `TermioWorker::with_termio_mut` from the
same key handling path. The worker thread sample showed it spending the same
window in `poll`, which matches the code path where `run_termio_worker` holds
the `Termio` mutex while calling `termio.pump_once(10, 4096)`. Because key
encoding synchronously reads terminal key-encoding state through
`Surface::key_encode_options()` and checks KAM through
`Surface::terminal_kam_enabled()`, every typed character can block behind the
worker's polling loop. Rapid synthetic typing amplifies this into a many-second
queue of main-thread key handling stalls.

The trace corroborates the diagnosis:

- `keyDown` and `keyAction` events are present for the typed command.
- individual `keyAction result=true` durations reach `931.861ms`, `710.630ms`,
  `677.503ms`, and many other hundreds-of-milliseconds stalls;
- termio pump events eventually drain and `present_live` eventually completes;
- the visible marker arrives only after the accumulated key handling stalls.

Hygiene checks run:

```bash
cargo fmt -- roastty/src/lib.rs roastty/src/termio.rs
cargo fmt --check --manifest-path roastty/Cargo.toml
cargo test --manifest-path roastty/Cargo.toml termio_worker --lib
bash -n scripts/roastty-app/issue806-exp1-latency.sh
git diff --check
scripts/roastty-app/issue806-exp1-latency.sh
```

Notes:

- `cargo test --manifest-path roastty/Cargo.toml termio_worker --lib` compiled
  successfully but matched zero tests. The ReleaseLocal harness build is the
  meaningful compile gate for the edited Rust/Swift app code in this experiment.
- The trace contains some interleaved lines because multiple threads append to
  the same file. The profiler sample and per-event timestamps were still
  sufficient to identify the blocking stage.
- No product behavior fix was made in this experiment.

## Conclusion

The root cause for the reproduced 34-37 second input delay is main-thread
keyboard handling contending on the `TermioWorker` mutex. `Surface::key` reaches
`Surface::write_encoded_key_event`, which synchronously calls
`key_encode_options()` and `terminal_kam_enabled()`. Those functions lock the
same `Termio` mutex that the worker holds while blocking in `pump_once`.

The next experiment should fix the latency by removing `Termio` mutex access
from the synchronous key handling path. A focused fix is to cache the terminal
key-encoding options and KAM state on `Surface`, refresh that cache from worker
pump results or event-drain time, and make `write_encoded_key_event` use the
cached values before queueing bytes.

## Completion Review

Fresh-context adversarial review returned `APPROVED` with no findings.

The reviewer independently checked that the implementation stayed
measurement-only, the harness targets `ReleaseLocal`, the result documentation
and README status are present, the reproduction evidence supports the `>30s`
latency claim, and the sample supports the mutex-contention root cause. The
reviewer also ran these read-only checks successfully:

```bash
cargo fmt --check --manifest-path roastty/Cargo.toml
bash -n scripts/roastty-app/issue806-exp1-latency.sh
git diff --check
```
