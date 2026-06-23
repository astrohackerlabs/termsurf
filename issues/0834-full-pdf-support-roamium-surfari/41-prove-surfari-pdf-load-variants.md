# Experiment 41: Prove Surfari PDF Load Variants

## Description

Experiments 39 and 40 established that Surfari's default snapshot-backed
presentation is visible, refreshes after real scroll input, and remains visible
after pane resize. The next PDF matrix slice should prove that Surfari can load
PDFs through the common URL and embedding paths before testing higher-level PDF
features such as links, find, toolbar controls, forms, and restrictions.

This experiment should test Surfari only. Roamium already has regression guards
for the corresponding core workflows; this step is about bringing Surfari's
basic PDF loading surface up to the same evidence level.

## Changes

- Add a focused harness, tentatively
  `scripts/test-issue-834-surfari-pdf-load-variants.sh`.
- Generate a deterministic two-page PDF fixture with high-contrast page colors,
  similar to Experiment 40's fixture, so visible proof does not depend on text
  rendering or antialiasing.
- Serve the same PDF through at least these paths:
  - full-page HTTP PDF with a `.pdf` extension and `application/pdf`;
  - extensionless HTTP PDF with `application/pdf`;
  - local `file://` PDF;
  - embedded PDF in an `iframe`;
  - embedded PDF in an `embed`;
  - embedded PDF in an `object`.
- Treat the three embedded forms as separate rows. A full Pass requires all
  three embedded forms to prove PDF pixels. If one embedded form fails while
  another passes, record the exact form-level result and classify the experiment
  Partial.
- For each scenario, launch repo-built Ghostboard with repo-built WebTUI and
  repo-built Surfari. Do not use installed artifacts.
- Run with `TERMSURF_SURFARI_CACONTEXT_LAYER` unset.
- For each scenario, require:
  - Surfari internal render proof tied to that scenario's URL/path;
  - fixture-specific PDF page color counts in the internal render proof where
    the top-level capture can see the PDF content;
  - nonzero CAContext;
  - Ghostboard-window overlay-cropped visible pixel proof using colors that
    occur only inside the PDF page content;
  - source/helper-window exclusion;
  - cleanup for Ghostboard, Surfari, WebTUI, and fixture servers.
- Use wrapper HTML colors that cannot satisfy the PDF target-color proof for the
  embedded scenarios. The wrapper may use neutral gray/white, but the PDF pages
  should use distinctive target colors such as green and magenta.
- Record a JSON summary with one row per scenario:
  - URL/path;
  - load evidence:
    - HTTP `.pdf` request path and `application/pdf` content type;
    - extensionless HTTP request path and `application/pdf` content type;
    - local `file://` URL observed by Surfari;
    - embedded HTML request plus embedded PDF resource request and content type;
  - repo binary paths;
  - scenario-specific internal render proof;
  - visible overlay crop counts;
  - pass/partial/fail classification;
  - cleanup status.
- Update this experiment file with the result.

## Verification

Run build and hygiene checks:

```bash
./surfari/libtermsurf_webkit/build.sh
cargo fmt -p surfari
cargo build -p surfari
cargo build -p webtui
(cd ghostboard && macos/build.nu --configuration Debug --action build)
bash -n scripts/test-issue-834-surfari-pdf-load-variants.sh
git diff --check
git -C webkit/src status --short
```

Run the load-variants harness:

```bash
rm -rf logs/issue-834-exp41-surfari-pdf-load-variants
env -u TERMSURF_SURFARI_CACONTEXT_LAYER \
  scripts/test-issue-834-surfari-pdf-load-variants.sh
```

Pass criteria:

- `TERMSURF_SURFARI_CACONTEXT_LAYER` is unset;
- all scenarios use repo-built Ghostboard, WebTUI, Surfari, and WebKit
  artifacts;
- full-page HTTP `.pdf` loads and is visible in the Ghostboard overlay crop;
- extensionless HTTP PDF loads and is visible in the Ghostboard overlay crop;
- local `file://` PDF loads and is visible in the Ghostboard overlay crop;
- embedded `iframe` PDF loads and is visible in the Ghostboard overlay crop;
- embedded `embed` PDF loads and is visible in the Ghostboard overlay crop;
- embedded `object` PDF loads and is visible in the Ghostboard overlay crop;
- each HTTP scenario records the expected request path and `application/pdf`
  content type;
- the local-file scenario records the `file://` URL observed by Surfari;
- each embedded scenario records both the wrapper HTML request and the embedded
  PDF resource request with `application/pdf`;
- each scenario records a passing Surfari internal render proof tied to that
  scenario's URL/path;
- visible and internal pixel proof uses fixture-specific PDF page target colors
  that cannot be supplied by the HTML wrapper;
- visible proof is bounded to the Ghostboard app window and overlay crop;
- source/helper-window pixels cannot satisfy the proof;
- cleanup leaves no running Ghostboard, Surfari, WebTUI, or fixture server
  process;
- `webkit/src` remains clean;
- design review and completion review are recorded.

Partial criteria:

- at least one variant passes, but another variant exposes a Surfari or harness
  gap;
- at least one embedded form passes but another embedded form fails;
- local `file://` loading needs a Surfari-specific access fix.

Failure criteria:

- Surfari PDF visibility regresses for the already-proven HTTP `.pdf` case;
- the harness only passes with `TERMSURF_SURFARI_CACONTEXT_LAYER=snapshot`;
- visible proof can be satisfied by a helper/source window;
- cleanup leaves running processes.

## Design Review

An external Codex review checked the design.

Initial verdict: **Changes required**.

Findings:

- embedded PDF proof could be overclaimed because the design allowed the
  implementation to choose whichever embedding form worked;
- visible proof needed to be explicitly tied to PDF page pixels, not wrapper
  HTML, placeholders, or plugin frames;
- pass criteria needed explicit per-scenario load evidence, including content
  type for HTTP resources and observed `file://` URL for local files;
- internal render proof needed to be scenario-specific and content-specific.

Resolution:

- `iframe`, `embed`, and `object` are now separate scenario rows, and full Pass
  requires all three;
- visible proof and internal proof must use fixture-specific PDF page colors
  that wrapper HTML cannot satisfy;
- pass criteria now require explicit request/content-type evidence for HTTP and
  embedded resources plus observed local-file URL evidence;
- internal render proof must be tied to each scenario's URL/path.

Follow-up verdict: **Approved**.

The reviewer found no remaining required design changes before the plan commit
and implementation.

## Result

**Result:** Pass.

Added `scripts/test-issue-834-surfari-pdf-load-variants.sh` and ran it against
repo-built Ghostboard, WebTUI, Surfari, and WebKit artifacts with
`TERMSURF_SURFARI_CACONTEXT_LAYER` unset.

The verification run `20260622-204730` produced
`logs/issue-834-exp41-surfari-pdf-load-variants/surfari-pdf-load-variants-summary.json`
with `overall_result = pass` and classification
`surfari-pdf-load-variants-proven`.

All six load variants passed:

- full-page HTTP `.pdf`;
- extensionless HTTP PDF with `application/pdf`;
- local `file://` PDF;
- embedded `iframe` PDF;
- embedded `embed` PDF;
- embedded `object` PDF.

For each scenario, the harness required the WebTUI Surfari launch request,
`BrowserReady`, WebTUI readiness, a Surfari trace for the scenario URL, nonzero
CAContext, scenario-specific internal render proof, AppKit-presented overlay
pixels, and visible Ghostboard overlay-cropped PDF page-color proof. The HTTP
and embedded scenarios also required request evidence with `application/pdf`.
The local-file scenario required the observed `file://` URL. The embedded
scenarios used form-specific PDF resource paths (`/iframe-fixture.pdf`,
`/embed-fixture.pdf`, and `/object-fixture.pdf`) so each embedded form had to
prove its own PDF request. The wrapper HTML used neutral colors, while the PDF
pages used green and magenta target colors, so wrapper pixels could not satisfy
the PDF visibility proof.

The summary recorded cleanup as successful: all scenario Ghostboard processes
were terminated and the fixture server was terminated.

Build and hygiene checks:

```bash
./surfari/libtermsurf_webkit/build.sh
cargo fmt -p surfari
cargo build -p surfari
cargo build -p webtui
(cd ghostboard && macos/build.nu --configuration Debug --action build)
bash -n scripts/test-issue-834-surfari-pdf-load-variants.sh
git diff --check
git -C webkit/src status --short
```

All checks passed. The WebKit C wrapper build emitted only the existing macOS
SDK/WebKit version warning. The Ghostboard build emitted the existing SwiftLint
warning in `SurfaceView_AppKit.swift`.

## Conclusion

Surfari's basic PDF load surface is now proven in the real TermSurf app for
full-page HTTP PDFs, extensionless HTTP PDFs, local file PDFs, and embedded PDFs
through `iframe`, `embed`, and `object`. The proof is tied to PDF page pixels
presented inside the Ghostboard overlay, not to helper windows or wrapper HTML.

The next Surfari PDF experiment can move beyond loading and visibility into
interactive PDF behavior such as scroll/key navigation, links, find/search,
toolbar controls, restrictions, forms, print, annotations, or context menus.

## Completion Review

An external Codex review checked the completed experiment.

Initial verdict: **Changes required**.

Findings:

- embedded PDF request evidence was not scenario-specific because the first
  implementation reused `/fixture.pdf` for every embedded form, allowing an
  earlier full-page request to satisfy later embedded request checks;
- the completion review itself still needed to be recorded before the result
  commit.

Resolution:

- the harness now gives each embedded form a distinct PDF resource path:
  `/iframe-fixture.pdf`, `/embed-fixture.pdf`, and `/object-fixture.pdf`;
- the fixture server serves those paths as `application/pdf`, and each embedded
  scenario records its wrapper path and PDF path in its scenario JSON;
- the harness was rerun after the fix and passed as run `20260622-204730`.

Follow-up verdict: **Approved**.

The reviewer found no remaining required findings and confirmed that the
scenario-specific embedded PDF request evidence and completion-review recording
issues were resolved.
