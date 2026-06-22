# Experiment 13: Fix Roamium PDF Form Sequences

## Description

Experiment 12 proved that individual Roamium PDF form controls work when tested
in fresh document instances:

- the text field accepted typed input with localized screenshot evidence;
- the checkbox toggled with localized screenshot evidence.

It also found a same-document sequence gap:

- `text-then-checkbox` failed with `form-checkbox-state-missing`;
- `checkbox-then-text` failed with `form-text-value-missing`.

This experiment should diagnose and fix, or precisely classify, same-document
multi-control PDF form interaction. The goal is to determine whether the gap is
caused by the hand-generated AcroForm fixture, Chromium PDF form focus behavior,
or TermSurf/Roamium focus and input routing.

Do not broaden this experiment to annotations, context menus, native print,
Surfari/WebKit, or non-form PDF behavior.

## Changes

1. Extend the forms harness with sequence diagnostics.

   Update `scripts/test-issue-834-pdf-forms.py` and
   `scripts/probe-pdf-forms.mjs` so the sequence scenarios record more than
   screenshot diffs. At minimum, record state before and after each interaction:

   - PDF viewer/plugin load state;
   - `document.activeElement`;
   - viewer properties related to form focus, especially `formFieldFocus_` and
     `documentHasFocus_` if exposed;
   - plugin rect, page rect, field screen rects, and click coordinates;
   - focused/active state changes visible through DevTools;
   - Roamium input trace lines for every mouse and keyboard event.

2. Validate the fixture before blaming product code.

   The deterministic AcroForm fixture must be checked for validity and
   interactivity assumptions. Use available local tools such as `qpdf --check`
   and source-level PDF inspection. If the fixture is invalid or underspecified,
   fix the fixture first and rerun Experiment 12-style scenarios.

   The result must explicitly answer whether the fixture is good enough to
   support a product conclusion.

3. Test focus-reset variants before editing product source.

   Add sequence variants that try small, user-realistic focus resets between
   controls, such as:

   - text, click page background, checkbox;
   - text, Escape, checkbox;
   - checkbox, click page background, text;
   - checkbox, Escape, text;
   - double-clicking the second control if single-click focus transfer is the
     only failing behavior.

   Record each variant separately with a named result. Do not choose a
   workaround as the product behavior unless it matches a reasonable user action
   and is documented as such.

4. Identify the first failing layer.

   Use named classifications:

   - `fixture-generation-gap`;
   - `pdf-load-failed`;
   - `devtools-target-discovery-failed`;
   - `form-geometry-observable-missing`;
   - `protocol-input-not-sent`;
   - `roamium-input-trace-missing`;
   - `form-focus-transfer-missing`;
   - `form-text-value-missing`;
   - `form-checkbox-state-missing`;
   - `form-sequence-workaround-required`;
   - `product-fix-required`;
   - `no-failure-observed`.

5. Make product changes only if diagnostics prove they are required.

   If the sequence failure is caused by TermSurf/Roamium input routing, make the
   smallest required product fix and rerun all Experiment 12 individual and
   sequence scenarios. Possible areas include mouse focus transfer, key target
   routing after PDF form focus changes, or PDF plugin focus state restoration.

   If Chromium source under `chromium/src/` must be modified:

   - create a fresh Issue 834 Chromium branch before editing;
   - update `chromium/README.md` with the branch;
   - build the affected target;
   - regenerate the Issue 834 Chromium patch archive.

   If the root cause is fixture quality or Chromium-native behavior that does
   not require a TermSurf product change, record that conclusion and do not edit
   product source.

## Verification

Verification for the completed result is:

```bash
node --check scripts/probe-pdf-forms.mjs

PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  scripts/test-issue-834-pdf-forms.py

python3 scripts/test-issue-834-pdf-forms.py \
  --log-dir logs/issue-834-exp13-roamium-pdf-form-sequences

git diff --check
```

If the experiment changes product source, also run the relevant build/test
commands and rerun the Experiment 12 final forms probe against the rebuilt
binary. If any Rust source changes, run `cargo fmt` and accept its output before
running the relevant Rust build/test command. Record all product-change
verification commands before completion review.

Required evidence:

- fixture validity is checked and recorded;
- every same-document sequence and focus-reset variant records a named
  classification;
- the summary records interaction-by-interaction DevTools state, geometry,
  screenshots, and Roamium input traces;
- the result explains whether the first failing layer is fixture, Chromium PDF
  form behavior, or TermSurf/Roamium integration;
- no non-form PDF behavior is changed without evidence that the forms fix
  requires it;
- markdown is formatted with Prettier;
- any Node helper passes `node --check`;
- any Python helper passes `PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile`,
  and `scripts/__pycache__/` is removed afterward;
- `git diff --check` passes;
- design review is recorded, all required design-review findings are fixed, the
  design is approved, and the plan commit exists before implementation begins;
- completion review is recorded before the result commit.

## Pass Criteria

This experiment passes if same-document text-field and checkbox interactions
work in both orders through the real TermSurf/Roamium PDF path, with stable
evidence for text value and checkbox state.

## Partial Criteria

This experiment is partial if it does not fully fix same-document form
sequences, but it proves the first failing layer and leaves a concrete next
implementation step.

## Failure Criteria

This experiment fails if it claims a product bug before validating the fixture,
relies on uncalibrated coordinates, changes product source before diagnostics
require it, or records sequence status without stable per-interaction evidence.

## Design Review

An adversarial Codex subagent reviewed the design with fresh context.

Initial verdict: **Changes Required**.

Required finding:

- The design allowed TermSurf/Roamium input-routing fixes but did not explicitly
  require `cargo fmt` if Rust source changed. The verification section now
  requires `cargo fmt` for Rust source changes, accepts formatter output, and
  requires product-change verification commands to be recorded before completion
  review.

Re-review verdict: **Approved**.
