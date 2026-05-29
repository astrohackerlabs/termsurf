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
