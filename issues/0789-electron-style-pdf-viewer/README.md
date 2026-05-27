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
