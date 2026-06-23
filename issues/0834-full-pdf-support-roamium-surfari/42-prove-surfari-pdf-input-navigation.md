# Experiment 42: Prove Surfari PDF Input Navigation

## Description

Experiment 41 proved Surfari can load and visibly present PDFs through the
common full-page, extensionless, local-file, and embedded paths. The next matrix
slice should prove that the visible Surfari PDF surface responds to real user
input routed through TermSurf.

This experiment should test Surfari only. It should start as a probe and make no
product changes unless the probe exposes a real TermSurf integration gap.

## Changes

- Add a focused harness, tentatively
  `scripts/test-issue-834-surfari-pdf-input-navigation.sh`.
- Generate a deterministic multi-page PDF fixture with page-specific target
  colors that can prove viewport movement without relying on text rendering:
  - page 1: green;
  - page 2: magenta;
  - page 3: cyan or another distinct non-wrapper color.
- Serve the PDF over HTTP as `application/pdf`.
- Launch repo-built Ghostboard with repo-built WebTUI and repo-built Surfari. Do
  not use installed artifacts.
- Run with `TERMSURF_SURFARI_CACONTEXT_LAYER` unset so the test proves the
  default Surfari presentation path.
- Establish the baseline:
  - WebTUI requested `browser=surfari`;
  - Surfari emitted `BrowserReady`;
  - WebTUI reached ready state;
  - Surfari trace recorded the PDF URL;
  - Surfari emitted nonzero CAContext;
  - Surfari internal render proof passed;
  - Ghostboard-window overlay-cropped visible proof shows page 1's target color.
- Exercise real input routed through the existing TermSurf path in separate,
  explicit scenarios:
  - scroll scenario: start from a freshly loaded PDF at page 1, send scroll
    wheel input, and require a page-color transition from green toward magenta;
  - keyboard scenario: start from a freshly loaded PDF at page 1, click inside
    the overlay to establish focus, send `PageDown`, and require a page-color
    transition from green toward magenta;
  - continue the keyboard scenario by sending `PageDown` again, when needed, to
    reach page 3/cyan before sending `PageUp`;
  - send `PageUp` from a known non-page-1 state and require a page-color
    transition back toward the previous page color.
- Prefer the same CGEvent/AppKit automation path already proven by Experiments
  40 and 41. If a specific key cannot be delivered in the macOS VM, record the
  exact failed event path and classify the experiment Partial rather than
  replacing user input with direct Surfari internals.
- For every scroll or keyboard navigation step, require correlated evidence:
  - Ghostboard input trace or app log evidence that the input was delivered to
    the Surfari overlay/pane;
  - Surfari input trace evidence for the corresponding scroll or key event when
    available;
  - Surfari snapshot/render refresh evidence after input;
  - pre/post Ghostboard-window overlay-cropped pixel counts proving the target
    page color changed in the expected direction from the explicitly recorded
    starting color.
- For the mouse click step, do not require a page-color delta. Require:
  - Ghostboard input trace or app log evidence that the click was delivered to
    the Surfari overlay/pane;
  - Surfari trace evidence for the mouse event and focus change when available,
    such as `focus-changed ... focused=true`;
  - no visible PDF regression after the click;
  - the subsequent keyboard event reaches Surfari after the click, proving the
    click/focus route is sufficient for keyboard navigation.
- Use explicit pixel thresholds:
  - the dominant pre-input target color must be at least 5,000 pixels inside the
    overlay crop;
  - the dominant post-input target color must be at least 5,000 pixels inside
    the overlay crop;
  - the expected target color must increase by at least 5,000 pixels, or 10% of
    the overlay crop, whichever is smaller;
  - source/helper-window pixels cannot satisfy the proof.
- Record a JSON summary with:
  - env state proving `TERMSURF_SURFARI_CACONTEXT_LAYER` was unset;
  - repo binary paths;
  - PDF URL and request/content-type evidence;
  - input method used for each step;
  - pre/post screenshot paths and target color counts for scroll, click,
    `PageDown`, and `PageUp`;
  - the explicitly recorded starting and ending target color for each scroll or
    key navigation step;
  - relevant Ghostboard, WebTUI, and Surfari trace lines;
  - pass/partial/fail classification per input type;
  - cleanup status for Ghostboard, Surfari, WebTUI, and the fixture server.
- Update this experiment file with the result.

## Verification

Run build and hygiene checks:

```bash
./surfari/libtermsurf_webkit/build.sh
cargo fmt -p surfari
cargo build -p surfari
cargo build -p webtui
(cd ghostboard && macos/build.nu --configuration Debug --action build)
bash -n scripts/test-issue-834-surfari-pdf-input-navigation.sh
git diff --check
git -C webkit/src status --short
```

Run the input-navigation harness:

```bash
rm -rf logs/issue-834-exp42-surfari-pdf-input-navigation
env -u TERMSURF_SURFARI_CACONTEXT_LAYER \
  scripts/test-issue-834-surfari-pdf-input-navigation.sh
```

Pass criteria:

- `TERMSURF_SURFARI_CACONTEXT_LAYER` is unset;
- all scenarios use repo-built Ghostboard, WebTUI, Surfari, and WebKit
  artifacts;
- the HTTP PDF request is recorded with `application/pdf`;
- Surfari internal render proof passes before input;
- baseline Ghostboard-window overlay-cropped visible proof shows page 1 target
  pixels;
- scroll wheel input is delivered through TermSurf and produces a visible
  green-to-magenta target-color transition from a freshly loaded page-1 state;
- mouse click inside the overlay is delivered through TermSurf, records concrete
  focus/input evidence, does not break PDF visibility, and is followed by a key
  event that reaches Surfari;
- `PageDown` input is delivered through TermSurf and produces a visible
  green-to-magenta target-color transition from a freshly loaded page-1 state;
- `PageUp` input is delivered through TermSurf and produces a visible
  target-color transition back toward the previous page color from a known
  non-page-1 state;
- every visible proof is bounded to the Ghostboard app window and overlay crop;
- source/helper-window pixels cannot satisfy the proof;
- cleanup leaves no running Ghostboard, Surfari, WebTUI, or fixture server
  process;
- `webkit/src` remains clean;
- design review and completion review are recorded.

Partial criteria:

- scroll works but keyboard navigation does not;
- keyboard input reaches Surfari traces but WebKit's PDF viewer does not move in
  the VM;
- mouse click is delivered but concrete focus evidence is unavailable, while
  subsequent keyboard delivery still proves routing;
- mouse click/focus works but page navigation needs a follow-up fixture or input
  route;
- one navigation direction passes but the reverse direction fails.

Failure criteria:

- baseline PDF visibility regresses;
- the harness requires `TERMSURF_SURFARI_CACONTEXT_LAYER=snapshot`;
- input proof can only pass by directly mutating Surfari internals;
- no real input path reaches the Surfari PDF overlay;
- visible proof can be satisfied by a helper/source window;
- cleanup leaves running processes.

## Design Review

An external Codex review checked the design.

Initial verdict: **Changes required**.

Findings:

- click proof contradicted the "every input step" pixel-delta requirement,
  because a focus click should not be required to move the PDF viewport;
- the navigation sequence was underspecified, leaving the starting viewport and
  expected color transition ambiguous for each input step;
- focus routing was too vague and needed concrete evidence that the click
  established a route for subsequent keyboard input.

Resolution:

- click now has separate semantics: TermSurf-routed mouse delivery, focus/input
  evidence, no visible PDF regression, and subsequent keyboard delivery;
- scroll and first `PageDown` now start from freshly loaded page-1 states with
  explicit green-to-magenta proof;
- `PageUp` now starts from a known non-page-1 state and must move back toward
  the previous page color;
- focus evidence now includes Ghostboard/Safari input traces,
  `focus-changed ... focused=true` when available, and proof that a subsequent
  key event reaches Surfari.

Follow-up verdict: **Approved**.

The reviewer found no remaining required design findings and approved the plan
for the Experiment 42 plan commit and implementation.

## Result

**Result:** Pass.

Added `scripts/test-issue-834-surfari-pdf-input-navigation.sh` and ran it
against repo-built Ghostboard, WebTUI, Surfari, and WebKit artifacts with
`TERMSURF_SURFARI_CACONTEXT_LAYER` unset.

The verification run `20260622-210703` produced
`logs/issue-834-exp42-surfari-pdf-input-navigation/surfari-pdf-input-navigation-summary.json`
with `overall_result = pass` and classification
`surfari-pdf-input-navigation-proven`.

The harness generates a deterministic three-page PDF:

- page 1: green;
- page 2: magenta;
- page 3: cyan.

It runs two fresh-load scenarios against `/navigation.pdf` served as
`application/pdf`:

- scroll scenario: enter Browse mode, send real CGEvent scroll input through the
  TermSurf path, require Surfari `scroll-event` trace evidence, require
  `snapshot-layer-refresh`, and prove a green-to-magenta visible delta in the
  Ghostboard overlay crop;
- keyboard scenario: enter Browse mode, click inside the overlay, require
  Surfari `mouse-event` trace evidence, prove the click did not break visible
  PDF pixels, send real `PageDown` and `PageUp` CGEvents through the TermSurf
  path, require Surfari `key-event` trace evidence, require
  `snapshot-layer-refresh`, and prove green-to-magenta then magenta-to-green
  visible deltas.

Key summary values:

- scroll: pass;
- click: pass;
- click-to-keyboard route: pass;
- PageDown: pass;
- PageUp: pass;
- cleanup: pass;
- scroll green drop: 621,860 pixels;
- scroll magenta rise: 371,700 pixels;
- PageDown green drop: 509,176 pixels;
- PageDown magenta rise: 140,998 pixels;
- PageUp magenta drop: 140,998 pixels;
- PageUp green rise: 149,864 pixels.

Build and hygiene checks:

```bash
./surfari/libtermsurf_webkit/build.sh
cargo fmt -p surfari
cargo build -p surfari
cargo build -p webtui
(cd ghostboard && macos/build.nu --configuration Debug --action build)
bash -n scripts/test-issue-834-surfari-pdf-input-navigation.sh
git diff --check
git -C webkit/src status --short
rm -rf logs/issue-834-exp42-surfari-pdf-input-navigation
env -u TERMSURF_SURFARI_CACONTEXT_LAYER \
  scripts/test-issue-834-surfari-pdf-input-navigation.sh
```

All checks passed. The WebKit C wrapper build emitted only the existing macOS
SDK/WebKit version warning. The Ghostboard build emitted the existing SwiftLint
warning in `SurfaceView_AppKit.swift`.

## Conclusion

Surfari PDF input navigation is now proven for the first interactive PDF slice:
real TermSurf-routed scroll wheel input, mouse click delivery, `PageDown`, and
`PageUp` all reach Surfari and produce the expected visible PDF behavior in the
Ghostboard overlay. The next Surfari PDF experiment can move to higher-level PDF
workflows such as links, find/search, toolbar controls, restrictions, forms,
print, annotations, context menus, or accessibility classification.

## Completion Review

An external Codex review checked the completed experiment.

Initial verdict: **Changes required**.

Findings:

- click/focus proof overclaimed the click's role because the first harness
  focused Surfari before the click and only checked mouse-event delivery plus
  preserved pixels;
- the JSON summary did not include the relevant matched Ghostboard, WebTUI,
  Surfari, and fixture-server evidence lines required by the design;
- the completion review itself still needed to be recorded before the result
  commit.

Resolution:

- click now records a separate `click_keyboard_route_status`, which passes only
  after a subsequent `PageDown` key event reaches Surfari after the click;
- each scenario JSON now records matched evidence lines for BrowserReady, WebTUI
  ready state, PDF request/content type, CAContext, render proof, Browse mode,
  focus, scroll/click/key input, and snapshot refreshes;
- the summary click status now requires both click delivery/visibility and the
  click-to-keyboard route proof;
- the harness was rerun after the fix and passed as run `20260622-210703`.

Follow-up verdict: **Approved**.

The reviewer found no remaining required findings. The remaining caveat is that
the harness proves click delivery, preserved visibility, and post-click keyboard
routing rather than proving that the click alone newly created focus; the result
language is intentionally scoped to that evidence.
