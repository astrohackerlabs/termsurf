# Experiment 25: Promote Roamium AppKit Activation Policy

## Description

Experiment 24 proved that the standalone native-print harness was missing a
representative `SetGuiActive(active=true)` message, and that the message now
reaches both Roamium and Chromium. Chromium also called the macOS content-shell
activation path from `SetGuiActive`, but AppKit state did not improve:

- before shell activation, `NSApp.activationPolicy` was `prohibited`;
- after `Shell::ActivateContents()`, `NSApp.activationPolicy` was still
  `prohibited`;
- `NSApp.active`, `NSApp.keyWindow`, and `NSApp.mainWindow` remained false/null;
- the print path later temporarily changed the policy to `regular`, but
  `activateIgnoringOtherApps:` and `makeKeyAndOrderFront:` still did not make
  the app active or the window key/main;
- native print still stalled at `mac-print-app-modal-response-missing`.

The next narrow hypothesis is that Roamium/content-shell is being initialized as
an AppKit app with `NSApplicationActivationPolicyProhibited`, and attempting to
promote it only inside the print completion block is too late or insufficient.
This experiment should explicitly promote the browser process to a regular
AppKit application before window activation and before native print, then trace
whether AppKit accepts that promotion.

This is still a probe, not a broad product decision. If `regular` causes dock or
menu behavior that is unsuitable for the hidden browser process, the experiment
should record that and the next experiment can test `accessory` or a more
targeted policy. The first requirement is evidence: prove whether activation
policy is the reason the native print panel cannot appear.

## Changes

1. Create a fresh Chromium branch for this issue experiment.

   ```bash
   cd chromium/src
   git checkout 148.0.7778.97-issue-834-exp24
   git checkout -b 148.0.7778.97-issue-834-exp25
   ```

   Update the branch table in `chromium/README.md`.

2. Add a macOS helper that explicitly prepares Roamium for AppKit activation.

   In `content/libtermsurf_chromium/ts_shell_window_mac.h` /
   `ts_shell_window_mac.mm`, add a small helper with native-print trace support:

   - records the current `NSApp.activationPolicy`;
   - if the policy is `prohibited`, sets it to
     `NSApplicationActivationPolicyRegular`;
   - records the boolean return value from `setActivationPolicy:`;
   - records whether the policy changed;
   - keeps this behavior macOS-only and scoped to Roamium/content-shell window
     activation.

   Prefer calling this helper immediately before shell window activation rather
   than inside `printing_context_mac.mm`, so the browser process is in a valid
   activation state before native print begins.

3. Call the helper from the `SetGuiActive(active=true)` path before
   `ActivateShellWindowForTermSurf(tab->shell)`.

   Keep `active=false` behavior unchanged. Do not attempt global macOS
   deactivation in this experiment.

4. Extend Chromium native-print trace output so the probe can distinguish:

   - policy before promotion;
   - policy after promotion;
   - shell activation before/after state;
   - print-path before/after activation state.

5. Reuse the existing Experiment 24 harness behavior.

   Do not weaken the safety gate, do not change the watcher, and do not treat
   `OK`, `printed`, or `kSuccess` as safe.

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

rm -rf logs/issue-834-exp25-appkit-activation-policy
python3 scripts/test-issue-834-pdf-native-print.py \
  --log-dir logs/issue-834-exp25-appkit-activation-policy \
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
- the Issue 834 patch archive is regenerated and includes the Experiment 25
  Chromium commit;
- `autoninja -C out/Default libtermsurf_chromium` passes;
- the harness still records `gui_active_sent=true`;
- logs prove whether AppKit policy promotion ran;
- logs record the `setActivationPolicy:` return value when promotion is
  attempted;
- logs prove whether `NSApp.activationPolicy` remains prohibited or becomes
  regular/accessory before shell activation and before native print;
- logs prove whether `NSApp.active`, `NSApp.keyWindow`, and `NSApp.mainWindow`
  change after policy promotion;
- no print job is submitted;
- if a native dialog appears, it is cancelled and queue state remains unchanged;
- markdown is formatted with Prettier;
- Python bytecode cache is removed after compilation;
- `git diff --check` passes;
- design review is recorded, all real design-review findings are fixed, the
  design is approved, and the plan commit exists before implementation begins;
- completion review is recorded before the result commit.

## Pass Criteria

This experiment passes if policy promotion lets Roamium native PDF print open a
native macOS print panel, the safety watcher cancels it, the callback path
reports cancellation rather than OK / printed / success, and print queue
evidence proves no job was submitted.

## Partial Criteria

This experiment is partial if native print still does not pass but the result
proves one of these narrower facts:

- `NSApp.activationPolicy` can be changed from prohibited before shell
  activation, but native print still stalls;
- policy promotion makes `NSApp.active`, `NSApp.keyWindow`, or
  `NSApp.mainWindow` change without completing safe native print cancellation;
- policy promotion is rejected or immediately reverted by AppKit/content-shell;
- the print path is not seeing the promoted policy even though `SetGuiActive`
  promoted it earlier.

## Failure Criteria

This experiment fails if it submits a print job, weakens the native print safety
gate, treats OK / printed / `kSuccess` as safe cancellation, changes unrelated
GUI/frontend code, leaves Chromium branch/patch records inconsistent, or makes
broad app lifecycle changes that are not traceable to the AppKit activation
policy hypothesis.

## Design Review

An adversarial Codex subagent reviewed the design with fresh context.

Verdict: **Approved**.

The reviewer found no Required findings. It confirmed that the README links
Experiment 25 as `Designed`, the design has Description, Changes, Verification,
and Pass / Partial / Failure criteria, the experiment follows directly from
Experiment 24, the native-print safety behavior remains intact, and no
implementation had started before design review.

The reviewer suggested one optional improvement: require the helper to log the
boolean return value from `setActivationPolicy:` so the result can distinguish
setter rejection from immediate reversion. That suggestion was accepted and
added to the Changes and Verification requirements.

## Result

**Result:** Partial

The AppKit activation policy hypothesis was real, but not sufficient to pass
native print.

The Chromium `SetGuiActive(active=true)` path now promotes Roamium from
`NSApplicationActivationPolicyProhibited` to
`NSApplicationActivationPolicyRegular` before calling the content-shell window
activation path. The probe records:

- `set-gui-active-policy-promotion policy_before=2 attempted=1 setter_result=1 policy_after=0 changed=1`;
- after shell activation, `activation_policy=0` but `app_active=0`, with no key
  or main window yet;
- by the print path's before-activation trace, AppKit has completed activation:
  `activation_policy=regular active=true`;
- the parent window is also key/main by then:
  `present=true key=true main=true visible=true ... matches_key=true matches_main=true`.

So Experiment 25 proves that:

- Roamium starts in prohibited AppKit activation policy;
- `setActivationPolicy:NSApplicationActivationPolicyRegular` succeeds when run
  before shell activation;
- the active/key/main state is available by the time native print starts;
- the previous inactive/non-key/non-main AppKit state is no longer the first
  remaining blocker.

Native print still does not pass:

- the harness records `gui_active_sent=true`;
- the probe status is `print-native-click-sent`;
- the first failing hop remains `mac-print-app-modal-response-missing`;
- the watcher does not observe a native print dialog;
- print queue state is unchanged;
- no print job is submitted;
- Roamium remains alive until harness shutdown.

Verification run:

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

rm -rf logs/issue-834-exp25-appkit-activation-policy
python3 scripts/test-issue-834-pdf-native-print.py \
  --log-dir logs/issue-834-exp25-appkit-activation-policy \
  --probe native-dialog \
  --allow-native-dialog-click
```

The final probe returned nonzero because the experiment did not reach safe
native-dialog cancellation. Its summary is at
`logs/issue-834-exp25-appkit-activation-policy/pdf-native-print-summary.json`.

The Chromium branch `148.0.7778.97-issue-834-exp25` was committed at
`588501a88803e2ce02e4c29a1918329316a22562`, and `chromium/patches/issue-834/`
was regenerated through `0082-Promote-AppKit-before-print.patch`.

## Completion Review

An adversarial Codex subagent reviewed the completed experiment with fresh
context.

Initial verdict: **Changes required**.

The reviewer found one Required documentation issue: this experiment initially
recorded the wrong Chromium commit hash. The actual Chromium branch tip and the
`0082` patch both identify `588501a88803e2ce02e4c29a1918329316a22562`.

Fix:

- corrected the recorded Chromium commit hash in this experiment file;
- formatted the file with Prettier.

Re-review verdict: **Approved**.

The reviewer confirmed that the recorded hash now matches `chromium/src` HEAD
and `chromium/patches/issue-834/0082-Promote-AppKit-before-print.patch`, with no
new Required findings.

## Conclusion

Experiment 25 eliminates AppKit activation policy and key/main window state as
the remaining native-print blocker. The next experiment should stop trying to
activate Roamium and instead examine the presentation API itself now that the
app/window are valid. The evidence points specifically at
`beginSheetWithPrintInfo:modalForWindow:nil`: it enters and exits without a
response or observable dialog even when the app is regular, active, and has a
key/main parent window. A likely next probe is to retry a real parent-window
sheet or synchronous modal path after policy promotion, with the same print-job
safety checks.
