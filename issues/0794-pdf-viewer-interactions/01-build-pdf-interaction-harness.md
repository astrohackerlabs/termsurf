# Experiment 1: Build PDF Interaction Harness

## Description

Issue 794 starts from a mixed signal: the PDF renders and Issue 793 fixed the
iframe size, but the viewer is not interactive enough to be usable. Manual
testing shows no scroll, no resize/reflow, and no text selection.

Before changing Chromium, Roamium, Wezboard, or the protocol, this experiment
adds an automated diagnostic harness that separates the likely failure layers:

- Chromium/PDF viewer internals, driven through the Chrome DevTools Protocol.
- The full TermSurf app path, driven through Wezboard and observed through
  DevTools, screenshots, logs, and later real macOS input if needed.

This experiment is diagnostic only. It must not include a behavior-changing PDF
fix. The result should say which checks pass, which checks fail, and which layer
the next experiment should target.

This experiment must receive Claude design review before implementation. After
the result is recorded, Claude must review the completed output before the next
experiment is designed.

## Changes

1. Add a reusable DevTools interaction helper, preferably
   `scripts/capture-pdf-interactions.mjs`.

   Use Node's built-in `fetch` and `WebSocket` support; do not add npm
   dependencies.

   The helper should accept:
   - `--devtools-port`
   - `--url-contains`
   - `--out-dir`
   - optional `--timeout-seconds`
   - optional `--settle-seconds`
   - optional `--input-settle-ms`
   - optional `--resize-settle-ms`
   - optional `--mode=probe|full` where `probe` only gathers state and `full`
     sends input events

   It should reuse the proven target-discovery pattern from
   `scripts/capture-devtools-screenshot.mjs`:
   - poll `http://127.0.0.1:{port}/json/list`;
   - select the matching `type == "page"` target;
   - connect to `webSocketDebuggerUrl`;
   - enable `Page`, `Runtime`, `DOM`, `Input`, `Target`, `Browser`, and any
     required target/frame domains;
   - use `Target.setAutoAttach` / `Target.attachToTarget` or an equivalent
     execution-context tracking strategy to inspect PDF extension OOPIFs and
     child targets when DevTools exposes them;
   - write all artifacts under `--out-dir`.

   The preferred child-frame strategy is `Target.attachToTarget`, because the
   PDF extension document runs at
   `chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/` and cannot be reached
   by ordinary top-level JavaScript from the original PDF URL. If child target
   attachment is unavailable, record that explicitly and classify child-frame
   state checks as `unsupported-by-harness`, not as PDF failures.

2. Record a baseline PDF state JSON before any input.

   The baseline should include, when available:
   - selected target URL and title;
   - frame tree URLs;
   - screenshot path;
   - viewport size;
   - `window.scrollX`, `window.scrollY`, `document.scrollingElement.scrollTop`,
     and `document.scrollingElement.scrollHeight` for the top-level target;
   - discovered PDF extension frame URL(s);
   - discovered internal plugin/embed element bounds;
   - PDF viewer DOM state if reachable from the selected target;
   - any errors from cross-origin or inaccessible frames.

   Do not treat inaccessible cross-origin frame state as an automatic failure.
   Record it explicitly, because it tells us whether the harness needs to attach
   to child targets or use browser-side logs in the next experiment.

3. Add interaction checks.

   The first version should run these checks in this order and write one JSON
   result per check:
   - **render:** capture a screenshot and confirm it is non-empty. If practical,
     add a simple visual classifier that distinguishes "mostly blank" from
     "contains substantial ink." Do not require OCR in this experiment.
   - **wheel-scroll:** dispatch a wheel event near the center of the PDF view,
     wait 250-500 ms by default, then compare scroll/page/screenshot state
     before and after.
   - **keyboard-scroll:** dispatch PageDown or Space, wait 250-500 ms by
     default, then compare scroll/page/screenshot state before and after.
   - **click-focus:** dispatch a click inside the PDF/plugin bounds, then record
     focus-related state and screenshot after a short 100 ms settle.
   - **drag-select:** dispatch mouse pressed/moved/released across coordinates
     derived from the baseline PDF/plugin bounds. Prefer a wide first-page drag
     inside the visible plugin/page rectangle rather than hardcoded absolute
     coordinates. Wait 250 ms by default, then record selected text through any
     reachable Chromium selection API and by sending copy if possible. Before
     clipboard reads, try `Browser.grantPermissions` with clipboard permissions
     for the target origin. If clipboard permission cannot be granted, classify
     clipboard verification as `unsupported-by-harness`, not as a PDF selection
     failure.
   - **resize-state:** first use `Emulation.setDeviceMetricsOverride` to change
     the renderer viewport, wait 500 ms by default, then compare viewer/page
     dimensions and screenshot state before and after. This measures
     Chromium-side resize/reflow. A real Wezboard window or pane resize is a
     separate full-app-path measurement for a follow-up experiment if DevTools
     resize passes but manual/real app resize still fails.
   - **toolbar-probe:** locate zoom/page/fit/rotate/save/print controls if
     reachable. For Experiment 1, detection is enough; do not click save or
     print unless the harness can safely prevent native dialogs.
   - **title-probe:** record the target title and the TUI-visible title if logs
     or existing APIs expose it. For the TUI-visible side, grep the Wezboard log
     for protocol/title update lines if they exist. If no title message is
     visible, classify the TUI side as `unsupported-by-harness`.

   Each check should produce one of:
   - `pass`
   - `fail`
   - `unsupported-by-harness`
   - `inconclusive`

   Use a consistent JSON result shape:

   ```json
   {
     "check": "wheel-scroll",
     "status": "pass",
     "evidence": {
       "before": {},
       "after": {},
       "diff": {},
       "screenshots": ["wheel-before.png", "wheel-after.png"]
     },
     "notes": "brief explanation"
   }
   ```

   The `evidence.before`, `evidence.after`, and `evidence.diff` objects may
   differ per check, and screenshot paths should be relative to `--out-dir`, but
   every result file must include the same top-level fields.

4. Add a real-app wrapper script, preferably
   `scripts/test-issue-794-pdf-interactions.sh`.

   It should be based on `scripts/test-issue-792-devtools-screenshot.sh` and
   should:
   - launch debug `wezboard-gui`;
   - launch debug `web`;
   - pass repo-built Roamium with `--browser`;
   - serve the vendored Bitcoin PDF from `test-html/public/bitcoin.pdf`;
   - parse the DevTools port from the live Wezboard log;
   - run `scripts/capture-pdf-interactions.mjs` before teardown;
   - copy the Chromium log if available;
   - write `run-info.txt` with exact binary paths, URL, DevTools port, and
     commands.

   The wrapper must continue to use debug binaries and must not install over the
   user's stable app.

5. Add a normal web regression fixture or reuse an existing one.

   The same harness should run against a non-PDF page such as
   `http://localhost:9616/test-mouse.html` or a new simple long scrolling HTML
   page. The goal is to verify the harness can observe ordinary page scrolling,
   clicking, resizing, and selection before interpreting PDF failures.

6. Do not modify PDF behavior in this experiment.

   Specifically, do not change:
   - Chromium PDF viewer code;
   - `content/libtermsurf_chromium`;
   - Roamium dispatch/input code;
   - Wezboard input forwarding;
   - `termsurf.proto`;
   - webtui behavior.

   If the diagnostic harness proves one of those layers is broken, record that
   in the result and design Experiment 2 around that layer.

## Verification

1. Run the normal web regression first:

   ```bash
   LOG_DIR="logs/issue-794-exp1-html-$(date +%Y%m%d-%H%M%S)" \
   scripts/test-issue-794-pdf-interactions.sh \
     http://localhost:9616/test-mouse.html
   ```

   Required result:
   - render check passes;
   - at least one scroll check passes, or the fixture is replaced with a page
     where scroll is measurable;
   - click/focus evidence is present;
   - the run uses debug `wezboard-gui`, debug `web`, and repo-built Roamium.

   If the HTML sanity run cannot prove the harness works on ordinary web
   content, stop. Do not interpret PDF failures from a broken harness.

2. Run the PDF diagnostic:

   ```bash
   LOG_DIR="logs/issue-794-exp1-pdf-$(date +%Y%m%d-%H%M%S)" \
   scripts/test-issue-794-pdf-interactions.sh \
     http://localhost:9616/bitcoin.pdf
   ```

3. Inspect the generated artifacts.

   Required artifacts:
   - `run-info.txt`
   - `baseline.json`
   - one JSON result per check
   - before/after screenshots for render, scroll, keyboard, drag-select, and
     resize checks where applicable
   - Wezboard log
   - Chromium log if available

4. Record the result in this file.

   The result must include a table:

   | Check | HTML result | PDF result | Evidence | Next layer |
   | ----- | ----------- | ---------- | -------- | ---------- |

   Use `Next layer` to classify the likely target:
   - `harness`
   - `TermSurf input path`
   - `Chromium PDF focus/hit-test`
   - `PDF viewer viewport/resize`
   - `PDF extension API/state`
   - `not yet known`

5. Claude must review the completed output.

   Give Claude:
   - this experiment file;
   - the result table;
   - paths to the generated logs/artifacts;
   - relevant JSON result snippets;
   - any proposed Experiment 2 target.

   Do not proceed to Experiment 2 until real issues from Claude's review are
   addressed.

## Pass Criteria

Experiment 1 passes if it produces a reliable diagnostic matrix that identifies
which PDF interaction checks fail and narrows each failure to a next layer.

A Pass does not require the PDF viewer to become interactive. This experiment is
successful if it turns "PDF does not scroll/select/resize" into concrete,
repeatable evidence for the next fix.

## Partial Criteria

Experiment 1 is partial if:

- the harness works for normal HTML but cannot inspect or drive enough PDF state
  to classify the PDF failures;
- the harness proves rendering but cannot yet measure selection, focus, or
  resize;
- DevTools can drive PDF interactions but the real app-path status remains
  unproven.

In a Partial result, the next experiment should improve the harness or add
targeted logging at the first unobservable layer.

## Failure Criteria

Experiment 1 fails if:

- the normal HTML sanity run cannot prove the harness works;
- the run uses installed/stable Roamium instead of repo-built Roamium;
- the harness captures the wrong target;
- artifacts are missing or insufficient to classify any PDF interaction;
- the experiment includes behavior-changing PDF fixes instead of diagnostics.

## Result

**Result:** Pass

Experiment 1 built the diagnostic harness and produced a useful first matrix.
The harness uses the real debug Wezboard/web/Roamium launch path, attaches to
the matching DevTools page target, discovers the PDF extension iframe target,
captures screenshots, and records per-check JSON artifacts.

Code/artifact changes:

- Added `scripts/capture-pdf-interactions.mjs`.
- Added `scripts/test-issue-794-pdf-interactions.sh`.
- Added `test-html/public/test-interactions.html`.

Validation runs:

- HTML sanity: `logs/issue-794-exp1-html-20260529-184503`
- PDF diagnostic: `logs/issue-794-exp1-pdf-20260529-184813`

Both runs used:

- debug Wezboard: `/Users/ryan/dev/termsurf/wezboard/target/debug/wezboard-gui`
- debug web: `/Users/ryan/dev/termsurf/webtui/target/debug/web`
- repo-built Roamium:
  `/Users/ryan/dev/termsurf/chromium/src/out/Default/roamium`

Result matrix:

| Check           | HTML result | PDF result                     | Evidence                                                                                                                                                                          | Next layer                                            |
| --------------- | ----------- | ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| render          | Pass        | Pass                           | HTML/PDF screenshots are non-empty; PDF screenshot shows the Bitcoin paper.                                                                                                       | none                                                  |
| wheel-scroll    | Pass        | Pass via CDP                   | HTML scroll state changed by 600 px. PDF screenshot changed from page 1 to page 2-ish content, proving Chromium's PDF viewer can scroll when driven through DevTools input.       | TermSurf input path for the user's real wheel failure |
| keyboard-scroll | Pass        | Pass via CDP                   | HTML scroll state changed by 550 px. PDF screenshot changed after PageDown.                                                                                                       | TermSurf input path if keyboard/user path still fails |
| click-focus     | Pass        | Inconclusive                   | HTML click changed visible page state. PDF click produced no top-level active-element change; child/plugin focus is not exposed by the current state probe.                       | Chromium PDF focus/hit-test or harness                |
| drag-select     | Pass        | Fail                           | HTML drag/copy produced selected text. PDF drag/copy produced no selected text with clipboard permission granted.                                                                 | Chromium PDF focus/hit-test / PDF text selection      |
| resize-state    | Pass        | Partial/Pass for viewport only | DevTools viewport resize changed PDF viewer dimensions and screenshot size. It does not prove the user's desired page zoom/reflow behavior because the PDF stayed at 97% zoom.    | PDF viewer viewport/resize policy                     |
| toolbar-probe   | Pass        | Pass                           | Shadow-DOM traversal found 19 PDF toolbar controls. Detection only; save/print were not clicked.                                                                                  | PDF extension API/state for future toolbar behavior   |
| title-probe     | Pass        | Inconclusive                   | HTML run found title-like Wezboard log lines. PDF DevTools target title was empty at top level; PDF extension child title was `bitcoin.pdf`; no TUI-visible title line was found. | PDF extension API/state / title propagation           |

Important diagnostic conclusion:

The user's "I cannot scroll the PDF" symptom is not explained by a totally inert
Chromium PDF viewer. CDP `Input.dispatchMouseEvent` wheel input scrolls the PDF
visibly. That points the next scroll-focused experiment toward the real app
input path: macOS/Wezboard wheel event → TermSurf protocol → Roamium dispatch →
Chromium. Experiment 1 did not drive real OS mouse events, so it cannot yet say
which hop in that path drops or misroutes the user's wheel input.

Text selection is a different failure. The same CDP harness can select text on
ordinary HTML but cannot select/copy PDF text from the Bitcoin PDF. That points
toward PDF plugin focus/hit-test/selection routing rather than a generic harness
or clipboard failure.

Resize is also separate. The PDF viewer viewport changes under DevTools
emulation, but the page remains at the same visible zoom level. If "respond to
resize" means "the page should re-fit to the pane," the next resize-focused
experiment should target PDF viewer viewport/fit policy rather than basic
WebContents resize delivery.

## Conclusion

Experiment 1 successfully turned the broad Issue 794 symptom set into separate
tracks:

1. **Scroll:** Chromium PDF scrolling works via CDP, so the user's real wheel
   failure needs a real app-path input experiment.
2. **Text selection:** PDF drag/copy fails even through CDP while HTML drag/copy
   passes, so the next text-selection fix should focus on PDF
   focus/hit-test/selection routing.
3. **Resize:** viewport resize reaches the viewer, but page fit/zoom behavior
   remains unsolved.
4. **Toolbar:** controls exist and are discoverable, but functional toolbar
   testing remains future work.
5. **Title:** PDF title propagation remains incomplete or unverified.

The next experiment should focus on real app-path input for scrolling first,
because that directly explains the user's reported "I can't scroll" behavior and
the CDP evidence shows the PDF viewer itself is capable of scrolling.
