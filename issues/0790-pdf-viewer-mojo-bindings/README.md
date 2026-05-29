+++
status = "open"
opened = "2026-05-28"
+++

# Issue 790: Expose Mojo JS Bindings to the PDF Viewer Frame

## Goal

Make Chromium's PDF viewer JavaScript run to completion in Roamium by exposing
the `Mojo` JS bindings interface to the PDF viewer frame, so
`chrome://resources/mojo/mojo/public/js/bindings.js` finds its `Mojo` global and
the viewer's `init()` runs. This is the next layer standing between a viewer
that reaches `getStreamInfo()` and a PDF that actually renders.

This issue continues directly from Issue 789.

## Background

### The larger goal

Opening a PDF with `web file.pdf` should render a working inline PDF viewer
inside Roamium (TermSurf's Chromium browser binary). Roamium is built on
Chromium's `content_shell`-style embedding, so it does not inherit Chrome's full
PDF viewer feature stack. The strategy — established across the prior issues —
is the **Electron model**: TermSurf does not turn Roamium into Chrome. It
provides TermSurf-owned glue for the specific pieces Chrome's PDF viewer
normally owns, mirroring only the narrow embedder hooks Electron uses, and never
importing Chrome's broad product subsystems.

### Project lineage (inline PDF rendering in Roamium)

- [Issue 776: PDF files show blank white screen instead of rendering](../0776-pdf-not-loading/README.md)
  — **closed.** Investigated the failure and proved that PDF rendering is not
  fixed by any single PDFium plugin toggle, wrapper page, MIME mapping, or
  direct link to Chrome's full browser implementation. Established that TermSurf
  needs its own small Electron-style embedder layer.
- [Issue 789: Electron-Style PDF Viewer Infrastructure](../0789-electron-style-pdf-viewer/README.md)
  — **closed.** Built that embedder layer across seven experiments. Result: the
  PDF stream handoff works (`TsPdfStreamStore`, response throttle, stream
  delegate), the viewer shell loads, the attach bookkeeping identifies the
  viewer frame, the `chrome.mimeHandlerPrivate` shim is installed, and — after
  solving `chrome://resources` loading as a **two-layer** problem (a
  browser-side WebUI URL-loader factory in Exp 6 plus a renderer-side
  origin-access grant in Exp 7) — the viewer's JS module graph executes and the
  viewer calls `getStreamInfo()`.
- **Issue 790 (this issue)** — continues from the exact point Issue 789 stopped.

### Where Issue 789 left off

Issue 789 Experiment 7 reached a **Pass (Stretch)**: with both halves of the
`chrome://resources` path in place, the viewer modules load and execute, and the
viewer calls the Experiment 5 `mimeHandlerPrivate.getStreamInfo()` shim, which
returns the correct stream metadata. The viewer then fails at a new, distinct
layer. The renderer logs, in order:

```text
[issue-789-exp5] viewer-api-call ... api=mimeHandlerPrivate method=getStreamInfo
[issue-789-exp5] get-stream-info ... result=ok
Uncaught ReferenceError: Mojo is not defined
    source: chrome://resources/mojo/mojo/public/js/bindings.js
Uncaught (in promise) TypeError: viewer.init is not a function
    source: chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/pdf/main.js
```

`chrome://resources/mojo/mojo/public/js/bindings.js` is now **served** (Issue
789 fixed that), but it references a `Mojo` global that does not exist in the
PDF viewer frame, so the bindings module throws. The downstream
`viewer.init is not a function` is a consequence: the viewer object never
finishes constructing because the Mojo bindings layer it depends on failed to
initialize.

The screenshot at the end of Issue 789 is still a blank viewer shell: the viewer
chrome never builds because `init()` does not run.

## Analysis

### What `Mojo` is and why it is missing

`Mojo` is Chromium's IPC layer. Chrome's PDF viewer is a privileged WebUI-style
frame that talks to its browser-side host and the PDF plugin over Mojo, using
the JavaScript bindings in `chrome://resources/mojo/...`. Those bindings require
the renderer frame to have **Mojo JS bindings enabled** — i.e. the frame must be
granted a `Mojo` interface object wired to a browser-side interface broker.
Chrome normally enables this for WebUI frames via the WebUI bindings policy
(historically `BINDINGS_POLICY_MOJO_WEB_UI` / `AllowBindings`), and there are
narrower per-frame mechanisms as well (e.g. enabling Mojo JS for a specific
frame or `RenderFrame`).

Roamium's content-shell base does not grant Mojo JS bindings to the PDF viewer
frame, so `window.Mojo` is undefined and the bindings module throws.

### The shape of the fix (to be determined by research)

The fix must mirror Issue 789's discipline: enable Mojo JS bindings **only for
the PDF viewer frame**, not broadly, and without importing Chrome's WebUI
controller stack, the extensions stack, GuestView, or MimeHandlerView. Candidate
mechanisms to investigate (the same research approach used in Issue 789 — trace
the Chromium source, find the legitimate caller, and check Electron's solution
in the local checkout):

- A per-frame Mojo-JS enable hook (e.g. enabling Mojo JS bindings on the viewer
  `RenderFrame`, or providing a frame-scoped interface broker) applied at the
  point the viewer frame commits — paralleling how Issue 789 gated the
  `chrome://resources` factory and the origin-access grant to the viewer frame.
- The browser-side grant that authorizes a frame to receive Mojo JS bindings
  (the WebUI bindings policy or its narrowest embedder-facing equivalent),
  scoped to the PDF viewer frame identified by the Issue 789 `TsPdfStreamStore`
  attach bookkeeping.
- How Electron exposes Mojo JS (or avoids needing it) for its embedded PDF
  viewer, read from the local Electron checkout.

There is real uncertainty here about which mechanism is both sufficient and
narrow, and about timing (Mojo JS must be enabled before the viewer's module
graph runs). The first experiment will resolve that by tracing the Chromium
source and Electron's approach before any code change, consistent with how Issue
789's experiments were designed.

### Constraints carried forward from Issue 789

- **Stay narrow.** Enable Mojo JS for the PDF viewer frame only. Do not enable
  it process-wide or for arbitrary frames.
- **No forbidden subsystems.** `content/libtermsurf_chromium` must continue to
  avoid `//chrome/browser/plugins:impl`,
  `//chrome/browser/extensions:extensions`, `//components/guest_view/browser`,
  and the broad WebUI controller / extensions browser-and-renderer stacks.
- **Preserve prior layers.** The Issue 789 stream handoff, attach bookkeeping,
  `mimeHandlerPrivate` shim, `chrome://resources` browser factory, and
  renderer-side origin-access grant must all keep working.
- **One experiment at a time.** Each experiment isolates one layer, records a
  result, and informs the next. Reaching the inner PDF plugin / content
  navigation (the layer after Mojo) is explicitly out of scope until Mojo JS is
  working.
- **Every Chromium change gets its own branch** (`148.0.7778.97-issue-790-expN`,
  forked from the last Issue 789 branch `148.0.7778.97-issue-789-exp7`) and is
  archived to `chromium/patches/`.

## Experiments

### Experiment 1: Enable Mojo JS on the PDF viewer frame via a minimal broker

#### Description

Resolve the `Mojo is not defined` failure by enabling Mojo JS bindings on the
PDF viewer frame, so the bindings module initializes and the viewer's `init()`
runs. Do it with the narrowest, most-restrictive mechanism, and make the
mechanism double as a probe that reveals which Mojo interfaces the viewer needs
next.

Research into the Chromium source (verified, not assumed) settled two facts that
shape this experiment:

- **The viewer frame is a subframe, so the context-only enable does not work.**
  In `RenderFrameImpl::DidCreateScriptContext`
  (`content/renderer/render_frame_impl.cc`), the context-only path
  (`enable_mojo_js_bindings_` / `BindingsPolicyValue::kMojoWebUi`) is gated on
  `IsMainFrame()`. The PDF viewer is the embedded extension subframe (frame tree
  node 2, parent 1 in the Issue 789 logs), so that path can never enable it. The
  **broker** path —
  `if (world_id == GLOBAL && mojo_js_interface_broker_.is_valid()) EnableMojoJSAndUseBroker(...)`
  — has no `IsMainFrame()` restriction; its own comment says "MojoJS interface
  broker can be enabled on subframes, and will limit the interfaces JavaScript
  can request to those provided in the broker." So the broker variant is the
  only one that works here, and it is also the more secure one (it restricts the
  interface set).

- **Mojo JS is never enabled by origin.** The gate is
  `kMojoWebUi || enable_mojo_js_bindings_ || valid broker` — there is no
  automatic enable for `chrome-extension://` origins. (A survey of Electron
  suggested component-extension frames get Mojo JS automatically; the Issue 789
  runtime evidence — `Mojo is not defined` on exactly such a frame — disproves
  that. Electron most likely runs the full extensions renderer, which TermSurf
  does not. TermSurf must enable it explicitly.)

The mechanism is
`RenderFrameHostImpl::EnableMojoJsBindingsWithBroker( mojo::PendingRemote<blink::mojom::BrowserInterfaceBroker>)`,
called before the viewer frame commits — the same hook WebUI uses
(`WebUIImpl::SetUpMojoInterfaceBroker` at `ReadyToCommitNavigation`). WebUI
passes a chrome-specific `PerWebUIBrowserInterfaceBroker`, which TermSurf must
not pull in; instead TermSurf supplies its own minimal broker.

For Experiment 1 that broker is intentionally **empty**: it implements
`blink::mojom::BrowserInterfaceBroker` and, for every `GetInterface` request,
logs the requested interface name and drops it. This is the most secure possible
starting point (the viewer JS gets the `Mojo` global but can reach no browser
interface), it unblocks `Mojo is not defined` so `init()` runs, and its log
output enumerates exactly which interfaces the viewer tries to bind — directly
informing Experiment 2.

#### Changes

1. New Chromium branch `148.0.7778.97-issue-789-exp7` →
   `148.0.7778.97-issue-790-exp1`. Add it to `chromium/README.md`.

2. New `content/libtermsurf_chromium/ts_pdf_mojo_interface_broker.{h,cc}`: a
   `TsPdfMojoInterfaceBroker` implementing
   `blink::mojom::BrowserInterfaceBroker`.
   `GetInterface(mojo::GenericPendingReceiver receiver)` logs
   `[issue-790-exp1] mojo-js-interface-requested name=<receiver.interface_name()>`
   and drops the receiver (lets it close). Add to `BUILD.gn`.

3. Add a minimal, non-WebUI Mojo-JS-with-broker entry point on
   `RenderFrameHostImpl`. The existing public
   `RenderFrameHostImpl::EnableMojoJsBindingsWithBroker(...)` is **not callable
   here**: it `CHECK(GetWebUI())`s (its comment: the broker's ownership is
   transferred to the frame's `WebUIController`). Our PDF viewer frame has no
   `WebUI`, so calling it would crash. But the underlying renderer call it makes
   —
   `GetFrameBindingsControl()->EnableMojoJsBindingsWithBroker(std::move(broker))`
   — is exactly what we need, and `GetFrameBindingsControl()` is private to
   `RenderFrameHostImpl`. So add a small sibling method (Chromium fork patch to
   `content/browser/renderer_host/render_frame_host_impl.{h,cc}`), e.g.
   `EnableMojoJsBindingsWithBrokerNoWebUI(broker)`, that forwards to
   `GetFrameBindingsControl()->EnableMojoJsBindingsWithBroker(std::move(broker))`
   **without** the `CHECK(GetWebUI())`. This is safe for our use: TermSurf keeps
   the broker alive with a self-owned receiver, so no `WebUIController`
   ownership transfer is involved. Do not weaken the existing CHECK'd method —
   add a parallel one so the WebUI invariant is untouched for every other
   caller.

4. In `TsPdfStreamStore::ReadyToCommitNavigation`
   (`content/libtermsurf_chromium/ts_pdf_stream_store.cc`), before the existing
   logic, gate on the committing frame being the active PDF viewer host frame
   (`IsPdfExtensionHostFrame(navigation_handle->GetRenderFrameHost())`, the same
   identity check Issue 789 Exp 6/7 used). When it matches, enable Mojo JS with
   a fresh self-owned broker via the new method:

   ```cpp
   mojo::PendingRemote<blink::mojom::BrowserInterfaceBroker> broker;
   mojo::MakeSelfOwnedReceiver(std::make_unique<TsPdfMojoInterfaceBroker>(),
                               broker.InitWithNewPipeAndPassReceiver());
   static_cast<RenderFrameHostImpl*>(rfh)
       ->EnableMojoJsBindingsWithBrokerNoWebUI(std::move(broker));
   ```

   Log `[issue-790-exp1] mojo-js-enabled frame_tree_node_id=<id>`. Guard so it
   is enabled at most once per viewer frame. (`RenderFrameHostImpl` is reachable
   because `content/libtermsurf_chromium` is part of the `content` component, as
   `WebUIImpl` does the same cast.)

5. Preserve all Issue 789 behavior. The gate reuses the existing
   `IsPdfExtensionHostFrame` identity check; no other navigation behavior
   changes.

6. Preserve dependency boundaries: no `//chrome/browser/plugins:impl`,
   `//chrome/browser/extensions:extensions`, `//components/guest_view/browser`,
   no WebUI controller stack. The broker is a plain `blink::mojom`
   implementation; `EnableMojoJsBindingsWithBroker` is a `content`-internal
   call.

7. Format (`clang-format`, `gn format BUILD.gn`), build
   (`autoninja -C out/Default libtermsurf_chromium`), and regenerate the patch
   archive.

#### Verification

1. Build; confirm forbidden deps still absent (`gn desc`).

2. Bitcoin PDF smoke
   (`test-issue-776-pdf.sh http://localhost:9616/bitcoin.pdf`). Required for
   Pass:
   - `[issue-790-exp1] mojo-js-enabled frame_tree_node_id=<id>` for the viewer
     frame;
   - no `Uncaught ReferenceError: Mojo is not defined`;
   - `[issue-790-exp1] mojo-js-interface-requested name=...` lines enumerating
     the interfaces the viewer requests (record them — they define Experiment
     2);
   - the viewer advances past the Issue 789 stopping point (`viewer.init` runs;
     name the new failure precisely).
3. Capture and classify the screenshot (same buckets as Issue 789).
4. HTML and non-PDF binary smoke (`index.html`, `test.bin`): no
   `[issue-790-exp1] mojo-js-enabled` line (the gate must not fire for
   non-viewer frames), and no regression in normal behavior.
5. Negative check: confirm no normal frame gets Mojo JS — the `mojo-js-enabled`
   log must appear only for the PDF viewer frame.

#### Pass Criteria

- Builds and links; forbidden deps absent.
- `Mojo is not defined` is gone; the viewer frame has the `Mojo` global.
- Mojo JS is enabled only for the PDF viewer frame (not normal HTML/binary
  frames).
- The viewer's requested interfaces are logged, and the viewer advances to a
  new, precisely named failure.
- HTML and non-PDF binary smoke show no regression.

#### Partial Criteria

Partial if the `Mojo` global appears but the viewer still cannot proceed for a
narrower, named reason (e.g. `init()` runs but immediately needs an interface
the empty broker drops, which is expected and informs Experiment 2; or a
different renderer error surfaces). Name the exact next failure and the
requested interface names.

#### Failure Criteria

- Mojo JS is enabled for frames other than the PDF viewer frame (over-broad).
- The build pulls in a forbidden subsystem to obtain the broker or the enable
  call.
- Normal HTML or non-PDF binary behavior regresses.
- `Mojo is not defined` persists (mechanism or timing wrong — e.g. the broker is
  applied too late, after the viewer's script context is created).

#### Result

**Result:** Pass

Enabling Mojo JS on the viewer frame resolved `Mojo is not defined`, the viewer
ran `init()`, and it advanced all the way to instantiating the inner PDF plugin
before hitting the next layer — a much larger jump than expected.

Chromium branch `148.0.7778.97-issue-790-exp1` (from
`148.0.7778.97-issue-789-exp7`). Changes:

- `content/browser/renderer_host/render_frame_host_impl.{h,cc}` —
  `EnableMojoJsBindingsWithBrokerNoWebUI(broker)`, identical to the existing
  broker method minus the `CHECK(GetWebUI())` (Codex caught that the standard
  method would crash on our non-WebUI frame). Safe because the broker is kept
  alive by a self-owned receiver, not a `WebUIController`.
- `content/libtermsurf_chromium/ts_pdf_mojo_interface_broker.{h,cc}` — a
  `blink::mojom::BrowserInterfaceBroker` that logs each `GetInterface` request
  and drops it (empty allowlist).
- `content/libtermsurf_chromium/ts_pdf_stream_store.cc` — in
  `ReadyToCommitNavigation`, when the committing frame is the active PDF viewer
  host frame (`IsPdfExtensionHostFrame`), enable Mojo JS with a fresh self-owned
  broker via the new method.

Verification:

- **Build / deps.** Builds and links; forbidden deps still absent. (Touches core
  `RenderFrameHostImpl`, which is `content`, not a forbidden product subsystem.)
- **Mojo global present.**
  `[issue-790-exp1] mojo-js-enabled frame_tree_node_id=2` fires for the viewer
  frame, and `Mojo is not defined` is gone (0 occurrences, down from the Issue
  789 failure).
- **Subframe confirmed.** The viewer is frame tree node 2 (a subframe), so the
  broker variant was indeed required; the context-only path would have been a
  no-op.
- **Interface probe.** The empty broker logged exactly one request:
  `[issue-790-exp1] mojo-js-interface-requested name=help_bubble.mojom.PdfHelpBubbleHandlerFactory`.
  (Notably the viewer did **not** block on a missing core PDF host interface at
  this stage — it proceeded to create the plugin.)
- **Viewer advanced far.** `viewer.init` ran (no console errors), the viewer
  re-reached `getStreamInfo()`, and then created the inner PDF content
  navigation with the real internal plugin mime
  (`application/x-google-chrome-pdf`).
- **Next failure (new layer).** The renderer then crashed:
  `FATAL: components/pdf/renderer/internal_plugin_renderer_helpers.cc:61] Check failed: IsPdfRenderer().`,
  in `pdf::CreateInternalPlugin`. The PDF plugin must be created in a process
  designated as a PDF renderer (the `IsPdfRenderer()` / `--pdf-renderer`
  machinery the Issue 776 logs already tracked). The `plugin-context` log shows
  `parent_is_remote=true` — the PDF content frame is an out-of-process child of
  the extension viewer frame, and that process is not flagged as a PDF renderer.
- **Screenshot.** Grey/blank overlay (renderer crashed at the CHECK).
- **HTML and non-PDF binary smoke.** `index.html` and `test.bin`: 0
  `mojo-js-enabled` lines and 0 FATAL/crash lines. The Mojo gate fires only for
  the PDF viewer frame; no regression.

#### Conclusion

The empty logging broker was the right call: it unblocked `Mojo is not defined`
with the most restrictive possible grant, and its one logged request plus the
subsequent progression showed the viewer needs almost nothing from the broker at
this stage — it ran `init()` and went straight to plugin creation. The
Codex-flagged `CHECK(GetWebUI())` would otherwise have crashed us immediately;
the no-WebUI sibling method is the clean fix.

The renderer crash is not a regression (HTML/binary paths are unaffected); it is
the next layer surfacing. The viewer has now traversed the entire JS path —
shell, resources, Mojo, init, getStreamInfo, plugin element — and the remaining
work is in the **process model**: the inner PDF plugin must run in a PDF
renderer process.

Next layer (Experiment 2): satisfy `IsPdfRenderer()` for the frame that hosts
the internal PDF plugin — i.e. get the PDF content frame into a process
designated as a PDF renderer (the `--pdf-renderer` process flag / the OOPIF PDF
process path), or route plugin creation so the CHECK is satisfied. This is the
process-model layer beneath the JS the viewer has now fully executed.

### Experiment 2: Enable the OOPIF PDF process path

#### Description

Make the PDF content frame land in a renderer process designated as a PDF
renderer, so `pdf::IsPdfRenderer()` is true and `CreateInternalPlugin` no longer
crashes. Research (verified in source) traced the full chain and found a single
root cause.

The designation is automatic in Chromium **once a navigation is marked
`is_pdf`**:

```text
PdfNavigationThrottle sets OpenURLParams.is_pdf=true   components/pdf/browser/pdf_navigation_throttle.cc:117
  -> NavigationRequest::is_pdf_ -> UrlInfo.is_pdf
  -> SiteInfo::is_pdf_  (also forces a dedicated process)   content/browser/site_info.cc
  -> SiteInstanceImpl::IsPdf()
  -> RenderProcessHostImpl sets RenderProcessFlags::kPdf      render_process_host_impl.cc:1574
  -> RenderProcessHostImpl appends switches::kPdfRenderer      render_process_host_impl.cc:3621
  -> renderer: pdf::IsPdfRenderer() == true                   internal_plugin_renderer_helpers.cc:30
```

Electron does **nothing** special for this — it relies entirely on the stock
throttle. But the throttle is gated:

```text
PdfNavigationThrottle::MaybeCreateThrottleFor / WillStartRequest
  if (!chrome_pdf::features::IsOopifPdfEnabled()) { return; }   pdf_navigation_throttle.cc:34
```

The Issue 790 Exp 1 run confirms the consequence empirically: every renderer
logged `host_is_pdf=false has_pdf_renderer=false`, and the viewer template
logged `pdf_oopif_enabled=absent`. So the OOPIF PDF feature
(`chrome_pdf::features::kPdfOopif`) is **off**, the throttle early-returns, no
navigation is ever marked `is_pdf`, and the PDF content process is never
designated — exactly why the plugin CHECK crashes.

Issue 789 deliberately built the **non-OOPIF** `mimeHandlerPrivate` viewer path.
But the modern PDF process model — the one Chrome and Electron ship — is the
OOPIF path, and the internal plugin's `IsPdfRenderer()` CHECK is part of it. So
the correct, "works exactly like Chrome" architecture is to enable OOPIF PDF.
This experiment makes the minimal change — turn the feature on — and observes
the consequences, because enabling it touches the viewer-attach path Issue 789
built non-OOPIF. The experiment is intentionally a small, high-information
probe: flip the feature, then learn whether the existing attach still works,
whether `is_pdf` now flows and the crash resolves, and what (if anything) the
OOPIF path needs reworked.

#### Changes

1. New Chromium branch `148.0.7778.97-issue-790-exp1` →
   `148.0.7778.97-issue-790-exp2`. Add it to `chromium/README.md`.

2. **Diagnose before flipping (Codex).** The viewer template's
   `pdf_oopif_enabled=absent` seen in Issue 789 may be a TermSurf-hardcoded
   template replacement (set where the Exp 5 viewer shell is built in
   `ts_pdf_viewer_url_loader_factory.cc`), which is **independent** of the real
   `chrome_pdf::features::IsOopifPdfEnabled()` feature/policy state. Flipping
   the feature while the template still hardcodes "absent" (or vice versa) would
   give an ambiguous result. So first log all three and reconcile them:
   - `[issue-790-exp2] oopif-state feature=<IsOopifPdfEnabled()> ...` (the real
     `chrome_pdf::features` state, including any policy bool it consults);
   - the actual `pdf_oopif_enabled` value TermSurf injects into the viewer
     template, and where it is set.

   Then enable OOPIF where it is actually off: if `IsOopifPdfEnabled()` is
   false, turn on `chrome_pdf::features::kPdfOopif` in the narrowest clean way
   (preferred: a scoped default-state override in TermSurf's main/feature setup,
   part of the build, not a hand-typed `--enable-features` flag); and if the
   viewer template hardcodes OOPIF off, make the template reflect the real
   feature state. Keep the `[issue-790-exp2] oopif-state ...` log so the result
   records the reconciled state.

3. Preserve all prior layers. Do not delete the Issue 789 non-OOPIF attach code
   in this experiment; observe whether OOPIF supersedes it. If OOPIF changes the
   viewer-attach flow, record exactly what changes — that informs whether later
   experiments simplify or remove the non-OOPIF path.

4. Preserve dependency boundaries (no forbidden subsystems). Enabling a
   `chrome_pdf` feature does not add a forbidden dependency.

5. Format, build, regenerate the patch archive.

#### Verification

1. Build; confirm forbidden deps absent.
2. Bitcoin PDF smoke. Record:
   - `[issue-790-exp2] oopif-state feature=true` with the viewer template
     `pdf_oopif_enabled` now present/true (the two reconciled);
   - **Decisive proof the throttle ran (Codex):** `PdfNavigationThrottle` fired
     for the inner content navigation — the Issue 789 `MapToOriginalUrl(...)`
     log with the same `internal_id`, then
     `[issue-776-exp4] append-command-line ... host_is_pdf=true has_pdf_renderer=true`
     for the PDF content process. Without this chain a "pass" could conflate
     feature state with unrelated process behavior;
   - whether the `Check failed: IsPdfRenderer()` crash is gone;
   - how far the viewer now gets (does the PDF plugin instantiate? does a page
     render? name the next failure precisely);
   - **Which viewer API path fires (Codex):** confirm whether the Issue 789
     attach sequence (`attach-handler-committed`, `viewer-api-installed`, Exp 1
     `mojo-js-enabled`, the `chrome://resources` factory, `getStreamInfo`) still
     fires unchanged under OOPIF — the `mimeHandlerPrivate` hybrid path — or
     whether OOPIF routes the viewer through a different (`pdfViewerPrivate`)
     attach so our hooks no longer match. Record exactly which hooks still
     match.
3. Screenshot: classify (PDF rendered / advanced-but-blank / new crash / etc.).
   If the PDF renders, test first-page scroll.
4. HTML and non-PDF binary smoke: no regression; normal pages unaffected by the
   feature flip.

#### Pass Criteria

- Builds; forbidden deps absent.
- `IsOopifPdfEnabled()` is true; the `IsPdfRenderer()` crash is gone; the PDF
  content process is PDF-designated (`host_is_pdf=true`).
- The viewer advances past plugin creation to a new, precisely named layer (or
  the PDF renders — the ultimate goal).
- HTML and non-PDF binary smoke show no regression.

Stretch Pass: the PDF visibly renders.

#### Partial Criteria

Partial if the crash is resolved and the process is designated, but the viewer
attach regresses under OOPIF (e.g. the Issue 789 `mimeHandlerPrivate` attach no
longer fires and the viewer needs the OOPIF attach path) or a new layer blocks
rendering. Name the exact next failure and whether it is an OOPIF-attach gap.

#### Failure Criteria

- Enabling OOPIF breaks the viewer entirely (no attach, no `getStreamInfo`) with
  no clear next step — would require reconsidering the OOPIF-vs-non-OOPIF
  decision.
- Normal HTML or non-PDF binary behavior regresses.
- The crash persists despite `IsOopifPdfEnabled()` being true (mechanism wrong;
  re-investigate where `is_pdf` is lost).
