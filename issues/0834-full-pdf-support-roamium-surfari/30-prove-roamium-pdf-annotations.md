# Experiment 30: Prove Roamium PDF Annotations

## Description

Experiment 11 turned Roamium PDF annotations from a broad unknown into a named
gap. The existing advanced probe only served a placeholder `annotation.pdf`, and
it classified the row as `annotation-state-observable-missing` because the
Chromium PDF viewer exposed an annotations menu item with a zero-size rect and
did not prove a rendered annotation or usable annotation-editing state.

Since then, forms and native print were handled separately. The next Roamium
matrix gap with a bounded probe path is annotations. This experiment should
prove what Roamium currently supports for PDF annotations through TermSurf:

- rendering of existing annotations embedded in a PDF;
- viewer/toolbar state relevant to annotation display and editing;
- whether annotation editing controls are available, disabled by Chromium
  feature flags, hidden in the menu, or blocked by TermSurf integration.

The experiment should not try to force Chromium annotation editing on if the
current Chromium build has it disabled by product flags. The goal is to separate
TermSurf integration bugs from current Chromium/PDF viewer capability and record
the result conservatively.

## Changes

- Update `scripts/test-issue-834-pdf-advanced.py`.
- Update `scripts/probe-pdf-advanced.mjs`.
- Add or extend deterministic PDF annotation fixture generation inside the
  harness.
- Keep the experiment limited to Roamium PDF annotations. Do not modify
  Surfari/WebKit, Ghostboard, protocol, Roamium process code, Chromium product
  code, native print code, forms code, or context-menu/accessibility probes
  unless a very small shared helper is needed by the annotation harness.

Implementation should add a real annotation fixture instead of the current
minimal placeholder. The fixture should be deterministic, generated locally in
the log directory, and should contain at least:

- a control PDF with the same base page but no annotation;
- an annotated PDF with a visibly rendered annotation in a known page region,
  such as a square, highlight, text, or free-text annotation;
- metadata in `fixtures.json` describing the annotation type, page coordinates,
  expected viewport region, and generator method.

The probe should collect stronger evidence than Experiment 11:

1. Load the control PDF and annotated PDF through the same Roamium/TermSurf
   harness path.
2. Capture screenshots after the PDF plugin reports successful load.
3. Crop or sample the expected annotation region and compare the annotated PDF
   against the control PDF so annotation rendering is proven by pixels, not only
   DOM state.
4. Collect PDF viewer state from DevTools, including:
   - `viewerProps.annotationMode_`;
   - `viewerProps.hasEdits_`;
   - `viewerProps.hasUnsavedEdits_`;
   - `viewerProps.hasCommittedInk2Edits_`;
   - `toolbarProps.annotationAvailable`;
   - `toolbarProps.annotationMode`;
   - `toolbarProps.pdfInk2Enabled`;
   - `toolbarProps.pdfTextAnnotationsEnabled_`;
   - the More Actions menu and any annotation-related controls.
5. If annotation editing controls are discoverable but hidden behind the More
   Actions menu, safely click the menu through DevTools DOM input and recapture
   state. Do not use native menu automation in this experiment.
6. Classify the row with one of:
   - `no-failure-observed` when existing annotation rendering is proven and
     annotation editing is either available or explicitly classified;
   - `annotation-rendering-failed`;
   - `annotation-fixture-generation-gap`;
   - `annotation-pixel-proof-missing`;
   - `annotation-editing-disabled-by-flags`;
   - `annotation-editing-ui-hidden`;
   - `annotation-editing-state-observable-missing`;
   - `pdf-load-failed`;
   - the existing setup failure hops.

If Chromium exposes annotation display but not editing, the result should say so
directly. It should not mark annotation editing as a TermSurf failure unless the
evidence shows that Chromium's controls should be available and TermSurf input,
layout, or IPC prevents them from working.

## Verification

Run syntax and hygiene checks:

```bash
node --check scripts/probe-pdf-advanced.mjs
rm -rf scripts/__pycache__
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  scripts/test-issue-834-pdf-advanced.py
rm -rf scripts/__pycache__
git diff --check
git -C chromium/src diff --check
```

Run the annotation probe:

```bash
rm -rf logs/issue-834-exp30-roamium-pdf-annotations
python3 scripts/test-issue-834-pdf-advanced.py \
  --log-dir logs/issue-834-exp30-roamium-pdf-annotations \
  --probe annotations
```

Inspect the summary:

```bash
python3 - <<'PY'
import json
from pathlib import Path

summary = json.loads(
    Path(
        "logs/issue-834-exp30-roamium-pdf-annotations/"
        "pdf-advanced-summary.json"
    ).read_text()
)
print(json.dumps({
    "first_failing_hop": summary.get("first_failing_hop"),
    "probe_status": summary.get("probe_status"),
    "rendering": summary.get("annotation_rendering"),
    "editing": summary.get("annotation_editing"),
}, indent=2, sort_keys=True))
PY
```

Pass criteria:

- the harness generates deterministic control and annotated PDF fixtures;
- both fixtures load through Roamium over the TermSurf protocol;
- screenshot or pixel evidence proves the expected annotation region differs
  from the control PDF in the annotated fixture;
- the summary records annotation rendering evidence and annotation editing
  state;
- if editing is unavailable, the summary names the specific reason, such as
  `annotation-editing-disabled-by-flags`;
- no native OS UI is opened;
- no unrelated product code is changed;
- hygiene checks pass.

Partial criteria:

- existing annotation rendering is proven, but editing availability remains
  unclassified;
- or fixture generation works, but the pixel proof exposes a concrete Roamium or
  Chromium failing hop.

Failure criteria:

- the experiment treats a placeholder PDF with no real annotation as proof;
- DOM state alone is used to claim annotation rendering;
- the probe mutates native OS UI or unrelated PDF workflows;
- a broad Chromium or Roamium product change is made before the annotation
  failing layer is identified;
- the README or experiment result overstates disabled Chromium editing UI as
  working TermSurf annotation support.

## Design Review

An adversarial Codex subagent reviewed the design with fresh context.

Verdict: **Approved**.

The reviewer found no findings. It confirmed that the README links Experiment 30
as `Designed`, the experiment has the required Description, Changes, and
Verification sections, the scope is limited to the remaining Roamium annotation
gap, the plan is technically plausible, and the verification criteria do not
overclaim annotation editing support when Chromium flags disable it.

## Result

**Result:** Pass

Implemented deterministic Roamium PDF annotation proof in the advanced PDF
harness.

Changes:

- `scripts/test-issue-834-pdf-advanced.py` now generates:
  - `annotation-control.pdf`, a matching control PDF with no annotation;
  - `annotation.pdf`, a deterministic square annotation PDF with a yellow fill,
    red border, and an appearance stream;
  - fixture metadata with the annotation type, page size, PDF-space rectangle,
    and expected rendered region.
- `scripts/probe-pdf-advanced.mjs` now preserves attached DevTools iframe
  sessions across polls and adds an annotation path that captures the control
  PDF, navigates to the annotated PDF, waits for the PDF plugin again, and
  captures the annotated state.
- The Python harness decodes Chromium screenshots with stdlib PNG/zlib code,
  maps the expected PDF annotation rectangle into screenshot pixels, compares
  the annotated crop against the control crop, and records
  `annotation_rendering` evidence.
- The harness records `annotation_editing` state from viewer and toolbar
  properties instead of treating disabled Chromium annotation editing UI as a
  TermSurf failure.

Verification run:

```bash
rm -rf logs/issue-834-exp30-roamium-pdf-annotations
python3 scripts/test-issue-834-pdf-advanced.py \
  --log-dir logs/issue-834-exp30-roamium-pdf-annotations \
  --probe annotations

rm -rf scripts/__pycache__
node --check scripts/probe-pdf-advanced.mjs
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  scripts/test-issue-834-pdf-advanced.py
rm -rf scripts/__pycache__
qpdf --check \
  logs/issue-834-exp30-roamium-pdf-annotations/fixtures/annotation-control.pdf
qpdf --check \
  logs/issue-834-exp30-roamium-pdf-annotations/fixtures/annotation.pdf
git diff --check
git -C chromium/src diff --check

rm -rf logs/issue-834-exp30-advanced-forms-smoke
python3 scripts/test-issue-834-pdf-advanced.py \
  --log-dir logs/issue-834-exp30-advanced-forms-smoke \
  --probe forms
```

Final annotation evidence:

- `logs/issue-834-exp30-roamium-pdf-annotations/pdf-advanced-summary.json`
  recorded `first_failing_hop = "no-failure-observed"` and
  `probe_status = "ok"`.
- Both fixtures loaded through Roamium over the TermSurf protocol:
  `/annotation-control.pdf` and `/annotation.pdf` each returned HTTP 200.
- The DevTools summary recorded `annotationControl.pluginLoaded = true` with
  title `annotation-control.pdf`, and `annotationAnnotated.pluginLoaded = true`
  with title `annotation.pdf`.
- The pixel proof compared the expected annotation region between the control
  and annotated screenshots:
  - load proof: both control and annotated plugins loaded, with expected titles,
    viewer `originalUrl` values, and viewer `fileName_` values;
  - region: `x1 = 814`, `y1 = 275`, `x2 = 987`, `y2 = 400`;
  - pixels sampled: `21625`;
  - changed pixels: `16692`;
  - changed ratio: `0.771884`;
  - mean RGB distance: `255.366`;
  - comparison status: `pass`.
- The annotation editing state recorded:
  - `toolbarAnnotationAvailable = true`;
  - `toolbarPdfInk2Enabled = false`;
  - `toolbarPdfTextAnnotationsEnabled = false`;
  - `annotationMode = "off"`;
  - `hasEdits = false`;
  - `hasUnsavedEdits = false`;
  - `hasCommittedInk2Edits = false`.
- The editing classification is `annotation-editing-disabled-by-flags`, which
  means existing annotation rendering is proven and annotation editing is not
  currently available in this Chromium viewer configuration.

Regression sanity:

- The existing advanced forms probe still runs and records the prior expected
  classification, `form-value-observable-missing`, with
  `roamium_mouse_event_line = true`. That confirms the shared DevTools session
  tracking change did not break the non-annotation advanced harness path.

## Conclusion

The Roamium annotations row is now proven for existing PDF annotation rendering.
TermSurf can display embedded PDF annotations through Roamium, and the evidence
is pixel-based rather than DOM-only.

Chromium's annotation editing controls are present at the metadata level but
disabled by current viewer flags in this build (`pdfInk2Enabled = false`,
`pdfTextAnnotationsEnabled = false`). That is recorded as an engine/viewer
capability classification, not a TermSurf integration failure.

The next Issue 834 Roamium experiment should target the remaining advanced
surfaces from Experiment 11: context menus or accessibility/searchify. Context
menus likely need a safe native-menu watcher before right-click automation can
be considered a pass.

## Completion Review

An adversarial Codex subagent reviewed the completed experiment with fresh
context.

Initial verdict: **Changes required**.

Required finding:

- The annotation rendering classifier could pass from a screenshot comparison
  without requiring proof that the control PDF plugin loaded. Because the
  DevTools helper can return the last collected state on timeout, a missing
  control plugin could still yield a screenshot and then be compared against the
  annotated screenshot.

Fix:

- Added `annotation_load_proof()` to require both the control and annotated PDF
  plugin states to be loaded before accepting pixel proof.
- The proof now checks:
  - `annotationControl.pluginLoaded = true`;
  - `annotationAnnotated.pluginLoaded = true`;
  - expected control and annotated titles;
  - expected viewer `originalUrl` values;
  - expected viewer `fileName_` values.
- If any load-proof check fails, `annotation_rendering.status` fails before the
  PNG comparison can count.

Additional verification after the fix:

- A direct helper probe confirmed that missing control plugin proof returns
  `status = "fail"` and
  `first_failing_hop = "annotation-pdf-load-proof-missing"`.
- The full annotation probe was rerun and passed with
  `annotation_rendering.load_proof.status = "pass"`.
- The advanced forms smoke probe was rerun and still recorded the prior expected
  classification, `form-value-observable-missing`.

Final verdict after re-review: **Approved**.

The re-review found no findings.
