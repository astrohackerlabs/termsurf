# Experiment 38: Diagnose Surfari CAContext Hosting

## Description

Experiment 37 proved that Surfari's `WKWebView` renders both the HTML control
and the PDF fixture internally. `WKWebView.takeSnapshot` captured the expected
HTML colors and WebKit green PDF page. Ghostboard-visible window and full-screen
captures remained blank even though Surfari exported a nonzero `CAContext` and
Ghostboard/AppKit logged overlay presentation.

The failure layer is therefore between Surfari's rendered WebKit layer and
Ghostboard-visible composition. This experiment should diagnose and, if a small
fix is clear, fix the `CAContext`/`CALayerHost` hosting path.

## Changes

- Inspect the current Surfari export path:
  - `CAContext.remoteContextWithOptions`;
  - `contents->remote_context.layer = contents->web_view.layer`;
  - source `TSHostWindow` visibility/alpha/order;
  - `WKWebView` layer hierarchy and bounds at export time.
- Inspect the current Ghostboard host path:
  - where `CALayerHost` is created;
  - how `contextId` is assigned;
  - host layer frame, opacity, z-order, masking, and lifecycle;
  - whether the layer is under or over terminal render layers.
- Add targeted diagnostics before changing behavior:
  - Surfari trace lines for source window alpha, ordered/visible state, web view
    bounds, layer bounds, layer hidden/opacity, and remote context id;
  - Ghostboard trace lines for `CALayerHost` frame, bounds, hidden/opacity,
    superlayer, and sublayer order.
- Test small candidate fixes one at a time, with each candidate individually
  recorded in the result:
  - wrapping the `WKWebView` layer in a dedicated root `CALayer` before
    assigning it to `CAContext.layer`;
  - keeping the source host window ordered but transparent vs minimally visible;
  - forcing layer layout/display before exporting the context;
  - adjusting Ghostboard host layer ordering or opacity if diagnostics show it
    is hidden/behind terminal layers.
- Reuse the Experiment 37 harness or add a focused harness, tentatively
  `scripts/test-issue-834-surfari-cacontext-hosting.sh`, that runs the HTML and
  PDF controls and requires:
  - Surfari internal render proof still passes;
  - Ghostboard app-window overlay pixel proof changes from fail to pass for HTML
    and PDF;
  - the visible proof is measured inside the Ghostboard app window/overlay
    bounds, not from arbitrary full-screen pixels;
  - any visible Surfari source/helper window pixels are explicitly excluded from
    the proof;
  - no regression in CAContext/presented-pixels logs.
- The harness summary must record the exact Ghostboard app binary path and build
  artifact used.
- Update this experiment file with the result.

## Verification

Run formatting/build/hygiene checks:

```bash
./surfari/libtermsurf_webkit/build.sh
cargo fmt -p surfari
cargo build -p surfari
cd ghostboard && macos/build.nu --configuration Debug --action build && cd ..
bash -n scripts/test-issue-834-surfari-cacontext-hosting.sh
git diff --check
git -C webkit/src status --short
```

If the experiment does not touch Ghostboard source, the Ghostboard rebuild may
be skipped, but the result must explicitly state that no Ghostboard files were
modified and record the existing app artifact used by the harness.

Run the focused harness:

```bash
rm -rf logs/issue-834-exp38-surfari-cacontext-hosting
scripts/test-issue-834-surfari-cacontext-hosting.sh
```

Inspect the summary:

```bash
python3 - <<'PY'
import json
from pathlib import Path

summary = json.loads(
    Path(
        "logs/issue-834-exp38-surfari-cacontext-hosting/"
        "surfari-cacontext-hosting-summary.json"
    ).read_text()
)
print(json.dumps({
    "overall_result": summary.get("overall_result"),
    "classification": summary.get("classification"),
    "candidate": summary.get("candidate"),
    "html_internal": summary.get("html", {}).get("internal_render"),
    "html_visible": summary.get("html", {}).get("visible_pixel_proof"),
    "pdf_internal": summary.get("pdf", {}).get("internal_render"),
    "pdf_visible": summary.get("pdf", {}).get("visible_pixel_proof"),
    "cleanup": summary.get("cleanup"),
}, indent=2, sort_keys=True))
PY
```

Pass criteria:

- repo-built Surfari/libtermsurf_webkit/Ghostboard/WebTUI/WebKit artifacts are
  used;
- Surfari internal render proof passes for HTML and PDF;
- Ghostboard app-window overlay pixel proof passes for HTML and PDF;
- the visible proof is bounded to the Ghostboard overlay/window and cannot be
  satisfied by a Surfari source/helper window elsewhere on screen;
- the summary names the specific fix that made hosted WebKit pixels visible;
- the summary records the Ghostboard app artifact path used;
- CAContext and AppKit presentation logs remain nonzero and consistent;
- cleanup leaves no running app/browser/server process;
- `webkit/src` remains clean;
- design review and completion review are recorded.

Partial criteria:

- diagnostics identify a likely failing layer but no fix is implemented yet;
- or one candidate makes HTML visible but PDF remains blank;
- or a fix works visually but introduces a cleanup, geometry, or lifecycle issue
  that requires a follow-up experiment.

Failure criteria:

- internal Surfari render proof regresses;
- the harness passes without visible pixel proof;
- product code changes are broad or unrelated to CAContext hosting;
- installed artifacts are used instead of repo-built artifacts;
- cleanup leaves running processes.

## Design Review

An external Codex review checked the design.

Initial verdict: **Changes required**.

Findings:

- verification did not require rebuilding Ghostboard even though the design
  allows Ghostboard host-layer changes;
- visible-pixel pass criteria could be fooled by a minimally visible Surfari
  source/helper window, because the plan did not explicitly require the proof to
  be bounded to the Ghostboard app window/overlay.

Resolution:

- verification now includes a Debug Ghostboard rebuild when Ghostboard files are
  touched, and requires the result to record the app artifact used;
- pass criteria now require Ghostboard app-window overlay pixel proof, bounded
  to the Ghostboard overlay/window, and explicitly exclude source/helper window
  pixels.

Follow-up verdict after fixes: **Approved**.

The reviewer found no remaining required design changes before the plan commit.

## Result

**Result:** Pass

The baseline `WKWebView.layer` export still reproduces the Ghostboard-visible
blank:

- run: `20260622-194729`;
- candidate: `baseline`;
- harness log:
  `logs/issue-834-exp38-surfari-cacontext-hosting/harness-20260622-194729.log`;
- classification: `candidate-did-not-fix-hosting`;
- Surfari internal render proof passed for HTML and PDF;
- Ghostboard app-window visible pixel proof failed for HTML and PDF.

The diagnostic local-layer candidate proved the Ghostboard `CAContext` /
`CALayerHost` path itself can host a Surfari-exported local layer. The failure
is therefore specific to using `WKWebView.layer` as the remote context layer,
not to Ghostboard's host layer plumbing.

The passing candidate is `snapshot-layer`. Surfari keeps the hidden live
`WKWebView` for navigation, input, and WebKit rendering, but exports a local
`CALayer` through `CAContext` and refreshes that layer from
`WKWebView.takeSnapshot` after export, resize, and navigation completion.

Final proof:

- run: `20260622-194754`;
- candidate: `snapshot-layer`;
- summary:
  `logs/issue-834-exp38-surfari-cacontext-hosting/surfari-cacontext-hosting-summary.json`;
- harness log:
  `logs/issue-834-exp38-surfari-cacontext-hosting/harness-20260622-194754.log`;
- Ghostboard app binary:
  `ghostboard/macos/build/Debug/TermSurf.app/Contents/MacOS/termsurf`;
- classification: `cacontext-hosting-fixed`;
- overall result: `pass`;
- HTML internal render: `pass`;
- PDF internal render: `pass`;
- HTML Ghostboard-window visible pixel proof: `pass`;
- PDF Ghostboard-window visible pixel proof: `pass`;
- cleanup: HTML process terminated, PDF process terminated, server terminated;
- `webkit/src` remained clean.

Verification commands run:

```bash
./surfari/libtermsurf_webkit/build.sh
cargo fmt -p surfari
cargo build -p surfari
(cd ghostboard && macos/build.nu --configuration Debug --action build)
bash -n scripts/test-issue-834-surfari-cacontext-hosting.sh
git diff --check
git -C webkit/src status --short
rm -rf logs/issue-834-exp38-surfari-cacontext-hosting \
  logs/issue-834-exp37-surfari-side-render-pixels \
  logs/issue-834-exp36-surfari-visual-compositing
TERMSURF_SURFARI_CACONTEXT_CANDIDATE=baseline \
  scripts/test-issue-834-surfari-cacontext-hosting.sh
TERMSURF_SURFARI_CACONTEXT_CANDIDATE=snapshot-layer \
  scripts/test-issue-834-surfari-cacontext-hosting.sh
```

## Conclusion

Surfari's public `WKWebView` layer tree is not directly hostable by Ghostboard's
existing `CALayerHost` path, even though WebKit renders the page and the
CAContext ID is nonzero. A Surfari-owned local CAContext layer is hostable, and
a snapshot-backed layer makes both HTML and PDF pixels visible in the real
Ghostboard window.

This fixes the immediate Surfari visible-rendering blocker for PDF proof. The
next experiment should make the snapshot-backed path usable as Surfari's normal
TermSurf presentation path, including refreshes after user input and scroll,
then continue the PDF feature matrix from the now-visible Surfari overlay.

## Completion Review

An external Codex review checked the completed experiment result.

Initial verdict: **Changes required**.

Findings:

- the completion review had not yet been recorded in this experiment file;
- the recorded verification block was not directly replayable from the repo root
  because it changed into `ghostboard/` and did not change back before later
  commands;
- the result overclaimed the failed layer by saying `WKWebView`/content-view
  when the recorded baseline evidence only proved `WKWebView.layer` failed.

Resolution:

- this completion review section was added;
- the Ghostboard build command was wrapped in a subshell so the remaining
  verification commands run from the repo root;
- the failure claim was narrowed to `WKWebView.layer`.

Follow-up verdict: **Approved**.

The reviewer found no remaining required changes before the result commit.
