# Experiment 29: Add Native Print to Roamium Regression Guards

## Description

Experiment 28 proved that Roamium native PDF print cancellation is now
automatable on this macOS VM:

- the native print safety preflight passes;
- the production native print control can be clicked by the guarded harness;
- document-modal print sheets are detected by live AppKit trace evidence;
- the watcher presses the sheet's Cancel button through PID-targeted
  Accessibility traversal constrained to a sheet/dialog-like AX subtree;
- Chromium reports the cancel callback;
- no print job is submitted;
- Roamium remains alive.

The durable Roamium PDF regression runner still reflects the old state from
Experiment 15: native print is listed only under `unsafe-manual` as skipped
because a safe native-dialog watcher was not yet proven. This experiment should
update the regression guard so native print is covered by an explicit
safety-gated tier without making it part of normal fast/focused runs.

The goal is durability, not new product behavior. The experiment should preserve
the existing smoke/focused/form tiers and add a clear opt-in tier for native
print cancellation.

## Changes

- Update `scripts/test-issue-834-roamium-pdf-regression.py`.
- Add an explicit native print regression tier, tentatively named
  `native-print`.
- The `native-print` tier should run:

  ```bash
  python3 scripts/test-issue-834-pdf-native-print.py \
    --log-dir <child-log-dir> \
    --probe native-dialog \
    --allow-native-dialog-click
  ```

- Classify the native print check as passing only when the child summary reports
  all of:
  - `first_failing_hop = "native-print-dialog-seen-cancelled"`;
  - `safety_gate_passed = true`;
  - `roamium_exited_before_shutdown = false`;
  - unchanged print queue before/after;
  - `print_dialog_watch.cancel_sent = true`;
  - `print_dialog_watch.sheet_evidence.observed = true`;
  - `print_dialog_watch.sheet_cancel.requireSheet = true`;
  - native trace includes `ts-scripted-print-callback-result-canceled`.
- Keep `smoke`, `focused`, and `forms` unchanged so routine runs do not open
  native OS UI.
- Update `unsafe-manual` so it no longer claims native print is unproven. It
  should either:
  - point users to the explicit `native-print` tier, or
  - list native print as skipped from `unsafe-manual` because it has its own
    guarded tier.
- Extend the runner summary if needed so future automation can tell that native
  print used the explicit safety gate.
- Do not modify Chromium, Roamium, Ghostboard, Surfari, or protocol code.

## Verification

Run hygiene checks:

```bash
rm -rf scripts/__pycache__
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  scripts/test-issue-834-roamium-pdf-regression.py \
  scripts/test-issue-834-pdf-native-print.py
rm -rf scripts/__pycache__
git diff --check
git -C chromium/src diff --check
```

Run the explicit native print tier:

```bash
rm -rf logs/issue-834-exp29-native-print-regression
python3 scripts/test-issue-834-roamium-pdf-regression.py \
  --log-dir logs/issue-834-exp29-native-print-regression \
  --tier native-print
```

Run the dry unsafe tier to prove native print does not run there:

```bash
rm -rf logs/issue-834-exp29-unsafe-manual
python3 scripts/test-issue-834-roamium-pdf-regression.py \
  --log-dir logs/issue-834-exp29-unsafe-manual \
  --tier unsafe-manual
```

Run at least one cheap existing tier to prove it is not disrupted:

```bash
rm -rf logs/issue-834-exp29-smoke
python3 scripts/test-issue-834-roamium-pdf-regression.py \
  --log-dir logs/issue-834-exp29-smoke \
  --tier smoke
```

Pass criteria:

- `native-print` exits 0 and records `overall_result = "pass"`;
- the native print child summary proves safe cancellation, unchanged print
  queue, Roamium liveness, sheet evidence, `requireSheet=true`, and Chromium's
  canceled callback trace;
- `unsafe-manual` exits 0 without running a production native print click;
- `smoke` still exits 0;
- generated summaries are current-run summaries, not stale reused files;
- README status and this experiment result accurately describe the tiering.

Partial criteria:

- the runner is updated, but the explicit native print tier exposes a new
  concrete failing hop while still preserving print safety and Roamium liveness.

Failure criteria:

- a print job is submitted;
- native print is added to `smoke` or `focused` by default;
- `unsafe-manual` clicks the production native print control;
- the runner reports success without proving the safety-gate fields listed
  above;
- stale child summaries can make the native print tier pass;
- unrelated product code is changed.

## Design Review

An adversarial Codex subagent reviewed the design with fresh context.

Verdict: **Approved**.

The reviewer found no findings. It confirmed that the design is linked from the
issue README as Experiment 29 with status `Designed`, has the required
Description, Changes, and Verification sections, follows directly from
Experiment 28, keeps native OS UI behind an explicit `native-print` tier,
preserves `smoke` / `focused` / `forms`, includes stale-summary and cheap-tier
checks, and has no implementation changes beyond the README link and new
experiment file.

## Result

**Result:** Pass

Added an explicit `native-print` tier to
`scripts/test-issue-834-roamium-pdf-regression.py`. The tier runs the guarded
native print harness with `--probe native-dialog --allow-native-dialog-click`
and classifies it with a dedicated native-print proof checker.

The native-print check passes only when the child summary proves:

- `first_failing_hop = "native-print-dialog-seen-cancelled"`;
- `safety_gate_passed = true`;
- `roamium_exited_before_shutdown = false`;
- print queue state is unchanged before/after;
- `print_dialog_watch.cancel_sent = true`;
- `print_dialog_watch.sheet_evidence.observed = true`;
- `print_dialog_watch.sheet_cancel.requireSheet = true`;
- native trace contains `ts-scripted-print-callback-result-canceled`.

The existing `smoke`, `focused`, and `forms` tiers were left unchanged, so
native OS UI remains out of routine fast/focused runs. The `unsafe-manual` tier
remains dry/list-only and now points to the explicit `native-print` tier instead
of claiming native print is still unproven.

Verification run:

```bash
rm -rf scripts/__pycache__
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  scripts/test-issue-834-roamium-pdf-regression.py \
  scripts/test-issue-834-pdf-native-print.py
rm -rf scripts/__pycache__
git diff --check
git -C chromium/src diff --check

rm -rf logs/issue-834-exp29-native-print-regression
python3 scripts/test-issue-834-roamium-pdf-regression.py \
  --log-dir logs/issue-834-exp29-native-print-regression \
  --tier native-print

rm -rf logs/issue-834-exp29-unsafe-manual
python3 scripts/test-issue-834-roamium-pdf-regression.py \
  --log-dir logs/issue-834-exp29-unsafe-manual \
  --tier unsafe-manual

rm -rf logs/issue-834-exp29-smoke
python3 scripts/test-issue-834-roamium-pdf-regression.py \
  --log-dir logs/issue-834-exp29-smoke \
  --tier smoke
```

Final evidence:

- `logs/issue-834-exp29-native-print-regression/roamium-pdf-regression-summary.json`
  recorded `overall_result = "pass"`,
  `first_failing_hop = "no-failure-observed"`, duration `92.173` seconds, and 1
  passing native print check.
- The native print child summary recorded
  `first_failing_hop = "native-print-dialog-seen-cancelled"`,
  `safety_gate_passed = true`, `roamium_exited_before_shutdown = false`,
  unchanged print queue state, `print_dialog_watch.cancel_sent = true`,
  `print_dialog_watch.sheet_evidence.observed = true`,
  `print_dialog_watch.sheet_cancel.requireSheet = true`, and one
  `ts-scripted-print-callback-result-canceled` native trace line.
- `logs/issue-834-exp29-unsafe-manual/roamium-pdf-regression-summary.json`
  recorded `overall_result = "skipped-unsafe"`, ran no checks, and listed native
  print as skipped from `unsafe-manual` because it has the replacement
  `native-print` tier.
- `logs/issue-834-exp29-smoke/roamium-pdf-regression-summary.json` recorded
  `overall_result = "pass"`, `first_failing_hop = "no-failure-observed"`,
  duration `31.915` seconds, and the same 2 passing smoke checks as before.

## Conclusion

Roamium native PDF print cancellation is now protected by a durable, explicit
regression tier. It is intentionally not part of the cheap `smoke` or broad
`focused` tiers because it opens native OS UI, but the `native-print` tier can
be run when validating PDF/native-print work.

The next Issue 834 experiment should continue the remaining Roamium PDF matrix
items, with native print no longer the blocking gap.

## Completion Review

An adversarial Codex subagent reviewed the completed experiment with fresh
context.

Initial verdict: **Changes required**.

Required finding:

- `scripts/test-issue-834-roamium-pdf-regression.py` allowed native print to
  pass if both `print_queue_before` and `print_queue_after` were missing,
  because the queue comparison defaulted absent fields to empty dictionaries.

Fix:

- `print_queue_unchanged()` now requires both queue fields to be present
  dictionaries, to have the same command keys, and to contain successful,
  non-timed-out command results before comparing the queue snapshots.

Additional verification after the fix:

```bash
python3 - <<'PY'
import importlib.util, sys

spec = importlib.util.spec_from_file_location(
    "reg", "scripts/test-issue-834-roamium-pdf-regression.py"
)
reg = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = reg
spec.loader.exec_module(reg)

base = {
    "first_failing_hop": "native-print-dialog-seen-cancelled",
    "safety_gate_passed": True,
    "roamium_exited_before_shutdown": False,
    "print_dialog_watch": {
        "cancel_sent": True,
        "sheet_evidence": {"observed": True},
        "sheet_cancel": {"requireSheet": True},
    },
    "probe_summary": {
        "print": {
            "printNativeLines": [
                "ts-scripted-print-callback-result-canceled"
            ]
        }
    },
}

print("missing queue:", reg.classify_native_print(base))
queue = {
    "lpstat_o": {
        "returncode": 0,
        "timed_out": False,
        "stdout": "",
        "stderr": "",
        "cmd": ["lpstat", "-o"],
    }
}
base["print_queue_before"] = queue
base["print_queue_after"] = queue
print("present queue:", reg.classify_native_print(base))
PY
```

The probe recorded:

- missing queue:
  `('fail', 'native-print-safety-proof-missing:queue_unchanged', None)`;
- present successful queue:
  `('pass', 'native-print-dialog-seen-cancelled', None)`.

The full `native-print` tier was rerun after the fix and still passed.

Final verdict after re-review: **Approved**.

The re-review found no findings.
