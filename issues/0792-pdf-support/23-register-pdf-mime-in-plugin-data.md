# Experiment 23: Register PDF MIME in Plugin Data

## Description

Experiment 22 proved that Blink's body-loader and parser path is healthy. The
problem is that Blink receives Chromium's 76-byte unsupported-plugin fallback
instead of TermSurf's 536-byte PDF wrapper:

```text
[issue-792-exp19] wrapper-payload ... bytes=536 has_template=1 has_iframe=1 has_about_blank=1
[issue-792-exp22] body-loader-start ... is_static_data=1 ...
[issue-792-exp22] body-data-received ... encoded_size=76 ... sample=<html><body><!-- no enabled plugin supports this MIME type --></
```

Claude traced that exact fallback string to
`third_party/blink/renderer/core/loader/frame_loader.cc::FillStaticResponseIfNeeded(...)`.
That function synthesizes the fallback when the response MIME is not supported
by Blink and renderer-side `PluginData::SupportsMimeType(mime_type)` returns
false.

TermSurf's `TsContentClient::AddPlugins(...)` currently registers the internal
PDF plugin only for `pdf::kInternalPluginMimeType`
(`application/x-google-chrome-pdf`). The top-level PDF navigation is checked as
`pdf::kPDFMimeType` (`application/pdf`) before the wrapper body can reach the
parser. Experiment 23 registers the PDF plugin for both MIME types and verifies
that the renderer no longer replaces the wrapper body with static fallback HTML.

This is the renderer-side counterpart of Experiment 17's browser-side
`intercepted_by_plugin` fix: the browser must not download the PDF, and the
renderer must not synthesize the unsupported-plugin fallback.

This experiment must receive Claude design review before implementation. After
implementation and result recording, Claude must review the completed output
before any next experiment is designed.

## Changes

1. Create the Chromium implementation branch.

   Start from the accepted Experiment 22 branch:

   ```bash
   git -C chromium/src checkout 148.0.7778.97-issue-792-exp22
   git -C chromium/src checkout -b 148.0.7778.97-issue-792-exp23
   ```

   Add the branch to `chromium/README.md` only after the branch builds and the
   result is accepted.

2. Register both PDF MIME types in TermSurf plugin data.

   In `content/libtermsurf_chromium/ts_content_client.cc`, update
   `TsContentClient::AddPlugins(...)` so the internal PDF plugin advertises both
   MIME types:

   ```text
   application/x-google-chrome-pdf
   application/pdf
   ```

   Use the same extension and description for both entries:

   ```text
   extension=pdf
   description=Portable Document Format
   ```

   Keep the plugin type as:

   ```text
   content::WebPluginInfo::PLUGIN_TYPE_BROWSER_INTERNAL_PLUGIN
   ```

   Preserve the existing internal MIME entry. Do not replace it with
   `application/pdf`; the extension viewer still uses
   `application/x-google-chrome-pdf`.

3. Add narrow diagnostic logging around static fallback.

   In `third_party/blink/renderer/core/loader/frame_loader.cc`, add temporary
   `[issue-792-exp23]` logs inside `FillStaticResponseIfNeeded(...)` for the
   local bitcoin PDF navigation only.

   Required logs:

   ```text
   [issue-792-exp23] static-response-check url=<url> mime_type=<mime> supported_mime=<0|1> has_plugin_data=<0|1> plugin_supports_mime=<0|1> action=<return|fallback>
   ```

   This log must prove whether `PluginData::SupportsMimeType("application/pdf")`
   becomes true after the plugin registration change.

4. Preserve Experiment 19-22 diagnostics.

   The verification needs the existing chain:

   ```text
   wrapper-payload bytes=536 has_iframe=1
   document-commit ...
   body-loader-start ...
   body-data-received ...
   parser-append-string ...
   declarative-shadow-root / first later missing gate
   ```

5. Do not change the PDF wrapper or stream manager.

   This experiment is specifically about renderer-side plugin MIME visibility.
   Do not change:
   - `TsPluginResponseInterceptorURLLoaderThrottle`;
   - `PdfViewerStreamManager`;
   - `MimeHandlerServiceImpl`;
   - data pipe ownership or completion ordering;
   - wrapper HTML generation;
   - extension resources;
   - parser scheduling.

6. Build and archive only after the result is accepted.

   Build with:

   ```bash
   cd chromium/src
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   autoninja -C out/Default libtermsurf_chromium
   ```

   If the experiment passes or produces a coherent diagnostic branch, commit the
   Chromium branch and regenerate:

   ```bash
   rm -rf ../../chromium/patches/issue-792/
   git format-patch 148.0.7778.97..HEAD -o ../../chromium/patches/issue-792/
   ```

## Verification

1. Build `libtermsurf_chromium` with `autoninja`.

2. Run the fake-GUI PDF smoke test against the local bitcoin PDF fixture:

   ```bash
   LOG_DIR="logs/issue-792-exp23-pdf-$(date +%Y%m%d-%H%M%S)"
   scripts/test-issue-792-fake-gui.py \
     http://127.0.0.1:9787/bitcoin.pdf \
     --serve-bitcoin-pdf \
     --log-dir "$LOG_DIR" \
     --seconds 18
   ```

3. Inspect `roamium.stderr`.

   Required success chain:

   ```text
   [issue-792-exp23] static-response-check ... mime_type=application/pdf ... plugin_supports_mime=1 action=return
   [issue-792-exp21] document-type-selected mime_type=application/pdf result=html is_for_external_handler=1
   [issue-792-exp22] body-loader-start ... is_static_data=0 ...
   [issue-792-exp22] body-data-received ... encoded_size=536 ... has_template=1 has_iframe=1 has_shadowrootmode=1 has_internal_id=1 ...
   [issue-792-exp22] parser-append-string ... has_template=1 has_iframe=1 has_shadowrootmode=1 ...
   [issue-792-exp21] declarative-shadow-root ...
   ```

   If the wrapper body reaches the parser but a later gate still fails, classify
   the first later missing transition:
   - no `declarative-shadow-root`: tokenizer/tree-builder issue;
   - shadow root attaches but no `frame-owner-inserted`: iframe in shadow root
     is not becoming a live frame owner;
   - frame owner inserts but no `load-or-redirect-subframe`: iframe insertion is
     not triggering subframe load;
   - `load-or-redirect-subframe result=1` but no child `pvs-finish`:
     browser-side child navigation is lost;
   - child `pvs-finish` appears but no extension viewer startup or stream-info:
     resume the extension-viewer diagnostics from Experiments 18-19.

4. Explicit failure criteria:
   - If `static-response-check ... plugin_supports_mime=0 action=fallback`
     remains, the new MIME was not visible to renderer-side plugin data.
   - If `body-data-received` still shows `encoded_size=76` or the unsupported
     MIME fallback sample, the static fallback is not fixed.
   - If `document-type-selected ... result=plugin` fires for the PDF navigation,
     the `application/pdf` MIME entry was not marked as an external handler;
     check that `GetPluginMimeTypesWithExternalHandlers(...)` still returns
     `application/pdf`.
   - If `application/pdf` reaches `OverrideCreatePlugin(...)` and creates a
     direct `PluginDocument` instead of the wrapper path, this fix is at the
     wrong layer; record the trace and redesign rather than adding more MIME
     aliases.

5. Run the normal HTML smoke test:

   ```bash
   LOG_DIR="logs/issue-792-exp23-html-$(date +%Y%m%d-%H%M%S)"
   scripts/test-issue-792-fake-gui.py \
     http://localhost:9616/index.html \
     --log-dir "$LOG_DIR" \
     --seconds 8
   ```

   The HTML control must not emit `[issue-792-exp23]` fallback logs and must not
   emit `[issue-792-exp22]` body/parser logs.

6. Record the result in this file.

   The result must include:
   - the exact PDF and HTML log directories;
   - whether renderer-side plugin data supports `application/pdf`;
   - whether `is_static_data` changes from `1` to `0`;
   - whether body bytes change from the 76-byte fallback to the 536-byte
     wrapper;
   - whether declarative shadow root and iframe creation advance;
   - the first remaining missing transition, if any;
   - the concrete next experiment implied by that transition.

## Result

Not run yet.

## Conclusion

Pending implementation.
