# Experiment 1: Prove Direct Browser State Smoke

## Description

Issue 810 found that Ghostboard likely relies on webtui's direct Roamium socket
for ordinary browser chrome/status messages after `BrowserReady`. Static
evidence says URL, loading state, title, hover target URL, console messages,
navigation, and runtime reload should work through that direct path, but there
is no current Ghostboard runtime walkthrough proving the visible behavior.

This experiment will add a focused runtime smoke to prove the normal direct path
for browser state and simple interruption-adjacent behavior before changing app
code. It deliberately does not cover JavaScript dialogs, HTTP auth, renderer
crash recovery, or color scheme; those need separate fixtures and ownership
decisions after this baseline is known.

## Changes

Planned harness changes:

- `scripts/ghostboard-geometry-matrix.sh`
  - Add a `browser-state-smoke` scenario.
  - Serve a temporary local HTML fixture from the harness. The fixture should:
    - set an initial title such as `Issue 816 State Smoke`;
    - update the title after load to prove `TitleChanged`;
    - emit a unique `console.log` marker;
    - include a visible link with a unique target URL for hover-target status;
    - include a link or button that opens `target=_blank`;
    - render a plain white page region that can be sampled by screenshot or
      pixel probe;
    - expose a reload marker so Cmd-R or webtui reload can be proven.
  - Launch debug Ghostboard with debug webtui and named/default Roamium, using
    the same no-installed-binary guarantees as the existing named-Roamium
    scenario.
  - Capture app log, Roamium trace, screenshots, and any terminal output needed
    to correlate visible TUI state.
  - Reuse existing geometry assertions so state evidence does not hide a broken
    overlay.

Planned probe/assertion changes:

- Assert each browser-state item at the consumer/UX boundary. Roamium trace can
  be supporting evidence, but it is not sufficient by itself for a `Pass`.
- Assert URL and loading-state transitions reach webtui's event/state layer and
  are reflected in terminal capture, screenshot/OCR, or another explicit
  visible-state probe.
- Assert the title update reaches webtui's state and visible terminal state.
- Move the mouse over the fixture link and assert the hover target URL reaches
  webtui's state and visible terminal state.
- Assert the console marker is received by webtui and visible in captured
  output.
- Trigger reload and assert a second load/reload marker reaches webtui's
  consumer boundary.
- Trigger `target=_blank` and assert the expected current product behavior: a
  new browser target should become visible to the user as a browser tab or URL
  state change for the target URL. A silent no-op is a failure. If Roamium or
  webtui currently implements a different explicit UX, record that behavior,
  owner, and evidence as `Partial` or `Fail` rather than treating undefined
  behavior as pass.
- Assert the page background is white in the browser viewport screenshot.
- Classify each sub-result as `Pass`, `Partial`, or `Fail`, with the owner:
  Ghostboard, webtui, Roamium, or test harness.

Planned issue-doc changes:

- Record the fixture, commands, logs, screenshots, per-feature result table, and
  any follow-up experiments required for failures or unproven paths.

## Verification

Formatting actions:

1. `prettier --write --prose-wrap always --print-width 80 issues/0816-ghostboard-browser-state-interruptions/README.md issues/0816-ghostboard-browser-state-interruptions/01-prove-direct-browser-state-smoke.md`.

Static/build checks:

1. `prettier --check --prose-wrap always --print-width 80 issues/0816-ghostboard-browser-state-interruptions/README.md issues/0816-ghostboard-browser-state-interruptions/01-prove-direct-browser-state-smoke.md`.
2. `bash -n scripts/ghostboard-geometry-matrix.sh`.
3. `shellcheck scripts/ghostboard-geometry-matrix.sh` if available.
4. `git diff --check`.

Runtime checks:

1. `scripts/ghostboard-geometry-matrix.sh browser-state-smoke`.
2. Confirm the harness records:
   - URL change to the fixture URL at the webtui/visible-state boundary;
   - loading start and finish at the webtui/visible-state boundary;
   - initial and updated page title at the webtui/visible-state boundary;
   - hover target URL for the link at the webtui/visible-state boundary;
   - console marker at the webtui/visible-output boundary;
   - reload marker after reload at the webtui/visible-state boundary;
   - target-blank user-visible browser tab or URL state change for the target
     URL, or an explicit `Partial`/`Fail` owner classification;
   - white browser viewport background.

Pass criteria:

- The scenario runs to completion under debug Ghostboard without installed
  binary leakage.
- URL, loading, title, hover target, console, reload, target blank, and white
  background are all proven at the webtui/visible consumer boundary. Roamium
  trace is supporting evidence only.
- Each proven behavior has a durable assertion in the harness.
- Any failure identifies the likely owner and enough evidence to design the next
  experiment.

Partial criteria:

- The harness runs and proves some, but not all, state behaviors. Missing
  behavior is recorded with owner and next experiment.

Fail criteria:

- The harness cannot reliably launch the fixture under Ghostboard.
- Evidence is too indirect to distinguish Ghostboard, webtui, Roamium, or
  harness ownership.
- The smoke requires broad app changes before proving a specific missing
  behavior.

## Design Review

Fresh-context adversarial review by Codex subagent `Avicenna`:

- **Initial verdict:** Changes required.
- **Required finding:** The design allowed URL/loading/title/status behavior to
  pass from Roamium traces or app logs alone, even though the issue needs
  visible Ghostboard/webtui behavior. Roamium trace should be supporting
  evidence, not sole pass evidence.
- **Required finding:** The `_blank` subcheck did not define expected behavior,
  making it impossible to prove or reject.
- **Resolution:** Accepted both findings. The design now requires webtui/visible
  consumer-boundary evidence for state behavior, and defines `_blank` as
  requiring a user-visible browser tab or URL state change for the target URL.
  Undefined or differing current behavior must be recorded as `Partial` or
  `Fail` with owner evidence.
- **Re-review verdict:** Approved. The reviewer confirmed both prior findings
  were resolved and no new required findings were introduced.
