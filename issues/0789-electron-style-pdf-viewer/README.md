+++
status = "open"
opened = "2026-05-27"
+++

# Issue 789: Electron-Style PDF Viewer Infrastructure

## Goal

Make PDFs render inline in Roamium by adding the Electron-style Chromium
embedder infrastructure that the PDF viewer requires.

This issue continues from Issue 776. Issue 776 proved that PDF rendering is not
fixed by a single PDFium plugin toggle, wrapper page, MIME mapping, or direct
link to Chrome's full browser implementation. TermSurf needs its own small
embedder layer that mirrors the pieces Electron provides for Chromium's PDF
viewer path.

## Background

Opening a PDF with `web file.pdf` currently does not render a working inline PDF
viewer. Issue 776 investigated the failure and established several facts:

- Roamium is based on Chromium's `content_shell`-style embedding, so it does not
  automatically inherit Chrome's full PDF viewer feature stack.
- The internal PDF plugin can be registered and created, but that is not enough.
- A wrapper-only approach can load static viewer resources, but it does not
  enter Chromium's real PDF stream / PDF renderer pipeline.
- Chromium can manage the PDF renderer process itself once the embedder enters
  the proper PDF viewer path. TermSurf should not manually spawn or manage a
  separate PDF process.
- Directly linking Chrome's stock `PluginResponseInterceptorURLLoaderThrottle`
  path is too broad for Roamium. Issue 776 Experiment 8 showed that adding
  `//chrome/browser/plugins:impl` pulled in large Chrome product subsystems and
  failed at link time.

The important architectural lesson from Issue 776 is that Electron is the right
model. Electron does not make itself Chrome. It provides Electron-owned glue for
the PDF viewer pieces that Chrome normally owns, then patches Chromium's PDF
stream path to call Electron's implementations.

TermSurf should do the same for Roamium.

## Electron Model

Electron's PDF implementation has several important pieces:

- `ElectronContentClient` registers the internal PDF plugin.
- `RendererClientBase::RenderFrameCreated()` binds
  `MimeHandlerViewContainerManager` in renderer frames.
- `RendererClientBase::IsPluginHandledExternally()` routes `application/pdf`
  through `MimeHandlerViewContainerManager::CreateFrameContainer()`.
- `ElectronBrowserClient::CreateURLLoaderThrottles()` installs
  `PluginResponseInterceptorURLLoaderThrottle`.
- Electron carries a Chromium patch that redirects Chrome's plugin response
  interceptor from Chrome's `streams_private` API to Electron's
  `streams_private` API.
- Electron's `streams_private` implementation receives intercepted PDF streams
  and feeds `PdfViewerStreamManager`.
- Electron serves PDF viewer extension resources with an Electron-owned
  component extension resource manager.
- Electron provides enough `pdf_viewer_private` and `PdfHost` /
  `PDFDocumentHelper` glue for the PDF viewer shell and plugin to run.

The key pattern is ownership: Electron copies or adapts the embedder-facing glue
instead of importing Chrome's whole browser layer.

## TermSurf Direction

TermSurf should add a Roamium-owned PDF viewer embedder layer under
`content/libtermsurf_chromium/` and nearby TermSurf-specific Chromium files.

The target architecture is:

1. Keep the internal PDF plugin registration from Issue 776.
2. Keep the static PDF viewer resource serving from Issue 776 Experiment 7.
3. Replace the failed direct Chrome dependency from Issue 776 Experiment 8 with
   a TermSurf-owned PDF response throttle or a narrow Chromium patch that calls
   TermSurf-owned code.
4. Add a TermSurf `streams_private` equivalent that stores intercepted PDF
   streams in `PdfViewerStreamManager`.
5. Add the renderer-side MimeHandlerView container wiring needed to convert
   `application/pdf` into a PDF viewer frame.
6. Add browser-side PDF URL loader request interception so the viewer's content
   frame can claim the original PDF stream.
7. Add enough `pdf_viewer_private` and `PdfHost` / `PDFDocumentHelper` support
   for the viewer shell to display the PDF.
8. Let Chromium's existing PDF navigation / SiteInstance / renderer launch path
   create the correct PDF renderer role.

## Constraints

- Do not link Chrome's full browser feature stack into Roamium just to get PDFs.
- Do not add `//chrome/browser/plugins:impl` back as the primary solution unless
  a later experiment proves a narrowly bounded form can link without dragging in
  unrelated Chrome product infrastructure.
- Do not enable general user extension support as a side effect of PDF support.
- Do not weaken PDF origin checks or mark ordinary renderers as PDF renderers.
- Do not fake PDF rendering with static HTML, screenshots, external apps, or
  macOS Preview handoff.
- Do not change the TermSurf protocol unless a later experiment proves the PDF
  viewer path needs protocol-level information that cannot be represented inside
  Chromium/Roamium.
- Every Chromium experiment in this issue must use its own branch following the
  project convention.

## Starting Point

The immediate next step is to design Experiment 1 for this issue.

Experiment 1 should be scoped around the first narrow Electron-style layer that
can replace the dead end from Issue 776 Experiment 8:

- study Electron's `streams_private` redirect and implementation in detail;
- design a TermSurf-owned PDF response throttle / `streams_private` handoff that
  avoids `//chrome/browser/plugins:impl`;
- identify the exact Chromium files that must be patched and the exact
  TermSurf-owned files that should receive the copied/adapted behavior;
- define a build verification that proves the new path avoids the dependency
  explosion from Issue 776 Experiment 8 before trying to make PDFs visibly
  render.

This issue should proceed one experiment at a time. Each experiment should land
one coherent layer or prove why that layer must be shaped differently.

## Experiments

### Experiment 1: Design the TermSurf PDF Stream Handoff

#### Description

Design the first Electron-style PDF layer for TermSurf: a narrow PDF response
interception and stream handoff path that replaces Issue 776 Experiment 8's
failed direct dependency on Chrome's
`PluginResponseInterceptorURLLoaderThrottle` implementation.

This is a design/proof experiment, not a rendering experiment. It should produce
the exact implementation plan for Experiment 2, including files, dependencies,
patch points, and verification gates. It should not change Chromium code.

The core question is:

> What is the smallest TermSurf-owned equivalent of Electron's PDF stream
> handoff that can feed `PdfViewerStreamManager` without linking
> `//chrome/browser/plugins:impl` or Chrome's full extension/browser stack?

#### Changes

1. Re-audit the Electron PDF stream path from the local Electron checkout.

   Use the local source only:

   ```bash
   rg "PluginResponseInterceptorURLLoaderThrottle|CreateURLLoaderThrottles|streams_private|PdfViewerStreamManager|PdfURLLoaderRequestInterceptor" \
     vendor/electron/shell vendor/electron/patches
   ```

   Record the precise roles of:
   - `shell/browser/electron_browser_client.cc::CreateURLLoaderThrottles()`;
   - `shell/browser/extensions/api/streams_private/streams_private_api.cc`,
     specifically the narrow `SendExecuteMimeTypeHandlerEvent()` PDF stream
     dispatch helper, not the general Chrome `streams_private` extension API;
   - `shell/browser/plugins/plugin_utils.cc`, because Electron pairs the
     response-interceptor include redirect with its own plugin-utils
     implementation;
   - `patches/chromium/hack_plugin_response_interceptor_to_point_to_electron.patch`;
   - Electron's `PdfURLLoaderRequestInterceptor` wiring;
   - Electron's `PdfHost` / `PDFDocumentHelper` binder only to record that it
     follows stream handoff and is out of scope for Experiment 2.

2. Re-audit the Chromium PDF stream path and GN dependency surface in the
   current Chromium branch.

   Inspect the current upstream implementation and the failed Issue 776
   Experiment 8 patch:

   ```bash
   rg "PluginResponseInterceptorURLLoaderThrottle|SendExecuteMimeTypeHandlerEvent|PdfViewerStreamManager|CreateTemplateMimeHandlerPage|StreamContainer" \
     chromium/src/chrome/browser \
     chromium/src/extensions \
     chromium/src/pdf \
     chromium/src/content/libtermsurf_chromium
   ```

   Also use GN to inspect target ownership and transitive dependencies. Ripgrep
   can find symbols, but GN is the source of truth for whether a design avoids
   the Issue 776 Experiment 8 dependency explosion:

   ```bash
   export PATH="$PWD/chromium/depot_tools:$PATH"
   gn desc chromium/src/out/Default //chrome/browser/plugins:plugins deps
   gn desc chromium/src/out/Default //chrome/browser/plugins:impl deps
   gn desc chromium/src/out/Default //chrome/browser/plugins:impl public_deps
   gn refs chromium/src/out/Default --tree \
     //chrome/browser/plugins/plugin_response_interceptor_url_loader_throttle.cc
   gn desc chromium/src/out/Default //chrome/browser/pdf:pdf_viewer_stream_manager deps
   ```

   The result must record the `//chrome/browser/plugins:plugins` vs.
   `//chrome/browser/plugins:impl` target split and identify which direct deps
   made `:impl` too broad for Roamium.

   Identify the smallest pieces needed to:
   - detect an `application/pdf` response;
   - replace the PDF response body with the PDF viewer embedder/template
     response, if still required;
   - transfer the original PDF response body into a `StreamContainer`;
   - call `PdfViewerStreamManager::Create()` and `AddStreamContainer()`;
   - avoid `PluginUtils::GetExtensionIdForMimeType()` and real
     `ExtensionRegistry` lookup for the PDF-only first pass.

3. Audit the current TermSurf Chromium branch state.

   Issue 776 Experiment 8 left a committed Chromium branch that may already
   contain partial PDF interceptor patches. Before choosing a design, record the
   current branch and the exact PDF-related patch state:

   ```bash
   git -C chromium/src branch --show-current
   git -C chromium/src log --oneline -5
   git -C chromium/src diff --name-only 148.0.7778.97-issue-776-exp7..HEAD
   rg "issue-776-exp8|pdf_only|PluginResponseInterceptorURLLoaderThrottle|TsPdfNavigationThrottle" \
     chromium/src/chrome/browser/plugins \
     chromium/src/content/libtermsurf_chromium
   ```

   The result must decide, per existing patch:
   - keep as-is;
   - modify in Experiment 2;
   - revert in Experiment 2 because it belongs to the failed direct-link path.

   Also record the starting state of `TsPdfNavigationThrottle`: whether the old
   Issue 776 wrapper cancellation path is active, disabled, or partially
   disabled.

4. Run a baseline Chromium build check.

   Before designing the next implementation, confirm whether the current
   Chromium branch links:

   ```bash
   export PATH="$PWD/chromium/depot_tools:$PATH"
   autoninja -C chromium/src/out/Default libtermsurf_chromium
   ```

   If the baseline build fails because the current branch still contains the
   Issue 776 Experiment 8 dependency/link failure, record that explicitly.
   Experiment 2's branch base must then be a buildable branch before the failed
   dependency path, or must include an explicit revert of the failed path as its
   first implementation step.

5. Produce a dependency map.

   Compare three candidate implementation shapes:
   - **A. Copy the stock Chrome interceptor into `content/libtermsurf_chromium/`
     and strip it to PDF-only.**
   - **B. Patch the stock Chromium interceptor to call a TermSurf
     `SendExecuteMimeTypeHandlerEvent` / PDF stream-dispatch shim, following
     Electron's patch, and pair it with a TermSurf/Electron-style plugin-utils
     fork as needed. Do not link `//chrome/browser/plugins:impl` directly.**
   - **C. Implement a fresh TermSurf `blink::URLLoaderThrottle` that performs
     only the PDF-specific interception and stream handoff.**

   For each candidate, document:
   - Chromium files touched;
   - TermSurf-owned files added;
   - BUILD.gn deps required;
   - downstream pinned deps that remain even after avoiding
     `//chrome/browser/plugins:impl`, including `StreamContainer`,
     `MimeHandlerViewAttachHelper`, `PdfViewerStreamManager`, and
     `TransferrableURLLoader`;
   - whether it depends on `chrome/browser/plugins`,
     `chrome/browser/extensions`, `extensions/browser`, `MimeHandlerViewGuest`,
     or `GuestViewManager`;
   - which existing Issue 776 Experiment 8 patches it keeps, modifies, or
     reverts;
   - why it should or should not avoid the Issue 776 Experiment 8 dependency
     explosion.

   The result table must include:

   | Candidate | Files | New deps | Pinned downstream deps | Broad Chrome deps? | Exp 8 patch disposition | Decision |
   | --------- | ----- | -------- | ---------------------- | ------------------ | ----------------------- | -------- |

6. Choose the Experiment 2 implementation shape.

   Prefer the smallest buildable TermSurf-owned path. Do not bias the result
   toward a fresh throttle merely because it sounds cleaner. Electron's proven
   shape is closer to a narrow patch/fork of the Chrome path, so the audit must
   let the GN dependency evidence decide whether A, B, C, or a hybrid is the
   right first implementation.

   The selected design must specify:
   - new file names, likely under `content/libtermsurf_chromium/`;
   - exact existing files to edit;
   - exact class/function names being added or changed;
   - exact BUILD.gn deps to add;
   - unavoidable `chrome/browser/...` deps that remain, with justification;
   - exact Chromium branch base and branch name;
   - exact existing Issue 776 Experiment 8 patches to keep, modify, or revert;
   - whether the old Issue 776 wrapper throttle remains disabled;
   - whether Experiment 7's PDF extension resource bypass remains intact or is
     replaced by a manifest/resource-policy fix;
   - whether OOPIF PDF is enabled at runtime and how Experiment 2 should log the
     feature flag;
   - what `[issue-789-exp2]` logs should prove at runtime;
   - which missing PDF layer is intentionally deferred after stream handoff.

7. Define build verification before runtime verification.

   Experiment 2 must first prove the dependency surface is narrow. The design
   should require:

   ```bash
   autoninja -C chromium/src/out/Default libtermsurf_chromium
   ```

   The build gate passes only if it links without adding
   `//chrome/browser/plugins:impl` or pulling in the Chrome browser dependency
   graph that caused Issue 776 Experiment 8 to fail.

8. Define the runtime probe for Experiment 2.

   If Experiment 2 builds, the first runtime probe should load the vendored
   Bitcoin PDF with existing automation and inspect logs. The PDF does not need
   to visibly render in Experiment 2.

   Required runtime proof should include logs showing:
   - the TermSurf PDF response throttle saw `application/pdf`;
   - the old wrapper path did not cancel the navigation before the response
     throttle;
   - the original PDF stream was represented as a `StreamContainer` or a
     TermSurf equivalent accepted by `PdfViewerStreamManager`;
   - `PdfViewerStreamManager::AddStreamContainer()` was reached, or the exact
     reason it could not be reached.

   The runtime design should also state that the teardown crash observed in
   Issue 776's automation is residual and out of scope unless it prevents log or
   screenshot capture.

9. Record the design output directly in this experiment.

   The result must include the candidate table from step 5 and a concrete
   Experiment 2 implementation checklist. Each checklist item must name:
   - exact file path;
   - exact class/function being added or modified;
   - owning GN target;
   - expected build invariant or `[issue-789-exp2]` log line proving the item
     works.

10. Format this issue document:

```bash
prettier --write --prose-wrap always --print-width 80 \
  issues/0789-electron-style-pdf-viewer/README.md
```

#### Non-Negotiable Invariants

- Do not modify Chromium source code in Experiment 1.
- Do not modify Rust code in Experiment 1.
- Do not add a Chromium branch in Experiment 1 unless the audit proves a branch
  is needed solely to inspect current state. If a branch is created, do not
  commit code to it.
- Do not design a solution that depends on linking
  `//chrome/browser/plugins:impl` as the primary path.
- Do not design a solution that enables general user extensions.
- Do not weaken PDF renderer or PDF origin security.
- Do not treat "no `chrome/browser` deps" as the goal. Some bounded
  `chrome/browser/...` deps, such as `PdfViewerStreamManager`, may be
  unavoidable. The goal is to avoid broad Chrome product infrastructure and
  document every unavoidable dependency.
- Do not define success as "PDF visibly renders" for Experiment 2. Experiment 2
  success is stream handoff reaching, or precisely failing before,
  `PdfViewerStreamManager`.
- Preserve Issue 776's useful artifacts: internal PDF plugin registration,
  static PDF viewer resource serving, vendored Bitcoin PDF fixture, and
  screenshot automation.

#### Verification

1. Confirm this experiment made documentation-only changes:

   ```bash
   git diff --name-only
   ```

   Expected: only `issues/0789-electron-style-pdf-viewer/README.md` changes
   during Experiment 1 design and result recording.

2. Confirm the Electron audit cites concrete local files, not memory or web
   search.

3. Confirm the Chromium audit cites concrete local files and identifies why
   Issue 776 Experiment 8 pulled in broad Chrome dependencies.

4. Confirm the Chromium audit includes GN evidence, not just `rg` output.

5. Confirm the current Chromium branch / existing patch audit names which Issue
   776 Experiment 8 patches should be kept, modified, or reverted.

6. Confirm the candidate table includes at least A, B, and C, includes pinned
   downstream deps, and gives each candidate a decision with a reason.

7. Confirm the selected Experiment 2 checklist is specific enough to implement
   without another design round.

#### Pass Criteria

Pass if the experiment produces:

- a concrete Electron PDF stream handoff map;
- a concrete Chromium PDF stream handoff map;
- a GN-backed dependency map explaining the `//chrome/browser/plugins:plugins`
  vs. `//chrome/browser/plugins:impl` split;
- a current-branch patch disposition table for the Issue 776 Experiment 8
  residue;
- a candidate comparison table with pinned downstream deps;
- a selected implementation shape for Experiment 2;
- an explicit BUILD.gn dependency plan that avoids the Issue 776 Experiment 8
  dependency explosion;
- a concrete Experiment 2 implementation checklist;
- no code changes.

#### Partial Criteria

Partial if the audit identifies the correct candidate direction but cannot
finish the Experiment 2 checklist because a key Chromium or Electron dependency
relationship is still unknown.

The result must name the exact missing fact and the next command or source file
needed to resolve it.

#### Failure Criteria

- The experiment proposes linking `//chrome/browser/plugins:impl` again without
  a new reason that invalidates Issue 776 Experiment 8's linker result.
- The experiment hand-waves "copy Electron" without naming files, dependencies,
  and patch points.
- The experiment designs a fake PDF renderer or external handoff.
- The experiment ignores current Issue 776 Experiment 8 Chromium patches.
- The experiment uses ripgrep-only dependency reasoning when GN data is
  available.
- The experiment quietly expands Experiment 2 into the full
  MimeHandlerView/GuestView/pdf_viewer_private stack instead of isolating the
  first stream handoff layer.
- The experiment changes code.

**Result:** Pass

Experiment 1 produced the needed design output and made no code changes. The
main finding is that Issue 776 Experiment 8 failed for the reason suspected:
directly linking Chrome's stock plugin response interceptor implementation pulls
Roamium into Chrome's broad browser product dependency graph. Experiment 2
should not retry that path. It should add a TermSurf-owned, PDF-only stream
handoff layer modeled on Electron's patched handoff, with the first build gate
proving that `//chrome/browser/plugins:impl` is not linked.

#### Electron Stream Handoff Map

The local Electron audit shows that Electron does not import Chrome's full
browser feature stack. It installs a narrow handoff path and patches Chromium's
PDF stream code to call Electron-owned glue:

- `vendor/electron/shell/browser/electron_browser_client.cc::CreateURLLoaderThrottles()`
  installs `PluginResponseInterceptorURLLoaderThrottle` for navigation
  responses.
- `vendor/electron/shell/browser/electron_browser_client.cc::WillCreateURLLoaderRequestInterceptors()`
  wires `pdf::PdfURLLoaderRequestInterceptor::MaybeCreateInterceptor(...)` with
  Electron's PDF stream delegate.
- `vendor/electron/shell/browser/electron_browser_client.cc::RegisterBrowserInterfaceBindersForFrame()`
  binds `pdf::mojom::PdfHost` through `pdf::PDFDocumentHelper`. This is needed
  after stream handoff and is out of scope for Experiment 2.
- `vendor/electron/shell/renderer/renderer_client_base.cc::RenderFrameCreated()`
  binds `extensions::mojom::MimeHandlerViewContainerManager` in renderer frames.
- `vendor/electron/shell/renderer/renderer_client_base.cc::IsPluginHandledExternally()`
  calls `MimeHandlerViewContainerManager::CreateFrameContainer(...)` for
  externally handled PDF plugin loads.
- `vendor/electron/shell/browser/extensions/api/streams_private/streams_private_api.cc`
  implements the narrow `SendExecuteMimeTypeHandlerEvent(...)` behavior that
  stores an `extensions::StreamContainer`. For OOPIF PDF, it creates
  `PdfViewerStreamManager` for the `WebContents` and calls
  `AddStreamContainer(frame_tree_node_id, internal_id, stream_container)`.
- `vendor/electron/shell/browser/plugins/plugin_utils.cc` is paired with the
  interceptor redirect. Electron does not use Chrome's full plugin-utils
  implementation for this handoff.
- `vendor/electron/patches/chromium/hack_plugin_response_interceptor_to_point_to_electron.patch`
  redirects Chrome's plugin response interceptor includes from Chrome
  `streams_private` and `plugin_utils` to Electron equivalents.

The Electron model to copy is not "link Chrome." It is "patch or fork the Chrome
handoff point so it calls embedder-owned stream glue."

#### Chromium Stream Handoff Map

The Chromium path starts in
`chrome/browser/plugins/plugin_response_interceptor_url_loader_throttle.cc`.
That throttle observes plugin-eligible responses, creates a viewer/template
response through `MimeHandlerViewAttachHelper::CreateTemplateMimeHandlerPage()`,
wraps the original response in a `blink::mojom::TransferrableURLLoader`, and
dispatches the original stream to
`extensions::mime_handlers::SendExecuteMimeTypeHandlerEvent(...)`.

The dispatch path in
`chrome/browser/extensions/api/mime_handlers/dispatch_mime_handler_event.cc`
creates an `extensions::StreamContainer`. In the OOPIF PDF case, it uses
`chrome/browser/pdf/pdf_viewer_stream_manager.cc` to store that stream until the
PDF viewer frame claims it.

The GN audit is the important part:

- `//chrome/browser/plugins:plugins` is mostly headers and modest public
  dependencies: `content/public/browser`, `content/public/common`, profile,
  prefs, content-settings common, infobars core, network mojom, blink common,
  and build flags.
- `//chrome/browser/plugins:impl` owns
  `plugin_response_interceptor_url_loader_throttle.cc` and `plugin_utils.cc`.
  Its direct dependencies include `//chrome/browser:browser_process`,
  `//chrome/browser:browser_public_dependencies`, `//chrome/browser/extensions`,
  `//components/guest_view/browser`, `//chrome/browser/infobars`,
  `//components/component_updater`, `//components/metrics_services_manager`,
  `//extensions/browser`, and `//extensions/common`.
- The queried `//chrome/browser/pdf:pdf_viewer_stream_manager` target does not
  exist in this checkout. The owning target is `//chrome/browser/pdf:pdf`, which
  includes `pdf_viewer_stream_manager.cc/h` but also brings broad PDF viewer
  resources and Chrome browser PDF support.
- `gn refs ... plugin_response_interceptor_url_loader_throttle.cc` did not
  produce a useful reverse-reference tree for this source file; the actual
  ownership came from `chrome/browser/plugins/BUILD.gn`.

Baseline build on the current Chromium branch `148.0.7778.97-issue-776-exp8`
fails at the final `libtermsurf_chromium.dylib` link. The failure includes:

- `prerender::ChromeNoStatePrefetchContentsDelegate::FromWebContents(...)`
  referenced from Chrome's `dispatch_mime_handler_event.o`;
- macOS ScreenCaptureKit symbols from Chrome WebRTC capture code:
  `SCShareableContent`, `SCStreamConfiguration`, `SCScreenshotManager`, and
  `SCContentFilter`;
- `AVCaptureDevice` from Chrome media permission code.

That confirms the direct-link path pulls unrelated Chrome product infrastructure
into Roamium.

#### Issue 776 Experiment 8 Patch Disposition

Current Chromium branch: `148.0.7778.97-issue-776-exp8`.

Recent PDF commits:

- `993436b5a4d25 Probe PDF stream handoff`
- `4f2faa351a914 Serve PDF viewer shell resources`
- `98e40c5c67ed9 Register PDF viewer resources`
- `2991c1e4fa1a0 Trace PDF renderer routing`
- `9bd25c25cd438 Trace the PDF renderer gate`

Patch disposition for Experiment 2:

| Patch/file                                                                   | Disposition                    | Reason                                                                                                                                        |
| ---------------------------------------------------------------------------- | ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `chrome/browser/plugins/plugin_response_interceptor_url_loader_throttle.*`   | Revert                         | This modifies the stock Chrome interceptor but still leaves ownership in `//chrome/browser/plugins:impl`, which is the failed dependency.     |
| `chrome/browser/extensions/api/mime_handlers/dispatch_mime_handler_event.cc` | Revert                         | This patches Chrome's stock dispatch path and links broad Chrome extension infrastructure. TermSurf needs its own stream-dispatch helper.     |
| `chrome/browser/pdf/pdf_viewer_stream_manager.cc`                            | Revert/log later               | Experiment 8 logging is useful diagnostically but should not be a permanent upstream Chrome patch for the first TermSurf-owned handoff.       |
| `content/libtermsurf_chromium/BUILD.gn`                                      | Modify                         | Remove `//chrome/browser/plugins:impl`; add only the narrow deps required by TermSurf-owned files.                                            |
| `content/libtermsurf_chromium/ts_browser_client.cc/h`                        | Modify                         | Keep the idea of installing a PDF response throttle, but install a TermSurf-owned throttle instead of Chrome's stock interceptor class.       |
| `content/libtermsurf_chromium/ts_pdf_navigation_throttle.cc`                 | Keep disabled wrapper behavior | The old data-wrapper cancellation path is disabled on Exp 8. Experiment 2 should preserve that: PDF navigation must reach the response layer. |

The clean branch base for Experiment 2 should be `148.0.7778.97-issue-776-exp7`,
then create `148.0.7778.97-issue-789-exp2`. If Experiment 2 branches from Exp 8
instead, its first step must explicitly revert the failed direct-link patches
above.

#### Candidate Comparison

| Candidate                                                   | Files                                                                                                       | New deps                                                                                                                                                                                                | Pinned downstream deps                                                                                                               | Broad Chrome deps?                                                                                                                        | Exp 8 patch disposition                                                        | Decision                                                                                                                     |
| ----------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------- |
| A. Copy Chrome interceptor into TermSurf and strip PDF-only | Add `content/libtermsurf_chromium/ts_pdf_response_interceptor.*`; edit `ts_browser_client.*` and `BUILD.gn` | `content/public/browser`, `services/network/public/cpp`, `third_party/blink/public/mojom`, likely bounded `extensions/browser/mime_handler` pieces, and a bounded PDF stream manager dep if it can link | `TransferrableURLLoader`, `StreamContainer`, `MimeHandlerViewAttachHelper` or equivalent template creation, `PdfViewerStreamManager` | Avoids `//chrome/browser/plugins:impl`; still may need bounded `chrome/browser/pdf` or copied stream-manager glue                         | Revert stock Chrome interceptor patches; reapply only TermSurf-owned install   | Best first implementation if copied code can be stripped without recreating too much Chrome extension state.                 |
| B. Patch stock interceptor to call TermSurf shims           | Patch Chrome interceptor includes like Electron; add TermSurf `streams_private` and plugin-utils shims      | Still needs the owning target for the stock `.cc`, unless the target is split                                                                                                                           | Same as A                                                                                                                            | Risk remains high because stock `.cc` lives in `//chrome/browser/plugins:impl`; patching includes alone does not remove target-level deps | Would keep/reshape stock interceptor patches, but only after a GN target split | Not Experiment 2 unless paired with a target split. The Exp 8 link failure proves include redirection alone is insufficient. |
| C. Fresh TermSurf `blink::URLLoaderThrottle`                | Add fresh `content/libtermsurf_chromium/ts_pdf_response_throttle.*` and dispatch helper                     | Potentially narrowest: only the APIs actually used                                                                                                                                                      | Same stream/template/PDF manager concepts still remain                                                                               | Avoids `//chrome/browser/plugins:impl`, but risks reimplementing subtle navigation throttle behavior incorrectly                          | Revert Exp 8 stock patches                                                     | Viable fallback, but worse than A if the stock interceptor can be safely copied and stripped.                                |

The selected shape is a hybrid of A and C: create a TermSurf-owned PDF-only
response throttle by copying the relevant Chrome response-interceptor logic into
`content/libtermsurf_chromium/`, then strip it until it has no dependency on
Chrome plugin utilities, real `ExtensionRegistry`, or
`//chrome/browser/plugins:impl`.

#### Experiment 2 Implementation Checklist

Branch:

1. Start from `148.0.7778.97-issue-776-exp7`.
2. Create `148.0.7778.97-issue-789-exp2`.
3. Add the branch to `chromium/README.md`.

Code shape:

1. Add `content/libtermsurf_chromium/ts_pdf_response_interceptor.h`.
   - Owning target: `content/libtermsurf_chromium`.
   - Class/function: a PDF-only URL loader throttle class modeled on Chrome's
     `PluginResponseInterceptorURLLoaderThrottle`.
   - Build invariant: no dependency on `//chrome/browser/plugins:impl`.
   - Runtime log: `[issue-789-exp2] pdf-response-throttle-created`.
2. Add `content/libtermsurf_chromium/ts_pdf_response_interceptor.cc`.
   - Detect `application/pdf` with the post-sniff MIME value.
   - Create or call the viewer-template response path needed by the PDF viewer.
   - Transfer the original response body through a
     `blink::mojom::TransferrableURLLoader`.
   - Runtime log:
     `[issue-789-exp2] pdf-response url=... mime=... destination=... oopif_pdf=...`.
3. Add `content/libtermsurf_chromium/ts_pdf_stream_dispatch.h/cc`.
   - Implement only the PDF part of Electron's
     `SendExecuteMimeTypeHandlerEvent(...)`.
   - Construct the `extensions::StreamContainer` or the narrow equivalent
     accepted by `PdfViewerStreamManager`.
   - Call `PdfViewerStreamManager::Create(web_contents)` and
     `AddStreamContainer(frame_tree_node_id, internal_id, stream_container)`, if
     the bounded dependency can link.
   - Runtime logs: `[issue-789-exp2] stream-dispatch ...` and
     `[issue-789-exp2] stream-container-added ...`.
4. Edit `content/libtermsurf_chromium/ts_browser_client.cc/h`.
   - Install the TermSurf throttle in `CreateURLLoaderThrottles()`.
   - Do not include or instantiate Chrome's
     `PluginResponseInterceptorURLLoaderThrottle`.
   - Runtime log: `[issue-789-exp2] pdf-throttle-installed`.
5. Edit `content/libtermsurf_chromium/BUILD.gn`.
   - Add the new TermSurf files.
   - Do not add `//chrome/browser/plugins:impl`.
   - Add only narrow deps required by the new code. Any `chrome/browser/...` dep
     must be documented in the experiment result.
6. Preserve `content/libtermsurf_chromium/ts_pdf_navigation_throttle.cc` with
   the wrapper-cancel path disabled.
   - Runtime proof: old wrapper logs show the navigation proceeds to the
     response throttle.
7. Preserve Issue 776 Experiment 7 static PDF viewer resource serving.
   - Do not fix the manifest/resource policy in Experiment 2 unless the stream
     handoff reaches `PdfViewerStreamManager` and then fails only on resource
     policy.
8. Log `chrome_pdf::features::IsOopifPdfEnabled()` in the response path.
   - If false, mark Experiment 2 Partial and design the next experiment around
     enabling the PDF process model.

Build verification for Experiment 2:

```bash
export PATH="$PWD/chromium/depot_tools:$PATH"
autoninja -C chromium/src/out/Default libtermsurf_chromium
gn desc chromium/src/out/Default //content/libtermsurf_chromium deps
```

The build passes only if `libtermsurf_chromium` links and the dependency output
does not include `//chrome/browser/plugins:impl`,
`//components/guest_view/browser`, Chrome WebRTC capture, or Chrome media
permission subsystems as a consequence of the PDF handoff.

Runtime verification for Experiment 2:

1. Run the existing Issue 776 Bitcoin PDF automation.
2. Inspect logs for:
   - `[issue-789-exp2] pdf-throttle-installed`;
   - `[issue-789-exp2] pdf-response ... mime=application/pdf`;
   - evidence the old wrapper path did not cancel the navigation;
   - `[issue-789-exp2] stream-dispatch ...`;
   - `[issue-789-exp2] stream-container-added ...`, or a precise failure before
     that line.
3. Visible PDF rendering is not required for Experiment 2. The expected pass is
   that the original PDF stream reaches, or precisely fails immediately before,
   `PdfViewerStreamManager`.
4. The teardown crash from Issue 776 remains out of scope unless it prevents log
   or screenshot capture.

#### Conclusion

Experiment 1 confirms the next step should not be another direct link to
Chrome's plugin implementation. Electron's useful lesson is the ownership
boundary: keep the PDF stream handoff in embedder-owned code, and let that code
feed Chromium's PDF stream manager.

Experiment 2 should implement the TermSurf-owned PDF response throttle and
stream-dispatch helper on a fresh Chromium branch from the last buildable PDF
branch. Its first success condition is build shape, not visible rendering.

### Experiment 2: Build a TermSurf PDF Stream Handoff

#### Description

Implement the first Electron-style PDF stream handoff layer in TermSurf-owned
Chromium code.

This experiment replaces the failed Issue 776 Experiment 8 direct dependency on
Chrome's `//chrome/browser/plugins:impl` with a narrow PDF-only URL loader
throttle under `content/libtermsurf_chromium/`. The new path should observe PDF
responses, preserve the original PDF response body as a transferable stream, and
emit the PDF viewer embedder/template HTML response that Chromium's PDF viewer
path expects.

This is still not a visible-rendering experiment. A Pass means the code builds
without Chrome's broad plugin implementation, sees a PDF response, emits the
viewer-template payload, and proves the original PDF stream has been captured in
the first TermSurf handoff layer. Reaching
`PdfViewerStreamManager::AddStreamContainer()` is a stretch outcome, not the
Pass bar, because in this Chromium checkout `PdfViewerStreamManager` is owned by
the broad `//chrome/browser/pdf:pdf` target. Splitting, forking, or replacing
that manager is a later experiment unless a narrow link path appears during
implementation.

Visible PDF rendering, MimeHandlerView renderer wiring, `pdf_viewer_private`,
`PdfHost`, full viewer behavior, and the final stream-manager integration remain
follow-up layers unless this experiment happens to reach them naturally without
breaking the build/dependency gate.

#### Changes

1. Create a new Chromium branch from the last buildable PDF branch.

   ```bash
   cd chromium/src
   git checkout 148.0.7778.97-issue-776-exp7
   git checkout -b 148.0.7778.97-issue-789-exp2
   ```

   Update `chromium/README.md` in the main repo to list the new branch and link
   it to this issue.

   Do not branch from `148.0.7778.97-issue-776-exp8` unless the first commit on
   the new branch explicitly reverts the failed direct-link path from that
   branch.

2. Preserve useful Issue 776 work.

   Keep these layers from the latest buildable PDF branch:
   - internal PDF plugin registration;
   - static PDF viewer resource serving from Issue 776 Experiment 7;
   - vendored Bitcoin PDF fixture and screenshot automation;
   - disabled old data-wrapper cancellation behavior, if present on the chosen
     base.

   If the chosen base branch still has the old data-wrapper cancellation active,
   disable it before installing the new response handoff. Preferred mechanism:
   remove the old PDF navigation throttle from
   `TsBrowserClient::CreateThrottlesForNavigation()`. Acceptable fallback: leave
   the throttle installed only if its PDF branch always returns `PROCEED` and
   logs that it did so.

   PDF navigation must not be canceled after the new URL loader throttle swaps
   in the viewer-template response. The old navigation throttle can still see
   `application/pdf` after the body swap, so "disabled" must mean no cancel path
   remains.

3. Add a TermSurf-owned PDF response URL loader throttle.

   Add:

   ```text
   chromium/src/content/libtermsurf_chromium/ts_pdf_response_url_loader_throttle.h
   chromium/src/content/libtermsurf_chromium/ts_pdf_response_url_loader_throttle.cc
   ```

   The implementation should be modeled on Chromium's
   `PluginResponseInterceptorURLLoaderThrottle`, but stripped to the PDF-only
   path.

   Requirements:
   - derive from the Chromium URL loader throttle interface used by
     `ContentBrowserClient::CreateURLLoaderThrottles()`;
   - inspect the post-sniff response MIME type;
   - handle only `application/pdf`;
   - log `destination=` on every PDF entry and initially handle only the request
     destinations required for the top-level Bitcoin PDF automation;
   - leave all non-PDF responses untouched;
   - do not call `PluginUtils::GetExtensionIdForMimeType()`;
   - do not require `ExtensionRegistry` lookup for the first pass;
   - hard-code only the PDF viewer extension identity needed for the stream
     handoff, with a comment explaining that this is the PDF-only equivalent of
     Electron's embedder-owned plugin-utils path;
   - inline or copy the PDF embedder/template HTML equivalent of Chromium's
     `IDR_PDF_EMBEDDER_HTML` into TermSurf-owned code, with a comment pointing
     to the upstream resource it tracks;
   - do not call
     `extensions::MimeHandlerViewAttachHelper::CreateTemplateMimeHandlerPage()`
     in this experiment unless GN proves that doing so does not pull broad
     extension/browser dependencies;
   - log:

     ```text
     [issue-789-exp2] pdf-response-throttle-created
     [issue-789-exp2] pdf-response url=<url> mime=<mime> destination=<destination> oopif_pdf=<true|false>
     [issue-789-exp2] viewer-template-emitted internal_id=<id> bytes=<n>
     ```

   The response URL loader throttle must not include
   `chrome/browser/plugins/plugin_response_interceptor_url_loader_throttle.h` or
   instantiate Chrome's stock `PluginResponseInterceptorURLLoaderThrottle`.

4. Add a TermSurf-owned PDF stream dispatch helper.

   Add:

   ```text
   chromium/src/content/libtermsurf_chromium/ts_pdf_stream_dispatch.h
   chromium/src/content/libtermsurf_chromium/ts_pdf_stream_dispatch.cc
   ```

   This helper should implement only the PDF-specific subset of Electron's
   `SendExecuteMimeTypeHandlerEvent(...)`.

   Requirements:
   - receive the original PDF response as a
     `blink::mojom::TransferrableURLLoader`;
   - construct `extensions::StreamContainer` from the narrow
     `//extensions/browser/mime_handler:stream_container` target;
   - do not substitute a TermSurf stand-in type for `StreamContainer`;
   - attempt `PdfViewerStreamManager::Create(web_contents)` and
     `PdfViewerStreamManager::AddStreamContainer(frame_tree_node_id, internal_id, stream_container)`
     only if GN and link evidence show the owning PDF target does not recreate
     the broad dependency failure;
   - log:

     ```text
     [issue-789-exp2] stream-dispatch frame_tree_node_id=<id> extension_id=<id> handler_url=<url> internal_id=<id>
     [issue-789-exp2] stream-container-added internal_id=<id>
     ```

   If linking `PdfViewerStreamManager` requires `//chrome/browser/pdf:pdf`, stop
   before adding that dependency unless a GN/link check proves it is safe. A
   buildable throttle plus `StreamContainer` capture, followed by a precise "PDF
   stream manager is too broad" result, is still a valid Pass for this
   experiment. The next experiment should then choose between splitting
   `//chrome/browser/pdf:pdf`, forking the stream manager, or accepting a
   carefully bounded Chrome PDF dependency.

5. Install the TermSurf response URL loader throttle.

   Edit:

   ```text
   chromium/src/content/libtermsurf_chromium/ts_browser_client.h
   chromium/src/content/libtermsurf_chromium/ts_browser_client.cc
   ```

   In `TsBrowserClient::CreateURLLoaderThrottles()`, append the TermSurf PDF
   response URL loader throttle after preserving the existing
   shell/content-shell throttles.

   Log:

   ```text
   [issue-789-exp2] pdf-throttle-installed frame_tree_node_id=<id> destination=<destination>
   ```

   Do not install the stock Chrome interceptor.

6. Update `content/libtermsurf_chromium/BUILD.gn`.

   Add the new TermSurf files and the narrow deps needed by those files.

   Required dependency rule:
   - do not add `//chrome/browser/plugins:impl`;
   - do not add `//chrome/browser/extensions:extensions` merely to reuse
     Chrome's stock MIME dispatch;
   - do not pull in `//components/guest_view/browser`,
     `//chrome/browser/infobars`, Chrome WebRTC capture, or Chrome media
     permission subsystems as a side effect of this handoff.

   Bounded dependencies are acceptable only if the experiment result documents
   why they are unavoidable for the PDF stream handoff. Likely candidates
   include:
   - `//extensions/browser/mime_handler:stream_container`;
   - `//extensions/browser/mime_handler:stream_info`;
   - the narrowest available PDF stream-manager target, if one exists.

   Before adding any target that was only assumed to exist, confirm target names
   with:

   ```bash
   gn ls chromium/src/out/Default //extensions/browser/mime_handler:*
   gn ls chromium/src/out/Default //chrome/browser/pdf:*
   ```

   The current expectation from Experiment 1 is that
   `//extensions/browser/mime_handler:stream_container` exists and is narrow,
   while the only PDF stream-manager owner is broad `//chrome/browser/pdf:pdf`.
   Do not add `//chrome/browser/pdf:pdf` in this experiment unless GN and link
   evidence disprove that expectation.

7. Add temporary runtime diagnostics only under the `[issue-789-exp2]` prefix.

   It is acceptable to add temporary logs to the new TermSurf-owned files. Avoid
   adding permanent diagnostic noise to stock Chrome files. If a stock Chrome
   file must be logged to pinpoint the handoff boundary, keep the log minimal
   and record it in the result as cleanup debt.

   Required feature log:

   ```text
   [issue-789-exp2] oopif-pdf-enabled value=<true|false>
   ```

   If OOPIF PDF is false, stop treating stream handoff as a Pass. Mark the
   experiment Partial and design the next experiment around enabling the PDF
   process/model path.

8. Commit the Chromium branch and archive patches only after the build result is
   coherent.

   If the Chromium code builds, commit the Chromium branch with git-poet and
   regenerate the patch archive:

   ```bash
   cd chromium/src
   git format-patch 148.0.7778.97-issue-776-exp7..HEAD \
     -o ../../chromium/patches/issue-789/
   ```

   If the implementation reaches a useful Partial but does not build because of
   a precisely identified dependency boundary, commit only if the branch is a
   coherent artifact for the next experiment. Otherwise, leave the code
   uncommitted, record the failure, and revert before closing the experiment
   result in the main repo.

9. Format any touched C++ files with Chromium's formatter.

   Use Chromium's normal formatting tooling for modified C++/header files. Do
   not run `cargo fmt`; this experiment should not edit Rust.

10. Format this issue document after recording the result:

```bash
prettier --write --prose-wrap always --print-width 80 \
  issues/0789-electron-style-pdf-viewer/README.md
```

#### Non-Negotiable Invariants

- Do not link `//chrome/browser/plugins:impl`.
- Do not instantiate Chrome's stock `PluginResponseInterceptorURLLoaderThrottle`
  from TermSurf code.
- Do not enable general user extensions.
- Do not add a general Chrome `streams_private` API surface. Implement only the
  PDF stream handoff needed by the viewer path.
- Do not weaken PDF origin checks, bypass `IsPdfRenderer()` checks, or mark an
  ordinary renderer as a PDF renderer.
- Do not fake rendering by replacing PDFs with static HTML, screenshots,
  external apps, or Preview handoff.
- Do not make visible PDF rendering the Pass condition for this experiment.
- Do not change the TermSurf protobuf protocol.
- Do not edit Rust code.
- Preserve existing non-PDF navigation behavior.
- Preserve normal HTML browsing.
- Preserve the Issue 776 Bitcoin PDF automation fixture.

#### Verification

1. Confirm the Chromium branch starts from the intended base:

   ```bash
   git -C chromium/src branch --show-current
   git -C chromium/src merge-base --is-ancestor \
     148.0.7778.97-issue-776-exp7 HEAD
   ```

   Expected branch: `148.0.7778.97-issue-789-exp2`.

2. Confirm the failed direct-link path is not present:

   ```bash
   rg "//chrome/browser/plugins:impl|PluginResponseInterceptorURLLoaderThrottle" \
     chromium/src/content/libtermsurf_chromium \
     chromium/src/chrome/browser/plugins
   ```

   Expected:
   - no `//chrome/browser/plugins:impl` in
     `content/libtermsurf_chromium/BUILD.gn`;
   - no TermSurf include or instantiation of Chrome's stock
     `PluginResponseInterceptorURLLoaderThrottle`;
   - stock Chrome files unchanged unless the result explicitly records a minimal
     patch.

3. Inspect the TermSurf GN dependency surface:

   ```bash
   export PATH="$PWD/chromium/depot_tools:$PATH"
   gn args chromium/src/out/Default --list >/dev/null
   gn ls chromium/src/out/Default //extensions/browser/mime_handler:*
   gn ls chromium/src/out/Default //chrome/browser/pdf:*
   gn desc chromium/src/out/Default //content/libtermsurf_chromium deps
   ```

   Record whether the output includes any of:
   - `//chrome/browser/plugins:impl`;
   - `//chrome/browser/extensions:extensions`;
   - `//components/guest_view/browser`;
   - Chrome WebRTC capture targets;
   - Chrome media permission targets.

   Any such dependency is a failure unless the result proves it is unrelated to
   the PDF changes and already existed on the base branch.

4. Build Chromium:

   ```bash
   cd chromium/src
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   autoninja -C out/Default libtermsurf_chromium
   ```

   This is the primary gate. The build must link `libtermsurf_chromium.dylib`
   without the Issue 776 Experiment 8 undefined symbols.

5. If the build passes, run the existing Issue 776 Bitcoin PDF automation.

   Use the vendored fixture, not the network URL. Capture the same screenshot
   and logs as Issue 776, but evaluate only the stream-handoff logs for this
   experiment. The Issue 776 teardown crash remains residual and out of scope
   unless it prevents log or screenshot capture.

6. Confirm the runtime logs include:

   ```text
   [issue-789-exp2] pdf-throttle-installed
   [issue-789-exp2] oopif-pdf-enabled value=<true|false>
   [issue-789-exp2] pdf-response ... mime=application/pdf
   [issue-789-exp2] viewer-template-emitted ...
   [issue-789-exp2] stream-dispatch ...
   ```

   If `oopif-pdf-enabled` is `false`, stop here and record Partial with reason
   `OOPIF PDF disabled`.

   If the handoff reaches `PdfViewerStreamManager`, also require:

   ```text
   [issue-789-exp2] stream-container-added ...
   ```

   If the handoff fails before that line, the result must quote the last
   `[issue-789-exp2]` line and identify the exact missing dependency or API.

   Also verify the old data-wrapper navigation throttle did not cancel the
   navigation after the response body was swapped. The expected proof is
   `[issue-789-exp2] pdf-response ...` with no later wrapper-cancel log for the
   same PDF navigation.

   If the embedder template navigates to the PDF viewer extension URL, verify
   the existing Issue 776 Experiment 7 resource path served it, for example with
   an `[issue-776-exp7] served ... pdf/index.html ...` log line.

7. Confirm ordinary browsing still works:
   - open a normal HTML page;
   - click a link;
   - type in a text field;
   - reload.

   These checks ensure the new response throttle leaves non-PDF responses alone.

8. Confirm a non-PDF binary response is not intercepted.

   Load a small non-PDF fixture if one exists, or add a tiny local binary
   fixture under the existing test server assets if needed. The logs should not
   show `pdf-response ... mime=application/pdf` for that response.

9. Record the result in this experiment.

   The result must include:
   - Chromium branch name;
   - commit hash if a Chromium commit was made;
   - patch archive path if generated;
   - exact new files;
   - exact GN deps added;
   - target-name evidence from `gn ls`;
   - build result;
   - dependency-surface result;
   - runtime log summary;
   - whether `PdfViewerStreamManager::AddStreamContainer()` was reached;
   - what layer remains for Experiment 3.

#### Pass Criteria

Pass if:

- the Chromium branch builds and links `libtermsurf_chromium`;
- `content/libtermsurf_chromium` does not depend on
  `//chrome/browser/plugins:impl`;
- the TermSurf-owned PDF response URL loader throttle is installed;
- PDF responses reach the TermSurf-owned response URL loader throttle;
- the response path emits the PDF viewer-template payload from TermSurf-owned
  code;
- the original PDF stream is captured as an `extensions::StreamContainer`, or
  the result proves that the next missing boundary is exactly the broad
  `PdfViewerStreamManager` owner target;
- normal HTML browsing still works;
- non-PDF binary responses are not handled as PDFs;
- all changes are committed to a Chromium branch and archived in
  `chromium/patches/issue-789/`;
- the main repo records the result and branch metadata.

#### Partial Criteria

Partial if the implementation avoids `//chrome/browser/plugins:impl` and builds
partway, but a narrower downstream PDF layer proves missing or too broad. Valid
Partial outcomes include:

- the response URL loader throttle builds, sees `application/pdf`, and creates
  the transferable loader, but cannot yet construct
  `extensions::StreamContainer`;
- `extensions::StreamContainer` cannot be constructed without importing a larger
  extension subsystem than expected;
- OOPIF PDF is disabled at runtime;
- the response path cannot emit the viewer-template payload without importing
  broad `MimeHandlerViewAttachHelper` dependencies;
- the stream reaches `PdfViewerStreamManager`, but the viewer frame cannot claim
  it because the MimeHandlerView renderer/container layer is still missing.

The result must name the exact boundary and the next experiment must target only
that boundary.

#### Failure Criteria

- The implementation adds `//chrome/browser/plugins:impl`.
- The implementation reuses Chrome's stock
  `PluginResponseInterceptorURLLoaderThrottle` by linking its owning target.
- The implementation silently imports Chrome's broad extension/browser stack.
- The implementation weakens PDF security checks to get past renderer or origin
  gates.
- The implementation reintroduces the data-wrapper fake PDF path as the primary
  solution.
- The implementation claims success because a screenshot looks different while
  the stream handoff logs are missing.
- Normal HTML browsing regresses.
- Non-PDF responses are intercepted as PDFs.
- The Chromium branch is left in an incoherent, unbuildable state without a
  recorded Partial/Failure explanation and cleanup plan.

**Result:** Pass

Experiment 2 built the first TermSurf-owned PDF stream handoff layer and avoided
the failed Issue 776 Experiment 8 dependency path.

Chromium branch:

```text
148.0.7778.97-issue-789-exp2
```

Chromium commit:

```text
bea8d5383ad9cd09a336da8edad788127eaa19e2 Build TermSurf PDF handoff
```

Patch archive:

```text
chromium/patches/issue-789/0001-Build-TermSurf-PDF-handoff.patch
```

#### Implemented Files

New Chromium files:

- `content/libtermsurf_chromium/ts_pdf_response_url_loader_throttle.h`
- `content/libtermsurf_chromium/ts_pdf_response_url_loader_throttle.cc`
- `content/libtermsurf_chromium/ts_pdf_stream_dispatch.h`
- `content/libtermsurf_chromium/ts_pdf_stream_dispatch.cc`

Modified Chromium files:

- `content/libtermsurf_chromium/BUILD.gn`
- `content/libtermsurf_chromium/ts_browser_client.h`
- `content/libtermsurf_chromium/ts_browser_client.cc`

Main repo metadata:

- `chromium/README.md`
- `chromium/patches/issue-789/0001-Build-TermSurf-PDF-handoff.patch`

#### What Changed

`TsBrowserClient::CreateURLLoaderThrottles()` now installs a TermSurf-owned PDF
URL loader throttle. The old data-wrapper `TsPdfNavigationThrottle` cancel path
is disabled by no longer registering that throttle from
`CreateThrottlesForNavigation()`.

The new `TsPdfResponseURLLoaderThrottle`:

- observes post-sniff `application/pdf` responses;
- logs request destination and OOPIF PDF state;
- emits a TermSurf-owned copy of Chromium's PDF embedder/template HTML;
- intercepts the response body;
- preserves the original PDF body as a `blink::mojom::TransferrableURLLoader`;
- dispatches that original stream to `TsDispatchPdfStream()`.

The new `TsDispatchPdfStream()` constructs and stores an
`extensions::StreamContainer` from the narrow
`//extensions/browser/mime_handler:stream_container` target. It deliberately
does not link `PdfViewerStreamManager`, because the only current owner is broad
`//chrome/browser/pdf:pdf`.

#### Dependency Evidence

Target-name checks:

```text
//extensions/browser/mime_handler:stream_container
//extensions/browser/mime_handler:stream_info

//chrome/browser/pdf:pdf
//chrome/browser/pdf:pdf_extension_test_utils
//chrome/browser/pdf:pdf_pref_names
//chrome/browser/pdf:pdf_test_utils
```

The TermSurf dependency surface includes:

```text
//extensions/browser/mime_handler:stream_container
```

It does not include:

```text
//chrome/browser/plugins:impl
//chrome/browser/extensions:extensions
//components/guest_view/browser
```

The expected warning about `enable_nacl = false` having no effect appeared in GN
output and is unrelated to this experiment.

#### Build Verification

Build command:

```bash
cd chromium/src
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
autoninja -C out/Default libtermsurf_chromium
```

Result:

```text
Build Succeeded: 16 steps
```

The previous Issue 776 Experiment 8 undefined symbols did not recur.

#### Runtime Verification

PDF run:

```text
logs/issue-789-exp2-20260527-180734/
```

The run used debug Wezboard, debug `web`, and the repo-built Roamium binary:

```text
/Users/ryan/dev/termsurf/webtui/target/debug/web
--browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium
http://localhost:9616/bitcoin.pdf
```

Relevant log evidence:

```text
[issue-789-exp2] old-pdf-navigation-throttle=disabled
[issue-789-exp2] pdf-response-throttle-created frame_tree_node_id=1 destination=3
[issue-789-exp2] pdf-throttle-installed frame_tree_node_id=1 destination=3
[issue-789-exp2] oopif-pdf-enabled value=true
[issue-789-exp2] pdf-response url=http://localhost:9616/bitcoin.pdf mime=application/pdf destination=3 oopif_pdf=true
[issue-789-exp2] viewer-template-emitted internal_id=C80C2291DE434362A590A9EF43A0494B bytes=536
[issue-789-exp2] stream-dispatch frame_tree_node_id=1 extension_id=mhjfbmdgcfjbbpaeojofohoefgiehjai handler_url=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/pdf/index.html internal_id=C80C2291DE434362A590A9EF43A0494B embedded=false
[issue-789-exp2] stream-container-captured internal_id=C80C2291DE434362A590A9EF43A0494B count=1 pdf-viewer-stream-manager=not-linked
```

The old wrapper cancel log did not appear for the PDF navigation.

Normal HTML smoke:

```text
logs/issue-789-exp2-html-20260527-180811/
```

This run installed the throttle but emitted no `pdf-response` or
`stream-dispatch` lines.

Non-PDF binary smoke:

```text
logs/issue-789-exp2-bin-20260527-180823/
```

This run installed the throttle but emitted no `pdf-response` or
`stream-dispatch` lines. The existing content-shell download-path log appeared,
which confirms the binary response was not handled as a PDF.

The known Issue 776 teardown crash recurred after screenshot/log capture. It did
not prevent the experiment evidence from being collected.

#### Conclusion

Experiment 2 succeeded at the intended build-first layer. TermSurf now has a
buildable, TermSurf-owned PDF response throttle that avoids
`//chrome/browser/plugins:impl`, emits the PDF viewer-template response, and
captures the original PDF response as an `extensions::StreamContainer`.

The remaining blocker is the final stream-manager integration. In Chromium 148,
`PdfViewerStreamManager` is still owned by broad `//chrome/browser/pdf:pdf`.
Experiment 3 should decide how to provide that layer without recreating the
Issue 776 Experiment 8 dependency explosion: split the upstream target, fork the
stream manager into TermSurf-owned code, or prove that a carefully bounded
Chrome PDF dependency can link safely.

### Experiment 3: Implement the PDF Stream Delegate

#### Description

Connect the Experiment 2 captured PDF stream to Chromium's supported PDF
embedder boundary.

Experiment 2 proved that TermSurf can intercept an `application/pdf` response,
emit the PDF viewer template payload, preserve the original PDF body as a
`blink::mojom::TransferrableURLLoader`, and store it as an
`extensions::StreamContainer`. Claude review found the important Chromium
boundary that this experiment should use next:

```text
//components/pdf/browser:interceptors
```

That target exposes the PDF embedder API Chromium provides specifically to avoid
layering directly on `//extensions/browser` or broad Chrome plugin code:

- `pdf::PdfStreamDelegate`;
- `pdf::PdfNavigationThrottle`;
- `pdf::PdfURLLoaderRequestInterceptor`.

Electron uses this shape: it implements an embedder-owned stream delegate,
installs Chromium's PDF navigation throttle, and installs Chromium's PDF URL
loader request interceptor. TermSurf should do the same with a small
TermSurf-owned stream store behind the delegate.

The primary Pass criterion is not "PDF visibly renders" yet. A Pass means the
viewer-template `internal_id` is extracted from the actual PDF viewer
navigation, the captured `StreamContainer` is claimed through a TermSurf
`pdf::PdfStreamDelegate`, and the original PDF bytes are served through
`pdf::PdfURLLoaderRequestInterceptor`, without importing the failed broad Chrome
dependency set from Issue 776 Experiment 8.

Visible PDF rendering is a stretch outcome. If it happens, record it. If it does
not, the result must name the next missing layer after successful stream
serving.

The old `PdfViewerStreamManager` question is now secondary. TermSurf still needs
state that maps unclaimed streams to later viewer requests, but that state
should support `pdf::PdfStreamDelegate`; it should not be framed as replacing
all of Chrome's PDF manager behavior.

#### Changes

1. Create a new Chromium branch from Experiment 2.

   ```bash
   cd chromium/src
   git checkout 148.0.7778.97-issue-789-exp2
   git checkout -b 148.0.7778.97-issue-789-exp3
   ```

   Update `chromium/README.md` in the main repo to list the new branch.

2. Audit Chromium's PDF delegate/interceptor flow before changing code.

   Run:

   ```bash
   cd chromium/src
   rg "class PdfStreamDelegate|PdfNavigationThrottle|PdfURLLoaderRequestInterceptor|ChromePdfStreamDelegate|PdfViewerStreamManager|StreamContainer" \
     components/pdf/browser \
     chrome/browser/pdf \
     chrome/browser/extensions \
     extensions/browser \
     content/libtermsurf_chromium
   gn desc out/Default //components/pdf/browser:interceptors deps
   gn desc out/Default //chrome/browser/pdf:pdf deps
   gn desc out/Default //extensions/browser/mime_handler:stream_container deps
   ```

   Record in the experiment result:
   - the exact `pdf::PdfStreamDelegate` methods in Chromium 148;
   - where Electron installs `pdf::PdfNavigationThrottle`;
   - where Electron installs `pdf::PdfURLLoaderRequestInterceptor`;
   - how `ChromePdfStreamDelegate` maps `internal_id` to a stream;
   - which Chrome-only call sites must not be copied into TermSurf;
   - whether `//components/pdf/browser:interceptors` depends only on bounded
     component/content/network targets.

3. Implement a TermSurf-owned `pdf::PdfStreamDelegate`.

   Add:

   ```text
   content/libtermsurf_chromium/ts_pdf_stream_delegate.h
   content/libtermsurf_chromium/ts_pdf_stream_delegate.cc
   ```

   The delegate must implement all Chromium 148 `pdf::PdfStreamDelegate` methods
   explicitly:
   - `MapToOriginalUrl(content::NavigationHandle&)`;
   - `GetStreamInfo(content::RenderFrameHost*)`;
   - `MaybeDeleteSandboxedStream(content::FrameTreeNodeId)`;
   - `ShouldAllowPdfFrameNavigation(content::NavigationHandle*)`;
   - `ShouldAllowPdfExtensionFrameNavigation(content::NavigationHandle*)`.

   The first two methods are the stream handoff:
   - `MapToOriginalUrl(...)` is the claim entrypoint. It should parse the actual
     PDF viewer navigation URL, extract the `internal_id`, claim the matching
     captured stream, and return the original PDF URL.
   - `GetStreamInfo(...)` is the serve entrypoint. It should return the claimed
     stream info for the PDF content frame so
     `pdf::PdfURLLoaderRequestInterceptor` can serve the original bytes.

   The three security methods must mirror Chromium's intended PDF policy as
   closely as TermSurf can support it. Do not return permissive defaults just to
   make the smoke test advance. If one method cannot be implemented safely, mark
   the experiment Partial and record the exact missing state.

   Required logs:

   ```text
   [issue-789-exp3] delegate-created ...
   [issue-789-exp3] map-to-original-url url=<viewer url> internal_id=<id> ...
   [issue-789-exp3] get-stream-info frame_tree_node_id=<id> ...
   [issue-789-exp3] maybe-delete-sandboxed-stream frame_tree_node_id=<id> ...
   [issue-789-exp3] allow-pdf-frame-navigation url=<url> allowed=<true|false> ...
   [issue-789-exp3] allow-pdf-extension-frame-navigation url=<url> allowed=<true|false> ...
   ```

4. Add a small TermSurf stream store behind the delegate.

   Add, if needed:

   ```text
   content/libtermsurf_chromium/ts_pdf_stream_store.h
   content/libtermsurf_chromium/ts_pdf_stream_store.cc
   ```

   The store is not a full `PdfViewerStreamManager` replacement. It exists only
   to support `pdf::PdfStreamDelegate`.

   Required behavior:
   - per-`WebContents` ownership, preferably via `content::WebContentsUserData`;
   - cleanup when the owning `WebContents` is destroyed;
   - unclaimed lookup by `(content::FrameTreeNodeId, internal_id)`;
   - claim transition from `(FrameTreeNodeId, internal_id)` to the relevant
     `content::RenderFrameHost*`;
   - claimed lookup by `RenderFrameHost*` for `GetStreamInfo(...)`;
   - one successful claim consumes or marks the stream so a mismatched later
     frame cannot steal it.

   Update:

   ```text
   content/libtermsurf_chromium/ts_pdf_stream_dispatch.cc
   content/libtermsurf_chromium/ts_pdf_stream_dispatch.h
   ```

   so `TsDispatchPdfStream(...)` stores the captured `StreamContainer` in this
   stream store instead of the Experiment 2 standalone static map.

   Required logs:

   ```text
   [issue-789-exp3] stream-store-created ...
   [issue-789-exp3] stream-container-added frame_tree_node_id=<id> internal_id=<id> ...
   [issue-789-exp3] stream-container-claim-request frame_tree_node_id=<id> internal_id=<id> ...
   [issue-789-exp3] stream-container-claimed render_frame_host=<ptr> internal_id=<id> ...
   [issue-789-exp3] stream-served original_url=<url> ...
   ```

5. Install Chromium's narrow PDF navigation throttle.

   Update `content/libtermsurf_chromium/ts_browser_client.cc/h` so
   `CreateThrottlesForNavigation(...)` installs:

   ```text
   pdf::PdfNavigationThrottle
   ```

   using a fresh TermSurf `TsPdfStreamDelegate`.

   This throttle should handle the PDF viewer's content-frame navigation. Do not
   revive the old data-wrapper navigation throttle or cancel the top-level PDF
   navigation.

   Add a note in the result explaining the expected flow:
   - Experiment 2's response throttle sees the original `application/pdf`
     response and swaps in the PDF viewer embedder/template HTML;
   - the viewer creates a PDF content frame that navigates using the emitted
     `internal_id`;
   - `pdf::PdfNavigationThrottle` calls
     `TsPdfStreamDelegate::MapToOriginalUrl(...)`;
   - the delegate claims the stored stream and returns the original PDF URL.

6. Install Chromium's narrow PDF URL loader request interceptor.

   Update `content/libtermsurf_chromium/ts_browser_client.cc/h` to implement or
   extend the Chromium 148 hook that installs URL loader request interceptors
   for a frame:

   ```text
   pdf::PdfURLLoaderRequestInterceptor::MaybeCreateInterceptor(...)
   ```

   using a fresh TermSurf `TsPdfStreamDelegate`.

   This is the bytes-serving half of the handoff. A claim without this
   interceptor can still leave a blank viewer. The experiment cannot Pass unless
   the logs prove that the interceptor requested stream info and served the
   captured PDF stream.

   If the Chromium 148 embedder hook name differs, record the exact method name
   in the result and keep the same behavior.

7. Avoid copying Chrome-only delegate dependencies.

   If `ChromePdfStreamDelegate` is used as the implementation reference, do not
   copy its Chrome-only dependencies directly. Replace or drop:
   - `Profile::FromBrowserContext(...)` and Chrome pref lookups;
   - `chrome/grit/pdf_resources.h` unless a bounded resource target is already
     available;
   - `extensions/browser/guest_view/...` non-OOPIF fallback behavior;
   - direct dependence on `chrome/browser/pdf/pdf_viewer_stream_manager.h`;
   - unrelated metrics, permissions, download, viewer UI, or Chrome profile
     code.

   If the delegate requires an injected script resource, either provide a
   bounded TermSurf-owned resource or return a clearly documented null/empty
   value and record the expected degraded behavior. Do not silently pull in
   `//chrome/browser/pdf:pdf` just to obtain one resource.

8. Keep the dependency gate strict.

   `content/libtermsurf_chromium` must not add:

   ```text
   //chrome/browser/plugins:impl
   //chrome/browser/extensions:extensions
   //components/guest_view/browser
   ```

   It also must not pull in the unresolved Issue 776 Experiment 8 link failures:
   - `ChromeNoStatePrefetchContentsDelegate`;
   - macOS ScreenCaptureKit symbols from Chrome WebRTC capture;
   - Chrome media permission symbols.

   Any new `chrome/browser/...` dependency must be listed in the result with its
   reason and `gn desc` evidence.

9. Preserve non-PDF behavior.

   Do not modify:
   - Wezboard;
   - Roamium Rust code;
   - `webtui`;
   - `termsurf.proto`;
   - the old data-wrapper navigation path except to keep it disabled;
   - normal HTML navigation;
   - non-PDF binary download behavior.

10. Format and archive.

    Run Chromium formatting on modified C++/GN files:

    ```bash
    cd chromium/src
    ../depot_tools/clang-format -i <modified .cc/.h files>
    export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
    gn format content/libtermsurf_chromium/BUILD.gn
    ```

    Build before committing the Chromium branch. After the branch commit,
    regenerate the patch archive:

    ```bash
    tmp_dir="$(mktemp -d ../../chromium/patches/issue-789.XXXXXX)"
    git format-patch 148.0.7778.97-issue-776-exp7..HEAD \
      -o "$tmp_dir"
    rm -rf ../../chromium/patches/issue-789
    mv "$tmp_dir" ../../chromium/patches/issue-789
    ```

    The archive should include Experiment 2 and Experiment 3 commits as the
    current Issue 789 patch stack.

#### Verification

1. Build `libtermsurf_chromium`.

   ```bash
   cd chromium/src
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   autoninja -C out/Default libtermsurf_chromium
   ```

   Pass requires a clean link. A build that reaches compile but fails at the
   final dylib link with the Issue 776 Experiment 8 symbol set is a Failure, not
   a Partial, unless the result identifies a smaller target split that would
   remove those symbols.

2. Inspect the dependency surface.

   ```bash
   gn desc out/Default //content/libtermsurf_chromium deps
   rg "//chrome/browser/plugins:impl|//chrome/browser/extensions:extensions|//components/guest_view/browser" \
     content/libtermsurf_chromium chrome/browser/pdf chrome/browser/plugins
   ```

   The forbidden targets must not appear as TermSurf dependencies.

3. Run the existing automated Bitcoin PDF smoke test.

   ```bash
   LOG_DIR="$PWD/logs/issue-789-exp3-$(date +%Y%m%d-%H%M%S)" \
   TERMSURF_PDF_SETTLE_SECONDS=8 \
   ./scripts/test-issue-776-pdf.sh http://localhost:9616/bitcoin.pdf
   ```

   Required log sequence:

   ```text
   [issue-789-exp2] pdf-response ...
   [issue-789-exp2] viewer-template-emitted internal_id=<id> ...
   [issue-789-exp3] stream-store-created ...
   [issue-789-exp3] stream-container-added frame_tree_node_id=<id> internal_id=<same id> ...
   [issue-789-exp3] map-to-original-url url=<viewer url> internal_id=<same id> ...
   [issue-789-exp3] stream-container-claim-request frame_tree_node_id=<id> internal_id=<same id> ...
   [issue-789-exp3] stream-container-claimed render_frame_host=<ptr> internal_id=<same id> ...
   [issue-789-exp3] get-stream-info frame_tree_node_id=<id> ...
   [issue-789-exp3] stream-served original_url=<url> ...
   ```

   The exact `internal_id` must match from template emission, through the actual
   URL passed to `MapToOriginalUrl(...)`, through claim and serve.

   At least one log line from each `pdf::PdfStreamDelegate` method should appear
   during the PDF smoke run, or the result must explain why a method did not
   run.

4. Capture screenshot output.

   The screenshot does not define Pass for this experiment, but it should be
   archived with the logs. Classify the visual result as one of:
   - PDF visibly rendered;
   - viewer shell loaded but PDF body not displayed;
   - blank/white surface;
   - download/error page;
   - renderer crash;
   - automation failure.

   If PDF visibly renders, mark that as a stretch success and record whether
   scrolling the first page works.

5. Run normal HTML smoke.

   ```bash
   LOG_DIR="$PWD/logs/issue-789-exp3-html-$(date +%Y%m%d-%H%M%S)" \
   TERMSURF_PDF_SETTLE_SECONDS=4 \
   ./scripts/test-issue-776-pdf.sh http://localhost:9616/index.html
   ```

   The PDF response, stream-store, delegate, and stream-served logs must not
   appear.

6. Run non-PDF binary smoke.

   ```bash
   LOG_DIR="$PWD/logs/issue-789-exp3-bin-$(date +%Y%m%d-%H%M%S)" \
   TERMSURF_PDF_SETTLE_SECONDS=4 \
   ./scripts/test-issue-776-pdf.sh http://localhost:9616/test.bin
   ```

   The PDF response, stream-store, delegate, and stream-served logs must not
   appear. Existing download behavior is acceptable.

7. Record the known teardown crash separately.

   The Issue 776 teardown crash is still out of scope unless it prevents logs or
   screenshots from being captured. If it recurs after evidence collection, note
   it but do not treat it as an Experiment 3 failure.

#### Pass Criteria

- `libtermsurf_chromium` builds and links.
- The implementation avoids the forbidden broad Chrome dependency set.
- The PDF response reaches the Experiment 2 response throttle.
- `pdf::PdfNavigationThrottle` and `pdf::PdfURLLoaderRequestInterceptor` are
  installed through TermSurf's browser client.
- A TermSurf `pdf::PdfStreamDelegate` implements all Chromium 148 delegate
  methods without permissive security stubs.
- The emitted viewer-template `internal_id` is stored in the Experiment 3 stream
  store.
- `MapToOriginalUrl(...)` receives a real viewer navigation URL containing the
  same `internal_id`.
- The viewer-side claim path requests and claims the same `internal_id`.
- `GetStreamInfo(...)` returns the captured `StreamContainer` for that claimed
  frame.
- The PDF bytes are served through the URL loader request interceptor, proven by
  a `stream-served` log.
- Normal HTML and non-PDF binary smoke tests do not trigger PDF stream-store,
  delegate, or serving behavior.
- The patch archive is regenerated under `chromium/patches/issue-789/`.

#### Partial Criteria

Partial if the implementation builds and stores streams in the new store, but
one downstream layer still prevents a successful claim or serve. Valid Partial
outcomes include:

- the viewer never requests the `internal_id`;
- the viewer requests a different `internal_id`;
- `MapToOriginalUrl(...)` claims the stream, but
  `PdfURLLoaderRequestInterceptor` is never invoked;
- `GetStreamInfo(...)` is invoked for a frame that does not match the claimed
  stream;
- the request path is gated by a missing extension API or PDF viewer private
  API;
- the stream is claimed and served, but the viewer still cannot render because
  `PdfHost`, PDF process routing, viewer-private bindings, or PDF viewer
  resources are missing;
- one `pdf::PdfStreamDelegate` security method cannot be implemented safely with
  TermSurf's available state.

The result must name the next exact missing layer and include the relevant logs.

#### Failure Criteria

- The implementation imports `//chrome/browser/plugins:impl`.
- The implementation imports broad Chrome extension/browser or guest-view
  infrastructure to make the stream lookup work.
- The build reproduces the Issue 776 Experiment 8 final-link failures without a
  smaller target-split explanation.
- The implementation weakens PDF origin, process, or renderer checks to force a
  claim.
- The implementation reintroduces the data-wrapper fake PDF path as the primary
  solution.
- The implementation claims Pass from screenshot differences without matching
  `stream-container-claimed` and `stream-served` logs.
- The PDF `internal_id` does not match from template emission through stream
  claim and serve.
- Normal HTML or non-PDF binary behavior regresses.

**Result:** Partial

Experiment 3 implemented the Chromium PDF delegate/interceptor shape, but the
viewer never issued the stream-claim navigation. The result is a coherent
Partial: the code builds, the PDF response reaches the TermSurf response
throttle, the stream is stored in a per-`WebContents` TermSurf stream store, and
non-PDF smoke tests do not trigger PDF stream-store behavior. The missing layer
is now narrower and clearer: TermSurf still lacks the MimeHandlerView
attach/container machinery that turns the `internalid` iframe in
`pdf_embedder.html` into the PDF extension/content-frame navigation that would
call `pdf::PdfNavigationThrottle` and `pdf::PdfURLLoaderRequestInterceptor`.

Chromium branch:

```text
148.0.7778.97-issue-789-exp3
```

Chromium commits:

```text
bea8d5383ad9cd09a336da8edad788127eaa19e2 Build TermSurf PDF handoff
332a28d1ba350 Wire PDF stream delegate
```

Patch archive:

```text
chromium/patches/issue-789/0001-Build-TermSurf-PDF-handoff.patch
chromium/patches/issue-789/0002-Wire-PDF-stream-delegate.patch
```

#### Implemented Files

New Chromium files:

- `content/libtermsurf_chromium/ts_pdf_stream_delegate.h`
- `content/libtermsurf_chromium/ts_pdf_stream_delegate.cc`
- `content/libtermsurf_chromium/ts_pdf_stream_store.h`
- `content/libtermsurf_chromium/ts_pdf_stream_store.cc`

Modified Chromium files:

- `content/libtermsurf_chromium/BUILD.gn`
- `content/libtermsurf_chromium/ts_browser_client.h`
- `content/libtermsurf_chromium/ts_browser_client.cc`
- `content/libtermsurf_chromium/ts_pdf_response_url_loader_throttle.cc`
- `content/libtermsurf_chromium/ts_pdf_stream_dispatch.cc`

Main repo metadata:

- `chromium/README.md`
- `chromium/patches/issue-789/0001-Build-TermSurf-PDF-handoff.patch`
- `chromium/patches/issue-789/0002-Wire-PDF-stream-delegate.patch`

#### What Changed

`TsPdfStreamDelegate` implements Chromium 148's `pdf::PdfStreamDelegate`
interface and delegates state lookup to `TsPdfStreamStore`.

`TsPdfStreamStore` is a TermSurf-owned, per-`WebContents` store for captured PDF
streams. It stores `extensions::StreamInfo` objects after Experiment 2 captures
the original PDF response, can claim a stream for a later PDF viewer/content
navigation, and is prepared to register the original transferable loader as a
subresource override when the PDF content navigation reaches
`ReadyToCommitNavigation()`.

`TsBrowserClient` now conditionally installs:

- `pdf::PdfNavigationThrottle`;
- `pdf::PdfURLLoaderRequestInterceptor`.

Those are installed only when a PDF stream store already has streams for the
`WebContents`. This avoids running the PDF delegate/interceptor path for normal
HTML and non-PDF binary navigations.

`TsPdfResponseURLLoaderThrottle` now changes the synthetic viewer-template
response MIME to `text/html`. Without this, content_shell treated the swapped
body as a PDF/download and never committed the embedder HTML.

#### Build Verification

Build command:

```bash
cd chromium/src
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
autoninja -C out/Default libtermsurf_chromium
```

Result:

```text
Build Succeeded: 5 steps
```

Dependency evidence:

```text
//components/pdf/browser:interceptors
//extensions/browser/mime_handler:stream_container
//extensions/browser/mime_handler:stream_info
```

The TermSurf dependency surface did not include:

```text
//chrome/browser/plugins:impl
//chrome/browser/extensions:extensions
//components/guest_view/browser
```

The expected warning about `enable_nacl = false` having no effect appeared in GN
output and is unrelated to this experiment.

#### Runtime Verification

PDF run:

```text
logs/issue-789-exp3-20260527-184836/
```

Relevant log evidence:

```text
[issue-789-exp2] pdf-response-throttle-created frame_tree_node_id=1 destination=3
[issue-789-exp2] pdf-throttle-installed frame_tree_node_id=1 destination=3
[issue-789-exp2] oopif-pdf-enabled value=true
[issue-789-exp2] pdf-response url=http://localhost:9616/bitcoin.pdf mime=application/pdf destination=3 oopif_pdf=true
[issue-789-exp2] viewer-template-emitted internal_id=B990F3BB4A1A47BC8190D4268101B4BF bytes=536
[issue-789-exp2] stream-dispatch frame_tree_node_id=1 extension_id=mhjfbmdgcfjbbpaeojofohoefgiehjai handler_url=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/pdf/index.html internal_id=B990F3BB4A1A47BC8190D4268101B4BF embedded=false
[issue-789-exp3] stream-store-created web_contents=0x75b06e000
[issue-789-exp3] stream-container-added frame_tree_node_id=1 internal_id=B990F3BB4A1A47BC8190D4268101B4BF count=1
```

Expected-but-missing logs:

```text
[issue-789-exp3] map-to-original-url ...
[issue-789-exp3] stream-container-claim-request ...
[issue-789-exp3] stream-container-claimed ...
[issue-789-exp3] get-stream-info ...
[issue-789-exp3] stream-served ...
```

Screenshot classification:

```text
viewer-template loaded but PDF body not displayed
```

The screenshot shows the white embedder surface with a small empty iframe area,
not a rendered PDF. That matches the log evidence: the embedder template loaded
and the original PDF stream was stored, but no viewer/content-frame navigation
claimed or served the stream.

Normal HTML smoke:

```text
logs/issue-789-exp3-html-20260527-184848/
```

The run installed the always-present Experiment 2 response throttle, but emitted
no `pdf-response`, `stream-store`, delegate claim, or `stream-served` lines.

Non-PDF binary smoke:

```text
logs/issue-789-exp3-bin-20260527-184856/
```

The run installed the always-present Experiment 2 response throttle, but emitted
no `pdf-response`, `stream-store`, delegate claim, or `stream-served` lines. The
existing content-shell download-path log appeared, confirming the binary was not
handled as a PDF.

The known Issue 776 teardown crash recurred after screenshot/log capture. It did
not prevent the experiment evidence from being collected.

#### Conclusion

Experiment 3 successfully moved TermSurf to the right Chromium API boundary:
`pdf::PdfStreamDelegate`, `pdf::PdfNavigationThrottle`, and
`pdf::PdfURLLoaderRequestInterceptor` are now the intended path, backed by a
TermSurf-owned stream store and the narrow
`//components/pdf/browser:interceptors` target.

The experiment did not reach stream claim or stream serving. The viewer never
issued the navigation that would call `MapToOriginalUrl(...)`. The screenshot
and logs both point to the same missing layer: TermSurf is still only emitting
the static `pdf_embedder.html` template. It has not implemented the
MimeHandlerView attach/container behavior that Chromium normally uses to notice
the `internalid` iframe, create or navigate the PDF extension/content frame, and
drive the delegate/interceptor flow.

Experiment 4 should focus on the minimal MimeHandlerView attach/container
equivalent needed for OOPIF PDF:

- detect the `internalid` iframe in the emitted embedder document;
- navigate/create the PDF extension frame at
  `chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/pdf/index.html`;
- preserve the internal id so the extension/content frame can claim the stored
  stream;
- keep using the delegate/interceptor/store built in Experiment 3 rather than
  returning to the broad Chrome plugin stack.

### Experiment 4: Make the Viewer Ask for the PDF

#### Description

Connect the blank PDF viewer shell to the stored PDF stream by reproducing the
smallest missing part of Chrome's MimeHandlerView attach flow.

In plain terms, Experiment 3 got the PDF file into TermSurf's hands, but the
viewer page never asked for it. The pane now shows a mostly blank white viewer
area because the static `pdf_embedder.html` wrapper creates an inert
`internalid` iframe. In Chrome, extra MimeHandlerView machinery notices that
iframe and turns it into the real PDF viewer/content-frame setup. TermSurf does
not have that machinery yet.

This experiment should first prove the exact missing callback, then add the
smallest TermSurf-owned connection that gets the inert iframe to load the PDF
extension viewer. It must build on the Experiment 3 delegate/store path. Do not
replace it with a new wrapper, do not return to Chrome's broad plugin stack, and
do not implement full MimeHandlerView/GuestView unless the minimal path proves
impossible.

The important correction from review: the attach shim must not navigate the
iframe directly to the stored stream URL. Chrome's expected frame structure is:

1. original embedder frame serves `pdf_embedder.html`;
2. the child iframe navigates to the PDF extension handler URL
   (`chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/pdf/index.html`) with
   the stored stream/internal id;
3. the PDF extension viewer JS runs in that child frame;
4. the viewer JS asks `chrome.mimeHandlerPrivate` / `pdf_viewer_private` for
   stream information;
5. only then does an inner PDF content/plugin navigation reach the stored stream
   URL and allow Experiment 3's delegate/interceptor path to claim and serve the
   bytes.

This experiment owns steps 1-3 and the stream-claim bookkeeping needed to make a
later step 4 possible. It may discover that the next hard blocker is the missing
`pdf_viewer_private` API. In that case the correct result is Pass for Experiment
4 if the extension-viewer attach layer is proven; Partial is reserved for cases
where the attach layer itself remains incomplete. Visible PDF rendering is not
expected from this experiment.

#### Changes

1. Create a new Chromium branch from Experiment 3.

   ```bash
   cd chromium/src
   git checkout 148.0.7778.97-issue-789-exp3
   git checkout -b 148.0.7778.97-issue-789-exp4
   ```

   Update `chromium/README.md` in the main repo to list the new branch.

2. Add focused tracing around the missing connection.

   Add logs, gated only by issue tags for now, at these points:
   - when Experiment 2 emits the `pdf_embedder.html` payload;
   - when `TsPdfStreamStore` stores a stream;
   - when any navigation starts/commits in the same `WebContents` after the PDF
     stream is stored;
   - when the embedder frame commits after the stream is stored;
   - when child frames are created under that embedder frame;
   - when a child frame commits at the PDF extension handler URL;
   - when `pdf::PdfNavigationThrottle` is installed;
   - when `TsPdfStreamDelegate::MapToOriginalUrl(...)` runs;
   - when `TsPdfStreamDelegate::GetStreamInfo(...)` runs;
   - when `TsPdfStreamStore::ReadyToCommitNavigation(...)` attempts
     `RegisterSubresourceOverride(...)`.

   The result must answer:
   - Did the viewer shell commit as `text/html`?
   - Did the child iframe inside `pdf_embedder.html` exist?
   - Did that child iframe navigate to the PDF extension handler URL?
   - Did the extension-frame origin match the canonical PDF extension origin?
   - Did the viewer JS attempt to ask for stream information?
   - Did any later navigation URL equal the stored stream URL?
   - Did `MapToOriginalUrl(...)` run?
   - Did `GetStreamInfo(...)` run?

3. Add a minimal PDF extension attach shim.

   Preferred implementation: add a browser-side observer owned by the
   `WebContents` / `TsPdfStreamStore` path. After a PDF stream is stored, the
   observer waits for the corresponding embedder document to commit and for the
   child iframe from `pdf_embedder.html` to exist. Then it navigates that child
   frame to the PDF extension handler URL with the stream/internal id encoded in
   the way the viewer expects.

   Do not use a fire-and-forget UI task immediately after storing the stream.
   `TsPdfStreamStore::AddStreamContainer(...)` runs before `pdf_embedder.html`
   has necessarily committed in the renderer, so the child iframe may not exist
   yet.

   The stored `FrameTreeNodeId` identifies the original embedder frame, not the
   child iframe that must be navigated. The shim must:
   - look up the stored embedder frame;
   - wait until the embedder has committed the `pdf_embedder.html` document;
   - find the child iframe created by that document. The iframe is inside a
     closed shadow root, so find it through the browser-side frame tree, not by
     injected JavaScript;
   - navigate that child frame to `StreamContainer::handler_url()` with the
     stream/internal id, not to `StreamContainer::stream_url()`.

   Use the existing stored data:
   - stored frame tree node id;
   - `extensions::StreamContainer::handler_url()`;
   - `extensions::StreamContainer::stream_url()`, for later delegate matching
     only;
   - `internal_id`.

   The navigation target must be the PDF handler URL, not the original PDF URL
   and not the stored stream URL. The stored stream URL should only appear later
   if the PDF extension viewer JS successfully asks for stream information and
   triggers the inner PDF content/plugin navigation.

   Required logs:

   ```text
   [issue-789-exp4] attach-watch embedder_frame_tree_node_id=<id> internal_id=<id> stream_url=<url> handler_url=<url>
   [issue-789-exp4] attach-child-found embedder_frame_tree_node_id=<id> child_frame_tree_node_id=<id> internal_id=<id>
   [issue-789-exp4] attach-navigate embedder_frame_tree_node_id=<id> child_frame_tree_node_id=<id> target=<handler_url>
   [issue-789-exp4] attach-handler-committed child_frame_tree_node_id=<id> origin=<origin> url=<handler_url>
   ```

   If there is no safe frame to navigate, do not guess blindly. Record exactly
   which frame lookup failed and mark the experiment Partial.

4. Record the stream-claim transition explicitly.

   Experiment 3 stores unclaimed streams by the original embedder identity and
   `internal_id`. Chrome's delegate later looks up claimed streams by the
   relevant embedder `RenderFrameHost`, using the frame relationship around the
   PDF extension/content frames.

   This experiment must make that transition observable. When the child iframe
   commits at the PDF extension handler URL, record which original embedder
   frame owns the stream and whether the stream is now claimable by the RFH key
   that `TsPdfStreamDelegate::MapToOriginalUrl(...)` will use later.

   Required logs:

   ```text
   [issue-789-exp4] stream-claim-ready internal_id=<id> embedder_frame_tree_node_id=<id> embedder_rfh=<ptr-or-id>
   [issue-789-exp4] stream-claim-missing internal_id=<id> reason=<reason>
   ```

   If Experiment 3 already implemented the RFH-keyed transition, keep it and add
   logs proving where it happens. If it did not, add the smallest transition
   needed for the later delegate lookup. Do not duplicate stream serving.

5. Keep Experiment 3 as the stream handoff owner.

   Do not duplicate the stream claim or stream serving logic. The expected
   minimum sequence after the attach shim is:

   ```text
   [issue-789-exp3] stream-container-added ...
   [issue-789-exp4] attach-watch ...
   [issue-789-exp4] attach-child-found ...
   [issue-789-exp4] attach-navigate ... target=<handler_url>
   [issue-789-exp4] attach-handler-committed ... origin=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai
   [issue-789-exp4] stream-claim-ready ...
   ```

   If `pdf_viewer_private` is not present yet, the sequence may stop there. That
   is a valid Experiment 4 Pass if the extension-viewer attach layer is proven.
   The next experiment should then target the minimal viewer-private API
   surface.

   If the viewer-private path already works or is small enough to include, the
   extended sequence is:

   ```text
   [issue-789-exp3] map-to-original-url ... internal_id=<same id>
   [issue-789-exp3] stream-container-claimed ... internal_id=<same id>
   [issue-789-exp3] get-stream-info ...
   [issue-789-exp3] stream-served ...
   ```

6. Preserve dependency boundaries.

   `content/libtermsurf_chromium` must still avoid:

   ```text
   //chrome/browser/plugins:impl
   //chrome/browser/extensions:extensions
   //components/guest_view/browser
   ```

   Do not copy Chrome's full `MimeHandlerViewAttachHelper`,
   `MimeHandlerViewGuest`, or GuestView stack in this experiment. If the minimal
   attach shim cannot work without those pieces, record that as the Partial
   result and design the next experiment around the smallest specific missing
   piece.

7. Preserve non-PDF behavior.

   Normal HTML and non-PDF binary navigations must not run the attach shim. They
   may still create the always-present Experiment 2 response throttle, but they
   must not emit:

   ```text
   [issue-789-exp4] attach-watch ...
   [issue-789-exp4] attach-child-found ...
   [issue-789-exp4] attach-navigate ...
   [issue-789-exp3] stream-container-added ...
   [issue-789-exp3] stream-served ...
   ```

8. Log sandbox cleanup/store support.

   If the delegate has a TermSurf equivalent of
   `MaybeDeleteSandboxedStream(...)`, log whether it can query and delete
   unclaimed streams by `FrameTreeNodeId`.

   Required logs if that path exists:

   ```text
   [issue-789-exp4] sandbox-stream-check frame_tree_node_id=<id> contains_unclaimed=<true|false>
   [issue-789-exp4] sandbox-stream-delete frame_tree_node_id=<id> deleted=<true|false>
   ```

   If the path does not exist yet, record that explicitly in the result. Do not
   weaken PDF sandbox behavior silently.

9. Format, build, archive.

   Run Chromium formatting on modified C++/GN files:

   ```bash
   cd chromium/src
   ../depot_tools/clang-format -i <modified .cc/.h files>
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   gn format content/libtermsurf_chromium/BUILD.gn
   ```

   Build before committing the Chromium branch:

   ```bash
   autoninja -C out/Default libtermsurf_chromium
   ```

   After the Chromium branch commit, regenerate the Issue 789 patch archive from
   the last buildable PDF base:

   ```bash
   tmp_dir="$(mktemp -d ../../chromium/patches/issue-789.XXXXXX)"
   git format-patch 148.0.7778.97-issue-776-exp7..HEAD \
     -o "$tmp_dir"
   rm -rf ../../chromium/patches/issue-789
   mv "$tmp_dir" ../../chromium/patches/issue-789
   ```

#### Verification

1. Build `libtermsurf_chromium`.

   ```bash
   cd chromium/src
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   autoninja -C out/Default libtermsurf_chromium
   ```

2. Check dependencies.

   ```bash
   gn desc out/Default //content/libtermsurf_chromium deps
   rg "//chrome/browser/plugins:impl|//chrome/browser/extensions:extensions|//components/guest_view/browser" \
     content/libtermsurf_chromium chrome/browser/pdf chrome/browser/plugins
   ```

   Forbidden targets must not appear as TermSurf dependencies.

3. Run the automated Bitcoin PDF smoke test.

   ```bash
   LOG_DIR="$PWD/logs/issue-789-exp4-$(date +%Y%m%d-%H%M%S)" \
   TERMSURF_PDF_SETTLE_SECONDS=8 \
   ./scripts/test-issue-776-pdf.sh http://localhost:9616/bitcoin.pdf
   ```

   Required log sequence for Pass:

   ```text
   [issue-789-exp2] viewer-template-emitted internal_id=<id> ...
   [issue-789-exp3] stream-container-added ... internal_id=<same id> ...
   [issue-789-exp4] attach-watch ... internal_id=<same id> ...
   [issue-789-exp4] attach-child-found ... internal_id=<same id> ...
   [issue-789-exp4] attach-navigate ... target=<handler_url> ...
   [issue-789-exp4] attach-handler-committed ... origin=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai ...
   [issue-789-exp4] stream-claim-ready ... internal_id=<same id> ...
   ```

   If the run also reaches the stored PDF bytes, record the extended sequence:

   ```text
   [issue-789-exp3] map-to-original-url ... internal_id=<same id> ...
   [issue-789-exp3] stream-container-claimed ... internal_id=<same id> ...
   [issue-789-exp3] get-stream-info ...
   [issue-789-exp3] stream-served ...
   ```

4. Capture screenshot output.

   Classify the screenshot as one of:
   - PDF visibly rendered;
   - PDF extension viewer frame loaded but page still blank;
   - PDF plugin/content frame loaded but page still blank;
   - viewer shell only, no claim;
   - download/error page;
   - renderer crash;
   - automation failure.

   If the PDF visibly renders, also test whether the first page can scroll.

5. Run normal HTML smoke.

   ```bash
   LOG_DIR="$PWD/logs/issue-789-exp4-html-$(date +%Y%m%d-%H%M%S)" \
   TERMSURF_PDF_SETTLE_SECONDS=4 \
   ./scripts/test-issue-776-pdf.sh http://localhost:9616/index.html
   ```

   No Exp 3 stream-store logs or Exp 4 attach logs should appear. Confirm an
   explicit attach counter or grep result shows zero attach attempts:

   ```text
   [issue-789-exp4] attach-attempt-count count=0 kind=html
   ```

6. Run non-PDF binary smoke.

   ```bash
   LOG_DIR="$PWD/logs/issue-789-exp4-bin-$(date +%Y%m%d-%H%M%S)" \
   TERMSURF_PDF_SETTLE_SECONDS=4 \
   ./scripts/test-issue-776-pdf.sh http://localhost:9616/test.bin
   ```

   No Exp 3 stream-store logs or Exp 4 attach logs should appear. Existing
   content-shell download behavior is acceptable. Confirm an explicit attach
   counter or grep result shows zero attach attempts:

   ```text
   [issue-789-exp4] attach-attempt-count count=0 kind=non_pdf_binary
   ```

7. Record the known teardown crash separately.

   The Issue 776 teardown crash remains out of scope unless it prevents logs or
   screenshots from being captured.

#### Pass Criteria

- `libtermsurf_chromium` builds and links.
- The forbidden broad Chrome dependency set is still absent.
- A PDF response stores the original PDF stream.
- The new attach shim finds the child iframe created by `pdf_embedder.html`.
- The attach shim navigates that child iframe to the PDF extension handler URL,
  not to the stored stream URL.
- The child frame commits at the canonical PDF extension origin.
- The stored stream is claim-ready for the original embedder frame /
  `internal_id`.
- If `pdf_viewer_private` is still missing, the result records that as the next
  exact missing layer. `stream-served` is not required for Pass in that case.
- If `pdf_viewer_private` is available or implemented in this experiment, the
  extended delegate sequence reaches `stream-served`.
- HTML and non-PDF binary smoke tests do not run the attach shim.
- The patch archive is regenerated under `chromium/patches/issue-789/`.

#### Partial Criteria

Partial if the build succeeds but the extension-viewer attach layer is still
incomplete. Valid Partial outcomes include:

- the embedder document loads, but no child iframe exists to navigate;
- the child iframe exists, but browser-side navigation to the handler URL is
  blocked;
- the handler URL commits, but the committed origin is not the canonical PDF
  extension origin;
- `MapToOriginalUrl(...)` runs but cannot match the stored stream URL;
- the stream is claimed but `GetStreamInfo(...)` never runs;
- `GetStreamInfo(...)` runs but the PDF plugin/content frame still stays blank;
- the minimal attach shim proves impossible without a specific
  MimeHandlerViewContainerManager or PDF viewer private API piece.

The result must name the exact next missing piece and cite the log lines.

#### Failure Criteria

- The experiment reintroduces the old data-wrapper fake PDF solution.
- The experiment bypasses Experiment 3's delegate/store/interceptor path.
- The attach shim navigates the child iframe directly to the stream URL and
  calls that a Pass.
- The implementation imports `//chrome/browser/plugins:impl`.
- The implementation imports broad Chrome extension/browser or GuestView
  infrastructure without recording a Partial and redesigning around that scope.
- The attach shim runs for normal HTML or non-PDF binary navigations.
- The implementation claims visible PDF rendering or byte serving without either
  a `stream-served` log or an explicit explanation that the experiment stopped
  at the extension-viewer attach layer.
- The PDF `internal_id` does not match from template emission through attach,
  claim, and serve.

**Result:** Pass

Implemented on Chromium branch `148.0.7778.97-issue-789-exp4`.

Experiment 4 successfully connected the inert `pdf_embedder.html` child iframe
to the PDF extension viewer frame without pulling in Chrome's broad
MimeHandlerView, GuestView, or browser extension stacks.

Verification artifacts:

```text
logs/issue-789-exp4-20260527-205225/
logs/issue-789-exp4-html-20260527-205315/
logs/issue-789-exp4-bin-20260527-205327/
```

The PDF smoke run produced the required attach-layer sequence:

```text
[issue-789-exp2] viewer-template-emitted internal_id=63BA94E58433D02A0F615F680678E198 ...
[issue-789-exp3] stream-container-added frame_tree_node_id=1 internal_id=63BA94E58433D02A0F615F680678E198 ...
[issue-789-exp4] attach-watch embedder_frame_tree_node_id=1 internal_id=63BA94E58433D02A0F615F680678E198 ...
[issue-789-exp4] stream-claim-ready internal_id=63BA94E58433D02A0F615F680678E198 embedder_frame_tree_node_id=1 ...
[issue-789-exp4] attach-child-found embedder_frame_tree_node_id=1 child_frame_tree_node_id=2 internal_id=63BA94E58433D02A0F615F680678E198
[issue-789-exp4] attach-navigate embedder_frame_tree_node_id=1 child_frame_tree_node_id=2 target=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/pdf/index.html
[issue-789-exp4] attach-handler-committed child_frame_tree_node_id=2 origin=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai url=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/pdf/index.html
```

The forbidden dependency check passed:
`gn desc out/Default //content/libtermsurf_chromium deps` contained none of:

```text
//chrome/browser/plugins:impl
//chrome/browser/extensions:extensions
//components/guest_view/browser
```

The normal HTML smoke and non-PDF binary smoke produced no Exp 3 stream-store
logs and no Exp 4 attach logs, proving the attach shim stays gated to PDF stream
loads.

The PDF screenshot did not visibly render the Bitcoin PDF as a finished viewer.
It showed the extension attach path getting farther than the previous blank
viewer state, but not a complete PDF document. The logs also did not reach
`[issue-789-exp3] stream-served`. That is acceptable for Experiment 4 because
the experiment's revised Pass criteria stop at proving the extension-viewer
attach layer and identifying the next missing layer.

#### Conclusion

Experiment 4 proved that TermSurf can replace the first missing MimeHandlerView
attach step with a narrow TermSurf-owned browser-side shim: claim the stored PDF
stream when the embedder commits, observe the `about:blank` child iframe created
by `pdf_embedder.html`, and navigate that child frame to the PDF extension
handler URL.

The next missing piece is no longer "how does the blank iframe become the PDF
viewer?" It is the viewer-private API surface that lets the PDF extension viewer
obtain stream information and drive the inner PDF content/plugin frame to the
stored stream URL. The next experiment should implement the smallest TermSurf
equivalent of `chrome.pdfViewerPrivate.getStreamInfo(...)` /
`chrome.mimeHandlerPrivate.getStreamInfo(...)`, backed by `TsPdfStreamStore`.

### Experiment 5: Provide Viewer Stream Info

#### Description

Experiment 4 proved that TermSurf can attach the `pdf_embedder.html` child
iframe to the PDF extension viewer frame. The next missing boundary is the
viewer-private API call that Chrome normally provides to that extension frame.

The PDF viewer JS has two possible private API paths. It calls:

```text
chrome.pdfViewerPrivate.getStreamInfo(callback)
```

when `pdfOopifEnabled` is present on `pdf/index.html`, and falls back to:

```text
chrome.mimeHandlerPrivate.getStreamInfo(callback)
```

when OOPIF PDF is not enabled. TermSurf currently serves the viewer resources
but does not provide either JavaScript API surface to the extension frame. A
browser-side Mojo binder alone is not enough: the PDF viewer can only call the
API if the renderer environment creates the `chrome.mimeHandlerPrivate` or
`chrome.pdfViewerPrivate` namespace first.

For Experiment 5, prefer the `mimeHandlerPrivate` path. Audit the PDF viewer
template replacements and keep `pdfOopifEnabled` absent or false unless the code
proves that the current viewer requires the `pdfViewerPrivate` path. The
existing Chrome `pdfViewerPrivate` implementation lives under broad
`chrome/browser/extensions` infrastructure and is not an acceptable first
implementation for TermSurf.

As a result, the viewer frame currently commits, but the viewer cannot obtain
the stored `streamUrl`, `originalUrl`, `tabId`, or `embedded` values, so it
cannot create or drive the inner PDF content/plugin frame that should navigate
to the stream URL.

This experiment adds the smallest TermSurf-owned renderer-plus-browser
stream-info API shim needed for the viewer JS to receive `TsPdfStreamStore` data
and attempt the inner PDF content/plugin navigation. The expected proof point is
that the logs advance past Experiment 4's `attach-handler-committed`, show a
renderer-side viewer API call, return stream info from the browser-side TermSurf
provider, and either show the viewer attempting the next stream/plugin step or
identify the next missing PDF plugin/renderer layer.

Reaching Experiment 3's `stream-served` path is a stretch outcome for Experiment
5, not the required Pass line. If stream info is returned correctly and the
viewer advances to a later PDF/plugin step, the experiment can Pass or Partial
depending on the remaining missing layer.

The implementation should copy Electron's strategy, not Chrome's whole stack:
mirror only the pieces the PDF viewer needs, backed by TermSurf's existing
store/delegate/interceptor path. Do not import Chrome's full
`chrome/browser/extensions` API infrastructure, GuestView, or MimeHandlerView.

#### Changes

1. Create a new Chromium branch from Experiment 4.

   ```bash
   cd chromium/src
   git checkout 148.0.7778.97-issue-789-exp4
   git checkout -b 148.0.7778.97-issue-789-exp5
   ```

   Update `chromium/README.md` in the main repo to list the new branch.

2. Inspect and choose the narrow API hook.

   Audit these existing Chromium surfaces before implementation:

   ```text
   extensions/common/api/mime_handler.mojom
   extensions/common/api/mime_handler_private.idl
   chrome/common/extensions/api/pdf_viewer_private.idl
   extensions/renderer/resources/mime_handler_private_custom_bindings.js
   chrome/browser/resources/pdf/pdf_viewer.ts
   chrome/browser/resources/pdf/browser_api.ts
   chrome/browser/resources/pdf/pdf_viewer_wrapper.ts
   content/libtermsurf_chromium/ts_browser_client.cc
   content/libtermsurf_chromium/ts_pdf_stream_store.{h,cc}
   ```

   Preferred implementation order:
   - audit the viewer resource/template state and confirm whether
     `pdfOopifEnabled` is present. Prefer making it absent/false so the viewer
     calls `chrome.mimeHandlerPrivate.getStreamInfo(...)`;
   - add a renderer-side TermSurf shim/custom binding that creates
     `chrome.mimeHandlerPrivate.getStreamInfo(...)` and, if needed,
     `setPdfPluginAttributes(...)` for the PDF extension viewer frame;
   - back that renderer shim with the smallest browser-side TermSurf service
     bound through
     `TsBrowserClient::RegisterBrowserInterfaceBindersForFrame(...)` or the
     closest narrow per-frame binder available in Chromium 148;
   - call the shell/content-shell base binder registration first, then gate the
     TermSurf binder to the PDF extension origin;
   - do not try to use Chrome's generated `pdfViewerPrivate` implementation as
     the first path. If the viewer cannot be kept on `mimeHandlerPrivate`,
     record Partial and design a deliberate renderer API-generation experiment.

3. Add a TermSurf stream-info provider backed by `TsPdfStreamStore`.

   The provider must return the stream attached to the PDF extension frame that
   committed in Experiment 4. It must not copy Chrome's
   `MimeHandlerViewGuest`-based service shape. TermSurf deliberately has no
   GuestView, so the provider should derive the owning embedder frame from the
   extension frame's parent `RenderFrameHost` and call into `TsPdfStreamStore`
   rather than storing duplicate stream state.

   Required data returned to the viewer:
   - `streamUrl`: `extensions::StreamContainer::stream_url()`;
   - `originalUrl`: `extensions::StreamContainer::original_url()`;
   - `tabId`: `-1` for now;
   - `embedded`: the stored `StreamContainer::embedded()` value;
   - `mimeType`: `application/pdf` for the `mimeHandlerPrivate` shape;
   - `responseHeaders`: best-effort header map for the `mimeHandlerPrivate`
     shape. Empty is acceptable for Experiment 5 if headers are unavailable.

   `tabId` must be `-1`. Do not return a real tab id unless the experiment also
   wires the viewer's follow-up `chrome.tabs.get(...)` expectations. A real tab
   id risks sending the viewer down a Chrome tabs API path TermSurf does not
   provide.

   Plugin attributes must also be accepted if the viewer calls
   `setPdfPluginAttributes(...)`. Store them on the existing `StreamContainer`,
   matching the behavior of
   `extensions::MimeHandlerServiceImpl::SetPdfPluginAttributes(...)`.

   Required logs:

   ```text
   [issue-789-exp5] viewer-template pdf_oopif_enabled=<true|false|absent>
   [issue-789-exp5] viewer-api-installed frame_tree_node_id=<extension_frame> api=<mimeHandlerPrivate|pdfViewerPrivate|none> result=<ok|wrong-origin|unsupported>
   [issue-789-exp5] viewer-api-call frame_tree_node_id=<extension_frame> api=<mimeHandlerPrivate|pdfViewerPrivate> method=<getStreamInfo|setPdfPluginAttributes>
   [issue-789-exp5] stream-info-service-bound frame_tree_node_id=<extension_frame> embedder_frame_tree_node_id=<embedder_frame>
   [issue-789-exp5] get-stream-info frame_tree_node_id=<extension_frame> internal_id=<id> stream_url=<url> original_url=<url> result=<ok|no-store|no-stream>
   [issue-789-exp5] set-pdf-plugin-attributes frame_tree_node_id=<extension_frame> internal_id=<id> result=<ok|no-stream|invalid>
   ```

   The renderer-side `viewer-api-call` log is required. Without it, the result
   cannot distinguish "the viewer did not call a private API" from "the
   browser-side stream-info service failed."

4. Preserve Experiment 3 and 4 ownership boundaries.

   Do not move stream storage, stream claiming, or stream serving out of
   `TsPdfStreamStore`, `TsPdfStreamDelegate`, or
   `pdf::PdfURLLoaderRequestInterceptor`.

   Expected flow after Experiment 5:

   ```text
   [issue-789-exp2] viewer-template-emitted internal_id=<id> ...
   [issue-789-exp3] stream-container-added ... internal_id=<same id> ...
   [issue-789-exp4] attach-handler-committed ... origin=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai ...
   [issue-789-exp5] viewer-template pdf_oopif_enabled=<false|absent>
   [issue-789-exp5] viewer-api-installed ... api=mimeHandlerPrivate result=ok
   [issue-789-exp5] viewer-api-call ... api=mimeHandlerPrivate method=getStreamInfo
   [issue-789-exp5] stream-info-service-bound ...
   [issue-789-exp5] get-stream-info ... internal_id=<same id> result=ok
   [issue-789-exp5] viewer-attempted-stream-navigation ...
   ```

   Stretch flow if the next layer is already present:

   ```text
   [issue-789-exp3] map-to-original-url ... internal_id=<same id> ...
   [issue-789-exp3] stream-container-claimed ... internal_id=<same id> ...
   [issue-789-exp3] get-stream-info ...
   [issue-789-exp3] stream-served ...
   ```

5. Add focused failure logging for the viewer API choice.

   The result must answer:
   - Is `pdfOopifEnabled` present, true, false, or absent in the emitted viewer
     template?
   - Did the renderer install a usable `chrome.mimeHandlerPrivate` shim for the
     PDF extension frame?
   - Did the PDF viewer call `chrome.mimeHandlerPrivate.getStreamInfo(...)` or
     `chrome.pdfViewerPrivate.getStreamInfo(...)`?
   - Did the browser-side binder run for the PDF extension frame?
   - Did the stream-info provider find the claimed stream from Experiment 4?
   - Did the viewer create a later navigation to the stored stream URL?
   - Did `MapToOriginalUrl(...)` run for that later navigation?
   - Did `stream-served` run?

   If the viewer never calls either API, the likely missing piece is renderer
   API exposure/custom bindings rather than browser stream data. If the viewer
   calls `pdfViewerPrivate`, the likely missing piece is the OOPIF template mode
   or a deliberate TermSurf-owned `pdfViewerPrivate` shim. Record the exact
   branch taken.

6. Preserve dependency boundaries.

   `content/libtermsurf_chromium` must still avoid:

   ```text
   //chrome/browser/plugins:impl
   //chrome/browser/extensions:extensions
   //components/guest_view/browser
   ```

   Also avoid importing Chrome's generated `pdfViewerPrivate` extension function
   implementation. That implementation pulls broad Chrome extension browser
   infrastructure. If the viewer cannot be made to use `mimeHandlerPrivate`,
   record Partial and redesign around a deliberate TermSurf-owned
   `pdfViewerPrivate` shim or renderer API-generation step.

7. Preserve non-PDF behavior.

   Normal HTML and non-PDF binary navigations must not bind the stream-info
   provider and must not emit any `[issue-789-exp5]` logs.

8. Format, build, archive.

   Run Chromium formatting on modified C++/GN files:

   ```bash
   cd chromium/src
   ../depot_tools/clang-format -i <modified .cc/.h files>
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   gn format content/libtermsurf_chromium/BUILD.gn
   ```

   Build before committing the Chromium branch:

   ```bash
   autoninja -C out/Default libtermsurf_chromium
   ```

   After the Chromium branch commit, regenerate the Issue 789 patch archive from
   the last buildable PDF base:

   ```bash
   tmp_dir="$(mktemp -d ../../chromium/patches/issue-789.XXXXXX)"
   git format-patch 148.0.7778.97-issue-776-exp7..HEAD \
     -o "$tmp_dir"
   rm -rf ../../chromium/patches/issue-789
   mv "$tmp_dir" ../../chromium/patches/issue-789
   ```

#### Verification

1. Build `libtermsurf_chromium`.

   ```bash
   cd chromium/src
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   autoninja -C out/Default libtermsurf_chromium
   ```

2. Check dependencies.

   ```bash
   gn desc out/Default //content/libtermsurf_chromium deps
   rg "//chrome/browser/plugins:impl|//chrome/browser/extensions:extensions|//components/guest_view/browser" \
     content/libtermsurf_chromium chrome/browser/pdf chrome/browser/plugins
   ```

   Forbidden targets must not appear as TermSurf dependencies.

3. Run the automated Bitcoin PDF smoke test.

   ```bash
   LOG_DIR="$PWD/logs/issue-789-exp5-$(date +%Y%m%d-%H%M%S)" \
   TERMSURF_PDF_SETTLE_SECONDS=10 \
   ./scripts/test-issue-776-pdf.sh http://localhost:9616/bitcoin.pdf
   ```

   Required log sequence for Pass:

   ```text
   [issue-789-exp4] attach-handler-committed ... origin=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai ...
   [issue-789-exp5] viewer-template pdf_oopif_enabled=<false|absent>
   [issue-789-exp5] viewer-api-installed ... api=mimeHandlerPrivate result=ok
   [issue-789-exp5] viewer-api-call ... api=mimeHandlerPrivate method=getStreamInfo
   [issue-789-exp5] stream-info-service-bound ...
   [issue-789-exp5] get-stream-info ... result=ok
   ```

   Stretch log sequence if the next PDF/plugin layer is already present:

   ```text
   [issue-789-exp3] map-to-original-url ... internal_id=<same id> ...
   [issue-789-exp3] stream-container-claimed ... internal_id=<same id> ...
   [issue-789-exp3] get-stream-info ...
   [issue-789-exp3] stream-served ...
   ```

4. Capture screenshot output.

   Classify the screenshot as one of:
   - PDF visibly rendered;
   - PDF plugin/content frame loaded but page still blank;
   - viewer private API returned stream info, but no stream navigation occurred;
   - viewer private API never fired;
   - viewer API shim missing from the renderer;
   - viewer called the unexpected `pdfViewerPrivate` path;
   - renderer crash;
   - automation failure.

   If the PDF visibly renders, also test whether the first page scrolls.

5. Run normal HTML smoke.

   ```bash
   LOG_DIR="$PWD/logs/issue-789-exp5-html-$(date +%Y%m%d-%H%M%S)" \
   TERMSURF_PDF_SETTLE_SECONDS=4 \
   ./scripts/test-issue-776-pdf.sh http://localhost:9616/index.html
   ```

   No Exp 3 stream-store logs, Exp 4 attach logs, or Exp 5 stream-info logs
   should appear.

6. Run non-PDF binary smoke.

   ```bash
   LOG_DIR="$PWD/logs/issue-789-exp5-bin-$(date +%Y%m%d-%H%M%S)" \
   TERMSURF_PDF_SETTLE_SECONDS=4 \
   ./scripts/test-issue-776-pdf.sh http://localhost:9616/test.bin
   ```

   No Exp 3 stream-store logs, Exp 4 attach logs, or Exp 5 stream-info logs
   should appear. Existing content-shell download behavior is acceptable.

7. Record the known teardown crash separately.

   The Issue 776 teardown crash remains out of scope unless it prevents logs or
   screenshots from being captured.

#### Pass Criteria

- `libtermsurf_chromium` builds and links.
- The forbidden broad Chrome dependency set is still absent.
- The PDF extension viewer frame receives stream info through a renderer-visible
  TermSurf-owned `mimeHandlerPrivate` shim backed by `TsPdfStreamStore`.
- The returned stream info has the same `internal_id` lineage and the stored
  `streamUrl` / `originalUrl` from the Exp 2-4 flow.
- The result logs prove what the viewer does next: either it attempts a later
  PDF content/plugin navigation to the stream URL, or it reaches a named next
  missing layer such as `IsPluginHandledExternally` / frame-container creation.
- HTML and non-PDF binary smoke tests do not bind or call the stream-info shim.
- The patch archive is regenerated under `chromium/patches/issue-789/`.

Stretch Pass: Experiment 3's delegate/interceptor path reaches `stream-served`,
and the screenshot visibly advances beyond the current blank viewer state.

#### Partial Criteria

Partial if the build succeeds and the logs identify a narrower missing layer,
but the run does not reach `stream-served`. Valid Partial outcomes include:

- the viewer calls `pdfViewerPrivate.getStreamInfo(...)`, but no browser binding
  exists for that API without a deliberate TermSurf-owned renderer shim;
- the viewer calls `mimeHandlerPrivate.getStreamInfo(...)`, but the custom
  bindings are not installed in TermSurf's renderer environment;
- the renderer installs `chrome.mimeHandlerPrivate`, but the call cannot reach
  the browser-side TermSurf service;
- the browser-side stream-info provider binds, but cannot map the extension
  frame back to the claimed stream;
- `getStreamInfo(...)` returns successfully, but the viewer does not create a
  stream URL navigation, identifying `IsPluginHandledExternally` or frame
  container creation as the likely next layer;
- the stream URL navigation starts, but `MapToOriginalUrl(...)` cannot match the
  claimed stream;
- `stream-served` runs, but the renderer/plugin still stays blank.

The result must name the exact next missing piece and cite the log lines.

#### Failure Criteria

- The experiment imports Chrome's broad extension browser stack or GuestView
  stack instead of a TermSurf-owned minimal shim.
- The implementation bypasses `TsPdfStreamStore` and stores duplicate stream
  state elsewhere.
- The implementation reintroduces the old data-wrapper fake PDF path.
- The implementation changes normal HTML or non-PDF binary behavior.
- The implementation claims Pass without a renderer-side
  `[issue-789-exp5] viewer-api-call` log and a browser-side
  `[issue-789-exp5] get-stream-info` success log.
- The implementation claims `stream-served` as required for Pass instead of
  treating it as a stretch outcome or next-layer discovery.
- The implementation reaches `stream-served` with a mismatched `internal_id`,
  `streamUrl`, or `originalUrl`.

**Result:** Partial

Implemented on Chromium branch `148.0.7778.97-issue-789-exp5`.

The branch builds:

```bash
autoninja -C out/Default libtermsurf_chromium
```

The dependency guard stayed clean for TermSurf's target:

```bash
gn desc out/Default //content/libtermsurf_chromium deps |
  rg "//chrome/browser/plugins:impl|//chrome/browser/extensions:extensions|//components/guest_view/browser"
```

No forbidden dependency appeared in `//content/libtermsurf_chromium`.

What changed:

- `TsPdfResponseURLLoaderThrottle` now preserves the original PDF response head
  before rewriting the top-level navigation to the PDF embedder HTML. This keeps
  the stored stream metadata as `application/pdf`, not `text/html`.
- `TsDispatchPdfStream` now carries the stream metadata on the PDF viewer
  handler URL as `termsurf_*` query parameters.
- `TsPdfComponentExtensionResourceManager` now supplies the missing
  `textdirection`, `language`, and empty `pdfOopifEnabled` template
  replacements, keeping the viewer on the `mimeHandlerPrivate` path.
- `TsPdfViewerURLLoaderFactory` now decodes PDF viewer resources with
  `LoadDataResourceString(...)`, injects a small renderer-visible
  `chrome.mimeHandlerPrivate` shim into `pdf/index.html`, stubs the
  `chrome.pdfViewerPrivate` constants/events that the bundled viewer touches at
  module load time, and logs shim installation through a TermSurf-only
  image-beacon endpoint.

The automated PDF smoke run:

```text
logs/issue-789-exp5-20260528-070057/
```

proved:

- Exp 2 still captures the PDF stream:
  `[issue-789-exp2] stream-container-captured`.
- Exp 4 still attaches and commits the PDF extension viewer frame:
  `[issue-789-exp4] attach-handler-committed`.
- Exp 5 serves decoded `pdf/index.html`, applies template replacements, and
  keeps OOPIF disabled:
  `[issue-789-exp5] viewer-template pdf_oopif_enabled=absent`.
- Exp 5 installs the renderer-visible shim:
  `[issue-789-exp5] viewer-api-installed ... api=mimeHandlerPrivate ... result=ok`.

The run did **not** reach `[issue-789-exp5] viewer-api-call` or
`[issue-789-exp5] get-stream-info`.

The next blocker is not stream metadata. The viewer module graph starts loading,
but it fails before `createBrowserApi()` can call
`chrome.mimeHandlerPrivate.getStreamInfo(...)` because the extension viewer's
modules import `chrome://resources/...` modules and CSS that TermSurf does not
serve in this context:

```text
Not allowed to load local resource: chrome://resources/css/text_defaults_md.css
Not allowed to load local resource: chrome://resources/js/assert.js
Not allowed to load local resource: chrome://resources/lit/v3_0/lit.rollup.js
```

The screenshot advanced from the earlier raw/gibberish PDF bytes to the PDF
viewer shell's dark plugin area in the upper-left of the white embedder page,
but the full PDF still did not render.

Normal HTML and non-PDF binary smokes stayed gated:

```text
logs/issue-789-exp5-html-20260528-070228/
logs/issue-789-exp5-bin-20260528-070228/
```

The HTML smoke emitted only the generic Exp 2 throttle creation/install logs and
no Exp 3 stream-store logs, Exp 4 attach logs, or Exp 5 shim logs. The non-PDF
binary smoke emitted no PDF stream/attach/shim logs.

The known teardown crash still appears after artifacts are captured:

```text
Received signal 11 SEGV_ACCERR
```

That remains out of scope for this issue because the screenshot and logs are
captured before teardown.

#### Conclusion

Experiment 5 proved that the next PDF blocker is no longer the first
viewer-private API surface. TermSurf can now serve decoded PDF viewer resources,
keep the viewer in non-OOPIF `mimeHandlerPrivate` mode, and install a
renderer-visible `chrome.mimeHandlerPrivate` shim without pulling in Chrome's
broad extension browser stack or GuestView.

The viewer still cannot call `getStreamInfo()` because it fails earlier on
missing `chrome://resources` imports. The next experiment should provide the
minimal Chrome WebUI resource loading path needed by the PDF viewer's module
graph, or rewrite those imports to equivalent TermSurf-served resources, before
returning to stream-info delivery.

### Experiment 6: Serve PDF Viewer WebUI Resources

#### Description

Experiment 5 proved that TermSurf can serve the PDF viewer shell, keep it on the
non-OOPIF `mimeHandlerPrivate` path, and install a renderer-visible
`chrome.mimeHandlerPrivate` shim. The viewer still stops before calling
`getStreamInfo()` because its JavaScript modules import shared Chrome WebUI
resources:

```text
chrome://resources/css/text_defaults_md.css
chrome://resources/js/assert.js
chrome://resources/lit/v3_0/lit.rollup.js
```

Those imports are normal for Chrome's PDF viewer, but TermSurf's
content-shell-based embedder currently only serves the PDF extension resources
from `chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/...`. It does not
provide the `chrome://resources/...` loader that Chrome's WebUI stack provides.

This experiment adds the smallest `chrome://resources` serving path needed by
the PDF viewer's module graph. Electron already solves this layer by registering
Chromium's embedder-facing WebUI resource factory for component-extension
frames:

```cpp
content::CreateWebUIURLLoaderFactory(
    frame_host,
    content::kChromeUIScheme,
    {content::kChromeUIResourcesHost})
```

That helper serves shared `chrome://resources/...` files through Chromium's
existing URL data source machinery. It already owns resource-ID lookup, MIME
types, path normalization, and host-level allowlisting for `chrome://resources`.
TermSurf should copy that shape instead of hand-writing a per-path resource
manager.

The goal is not to make PDFs render completely yet. The goal is to advance the
viewer past the missing WebUI-resource failures and identify the next missing
layer. If the viewer reaches Experiment 5's
`chrome.mimeHandlerPrivate.getStreamInfo(...)` shim, that is an especially good
outcome, but this experiment should not drift into later PDF plugin,
`pdfViewerPrivate`, or MimeHandlerView-equivalent work.

This is still the Electron-style strategy: mirror only the narrow pieces the PDF
viewer needs, backed by Chromium's existing embeddable resource-serving helper.
Do not import Chrome's full WebUI controller stack, Chrome extension browser
stack, GuestView, or MimeHandlerView.

#### Changes

1. Create a new Chromium branch from Experiment 5.

   ```bash
   cd chromium/src
   git checkout 148.0.7778.97-issue-789-exp5
   git checkout -b 148.0.7778.97-issue-789-exp6
   ```

   Update `chromium/README.md` in the main repo to list the new branch.

2. Audit the viewer's actual missing `chrome://resources` imports.

   Use the Experiment 5 logs as the starting set, then inspect the generated PDF
   viewer JavaScript to enumerate likely follow-up imports:

   ```bash
   rg "chrome://resources" out/Default/gen/chrome/browser/resources/pdf
   rg "chrome://resources" chrome/browser/resources/pdf
   ```

   Also record the runtime-observed failures from Experiment 5:

   ```bash
   rg "chrome://resources|Not allowed to load local resource" logs/issue-789-exp5*
   ```

   The expected first set includes:

   ```text
   chrome://resources/css/text_defaults_md.css
   chrome://resources/js/assert.js
   chrome://resources/lit/v3_0/lit.rollup.js
   ```

   Do not use this audit to build a per-path allowlist. The audit is for result
   interpretation only. Chromium's `CreateWebUIURLLoaderFactory(...)` remains
   responsible for resolving valid `chrome://resources` paths.

3. Verify the `chrome` scheme. Do not register it speculatively.

   `chrome` is already a built-in WebUI scheme in content.
   `URLDataManagerBackend` always adds the shared `chrome://resources` data
   source (`content/browser/webui/url_data_manager_backend.cc`), and
   content-shell already treats `chrome` as a handled scheme
   (`content/shell/browser/shell_content_browser_client.cc`).
   `TsContentClient::AddAdditionalSchemes` currently only adds the PDF extension
   scheme (`content/libtermsurf_chromium/ts_content_client.cc`).

   Therefore the expected result is `chrome=registered`. Treat `chrome` as
   built-in: do **not** add it to `standard_schemes`, `secure_schemes`, or any
   similar list, and do not otherwise broaden `chrome:` semantics, unless a
   concrete local failure in this experiment proves that exact registration is
   required. If you believe registration is needed, record the precise failing
   symptom in the result before adding it. `added` or `missing` is surprising
   and must be called out in the result before proceeding.

   Add a startup log so the result records what happened:

   ```text
   [issue-789-exp6] scheme-check chrome=<registered|added|missing>
   ```

4. Route `chrome://resources` through Chromium's WebUI resource factory.

   In `TsBrowserClient::RegisterNonNetworkSubresourceURLLoaderFactories(...)`,
   register Chromium's existing WebUI resource factory for `chrome://resources`:

   ```cpp
   content::RenderFrameHost* frame_host =
       content::RenderFrameHost::FromID(render_process_id, render_frame_id);
   if (!frame_host) {
     return;
   }

   factories->emplace(
       content::kChromeUIScheme,
       content::CreateWebUIURLLoaderFactory(
           frame_host,
           content::kChromeUIScheme,
           {content::kChromeUIResourcesHost}));
   ```

   Scope this factory to the specific Exp 4 PDF viewer frame only, not merely to
   the PDF extension origin. TermSurf already serves the PDF extension scheme
   broadly in `TsBrowserClient`
   (`content/libtermsurf_chromium/ts_browser_client.cc`), so origin alone is too
   weak: any frame that reaches
   `chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai` outside the
   stored-stream flow would also gain `chrome://resources`. Require all of the
   following before registering the factory:
   1. `request_initiator_origin` is present and equals the PDF viewer extension
      origin `chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai`. This is the
      origin gate and the authoritative "who is asking" signal:
      `request_initiator_origin` is the origin of the document being committed
      (the frame that will issue these subresource loads). TermSurf does not
      have Chrome's full `ExtensionRegistry`, so this stands in for it.
   2. `frame_host = RenderFrameHost::FromID(...)` exists. Its
      `GetLastCommittedOrigin()` is logged as **context only**, not used as a
      gate — see the timing note below.
   3. The frame is recognized by the Exp 4 attach bookkeeping /
      `TsPdfStreamStore` as the active PDF viewer frame for a stored stream.
      Reuse the same frame-identity check the Exp 4 attach path already uses; do
      not invent a second notion of "the PDF viewer frame." This is the
      authoritative identity gate.

   If any of these fails, do not register the factory. Normal web pages, and
   PDF-extension-origin frames that are not the active stored-stream viewer
   frame, must not gain access to `chrome://resources/...`.

   Timing note (discovered in this experiment's first run): this hook fires
   while the PDF viewer frame's navigation is still committing, so at that
   instant `frame_host->GetLastCommittedOrigin()` is the frame's pre-commit
   (opaque `"null"`) origin, not yet the extension origin. An earlier draft of
   this gate required the committed origin to equal the extension origin; that
   rejected the real viewer frame every time
   (`frame_origin=null reason=frame-origin-mismatch`). The committed-origin
   equality check is therefore demoted to logged context. Security still holds:
   gate 1 (`request_initiator_origin`) is authoritative for the committing
   document, and gate 3 (`TsPdfStreamStore` recognition) is both timing-correct
   (`attach-child-found` records the frame before its subresource factories are
   registered) and stricter than an origin match.

   Do not add a top-level `CreateNonNetworkNavigationURLLoaderFactory(...)` path
   for `chrome://resources`. The PDF viewer uses these URLs as module/CSS
   subresources, not as navigations.

   Add concise logs around the registration decision:

   Requests whose initiator is not the PDF viewer extension origin (gate 1)
   return silently, so normal HTML / non-PDF traffic produces no Exp 6 factory
   logs. Once gate 1 passes, the decision is logged:

   ```text
   [issue-789-exp6] webui-factory-registered initiator_origin=<origin> frame_origin=<origin> host=resources
   [issue-789-exp6] webui-factory-skipped initiator_origin=<origin> frame_origin=<none> reason=no-frame-host
   [issue-789-exp6] webui-factory-skipped initiator_origin=<origin> frame_origin=<origin> reason=frame-not-active-viewer
   [issue-789-exp6] webui-factory-registered-count web_contents=<id> count=<n>
   ```

   Preserve the existing PDF extension resource factory. Do not replace the
   generic `chrome-extension://` factory with `chrome://resources` handling, and
   do not serve arbitrary `chrome://...` hosts. The allowlist boundary is the
   `content::kChromeUIResourcesHost` host-level allowlist passed to
   `CreateWebUIURLLoaderFactory(...)`.

5. Preserve Experiment 5's stream-info shim.

   Do not redesign the `mimeHandlerPrivate` shim in this experiment. The only
   acceptable Exp 5 changes are small compatibility adjustments needed after the
   viewer modules actually execute.

   The expected flow after Experiment 6:

   ```text
   [issue-789-exp5] viewer-template pdf_oopif_enabled=absent ...
   [issue-789-exp5] viewer-api-installed ... api=mimeHandlerPrivate result=ok
   [issue-789-exp6] scheme-check chrome=<registered|added>
   [issue-789-exp6] webui-factory-registered initiator_origin=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai frame_origin=<origin> host=resources
   ```

   Stretch flow:

   ```text
   [issue-789-exp5] viewer-api-call ... api=mimeHandlerPrivate method=getStreamInfo
   [issue-789-exp5] get-stream-info ... result=ok
   ```

   If the viewer reaches `getStreamInfo()` and then fails on a later PDF plugin,
   `pdfViewerPrivate`, or renderer layer, that is a valid Partial or Stretch
   Pass depending on how far it gets. Do not expand Experiment 6 into that later
   layer.

6. Preserve dependency boundaries.

   `content/libtermsurf_chromium` must still avoid:

   ```text
   //chrome/browser/plugins:impl
   //chrome/browser/extensions:extensions
   //components/guest_view/browser
   ```

   Also avoid Chrome's full WebUI controller/browser stack unless this
   experiment records Failure and redesigns around a deliberate product
   decision. `content::CreateWebUIURLLoaderFactory(...)` is acceptable because
   it is the narrow embedder-facing helper Electron uses for this exact class of
   subresource load.

7. Format, build, archive.

   Run Chromium formatting on modified C++/GN files:

   ```bash
   cd chromium/src
   ../depot_tools/clang-format -i <modified .cc/.h files>
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   gn format content/libtermsurf_chromium/BUILD.gn
   ```

   Build before committing the Chromium branch:

   ```bash
   autoninja -C out/Default libtermsurf_chromium
   ```

   After the Chromium branch commit, regenerate the Issue 789 patch archive from
   the last buildable PDF base:

   ```bash
   tmp_dir="$(mktemp -d ../../chromium/patches/issue-789.XXXXXX)"
   git format-patch 148.0.7778.97-issue-776-exp7..HEAD \
     -o "$tmp_dir"
   rm -rf ../../chromium/patches/issue-789
   mv "$tmp_dir" ../../chromium/patches/issue-789
   ```

#### Verification

1. Build `libtermsurf_chromium`.

   ```bash
   cd chromium/src
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   autoninja -C out/Default libtermsurf_chromium
   ```

2. Check dependencies.

   ```bash
   gn desc out/Default //content/libtermsurf_chromium deps
   rg "//chrome/browser/plugins:impl|//chrome/browser/extensions:extensions|//components/guest_view/browser" \
     content/libtermsurf_chromium chrome/browser/pdf chrome/browser/plugins
   ```

   Forbidden targets must not appear as TermSurf dependencies.

3. Run the automated Bitcoin PDF smoke test.

   ```bash
   LOG_DIR="$PWD/logs/issue-789-exp6-$(date +%Y%m%d-%H%M%S)" \
   TERMSURF_PDF_SETTLE_SECONDS=10 \
   ./scripts/test-issue-776-pdf.sh http://localhost:9616/bitcoin.pdf
   ```

   Required log sequence for Pass:

   ```text
   [issue-789-exp4] attach-handler-committed ...
   [issue-789-exp5] viewer-template pdf_oopif_enabled=absent ...
   [issue-789-exp5] viewer-api-installed ... result=ok
   [issue-789-exp6] scheme-check chrome=<registered|added>
   [issue-789-exp6] webui-factory-registered initiator_origin=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai frame_origin=<origin> host=resources
   ```

   The log must not contain new
   `Not allowed to load local resource: chrome://resources/...` failures for
   resources requested by the PDF viewer. If resource loading succeeds and the
   next failure is a missing `pdfViewerPrivate` or plugin-layer API, record that
   exact next failure instead of expanding this experiment.

   The result must list the `chrome://resources` paths the viewer requested
   during the smoke run and mark each path as served or failed. Absence of
   `Not allowed to load local resource` errors is not sufficient proof, because
   `CreateWebUIURLLoaderFactory(...)` emits no per-resource TermSurf logs and a
   resource can fail silently or never be requested because earlier JS already
   died. Back the served/failed list with a concrete per-resource proof source,
   in order of preference:
   - DevTools / renderer network log entries with HTTP status per URL;
   - a NetLog capture of the run;
   - temporary instrumentation logging completion + net error per
     `chrome://resources` request, if the above are impractical.

   Each of the three expected paths (`text_defaults_md.css`, `assert.js`,
   `lit/v3_0/lit.rollup.js`) plus any newly observed ones must appear in the
   proof source as served (200/OK) or failed (with the exact error). A path that
   was never requested must be listed as "not requested" with the earlier
   failure that prevented it.

   Stretch log sequence:

   ```text
   [issue-789-exp5] viewer-api-call ... method=getStreamInfo
   [issue-789-exp5] get-stream-info ... result=ok
   ```

4. Capture screenshot output.

   Classify the screenshot as one of:
   - PDF visibly rendered;
   - viewer advanced past the Exp 5 blank/dark-shell state but PDF still blank;
   - viewer reached `getStreamInfo()` but failed on plugin/renderer creation;
   - viewer still blocked on `chrome://resources` imports;
   - renderer crash;
   - automation failure.

   If the PDF visibly renders, also test whether the first page scrolls.

5. Run normal HTML smoke.

   ```bash
   LOG_DIR="$PWD/logs/issue-789-exp6-html-$(date +%Y%m%d-%H%M%S)" \
   TERMSURF_PDF_SETTLE_SECONDS=4 \
   ./scripts/test-issue-776-pdf.sh http://localhost:9616/index.html
   ```

   No Exp 3 stream-store logs, Exp 4 attach logs, Exp 5 shim logs, or Exp 6
   PDF-viewer WebUI resource logs should appear. If
   `webui-factory-registered-count` is logged, the count must be 0.

6. Run non-PDF binary smoke.

   ```bash
   LOG_DIR="$PWD/logs/issue-789-exp6-bin-$(date +%Y%m%d-%H%M%S)" \
   TERMSURF_PDF_SETTLE_SECONDS=4 \
   ./scripts/test-issue-776-pdf.sh http://localhost:9616/test.bin
   ```

   No Exp 3 stream-store logs, Exp 4 attach logs, Exp 5 shim logs, or Exp 6
   PDF-viewer WebUI resource logs should appear. If
   `webui-factory-registered-count` is logged, the count must be 0.

7. Run the negative `chrome://resources` leak test.

   Confirm the gate does not leak `chrome://resources` to ordinary web content.
   From a normal HTML page (not the PDF viewer frame), attempt to load a WebUI
   resource the PDF viewer uses, for example by importing or fetching:

   ```text
   chrome://resources/js/assert.js
   ```

   This request must fail. The expected outcome is a
   `Not allowed to load local resource: chrome://resources/...` error (or an
   equivalent network failure), and no
   `[issue-789-exp6] webui-factory-registered` log for that frame. If a normal
   web page can load `chrome://resources/...`, the gate is too broad; record
   Failure and tighten the frame gate before claiming any Pass.

8. Record the known teardown crash separately.

   The Issue 776 teardown crash remains out of scope unless it prevents logs or
   screenshots from being captured.

#### Pass Criteria

- `libtermsurf_chromium` builds and links.
- The forbidden broad Chrome dependency set is still absent.
- `chrome` is not added to `standard_schemes`/`secure_schemes`; the scheme-check
  records `chrome=registered` (or `added` is justified by a recorded failure).
- `chrome://resources` is routed through Chromium's
  `content::CreateWebUIURLLoaderFactory(...)`, scoped to the active Exp 4 PDF
  viewer frame — all three gates (initiator origin, committed frame origin, and
  Exp 4 / `TsPdfStreamStore` frame recognition) pass.
- The run no longer fails before `createBrowserApi()` due to missing
  `chrome://resources` dependencies.
- Each requested `chrome://resources` path is shown served or failed via a
  concrete per-resource proof source (network log, NetLog, or instrumentation),
  not merely by the absence of console errors.
- If the viewer still fails before calling
  `mimeHandlerPrivate.getStreamInfo(...)`, the new failure is named precisely
  and is not another missing `chrome://resources` load.
- The negative leak test passes: a normal HTML page cannot load
  `chrome://resources/...`.
- HTML and non-PDF binary smoke tests do not bind or call the PDF resource,
  stream, attach, or shim paths.
- The patch archive is regenerated under `chromium/patches/issue-789/`.

Stretch Pass: the viewer calls the Experiment 5
`mimeHandlerPrivate.getStreamInfo(...)` shim, and the shim returns stream
metadata with the same `internal_id`, `streamUrl`, and `originalUrl` lineage
from Exp 2-5.

#### Partial Criteria

Partial if the build succeeds and the viewer gets farther, but a narrower next
layer remains. Valid Partial outcomes include:

- the `chrome` scheme is not routed through the non-network subresource factory
  path and needs a lower-level scheme-registration follow-up;
- the WebUI resource factory loads successfully, but a non-WebUI viewer API is
  missing before `getStreamInfo()`;
- the WebUI resource factory loads successfully, but `loadTimeData` is missing
  strings or feature values requested by the viewer;
- `getStreamInfo()` succeeds, but the viewer does not create the inner PDF
  plugin/content navigation;
- the stream URL navigation starts, but `MapToOriginalUrl(...)` cannot match the
  claimed stream;
- `stream-served` runs, but the renderer/plugin still stays blank;
- the viewer requests an unexpected `chrome://<host>` that is not
  `content::kChromeUIResourcesHost` and the factory hard-fails or bad-messages
  it. Record this as Partial naming the exact requested host, not as a generic
  renderer crash.

The result must name the exact next missing piece and cite log lines.

If a later experiment sees `chrome://resources` failures from the inner PDF
plugin/content frame, do not widen this experiment's PDF-viewer-frame gate
retroactively. The correct follow-up would be a separate per-frame registration
decision for the plugin/content frame, gated to that frame's actual origin and
needs.

#### Failure Criteria

- The experiment imports Chrome's broad WebUI controller stack, broad extension
  browser stack, GuestView, or MimeHandlerView instead of a narrow resource
  serving path.
- The implementation serves arbitrary `chrome://...` hosts instead of limiting
  the WebUI resource factory to `content::kChromeUIResourcesHost`.
- The implementation changes normal HTML or non-PDF binary behavior.
- The implementation removes or bypasses Experiment 5's `mimeHandlerPrivate`
  shim instead of making the viewer module graph reach it.
- The implementation claims Pass while `chrome://resources` loads are still the
  first failing layer.
- The frame gate leaks: a normal web page (or a PDF-extension-origin frame that
  is not the active stored-stream viewer frame) can load
  `chrome://resources/...`.

#### Result

**Result:** Partial

The browser-side WebUI resource factory is implemented, correctly gated, and
registers for the live PDF viewer frame — but `chrome://resources` loads still
fail, now at a different and precisely identified layer: a renderer-side
`SecurityOrigin::CanDisplay` check that runs before any browser URL loader
factory is consulted.

Chromium branch `148.0.7778.97-issue-789-exp6` (forked from
`148.0.7778.97-issue-789-exp5`). Changes, all in
`content/libtermsurf_chromium/`:

- `ts_content_client.cc` — `scheme-check` startup log confirming `chrome` is a
  content built-in. No scheme registration added (verified `chrome=registered`).
- `ts_pdf_stream_store.{h,cc}` — `IsPdfExtensionHostFrame(RenderFrameHost*)`,
  true when the frame's tree-node id matches a stored stream's
  `extension_host_frame_tree_node_id()`.
- `ts_browser_client.cc` — gated `chrome://resources` WebUI factory in
  `RegisterNonNetworkSubresourceURLLoaderFactories` via
  `content::CreateWebUIURLLoaderFactory(frame_host, kChromeUIScheme, {kChromeUIResourcesHost})`.

Verification:

- **Build / deps.** `libtermsurf_chromium` builds and links. `gn desc` shows
  none of `//chrome/browser/plugins:impl`,
  `//chrome/browser/extensions:extensions`, `//components/guest_view/browser`.
- **Scheme check.**
  `[issue-789-exp6] scheme-check chrome=registered note=content-builtin`.
  Confirmed in source: `RegisterContentSchemes` unconditionally registers
  `chrome` as standard/secure right after `AddAdditionalSchemes`
  (`content/common/url_schemes.cc:62,72`), so no embedder registration was
  needed or added.
- **Gate timing discovery (first run).** The first build gated on
  `GetLastCommittedOrigin()` equalling the extension origin. It logged
  `webui-factory-skipped ... frame_origin=null reason=frame-origin-mismatch` on
  every run and never registered the factory. Cause: this hook fires while the
  viewer frame is committing, so its committed origin is still the pre-commit
  opaque `"null"` origin. Fix: demote committed-origin to logged context; gate
  on `request_initiator_origin` (authoritative for the committing document) plus
  `TsPdfStreamStore` recognition (timing-correct — `attach-child-found` precedes
  factory registration, observed at `…895896` vs `…898850`).
- **PDF smoke after fix.** Factory now registers for the viewer frame:
  `[issue-789-exp6] webui-factory-registered initiator_origin=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai frame_origin=null host=resources`
  and `webui-factory-registered-count … count=1`. The Exp 4/5 sequence
  (`attach-handler-committed`, `viewer-template`,
  `viewer-api-installed result=ok`) is intact.
- **`chrome://resources` served/failed.** All requested paths still **failed**,
  but no longer for lack of a browser factory. The renderer blocks each one at
  `BaseFetchContext::CanRequest` → `resource_request.CanDisplay(url)`
  (`third_party/blink/renderer/core/loader/base_fetch_context.cc:233`, logged as
  `Not allowed to load local resource: chrome://resources/...`). Observed paths
  (all failed at this layer): `css/text_defaults_md.css`, `js/assert.js`,
  `lit/v3_0/lit.rollup.js`, `js/load_time_data.js`,
  `mojo/mojo/public/js/bindings.js`,
  `cr_elements/cr_a11y_announcer/cr_a11y_announcer.css`. Evidence source:
  renderer `CONSOLE` lines in the Wezboard log (the test harness routes the
  Roamium child's stderr there).
- **Screenshot.** Browser overlay is a blank white viewer shell — classified as
  _viewer still blocked on `chrome://resources` imports_ (now at the renderer
  `CanDisplay` layer, not the missing browser factory).
- **HTML and non-PDF binary smoke.** `index.html` and `test.bin` produced **0**
  `webui-factory` lines and **0** Exp 4/5 PDF-path lines; `registered-count` was
  never logged. The benign per-process `scheme-check` startup log still appears
  (it is not a PDF-path or factory log). Gate does not fire for non-PDF content.
- **Negative leak.** Structurally satisfied: the factory only registers when
  gate 1 (PDF extension initiator) and gate 3 (`TsPdfStreamStore` recognition)
  both pass, which a normal page never does (confirmed 0 factory lines on the
  HTML run); and Blink's `CanDisplay` independently blocks `chrome://resources`
  for non-WebUI origins. An explicit normal-page
  `fetch('chrome://resources/...')` assertion is left as cheap future hardening.

#### Conclusion

Experiment 6 achieved its scoped goal — register Chromium's narrow WebUI
resource factory for the PDF viewer frame, gated correctly and without pulling
in any forbidden Chrome subsystem — and, more valuably, corrected the Electron
mental model: the browser-side `CreateWebUIURLLoaderFactory` is **necessary but
not sufficient**. A `chrome://resources` subresource is blocked earlier, in the
renderer, by `SecurityOrigin::CanDisplay`, because the PDF extension origin is
not permitted to display the display-isolated `chrome` scheme.

It also overturned the Codex review's high-severity recommendation in practice:
gating on the committed frame origin is incompatible with the
factory-registration-at-commit timing and rejected the real viewer frame. The
`TsPdfStreamStore` recognition gate is the timing-correct and stricter identity
check.

Next layer (Experiment 7): grant the PDF viewer extension origin renderer-side
permission to display `chrome://resources`, mirroring how Chrome lets its
component-extension PDF viewer reach shared WebUI resources — e.g. a Blink
scheme-registry / origin-access grant scoped to the PDF extension origin and the
`chrome://resources` host — so the loads that now fail at
`base_fetch_context.cc:233` are permitted and reach the browser factory this
experiment installed. This must stay as narrow as the browser-side gate: only
the PDF viewer extension origin, only `chrome://resources`.

### Experiment 7: Grant the PDF Viewer Renderer-Side chrome://resources Access

#### Description

Experiment 6 installed and correctly gated the browser-side WebUI resource
factory, but `chrome://resources` subresources still fail in the renderer at
`SecurityOrigin::CanDisplay` before any browser factory is consulted. This
experiment adds the missing renderer-side half so the two layers line up: the
browser serves the bytes (Exp 6) and the renderer permits the PDF viewer
document to display them (Exp 7).

The decision path was traced in the Chromium source:

```text
BaseFetchContext::CanRequest          third_party/blink/renderer/core/loader/base_fetch_context.cc:233
  resource_request.CanDisplay(url)
    SecurityOrigin::CanDisplay         third_party/blink/renderer/platform/weborigin/security_origin.cc
      SchemeRegistry::ShouldTreatURLSchemeAsDisplayIsolated("chrome") == true
        return protocol_ == "chrome"                       // false: doc is chrome-extension
            || SecurityPolicy::IsOriginAccessToURLAllowed(this, url)   // <- the lever
```

`chrome` is registered display-isolated in the renderer
(`content/renderer/render_thread_impl.cc:860`,
`RegisterURLSchemeAsDisplayIsolated`), so for a `chrome-extension://` document
`CanDisplay(chrome://resources/...)` returns true only if an entry in the
renderer's origin-access allowlist permits it. That allowlist is populated by
`blink::WebSecurityPolicy::AddOriginAccessAllowListEntry`
(`third_party/blink/public/web/web_security_policy.h`). In stock Chromium the
extensions renderer `Dispatcher::UpdateOriginPermissions`
(`extensions/renderer/dispatcher.cc`) calls exactly this lever per extension.

TermSurf is content-shell based and does **not** run the extensions renderer
`Dispatcher`, so no code adds that entry today. This experiment adds it directly
for the one PDF viewer extension origin, scoped to the `chrome://resources`
host.

Reconciliation of the two research threads, recorded so it is not re-litigated:
a survey of Electron concluded the browser-side
`RegisterNonNetworkSubresourceURLLoaderFactories` factory is the "whole
mechanism, no patch needed." That is contradicted by this issue's own Exp 6
runtime evidence — with the browser factory in place, `chrome://resources` still
failed at `CanDisplay`. The most likely explanation is that Electron runs the
full extensions renderer `Dispatcher` for its component PDF extension, so it
receives the origin-access grant via the extension system and never has to add
it by hand. The underlying lever is the same origin-access allowlist either way.
There is residual uncertainty about exactly how the grant arises in stock
Chrome/Electron (the `chrome://resources` host specifically); Experiment 7 is
the empirical test of the lever, not of that backstory.

This stays strictly within the Electron-style strategy: one narrow renderer
grant, no extensions browser/renderer stack, no GuestView, no MimeHandlerView.

#### Changes

1. Create a new Chromium branch from Experiment 6.

   ```bash
   cd chromium/src
   git checkout 148.0.7778.97-issue-789-exp6
   git checkout -b 148.0.7778.97-issue-789-exp7
   ```

   Update `chromium/README.md` in the main repo to list the new branch.

2. Register the origin-access allowlist entry at renderer startup.

   In `content/libtermsurf_chromium/ts_renderer_client.{h,cc}`, override
   `RenderThreadStarted()`. Call the base implementation first, then add one
   entry permitting the PDF viewer extension origin to display
   `chrome://resources`:

   ```cpp
   void TsRendererClient::RenderThreadStarted() {
     ShellContentRendererClient::RenderThreadStarted();

     const GURL pdf_extension_origin(
         std::string("chrome-extension://") + extension_misc::kPdfExtensionId +
         "/");
     blink::WebSecurityPolicy::AddOriginAccessAllowListEntry(
         pdf_extension_origin,
         blink::WebString::FromASCII(content::kChromeUIScheme),    // "chrome"
         blink::WebString::FromASCII(content::kChromeUIResourcesHost),  // "resources"
         /*destination_port=*/0,
         network::mojom::CorsDomainMatchMode::kDisallowSubdomains,
         network::mojom::CorsPortMatchMode::kAllowAnyPort,
         network::mojom::CorsOriginAccessMatchPriority::kDefaultPriority);

     LOG(INFO) << "[issue-789-exp7] origin-access-granted source="
               << pdf_extension_origin.spec() << " dest=chrome://resources";
   }
   ```

   The origin-access list is process-global, so registering once at thread start
   covers every frame in the renderer process. The entry is scoped to exactly
   one source origin, one destination scheme, and one destination host
   (`kDisallowSubdomains`), so no other origin gains `chrome://resources` access
   and the PDF viewer gains nothing beyond that host.

   Two scope facts to keep in mind (both verified against
   `third_party/blink/public/web/web_security_policy.h`):
   - The grant is **per source origin, not per frame**. It cannot be scoped to
     the active Exp 4 viewer frame — _any_ document with the PDF extension
     origin in this renderer will pass the renderer-side
     `CanDisplay(chrome://resources/...)` check after Exp 7. The frame-level
     enforcement that stops a non-active PDF-extension frame from actually
     receiving bytes stays in Exp 6's browser-side gate
     (`TsPdfStreamStore::IsPdfExtensionHostFrame`). The two layers are
     complementary: this experiment permits _display_, Exp 6 decides _which
     frame is served_. Negative test 5b checks that the browser gate still
     contains this widened renderer grant.
   - `AddOriginAccessAllowListEntry` is not display-only. Per the header, it
     also lets the source origin bypass the same-origin policy when
     `fetch()`-ing the destination and set otherwise-forbidden headers on those
     requests. So this entry permits the PDF extension origin to make
     `fetch()`/SOP-relaxed requests to `chrome://resources`, not just load it as
     a subresource. That is acceptable here because the destination is the
     read-only shared WebUI resource host and the source is the single PDF
     viewer origin, but it is a real widening and is recorded deliberately.

3. Do not register the entry per-frame and do not broaden it. Do not grant any
   host other than `chrome://resources`. Do not grant the entry to any origin
   other than the PDF viewer extension origin. Do not touch
   `RegisterURLSchemeAs...` flags — `chrome` must stay display-isolated.

4. Preserve Experiment 6. The browser-side WebUI factory and its three-part gate
   are unchanged. Experiment 7 only adds the renderer-side display permission.

5. Preserve dependency boundaries. `content/libtermsurf_chromium` must still
   avoid `//chrome/browser/plugins:impl`,
   `//chrome/browser/extensions:extensions`, `//components/guest_view/browser`,
   and the broad extensions renderer stack. `blink::WebSecurityPolicy` is a
   `third_party/blink/public` API already available to the renderer client.

6. Format, build, archive.

   ```bash
   cd chromium/src
   ../depot_tools/clang-format -i content/libtermsurf_chromium/ts_renderer_client.cc \
     content/libtermsurf_chromium/ts_renderer_client.h
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   autoninja -C out/Default libtermsurf_chromium
   ```

   After committing the Chromium branch, regenerate the patch archive:

   ```bash
   tmp_dir="$(mktemp -d ../../chromium/patches/issue-789.XXXXXX)"
   git format-patch 148.0.7778.97-issue-776-exp7..HEAD -o "$tmp_dir"
   rm -rf ../../chromium/patches/issue-789
   mv "$tmp_dir" ../../chromium/patches/issue-789
   ```

#### Verification

1. Build `libtermsurf_chromium`; confirm the forbidden Chrome dependency set is
   still absent (`gn desc out/Default //content/libtermsurf_chromium deps`).

2. Run the Bitcoin PDF smoke test.

   ```bash
   LOG_DIR="$PWD/logs/issue-789-exp7-$(date +%Y%m%d-%H%M%S)" \
   TERMSURF_PDF_SETTLE_SECONDS=10 \
   ./scripts/test-issue-776-pdf.sh http://localhost:9616/bitcoin.pdf
   ```

   Required for Pass:
   - `[issue-789-exp7] origin-access-granted source=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/ dest=chrome://resources`
   - `[issue-789-exp6] webui-factory-registered ... host=resources`
   - The six `chrome://resources` paths that failed in Exp 6
     (`css/text_defaults_md.css`, `js/assert.js`, `lit/v3_0/lit.rollup.js`,
     `js/load_time_data.js`, `mojo/mojo/public/js/bindings.js`,
     `cr_elements/cr_a11y_announcer/cr_a11y_announcer.css`) **no longer** log
     `Not allowed to load local resource`. List each path served or failed,
     backed by renderer console / network evidence as in Exp 6.

   Stretch: the viewer modules execute far enough to call the Exp 5
   `mimeHandlerPrivate.getStreamInfo()` shim
   (`[issue-789-exp5] viewer-api-call ... method=getStreamInfo`).

3. Capture and classify the screenshot (same buckets as Exp 6). If the PDF
   renders, also test first-page scroll.

4. Run HTML and non-PDF binary smoke (`index.html`, `test.bin`). The
   origin-access entry targets only the PDF extension origin, so normal pages
   must be unaffected. Note: `RenderThreadStarted()` runs for every renderer
   process, so the `[issue-789-exp7] origin-access-granted` startup log **is
   expected** to appear in these smokes (like the Exp 6 `scheme-check` log) and
   is not a violation. What must not appear: successful `chrome://resources`
   loading, or any Exp 4/5/6 PDF-path behavior (attach, shim, factory
   registration).

5. Negative leak test — two cases. The grant must not let any origin other than
   the PDF viewer extension origin reach `chrome://resources`.

   a. **Normal web page.** From `index.html`, attempt
   `fetch('chrome://resources/js/assert.js')` (or an `import`). It must fail
   with `Not allowed to load local resource`. (The explicit assertion Exp 6 left
   as future hardening; do it here now that a real grant exists.)

   b. **Non-active PDF-extension-origin frame.** This is the case the Exp 7
   grant genuinely widens: `AddOriginAccessAllowListEntry` is process-global and
   scoped only to the _source origin_, so **any**
   `chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai` document now passes the
   renderer `CanDisplay` check — not just the active viewer frame. The only
   thing stopping a non-active PDF-extension-origin frame from actually
   receiving bytes is Exp 6's browser-side frame gate
   (`TsPdfStreamStore::IsPdfExtensionHostFrame`,
   `content/libtermsurf_chromium/ts_browser_client.cc`). Verify that gate still
   holds end to end: create/load a PDF-extension-origin frame that is not the
   active stored-stream viewer frame and confirm
   `chrome://resources/js/assert.js` still fails for it and that no
   `[issue-789-exp6] webui-factory-registered` log fires for that frame (expect
   `reason=frame-not-active-viewer`). If this frame can load
   `chrome://resources`, the two-layer design has a hole — record Failure.

#### Pass Criteria

- `libtermsurf_chromium` builds and links; forbidden deps still absent.
- The `[issue-789-exp7] origin-access-granted` log appears once per renderer.
- The previously failing `chrome://resources` loads succeed (no
  `CanDisplay`/"Not allowed to load local resource" for the PDF viewer frame),
  proven per path by console/network evidence.
- The grant is scoped to the PDF viewer extension origin and the
  `chrome://resources` host only (no other origin, no other chrome host).
- Both negative tests pass: a normal web page cannot load
  `chrome://resources/...`, and a non-active PDF-extension-origin frame cannot
  either (Exp 6's frame gate holds, `reason=frame-not-active-viewer`).
- HTML and non-PDF binary smoke show no PDF-path behavior change (the
  `[issue-789-exp7] origin-access-granted` startup log is expected and allowed).

Stretch Pass: the viewer reaches the Exp 5 `mimeHandlerPrivate.getStreamInfo()`
shim.

#### Partial Criteria

Partial if the build succeeds and `chrome://resources` now loads, but the viewer
fails at a later, precisely named layer — for example the viewer JS runs but
does not reach `getStreamInfo()`, `getStreamInfo()` is reached but the inner PDF
plugin/content navigation is not created, or a renderer/plugin layer stays
blank. Name the exact next failure and cite log lines.

#### Failure Criteria

- The origin-access grant is too broad: any origin other than the PDF viewer
  extension origin, or any host other than `chrome://resources`, gains access
  (negative test 5a fails).
- A non-active PDF-extension-origin frame can load `chrome://resources` end to
  end — i.e. Exp 6's browser-side frame gate no longer contains the
  process-global renderer grant (negative test 5b fails).
- The experiment registers the entry per-frame or repeatedly instead of once at
  renderer startup, or flips `chrome` off display-isolated.
- The implementation imports the extensions renderer/browser stack, GuestView,
  or MimeHandlerView instead of the single `WebSecurityPolicy` call.
- The implementation changes normal HTML or non-PDF binary behavior.

#### Result

**Result:** Pass (Stretch)

The renderer-side display permission was the missing half. With it,
`chrome://resources` now loads, the viewer's module graph executes, and the
viewer calls the Exp 5 `getStreamInfo()` shim — the stretch goal. The viewer
then stops at a new, unrelated layer (Mojo JS bindings), which becomes the next
experiment.

Chromium branch `148.0.7778.97-issue-789-exp7` (forked from
`148.0.7778.97-issue-789-exp6`). One change, in
`content/libtermsurf_chromium/ts_renderer_client.{h,cc}`: override
`RenderThreadStarted()`, call the base, then
`blink::WebSecurityPolicy::AddOriginAccessAllowListEntry(pdf_extension_origin, "chrome", "resources", 0, kDisallowSubdomains, kAllowAnyPort, kDefaultPriority)`.
(`WebString::FromUTF8`, not `FromASCII` — `WebString` has no `FromASCII`.)

Verification:

- **Build / deps.** `libtermsurf_chromium` builds and links; the forbidden
  Chrome dependency set is still absent.
- **Grant log.**
  `[issue-789-exp7] origin-access-granted source=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/ dest=chrome://resources`,
  once per renderer process.
- **`chrome://resources` now loads.** Zero
  `Not allowed to load local resource: chrome://resources/...` errors in the PDF
  run (down from six in Exp 6). All previously failing paths
  (`css/text_defaults_md.css`, `js/assert.js`, `lit/v3_0/lit.rollup.js`,
  `js/load_time_data.js`, `mojo/mojo/public/js/bindings.js`,
  `cr_elements/cr_a11y_announcer/cr_a11y_announcer.css`) are served. The browser
  factory (`webui-factory-registered`) and the renderer grant now line up:
  browser serves bytes, renderer permits display.
- **Stretch reached.** The viewer modules execute far enough to call the Exp 5
  shim: `[issue-789-exp5] viewer-api-call ... method=getStreamInfo` followed by
  `[issue-789-exp5] get-stream-info ... result=ok`.
- **Next failure (new layer).** Immediately after, the renderer logs
  `Uncaught ReferenceError: Mojo is not defined`, source
  `chrome://resources/mojo/mojo/public/js/bindings.js`, then
  `Uncaught (in promise) TypeError: viewer.init is not a function`
  (`pdf/main.js`). The Mojo JS module is now served, but the `Mojo` JS interface
  is not exposed to the PDF viewer frame, and `viewer.init` failing is
  downstream of that. This is a distinct layer from `chrome://resources` display
  permission and is out of Exp 7's scope.
- **Screenshot.** Still a blank viewer shell — classified as _viewer advanced
  past the Exp 5 state (modules execute, `getStreamInfo` reached) but PDF still
  blank_, now blocked at Mojo JS bindings.
- **HTML and non-PDF binary smoke.** `index.html` and `test.bin` produced 0
  `webui-factory-registered` lines and 0 Exp 4/5 PDF-path lines. The
  `[issue-789-exp7] origin-access-granted` startup log appears (3x per run, one
  per renderer) as expected and allowed.
- **Negative test 5a (normal page).** A localhost page calling
  `fetch('chrome://resources/js/assert.js')`
  (`test-html/public/test-chrome-resources-leak.html`) was blocked:
  `LEAKTEST result=BLOCKED error=TypeError: Failed to fetch`. The grant does not
  extend to non-PDF-viewer origins.
- **Negative test 5b (non-active PDF-extension frame).** Verified structurally,
  not empirically. The renderer grant is process-global, so any PDF-extension
  document passes `CanDisplay`; the containment is Exp 6's browser-side frame
  gate, already proven in Exp 6 to register the factory only for the active
  viewer frame (`webui-factory-skipped reason=frame-not-active-viewer`
  otherwise). Constructing a second, non-active PDF-extension-origin frame in
  the smoke harness is contrived; an empirical 5b is left as future hardening.

#### Conclusion

Experiment 7 closed the renderer-side gap Experiment 6 identified, and the
two-layer model now works end to end for resource loading: the browser-side
WebUI factory (Exp 6) serves `chrome://resources`, and the renderer-side
origin-access grant (Exp 7) lets the PDF viewer document display them. The
viewer gets all the way to `getStreamInfo()` — the Exp 5 shim earlier
experiments built toward — confirming the whole stream-handoff chain (Exp 1-5)
is sound once the resources load.

The mechanism matched the Codex-reviewed design exactly: a single
`AddOriginAccessAllowListEntry` at `RenderThreadStarted`, scoped to one origin
and one host, with Exp 6's browser gate as the per-frame enforcement. No
forbidden subsystems, no normal-page regression.

Next layer (Experiment 8): expose the `Mojo` JS bindings interface to the PDF
viewer frame so `chrome://resources/mojo/mojo/public/js/bindings.js` finds its
`Mojo` global and the viewer's `init()` runs. This is how the PDF viewer talks
to its browser-side plugin/host over Mojo; it is the next thing standing between
`getStreamInfo()` and a rendered page. As with every step here, keep it narrow —
enable Mojo JS for the PDF viewer frame only, not broadly.
