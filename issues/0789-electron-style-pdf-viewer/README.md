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
Chrome's `//chrome/browser/plugins:impl` with a narrow PDF-only response
interceptor under `content/libtermsurf_chromium/`. The new path should observe
PDF responses, preserve the original PDF response body as a transferable stream,
and hand that stream to `PdfViewerStreamManager` or fail at a precise, logged
boundary immediately before that handoff.

This is still not a visible-rendering experiment. A Pass means the code builds
without Chrome's broad plugin implementation and proves the PDF stream reaches,
or precisely fails before, the stream-manager handoff. Visible PDF rendering,
MimeHandlerView renderer wiring, `pdf_viewer_private`, `PdfHost`, and full
viewer behavior remain follow-up layers unless this experiment happens to reach
them naturally.

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
   disable it before installing the new response handoff. PDF navigation must be
   allowed to reach the response stage.

3. Add a TermSurf-owned PDF response interceptor.

   Add:

   ```text
   chromium/src/content/libtermsurf_chromium/ts_pdf_response_interceptor.h
   chromium/src/content/libtermsurf_chromium/ts_pdf_response_interceptor.cc
   ```

   The implementation should be modeled on Chromium's
   `PluginResponseInterceptorURLLoaderThrottle`, but stripped to the PDF-only
   path.

   Requirements:
   - derive from the Chromium URL loader throttle interface used by
     `ContentBrowserClient::CreateURLLoaderThrottles()`;
   - inspect the post-sniff response MIME type;
   - handle only `application/pdf`;
   - leave all non-PDF responses untouched;
   - do not call `PluginUtils::GetExtensionIdForMimeType()`;
   - do not require `ExtensionRegistry` lookup for the first pass;
   - hard-code only the PDF viewer extension identity needed for the stream
     handoff, with a comment explaining that this is the PDF-only equivalent of
     Electron's embedder-owned plugin-utils path;
   - log:

     ```text
     [issue-789-exp2] pdf-response-throttle-created
     [issue-789-exp2] pdf-response url=<url> mime=<mime> destination=<destination> oopif_pdf=<true|false>
     ```

   The response interceptor must not include
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
   - create the narrow stream representation required by Chromium's PDF stream
     manager, preferably `extensions::StreamContainer` if it can be linked
     without dragging in the failed Chrome product graph;
   - call `PdfViewerStreamManager::Create(web_contents)`;
   - call
     `PdfViewerStreamManager::AddStreamContainer(frame_tree_node_id, internal_id, stream_container)`
     if the manager is linkable;
   - log:

     ```text
     [issue-789-exp2] stream-dispatch frame_tree_node_id=<id> extension_id=<id> handler_url=<url> internal_id=<id>
     [issue-789-exp2] stream-container-added internal_id=<id>
     ```

   If linking `PdfViewerStreamManager` requires `//chrome/browser/pdf:pdf` and
   that target reproduces the broad link failure, do not silently expand the
   dependency surface. Mark the experiment Partial, record the exact missing
   symbols or broad deps, and design the next experiment around extracting or
   replacing that stream-manager layer.

5. Install the TermSurf response interceptor.

   Edit:

   ```text
   chromium/src/content/libtermsurf_chromium/ts_browser_client.h
   chromium/src/content/libtermsurf_chromium/ts_browser_client.cc
   ```

   In `TsBrowserClient::CreateURLLoaderThrottles()`, append the TermSurf PDF
   response interceptor after preserving the existing shell/content-shell
   throttles.

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
   - the narrowest available PDF stream-manager target, if one exists;
   - `//chrome/browser/pdf:pdf` only if GN and link evidence show it does not
     recreate the Issue 776 Experiment 8 dependency failure.

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
   git format-patch 148.0.7778.97..HEAD -o ../../chromium/patches/issue-789/
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
   experiment.

6. Confirm the runtime logs include:

   ```text
   [issue-789-exp2] pdf-throttle-installed
   [issue-789-exp2] oopif-pdf-enabled value=<true|false>
   [issue-789-exp2] pdf-response ... mime=application/pdf
   [issue-789-exp2] stream-dispatch ...
   ```

   If the handoff reaches `PdfViewerStreamManager`, also require:

   ```text
   [issue-789-exp2] stream-container-added ...
   ```

   If the handoff fails before that line, the result must quote the last
   `[issue-789-exp2]` line and identify the exact missing dependency or API.

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
- the TermSurf-owned PDF response interceptor is installed;
- PDF responses reach the TermSurf-owned response interceptor;
- the original PDF stream reaches `PdfViewerStreamManager::AddStreamContainer()`
  or a directly equivalent stream-manager handoff;
- normal HTML browsing still works;
- non-PDF binary responses are not handled as PDFs;
- all changes are committed to a Chromium branch and archived in
  `chromium/patches/issue-789/`;
- the main repo records the result and branch metadata.

#### Partial Criteria

Partial if the implementation avoids `//chrome/browser/plugins:impl` and builds
partway, but a narrower downstream PDF layer proves missing or too broad. Valid
Partial outcomes include:

- the response interceptor builds, sees `application/pdf`, and creates the
  transferable loader, but `PdfViewerStreamManager` cannot be linked without
  pulling in broad Chrome product dependencies;
- `extensions::StreamContainer` cannot be constructed without importing a larger
  extension subsystem than expected;
- OOPIF PDF is disabled at runtime;
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
