# Experiment 11: Inventory Roamium Advanced PDF Surfaces

## Description

Experiments 2 through 10 prove most Roamium core PDF workflows, classify the
remaining native-print automation blocker, and leave the advanced Issue 798
surfaces unproven:

- forms;
- annotations;
- context menus;
- accessibility/searchify.

Issue 798 said the first advanced-feature step should be diagnostic: inventory
upstream support, identify TermSurf integration points, and choose the smallest
feature slice to prove end to end. This experiment should do that for current
Roamium without changing product behavior.

The goal is not to implement advanced PDF features yet. The goal is to produce
current evidence for which advanced surfaces are already observable through the
real TermSurf/Roamium PDF path, which surfaces need a better fixture/probe, and
which surface should become the next implementation experiment.

## Changes

1. Audit current Chromium PDF advanced-surface support.

   Inspect the current Chromium checkout, especially:

   - `chrome/browser/resources/pdf/pdf_viewer.html`;
   - `chrome/browser/resources/pdf/pdf_viewer.ts`;
   - `chrome/browser/resources/pdf/controller.ts`;
   - `chrome/browser/resources/pdf/pdf_internal_plugin_wrapper.ts`;
   - `chrome/browser/resources/pdf/ink2_manager.ts`;
   - `chrome/browser/resources/pdf/constants.ts`;
   - relevant `pdf/` and `components/pdf/` sources for form, annotation,
     context-menu, accessibility, and searchify messages.

   Record source-level integration points in the experiment result. Do not edit
   Chromium source in this experiment.

2. Add a focused advanced-surface probe harness.

   Add `scripts/test-issue-834-pdf-advanced.py` and, if useful,
   `scripts/probe-pdf-advanced.mjs`.

   The harness should:

   - launch repo-built `chromium/src/out/Default/roamium`;
   - serve generated fixtures from a local HTTP server;
   - create a tab through the TermSurf protocol;
   - resize and focus the tab;
   - attach DevTools to the PDF viewer path and any PDF extension child target;
   - write `<log-dir>/pdf-advanced-summary.json`.

3. Generate deterministic fixtures in the log directory.

   Generate fixtures instead of committing binaries unless the result proves a
   static fixture is required. Include at least:

   - an AcroForm-style PDF with a visible text field and checkbox;
   - a PDF with an existing text annotation or link/comment marker if a small
     deterministic fixture can be generated safely;
   - a normal valid control PDF, using `test-html/public/bitcoin.pdf` or a
     generated minimal PDF.

   If an annotation fixture cannot be generated cheaply, record that as
   `annotation-fixture-gap` and still audit the UI/control availability.

4. Probe forms.

   The forms probe should establish the strongest stable evidence available
   without product changes:

   - the form PDF loads as a PDF;
   - the viewer/plugin reports document success;
   - clicking at the expected text-field coordinates is routed through the
     TermSurf protocol;
   - keyboard input is sent after the click;
   - any stable Chromium observable for form focus, typed value, screenshot
     change, or plugin/viewer state is recorded.

   If no stable form-value observable exists, classify the first missing layer
   instead of claiming support.

5. Probe annotation UI availability.

   The annotation probe should record:

   - `loadTimeData` flags related to Ink/annotation support;
   - whether annotation controls are present, hidden, disabled, or absent;
   - whether entering annotation mode is possible through the PDF viewer UI;
   - whether any stable state such as `annotationMode_`,
     `hasCommittedInk2Edits_`, side-panel controls, or toolbar controls changes.

   Do not claim annotation support from source presence alone. If the UI is
   disabled by flags or missing integration, classify it.

6. Probe context-menu behavior safely.

   The context-menu probe must not send a real right-click at plugin coordinates
   unless a native-menu watcher is already ready. Watcher readiness must be
   proven before the click, and the harness must have a `finally`/cleanup path
   that sends Escape or otherwise dismisses any opened menu.

   If no safe native-menu watcher is available, skip the right-click entirely
   and classify `context-menu-native-watcher-missing` or
   `context-menu-observation-gap`.

   If the watcher is ready, the probe should record:

   - whether the protocol right-click was sent at PDF plugin coordinates;
   - whether Roamium/PDF input trace saw the event;
   - whether DevTools, DOM state, screenshots, or another stable signal changed;
   - whether a native menu appeared and was dismissed safely.

7. Probe accessibility/searchify status.

   The accessibility/searchify probe should record source and runtime evidence
   without claiming implementation from source presence alone:

   - `loadTimeData` flags and viewer properties related to searchify or
     accessibility;
   - whether the searchify progress toast or related state is present, disabled,
     hidden, or absent;
   - whether the plugin/viewer exposes stable accessibility/searchify state
     through DevTools;
   - source-level Chromium integration points for searchify and accessibility
     messages.

   Accessibility/searchify is optional in the Issue 834 matrix, but this
   experiment still must classify its current Roamium status because it is part
   of the advanced-surface inventory.

8. Classify first failing layer per surface.

   Use named classifications:

   - `fixture-generation-gap`;
   - `pdf-load-failed`;
   - `devtools-target-discovery-failed`;
   - `protocol-input-not-sent`;
   - `roamium-input-trace-missing`;
   - `form-focus-observable-missing`;
   - `form-value-observable-missing`;
   - `annotation-ui-disabled-by-flags`;
   - `annotation-ui-missing`;
   - `annotation-state-observable-missing`;
   - `context-menu-observation-gap`;
   - `context-menu-native-watcher-missing`;
   - `accessibility-searchify-disabled-by-flags`;
   - `accessibility-searchify-observable-missing`;
   - `accessibility-searchify-source-only`;
   - `no-failure-observed`.

9. Do not fix product code in this experiment.

   This experiment is diagnostic/probe-only. If a real product gap is found,
   record it and design the next experiment around the smallest concrete fix. Do
   not modify Chromium, Roamium, Ghostboard, Surfari, WebKit, protocol, or other
   product source.

## Verification

Verification for the completed result is:

```bash
python3 scripts/test-issue-834-pdf-advanced.py \
  --log-dir logs/issue-834-exp11-advanced-forms \
  --probe forms

python3 scripts/test-issue-834-pdf-advanced.py \
  --log-dir logs/issue-834-exp11-advanced-annotations \
  --probe annotations

python3 scripts/test-issue-834-pdf-advanced.py \
  --log-dir logs/issue-834-exp11-advanced-context-menu \
  --probe context-menu

python3 scripts/test-issue-834-pdf-advanced.py \
  --log-dir logs/issue-834-exp11-advanced-accessibility-searchify \
  --probe accessibility-searchify
```

Required checks:

- the source audit lists the current Chromium integration points for forms,
  annotations, context menus, and accessibility/searchify;
- every probe writes `pdf-advanced-summary.json`;
- generated fixture paths, byte sizes, and generation status are recorded;
- each surface records a named first failing layer or `no-failure-observed`;
- the context-menu probe does not send right-click unless watcher readiness is
  proven first, and records cleanup/dismissal evidence if any menu is opened;
- no product source files are changed;
- any new Node helper passes `node --check`;
- any new Python helper passes
  `PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile`, and `scripts/__pycache__/`
  is removed afterward;
- markdown is formatted with Prettier;
- `git diff --check` passes;
- design review is recorded, all required design-review findings are fixed, the
  design is approved, and the plan commit exists before implementation begins;
- completion review is recorded before the result commit.

## Pass Criteria

This experiment passes if all four advanced surface probes run through the real
TermSurf/Roamium PDF path, produce stable evidence, and classify each surface
with either `no-failure-observed` or a concrete named missing layer that is
actionable for the next experiment.

## Partial Criteria

This experiment is partial if at least one advanced surface is probed and
classified, but another surface cannot be classified because of a fixture or
automation gap that needs a follow-up probe experiment.

## Failure Criteria

This experiment fails if the harness cannot load a valid PDF control, changes
product source, claims support based only on source presence, leaves a native
context menu open, or records advanced-surface status without stable evidence.

## Design Review

An adversarial Codex subagent reviewed the design with fresh context.

Initial verdict: **Changes Required**.

Required findings:

- Accessibility/searchify was listed in the experiment scope but excluded from
  runnable probes and pass criteria. The design now includes an
  accessibility/searchify probe, named classifications, a verification command,
  and pass criteria covering all four advanced surfaces.
- Context-menu safety was not enforceable before a right-click could be sent.
  The design now requires native-menu watcher readiness before any real
  right-click at plugin coordinates; if no watcher is available, the harness
  must skip the click and classify the gap. It also requires cleanup/dismissal
  evidence if a menu is opened.

Re-review verdict: **Approved**.
