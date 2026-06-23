# Experiment 50: Prove Separated-Token PDF Copy Oracle

## Description

Experiment 49 mapped embedded Surfari selection geometry with a separated-token
fixture. The embedded direct-copy diagnostic repeatedly copied `LEFT834 MID834`
but missed `RIGHT834`, which looks like a right-edge selection gap. However,
Experiment 49 correctly stayed Partial because it did not prove that the exact
separated-token fixture is copyable in standalone PDFKit and standalone
`WKWebView`.

This experiment should fill that oracle gap only. It should prove whether the
exact separated-token PDF fixture used by Experiment 49 can be selected and
copied from standalone controls outside Ghostboard/Surfari. If the oracle
passes, the next experiment can rerun the embedded matrix and interpret the
right-edge pattern with stronger evidence.

## Changes

- Add or extend a standalone oracle harness, tentatively
  `scripts/test-issue-834-separated-token-copy-oracle.sh`.
- Reuse the proven standalone-control approach from
  `scripts/test-issue-834-pdf-copy-oracles.sh`:
  - temporary Swift probe app;
  - `NSTextView` known-good control;
  - PDFKit `PDFView`;
  - standalone `WKWebView` loading the generated PDF;
  - trusted copy routes from Experiment 46, especially CGEvent `Cmd+C` and
    in-process AppKit `copy:`.
- Generate the exact separated-token PDF fixture from Experiment 49:
  - tokens: `LEFT834`, `MID834`, `RIGHT834`;
  - text positions matching the embedded harness summary;
  - same font and page geometry;
  - a summary that records text operators and approximate token boxes.
- Prove fixture identity instead of relying on parallel reimplementation:
  - prefer reusing the same fixture-generation path/constants from
    `scripts/test-issue-834-surfari-pdf-selection-copy.sh`;
  - if the oracle harness generates the PDF independently, record a stream-level
    comparison against the embedded harness fixture metadata proving the page
    geometry, font, text operators, token text, and token positions match;
  - include the exact text-operator string and token-box JSON in the summary so
    Experiment 49 and Experiment 50 artifacts can be compared mechanically.
- Add a fixture extraction preflight:
  - parse the generated PDF text operators;
  - require all three tokens to appear in order;
  - fail before copy tests if extraction does not prove the fixture.
- For each standalone control and route, record:
  - control name and copy route;
  - window bounds and drag coordinates;
  - selected screenshot and after-copy screenshot;
  - clipboard sentinel, after-copy sample/hash/length, token containment, and
    pasteboard change counts;
  - whether all three expected tokens were copied.
- Use an over-wide drag that starts before `LEFT834` and ends safely past
  `RIGHT834`.
- Keep clipboard state safe:
  - save the original clipboard once at harness start;
  - restore it from a trap on every exit path;
  - use distinct sentinels per control/route attempt;
  - record final restoration status in the summary.
- Apply this outcome matrix:
  - **separated-token-oracle-pass:** PDF extraction passes and at least one
    trusted route copies all three tokens from both standalone PDFKit `PDFView`
    and standalone `WKWebView`;
  - **pdfkit-only-oracle-pass:** PDF extraction passes and PDFKit copies all
    tokens, but standalone `WKWebView` does not;
  - **webkit-only-oracle-pass:** PDF extraction passes and standalone
    `WKWebView` copies all tokens, but PDFKit does not;
  - **fixture-extraction-gap:** PDF extraction does not prove all three tokens
    in order;
  - **selection-or-copy-gap:** extraction passes, but neither PDFKit nor
    standalone `WKWebView` copies all three tokens through any trusted route;
  - **harness-insufficient:** required screenshots, clipboard evidence, or route
    evidence is missing.
- Map result status:
  - **Pass:** `separated-token-oracle-pass`, `pdfkit-only-oracle-pass`,
    `webkit-only-oracle-pass`, `fixture-extraction-gap`, or
    `selection-or-copy-gap`, with complete evidence and clipboard restoration;
  - **Partial:** `harness-insufficient` with useful logs;
  - **Fail:** clipboard restoration failure, missing summary, or no standalone
    PDFKit/WKWebView run.
- Add an explicit embedded-interpretation gate to the result:
  - only `separated-token-oracle-pass` authorizes rerunning the embedded Surfari
    matrix and interpreting a repeated `LEFT834 MID834` / missing-`RIGHT834`
    pattern as a likely embedded right-edge selection gap;
  - one-sided oracle outcomes (`pdfkit-only-oracle-pass` or
    `webkit-only-oracle-pass`) are useful, but they must narrow the next
    experiment to the failing native-control path before embedded Surfari is
    interpreted;
  - negative fixture/control outcomes (`fixture-extraction-gap` or
    `selection-or-copy-gap`) block embedded interpretation and should redirect
    the next experiment to fixture design or native-control selection/copy.
- Do not modify Ghostboard, Surfari, WebKit, or protocol code in this
  experiment. This is an oracle-only experiment.
- Update this experiment file with the result.

## Verification

Run hygiene checks:

```bash
bash -n scripts/test-issue-834-separated-token-copy-oracle.sh
git diff --check
git -C webkit/src status --short
```

Run the oracle harness:

```bash
rm -rf logs/issue-834-exp50-separated-token-copy-oracle
scripts/test-issue-834-separated-token-copy-oracle.sh
```

Pass criteria:

- the generated PDF fixture contains `LEFT834`, `MID834`, and `RIGHT834` in
  order;
- standalone PDFKit `PDFView` and standalone `WKWebView` are both exercised;
- each attempted copy starts from a unique sentinel and records independent
  clipboard evidence;
- at least one explicit outcome class from the matrix is selected;
- clipboard restoration succeeds;
- no product code is changed;
- the result states whether the embedded-interpretation gate is open or closed;
- completion review is recorded.

Partial criteria:

- extraction passes and one standalone control produces useful evidence, but
  another control cannot be launched or copied from reliably;
- screenshots or clipboard evidence are incomplete but still identify a likely
  next experiment.

Failure criteria:

- the harness cannot create or load the PDF fixture;
- clipboard state is not restored;
- the result draws embedded Surfari conclusions instead of oracle conclusions;
- product code changes outside the standalone oracle harness.

## Design Review

An external Codex review checked the design.

Initial verdict: **Changes required**.

Findings:

- the "exact separated-token fixture" requirement was not strict enough because
  a parallel fixture generator could drift from the embedded Surfari harness;
- the outcome matrix did not say which oracle outcomes actually authorize
  interpreting the embedded Surfari right-edge evidence.

Resolution:

- required reuse of the same fixture-generation path/constants where possible,
  or a stream/operator comparison proving the standalone and embedded fixtures
  match in page geometry, font, text operators, token text, and token positions;
- added an explicit embedded-interpretation gate: only
  `separated-token-oracle-pass` opens the path to rerun and interpret the
  embedded Surfari matrix directly.

Follow-up verdict: **Approved**.

The reviewer found no remaining must-fix design issues and approved the
Experiment 50 plan commit.
