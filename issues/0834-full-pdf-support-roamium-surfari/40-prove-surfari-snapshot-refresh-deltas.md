# Experiment 40: Prove Surfari Snapshot Refresh Deltas

## Description

Experiment 39 made Surfari's snapshot-backed `CAContext` layer the default and
proved that HTML and PDF overlays are visible in the real Ghostboard window
without setting `TERMSURF_SURFARI_CACONTEXT_LAYER=snapshot`. It stopped at a
Partial result because the harness did not prove that visible hosted pixels
change after interaction or resize.

This experiment should close that evidence gap. The goal is not to add new PDF
features yet; it is to prove that Surfari's default hosted snapshot layer
continues to update when the user-visible browser viewport changes.

## Changes

- Add a focused harness, tentatively
  `scripts/test-issue-834-surfari-snapshot-refresh-deltas.sh`.
- Reuse repo-built Ghostboard, WebTUI, Surfari, and WebKit artifacts. Do not use
  installed `web`, installed Surfari/Roamium artifacts, or Homebrew-installed
  app bundles.
- Run with `TERMSURF_SURFARI_CACONTEXT_LAYER` unset so the test proves the
  default Surfari presentation path.
- Serve deterministic fixtures whose visible pixels make deltas easy to prove:
  - an HTML fixture with a scrollable top region and a visually distinct lower
    region as a control;
  - a PDF fixture with a visually distinct first page/region and second
    page/region. A full Pass requires the PDF fixture; HTML-only proof is
    Partial.
- Launch Ghostboard, run the repo-built `web --browser <repo surfari>` path, and
  wait for Surfari internal render proof plus Ghostboard-window visible proof.
- Capture a pre-interaction Ghostboard-window screenshot bounded to the overlay
  region.
- Send a real input event through the existing TermSurf path. Prefer a scroll
  wheel event because it exercises the PDF viewport without relying on text
  focus. If direct CGEvent delivery is unreliable in the VM, record the exact
  failure and fall back only to another real TermSurf-routed input path, not to
  mutating Surfari internals directly.
- Wait for Surfari's app log to record a matching refresh reason, such as
  `scroll`, `mouse-event`, `mouse-drag`, or `key-event`.
- Capture a post-interaction Ghostboard-window screenshot of the same overlay
  region and require the target pixel counts to change in the expected
  direction. The harness should define fixture-specific target colors and use
  explicit thresholds:
  - pre-interaction dominant target color count is at least 5,000 pixels inside
    the overlay crop;
  - post-interaction dominant target color count is at least 5,000 pixels inside
    the same logical overlay crop;
  - the pre-dominant color decreases by at least 5,000 pixels or 20% of the
    overlay crop, whichever is smaller;
  - the post-dominant color increases by at least 5,000 pixels or 20% of the
    overlay crop, whichever is smaller;
  - full-window or source/helper-window pixels cannot satisfy these counts.
- Trigger a deterministic resize of the Ghostboard window or pane, wait for
  updated `presented_pixels`/`Resize` evidence, and capture the post-resize
  Ghostboard-window overlay at the new dimensions.
- Record a JSON summary with:
  - env state proving `TERMSURF_SURFARI_CACONTEXT_LAYER` was unset;
  - repo binary paths;
  - pre/post interaction screenshot paths and target pixel counts;
  - input method used;
  - refresh reasons observed;
  - resize method used;
  - pre/post `Resize`, `CaContext`, and `presented_pixels` evidence;
  - pre/post resize overlay frame, crop rectangle, screenshot dimensions, and
    CAContext pixel dimensions;
  - cleanup status for Ghostboard, Surfari, WebTUI, and fixture servers.
- Update this experiment file with the result.

## Verification

Run build and hygiene checks:

```bash
./surfari/libtermsurf_webkit/build.sh
cargo fmt -p surfari
cargo build -p surfari
cargo build -p webtui
(cd ghostboard && macos/build.nu --configuration Debug --action build)
bash -n scripts/test-issue-834-surfari-snapshot-refresh-deltas.sh
git diff --check
git -C webkit/src status --short
```

Run the refresh-delta harness:

```bash
rm -rf logs/issue-834-exp40-surfari-snapshot-refresh-deltas
env -u TERMSURF_SURFARI_CACONTEXT_LAYER \
  scripts/test-issue-834-surfari-snapshot-refresh-deltas.sh
```

Pass criteria:

- the harness runs with `TERMSURF_SURFARI_CACONTEXT_LAYER` unset;
- the harness records repo-built Ghostboard, WebTUI, Surfari, and WebKit paths;
- Surfari internal render proof passes before interaction;
- Ghostboard-window visible proof passes before interaction;
- the visible proof is bounded to the Ghostboard app window and overlay crop;
- source/helper-window pixels are excluded and cannot satisfy the proof;
- the input event is a real TermSurf-routed user input event, not a direct
  internal snapshot mutation;
- the app log records the expected Surfari snapshot refresh reason after input;
- PDF post-input Ghostboard-window overlay pixels differ from PDF pre-input
  pixels according to the fixture-specific target colors and threshold rules
  above;
- HTML post-input delta is also recorded as a control, but HTML-only delta does
  not satisfy full Pass;
- resize method is recorded;
- resize produces correlated pre/post `Resize`, `CaContext`, and
  `presented_pixels` evidence;
- resize produces different overlay frame, crop rectangle, screenshot
  dimensions, or CAContext pixel dimensions;
- post-resize Ghostboard-window visible proof is captured from the new overlay
  crop and source/helper-window pixels are excluded;
- cleanup leaves no running Ghostboard, Surfari, WebTUI, or fixture server
  process;
- `webkit/src` remains clean;
- design review and completion review are recorded.

Partial criteria:

- default refresh works for either interaction or resize, but not both;
- the harness proves a real input event reaches Surfari but cannot get a stable
  pixel delta in the VM;
- the harness proves HTML refresh but PDF refresh needs a follow-up fixture or
  PDF-specific input path.

Failure criteria:

- default Surfari presentation regresses to blank;
- the harness requires `TERMSURF_SURFARI_CACONTEXT_LAYER=snapshot`;
- the harness can only pass by directly mutating Surfari internals;
- input automation cannot be delivered or observed;
- resize evidence cannot be captured;
- cleanup leaves running processes.

## Design Review

An external Codex review checked the design.

Initial verdict: **Changes required**.

Findings:

- full Pass could be overclaimed with HTML-only refresh evidence even though the
  missing evidence is needed for the Surfari PDF matrix;
- "pixels differ in the expected direction" needed explicit target colors,
  bounds, thresholds, and tolerance;
- the pass criteria needed to explicitly carry forward the Ghostboard
  app-window/overlay-bound proof and source/helper-window exclusion from
  Experiments 38 and 39;
- resize proof needed to record the resize method plus correlated `Resize`,
  `CaContext`, `presented_pixels`, overlay frame, crop, screenshot, and
  CAContext dimension evidence.

Resolution:

- full Pass now requires a PDF before/after visible pixel delta; HTML-only proof
  is Partial;
- the design now requires fixture-specific target colors with minimum counts and
  minimum delta thresholds inside the overlay crop;
- the design explicitly excludes source/helper-window pixels from satisfying
  visible proof;
- resize evidence now requires the method and correlated pre/post protocol,
  AppKit, crop, screenshot, and CAContext dimensions.

Follow-up verdict: **Approved**.

The reviewer found no remaining required design changes before the plan commit
and implementation.
