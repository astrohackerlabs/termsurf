# Experiment 60: Compare PDF Action Paths

## Description

Experiments 56, 57, and 59 ruled out three simple explanations for Surfari's
embedded PDF copy failure:

- making the embedded WebKit host key/main or explicitly routing `copy:` to the
  top-level `WKWebView` did not recover all tokens;
- sending the mouse stream through alternate AppKit dispatch targets changed
  selection shape in one cell but did not recover all tokens;
- naively resizing the hidden `WKWebView` and/or converting mouse points by the
  display scale did not make PDF copy pass.

Experiment 54 still gives us a known-good standalone `WKWebView` PDF control:
the separated-token fixture can copy `LEFT834 MID834 RIGHT834` under calibrated
gestures. The next step is to compare the successful standalone action path and
the failing embedded Surfari action path in one run, using the same fixture,
gesture family, pasteboard checks, responder/action probes, and observable PDF
selection probes.

This experiment is diagnostic. It should identify the first material divergence
between standalone success and embedded failure. It should not change normal
Surfari behavior unless a narrow env-gated probe is needed to capture evidence.

## Changes

- Add a focused harness, tentatively
  `scripts/test-issue-834-surfari-pdf-action-path-compare.sh`.
- Reuse the exact separated-token fixture and calibration gates from Experiments
  50 and 54:
  - tokens: `LEFT834`, `MID834`, `RIGHT834`;
  - only compare cells with matched successful Experiment 54 standalone
    baselines;
  - include at least `oracle-base` and one y-axis neighbor from the standalone
    success band.
- In the same harness run, execute:
  - a standalone `WKWebView` success control using the calibrated gesture;
  - an embedded Surfari run using the matched calibrated gesture;
  - an embedded no-selection copy control to prove the pasteboard sentinel does
    not change without a real PDF selection.
- Record matching action-path evidence for standalone and embedded:
  - key/main window state;
  - first responder and responder chain;
  - `NSApp targetForAction:to:from:` for `copy:`, using `nil`, the `WKWebView`,
    and the hit-test target as the `from` object when possible;
  - whether `validateUserInterfaceItem:` or `validateMenuItem:` reports copy as
    enabled for any resolved target that responds to those selectors;
  - hit-test target and nearest PDF-related descendant classes such as
    `WKFlippedView` and `WKPDFHUDView`;
  - pasteboard change count and sample before selection, after selection, after
    external Cmd+C, after explicit target copy probes, and after fallback
    select-all;
  - JavaScript `document.getSelection()` and active element state, even though
    prior results suggest WebKit's PDF plugin selection is not exposed there;
  - any observable Objective-C selection-like selectors found on hit targets or
    PDF-related descendant views, recorded as method presence and safe return
    summaries only.
- Keep explicit copy probes diagnostic-only and record them separately from the
  primary external Cmd+C route. A direct probe that changes pasteboard contents
  is a clue, not product behavior.
- Avoid patched WebKit internals in this experiment. Experiment 58 showed that
  local WebKit tracing did not attach to the active system WebKit path. This
  experiment should stay in the app-facing `WKWebView`/AppKit layer unless the
  result proves that layer is exhausted.
- Add summary classification:
  - **action-path-equivalent-selection-missing:** standalone and embedded have
    materially equivalent action/responder/copy routing, but embedded has no
    observable selected text and does not change the pasteboard;
  - **copy-target-gap:** standalone resolves an enabled copy target while
    embedded does not, or embedded resolves a different target that cannot copy;
  - **pasteboard-write-gap:** embedded exposes an apparently valid selection and
    enabled copy target, but the pasteboard does not change;
  - **selection-state-gap:** standalone exposes selected text or selection-like
    state while embedded does not after matched gestures;
  - **direct-copy-candidate:** an explicit diagnostic copy route copies all
    tokens in embedded Surfari while primary external Cmd+C does not;
  - **harness-insufficient:** gates are closed, standalone success is not
    reproduced, embedded failure is not reproduced, traces are missing, or
    clipboard restoration fails.
- Apply classification precedence:
  1. `harness-insufficient` for closed gates, missing evidence, missing baseline
     reproduction, fixture mismatch, or clipboard restoration failure.
  2. `direct-copy-candidate` if a diagnostic explicit route copies all tokens
     from embedded Surfari while primary Cmd+C still fails.
  3. `pasteboard-write-gap` if selection and enabled target evidence exist but
     pasteboard does not change.
  4. `copy-target-gap` if copy target resolution or enablement differs
     materially.
  5. `selection-state-gap` if standalone shows selected text/state and embedded
     does not under matched gestures.
  6. `action-path-equivalent-selection-missing` if responder/action evidence is
     equivalent and the remaining gap is below the app-facing layer.

## Verification

Run hygiene checks:

```bash
bash -n scripts/test-issue-834-surfari-pdf-action-path-compare.sh
cargo fmt -p surfari -- --check
surfari/libtermsurf_webkit/build.sh
cargo build -p surfari
git diff --check
git -C webkit/src status --short
```

Run the action-path comparison:

```bash
rm -rf logs/issue-834-exp60-surfari-pdf-action-path-compare
scripts/test-issue-834-surfari-pdf-action-path-compare.sh
```

Pass criteria:

- Experiment 50 oracle gate is open;
- Experiment 54 standalone calibration gate is open;
- standalone `WKWebView` reproduces all-token copy for the selected calibrated
  cells in the same run;
- embedded Surfari reproduces the missing-token or no-copy failure for the same
  fixture and matched gestures in the same run;
- the no-selection embedded control leaves the pasteboard sentinel unchanged;
- standalone and embedded records include responder, action target, action
  enablement, hit-test/PDF-descendant, JavaScript selection, selection-like
  selector, pasteboard, and trace-path evidence;
- one explicit non-`harness-insufficient` classification is selected;
- normal Surfari behavior is unchanged without any env-gated diagnostic flags;
- completion review is recorded.

Partial criteria:

- standalone and embedded reproduce, but one selection-like probe is blocked by
  AppKit/WebKit privacy or private API boundaries;
- the comparison narrows the next target but cannot select one classification
  confidently;
- only some calibrated cells remain comparable.

Failure criteria:

- clipboard state is not restored;
- standalone all-token copy does not reproduce;
- embedded failure does not reproduce;
- fixture identity does not match the oracle;
- the harness overclaims a root cause without matched standalone and embedded
  evidence.

## Design Review

Codex reviewed the Experiment 60 design before implementation and found no
blocking issues. The review agreed that comparing successful standalone
`WKWebView` PDF action/copy state against failing embedded Surfari state follows
from Experiments 56 through 59, because responder activation, outer mouse
dispatch, WebKit-internal tracing, and naive point scaling have all been bounded
or rejected.

The review specifically approved the controls: Experiment 50 and 54 gates,
matched standalone baselines, same-run standalone success, same-run embedded
failure, and the no-selection pasteboard sentinel control. It also agreed that
explicit copy probes must stay diagnostic-only and that the result language must
avoid claiming a private WebKit/PDFKit root cause from app-facing evidence
alone.

The design is approved for implementation after the plan commit.
