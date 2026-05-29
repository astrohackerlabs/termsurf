# Experiment 10: Register PDF Viewer Mojo Binders

## Description

Experiment 9 solved the PDF extension activation gate. The direct PDF extension
smoke now reaches a blessed extension context, binds `chrome.pdfViewerPrivate`,
and then fails at the next missing Chromium embedder layer:

```text
Terminating render process for bad Mojo message: Received bad user message: No binder found for interface help_bubble.mojom.PdfHelpBubbleHandlerFactory for the frame/document scope
```

Electron and Chrome both register PDF viewer Mojo binders in the embedder:

- `help_bubble::mojom::PdfHelpBubbleHandlerFactory` as a frame binder;
- `pdf::mojom::PdfHost` as a render-frame-host associated interface, backed by
  `pdf::PDFDocumentHelper`.

Experiment 10 adds only that binder layer, with PDF-id diagnostics around each
request. It does not add PDF navigation interception, stream management,
guest-view, MimeHandlerView, or a PDF renderer process switch. The expected best
outcome is that the direct PDF extension smoke advances past the help-bubble bad
Mojo message and either reaches the next missing binder or continues far enough
to prove the next real layer.

This experiment must receive Claude design review before implementation. After
implementation and result recording, Claude must review the completed output
before any next experiment is designed.

## Changes

1. Create the Chromium implementation branch.

   Start from the accepted Experiment 9 branch:

   ```bash
   git -C chromium/src checkout 148.0.7778.97-issue-792-exp9
   git -C chromium/src checkout -b 148.0.7778.97-issue-792-exp10
   ```

   Add the branch to `chromium/README.md` only after the branch builds and the
   result is accepted.

2. Add a TermSurf PDF document helper client.

   Add a small TermSurf-owned equivalent of Electron's
   `ElectronPDFDocumentHelperClient` under `content/libtermsurf_chromium/`.

   The client should derive from `pdf::PDFDocumentHelperClient` and keep methods
   no-op unless this experiment's smoke proves a method is needed to advance the
   viewer. It may log low-volume PDF-only diagnostics such as:

   ```text
   [issue-792-exp10] pdf-document-load-complete
   [issue-792-exp10] pdf-content-restrictions restrictions=<n>
   [issue-792-exp10] pdf-plugin-can-save can_save=<0|1>
   ```

   Do not depend on Chrome's `ChromePDFDocumentHelperClient`; that class pulls
   in Chrome UI/profile behavior that TermSurf does not own. The Electron model
   is the right shape: embedder-owned helper client, canonical Chromium
   `PDFDocumentHelper`.

3. Register `pdf::mojom::PdfHost`.

   Override
   `TsBrowserClient::RegisterAssociatedInterfaceBindersForRenderFrameHost()`.

   Requirements:
   - call `ShellContentBrowserClient`'s implementation first if the base method
     is non-empty on Chromium 148;
   - register `pdf::mojom::PdfHost` through
     `pdf::PDFDocumentHelper::BindPdfHost()`;
   - pass a new `TsPDFDocumentHelperClient`;
   - follow Electron's universal-registration pattern rather than trying to
     pre-gate registration by extension id.

   The interface is only requested by frames hosting the PDF viewer. Log on
   invocation, not registration, to prove which frame exercises it.

   Required log, at most once per requesting frame:

   ```text
   [issue-792-exp10] pdf-host-binder frame_url=<url> site_url=<url>
   ```

   If the interface is never requested during the direct extension smoke, record
   that explicitly; do not remove the binder just because the first run does not
   need it.

4. Register `help_bubble::mojom::PdfHelpBubbleHandlerFactory`.

   Do not import `chrome/browser/pdf/pdf_help_bubble_handler_factory.h` or
   depend on `//chrome/browser/pdf`. Chrome's factory exists to drive Chrome's
   PDF help-bubble UI, which TermSurf does not have. Pulling that target into
   `libtermsurf_chromium` violates the prior Issue 792 dependency discipline.

   Instead, implement a TermSurf-owned no-op factory directly under
   `content/libtermsurf_chromium/`. It must:
   - implement `help_bubble::mojom::PdfHelpBubbleHandlerFactory`;
   - use `mojo::MakeSelfOwnedReceiver` or an equivalent self-owned receiver
     lifetime;
   - accept `CreateHelpBubbleHandler(...)` and immediately drop/no-op the
     handler receiver;
   - be gated to PDF extension frames so arbitrary web pages cannot exercise the
     interface.

   In `TsBrowserClient::RegisterBrowserInterfaceBindersForFrame()`, register the
   TermSurf-owned factory. Log on invocation, not registration, to prove that
   the PDF viewer frame requested it.

   Required log, at most once per requesting frame:

   ```text
   [issue-792-exp10] pdf-help-bubble-binder frame_url=<url> site_url=<url>
   ```

5. Keep dependencies explicit.

   Allowed dependencies for this experiment:
   - `//components/pdf/browser`
   - `//ui/webui/resources/cr_components/help_bubble:mojo_bindings`
   - the narrow Mojo/base/content targets required by the new TermSurf-owned
     helper classes

   Forbidden dependencies:
   - `//chrome/browser/pdf`
   - `//chrome/browser/pdf:pdf`
   - `//chrome/browser/ui`
   - broad `//chrome/browser/*` UI/profile targets

   If one of the allowed targets has an unexpected transitive blocker, record
   the exact GN or compile error and stop rather than importing the forbidden
   Chrome target.

6. Diagnose the next requested PDF viewer Mojo interface.

   Keep Experiment 9's direct extension smoke and inspect logs for any new "No
   binder found for interface ..." message after the help-bubble binder is
   installed.

   If a new missing binder appears, do not add it in this experiment unless it
   is already part of the two-binder Electron/Chrome pair above. Record it as
   the next gate for Experiment 11.

   If no new Mojo binder error appears and the viewer instead reaches a
   JavaScript API-function failure such as `pdfViewerPrivate.getStreamInfo` not
   being implemented, record that as the next gate. That would mean the work has
   moved from binder registration to the Electron-style PDF stream handoff.

7. Build and archive only after verification.

   Build:

   ```bash
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   git -C chromium/src cl format --upstream=148.0.7778.97-issue-792-exp9 --full
   autoninja -C chromium/src/out/Default libtermsurf_chromium
   ```

   If the branch builds and verification passes or produces a useful Partial, do
   the full bookkeeping after Claude after-review accepts the result:
   - commit the Chromium branch;
   - regenerate `chromium/patches/issue-792/`;
   - add the new branch row to `chromium/README.md`;
   - update Experiment 10's line in `issues/0792-pdf-support/README.md` from
     `Designed` to the final status.

## Verification

1. Confirm starting state.

   ```bash
   git status --short
   git -C chromium/src status --short
   git -C chromium/src branch --show-current
   ```

   Chromium should start clean on `148.0.7778.97-issue-792-exp9`.

2. Build the branch.

   ```bash
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   git -C chromium/src cl format --upstream=148.0.7778.97-issue-792-exp9 --full
   autoninja -C chromium/src/out/Default libtermsurf_chromium
   ```

3. Run the direct PDF extension smoke.

   Reuse the debug screenshot harness against:

   ```text
   chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/index.html
   ```

   Required evidence:
   - Experiment 9's activation success still appears:

     ```text
     [issue-792-exp9] renderer-activate-extension ... active=1
     [issue-792-exp9] pdf-script-context ... context=BLESSED_EXTENSION ... pdfViewerPrivate_available=1
     [issue-792-exp8] schema-request name=pdfViewerPrivate found=1
     ```

   - Experiment 10's binder logs appear:

     ```text
     [issue-792-exp10] pdf-help-bubble-binder ...
     ```

   - The previous
     `No binder found for interface help_bubble.mojom.PdfHelpBubbleHandlerFactory`
     message is gone.

   - If `pdf::mojom::PdfHost` is requested, the run also shows:

     ```text
     [issue-792-exp10] pdf-host-binder ...
     ```

4. Run normal HTML regression smoke.

   Load:

   ```text
   http://localhost:9616/index.html
   ```

   Pass requires the page to render or lifecycle logs to reach `TitleChanged`
   and `LoadingState`, with no extension IPC crash.

5. Run the PDF unchanged smoke.

   Load:

   ```text
   http://localhost:9616/bitcoin.pdf
   ```

   Unless this experiment unexpectedly changes only direct PDF extension
   behavior enough to alter navigation, direct PDF navigation is still expected
   to follow the content_shell download path. A browser crash, renderer IPC
   crash, or hang is a failure.

## Pass Criteria

The experiment passes if:

- `libtermsurf_chromium` builds;
- the direct PDF extension smoke no longer terminates on the missing
  `PdfHelpBubbleHandlerFactory` binder;
- Experiment 9's private API availability remains intact;
- the result records the next observed PDF viewer gate, if any;
- no forbidden `//chrome/browser/pdf` or broad Chrome browser UI dependency is
  added;
- HTML and unchanged PDF regression smokes do not crash or hang before artifact
  capture.

## Partial Criteria

The experiment is Partial if it builds and proves the binder layer shape, but
the viewer stops at a new nearby browser-side layer before enough evidence
exists to decide the next implementation slice. Examples:

- the TermSurf-owned help-bubble factory cannot be built against the mojom-only
  help-bubble target without a separate dependency design;
- `PdfHost` cannot be bound without additional `PDFDocumentHelper` dependencies;
- the help-bubble bad Mojo message is gone, but a new missing binder appears
  before the viewer can continue;
- the help-bubble bad Mojo message is gone, but the next failure is a missing
  PDF stream/API implementation rather than another binder;
- the direct smoke reaches a new crash whose stack identifies the next layer.

Every Partial result must record the exact blocker and the next experiment's
target.

## Failure Criteria

The experiment fails if:

- it exposes PDF viewer binders to arbitrary web pages instead of keeping them
  PDF-viewer-scoped;
- it implements PDF navigation interception, streams, guest-view,
  MimeHandlerView, `PdfViewerStreamManager`, or `--pdf-renderer`;
- it adds `//chrome/browser/pdf`, `//chrome/browser/pdf:pdf`,
  `//chrome/browser/ui`, or broad Chrome browser UI stacks instead of the two
  targeted PDF viewer binders;
- it globally suppresses bad Mojo messages;
- it removes or weakens Experiment 9's extension activation fix;
- ordinary HTML pages crash, hang, or lose normal lifecycle messages;
- direct PDF navigation regresses into a crash, hang, or renderer IPC failure;
- the build cannot complete.
