# Experiment 7: Prove Visible Profile Identity

## Description

Experiment 1 left profile display/user-visible profile identity uncovered.
Experiments 2 through 6 proved the profile, tab, pane, server, reconnect, and
cleanup lifecycle behavior, but none of those runs proved that a user can see
which browser/profile/tab a viewport belongs to.

webtui already computes an `identity_label` in the viewport block footer using
the same value that is rendered on screen:

- normal browser with a known tab id: `browser_label/profile#tab_id`;
- normal browser before `TabReady`: `browser_label/profile#loading`;
- DevTools: `browser_label/profile#inspected_tab_id`, rendered under a
  `DevTools · ...` viewport title.

This experiment will add narrow state-trace instrumentation for that rendered
identity label, then add a focused Ghostboard runtime scenario that proves the
label for the default profile, a non-default profile, and DevTools. The trace is
used because it records the exact string passed to ratatui for the visible
footer without relying on OCR or screenshot pixel matching.

## Changes

Planned webtui changes:

- `webtui/src/main.rs`
  - Include the computed viewport `identity_label` in the existing
    `TERMSURF_WEBTUI_STATE_TRACE_FILE` render-state trace.
  - Include enough adjacent fields to make the assertion non-ambiguous: profile,
    browser label, current tab id, inspected tab id, and whether the view is
    DevTools.
  - Do not change the rendered UI string, layout, protocol messages, or normal
    behavior.

Planned harness changes:

- `scripts/ghostboard-geometry-matrix.sh`
  - Add a `visible-profile-identity` scenario.
  - Launch browser A with the default profile and assert the webtui trace
    reports a visible identity label matching `roamium/default#<tab>`.
  - Open a second native tab and launch browser B with `--profile profilea`;
    assert the webtui trace reports `roamium/profilea#<tab>` for browser B.
  - Open DevTools for browser B and assert the webtui trace reports the DevTools
    identity label for the inspected browser B tab:
    `roamium/profilea#<browser_b_tab_id>`.
  - Assert the profile-specific Ghostboard `SetOverlay` and `SetDevtoolsOverlay`
    logs agree with the traced labels so the trace cannot pass with a stale or
    unrelated render state.

Planned issue-document changes:

- Record the result in this experiment file.
- Update the Issue 818 README status for Experiment 7 after verification.

Planned app source changes:

- No Ghostboard or Roamium source changes are planned.
- The only planned source change is webtui trace instrumentation for the
  already-rendered identity label.

## Verification

Formatting actions:

1. `cargo fmt -p webtui`.
2. `prettier --write --prose-wrap always --print-width 80 issues/0818-ghostboard-profile-tab-lifecycle-matrix/README.md issues/0818-ghostboard-profile-tab-lifecycle-matrix/07-prove-visible-profile-identity.md`.

Static checks:

1. `git diff --check`.
2. `bash -n scripts/ghostboard-geometry-matrix.sh`.
3. `cargo check -p webtui`.

Runtime checks:

1. `scripts/ghostboard-geometry-matrix.sh visible-profile-identity`.

Pass criteria:

- The scenario launches browser A with the default profile.
- webtui traces a rendered identity label for browser A matching
  `roamium/default#<browser_a_tab_id>`.
- Browser A's traced tab id agrees with Ghostboard/Roamium tab-ready evidence.
- The scenario launches browser B with `--profile profilea`.
- webtui traces a rendered identity label for browser B matching
  `roamium/profilea#<browser_b_tab_id>`.
- Browser B's traced profile and tab id agree with Ghostboard `SetOverlay` and
  Roamium tab-ready evidence.
- The scenario opens DevTools for browser B.
- webtui traces a DevTools identity label matching
  `roamium/profilea#<browser_b_tab_id>`, where the id is the inspected browser
  tab id rather than the DevTools tab id.
- Ghostboard `SetDevtoolsOverlay` and `CreateDevtoolsTab` logs agree that the
  DevTools pane targets browser B's profile and inspected tab.
- No screenshot/OCR-only evidence is required; the trace must be the same string
  that webtui renders in the viewport footer.

Partial criteria:

- Normal browser identity labels pass for default and non-default profiles, but
  DevTools identity is inconclusive.
- The trace proves webtui computes the correct identity label, but a separate
  issue is needed for screenshot-level visual verification.
- The scenario exposes a specific profile-identity bug that should be fixed in
  the next experiment.

Fail criteria:

- webtui does not expose a traceable identity label without changing the
  rendered UI semantics.
- The normal browser identity omits the browser, profile, or tab id.
- The non-default profile label reports `default` or another stale profile.
- The DevTools label uses the DevTools tab id instead of the inspected browser
  tab id.
- The traced labels cannot be correlated with Ghostboard/Roamium runtime
  evidence.

## Design Review

This experiment is plan-only until a fresh-context adversarial design review
approves it. Record the reviewer verdict here, fix all real findings, and commit
the approved plan before implementation begins.

Fresh-context adversarial design review by Codex subagent `Zeno the 2nd`:

- **Verdict:** Approved.
- **Required findings:** None.
- **Reviewer checks:** The reviewer confirmed the Issue 818 README links
  Experiment 7 as `Designed`, the experiment has the required sections, the
  scope is limited to visible profile identity, the plan targets the existing
  webtui `identity_label` footer string without claiming screenshot/OCR proof,
  the pass/fail criteria cover default profile, non-default profile, and
  DevTools inspected-tab identity, and the hygiene checks cover Rust, shell, and
  Markdown.

## Completion Gate

After implementation and verification:

- add `## Result` and `## Conclusion` to this experiment file;
- update the Issue 818 README experiment status from `Designed` to `Pass`,
  `Partial`, or `Fail`;
- request a fresh-context completion review;
- fix all real completion-review findings and record the final verdict in this
  file; and
- commit the reviewed result separately before designing or implementing the
  next experiment.
