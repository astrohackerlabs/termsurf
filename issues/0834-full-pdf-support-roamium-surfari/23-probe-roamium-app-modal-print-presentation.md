# Experiment 23: Probe Roamium App-Modal Print Presentation

## Description

Experiment 22 proved that Roamium can reach macOS `NSPrintPanel` presentation
from `PrintingContextMac`, but neither presentation path is currently usable:

- `runModalWithPrintInfo:` is entered and never returns;
- `beginSheetUsingPrintInfo:onWindow:completionHandler:` is entered and returns
  from the begin call, but its completion handler never fires;
- forcing `NSApplicationActivationPolicyRegular`, ordering the parent window
  front, and calling `activateIgnoringOtherApps:` still leaves `NSApp.active`
  false and the parent window non-key/non-main;
- no observable print-panel candidate appears;
- the print queues remain unchanged.

The nearby content-shell macOS code provides an important clue:
`ShellJavaScriptDialog` uses `beginSheetModalForWindow:nil`, with a code comment
that `nil` makes the dialog app-modal. TermSurf already replaces content shell's
platform delegate with `TsShellPlatformDelegate` for JavaScript dialogs and file
choosers, but PDF print still goes through generic `PrintingContextMac` without
a TermSurf-specific presentation hook.

This experiment should determine whether Roamium's native print panel is stuck
because the parent-window sheet path is wrong for the content-shell/TermSurf
embedding, and whether app-modal print-panel presentation behaves like the known
content-shell JavaScript dialog path.

## Changes

1. Create a fresh Chromium branch for this issue experiment.

   ```bash
   cd chromium/src
   git checkout 148.0.7778.97-issue-834-exp22
   git checkout -b 148.0.7778.97-issue-834-exp23
   ```

   Update the branch table in `chromium/README.md`.

2. Add trace points that compare the app/window state used by content-shell
   dialogs with the state seen by print.

   Relevant files to inspect and, if needed, instrument:

   - `content/shell/browser/shell_javascript_dialog_mac.mm`
   - `content/shell/browser/shell_platform_delegate_mac.mm`
   - `content/libtermsurf_chromium/ts_javascript_dialog_manager.cc`
   - `printing/printing_context_mac.mm`

   The trace should answer:

   - whether content-shell JavaScript dialogs can complete in this Roamium
     process when shown app-modally;
   - whether those dialogs require `NSApp.active == true`;
   - whether print-panel presentation differs only by parent-window sheet vs
     app-modal sheet/modal behavior;
   - whether TermSurf's hidden/embedded content-shell window has an AppKit state
     that makes parent-window sheets impossible.

3. Try one narrow print-panel presentation change after tracing.

   Preferred order:

   1. Try presenting `NSPrintPanel` app-modally, analogous to
      `ShellJavaScriptDialog`'s `beginSheetModalForWindow:nil`, while preserving
      the async `AskUserForSettings` callback contract.
   2. If AppKit does not expose an app-modal sheet API for `NSPrintPanel`, use
      the closest available documented print-panel API and record why it is the
      closest analogue.
   3. Keep the Experiment 22 parent-window sheet and modal traces available only
      as fallback/diagnostics. Do not pile unrelated launch or bundle changes
      into this experiment unless trace evidence proves they are required before
      print-panel presentation can be tested.

4. Preserve the native-print safety contract.

   The harness must still:

   - require `--allow-native-dialog-click` for the production print click;
   - run the harmless preflight before clicking the production PDF print
     control;
   - record print queues before and after;
   - classify any queue delta as `native-print-job-submitted-unexpectedly`;
   - cancel any observed native dialog;
   - treat OK / printed / `kSuccess` as unsafe unless queue evidence and watcher
     evidence prove no job was submitted;
   - require observed cancellation and unchanged queues for a pass.

5. Update the native-print harness only if new trace events need a precise
   first-failing-hop classification.

   Expected possible new classifications:

   - `mac-print-app-modal-response-missing`;
   - `mac-print-app-modal-response-cancel-no-observed-dialog`;
   - `native-print-dialog-seen-cancelled`.

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

rm -rf logs/issue-834-exp23-app-modal-print-presentation
python3 scripts/test-issue-834-pdf-native-print.py \
  --log-dir logs/issue-834-exp23-app-modal-print-presentation \
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
- the guarded native print probe records the attempted presentation path;
- no print job is submitted;
- if a native dialog appears, it is cancelled and queue state remains unchanged;
- if the dialog still does not appear, the result identifies the next precise
  failing sub-hop and whether app-modal presentation changed behavior compared
  with Experiment 22;
- markdown is formatted with Prettier;
- Python bytecode cache is removed after compilation;
- `git diff --check` passes;
- design review is recorded, all real design-review findings are fixed, the
  design is approved, and the plan commit exists before implementation begins;
- completion review is recorded before the result commit.

## Pass Criteria

This experiment passes if Roamium native PDF print opens a native macOS print
panel, the safety watcher cancels it, the callback path reports cancellation
rather than OK / printed / success, and print queue evidence proves no job was
submitted.

## Partial Criteria

This experiment is partial if native print still does not pass but the result
proves whether app-modal print-panel presentation changes the Experiment 22
failure mode, or proves that the remaining blocker is earlier app/process/window
activation state rather than parent-window sheet presentation.

## Failure Criteria

This experiment fails if it submits a print job, weakens the native print safety
gate, treats OK / printed / `kSuccess` as safe cancellation, leaves Chromium
branch/patch records inconsistent, or makes broad launch/bundle/AppKit changes
without trace evidence that they target the current presentation failure.

## Design Review

An adversarial Codex subagent reviewed the design with fresh context.

Verdict: **Approved**.

The reviewer found no Required findings. It confirmed that Experiment 23 is
linked as `Designed`, has Description / Changes / Verification plus Pass /
Partial / Failure criteria, follows directly from Experiment 22's
`mac-print-sheet-response-missing` result, preserves the native-print safety
contract, includes branch / patch / build / harness / probe / diff / review /
commit gates, and requires the plan commit before implementation begins.

## Result

**Result:** Partial

Chromium branch `148.0.7778.97-issue-834-exp23` was created from
`148.0.7778.97-issue-834-exp22`, and `chromium/README.md` was updated with the
new branch. Chromium commit `890c437d5fc8a9773ed9e4de2d725b99a0aa7bb6` records
the fork change, and the cumulative patch archive was regenerated through
`chromium/patches/issue-834/0080-Probe-app-modal-PDF-print-panels.patch`.

The implementation tried two app-modal print-panel variants during iteration:

1. `beginSheetUsingPrintInfo:onWindow:completionHandler:` with a nil parent
   window variable.
2. The older async
   `beginSheetWithPrintInfo:modalForWindow:delegate:didEndSelector:contextInfo:`
   path with a nil parent window and a retained delegate helper.

The first variant was an intermediate, unarchived probe. It built only after
avoiding a literal `nil` argument, but appeared to fail at runtime by returning
from the begin call without retaining/running the completion block, which
destroyed Chromium's pending `ScriptedPrintCallback`. That intermediate log
directory was overwritten by the final delegate-based verification run, so this
is recorded only as development context, not as primary experiment evidence. The
harness keeps a classifier for that possible failure:

`mac-print-app-modal-callback-dropped-crash`.

The verified second variant avoided callback destruction by retaining a
`TermSurfPrintPanelDelegate` helper through `contextInfo`. It built and ran, but
the guarded native-print probe still did not observe a native print panel and
the delegate callback never fired.

Verification run:

```bash
cd chromium/src
/Users/astrohacker/dev/termsurf/chromium/depot_tools/autoninja \
  -C out/Default libtermsurf_chromium

cd /Users/astrohacker/dev/termsurf
rm -rf scripts/__pycache__
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  scripts/test-issue-834-pdf-native-print.py
rm -rf scripts/__pycache__
node --check scripts/probe-pdf-save-print-title-local.mjs
git diff --check

rm -rf logs/issue-834-exp23-app-modal-print-presentation scripts/__pycache__
python3 scripts/test-issue-834-pdf-native-print.py \
  --log-dir logs/issue-834-exp23-app-modal-print-presentation \
  --probe native-dialog \
  --allow-native-dialog-click
```

Evidence from
`logs/issue-834-exp23-app-modal-print-presentation/pdf-native-print-summary.json`:

- `first_failing_hop`: `mac-print-app-modal-response-missing`;
- `safety_gate_passed`: `true`;
- `probe_status`: `ok`;
- `server_register_received`: `true`;
- `tab_ready_id`: `1`;
- `devtools_port`: `50853`;
- `roamium_exited_before_shutdown`: `false`;
- `roamium_exit_code_before_shutdown`: `null`;
- `print_dialog_watch.dialog_observed`: `false`;
- `print_dialog_watch.cancel_sent`: `false`;
- `print_queue_before.lpstat_o.stdout`: empty;
- `print_queue_after.lpstat_o.stdout`: empty;
- `print_queue_before.lpstat_W_completed_o.stdout`: empty;
- `print_queue_after.lpstat_W_completed_o.stdout`: empty.

The native trace reached:

```text
mac-ask-user-before-activation-app activation_policy=prohibited active=false
mac-ask-user-before-activation-window present=true key=false main=false visible=true miniaturized=false can_key=true can_main=true matches_key=false matches_main=false
mac-ask-user-set-activation-policy-regular-enter
mac-ask-user-set-activation-policy-regular-exit
mac-ask-user-after-activation-app activation_policy=regular active=false
mac-ask-user-after-activation-window present=true key=false main=false visible=true miniaturized=false can_key=true can_main=true matches_key=false matches_main=false
mac-ask-user-begin-app-modal-sheet-enter
mac-ask-user-begin-app-modal-sheet-exit
```

No `mac-ask-user-app-modal-sheet-response-cancel`,
`mac-ask-user-app-modal-sheet-response-printed`, or `mac-ask-user-callback-*`
event was observed. No print job was submitted.

## Conclusion

App-modal print-panel presentation did not solve native PDF print. It did
distinguish two important behaviors:

- the modern block-based sheet API requires a real parent window for safe async
  callback retention at compile time; the unarchived nil-parent runtime probe
  was not strong enough to keep as primary evidence;
- the deprecated delegate-based async API can retain the Chromium callback
  safely with a helper object, but app-modal nil-window presentation still
  produces no observable print panel and no completion callback.

This makes the parent-window sheet vs app-modal distinction less likely to be
the root cause. The remaining evidence still points at Roamium's process/window
activation state: even after switching to regular activation policy and ordering
the content-shell window front, `NSApp.active` stays false and the parent window
stays non-key/non-main. The next experiment should focus earlier in the macOS
application lifecycle and content-shell platform delegate, not on more
`NSPrintPanel` presentation variants.

## Completion Review

An adversarial Codex subagent reviewed the completed experiment with fresh
context.

Initial verdict: **Changes Required**.

Required finding:

- The result recorded the intermediate block-based nil-window
  `beginSheetUsingPrintInfo:onWindow:completionHandler:` crash as though it were
  primary experiment evidence, but the final log directory only contains the
  delegate-based app-modal run.

Fix:

- Revised the result to mark the block-based nil-window crash as unarchived
  development context, not primary experiment evidence.
- Kept the verified Experiment 23 conclusion anchored to the archived
  delegate-based run, which records `mac-print-app-modal-response-missing`, no
  observed dialog, no cancel sent, unchanged queues, and no pre-shutdown Roamium
  exit.

Re-review verdict: **Approved**.

The reviewer confirmed that the prior Required finding is resolved and found no
new Required issue.
