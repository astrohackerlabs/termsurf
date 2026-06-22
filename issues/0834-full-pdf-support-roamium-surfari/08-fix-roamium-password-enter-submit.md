# Experiment 8: Fix Roamium PDF Password Enter Submission

## Description

Experiment 7 proved that Roamium can load password-protected PDFs when the
password is typed through TermSurf protocol key events and the visible Chromium
PDF password dialog submit button is clicked. It also found a keyboard parity
gap: Enter key down/up events reach the Chromium PDF extension target, but the
password dialog remains open and does not submit.

This experiment should determine why Enter submission fails, fix the narrowest
real integration bug, and rerun the password PDF probes until the
password-protected row no longer needs the click-submit caveat.

Because the failing path is inside Roamium/Chromium PDF input routing, this
experiment may modify the Chromium fork if investigation proves the current
synthetic keyboard event is incomplete. If Chromium source changes are needed,
create a fresh Chromium branch for Experiment 8 before editing Chromium source,
update the Chromium branch table, build `libtermsurf_chromium`, and archive the
patches according to `chromium/AGENTS.md`.

## Changes

1. Reproduce the Experiment 7 Enter-only failure from a clean log directory.

   Run:

   ```bash
   python3 scripts/test-issue-834-pdf-password.py \
     --log-dir logs/issue-834-exp8-password-enter-before \
     --probe password-protected \
     --credential-flow correct-only \
     --submit-mode enter
   ```

   The expected starting failure is
   `first_failing_hop = "correct-password-not-accepted"` with Enter key down/up
   events recorded as `windows_key_code = 13`.

2. Inspect the synthetic key event path.

   Compare the TermSurf-generated Enter event with Chromium's expected
   keyboard-event fields for activation keys. Inspect at least:

   - `chromium/src/content/libtermsurf_chromium/ts_browser_main_parts.cc`;
   - the local `ts_forward_key_event` wrapper;
   - Chromium references for `NativeWebKeyboardEvent`, `DomCode::ENTER`,
     `DomKey::ENTER`, `VKEY_RETURN`, `text`, `unmodified_text`, and platform
     native event fields;
   - existing TermSurf PDF keyboard probes, especially find/search and page
     navigation, to avoid regressing working keyboard behavior.

   Determine whether the failure is caused by:

   - missing or incorrect `dom_key`;
   - missing or incorrect `dom_code`;
   - sending a `kChar` event for Enter when the PDF dialog expects only raw key
     events;
   - missing macOS/native keyboard fields;
   - focus targeting the `cr-input` host instead of the native inner input;
   - Chromium PDF viewer dialog behavior that requires button activation even
     for real Enter;
   - another specific layer.

3. Fix only the proven layer.

   Prefer a narrow fix that improves TermSurf key synthesis for non-text keys
   without changing the protocol or broad PDF behavior. Do not add DevTools DOM
   submission, JavaScript button clicks, or test-only bypasses. The password
   must still be typed and submitted through the TermSurf protocol input path.

   If Chromium is changed:

   - create a branch named `148.0.7778.97-issue-834-exp8` from the current
     relevant Issue 834 Chromium branch;
   - update `chromium/README.md`;
   - build with:

     ```bash
     cd chromium/src
     export PATH="/Users/astrohacker/dev/termsurf/chromium/depot_tools:$PATH"
     autoninja -C out/Default libtermsurf_chromium
     ```

   - regenerate the cumulative Issue 834 patch archive under
     `chromium/patches/issue-834/`, preserving the established Issue 834
     Chromium archive location.

4. Rerun focused password probes.

   Use fresh final log directories:

   ```bash
   python3 scripts/test-issue-834-pdf-password.py \
     --log-dir logs/issue-834-exp8-password-enter-after \
     --probe password-protected \
     --credential-flow correct-only \
     --submit-mode enter
   python3 scripts/test-issue-834-pdf-password.py \
     --log-dir logs/issue-834-exp8-password-wrong-enter-after \
     --probe password-protected \
     --credential-flow wrong-only \
     --submit-mode enter
   python3 scripts/test-issue-834-pdf-password.py \
     --log-dir logs/issue-834-exp8-password-control-after \
     --probe unrestricted-control
   ```

5. Run keyboard regression probes.

   At minimum rerun the Roamium PDF keyboard probes already introduced in Issue
   834:

   ```bash
   python3 scripts/test-issue-834-pdf-navigation.py \
     --log-dir logs/issue-834-exp8-keyboard-page-scroll-regression \
     --serve-bitcoin-pdf \
     --probe keyboard-page-scroll
   python3 scripts/test-issue-834-pdf-navigation.py \
     --log-dir logs/issue-834-exp8-toolbar-page-selector-regression \
     --serve-bitcoin-pdf \
     --probe toolbar-page-selector
   python3 scripts/test-issue-834-pdf-find.py \
     --log-dir logs/issue-834-exp8-find-positive-regression \
     --probe positive-search
   ```

   Each regression summary should record
   `first_failing_hop = "no-failure-observed"` or the result must explain the
   concrete regression before proceeding.

   Rerun the password click-submit probes from Experiment 7 if the Enter fix
   touches shared key routing.

   Add a smaller regression command only if the existing probes are too broad,
   but do not replace them with a weaker check unless the result explains why.

## Verification

Verification for the completed result is:

- the pre-fix Enter-only failure is reproduced and classified from
  `logs/issue-834-exp8-password-enter-before`;
- the result identifies the exact failing layer before applying a fix;
- no DevTools DOM mutation or JavaScript submission is used as a product or test
  substitute;
- after the fix, correct-password Enter submission exits 0 with
  `first_failing_hop = "no-failure-observed"`, `submit_mode = "enter"`,
  `correct_password_loaded = true`, and Enter key-code evidence;
- wrong-password Enter submission exits 0 with `wrong_password_rejected = true`
  and, where stable, `wrong_password_invalid_observed = true`;
- unrestricted PDF control still exits 0;
- raw fixed test passwords do not appear in summaries or logs;
- required keyboard regression probes still pass;
- if Chromium source changes, `chromium/README.md` and the cumulative
  `chromium/patches/issue-834/` archive are updated and
  `autoninja -C out/Default libtermsurf_chromium` passes;
- if Chromium source does not change, the result explains where the fix landed;
- `node --check scripts/probe-pdf-password.mjs` passes if the Node probe is
  edited;
- `PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile scripts/test-issue-834-pdf-password.py`
  passes if the Python harness is edited, and `scripts/__pycache__/` is removed
  afterward;
- markdown is formatted with Prettier;
- `git diff --check` passes;
- design review is recorded, all required design-review findings are fixed, the
  design is approved, and the plan commit exists before implementation begins;
- completion review is recorded before the result commit.

## Design Review

Fresh-context adversarial review by Codex subagent `Ohm`: **Changes required**.

Required findings:

- The design omitted the required design-review and plan-commit gate before
  implementation. Fixed by adding explicit verification that design review is
  recorded, required findings are fixed, approval is obtained, and the plan
  commit exists before implementation begins.
- The design originally named `chromium/patches/issue-834-exp8/`, which
  conflicted with the established cumulative Issue 834 Chromium patch archive.
  Fixed by requiring updates to `chromium/patches/issue-834/`.

Optional finding:

- Keyboard regression verification named the prior experiments but did not give
  exact commands or expected summary status. Fixed by adding concrete navigation
  and find/search regression commands and requiring
  `first_failing_hop = "no-failure-observed"` or an explained regression.

Fresh-context adversarial re-review by Codex subagent `Lagrange`: **Approved**.

Findings: none.

The reviewer confirmed that the design-review/plan-commit gate, cumulative
`chromium/patches/issue-834/` archive requirement, and concrete keyboard
regression commands are fixed, with no new required findings.

## Pass Criteria

This experiment passes if Roamium password-protected PDFs can be submitted with
Enter through the TermSurf protocol, wrong-password Enter submission is rejected
correctly, and existing Roamium PDF keyboard workflows do not regress.

## Partial Criteria

This experiment is partial if it reproduces and classifies the Enter submission
failure but cannot safely fix it in this experiment, or if the fix works for the
correct-password path but exposes a separate keyboard regression.

## Failure Criteria

This experiment fails if the pre-fix failure cannot be reproduced, if the fix
submits the password through DevTools or a test-only bypass instead of TermSurf
protocol input, if raw test passwords leak into logs, or if a Chromium change is
made without the required branch, build, and patch archive workflow.
