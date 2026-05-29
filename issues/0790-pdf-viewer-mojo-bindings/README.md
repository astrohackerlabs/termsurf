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

#### Result

**Result:** Partial

The diagnostic disproved the experiment's own hypothesis — OOPIF PDF is
**already enabled** — and pinpointed the real blocker: the viewer is running in
non-OOPIF mode despite the feature being on.

Chromium branch `148.0.7778.97-issue-790-exp2` (from `-exp1`). Change: a
diagnostic log in `TsBrowserClient::CreateThrottlesForNavigation`
(`ts_browser_client.cc`) recording `IsOopifPdfEnabled()` and
`FeatureList::IsEnabled(kPdfOopif)`. No functional change (log only, inside the
existing `HasStreams` PDF branch), so no regression possible.

Findings:

- **OOPIF is already on.** `[issue-790-exp2] oopif-state combined=1 feature=1`.
  Verified in source: `kPdfOopif` is `ENABLED_BY_DEFAULT` on non-ChromeOS
  (`pdf/pdf_features.cc:30`), and `g_is_oopif_pdf_policy_enabled` defaults to
  `true` (`pdf/pdf_features.cc:15`), so `IsOopifPdfEnabled()` is true with no
  action. The Exp 2 premise (turn the feature on) was wrong.
- **But the process is still not designated.** All renderers remain
  `host_is_pdf=false has_pdf_renderer=false`, and the
  `Check failed: IsPdfRenderer()` crash is unchanged.
- **Root cause reframed.** The Exp 1 `plugin-context` log shows the internal
  plugin being created in the **PDF extension viewer frame itself**
  (`frame_origin=chrome-extension://…mhjfbmdgcf…`,
  `parent_origin=http://localhost`), not in a separate PDF content OOPIF. That
  is the **non-OOPIF** plugin-creation path. Combined with the Issue 789
  `viewer-template pdf_oopif_enabled=absent` (the served viewer HTML lacks the
  `pdfOopifEnabled` flag), the conclusion is: even though the Chromium _feature_
  is on, the **viewer JS runs in non-OOPIF mode** because TermSurf serves the
  PDF viewer resources without injecting the `pdfOopifEnabled` loadTimeData that
  Chrome's PDF WebUI normally provides. So the viewer never creates the separate
  PDF content frame that the `PdfNavigationThrottle` would mark `is_pdf` (and
  that would get a `--pdf-renderer` process); instead it makes the plugin
  in-frame and crashes the `IsPdfRenderer()` CHECK.
- **Pivot understood (Codex's warning realized).** The mismatch Codex flagged —
  Issue 789's non-OOPIF `mimeHandlerPrivate` attach vs. the OOPIF process model
  — is exactly what bites here. The fix is not a feature flip; it is making the
  viewer actually run in OOPIF mode.

#### Conclusion

Experiment 2 was a cheap, high-value probe: one log line eliminated the "enable
the feature" dead-end and converted a vague "process model" problem into a
specific one — **the viewer runs non-OOPIF, so no PDF content process is ever
created.** The internal-plugin `IsPdfRenderer()` CHECK is fundamentally part of
the OOPIF process model, so the viewer must take the OOPIF path for the plugin
to live in a PDF-designated process.

Next layer (Experiment 3): make the PDF viewer run in OOPIF mode — inject the
`pdfOopifEnabled` loadTimeData (and any companion flags Chrome's PDF WebUI sets)
into the viewer HTML TermSurf serves, so the viewer JS takes the OOPIF
content-frame path: it creates a child PDF content frame that navigates to the
stream, `PdfNavigationThrottle::WillStartRequest` marks that navigation
`is_pdf=true`, the content frame gets a `--pdf-renderer` process, and
`CreateInternalPlugin` runs there with `IsPdfRenderer()` satisfied. Research
must confirm where the viewer reads `pdfOopifEnabled` and where TermSurf serves
the viewer template (`ts_pdf_viewer_url_loader_factory.cc` /
`ts_pdf_component_extension_resource_manager.cc`), and whether the OOPIF attach
needs the Issue 789 `mimeHandlerPrivate` hooks adjusted.

### Experiment 3: Flip the viewer to OOPIF mode and shim pdfViewerPrivate

#### Description

Make the viewer run in OOPIF mode and observe whether TermSurf's custom
`TsPdfStreamStore` stack can drive the OOPIF content-frame flow (so the PDF
content frame gets a `--pdf-renderer` process and `CreateInternalPlugin`
succeeds), or whether OOPIF requires `//chrome/browser/pdf`'s
`PdfViewerStreamManager`. This is a deliberately cheap, high-information probe
at an architectural fork.

Research (verified in source) established the mechanics:

- The viewer chooses its mode from an HTML attribute: `pdf_viewer_base.ts:159`
  reads `document.documentElement.hasAttribute('pdfOopifEnabled')`, set by the
  `$i18n{pdfOopifEnabled}` template replacement in the viewer `index.html`.
- TermSurf hardcodes that replacement to empty
  (`ts_pdf_component_extension_resource_manager.cc:168`
  `replacements["pdfOopifEnabled"] = ""`), forcing non-OOPIF. Chrome/Electron
  set it from `IsOopifPdfEnabled()` (`pdf_extension_util.cc:86`).
- **In OOPIF mode the viewer calls `chrome.pdfViewerPrivate.getStreamInfo()`,
  not `chrome.mimeHandlerPrivate.getStreamInfo()`** (`browser_api.ts:182`), plus
  `pdfViewerPrivate` for `onSave`, `setPdfPluginAttributes`,
  `setPdfDocumentTitle`. The Exp 5 shim
  (`ts_pdf_viewer_url_loader_factory.cc:81`) **already defines
  `chrome.pdfViewerPrivate.getStreamInfo` plus event/stub members** (Codex), so
  this experiment audits/logs the existing shim rather than adding a duplicate.
- Chrome/Electron's OOPIF browser side uses `//chrome/browser/pdf`
  `PdfViewerStreamManager`, but that is Chrome's stream store / frame
  bookkeeping, **not** the unavoidable creator of the PDF content frame (Codex).
  The content frame is created by the viewer's external-plugin handling
  (`IsPluginHandledExternally`) and navigated to the stream URL, which
  TermSurf's `PdfNavigationThrottle` + `TsPdfStreamDelegate::MapToOriginalUrl`
  already intercept. So the custom `TsPdfStreamStore` stack can in principle
  drive OOPIF without `//chrome/browser/pdf` — this experiment tests that.
- **The decisive missing piece (Codex):** in OOPIF mode the PDF content frame is
  created only if `TsRendererClient::IsPluginHandledExternally()` routes the
  `application/pdf` embed externally (the Electron
  `RendererClientBase::IsPluginHandledExternally` →
  `MimeHandlerViewContainerManager::CreateFrameContainer()` model). TermSurf's
  `IsPluginHandledExternally` currently returns the base value (false), so the
  plugin is created in-frame and crashes. Flipping `pdfOopifEnabled` and
  shimming `pdfViewerPrivate` alone would reproduce the same in-frame crash.
  This experiment must also address external plugin handling, or explicitly
  treat "content frame not created" as the expected Partial.

This is the right architectural direction (the internal-plugin `IsPdfRenderer()`
CHECK is part of the OOPIF model; the non-OOPIF MimeHandlerView path is legacy).
But rather than commit up front to adopting `PdfViewerStreamManager`, the
experiment flips the flag and extends the shim, then reads the logs to see how
far the custom stack carries OOPIF — turning the open question ("can our stack
do OOPIF, or must we adopt chrome/browser/pdf?") into evidence.

#### Changes

1. New Chromium branch `148.0.7778.97-issue-790-exp2` →
   `148.0.7778.97-issue-790-exp3`. Add it to `chromium/README.md`.

2. Set the viewer template's `pdfOopifEnabled` from the real feature state
   instead of hardcoded empty
   (`ts_pdf_component_extension_resource_manager.cc:168`):
   `replacements["pdfOopifEnabled"] = chrome_pdf::features::IsOopifPdfEnabled() ? "pdfOopifEnabled" : ""`.
   (Mirrors `pdf_extension_util.cc:86`; with OOPIF on, the attribute is present
   and the viewer takes the OOPIF path.)

3. Audit/extend TermSurf's existing viewer-API shim
   (`ts_pdf_viewer_url_loader_factory.cc`). It already defines
   `chrome.pdfViewerPrivate.getStreamInfo` plus stubs; confirm it returns the
   same stream metadata as the `mimeHandlerPrivate` path (Codex notes
   `pdfViewerPrivate.StreamInfo` omits `mimeType`/`responseHeaders` but extra
   fields are harmless, and `tabId: -1` avoids a `chrome.tabs` follow-up), and
   that `onSave`/`setPdfPluginAttributes`/`setPdfDocumentTitle`/
   `onShouldUpdateViewport`/`isAllowedLocalFileAccess` are present as no-ops.
   Ensure it logs
   `[issue-790-exp3] viewer-api-call api=pdfViewerPrivate method=...`.

4. **Route the PDF embed externally so the OOPIF content frame is created
   (Codex).** Make `TsRendererClient::IsPluginHandledExternally()` return true
   for the PDF mime (`application/pdf` / the internal plugin mime) on the PDF
   viewer frame, mirroring Electron's
   `RendererClientBase::IsPluginHandledExternally` →
   `MimeHandlerViewContainerManager::CreateFrameContainer()`, so the viewer
   creates an out-of-process content frame that navigates to the stream URL
   (which the existing throttle marks `is_pdf`) instead of instantiating the
   plugin in the extension frame. Keep this scoped to the PDF viewer frame. If
   the narrow external-handling hook proves larger than one experiment, split it
   out — but then this experiment's expected result is the Partial where the
   content frame is not yet created.

5. Preserve the Issue 789/Exp 1 hooks (stream store, attach bookkeeping,
   `chrome://resources` factory, Mojo enable). Observe whether they still fire
   under OOPIF.

6. Preserve dependency boundaries. This experiment must NOT add
   `//chrome/browser/pdf` or any forbidden subsystem — its purpose is to learn
   whether the custom stack suffices. If the result shows
   `PdfViewerStreamManager` is required, that adoption is a separate,
   explicitly-decided later experiment.

7. Format, build, regenerate patches.

#### Verification

1. Build; forbidden deps absent.
2. Bitcoin PDF smoke. Record:
   - `viewer-template pdf_oopif_enabled=true` now (attribute present);
   - which API path the viewer uses (`pdfViewerPrivate` calls now, not
     `mimeHandlerPrivate`);
   - whether a separate PDF content frame is created and navigates to the stream
     (a new child frame distinct from the extension viewer frame);
   - whether `PdfNavigationThrottle::WillStartRequest` marks it `is_pdf`
     (`MapToOriginalUrl`), and `host_is_pdf=true has_pdf_renderer=true` appears
     for the content process;
   - whether the `Check failed: IsPdfRenderer()` crash is gone and whether the
     plugin instantiates / the PDF renders;
   - name the exact next failure precisely. **Anticipated next layer (Codex):**
     even once the content process is PDF-designated, rendering may fail because
     TermSurf sets an empty `injected_script` in
     `TsPdfStreamStore::BuildStreamInfo` (`ts_pdf_stream_store.cc:77`), whereas
     Chrome injects `IDR_PDF_PDF_INTERNAL_PLUGIN_WRAPPER_ROLLUP_JS` next to the
     internal plugin (`chrome_pdf_stream_delegate.cc:196`, placed by
     `plugin_response_writer.cc`). If the plugin loads but stays blank, this is
     the likely cause and the next experiment.
3. Screenshot: classify (PDF rendered / advanced-but-blank / new crash).
4. HTML and non-PDF binary smoke: no regression.

#### Pass Criteria

- Builds; forbidden deps absent; no `//chrome/browser/pdf`.
- Viewer runs OOPIF: uses `pdfViewerPrivate`, creates the PDF content frame, the
  content process is PDF-designated, and the `IsPdfRenderer()` crash is gone.
- The viewer advances to a new precisely named layer (or the PDF renders).
- HTML and non-PDF binary smoke show no regression.

Stretch Pass: the PDF visibly renders.

#### Partial Criteria

Partial if OOPIF mode engages (pdfViewerPrivate path, attribute present) and the
crash changes, but the PDF does not yet render — e.g. the content frame is not
created because the custom stack does not drive the OOPIF content navigation the
way `PdfViewerStreamManager` would, or `getStreamInfo` via `pdfViewerPrivate`
needs more fields. Name the exact gap; if it points to needing
`PdfViewerStreamManager`, record that as the decision point for the next
experiment.

#### Failure Criteria

- OOPIF mode regresses the viewer to before Exp 1 (no `getStreamInfo`, no
  attach) with no clear next step.
- Normal HTML or non-PDF binary behavior regresses.
- Achieving OOPIF required pulling in a forbidden subsystem.

#### Result

**Result:** Partial (as anticipated)

OOPIF viewer mode now engages, but the PDF embed is still created in-frame
because TermSurf does not yet route the plugin externally — exactly the Partial
the design and Codex predicted. The next layer is `IsPluginHandledExternally`.

Chromium branch `148.0.7778.97-issue-790-exp3` (from `-exp2`). Change: one line
in `ts_pdf_component_extension_resource_manager.cc` — set the viewer template's
`pdfOopifEnabled` from `chrome_pdf::features::IsOopifPdfEnabled()` instead of
hardcoded empty. The Exp 5 shim already provided `chrome.pdfViewerPrivate`
(getStreamInfo + stubs), so no shim change was needed (Codex was right).

Verification:

- **OOPIF mode engaged.** `viewer-template pdf_oopif_enabled=true` (was
  `absent`), and the viewer now calls
  `api=pdfViewerPrivate method=getStreamInfo` instead of `mimeHandlerPrivate` —
  the viewer JS took the OOPIF branch.
- **But the crash persists, in-frame.** `Check failed: IsPdfRenderer()` still
  fires, all renderers remain `host_is_pdf=false`, and the `plugin-context` log
  is unchanged: the internal plugin is created in the **extension viewer frame**
  (`frame_origin=chrome-extension://…mhjfbmdgcf…`,
  `parent_origin=http://localhost`), not in a separate out-of-process PDF
  content frame. The viewer's `IsPluginHandledExternally` hook still returns the
  base value, so the `application/pdf` embed is handled in-frame.
- **No regression.** `index.html` and `test.bin` smoke: 0 FATAL/crash, 0
  `viewer-template` logs (the PDF viewer path does not engage for non-PDF
  content; normal pages are unaffected by the template change).
- **Screenshot:** grey/blank overlay (renderer crashed at the CHECK), same as
  Exp 1.

#### Conclusion

Experiment 3 confirmed the viewer-mode layer: a single template flag flips the
viewer from the legacy `mimeHandlerPrivate`/in-frame path to the OOPIF
`pdfViewerPrivate` path, and the existing shim already supports it. But OOPIF
mode alone does not relocate the plugin — the embed is still created in-frame
until the renderer routes it externally. This cleanly isolates the next layer.

Next layer (Experiment 4): implement external plugin handling —
`TsRendererClient::IsPluginHandledExternally()` returns true for the PDF mime on
the PDF viewer frame and routes the embed through the Electron-style
`MimeHandlerViewContainerManager::CreateFrameContainer()` path (binding the
container manager in renderer frames, per the Electron model), so the viewer
creates an out-of-process PDF content frame. That frame navigates to the stream
URL, the existing `PdfNavigationThrottle` marks it `is_pdf`, it gets a
`--pdf-renderer` process, and `CreateInternalPlugin` runs there with
`IsPdfRenderer()` satisfied. Research must confirm the renderer-side container
manager is reachable **without** `//components/guest_view/browser` or the
forbidden extensions stacks; if it is not, find the narrowest external-routing
mechanism that is.

## Architectural decision point (before Experiment 4)

Experiment 4 research (verified in Chromium + Electron) found that the OOPIF PDF
content frame cannot be created by a renderer-only change. The chain is:

```text
TsRendererClient::IsPluginHandledExternally() returns true for the PDF mime
  -> MimeHandlerViewContainerManager::CreateFrameContainer() returns true
     (renderer side; deps are SAFE: //extensions/renderer guest_view code +
      //components/guest_view/renderer + :common mojom)
  -> Blink HTMLPlugInElement::LoadOrRedirectSubframe() creates a child frame
  -> BUT the frame stays in-process unless the BROWSER hoists it into an OOPIF.
```

The browser-side hoist is provided by either:

- `//extensions/browser` guest-view `MimeHandlerViewAttachHelper`
  (`PrepareForInnerWebContentsAttach`) — transitively pulls
  `//components/guest_view/browser`, which Issue 789/790 **explicitly forbid**;
  or
- `//chrome/browser/pdf` `PdfViewerStreamManager` — the modern OOPIF path; not
  on the explicit forbidden list, but a Chrome product subsystem the "narrow
  Electron-glue" philosophy avoids, and the component Issue 789 deliberately
  reimplemented as the custom `TsPdfStreamStore`.

Electron pulls **both** `//extensions/browser` and `//chrome/browser/pdf`. The
custom `TsPdfStreamStore` stack carried the viewer all the way to plugin
creation, but it has no equivalent of the browser-side OOPIF attach. This is a
genuine fork in the project's architecture and a relaxation of a
user-established constraint, so the direction was put to the user before
spending the remaining experiment budget.

**Decision (user, 2026-05-28): adopt `//chrome/browser/pdf` — accept the full
footprint.** Verification with `gn desc //chrome/browser/pdf:pdf deps --all`
confirmed that adopting it transitively pulls `//components/guest_view/browser`,
`//extensions:extensions`, `//chrome/common/extensions:extensions`, and
`//chrome/browser/plugins` — i.e. the full Electron-style footprint, not a
narrow add. The user accepted this: TermSurf will use Chromium's canonical OOPIF
PDF infrastructure (`PdfViewerStreamManager` + the guest-view mime-handler
attach + the renderer `MimeHandlerViewContainerManager`), mirroring Electron.

**Consequence for the constraints above:** the "no forbidden subsystems"
constraint (`//components/guest_view/browser`,
`//chrome/browser/extensions:extensions`, `//chrome/browser/plugins:impl`, broad
WebUI/extensions stacks) is **retired for the PDF feature** as of this decision.
The "narrow Electron-glue" philosophy still applies everywhere else, but PDF
support adopts the canonical Chrome infrastructure. Much of Issue 789's custom
stack (`TsPdfStreamStore`, the `mimeHandlerPrivate` shim, the bespoke stream
delegate) is expected to be superseded by `PdfViewerStreamManager` /
`chrome_pdf_stream_delegate`; later experiments will remove the now-dead custom
code once the canonical path renders a PDF, so the final implementation stays
clean (no dual stacks left behind).

### Experiment 4: Adopt the canonical OOPIF PDF infrastructure (first integration)

#### Description

Begin adopting Chromium's canonical OOPIF PDF stack (the decision above): wire
the renderer-side external plugin routing and the browser-side
`PdfViewerStreamManager` so the PDF content embed becomes a real out-of-process
PDF frame. This is a large integration that may span more than one experiment;
Experiment 4 is the first concrete step and a probe of what content-shell wiring
the canonical stack still needs.

The canonical pieces (verified, mirroring Electron):

- Renderer: `ContentRendererClient::IsPluginHandledExternally()` routes the PDF
  mime through `MimeHandlerViewContainerManager::CreateFrameContainer()`, and
  the container manager interface is bound in `RenderFrameCreated()`
  (`extensions/renderer` guest-view, `//components/guest_view/renderer`).
- Browser: `PdfViewerStreamManager` (`//chrome/browser/pdf`) observes the
  WebContents, intercepts the PDF stream, and orchestrates the OOPIF attach;
  `ChromePdfStreamDelegate` supplies the stream/`injected_script`. This is what
  designates the content frame `is_pdf` and lets `CreateInternalPlugin` run in a
  `--pdf-renderer` process.

#### Changes

1. New Chromium branch `148.0.7778.97-issue-790-exp3` →
   `148.0.7778.97-issue-790-exp4`. Add it to `chromium/README.md`.

2. `content/libtermsurf_chromium/BUILD.gn`: add the canonical deps —
   `//chrome/browser/pdf:pdf`, `//extensions/renderer`,
   `//components/guest_view/renderer`, `//components/guest_view/common:mojom`,
   and whatever `PdfViewerStreamManager` registration needs. (Accept the
   guest_view/browser + extensions + plugins footprint per the decision.)

3. Renderer (`ts_renderer_client.cc`): implement `IsPluginHandledExternally()`
   for the PDF mime via `MimeHandlerViewContainerManager` (mirror Electron's
   `renderer_client_base.cc`), and bind
   `extensions::mojom::MimeHandlerViewContainerManager` in
   `RenderThreadStarted`/ the frame-created path. Keep the Exp 1 Mojo enable.

4. Browser: drive `pdf::PdfViewerStreamManager` (the canonical replacement for
   `TsPdfStreamStore`). Per Codex, registration alone is insufficient — wire
   these explicitly:
   - **Feed the captured stream in:** call `PdfViewerStreamManager::Create(...)`
     - `AddStreamContainer(...)` with the intercepted PDF stream (the same
       stream TermSurf already captures), so the manager has the data it needs.
   - **Content-shell-safe delegate:** `ChromePdfStreamDelegate` assumes a Chrome
     `Profile`, which Roamium lacks. Use a TermSurf `PdfStreamDelegate` (keep
     `TsPdfStreamDelegate`'s logic) that feeds `PdfViewerStreamManager`, rather
     than `ChromePdfStreamDelegate`, unless/until Roamium has a real `Profile`.
   - **Preserve `pdf_plugin_attributes`:**
     `ChromePdfStreamDelegate::MapToOriginalUrl` (and the canonical path)
     rejects streams without `pdf_plugin_attributes()`. Keep the Issue 789
     pre-population in `ts_pdf_stream_dispatch.cc`, or make
     `pdfViewerPrivate.setPdfPluginAttributes` actually store them — the shim's
     no-op is not acceptable on the canonical path.

5. **Swap, don't dual-stack (Codex).** Disable TermSurf's custom
   `pdf::PdfNavigationThrottle` + `PdfURLLoaderRequestInterceptor` wired to
   `TsPdfStreamDelegate` for PDF navigations when the canonical manager is
   active, so the two stacks do not double-intercept the same stream. Move the
   `chrome://resources` factory gate and the Exp 1 Mojo-enable gate off
   `TsPdfStreamStore::IsPdfExtensionHostFrame` and onto canonical PDF-frame
   recognition (e.g. `pdf_frame_util::GetEmbedderHost` /
   `PdfViewerStreamManager` frame identity), so the Issue 789 gates still fire
   for the right frame under the canonical stack. Leave the now-unused custom
   code in place but inert for this experiment; a later experiment deletes it
   once the canonical path renders (no dual stacks in the final tree).

6. Determine during implementation whether the content-shell base needs
   additional extensions/guest-view wiring (an `ExtensionsBrowserClient` /
   guest-view manager / `MimeHandlerViewAttachHelper`) for the OOPIF attach to
   function, and record exactly what is missing — that is the most likely
   Partial outcome and defines Experiment 5.

7. Format, build, regenerate patches.

#### Verification

1. Build; the new deps link. (Forbidden-deps check no longer applies to PDF;
   instead confirm the HTML/binary smokes still pass — no regression to
   non-PDF.)
2. Bitcoin PDF smoke. Record:
   - whether the PDF content frame is now created out-of-process and navigates
     to the stream (`host_is_pdf=true`, `has_pdf_renderer=true`);
   - whether `Check failed: IsPdfRenderer()` is gone and the plugin
     instantiates;
   - whether the PDF visibly renders (the goal) or the exact next failure;
   - which stack drove it (`PdfViewerStreamManager` vs leftover
     `TsPdfStreamStore` logs).
3. Screenshot: classify (PDF rendered / advanced / new crash).
4. HTML and non-PDF binary smoke: no regression.

#### Pass Criteria

- Builds and links with the canonical deps.
- The PDF content frame is out-of-process and PDF-designated; the
  `IsPdfRenderer()` crash is gone.
- The PDF visibly renders, **or** the viewer advances to a precisely named next
  layer (e.g. the `injected_script` / plugin-wrapper layer Codex flagged).
- HTML and non-PDF binary smoke show no regression.

Stretch Pass: the PDF visibly renders and the first page scrolls.

#### Partial Criteria

Partial if the canonical stack is wired and builds, but the OOPIF attach needs
additional content-shell extensions/guest-view wiring that is not yet present
(e.g. `PdfViewerStreamManager` has no `ExtensionsBrowserClient`/guest-view
manager to attach through). Name the exact missing wiring; it defines
Experiment 5.

#### Failure Criteria

- The new deps fail to build/link in the content-shell configuration with no
  tractable fix.
- Normal HTML or non-PDF binary behavior regresses.
- The canonical stack cannot function without effectively rebuilding the full
  Chrome browser (i.e. the adoption is not viable) — would force reconsidering
  the custom-attach path.

#### Result

**Result:** Partial (scoped to a cheap probe)

Before pulling the full `//chrome/browser/pdf` footprint, I tested the cheapest
hypothesis: have `TsRendererClient::IsPluginHandledExternally()` return true for
the internal PDF plugin mime (Electron's PDF branch), which — if it worked —
would make Blink create the OOPIF content frame using TermSurf's _existing_
throttle, with no new deps. It does **not** work, for a precise reason that
sharpens the next experiment.

Chromium branch `148.0.7778.97-issue-790-exp4` (from `-exp3`). Change: the
return-true + diagnostic in `ts_renderer_client.cc IsPluginHandledExternally`
(no new deps).

- **`IsPluginHandledExternally` is never invoked for the PDF plugin mime.** The
  `[issue-790-exp4] is-plugin-handled-externally` log never fired. Blink only
  calls `IsPluginHandledExternally` when the object's content type is
  `kExternalPlugin` (`html_plugin_element.cc`); the internal PDF plugin
  (`application/x-google-chrome-pdf`, `PLUGIN_TYPE_BROWSER_INTERNAL_PLUGIN`) is
  treated as an **in-process** plugin, so Blink goes straight to
  `OverrideCreatePlugin` and instantiates it in the extension viewer frame.
- **Crash unchanged.** `Check failed: IsPdfRenderer()` still fires;
  `plugin-context` still shows in-frame creation; `host_is_pdf=false`.
- **No regression.** HTML/binary smokes unaffected (the change only adds a
  return path that is never reached without the external object type).

#### Conclusion

The minimal renderer-only routing is conclusively insufficient: the external
object type that triggers OOPIF frame creation only exists when the plugin is
registered/handled through the guest-view (MimeHandlerView) machinery — which is
exactly the canonical stack the decision above approved adopting. This cheaply
rules out the no-deps path and confirms Experiment 5 must do the real
integration.

Next layer (Experiment 5): the full canonical OOPIF PDF adoption — add
`//chrome/browser/pdf` + the guest-view renderer/browser deps, register the PDF
internal plugin so its object type is external (guest-view handled), drive
`PdfViewerStreamManager` with a content-shell-safe `PdfStreamDelegate`, swap out
the custom throttle/interceptor, and rewire the `chrome://resources`/Mojo gates
onto canonical PDF-frame identity (all per the Experiment 4 design's Changes,
now executed as Experiment 5). Expect this to be large and possibly itself
iterative; the most likely first outcome is identifying the content-shell
extensions/guest-view wiring `PdfViewerStreamManager` still needs.

### Experiment 5: Link the canonical PDF stack; map the assembly and gating gap

#### Description

Execute the first step of the canonical adoption: pull `//chrome/browser/pdf` +
the guest-view renderer/browser deps into `libtermsurf_chromium`, confirm it
links and does not regress, and map exactly how Chrome assembles the OOPIF PDF
flow so the remaining wiring is concrete.

#### Result

**Result:** Partial — feasibility confirmed, gating prerequisite identified

Chromium branch `148.0.7778.97-issue-790-exp5` (from `-exp4`).

- **Deps link cleanly (the key de-risking).** Adding `//chrome/browser/pdf`,
  `//extensions/renderer`, `//components/guest_view/{browser,renderer}`,
  `//components/guest_view/common:mojom` compiled (141 targets) and linked into
  the content*shell-based `libtermsurf_chromium` with a single addition:
  `AVFoundation.framework` (the only undefined symbol was
  `OBJC_CLASS*$\_AVCaptureDevice`, from the new deps' media-capture code). The feared `Profile`/`BrowserProcess`
  symbol explosion did **not** occur.
- **No regression.** With the deps linked but unwired: `index.html` renders (0
  FATAL); `bitcoin.pdf` still shows the pre-existing `IsPdfRenderer` crash
  (unchanged — nothing wired yet); no new startup crash from the
  extensions/guest-view static initializers.
- **Canonical assembly mapped.** The OOPIF PDF flow is:
  `PluginResponseInterceptorURLLoaderThrottle` (intercepts the PDF response;
  `plugin_response_interceptor_url_loader_throttle.cc:136` keys on the PDF
  extension id) → creates the stream → `PdfViewerStreamManager::Create` /
  `AddStreamContainer` (per-WebContents; the canonical counterpart of TermSurf's
  `TsPdfStreamStore`) → drives the OOPIF viewer + attach → renderer guest-view
  `MimeHandlerViewContainerManager` external plugin handling.
- **Gating prerequisite identified.** That whole flow presupposes the
  **extensions browser system is running**: an `ExtensionsBrowserClient` and
  `ExtensionRegistry` with the PDF component extension registered as the
  `application/pdf` mime handler. That registration is what makes the embed's
  object type "external" (the missing condition Exp 4 proved) and what lets
  `PdfViewerStreamManager` attach the OOPIF. content_shell does not run this
  system. Standing it up (à la Electron's `shell/browser/extensions/*`:
  `ElectronExtensionSystem`, `ElectronExtensionsBrowserClient`, component
  extension registration) is the substantial remaining prerequisite.

#### Conclusion

Experiment 5 turned the open feasibility question into a concrete, bounded
engineering plan: the canonical PDF stack links and runs in TermSurf's binary,
and the path to rendering is now "stand up the extensions browser
infrastructure, then wire the response interceptor + `PdfViewerStreamManager` +
renderer guest-view through it." This is a real port (Electron spreads it across
a lot of code), so it will span multiple experiments.

Next layer (Experiment 6): stand up the minimal extensions browser
infrastructure in TermSurf's content_shell base — an `ExtensionsBrowserClient`
implementation, an `ExtensionRegistry`, and registration of the PDF viewer as a
component extension that handles `application/pdf` — mirroring Electron's
`ElectronExtensionSystem` / `ElectronExtensionsBrowserClient`, scoped to what
the PDF OOPIF flow needs. Then wire the response interceptor +
`PdfViewerStreamManager` on top.

### Experiment 6: Stand up the extensions browser infrastructure

#### Description

Stand up the minimal extensions browser system in TermSurf's content_shell base
so the PDF viewer registers as the `application/pdf` mime handler and the
canonical OOPIF flow can engage. Research mapped the exact minimal port from
Electron's `shell/browser/extensions/`.

#### Scope (honest assessment from research)

This is the largest single experiment in the project — a real port, not a hook.
The minimal pieces (~1,500–2,000 LOC of new scaffolding across ~12 files,
adapted from Electron):

- `TsExtensionsBrowserClient` (subclass `extensions::ExtensionsBrowserClient`) —
  `Init`, `GetExtensionSystemFactory`, `GetComponentExtensionResourceManager`
  (TermSurf already has `TsPdfComponentExtensionResourceManager`),
  `GetProcessManagerDelegate`, `CreateExtensionHostDelegate`,
  `GetPrefServiceForContext`, `AllowCrossRendererResourceLoad`, etc.
- `TsExtensionSystem` + `TsExtensionSystemFactory` (subclass
  `extensions::ExtensionSystem`) — `InitForRegularProfile`,
  `LoadComponentExtensions` (loads the PDF viewer via
  `pdf_extension_util::GetManifest()` → `Extension::Create(kComponent)` →
  `registrar()->AddExtension`).
- `TsExtensionLoader`, `TsProcessManagerDelegate`, `TsExtensionHostDelegate`,
  `TsExtensionsAPIClient`, an extension `WebContentsObserver`, and the
  `EnsureBrowserContextKeyedServiceFactoriesBuilt` registration.

Init sequence (from research):

1. In `TsBrowserMainParts` (PreMainMessageLoopRun): set `ExtensionsClient`, set
   `TsExtensionsBrowserClient` + `Init()`, build the keyed-service factories.
2. Per `ShellBrowserContext`: `CreateBrowserContextServices`, then
   `TsExtensionSystem::InitForRegularProfile(true)` (loads the PDF component
   extension), then `FinishInitialization()`.

Required keyed services: `ExtensionRegistry`, `ExtensionSystem`, `EventRouter`,
`ProcessManager`, `ServiceWorkerManager`. PDF manifest declares
`"mime_types": ["application/pdf"]` + `"mime_types_handler": "index.html"`,
which `MimeTypesHandlerParser` ties to the extension — making the `<embed>`
externally handled (the condition Exp 4 found missing).

Because this is large, it will likely be split across more than one experiment
(stand up the system; then wire `PdfViewerStreamManager` + the response
interceptor; then debug content-shell gaps). It may not fit within the remaining
experiment budget — see the issue's running note.

#### Changes / Verification / Criteria

To be detailed as implementation proceeds (the port is being built
incrementally, each build verified for link + no-regression, mirroring the Issue
789 cadence). Pass for this experiment: the extensions system initializes, the
PDF component extension is registered in the `ExtensionRegistry` (logged), and
`application/pdf` is recognized as externally handled — with no regression to
HTML/non-PDF behavior. The PDF need not render yet; that follows once
`PdfViewerStreamManager` is wired on top.
