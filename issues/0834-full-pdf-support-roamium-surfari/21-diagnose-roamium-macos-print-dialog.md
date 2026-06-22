# Experiment 21: Diagnose Roamium macOS Print Dialog

## Description

Experiment 20 fixed Roamium's missing browser-side PDF print settings. The
native print path now reaches `PrintingContext::AskUserForSettings()`, but the
guarded probe still records `native-print-click-sent-no-dialog` and
`scripted-print-settings-null`.

The next unknown is inside the macOS dialog handoff. Chromium's
`PrintingContextMac::AskUserForSettings()` does not open the `NSPrintPanel`
inline. It stores the callback, sets a `CATransaction` completion block, and
opens `[panel runModalWithPrintInfo:]` from that completion block. This
experiment should prove which sub-hop fails for Roamium:

- the call into `AskUserForSettings()` never reaches the macOS implementation;
- the call is on the wrong thread or without a usable native parent;
- the `CATransaction` completion block never runs;
- `runModalWithPrintInfo:` runs but immediately returns cancel/failure;
- the native dialog appears but the existing watcher misses it;
- a different TermSurf integration issue prevents the dialog from being
  presented.

## Changes

1. Create a fresh Chromium branch for this issue experiment.

   ```bash
   cd chromium/src
   git checkout 148.0.7778.97-issue-834-exp20
   git checkout -b 148.0.7778.97-issue-834-exp21
   ```

   Update the branch table in `chromium/README.md`.

2. Add narrow, temporary trace points around the Roamium native-print dialog
   handoff.

   The trace should be enough to classify the exact failing sub-hop without
   requiring a debugger. It should include:

   - `TsPdfPrintManager::ScriptedPrint()` before calling `AskUserForSettings()`,
     including expected page count, selection state, scripted state, and whether
     the stored print request/context exists;
   - the `AskUserForSettings()` callback result before converting it to a null
     or non-null Mojo response;
   - macOS `PrintingContextMac::AskUserForSettings()` entry, including
     `NSThread.isMainThread`;
   - whether `delegate_->GetParentView()` returns a native view and whether the
     native view has a window;
   - immediately before and after installing the `CATransaction` completion
     block;
   - entry into the completion block;
   - immediately before and after `[panel runModalWithPrintInfo:]`, including
     the modal response code;
   - whether the callback reports `kSuccess`, `kCanceled`, or another result.

   Prefer writing to the existing
   `TERMSURF_PDF_NATIVE_PRINT_TRACE_FILE`/native-print trace mechanism so the
   existing probe summary can capture the new events. If the trace helper is not
   accessible from `printing/printing_context_mac.mm`, add a tiny file-append
   helper local to the experiment rather than broadening product APIs.

3. Extend the native-print probe classifier only as needed to report the new
   first failing sub-hop.

   Preserve all existing safety gates:

   - no native print click unless `--allow-native-dialog-click` is present;
   - preflight watcher must pass before production click;
   - queue state must be captured before and after;
   - any observed dialog must be cancelled;
   - an unexpected print job is a hard failure.

   New classifier names should distinguish at least:

   - macOS dialog callback returned cancel without an observed dialog;
   - macOS dialog completion block did not run;
   - macOS dialog ran but watcher missed it;
   - native parent/window was missing;
   - observed dialog was cancelled safely and the modal/callback returned
     cancel;
   - macOS dialog returned OK or callback `kSuccess`, which is a hard safety
     failure unless separate queue evidence proves no job was submitted and the
     result does not count it as cancellation.

4. Run the guarded native print probe and use the trace to decide whether this
   experiment can fix the issue or should stop at a proven diagnosis.

   If the trace identifies a narrow TermSurf integration bug, fix it in the same
   Chromium branch and rerun the guarded probe. Examples of acceptable narrow
   fixes include preserving a required native parent/window handle, deferring
   the call to the correct AppKit/UI sequence, or forcing the modal handoff
   through the same safe path Chromium expects.

   If the trace shows an upstream/platform behavior that needs a larger design,
   do not guess. Record Partial with the exact failing sub-hop and the source
   evidence for the next experiment.

## Verification

Verification for the completed result is:

```bash
git status --short
git -C chromium/src status --short
git -C chromium/src rev-parse --abbrev-ref HEAD
git -C chromium/src rev-parse HEAD
git diff --check

cd chromium/src
export PATH="/Users/astrohacker/dev/termsurf/chromium/depot_tools:$PATH"
autoninja -C out/Default libtermsurf_chromium

cd /Users/astrohacker/dev/termsurf
rm -rf scripts/__pycache__
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  scripts/test-issue-834-pdf-native-print.py
rm -rf scripts/__pycache__
node --check scripts/probe-pdf-save-print-title-local.mjs

rm -rf logs/issue-834-exp21-macos-print-dialog
python3 scripts/test-issue-834-pdf-native-print.py \
  --log-dir logs/issue-834-exp21-macos-print-dialog \
  --probe native-dialog \
  --allow-native-dialog-click

git diff --check
```

After committing Chromium branch changes inside `chromium/src`, regenerate the
cumulative Issue 834 patch archive from the local Chromium 148.0.7778.97 shallow
base:

```bash
cd /Users/astrohacker/dev/termsurf/chromium/src
rm -rf ../patches/issue-834
git format-patch 6b3fa66a923a9442c8ab0bc71b4b41ff24528d3b..HEAD \
  -o ../patches/issue-834
```

Required evidence:

- `chromium/README.md` records the new Chromium branch;
- Chromium source changes are committed inside `chromium/src`;
- `autoninja -C out/Default libtermsurf_chromium` passes;
- the Issue 834 patch archive is regenerated;
- the guarded native print probe records the new macOS dialog sub-hop trace;
- the result identifies whether the `CATransaction` completion block runs;
- the result identifies whether `[panel runModalWithPrintInfo:]` runs and what
  response it returns;
- no print job is submitted;
- if a native dialog appears, it is cancelled and queue state remains unchanged;
- if the watcher misses a dialog that trace proves appeared, the result records
  the watcher gap separately from product behavior;
- markdown is formatted with Prettier;
- Python bytecode cache is removed after compilation;
- `git diff --check` passes;
- design review is recorded, all real design-review findings are fixed, the
  design is approved, and the plan commit exists before implementation begins;
- completion review is recorded before the result commit.

## Pass Criteria

This experiment passes if Roamium native PDF print opens a native macOS print
dialog, the safety watcher cancels it, the print queue remains unchanged, and
the trace proves the path reached a successful dialog handoff.

## Partial Criteria

This experiment is partial if it does not make native print pass but replaces
`native-print-click-sent-no-dialog` with a more precise, source-backed failing
sub-hop such as missing native parent/window, completion block not running,
modal response cancel, or watcher miss.

## Failure Criteria

This experiment fails if it submits a print job, weakens the native print safety
gate, leaves the first failing hop no more precise than Experiment 20, leaves
Chromium branch/patch records inconsistent, or claims native print support
without native dialog cancellation and no-job evidence.

## Design Review

An adversarial Codex subagent reviewed the design with fresh context.

Initial verdict: **Changes Required**.

Required finding:

- The classifier bucket `macOS dialog returned success and was cancelled safely`
  was unsafe and contradictory. Chromium maps `NSModalResponseOK` from
  `[panel runModalWithPrintInfo:]` to `mojom::ResultCode::kSuccess`, while
  cancellation maps to `kCanceled`, so treating success as compatible with safe
  cancellation could misclassify a Print-button path as safe progress.

Fix:

- Replaced that bucket with separate states for observed-dialog cancellation
  versus modal OK / callback `kSuccess`. The design now treats OK/success as a
  hard safety failure unless queue evidence proves no job was submitted, and it
  must not count success as cancellation.

Re-review verdict: **Approved**.

The reviewer confirmed the prior Required finding is resolved and found no new
Required findings.

## Result

**Result:** Partial

Implemented the diagnostic path planned by this experiment:

- added browser-side Roamium print-manager trace events in
  `TsPdfPrintManager::ScriptedPrint()`;
- added macOS `PrintingContextMac::AskUserForSettings()` trace events around
  parent view/window discovery, `CATransaction` completion-block installation,
  completion-block entry, and `[panel runModalWithPrintInfo:]`;
- extended the native-print harness to merge browser-process `pdf-native-print`
  stderr lines into `probe_summary.print.printNativeLines`;
- extended the native-print classifier with macOS-specific sub-hop names;
- extended the CoreGraphics watcher to retain a small candidate-window snapshot
  when no `Print`/`Printer` dialog is observed.

The first probe run exposed a bug in the initial trace implementation:
browser-side `base::AppendToFile()` tripped Chromium's blocking-call DCHECK
during the synchronous print Mojo call. The trace implementation was changed to
use Chromium logging for browser/macOS events, and the Python harness now folds
those stderr trace lines into the existing native-print event list.

The final guarded probe completed with:

```text
first_failing_hop = "mac-print-modal-response-missing"
safety_gate_passed = true
probe_status = "ok"
print_status = "print-native-click-sent"
dialog_observed = false
cancel_sent = false
print_queue_before = ""
print_queue_after = ""
```

The trace proves:

```text
ts-scripted-print-enter
ts-scripted-print-call-ask-user-for-settings
mac-ask-user-enter main_thread=true
mac-ask-user-parent-view-present
mac-ask-user-parent-window-present
mac-ask-user-install-completion-block-enter
mac-ask-user-install-completion-block-exit
mac-ask-user-completion-block-enter
mac-ask-user-run-modal-enter
```

No `mac-ask-user-modal-response-ok`, `mac-ask-user-modal-response-cancel`, or
callback result event was recorded before the harness terminated Roamium. The
CoreGraphics watcher did not observe a `Print` or `Printer` window. Its
candidate-window snapshot repeatedly saw the existing Roamium `Content Shell`
window but no separate visible print panel candidate.

Verification evidence:

- `autoninja -C out/Default libtermsurf_chromium` passed on
  `148.0.7778.97-issue-834-exp21`.
- Chromium source commit: `94e579f240d99bad9de011fc8f652939b997dc69`.
- `PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile scripts/test-issue-834-pdf-native-print.py`
  passed and `scripts/__pycache__` was removed.
- `node --check scripts/probe-pdf-save-print-title-local.mjs` passed.
- `git diff --check` and `git -C chromium/src diff --check` passed.
- `scripts/test-issue-834-pdf-native-print.py --log-dir logs/issue-834-exp21-macos-print-dialog --probe native-dialog --allow-native-dialog-click`
  completed with `probe_status = "ok"`,
  `first_failing_hop = "mac-print-modal-response-missing"`, and unchanged print
  queues.
- The cumulative Issue 834 patch archive was regenerated from
  `6b3fa66a923a9442c8ab0bc71b4b41ff24528d3b` and now includes
  `0078-Trace-macOS-PDF-print-modal.patch`.

## Conclusion

Experiment 21 replaced the broad `native-print-click-sent-no-dialog` failure
with a precise macOS sub-hop. Roamium reaches the macOS print implementation on
the main thread with a native parent window, installs and enters Chromium's
`CATransaction` completion block, and calls `[panel runModalWithPrintInfo:]`.
The path then blocks without an observed/cancellable native print panel and
without a modal response.

The next experiment should focus on why `NSPrintPanel runModalWithPrintInfo:`
does not produce a visible window or response in Roamium's content-shell style
embedding. Likely directions are AppKit application activation/presentation
state, modal-session behavior in this embedding, and whether the print panel
needs to be presented as a sheet or with an explicit app/window activation step
instead of Chromium's stock modal path.

## Completion Review

An adversarial Codex subagent reviewed the completed result with fresh context.

Verdict: **Approved**.

The reviewer found no Required findings.
