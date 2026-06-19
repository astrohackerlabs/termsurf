# Experiment 10: Localize Display-Move Single-Display Focus

## Description

Experiment 9 fixed the cleanup-time browser socket panic and resumed the
inherited viewport matrix. The first remaining failure was
`display-move-backing-scale`.

In this macOS VM only one display is available, so the cross-display part of the
row cannot run. The row correctly entered its single-display fallback, verified
the overlay geometry and hit-test, pressed Enter to put `webtui` into Browse
mode, and then failed waiting for Roamium focus evidence:

```text
PARTIAL: only one display is available; cross-display move cannot run in this VM
single_display_mode_key=enter=Mode::Browse
PASS: single-display webtui entered browse mode
FAIL: missing Roamium observed focus=true on single display
```

The app log shows the GUI side did emit focus after the Enter key:

```text
ModeChanged: pane_id=... browsing=true
FocusChanged: pane_id=... tab_id=1 focused=true
```

But the Roamium trace did not contain the expected post-Enter line:

```text
focus-changed tab=1 pane=... ffi=ts_set_focus focused=true
```

This experiment should localize that gap before changing product behavior. The
likely possibilities are:

- Ghostboard sends `FocusChanged`, but `sendProtobuf` fails or races a browser
  disconnect.
- Roamium's existing focus trace/FFI path does not run after Enter. Today that
  trace line is emitted immediately before `ts_set_focus`, so this experiment
  can distinguish Ghostboard send/routing from Roamium trace/FFI execution, but
  it cannot separately prove Roamium message receipt vs FFI application unless a
  later experiment adds an explicit Roamium receive-side diagnostic.
- The single-display fallback harness starts its trace cursor too late or waits
  for a focus transition that already happened before Enter.
- The single-display fallback differs from the multi-display path in how it
  primes browser focus or Browse/Control mode.

## Changes

- `scripts/ghostboard-geometry-matrix.sh`
  - Add targeted diagnostics to `display-move-backing-scale` single-display
    fallback only.
  - Capture app-side `ModeChanged`, `FocusChanged`, `KeyEvent`, and any
    focus-send failure evidence around the Enter key.
  - Capture Roamium focus trace lines before and after Enter, including whether
    a prior `focused=true` line already exists before the harness starts
    waiting.
  - Keep the existing failure condition intact unless the evidence proves the
    harness expectation is wrong.
- `roamium/src/dispatch.rs`
  - Do not modify in this experiment. If the evidence requires separating
    Roamium message receipt from `ts_set_focus` application, record that as the
    next experiment instead of broadening this one.
- `issues/0826-update-ghostboard-to-latest-ghostty/README.md`
  - Link this experiment with status `Designed`, then update the status after
    the result is known.
- `issues/0826-update-ghostboard-to-latest-ghostty/10-localize-display-move-single-display-focus.md`
  - Record design, verification, result, reviews, and conclusion.

Do not modify Ghostboard product code, `webtui/`, `roamium/`, `chromium/`, or
`proto/termsurf.proto` in this experiment unless the added diagnostics prove a
specific product bug that can be fixed narrowly in the same experiment.

## Verification

Before changes, preserve the failure context:

```bash
rg -n 'PARTIAL:|FAIL:|single_display|ModeChanged:|FocusChanged:|focus-changed|ts_set_focus|key-event' \
  logs/ghostboard-geometry-display-move-backing-scale-harness-20260619-125707.log \
  logs/ghostboard-geometry-display-move-backing-scale-app-20260619-125707.log \
  logs/ghostboard-geometry-display-move-backing-scale-roamium-20260619-125707.log \
  > logs/issue-0826-exp10-before-focus-failure-evidence.log
```

After diagnostic changes, run static checks:

```bash
bash -n scripts/ghostboard-geometry-matrix.sh
prettier --write --prose-wrap always --print-width 80 \
  issues/0826-update-ghostboard-to-latest-ghostty/README.md \
  issues/0826-update-ghostboard-to-latest-ghostty/10-localize-display-move-single-display-focus.md
git diff --check
```

Rerun the failing row with overrides unset:

```bash
env -u TERMSURF_GHOSTBOARD_APP \
  -u TERMSURF_WEB \
  -u TERMSURF_ROAMIUM \
  -u TERMSURF_INSTALLED_ROAMIUM \
  scripts/ghostboard-geometry-matrix.sh display-move-backing-scale \
  > logs/issue-0826-exp10-display-move-rerun.log 2>&1
```

Extract the latest artifacts:

```bash
APP_LOG="$(ls -t logs/ghostboard-geometry-display-move-backing-scale-app-*.log | head -1)"
HARNESS_LOG="$(ls -t logs/ghostboard-geometry-display-move-backing-scale-harness-*.log | head -1)"
ROAMIUM_TRACE="$(ls -t logs/ghostboard-geometry-display-move-backing-scale-roamium-*.log | head -1)"

printf 'APP_LOG=%s\nHARNESS_LOG=%s\nROAMIUM_TRACE=%s\n' \
  "$APP_LOG" "$HARNESS_LOG" "$ROAMIUM_TRACE" \
  > logs/issue-0826-exp10-selected-artifacts.log

rg -n 'single_display|ModeChanged:|FocusChanged:|KeyEvent:|focus diagnostic|focus send|SetFocus|focused=true|focused=false' \
  "$APP_LOG" "$HARNESS_LOG" \
  > logs/issue-0826-exp10-app-harness-focus-evidence.log || true
rg -n 'focus-changed|ts_set_focus|key-event' "$ROAMIUM_TRACE" \
  > logs/issue-0826-exp10-roamium-focus-evidence.log || true
```

If diagnostics prove that the harness expected a duplicate focus transition
after Enter but Roamium was already focused before Enter, fix only the harness
expectation and rerun the row. If diagnostics prove Ghostboard sends
`FocusChanged` but the Roamium focus trace/FFI path does not run, record whether
the failure is still at the Ghostboard send/routing boundary or needs a later
Roamium receive-side diagnostic. Do not claim to distinguish Roamium receipt
from FFI application using the current Roamium trace alone.

After any fix in this experiment, rerun:

```bash
env -u TERMSURF_GHOSTBOARD_APP \
  -u TERMSURF_WEB \
  -u TERMSURF_ROAMIUM \
  -u TERMSURF_INSTALLED_ROAMIUM \
  scripts/ghostboard-geometry-matrix.sh display-move-backing-scale \
  > logs/issue-0826-exp10-display-move-after-fix.log 2>&1
```

Run final scope checks:

```bash
ps -axo pid,comm,args \
  | rg 'TermSurf\.app/Contents/MacOS/termsurf|target/debug/web|chromium/src/out/Default/roamium' \
  | rg -v 'rg|ps -axo|zsh -lc' \
  > logs/issue-0826-exp10-post-cleanup-processes.log || true
test ! -s logs/issue-0826-exp10-post-cleanup-processes.log
git status --short -- webtui roamium proto/termsurf.proto chromium/README.md chromium/patches \
  > logs/issue-0826-exp10-forbidden-top-status.log
git -C chromium/src status --short > logs/issue-0826-exp10-chromium-status.log
git -C chromium/src diff --name-only > logs/issue-0826-exp10-chromium-diff-name-only.log
git diff --name-only > logs/issue-0826-exp10-git-diff-name-only.log
test ! -s logs/issue-0826-exp10-forbidden-top-status.log
test ! -s logs/issue-0826-exp10-chromium-status.log
test ! -s logs/issue-0826-exp10-chromium-diff-name-only.log
```

Pass criteria:

- The experiment identifies where the focus evidence is lost among the layers
  this experiment can prove: harness timing, Ghostboard send/routing, or Roamium
  focus trace/FFI path.
- If the evidence reaches the Roamium boundary but cannot distinguish receipt
  from FFI application, the result says so explicitly and does not overclaim.
- If the issue is fixed in this experiment, `display-move-backing-scale` passes
  on the single-display VM path or records only the expected cross-display
  partial note.
- If the issue is not fixed in this experiment, the next experiment has concrete
  evidence naming the failing layer and exact logs to inspect.
- `bash -n`, Prettier, and `git diff --check` are clean.
- Cleanup leaves no stale matching app, web, or Roamium processes.
- No forbidden paths are modified: `webtui/`, `roamium/`, `chromium/`, or
  `proto/termsurf.proto`.
- Any `ghostboard/` product-code change is explicitly justified by the recorded
  localization evidence, and `logs/issue-0826-exp10-git-diff-name-only.log`
  names the changed files.
- The nested `chromium/src` checkout has no uncommitted status or diff from this
  experiment.

Partial criteria:

- Diagnostics localize the failure but the fix is product-code risky enough to
  require a separate experiment.
- `display-move-backing-scale` passes, but the next remaining matrix row fails
  with clear evidence.

Fail criteria:

- The experiment changes product behavior before localizing the failure.
- Diagnostic output is too weak to distinguish harness timing from product focus
  routing.
- A failure is hidden by shell pipeline behavior.

## Design Review

An adversarial Codex subagent reviewed the initial design with fresh context.

**Verdict:** Changes required.

Required findings and fixes:

- The initial design claimed it could distinguish Roamium receive/apply from
  Roamium trace emission, but `roamium/src/dispatch.rs` emits the
  `focus-changed ... ffi=ts_set_focus` trace immediately before calling
  `ts_set_focus`. Fixed by narrowing the experiment's claim: it can distinguish
  Ghostboard send/routing from Roamium trace/FFI execution, but cannot separate
  Roamium message receipt from FFI application without a later Roamium
  diagnostic.
- The stale-process cleanup check wrote a log file but did not fail if the log
  was non-empty. Fixed by adding
  `test ! -s logs/issue-0826-exp10-post-cleanup-processes.log`.

Optional finding and fix:

- The scope gate allowed a same-experiment Ghostboard product-code fix after
  localization, but final scope checks did not record the full changed-file set.
  Fixed by adding `logs/issue-0826-exp10-git-diff-name-only.log` and requiring
  any `ghostboard/` product-code change to be justified by the recorded
  localization evidence.

The final re-review approved the design with no remaining required findings.

## Result

**Result:** Pass

Implemented the approved diagnostics in `scripts/ghostboard-geometry-matrix.sh`
for only the `display-move-backing-scale` single-display fallback. The
diagnostics now log:

- the Roamium focus trace cursor before Enter;
- pre-Enter true/false focus counts and last focus line;
- the exact AppKit Enter key-down line;
- the exact app-side `ModeChanged` and `FocusChanged` lines after Enter;
- post-Enter true/false focus counts and deltas;
- the last post-Enter Roamium focus trace line.

No Ghostboard product code, `webtui/`, `roamium/`, `chromium/`, or
`proto/termsurf.proto` files were changed.

The first diagnostic rerun passed:

```text
display_move_rc=0
```

The final rerun with exact app-line capture also passed:

```text
display_move_after_fix_rc=0
```

The latest evidence was written to:

```text
logs/issue-0826-exp10-selected-artifacts.log
logs/issue-0826-exp10-app-harness-focus-evidence.log
logs/issue-0826-exp10-roamium-focus-evidence.log
logs/issue-0826-exp10-display-move-after-fix.log
```

The final run used:

```text
APP_LOG=logs/ghostboard-geometry-display-move-backing-scale-app-20260619-131043.log
HARNESS_LOG=logs/ghostboard-geometry-display-move-backing-scale-harness-20260619-131043.log
ROAMIUM_TRACE=logs/ghostboard-geometry-display-move-backing-scale-roamium-20260619-131043.log
```

The relevant single-display focus evidence was:

```text
PARTIAL: only one display is available; cross-display move cannot run in this VM
single_display_trace_start_line=17
single_display_pre_enter_focus_true_count=0
single_display_pre_enter_focus_false_count=0
single_display_pre_enter_last_focus=<none>
single_display_mode_key=enter=Mode::Browse
PASS: single-display webtui entered browse mode
PASS: single-display Ghostboard emitted focus=true after browse mode
single_display_mode_app_log_line=info(termsurf): ModeChanged: pane_id=0AF1CD11-7011-4C7F-8C93-A3A322921673 browsing=true
single_display_focus_app_log_line=info(termsurf): FocusChanged: pane_id=0AF1CD11-7011-4C7F-8C93-A3A322921673 tab_id=1 focused=true
single_display_enter_key_appkit_log_line=383:TermSurf geometry layer=appkit event=key_down scenario=display-move-backing-scale ... note=key_code=36 modifiers=0 focused=true
single_display_post_enter_focus_true_count=1
single_display_post_enter_focus_false_count=0
single_display_focus_true_delta=1
single_display_focus_false_delta=0
single_display_post_enter_last_focus=18:roamium focus-changed tab=1 pane=0AF1CD11-7011-4C7F-8C93-A3A322921673 ffi=ts_set_focus focused=true
PASS: Roamium observed focus=true on single display
PASS: single-display keyboard marker reached browser
PASS: scenario display-move-backing-scale partial-single-display
```

The row still records the expected VM limitation as a partial note because only
one display is available, but the single-display fallback now passes its
overlay, focus, and browser keyboard checks. The previous missing Roamium focus
line did not reproduce in either rerun.

Verification checks:

```text
bash -n scripts/ghostboard-geometry-matrix.sh
prettier --write --prose-wrap always --print-width 80 \
  issues/0826-update-ghostboard-to-latest-ghostty/README.md \
  issues/0826-update-ghostboard-to-latest-ghostty/10-localize-display-move-single-display-focus.md
git diff --check
test ! -s logs/issue-0826-exp10-post-cleanup-processes.log
test ! -s logs/issue-0826-exp10-forbidden-top-status.log
test ! -s logs/issue-0826-exp10-chromium-status.log
test ! -s logs/issue-0826-exp10-chromium-diff-name-only.log
```

All checks passed. `logs/issue-0826-exp10-git-diff-name-only.log` contained
only:

```text
scripts/ghostboard-geometry-matrix.sh
```

## Conclusion

Experiment 10 did not find a reproducible Ghostboard or Roamium product bug. The
diagnostics proved that, in the current runs, Enter reaches AppKit, webtui
enters Browse mode, Ghostboard emits `FocusChanged focused=true`, Roamium
records a new `ts_set_focus focused=true` trace after Enter, and browser
keyboard forwarding works in the single-display fallback.

The next experiment should resume the inherited viewport matrix after
`display-move-backing-scale` to find the next remaining failure, if any.

## Completion Review

An adversarial Codex subagent reviewed the completed experiment with fresh
context.

**Verdict:** Approved.

Findings:

- No required findings.
- No optional findings.
- No nit findings.

The reviewer independently verified that
`bash -n scripts/ghostboard-geometry-matrix.sh` and `git diff --check` were
clean, that the result commit had not already been made, that forbidden-path and
Chromium scope checks were clean, and that the final focused logs showed the
single-display fallback passing overlay, focus, and browser keyboard checks
without hiding the expected one-display partial note.
