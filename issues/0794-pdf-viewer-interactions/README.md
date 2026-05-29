+++
status = "open"
opened = "2026-05-29"
+++

# Issue 794: Complete PDF Viewer Interactions

## Goal

Make TermSurf's Chromium PDF support behave like a usable in-pane PDF viewer,
not just a first-page renderer. A PDF must scroll, respond to resize, accept
clicks, allow text selection and copy, and expose the normal PDF viewer controls
that Chromium/Electron users expect.

## Background

Issues 792 and 793 proved the hard loading path:

- Issue 792 made a full-page PDF render inside Roamium by bringing in the
  Electron-style PDF component-extension, stream, and MimeHandler plumbing.
- Issue 793 fixed the tiny PDF iframe by preserving the PDF viewer CSS as a web
  accessible resource.

Manual testing after those fixes shows the current state is still not a complete
PDF viewer:

- The first page renders.
- The first page initially fits the visible webview height.
- Wheel scrolling does not move through the PDF.
- Resizing the webview does not resize/reflow the rendered pages.
- Click and drag text selection does not work.

That strongly suggests the remaining work is not "load one more PDF resource."
The viewer is present, but the interaction, viewport, and browser-side feature
surfaces are incomplete.

## Electron and Chromium Feature Inventory

Electron does not get complete PDF support by only calling
`pdf::CreateInternalPlugin()`. Its implementation mirrors the pieces of Chrome's
PDF viewer that an embedder must provide:

- `ElectronBrowserClient::CreateThrottlesForNavigation()` installs
  `pdf::PdfNavigationThrottle`.
- `ElectronBrowserClient::WillCreateURLLoaderRequestInterceptors()` installs
  `pdf::PdfURLLoaderRequestInterceptor`.
- `ElectronBrowserClient::CreateURLLoaderThrottles()` installs
  `PluginResponseInterceptorURLLoaderThrottle`.
- `ElectronBrowserClient::RegisterAssociatedInterfaceBindersForRenderFrameHost()`
  binds `pdf::mojom::PdfHost` through `pdf::PDFDocumentHelper`.
- Electron registers the PDF extension as a component extension and serves PDF
  resources plus template/i18n replacements through
  `ElectronComponentExtensionResourceManager`.
- Electron implements `pdfViewerPrivate` extension functions including
  `getStreamInfo`, `setPdfPluginAttributes`, `setPdfDocumentTitle`,
  `isAllowedLocalFileAccess`, and save-to-drive stubs or flows.
- Electron's PDF document helper propagates `SetPluginCanSave()` into
  `PdfViewerStreamManager` and emits a print-ready signal when content
  restrictions indicate printing is available.
- Electron's renderer client handles the internal PDF plugin and external plugin
  handling paths that create the PDF extension frame/container.

Chromium's PDF tests show the feature surface that a complete viewer needs:

- Full-page PDF load with the expected frame tree.
- Embedded PDF load with the expected frame tree.
- Focus into the full-page PDF.
- Mouse click and drag selection inside the PDF plugin.
- Selected text propagation/copy.
- Wheel scrolling and keyboard navigation.
- Resize and viewport updates.
- Toolbar controls for page navigation, zoom, fit modes, rotation, save, and
  print.
- Correct document title for titled and untitled PDFs.
- Local `file://` PDFs and extensionless local PDFs.
- Link navigation and PDF permissions.
- Print, save/download, and content restriction behavior.
- Form focus, context menus, accessibility/searchify, and XFA where enabled.

## Current TermSurf Gaps

The current TermSurf PDF code has enough infrastructure to render, but several
Electron/Chromium behavior paths are still stubbed, unverified, or absent.

### Input, Focus, and Hit Testing

The current manual result proves rendering, not interaction. We do not yet know
whether wheel, click, drag, or keyboard events are reaching the PDF extension
frame and internal plugin with the focus and hit-test state Chromium expects.

Chromium's own PDF tests explicitly click into the plugin frame and wait for hit
test data before asserting focus or text selection. TermSurf currently needs a
dedicated experiment that proves where input stops:

- Wezboard synthetic mouse/wheel forwarding.
- Roamium protocol dispatch.
- Chromium `WebContents` input routing.
- PDF extension frame focus.
- Internal PDF plugin event handling.

Do not assume this is a Wezboard bug or a Chromium bug until that ladder is
measured.

### Scroll and Resize

The user-visible symptom is that the first page fits the initial webview height,
but the PDF does not scroll and does not resize when the pane changes size.

The most likely missing surfaces are:

- Viewer viewport update plumbing, including the
  `pdfViewerPrivate.onShouldUpdateViewport` path used by the PDF viewer.
- Browser-side frame/stream manager state that tells the viewer and plugin when
  dimensions change.
- Input/focus routing for wheel events if the viewer never receives scroll
  input.

These must be separated in the first experiment. A resize failure and a wheel
failure can share a symptom but have different causes.

### Text Selection and Copy

Text selection needs more than the page bitmap:

- The PDF plugin must receive mouse down/move/up events.
- The PDF plugin frame must be focusable.
- Selected text must propagate through Chromium's selection APIs.
- Copy must reach the selected PDF text, not the surrounding extension document
  or terminal pane.

Chromium's PDF tests cover selected text replies and direct `GetSelectedText()`
checks. TermSurf needs equivalent verification.

### Save, Print, and Toolbar State

TermSurf's `TsPDFDocumentHelperClient` currently logs callbacks such as
`UpdateContentRestrictions()`, `OnSaveURL()`, and `SetPluginCanSave()`. Electron
turns these into behavior:

- `SetPluginCanSave()` updates `PdfViewerStreamManager`.
- Content restriction changes produce a print-ready signal.
- Save/download and print actions route through browser-side helpers.

These may not block scrolling, but they are part of a complete PDF viewer and
must be tracked.

### Document Metadata and Local File Behavior

Electron implements `pdfViewerPrivate.setPdfDocumentTitle()` so the PDF viewer
can update the tab title. Chrome's tests also cover titled PDFs, untitled PDFs,
local `file://` PDFs, and extensionless local PDF files.

TermSurf should not treat HTTP-only PDF rendering as complete support.

## Automation Strategy

The remaining PDF work should be tested automatically wherever possible. Manual
visual inspection is useful as a final sanity check, but the experiments should
produce objective pass/fail evidence for the interaction surface.

Use two levels of automation:

- **Chromium-level automation:** Drive Roamium through the Chrome DevTools
  Protocol. Use `Input.dispatchMouseEvent` for clicks, drags, and wheel events;
  use `Input.dispatchKeyEvent` for keyboard navigation; use screenshots and
  JavaScript evaluation to inspect viewer state, scroll position, page number,
  selected text, toolbar state, and resize behavior. This is the most precise
  way to prove whether the Chromium/PDF internals work.
- **Real app-path automation:** Drive the actual Wezboard window with macOS
  input events, then inspect screenshots and logs. This tests the complete
  TermSurf path: Wezboard input forwarding, protocol messages, Roamium dispatch,
  Chromium input routing, PDF extension focus, and the internal PDF plugin.

The first diagnostic harness should cover:

- **Render:** The PDF screenshot contains visible PDF text/pages.
- **Scroll:** Wheel input changes PDF viewer scroll state or visibly changes the
  screenshot.
- **Resize/reflow:** Resizing the webview changes PDF viewer viewport and page
  dimensions.
- **Click/focus:** Clicking inside the PDF focuses the PDF/plugin frame or
  produces the expected focus logs/state.
- **Drag selection:** Dragging across known text produces selected text through
  Chromium selection APIs or through the clipboard after copy.
- **Keyboard navigation:** PageDown, Arrow keys, and Space change scroll
  position or page number.
- **Toolbar controls:** Zoom, page navigation, fit mode, and rotate controls
  change viewer state.
- **Save/download:** Save/download actions fire the expected browser-side path
  or produce an output file in a controlled test directory.
- **Print:** Print actions reach the print pathway. The automated test should
  stop before invoking a real printer.
- **Title:** The tab/title state reflects the PDF title or the expected URL
  fallback for untitled PDFs.
- **Local file:** `file://` PDFs behave the same as the local HTTP fixture.
- **Normal web regression:** A non-PDF page still scrolls, clicks, resizes, and
  accepts text selection normally.

Some checks will be inherently easier to automate than others. Native print
dialogs, OS save panels, and subjective visual quality can remain screenshot or
log triage points. The core question, "does PDF support work as an interactive
viewer," should be automated with state checks, screenshots, and targeted logs.

## Proposed Direction

This issue should not be solved with one giant patch. The work should proceed as
experiments, each reviewed by Claude before implementation and again after
completion:

1. First, build an interaction diagnostic harness for the current PDF viewer. It
   must distinguish rendering, wheel scroll, resize/reflow, click focus,
   drag-select text, selected-text copy, toolbar save/print state, and title
   updates.
2. Then fix the first measured broken layer. If input never reaches Chromium,
   fix the TermSurf input path. If input reaches Chromium but not the PDF
   plugin, fix the PDF frame/focus/hit-test layer. If resize reaches the webview
   but not the PDF viewer, fix the viewport/update path.
3. Continue one layer at a time until the PDF viewer is usable, not merely
   visible.

Electron remains the guide for embedder-owned browser infrastructure. Chromium's
PDF tests remain the feature checklist.

## Constraints

- Do not reopen or modify closed Issues 792 and 793.
- Preserve the existing PDF loading and iframe sizing fixes.
- Do not add more PDF protocol surface unless an experiment proves the current
  browser input/resize messages are insufficient.
- Do not make Wezboard-specific input changes until Chromium-side PDF routing
  has been measured.
- If an experiment modifies Chromium, create a fresh Chromium branch for this
  issue and add it to `chromium/README.md`.
- Every experiment design and every completed experiment result must be reviewed
  by Claude. Fix real issues from the review before proceeding.
