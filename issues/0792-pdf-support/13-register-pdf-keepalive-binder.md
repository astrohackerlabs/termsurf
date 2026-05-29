# Experiment 13: Register PDF KeepAlive Binder

## Description

Experiment 12 registered the PDF viewer's MIME-handler binders and crossed the
missing `extensions.mime_handler.MimeHandlerService` gate. The direct PDF
extension smoke then stopped at the next frame-scoped Mojo binder:

```text
No binder found for interface extensions.KeepAlive for the frame/document scope
```

This binder is not PDF-specific. The renderer module
`extensions/renderer/resources/keep_alive.js` creates it for extension API
promise plumbing. The PDF viewer's `mimeHandlerPrivate` custom bindings create a
keepalive before awaiting the underlying `getStreamInfo()` promise, so
`KeepAlive` must bind before Experiment 12's diagnostic `GetStreamInfo()` can
run.

Chromium already has the real implementation:

```text
extensions/browser/mojo/keep_alive_impl.{cc,h}
```

`extensions::KeepAliveImpl` is an RAII Mojo service: construction increments the
extension's lazy keepalive count through `ProcessManager`, and pipe disconnect
decrements it. Chromium normally registers it from
`extensions::PopulateExtensionFrameBinders()`, which is called by
`TsExtensionsBrowserClient::RegisterBrowserInterfaceBindersForFrame()`.

Experiment 13 should first log whether TermSurf's extension-frame binder path
recognizes the PDF frame **and** whether it actually reaches
`PopulateExtensionFrameBinders()`. Then it should add the smallest PDF-extension
fallback needed to register the real Chromium extension-frame binders for PDF
extension frames. Per this issue's Electron-first principle, prefer the
Electron-style direct `PopulateExtensionFrameBinders(...)` call over registering
only `KeepAliveImpl` by hand, unless the broader canonical binder set proves
unusable. It must not add a fake keepalive unless the real implementation proves
unusable. It must not implement stream handoff, guest-view,
`PdfViewerStreamManager`, PDF navigation interception, or PDF renderer
process-model changes.

This experiment must receive Claude design review before implementation. After
implementation and result recording, Claude must review the completed output
before any next experiment is designed.

## Changes

1. Create the Chromium implementation branch.

   Start from the accepted Experiment 12 branch:

   ```bash
   git -C chromium/src checkout 148.0.7778.97-issue-792-exp12
   git -C chromium/src checkout -b 148.0.7778.97-issue-792-exp13
   ```

   Add the branch to `chromium/README.md` only after the branch builds and the
   result is accepted.

2. Instrument the extension-frame binder decision.

   In `TsBrowserClient::RegisterBrowserInterfaceBindersForFrame()`, log whether
   the existing extension binder path detects an extension for the PDF viewer
   frame before registering browser interfaces.

   Required log:

   ```text
   [issue-792-exp13] extension-frame-binder-check frame_url=<url> site_url=<url> observer=<0|1> extension_id=<id|none> is_pdf_extension=<0|1>
   ```

   Add a second diagnostic inside
   `TsExtensionsBrowserClient::RegisterBrowserInterfaceBindersForFrame()` around
   the actual `extensions::PopulateExtensionFrameBinders(...)` call.

   Required log:

   ```text
   [issue-792-exp13] extensions-frame-binder-invoked frame_url=<url> site_url=<url> extension_id=<id> populate_called=<0|1>
   ```

   These logs answer whether the existing path:

   ```text
   ExtensionWebContentsObserver::GetForWebContents(...)
     ->GetExtensionFromFrame(...)
     ->ExtensionsBrowserClient::RegisterBrowserInterfaceBindersForFrame(...)
     ->PopulateExtensionFrameBinders(...)
   ```

   is active for the PDF extension page. The first log answers whether the frame
   was recognized as an extension frame. The second log answers whether the
   canonical extension-frame binders were actually populated. Record both
   answers in the result.

3. Prefer Electron-style extension-frame binder population.

   If the existing extension binder path does not register
   `extensions.KeepAlive` for the PDF extension frame, prefer adding an
   Electron-style direct invocation of:

   ```text
   extensions::PopulateExtensionFrameBinders(map, render_frame_host, extension)
   ```

   from `TsBrowserClient::RegisterBrowserInterfaceBindersForFrame()`, gated to
   the PDF component extension. Electron calls this helper directly from its
   browser client when registering frame binders for extension frames. The
   helper registers `KeepAlive` plus the other standard extension-frame
   interfaces in one canonical call, instead of forcing a "one missing extension
   binder per experiment" treadmill.

   Use the PDF extension object from one of these sources, in order:
   - the existing `ExtensionWebContentsObserver::GetExtensionFromFrame(...)`
     result, if present;
   - the enabled PDF component extension from `ExtensionRegistry`, if the frame
     URL is the PDF extension but the observer path did not return an extension.

   Required log:

   ```text
   [issue-792-exp13] pdf-extension-frame-binders frame_url=<url> site_url=<url> extension_id=<id> source=<canonical|direct-populate>
   ```

   Use `source=canonical` when the existing
   `TsExtensionsBrowserClient::RegisterBrowserInterfaceBindersForFrame()` path
   ran. Use `source=direct-populate` when the direct Electron-style fallback ran
   from `TsBrowserClient`.

4. Keep explicit `KeepAliveImpl` as a last resort only.

   If direct `PopulateExtensionFrameBinders(...)` cannot be invoked cleanly
   because another binder in the broader canonical set causes a build-time or
   immediate startup failure, then fall back to registering only
   `extensions.KeepAlive` with Chromium's real implementation:

   ```text
   extensions::KeepAliveImpl::Create(...)
   ```

   Include, only if this last-resort path is used:

   ```text
   extensions/browser/mojo/keep_alive_impl.h
   extensions/common/mojom/keep_alive.mojom.h
   ```

   The explicit `KeepAliveImpl` fallback should:
   - apply only to the PDF component extension
     (`extension_misc::kPdfExtensionId`);
   - use the enabled PDF extension object from `ExtensionRegistry`;
   - use the frame's `BrowserContext`;
   - bind `extensions::KeepAlive` to `KeepAliveImpl::Create`;
   - log when it registers the fallback.

   Required log:

   ```text
   [issue-792-exp13] pdf-keepalive-binder frame_url=<url> site_url=<url> extension_id=<id> source=keepalive-only
   ```

   For this last-resort standalone `map->Add<extensions::KeepAlive>(...)` path,
   use the same bind-time gate shape as Experiment 10's help-bubble factory and
   Experiment 12's MIME-handler binders: the `map->Add` may be available on the
   frame binder map, but the bind function must re-check that the requesting
   `RenderFrameHost` belongs to the PDF component extension before invoking
   `KeepAliveImpl::Create`. Non-PDF frames must drop the receiver.

5. Avoid duplicate binder registration.

   `extensions::PopulateExtensionFrameBinders()` already adds `KeepAlive`. Do
   not add a second `extensions::KeepAlive` registration to the same
   `BinderMapWithContext`.

   The implementation should make one of these choices:
   - If the canonical extension path sees the PDF extension, let it register the
     extension-frame binders and only log `source=canonical`.
   - If the canonical extension path does not see the PDF extension, call
     `PopulateExtensionFrameBinders(...)` directly for the PDF extension frame
     and log `source=direct-populate`.
   - If direct population proves unusable, register only the explicit
     `KeepAliveImpl` fallback and log `source=keepalive-only`.

   Do not call both paths for the same PDF extension frame.

6. Keep Experiment 12's MIME-handler binders intact.

   Do not remove or weaken:
   - `BindTsMimeHandlerService`;
   - `BindTsBeforeUnloadControl`;
   - the PDF-extension gate in `ts_mime_handler_binders.cc`;
   - Experiment 10's `PdfHelpBubbleHandlerFactory`;
   - Experiment 10's associated `pdf::mojom::PdfHost`;
   - Experiment 11's extension renderer resource-pack loading.

7. Keep this slice diagnostic and binder-only.

   Forbidden in this experiment:
   - implementing stream handoff;
   - creating or storing real `StreamContainer` objects outside Chromium's real
     keepalive path;
   - wiring `MimeHandlerViewGuest`;
   - wiring guest-view attach helpers;
   - wiring `PdfViewerStreamManager`;
   - changing PDF navigation interception;
   - changing the direct PDF download path;
   - adding `--pdf-renderer` process-model logic;
   - changing PDF extension manifest permissions;
   - adding Chrome browser UI stacks.

8. Build and archive only after verification.

   Build:

   ```bash
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   git -C chromium/src cl format --upstream=148.0.7778.97-issue-792-exp12 --full
   autoninja -C chromium/src/out/Default libtermsurf_chromium
   ```

   If the branch builds and verification passes or produces a useful Partial, do
   the full bookkeeping after Claude after-review accepts the result:
   - commit the Chromium branch;
   - regenerate `chromium/patches/issue-792/`;
   - add the new branch row to `chromium/README.md`;
   - update Experiment 13's line in `issues/0792-pdf-support/README.md` from
     `Designed` to the final status.

## Verification

1. Confirm starting state.

   ```bash
   git status --short
   git -C chromium/src status --short
   git -C chromium/src branch --show-current
   ```

   Chromium should start clean on `148.0.7778.97-issue-792-exp12`.

2. Build the branch.

   ```bash
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   git -C chromium/src cl format --upstream=148.0.7778.97-issue-792-exp12 --full
   autoninja -C chromium/src/out/Default libtermsurf_chromium
   ```

3. Run the direct PDF extension smoke.

   Reuse the debug screenshot harness against:

   ```text
   chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/index.html
   ```

   Required evidence:
   - Experiment 9 activation remains intact:

     ```text
     [issue-792-exp9] renderer-activate-extension ... active=1
     [issue-792-exp9] pdf-script-context ... context=BLESSED_EXTENSION ... pdfViewerPrivate_available=1
     [issue-792-exp8] schema-request name=pdfViewerPrivate found=1
     ```

   - Experiment 10 help-bubble binder remains intact:

     ```text
     [issue-792-exp10] pdf-help-bubble-binder ...
     [issue-792-exp10] pdf-help-bubble-create-handler ...
     ```

   - Experiment 11 resource-pack load remains intact:

     ```text
     [issue-792-exp11] extensions-renderer-pak ... loaded=1 ...
     ```

   - Experiment 12 MIME-handler binders remain intact:

     ```text
     [issue-792-exp12] mime-handler-service-binder ...
     [issue-792-exp12] before-unload-control-binder ...
     ```

   - Experiment 13 binder evidence appears:

     ```text
     [issue-792-exp13] extension-frame-binder-check ...
     [issue-792-exp13] extensions-frame-binder-invoked ...  # if canonical path ran
     [issue-792-exp13] pdf-extension-frame-binders ...
     [issue-792-exp13] pdf-keepalive-binder ...             # only if keepalive-only fallback was needed
     ```

   - The previous bad-Mojo binder failure is gone:

     ```text
     No binder found for interface extensions.KeepAlive
     ```

   Record the next observed error exactly. The expected useful next gate is
   Experiment 12's diagnostic null-stream path:

   ```text
   [issue-792-exp12] mime-handler-get-stream-info ... stream_info=null
   ```

   If `GetStreamInfo()` runs and the viewer reports "Stream has been aborted,"
   the experiment passes and the next experiment should target real PDF stream
   handoff. If a different missing binder appears first, record that as a
   Partial and make it the next experiment's target.

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

   Direct PDF navigation is still expected to follow the content_shell download
   path. A browser crash, renderer IPC crash, or hang is a failure.

## Pass Criteria

The experiment passes if:

- `libtermsurf_chromium` builds;
- the direct PDF extension smoke no longer dies at missing
  `extensions.KeepAlive`;
- the result records whether the canonical extension-frame binder path saw the
  PDF extension frame;
- the result records whether the extension binders used the canonical path,
  direct `PopulateExtensionFrameBinders(...)`, or the keepalive-only fallback;
- Experiment 12's `mime-handler-get-stream-info` log appears with
  `stream_info=null`;
- the result records the renderer's null-stream behavior or next gate;
- Experiment 9, 10, 11, and 12 evidence remains intact;
- HTML and unchanged PDF regression smokes do not crash or hang before artifact
  capture.

## Partial Criteria

The experiment is Partial if it builds and registers or diagnoses some part of
the `KeepAlive` path but does not reach `GetStreamInfo()`. Examples:

- the canonical extension-frame binder path sees the PDF extension but the
  binder still does not register, indicating a deeper
  `PopulateExtensionFrameBinders` problem;
- direct `PopulateExtensionFrameBinders(...)` cannot be used cleanly, and the
  result proves why;
- the keepalive-only fallback registers but `KeepAliveImpl` cannot be used with
  TermSurf's current `ProcessManager`/extension state;
- `KeepAlive` binds, but another missing binder kills the renderer before
  `GetStreamInfo()`;
- a non-PDF regression appears that is clearly attributable to the keepalive
  binding path.

Every Partial result must record the exact blocker and the next experiment's
target.

## Failure Criteria

The experiment fails if:

- it implements fake keepalive while Chromium's real `KeepAliveImpl` is usable;
- it registers duplicate `extensions.KeepAlive` binders for the same frame;
- it implements real PDF stream handoff;
- it creates or stores real PDF stream containers;
- it wires `MimeHandlerViewGuest`, guest-view attach helpers, or
  `PdfViewerStreamManager`;
- it changes PDF navigation interception or the direct PDF download path;
- it adds `--pdf-renderer` process-model logic;
- it changes PDF extension manifest permissions;
- it removes or weakens Experiment 9 activation;
- it removes or weakens Experiment 10 PDF viewer binders;
- it removes or weakens Experiment 11 renderer resource-pack loading;
- it removes or weakens Experiment 12 MIME-handler binders;
- ordinary HTML pages crash, hang, or lose normal lifecycle messages;
- direct PDF navigation regresses into a crash, hang, or renderer IPC failure;
- the build cannot complete.
