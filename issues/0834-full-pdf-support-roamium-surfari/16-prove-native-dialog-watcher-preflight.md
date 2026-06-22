# Experiment 16: Prove Native Dialog Watcher Preflight

## Description

Experiment 10 left Roamium native PDF print support Partial because the harness
could not prove a safe cancel-only macOS dialog watcher. The failure was not yet
product behavior: the harmless preflight dialog opened, but System Events
reported that `osascript` was not allowed assistive access, so the harness could
not objectively observe and dismiss even a harmless non-print dialog.

This experiment should solve only the safety preflight problem. It must not
click the production PDF print control. The output should be a reusable
preflight mechanism that proves, on this VM, that automation can:

- open a harmless native dialog;
- observe it through an objective OS-level or screen-level mechanism;
- cancel/dismiss it;
- verify that it disappeared;
- record enough evidence to decide whether the production print experiment can
  safely proceed next.

## Changes

1. Add or extend a native-dialog preflight harness.

   Update `scripts/test-issue-834-pdf-native-print.py` or add a small helper
   script if that keeps the logic cleaner. The harness should support a
   preflight-only probe that does not launch Roamium and does not click PDF
   print.

   Suggested command:

   ```bash
   python3 scripts/test-issue-834-pdf-native-print.py \
     --log-dir logs/issue-834-exp16-native-dialog-preflight \
     --probe watcher-preflight
   ```

   If it is cleaner to add a dedicated script, use a name like
   `scripts/test-issue-834-native-dialog-preflight.py` and keep Experiment 10's
   native print harness unchanged except for reusing the proven mechanism later.

2. Test multiple harmless watcher mechanisms.

   At minimum, evaluate the existing System Events watcher and one additional
   mechanism that may work in a macOS VM without relying on the same failing
   permission path. Candidate mechanisms include:

   - System Events / AppleScript window enumeration and Escape, after the user
     has granted assistive access;
   - direct AppleScript dialog-process control that does not require enumerating
     every process, if possible;
   - screenshot-based observation plus a safe keyboard Escape event;
   - CoreGraphics or Accessibility API observation through a small Swift helper;
   - another local macOS mechanism that can objectively observe and cancel the
     harmless dialog.

   The true working mechanism may be something else. Record each attempted
   mechanism, including failures and permission errors.

3. Keep the dialog harmless.

   The preflight dialog must not be a print dialog. Use a harmless dialog with a
   unique title such as `TermSurf Native Print Safety Preflight`. The dialog may
   be created with `osascript display dialog`, a tiny Swift/AppKit helper, or
   another local mechanism.

   The harness must clean up the dialog on failure. If it cannot prove cleanup,
   it must record a failure and leave production print disabled for the next
   experiment.

4. Produce a machine-readable summary.

   Write `<log-dir>/native-dialog-preflight-summary.json` with at least:

   - `first_failing_hop`;
   - `overall_result`;
   - `mechanisms`;
   - for each mechanism: `name`, `observed`, `cancel_sent`, `disappeared`,
     `returncode`, stdout/stderr paths or snippets, and permission diagnostics;
   - `selected_mechanism`, if one passes;
   - `safe_for_production_print_probe`, boolean;
   - confirmation that no production print click was attempted.

   Classifications:

   - `no-failure-observed`;
   - `dialog-open-failed`;
   - `dialog-observation-failed`;
   - `dialog-cancel-failed`;
   - `dialog-disappearance-not-proven`;
   - `permission-denied`;
   - `automation-gap`.

5. Update the native print path only if the preflight passes.

   If a mechanism passes, update Experiment 10's native print harness so the
   existing `native-dialog` probe can use that same mechanism in a future
   experiment. Do not click production print in this experiment.

   If no mechanism passes, record Partial with the exact permission or
   automation blocker and do not change production print behavior.

## Verification

Verification for the completed result is:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  scripts/test-issue-834-pdf-native-print.py

python3 scripts/test-issue-834-pdf-native-print.py \
  --log-dir logs/issue-834-exp16-native-dialog-preflight \
  --probe watcher-preflight

git diff --check
```

If a new helper script is added, compile/check it as appropriate. If a Swift
helper is added, build it with the repo's normal local tooling or a direct
`swiftc` command recorded in the result.

Required evidence:

- no production PDF print control is clicked;
- the harmless dialog title is unique and recorded;
- at least two watcher mechanisms are attempted or a clear reason is recorded
  for why only one is available;
- each mechanism records observation, cancellation, disappearance, and
  permission diagnostics;
- if a mechanism passes, the result records
  `safe_for_production_print_probe = true` and names `selected_mechanism`;
- if no mechanism passes, the result records the first failing layer and the
  next external permission or implementation step;
- no print queue job is submitted;
- Python bytecode cache is removed after compilation;
- markdown is formatted with Prettier;
- `git diff --check` passes;
- design review is recorded, all real design-review findings are fixed, the
  design is approved, and the plan commit exists before implementation begins;
- completion review is recorded before the result commit.

## Pass Criteria

This experiment passes if a harmless native-dialog watcher mechanism objectively
observes, cancels, and verifies disappearance of the dialog, records that no
production print click occurred, and marks the next native PDF print experiment
safe to attempt the production print click behind that mechanism.

## Partial Criteria

This experiment is partial if it improves diagnostics or tests additional
mechanisms but still cannot prove a safe watcher on this VM.

## Failure Criteria

This experiment fails if it clicks production print, leaves a harmless dialog
open without recording cleanup failure, claims watcher readiness without
observation/cancel/disappearance evidence, or omits permission diagnostics for
failed mechanisms.

## Design Review

An adversarial Codex subagent reviewed the design with fresh context.

Verdict: **Approved**.

The reviewer found no required issues.
