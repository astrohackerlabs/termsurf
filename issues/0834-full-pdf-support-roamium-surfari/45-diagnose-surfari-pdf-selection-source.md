# Experiment 45: Diagnose Surfari PDF Selection Source

## Description

Experiment 44 proved that Ghostboard forwards drag input and Browse-mode `Cmd+C`
to Surfari's PDF surface, but the clipboard remains unchanged. The next step is
to determine where the failure lives before changing product code.

This experiment should compare the same deterministic PDF fixture across
standalone macOS controls and Surfari:

- a native/programmatic PDFKit text extraction control;
- a known-good selectable text automation control, such as an `NSTextView` or a
  `WKWebView` loading plain HTML text;
- a standalone PDFKit `PDFView` control;
- a standalone `WKWebView` control loading the PDF;
- Surfari inside Ghostboard using the existing Experiment 44 harness evidence.

The goal is to classify the first failing layer:

- fixture problem: the generated PDF renders but is not selectable/copyable in
  normal macOS PDF controls;
- WebKit/PDFKit behavior: PDFKit can copy it, but WKWebView cannot;
- Surfari integration problem: standalone WKWebView can copy it, but Surfari
  inside Ghostboard cannot;
- automation problem: manual/native control behavior differs from the automated
  CGEvent route.

## Changes

- Add a focused diagnostic harness, tentatively
  `scripts/test-issue-834-surfari-pdf-selection-source.sh`.
- Reuse or generate the same PDF text fixture metadata from Experiment 44:
  - marker: `TS834PDFCOPYQXJZ`;
  - accepted substring: `TS834PDFCOPYQXJZ`;
  - real PDF text operator, not raster text;
  - explicit font encoding and PDF-space text box.
- Create a temporary standalone PDFKit probe app or Swift script:
  - first run a non-CGEvent fixture control using `PDFDocument.string` or an
    explicit `PDFSelection` over the fixture page to prove whether PDFKit can
    extract the marker from the generated PDF at all;
  - display the PDF in a visible `PDFView`;
  - use real CGEvent drag selection and `Cmd+C`;
  - save, prime, read, and restore the clipboard with the same safety rules as
    Experiment 44;
  - record whether the clipboard contains the accepted substring.
- Create a known-good selectable text automation control:
  - use either an `NSTextView` or a `WKWebView` loading HTML that contains the
    same marker;
  - use the same CGEvent drag and `Cmd+C` path;
  - require this control to prove the harness can select and copy visible text
    with the current macOS permissions.
- Create a temporary standalone WKWebView probe app or Swift script:
  - display the PDF in a visible `WKWebView`;
  - use real CGEvent drag selection and `Cmd+C`;
  - use the same clipboard safety and evidence rules;
  - record whether the clipboard contains the accepted substring.
- Re-run or consume the current Surfari probe path:
  - either call `scripts/test-issue-834-surfari-pdf-selection-copy.sh`, or reuse
    its latest summary only if the summary was produced in the same experiment
    run and the paths are recorded;
  - keep Surfari's result separate from the standalone controls.
- Record for each control:
  - process/app path;
  - PDF path/URL;
  - window bounds;
  - text-region drag coordinates;
  - clipboard before/after hashes, lengths, bounded sample, and restore status;
  - pasteboard change indicators;
  - screenshots before and after drag;
  - pass/partial/fail classification.
- Apply this explicit outcome matrix:
  - **fixture problem:** native/programmatic PDFKit text extraction cannot
    obtain the marker from the generated PDF;
  - **automation problem:** native/programmatic PDFKit extraction succeeds, but
    the known-good selectable text automation control fails;
  - **PDFKit view automation/selection problem:** native/programmatic PDFKit
    extraction and the known-good text automation control succeed, but visible
    `PDFView` CGEvent selection/copy fails;
  - **WebKit/PDFKit behavior:** native/programmatic PDFKit extraction,
    known-good text automation, and visible `PDFView` selection/copy succeed,
    but standalone `WKWebView` PDF selection/copy fails;
  - **Surfari integration problem:** standalone `WKWebView` PDF selection/copy
    succeeds, but Surfari inside Ghostboard fails;
  - **no failure:** all controls, including Surfari, copy the marker.
- Protect the clipboard across the multi-probe run:
  - save the original clipboard exactly once at harness start;
  - restore it from a trap on every exit path;
  - use distinct per-probe sentinels;
  - record per-probe pasteboard change counts;
  - record final restoration status after all probes complete.
- Do not modify Ghostboard, Surfari, WebKit, protocol, or product code in this
  experiment unless the diagnostic harness itself cannot run because of an
  obvious harness bug.
- Update this experiment file with the result.

## Verification

Run hygiene checks:

```bash
bash -n scripts/test-issue-834-surfari-pdf-selection-source.sh
git diff --check
git -C webkit/src status --short
```

Run the diagnostic harness:

```bash
rm -rf logs/issue-834-exp45-surfari-pdf-selection-source
scripts/test-issue-834-surfari-pdf-selection-source.sh
```

Pass criteria:

- clipboard safety is preserved across the whole multi-probe run: original
  clipboard saved once, distinct per-probe sentinels written, after-copy state
  recorded, and original clipboard restored from the final trap path;
- the same marker and accepted substring are used in all probes;
- native/programmatic PDFKit extraction, known-good selectable text automation,
  visible PDFKit `PDFView`, standalone WKWebView PDF, and Surfari results are
  recorded separately;
- the harness records enough coordinate and screenshot evidence to show each
  probe targeted the visible text region;
- the result classifies the first failing layer into one of:
  - fixture problem;
  - automation problem;
  - PDFKit view automation/selection problem;
  - WebKit/PDFKit behavior;
  - Surfari integration problem;
  - no failure; using the explicit outcome matrix above;
- no product code is changed;
- completion review is recorded.

Partial criteria:

- at least one standalone control runs and produces useful evidence, but another
  control cannot be automated because of a macOS permission or windowing
  limitation;
- the diagnostic identifies a likely failing layer but cannot fully separate
  WebKit behavior from automation behavior;
- Surfari's Experiment 44 harness cannot be rerun, but the standalone probes
  still produce useful current evidence.

Failure criteria:

- none of the probes can display the fixture;
- the harness mutates the clipboard without restoring it;
- the result claims a failing layer without control evidence;
- product code is changed before the failure layer is classified.

## Design Review

An external Codex review checked the design.

Initial verdict: **Changes required**.

Findings:

- the design listed automation as a possible failing layer but tested standalone
  controls only through CGEvent drag and `Cmd+C`, so it could not distinguish
  fixture, coordinate, focus, or automation failures;
- the pass criteria required a first-failing-layer classification but did not
  define which probe outcomes prove each classification;
- clipboard safety needed to be explicit for a multi-probe harness so one
  probe's sentinel or copied value could not become the next probe's "original"
  clipboard state.

Resolution:

- added a native/programmatic PDFKit extraction control for fixture
  selectability;
- added a known-good selectable text automation control to prove CGEvent
  selection/copy works in the current macOS permission environment;
- added an explicit outcome matrix for fixture, automation, PDFKit view,
  WebKit/PDFKit, Surfari integration, and no-failure classifications;
- made multi-probe clipboard save/restore and distinct sentinels explicit.

Follow-up verdict: **Approved**.

The reviewer found no remaining required findings and approved the design for
the Experiment 45 plan commit.
