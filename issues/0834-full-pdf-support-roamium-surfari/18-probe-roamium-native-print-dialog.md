# Experiment 18: Probe Roamium Native Print Dialog

## Description

Experiments 10, 16, and 17 established the safety requirements for native PDF
print testing:

- contained Roamium PDF print reaches the contained intercept;
- System Events cannot be relied on in this VM because assistive access is
  denied;
- CoreGraphics can observe harmless native dialogs;
- Accessibility `AXPress` can press a harmless dialog's Cancel button and the
  dialog reports `User canceled. (-128)`;
- no production print click should happen unless the harmless preflight passes.

This experiment should finally test the real Roamium PDF native print path. It
may click the production PDF print control only behind the proven
CoreGraphics/Accessibility watcher and the explicit
`--allow-native-dialog-click` flag. It must never submit a print job.

## Changes

1. Reconfirm the safety preflight.

   Run the preflight-only watcher probe from Experiment 17:

   ```bash
   python3 scripts/test-issue-834-pdf-native-print.py \
     --log-dir logs/issue-834-exp18-native-print-preflight \
     --probe watcher-preflight
   ```

   Required evidence:

   - `safe_for_production_print_probe = true`;
   - `selected_mechanism = "accessibility-press-cancel-button"`;
   - no production print click was attempted;
   - print queues before and after are unchanged.

2. Run the guarded production native-dialog probe.

   Run:

   ```bash
   python3 scripts/test-issue-834-pdf-native-print.py \
     --log-dir logs/issue-834-exp18-native-print-dialog \
     --probe native-dialog \
     --allow-native-dialog-click
   ```

   The harness must:

   - rerun the same harmless preflight internally before the production click;
   - launch repo-built Roamium;
   - serve the deterministic Bitcoin PDF fixture;
   - create a tab through the TermSurf protocol;
   - capture initial print queue state;
   - click the PDF viewer's production print control only after the preflight
     passes;
   - observe the native Print/Printer dialog using CoreGraphics;
   - press its Cancel button using Accessibility;
   - verify the dialog disappears;
   - capture final print queue state;
   - write `pdf-native-print-summary.json`.

3. Classify the result.

   Use the existing native-print classifications:

   - `native-print-safety-gate-not-ready`;
   - `native-print-control-missing`;
   - `native-print-disabled-by-load-time-flags`;
   - `native-print-click-not-sent`;
   - `native-print-click-sent-no-dialog`;
   - `native-print-dialog-seen-cancelled`;
   - `native-print-dialog-seen-cancel-failed`;
   - `native-print-job-submitted-unexpectedly`;
   - `native-print-observation-gap`.

   If a native dialog appears and is cancelled with no queue change, the result
   should be Pass. If the click is sent but no dialog appears, or cancellation
   fails, record Partial with the first failing hop.

4. Do not make product changes in this experiment.

   This is a guarded probe/classification experiment. If the probe exposes a
   real Roamium, Chromium, Ghostboard, or protocol gap, record it and design a
   follow-up implementation experiment.

## Verification

Verification for the completed result is:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  scripts/test-issue-834-pdf-native-print.py

rm -rf scripts/__pycache__

python3 scripts/test-issue-834-pdf-native-print.py \
  --log-dir logs/issue-834-exp18-native-print-preflight \
  --probe watcher-preflight

python3 scripts/test-issue-834-pdf-native-print.py \
  --log-dir logs/issue-834-exp18-native-print-dialog \
  --probe native-dialog \
  --allow-native-dialog-click

git diff --check
```

Required evidence:

- the preflight command passes before the native-dialog command;
- the native-dialog command records its own successful internal preflight before
  any production print click;
- production print is clicked only with `--allow-native-dialog-click`;
- native dialog observation, cancellation, and disappearance evidence are
  recorded;
- print queue state before and after is recorded;
- no print job is submitted;
- if the native dialog does not appear, the result records bridge/control/click
  evidence before classifying the failure;
- Python bytecode cache is removed after compilation;
- markdown is formatted with Prettier;
- `git diff --check` passes;
- design review is recorded, all real design-review findings are fixed, the
  design is approved, and the plan commit exists before implementation begins;
- completion review is recorded before the result commit.

## Pass Criteria

This experiment passes if Roamium opens the native PDF print dialog from the
real PDF viewer path, the proven CoreGraphics/Accessibility watcher cancels it,
the dialog disappears, and print queue evidence proves no real print job was
submitted.

## Partial Criteria

This experiment is partial if the safety preflight fails and no production print
click occurs, or if the safety preflight passes but the real native print path
does not open a dialog, cannot be observed, cannot be cancelled, or exposes a
product gap that requires a follow-up implementation experiment.

## Failure Criteria

This experiment fails if it submits a print job, clicks production print without
the preflight and explicit allow flag, ignores a print queue change, or claims
native print support without native dialog and no-job evidence.

## Design Review

An adversarial Codex subagent reviewed the design with fresh context.

Initial verdict: **Changes Required**.

Required finding:

- the Partial criteria omitted the safety-gate-not-ready path, even though the
  design listed that as a valid classification.

Optional finding:

- the verification command block omitted `rm -rf scripts/__pycache__` after
  Python compilation.

Fixes:

- Partial criteria now include safety preflight failure with no production print
  click;
- verification now includes bytecode cache cleanup.

Re-review verdict: **Approved**.

The reviewer found no remaining issues.

## Result

**Result:** Partial

The guarded production native-dialog probe ran after the harmless preflight
passed. The harness compiled, the standalone watcher preflight passed, and the
native-dialog run recorded its own successful internal preflight before allowing
the production print click.

Standalone preflight evidence from
`logs/issue-834-exp18-native-print-preflight/native-dialog-preflight-summary.json`:

- `overall_result = "pass"`;
- `first_failing_hop = "no-failure-observed"`;
- `selected_mechanism = "accessibility-press-cancel-button"`;
- `safe_for_production_print_probe = true`;
- `production_print_click_attempted = false`.

Production native-dialog evidence from
`logs/issue-834-exp18-native-print-dialog/pdf-native-print-summary.json`:

- `safety_gate_passed = true`;
- internal `preflight.passed = true`;
- internal `preflight.selected_mechanism = "accessibility-press-cancel-button"`;
- `server_register_received = true`;
- `tab_ready_id = 1`;
- `probe_status = "ok"`;
- `probe_summary.print.status = "print-native-click-sent"`;
- `probe_summary.print.clicked = true`;
- `probe_summary.print.controlFound = true`;
- the PDF viewer JavaScript emitted `viewer-on-print`, `controller-print`, and
  `post-message` records for activation `native-print-1`;
- native Chromium trace lines reached
  `pdf_view_web_plugin.cc event=handle-print`;
- `print_dialog_watch.dialog_observed = false`;
- `print_dialog_watch.cancel_sent = false`;
- `first_failing_hop = "native-print-click-sent-no-dialog"`;
- `lpstat -o` and `lpstat -W completed -o` were empty before and after the
  probe, so no print job was submitted.

The product path is therefore still not proven complete. Roamium successfully
reaches the PDF print control and native print bridge, but no observable macOS
native Print/Printer dialog appears in this VM during the watcher window.

Verification run:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  scripts/test-issue-834-pdf-native-print.py

rm -rf scripts/__pycache__

python3 scripts/test-issue-834-pdf-native-print.py \
  --log-dir logs/issue-834-exp18-native-print-preflight \
  --probe watcher-preflight

python3 scripts/test-issue-834-pdf-native-print.py \
  --log-dir logs/issue-834-exp18-native-print-dialog \
  --probe native-dialog \
  --allow-native-dialog-click

git diff --check
```

The native-dialog command exited nonzero because it classified the result as
`native-print-click-sent-no-dialog`, not because it submitted a print job.

## Completion Review

An adversarial Codex subagent reviewed the completed experiment with fresh
context.

Verdict: **Approved**.

The reviewer found no Required findings.

Optional finding:

- The recorded verification block omitted `git diff --check`, even though the
  experiment's verification plan required it.

Fix:

- Added `git diff --check` to the recorded verification block.

## Conclusion

Experiment 18 proved that the automation safety gate is strong enough to click
the real Roamium PDF print control without submitting a job. It did not prove
native print support, because the production click reached Chromium's PDF print
bridge but did not surface an observable native print dialog.

The next experiment should investigate why Chromium's PDF
`pdf_view_web_plugin.cc` print path reaches `handle-print` but does not display
the macOS print dialog. That experiment should compare the current TermSurf
Chromium embedding path against the upstream Chrome/content-shell print plumbing
and identify whether Roamium is missing print manager, WebContents delegate,
print preview, or platform dialog integration.
