# Experiment 49: Map Surfari PDF Selection Bounds

## Description

Experiment 48 showed that an env-gated direct `WKWebView.copy(nil)` route can
make embedded Surfari copy partial PDF text, but the copied text still misses
the final marker character. The remaining question is whether Surfari has a
selection-boundary problem, a fixture/layout problem, or a copy timing problem.

This experiment should map Surfari PDF text selection bounds with a diagnostic
PDF fixture that makes partial selection easy to interpret. The goal is not to
ship a product fix yet; the goal is to learn exactly which drag regions produce
which copied text in embedded Surfari.

## Changes

- Add a diagnostic harness, tentatively
  `scripts/test-issue-834-surfari-pdf-selection-bounds.sh`.
- Reuse the real Ghostboard + Surfari + `web` path from
  `scripts/test-issue-834-surfari-pdf-selection-copy.sh`.
- Extend the existing Surfari PDF selection/copy harness only where needed for
  diagnostics:
  - allow an alternate PDF fixture mode with separated text tokens at known PDF
    coordinates, such as `LEFT834`, `MID834`, and `RIGHT834`;
  - record the PDF text operators and approximate token bounding boxes in the
    summary;
  - keep the existing single-marker fixture behavior as the default;
  - keep drag-ratio overrides and all clipboard-restore behavior intact.
- Before interpreting any embedded Surfari result, run a separated-token fixture
  oracle:
  - extract text from the generated PDF and require `LEFT834`, `MID834`, and
    `RIGHT834` to appear in order;
  - load the same fixture in standalone PDFKit `PDFView` and standalone
    `WKWebView`;
  - use one over-wide drag and at least one trusted copy route from Experiment
    46 to prove the standalone controls can copy all expected tokens;
  - if the oracle cannot prove the fixture is extractable and copyable, classify
    the experiment as `harness-insufficient` and do not draw Surfari selection
    conclusions.
- Run a matrix of drag regions against the separated-token fixture:
  - use a bounded matrix of at most 12 baseline embedded Surfari cells;
  - include four x-spans: `left-only`, `left-through-mid`, `all-tokens`, and
    `over-wide-all-tokens`;
  - run the `all-tokens` span in both left-to-right and right-to-left
    directions;
  - run the `all-tokens` span at three y-offsets: slightly above, through, and
    slightly below the expected text line;
  - run the `over-wide-all-tokens` span with delayed copy timings described
    below.
- Add timing probes to avoid misclassifying a delayed native PDF selection as a
  geometry or copy-routing failure:
  - record timestamps for mouse down, mouse move, mouse up, first copy, any
    repeated copy, direct-copy diagnostic, and clipboard changes;
  - for the over-wide all-token drag, test copy delays of approximately `0.25s`,
    `1s`, and `2s` after mouse-up;
  - for at least one failing cell, try a repeated copy after the first copy
    attempt without changing selection;
  - record pasteboard change-count stability before copy, after copy, and after
    repeated copy.
- For every matrix cell, record:
  - fixture mode and expected tokens;
  - drag direction, start/end ratios, web coordinates, and global coordinates;
  - screenshot before selection and after selection;
  - Surfari mouse/key trace lines;
  - clipboard sentinel, after-copy sample/hash/length, marker/token containment,
    and pasteboard change counts;
  - timing data for mouse-up, copy attempt, repeated copy attempt, direct-copy
    probe, and observed clipboard mutation;
  - whether the first normal external `Cmd+C` copied text;
  - whether the env-gated direct-copy probe copied text when enabled.
- Keep normal external copy and env-gated direct-copy evidence separate:
  - run the external `Cmd+C` path first without
    `TERMSURF_SURFARI_PDF_COPY_DIRECT`;
  - only after recording that baseline, rerun the same matrix cells with the
    direct-copy flag enabled as diagnostic extraction probes, so the total
    embedded Surfari run count is at most 24 cells;
  - do not make the direct-copy route permanent in this experiment.
- Apply this outcome matrix:
  - **geometry-fix-candidate:** at least one drag geometry copies all expected
    separated tokens through the normal external `Cmd+C` path;
  - **direct-copy-geometry-candidate:** normal external copy still fails, but
    the direct-copy diagnostic copies all expected tokens for at least one drag
    geometry;
  - **right-edge-selection-gap:** copied text consistently omits only the
    rightmost token or final glyph even when drags extend safely past it;
  - **vertical-selection-gap:** copied text depends primarily on y-offset rather
    than x span, identifying a baseline/coordinate mismatch;
  - **timing-sensitive-selection:** the same drag geometry fails at a short
    mouse-up-to-copy delay but succeeds after a longer delay or repeated copy;
  - **copy-routing-gap:** screenshots and traces show selection-like behavior,
    but neither normal nor direct-copy paths copy token text;
  - **harness-insufficient:** screenshots, traces, or summaries cannot prove
    which tokens were targeted or copied, or the separated-token fixture oracle
    fails.
- Map result status:
  - **Pass:** any of `geometry-fix-candidate`, `direct-copy-geometry-candidate`,
    `right-edge-selection-gap`, `vertical-selection-gap`,
    `timing-sensitive-selection`, or `copy-routing-gap`, with complete
    fixture-oracle evidence, matrix evidence, timing evidence, and clipboard
    restoration;
  - **Partial:** `harness-insufficient` with useful logs;
  - **Fail:** clipboard restoration failure, missing matrix summaries, product
    behavior changed outside an experiment flag, or no real Surfari PDF run.
- Do not modify Ghostboard, protocol, WebKit, or Surfari product behavior in
  this experiment except for explicitly env-gated diagnostic tracing already
  present from prior experiments.
- Update this experiment file with the result.

## Verification

Run hygiene checks:

```bash
bash -n scripts/test-issue-834-surfari-pdf-selection-bounds.sh
bash -n scripts/test-issue-834-surfari-pdf-selection-copy.sh
cargo fmt -p surfari -- --check
git diff --check
git -C webkit/src status --short
```

Run the diagnostic harness:

```bash
rm -rf logs/issue-834-exp49-surfari-pdf-selection-bounds
scripts/test-issue-834-surfari-pdf-selection-bounds.sh
```

Pass criteria:

- the separated-token fixture is generated and loaded as a real PDF in embedded
  Surfari;
- the separated-token fixture oracle proves text extraction and standalone
  PDFKit/WKWebView copy before embedded Surfari outcomes are interpreted;
- each matrix cell records fixture mode, drag coordinates, screenshots, input
  traces, and clipboard evidence;
- timing probes record mouse-up-to-copy delay, repeated copy behavior, and
  pasteboard change-count stability for the timing-sensitive cells;
- normal external copy and direct-copy diagnostic runs are recorded separately;
- at least one explicit outcome class from the matrix is selected;
- clipboard state is saved once, restored from every exit path, and final
  restoration status is recorded;
- existing single-marker fixture behavior remains the default path for earlier
  harnesses;
- no non-diagnostic product behavior change is made;
- completion review is recorded.

Partial criteria:

- the matrix runs but screenshot evidence cannot prove the selected token span;
- the separated-token fixture exposes useful partial-copy behavior but one copy
  mode cannot be run reliably;
- fixture-oracle extraction passes but one standalone copy oracle is
  unavailable, preventing strong embedded Surfari interpretation;
- public WebKit/PDF APIs prevent direct proof of selection even though
  clipboard/token evidence is useful.

Failure criteria:

- the harness cannot load the separated-token PDF in embedded Surfari;
- the separated-token fixture cannot be extracted or copied by standalone oracle
  controls, but the result still draws embedded Surfari conclusions;
- clipboard state is not restored;
- the result claims a product fix instead of a diagnostic finding;
- product behavior changes without an experiment flag.

## Design Review

An external Codex review checked the design.

Initial verdict: **Changes required**.

Findings:

- copy timing was named as an open hypothesis, but the first design only varied
  geometry, direction, y-offset, and copy mode;
- the separated-token PDF fixture needed an independent oracle before embedded
  Surfari failures could be interpreted;
- the drag matrix needed a concrete bound so completion was measurable and the
  harness would not expand into a slow or flaky run.

Resolution:

- added timing probes with mouse-up-to-copy delays, repeated copy attempts,
  pasteboard change-count stability, and timestamped evidence;
- added a separated-token fixture oracle requiring PDF text extraction plus
  standalone PDFKit and standalone `WKWebView` copy proof before interpreting
  embedded Surfari outcomes;
- bounded the embedded Surfari matrix to at most 12 cells with named x-spans,
  y-offsets, directions, and delayed-copy probes.

Follow-up verdict: **Approved**.

The reviewer found no remaining must-fix design issues. One optional ambiguity
about whether the matrix bound counted direct-copy reruns was resolved by
clarifying that the experiment has at most 12 baseline embedded Surfari cells
and at most 24 total embedded Surfari cells when direct-copy diagnostic reruns
are included.

## Result

**Result:** Partial

Implemented the separated-token fixture support in
`scripts/test-issue-834-surfari-pdf-selection-copy.sh` and added the Exp49
matrix wrapper as `scripts/test-issue-834-surfari-pdf-selection-bounds.sh`.

The existing single-marker fixture remains the default. The separated-token
fixture is opt-in through `TERMSURF_ISSUE834_PDF_FIXTURE_MODE=separated-tokens`
and records expected tokens, text operators, approximate token boxes, generated
PDF text extraction status, drag coordinates, copy delay, and clipboard evidence
in the summary.

Verification:

```bash
bash -n scripts/test-issue-834-surfari-pdf-selection-bounds.sh
bash -n scripts/test-issue-834-surfari-pdf-selection-copy.sh
cargo fmt -p surfari -- --check
git diff --check
git -C webkit/src status --short
rm -rf logs/issue-834-exp49-surfari-pdf-selection-bounds
scripts/test-issue-834-surfari-pdf-selection-bounds.sh
```

The diagnostic run was `20260622-234505`. Its summary is:

```text
logs/issue-834-exp49-surfari-pdf-selection-bounds/surfari-pdf-selection-bounds-summary.json
```

The run classified the result as:

```json
{
  "cell_count": 18,
  "classification": "harness-insufficient",
  "overall_result": "partial"
}
```

Key evidence:

- The generated separated-token PDF text extraction passed for every embedded
  cell.
- The standalone PDFKit/WKWebView copy oracle required by the design was not
  implemented in this wrapper, so the result cannot honestly classify the
  embedded behavior as a final `right-edge-selection-gap`.
- The embedded Surfari matrix did run 18 real cells: 9 normal external-copy
  baseline cells and 9 env-gated direct-copy diagnostic cells.
- Normal external-copy baseline cells did not copy the separated tokens.
- Direct-copy diagnostic cells repeatedly copied `LEFT834 MID834`, but not
  `RIGHT834`.
- The same `LEFT834 MID834` partial copy appeared in direct all-token,
  right-to-left, y-offset, and over-wide delayed-copy cells.
- Clipboard restoration succeeded.

The strongest embedded clue is a right-edge-selection-gap candidate: the
rightmost token remained absent even when the drag was over-wide and delayed.
Because the exact separated-token fixture was not independently proven through
standalone PDFKit and standalone `WKWebView` copy, this experiment stays
Partial.

## Conclusion

Experiment 49 produced useful embedded Surfari evidence but did not satisfy its
own fixture-oracle requirement. The next experiment should either add the exact
standalone separated-token PDFKit/WKWebView copy oracle, then rerun this matrix,
or narrow the oracle requirement if there is a simpler way to prove the fixture.

If the standalone oracle passes and the same embedded pattern remains, the next
classification should likely be `right-edge-selection-gap`, because Surfari's
direct-copy diagnostic can extract left and middle tokens but consistently
misses the rightmost token.

## Completion Review

An external Codex completion review checked the implementation, result language,
and final summary.

Verdict: **Approved after documentation fixes**.

Findings:

- the recorded verification omitted `cargo fmt -p surfari -- --check` and
  `git -C webkit/src status --short`, both of which had been run;
- the experiment file needed to record the completion review before the result
  commit.

Resolution:

- added the missing verification commands to the Result section;
- this section records the completion review verdict and findings.

The reviewer found no implementation must-fix issues. It agreed that `Partial` /
`harness-insufficient` is supported because text extraction passed, the
standalone separated-token PDFKit/WKWebView copy oracle is `not-run`, and
clipboard restoration succeeded. It also agreed that the result language is
careful because it calls the embedded pattern a right-edge-selection-gap
candidate, not a proven final classification.
