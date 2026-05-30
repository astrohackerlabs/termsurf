# Experiment 2: Trace Real Wheel Input

## Description

Experiment 1 proved that Chromium's PDF viewer can scroll when driven through
DevTools `Input.dispatchMouseEvent`. That rules out a completely inert PDF
viewer. The user-visible failure is narrower: real wheel input in the Wezboard
window does not scroll the PDF.

Experiment 2 traces the real app-path input chain:

```text
macOS wheel event
→ Wezboard mouse/wheel handling
→ TermSurf protocol message
→ Roamium input dispatch
→ Chromium mouse wheel event
→ PDF viewer scroll
```

This experiment should identify the first hop where real wheel input is lost,
misclassified, targeted at the wrong pane/frame, or delivered with unusable
coordinates.

This experiment may add diagnostic logging and a real OS-input harness. It must
not include a behavior-changing fix unless the measured root cause is so small
and unambiguous that the fix is safer than leaving the experiment as diagnostic
only. If a fix is included, the result must clearly separate diagnosis from the
fix and still run the before/after evidence.

This experiment must receive Claude design review before implementation. After
the result is recorded, Claude must review the completed output before the next
experiment is designed.

## Changes

1. Add or extend a real OS-input harness for Issue 794.

   Preferred script: `scripts/test-issue-794-real-wheel.sh`.

   It should reuse the debug launch path from
   `scripts/test-issue-794-pdf-interactions.sh`:
   - launch debug `wezboard-gui`;
   - launch debug `web`;
   - pass repo-built Roamium with `--browser`;
   - load `http://localhost:9616/bitcoin.pdf`;
   - parse the DevTools port;
   - set `TERMSURF_PDF_INPUT_TRACE_FILE="$LOG_DIR/pdf-input.log"` before
     launching Wezboard so Wezboard, Roamium, and Chromium children inherit the
     same absolute trace path;
   - capture a before screenshot/state with
     `scripts/capture-pdf-interactions.mjs` in probe mode;
   - focus the Wezboard window;
   - send real macOS wheel input at coordinates inside the visible webview;
   - capture an after screenshot/state with the DevTools helper in probe mode;
   - write all artifacts under `logs/issue-794-exp2-*`.

   If macOS event injection is unavailable from the agent environment, do not
   fake it with CDP input. Record the automation limitation and keep the
   experiment focused on instrumentation plus manual reproduction.

   Preferred injection mechanism: a tiny Swift or C helper using
   `CGEventCreateScrollWheelEvent`, because that is closest to a real hardware
   wheel event. `cliclick` is acceptable as a secondary option if installed.
   AppleScript/System Events is a last resort because it may not produce the
   same event shape Wezboard receives from hardware.

2. Add gated trace logging for the input chain.

   Use a new env var gate:

   ```text
   TERMSURF_PDF_INPUT_TRACE=1
   ```

   Read the destination path from:

   ```text
   TERMSURF_PDF_INPUT_TRACE_FILE=/absolute/path/to/pdf-input.log
   ```

   The wrapper script must set this to `$LOG_DIR/pdf-input.log` before launching
   Wezboard. All child processes must append to this absolute path so the hop
   trace is a single file. If the env var is missing, fall back to
   `${TMPDIR:-/tmp}/termsurf/pdf-input.log`, not a cwd-relative path.

   Follow the existing direct-to-file debug-log convention:
   - open/truncate once when tracing is enabled;
   - append one structured line per event;
   - do not rely only on stdout/stderr.

3. Instrument Wezboard's browser-overlay wheel path.

   Locate the current wheel/mouse forwarding path, starting from:
   - `wezboard/wezboard-gui/src/termwindow/mouseevent.rs`
   - nearby TermSurf overlay/input code under
     `wezboard/wezboard-gui/src/termsurf`

   For each wheel event that should target a browser overlay, log:
   - window id / pane id / tab id if available;
   - raw window pixel coordinate;
   - pane-local pixel coordinate;
   - browser overlay rect;
   - whether the event was considered inside the overlay;
   - forwarded protocol message type;
   - forwarded coordinates and deltas;
   - whether the terminal consumed the wheel instead.

4. Instrument Roamium's received wheel path.

   Locate the protocol receive and dispatch path, starting from:
   - `roamium/src/dispatch.rs`
   - `roamium/src/ffi.rs`

   For each received wheel/mouse event for the target tab, log:
   - tab id;
   - event type;
   - received coordinates and deltas;
   - whether the event is forwarded to Chromium;
   - which FFI function is called.

5. Instrument Chromium's TermSurf mouse-event entry point if needed.

   If Wezboard and Roamium logs prove the wheel reaches the C FFI boundary but
   the PDF still does not scroll, add temporary gated logging in the current
   Chromium branch around the TermSurf `ts_forward_*mouse*` / wheel path.

   If Chromium is modified:
   - create a new Chromium branch for Issue 794, using
     `148.0.7778.97-issue-794-exp2` forked from the last known good PDF branch
     `148.0.7778.97-issue-793-exp1`;
   - add it to `chromium/README.md`;
   - keep the logging gated by `TERMSURF_PDF_INPUT_TRACE`;
   - do not alter input behavior in the tracing pass.

6. Keep Experiment 1's DevTools harness as the oracle.

   The run should include a CDP wheel sanity check or reference Experiment 1's
   artifact proving that DevTools wheel input scrolls the same PDF. This avoids
   confusing a PDF-viewer regression with a real-app input-path failure.

7. Run `cargo fmt` after any Rust edits.

   Accept formatter output as-is.

## Verification

1. Re-run the Experiment 1 CDP PDF scroll sanity check if the code changed in a
   way that could affect it:

   ```bash
   LOG_DIR="logs/issue-794-exp2-cdp-pdf-$(date +%Y%m%d-%H%M%S)" \
   TERMSURF_PDF_SETTLE_SECONDS=12 \
   scripts/test-issue-794-pdf-interactions.sh \
     http://localhost:9616/bitcoin.pdf
   ```

   Required evidence:
   - `wheel-scroll.json` is `pass`;
   - the before/after screenshots visibly differ.

2. Run the real wheel trace harness:

   ```bash
   LOG_DIR="logs/issue-794-exp2-realwheel-$(date +%Y%m%d-%H%M%S)" \
   TERMSURF_PDF_INPUT_TRACE=1 \
   scripts/test-issue-794-real-wheel.sh
   ```

3. Inspect artifacts:
   - `run-info.txt`
   - before/after DevTools state and screenshots
   - `$LOG_DIR/pdf-input.log` set through `TERMSURF_PDF_INPUT_TRACE_FILE`
   - Wezboard log
   - Roamium/Chromium log if copied

4. Classify the first failing hop:

   | Hop                              | Evidence                              | Status                  |
   | -------------------------------- | ------------------------------------- | ----------------------- |
   | OS event generated               | before/after harness or manual action | yes/no                  |
   | Wezboard sees wheel              | `pdf-input.log` Wezboard line         | yes/no                  |
   | Wezboard targets browser overlay | overlay hit line                      | yes/no                  |
   | Wezboard sends protocol input    | forwarded message line                | yes/no                  |
   | Roamium receives input           | dispatch line                         | yes/no                  |
   | Roamium calls Chromium FFI       | FFI line                              | yes/no                  |
   | Chromium receives wheel          | Chromium trace line if needed         | yes/no/not instrumented |
   | PDF viewer scrolls               | DevTools after screenshot/state       | yes/no                  |

5. Record the result in this file.

   The result must include:
   - exact log directory paths;
   - the first failing hop;
   - whether CDP wheel still scrolls the same PDF;
   - whether real wheel input scrolls;
   - whether the next experiment should fix Wezboard, protocol dispatch, Roamium
     FFI, Chromium input routing, or test automation.

6. Claude must review the completed output.

   Do not proceed to Experiment 3 until real issues from Claude's review are
   addressed.

## Pass Criteria

Experiment 2 passes if it identifies the first real app-path wheel-input failure
with direct log or screenshot evidence, and the classification is specific
enough to design a targeted fix.

## Partial Criteria

Experiment 2 is partial if:

- the trace proves only part of the path;
- OS event injection is unavailable, but manual reproduction plus logs still
  identify a likely failing hop;
- the wheel reaches Chromium but PDF scroll still does not happen and more
  Chromium-side instrumentation is needed.

## Failure Criteria

Experiment 2 fails if:

- it uses CDP input as a substitute for real app-path wheel input;
- it changes wheel/input behavior without first proving the failing hop;
- it cannot distinguish "Wezboard never saw the wheel" from "Roamium received
  but did not forward";
- it omits `cargo fmt` after Rust edits;
- it modifies Chromium without creating an Issue 794 Chromium branch.

## Result

**Result:** Partial

Experiment 2 added the real app-path wheel trace infrastructure and attempted
agent-side macOS wheel injection. The instrumentation is in place, but the agent
could not generate a wheel event that Wezboard received.

Code/artifact changes:

- Added `scripts/test-issue-794-real-wheel.sh`.
- Added gated Wezboard trace logging in:
  - `wezboard/wezboard-gui/src/termwindow/mod.rs`
  - `wezboard/wezboard-gui/src/termwindow/mouseevent.rs`
  - `wezboard/wezboard-gui/src/termsurf/input.rs`
- Added gated Roamium trace logging in `roamium/src/dispatch.rs`.
- Added startup trace initialization in Wezboard and Roamium so a run can
  distinguish "trace env was inherited" from "no input event reached the trace
  point."
- The trace gate is `TERMSURF_PDF_INPUT_TRACE=1`.
- The shared trace path is `TERMSURF_PDF_INPUT_TRACE_FILE`, set by the wrapper
  to `$LOG_DIR/pdf-input.log`.

Formatter/build evidence:

- `cargo fmt` was attempted through the default shell but `cargo` was not on
  PATH.
- `cargo fmt` was then attempted through Homebrew rustup and stable toolchain
  cargo, but those cargo frontends did not expose the `fmt` subcommand.
- `cargo-fmt --manifest-path ... --all` was run successfully with an explicit
  PATH that includes the stable toolchain and Homebrew rustup. It emitted
  existing warnings about unstable `imports_granularity = Module`, then exited
  successfully.
- Debug Roamium rebuilt successfully.
- Debug Wezboard rebuilt successfully.

Runs:

- First attempt: `logs/issue-794-exp2-realwheel-20260529-190314`
  - Failed at the AppleScript/System Events window activation/coordinate step:
    `osascript is not allowed assistive access. (-1719)`.
  - This validated the design's warning that AppleScript is a weak automation
    path.
- Second attempt: `logs/issue-794-exp2-realwheel-20260529-190430`
  - Replaced AppleScript with a Swift/CoreGraphics helper that finds the
    Wezboard window by PID with `CGWindowListCopyWindowInfo`.
  - The helper found the window and posted scroll events, but Wezboard received
    no raw scroll event.
- Third attempt: `logs/issue-794-exp2-realwheel-20260529-190522`
  - Added `NSRunningApplication.activate(...)` before posting scroll events.
  - The helper again found the window and posted scroll events.
  - Wezboard still received no raw scroll event.

Third-attempt evidence:

```text
window=0.0,45.0,1134.0,1216.0
target=623.7,653.0
```

The expected trace file was not created:

```text
logs/issue-794-exp2-realwheel-20260529-190522/pdf-input.log
```

After Claude's completion review, this ambiguity was fixed by adding explicit
startup trace initialization in both Wezboard and Roamium. A future manual trace
run should now create `$LOG_DIR/pdf-input.log` with `trace-init` lines even
before any wheel event is received. If that file is absent in a future run, the
problem is trace environment propagation rather than wheel routing.

The before/after DevTools screenshots were byte-identical:

```text
before/baseline.png = 411453 bytes
after/baseline.png  = 411453 bytes
cmp_exit=0
```

Hop classification:

| Hop                              | Evidence                                                                                                        | Status           |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------- | ---------------- |
| OS event generated               | Swift helper found the window and called `CGEvent(...).post(...)`, but macOS provided no delivery confirmation. | inconclusive     |
| Wezboard sees wheel              | No `$LOG_DIR/pdf-input.log`; no `window-raw-scroll` line.                                                       | no               |
| Wezboard targets browser overlay | No raw event reached the trace point.                                                                           | no evidence      |
| Wezboard sends protocol input    | No raw event reached the trace point.                                                                           | no evidence      |
| Roamium receives input           | No Roamium trace line.                                                                                          | no               |
| Roamium calls Chromium FFI       | No Roamium trace line.                                                                                          | no               |
| Chromium receives wheel          | Not instrumented; upstream hops did not fire.                                                                   | not instrumented |
| PDF viewer scrolls               | Before/after screenshots identical.                                                                             | no               |

Important distinction:

This Partial result does **not** prove that the user's hardware wheel event
fails before Wezboard. It proves only that this agent's synthetic macOS wheel
injection did not reach Wezboard. The trace points are now in place for a real
manual reproduction.

## Conclusion

Experiment 2 produced useful infrastructure but did not complete the intended
real app-path diagnosis because agent-side OS event injection did not deliver a
wheel event to Wezboard.

What we know:

1. Experiment 1 proved CDP wheel input scrolls the PDF.
2. Experiment 2's synthetic CGEvent wheel input did not reach Wezboard in this
   agent environment.
3. The Wezboard/Roamium trace gate now initializes at process startup, so it is
   ready for a manual reproduction: if the user runs the debug app with
   `TERMSURF_PDF_INPUT_TRACE=1` and
   `TERMSURF_PDF_INPUT_TRACE_FILE=/absolute/path`, then scrolls the PDF with
   real hardware, the resulting trace should identify the first real failing
   hop.

The next step should be a manual-trace experiment or a user-run test using the
new instrumentation. Do not infer a product fix from the synthetic-event failure
alone.
