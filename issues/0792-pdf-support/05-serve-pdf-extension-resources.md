# Experiment 5: Serve PDF Extension Resources

## Description

Experiment 4 proved the PDF viewer resource bytes exist in `ui::ResourceBundle`.
The next missing layer is serving those bytes through the normal extension
URL-loader path.

Today TermSurf has a registered PDF component extension, but
`TsExtensionsBrowserClient::GetBundleResourcePath()` always returns empty,
`TsExtensionsBrowserClient::LoadResourceFromResourceBundle()` is `NOTREACHED()`,
and `TsBrowserClient` does not install a `chrome-extension://` non-network URL
loader factory. That means a navigation to:

```text
chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/index.html
```

cannot prove the viewer shell is actually loadable.

This experiment wires the narrow, Electron-shaped resource-serving layer:

- map PDF component-extension URLs to GRIT resource ids;
- serve those ids from `ui::ResourceBundle`;
- install the extension URL-loader factories needed for main-resource and
  subresource loads;
- verify the viewer HTML is fetched through Chromium's extension protocol.

This experiment still does **not** wire PDF navigation, `PdfNavigationThrottle`,
`PdfViewerStreamManager`, guest-view, MimeHandlerView, PDF stream APIs, or
`--pdf-renderer`. It is only about making the already-registered component
extension's static resources loadable.

This experiment must receive Claude design review before implementation. After
implementation and result recording, Claude must review the completed output
before any next experiment is designed.

## Changes

1. Create the Chromium implementation branch.

   Start from the accepted Experiment 4 branch:

   ```bash
   git -C chromium/src checkout 148.0.7778.97-issue-792-exp4
   git -C chromium/src checkout -b 148.0.7778.97-issue-792-exp5
   ```

   Add the branch to `chromium/README.md` only after the branch builds and the
   result is accepted.

2. Install extension scheme URL-loader factories in `TsBrowserClient`.

   Add the same minimal factory hooks Electron and app_shell use:
   - `CreateNonNetworkNavigationURLLoaderFactory()`
   - `RegisterNonNetworkWorkerMainResourceURLLoaderFactories()`
   - `RegisterNonNetworkServiceWorkerUpdateURLLoaderFactories()`
   - `RegisterNonNetworkSubresourceURLLoaderFactories()`

   For `extensions::kExtensionScheme`, return the corresponding factory from
   `extensions/browser/extension_protocols.h`:
   - `CreateExtensionNavigationURLLoaderFactory(...)`
   - `CreateExtensionWorkerMainResourceURLLoaderFactory(...)`
   - `CreateExtensionServiceWorkerScriptURLLoaderFactory(...)`
   - `CreateExtensionURLLoaderFactory(...)`

   Match Electron's behavior for this slice: no `WebViewGuest` support, so pass
   `false` for the navigation factory's `is_web_view_request` argument.

   Do not add `ExtensionNavigationThrottle`, `WebViewGuest`, guest-view,
   `WillCreateURLLoaderFactory()` proxying, or WebRequest plumbing in this
   experiment. If direct `chrome-extension://.../index.html` navigation cannot
   work without those pieces, record Partial and design the next experiment
   around the missing hook.

3. Implement `TsExtensionsBrowserClient::GetBundleResourcePath()`.

   For requests whose URL scheme is `chrome-extension` and whose host is
   `extension_misc::kPdfExtensionId`, use a hard early-return guard:

   ```cpp
   if (!request.url.SchemeIs(extensions::kExtensionScheme) ||
       request.url.host() != extension_misc::kPdfExtensionId) {
     *resource_id = 0;
     return {};
   }
   ```

   Then:
   - convert the URL to a relative file path with
     `extensions::file_util::ExtensionURLToRelativeFilePath(request.url)`;
   - ask `TsComponentExtensionResourceManager` whether that relative path maps
     to a component resource id under the synthetic PDF GRIT prefix;
   - if found, return the relative path and set `resource_id`;
   - otherwise return an empty path and leave `resource_id = 0`.

   Do not require the registered extension root to be under `DIR_ASSETS`.
   TermSurf's Experiment 3 component extension root is a synthetic profile path,
   while the bytes live in `pdf_resources.pak`. Chrome/Electron use a resources
   parent-path check because their component extension roots are laid out under
   the Chrome resources directory. TermSurf's source of truth for this slice is
   the PDF resource map loaded by `TsComponentExtensionResourceManager`.

   The generated `kPdfResources` keys are prefixed with `pdf/`, for example
   `pdf/index.html`, while a request for
   `chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/index.html` produces the
   relative path `index.html`. For PDF-extension requests, call the component
   resource manager with a synthetic extension path:

   ```cpp
   base::FilePath(FILE_PATH_LITERAL("pdf"))
   ```

   and the request-relative path. This deliberately maps "the PDF component
   extension" to Chromium's `pdf/` GRIT resource prefix and preserves the
   Experiment 3 resource-manager self-test.

4. Implement a TermSurf-owned resource-bundle URL loader.

   Add a small helper under:

   ```text
   chromium/src/content/libtermsurf_chromium/extensions/
   ```

   Suggested name:

   ```text
   ts_extension_resource_loader.{h,cc}
   ```

   The helper should be a narrow adaptation of Chromium's
   `chrome/browser/extensions/chrome_url_request_util.cc`
   `ResourceBundleFileLoader`:
   - read/decode text resources with
     `ui::ResourceBundle::LoadDataResourceString(resource_id)`, then write the
     decoded string bytes to the response with no `Content-Encoding` header;
   - for non-text resources, use a compression-aware path:
     `LoadDataResourceBytes(resource_id)` only if the resource is not
     compressed, or an explicit ResourceBundle decompression helper if Chromium
     148 exposes one;
   - fail cleanly rather than sending compressed bytes with an uncompressed MIME
     type;
   - apply component-extension template replacements from
     `GetComponentExtensionResourceManager()->GetTemplateReplacementsForExtension()`
     for HTML and JavaScript resources;
   - infer MIME type from the relative file path with
     `net::GetWellKnownMimeTypeFromFile()`, falling back to
     `application/octet-stream`;
   - set response headers, content length, MIME type, and UTF-8 charset for text
     resources;
   - write bytes through a Mojo data pipe and complete with `net::OK`;
   - fail cleanly with `net::ERR_FAILED` if bytes cannot be read.

   Use the standard self-owned URLLoader lifetime pattern from Chromium's
   `ResourceBundleFileLoader`: bind both the `URLLoader` receiver and
   `URLLoaderClient` remote, delete the loader after completion or disconnect,
   and do not store per-request Mojo state on `TsExtensionsBrowserClient`.

   Do not depend on `//chrome/browser/extensions` just to call
   `chrome_url_request_util::LoadResourceFromResourceBundle()`. Electron uses
   that helper, but TermSurf should keep this layer embedder-owned and avoid a
   broad Chrome browser dependency: the helper lives in the forbidden
   `//chrome/browser/extensions` target family. Copy the small loader pattern
   instead.

   The initial PDF template replacement map is expected to be empty in this
   slice. If later viewer resources require `$i18n{...}` or other populated
   replacements, that becomes a separate follow-up.

5. Wire `TsExtensionsBrowserClient::LoadResourceFromResourceBundle()`.

   Replace the current `NOTREACHED()` with a call into the TermSurf-owned loader
   from step 4.

6. Add structured diagnostics.

   Use Chromium `LOG(INFO)` lines with this exact prefix:

   ```text
   [issue-792-exp5]
   ```

   Required low-volume lines:

   ```text
   [issue-792-exp5] extension-factory scheme=chrome-extension type=<navigation|worker-main|service-worker|subresource>
   [issue-792-exp5] bundle-resource url=<url> path=<relative_path> resource_id=<id> found=<0|1>
   [issue-792-exp5] bundle-resource-load path=<relative_path> resource_id=<id> bytes=<n> mime=<mime> ok=<0|1>
   ```

   Keep the logs low-volume. One line per factory creation and one line per
   resource lookup/load is enough for this experiment.

7. Build and archive only after verification.

   Build:

   ```bash
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   git -C chromium/src cl format --upstream=148.0.7778.97-issue-792-exp4 --full
   autoninja -C chromium/src/out/Default libtermsurf_chromium
   ```

   If the branch builds and verification passes or produces a useful Partial, do
   the full bookkeeping after Claude after-review accepts the result:
   - commit the Chromium branch;
   - regenerate `chromium/patches/issue-792/`;
   - add the new branch row to `chromium/README.md`;
   - update Experiment 5's line in `issues/0792-pdf-support/README.md` from
     `Designed` to the final status.

## Verification

1. Confirm starting state.

   ```bash
   git status --short
   git -C chromium/src status --short
   git -C chromium/src branch --show-current
   ```

   Chromium should start clean on `148.0.7778.97-issue-792-exp4`.

2. Build the branch.

   ```bash
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   git -C chromium/src cl format --upstream=148.0.7778.97-issue-792-exp4 --full
   autoninja -C chromium/src/out/Default libtermsurf_chromium
   ```

3. Run a direct extension-resource smoke.

   Reuse the debug screenshot harness against:

   ```text
   chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/index.html
   ```

   Pass requires the logs to show:
   - a `navigation` factory for `chrome-extension`;
   - `bundle-resource ... path=index.html resource_id=21596 found=1`;
   - `bundle-resource-load ... path=index.html resource_id=21596 bytes=<nonzero> mime=text/html ok=1`;
   - no crash, hang, extension IPC crash, or `NOTREACHED()` from
     `LoadResourceFromResourceBundle()`.

   The page may visually show a blank viewer shell or JavaScript errors because
   PDF viewer APIs and stream state are still out of scope. Visual PDF rendering
   is not required.

4. Confirm subresources are attempted through the same path.

   Inspect logs from the direct extension-resource smoke. If `index.html`
   requests additional extension resources, those requests should produce
   `subresource` factory and `bundle-resource-load` lines. At least one
   successful `index.html` load is required for Pass; successful subresources
   are desirable but may identify the next missing layer.

   Subresources are expected to be the most likely Partial result after
   `index.html` loads. They may expose the next missing layer: process-map
   insertion for extension renderers, re-introducing narrowly scoped
   `web_accessible_resources`, or viewer-specific API/CSP requirements. Do not
   solve those in this experiment.

5. Run normal HTML regression smoke.

   Load:

   ```text
   http://localhost:9616/index.html
   ```

   Pass requires the page to render or lifecycle logs to reach `TitleChanged`
   and `LoadingState`, with no extension IPC crash.

6. Run the PDF unchanged smoke.

   Load:

   ```text
   http://localhost:9616/bitcoin.pdf
   ```

   The PDF is still expected to take the default content_shell download path
   because this experiment does not install PDF navigation or stream handling. A
   browser crash, renderer IPC crash, or hang is a failure.

7. Run Claude review after recording the result.

   Provide Claude with the experiment file, Chromium diff, build output summary,
   runtime logs, screenshot artifact paths, and the recorded result. Fix all
   real findings before proceeding.

## Pass Criteria

- Chromium branch `148.0.7778.97-issue-792-exp5` builds `libtermsurf_chromium`.
- Direct navigation to the PDF component extension's `index.html` reaches
  Chromium's extension URL-loader path.
- `index.html` is served from `ui::ResourceBundle` with non-empty bytes and
  `text/html` MIME type.
- Experiment 4 resource-pak loading and Experiment 3 component-extension
  registration still work.
- Normal HTML browsing still works through the debug TermSurf path.
- Loading `bitcoin.pdf` does not crash; rendering is not required.
- Claude reviews the completed result and agrees it is good enough to proceed.

## Partial Criteria

Partial if:

- direct `chrome-extension://.../index.html` navigation creates the extension
  URL-loader factory but fails before `GetBundleResourcePath()`;
- `GetBundleResourcePath()` resolves `index.html` but the TermSurf-owned loader
  cannot return bytes;
- the main HTML resource loads, but required viewer subresources expose a
  separate missing extension API, CSP, or process-model layer;
- the branch builds and records exactly which missing factory or browser hook is
  needed next.

## Failure Criteria

- The experiment imports `//chrome/browser/extensions` or broad Chrome browser
  UI/resource stacks instead of adding a narrow TermSurf-owned loader.
- The experiment wires PDF navigation, stream handling, guest-view,
  MimeHandlerView, or `--pdf-renderer`.
- The experiment tries to make `bitcoin.pdf` render visually.
- The experiment changes TermSurf protocol, Wezboard, Roamium Rust, or webtui.
- The experiment regresses normal HTML browsing or reintroduces the extension
  renderer IPC crash.
- The experiment proceeds without Claude design review or ignores real Claude
  findings.

## Result

**Result:** Pass

Experiment 5 built and verified the PDF component extension resource-serving
layer.

Implementation branch:

```text
148.0.7778.97-issue-792-exp5
```

Build command:

```bash
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
git -C chromium/src cl format --upstream=148.0.7778.97-issue-792-exp4 --full
autoninja -C chromium/src/out/Default libtermsurf_chromium
```

Final build result:

```text
Build Succeeded: 2 steps
```

Direct extension-resource smoke artifact:

```text
logs/issue-792-exp5-extension-20260529-094357/
```

Required diagnostics appeared:

```text
[issue-792-exp5] extension-factory scheme=chrome-extension type=navigation
[issue-792-exp5] bundle-resource url=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/index.html path=index.html resource_id=21596 found=1
[issue-792-exp5] bundle-resource-load path=index.html resource_id=21596 bytes=529 mime=text/html ok=1
```

The run also created a subresource factory:

```text
[issue-792-exp5] extension-factory scheme=chrome-extension type=subresource
```

The viewer shell then hit expected next-layer browser policy gaps:

```text
Not allowed to load local resource: chrome://resources/css/text_defaults_md.css
Loading the script 'chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/main.js' violates the following Content Security Policy directive...
Loading the script 'chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/pdf_viewer_wrapper.js' violates the following Content Security Policy directive...
```

Those are not failures for this experiment. The pass claim is deliberately only
that the direct `index.html` extension resource reaches Chromium's extension
URL-loader path and is served from `ui::ResourceBundle`. CSP, process-map,
`web_accessible_resources`, `chrome://resources`, and viewer API wiring remain
future layers.

Normal HTML regression-smoke artifact:

```text
logs/issue-792-exp5-html-20260529-094500/
```

The HTML run reached the expected lifecycle messages:

```text
LoadingState
TitleChanged
LoadingState
```

PDF unchanged-smoke artifact:

```text
logs/issue-792-exp5-pdf-20260529-094515/
```

The PDF run still takes the pre-existing content_shell download path:

```text
ShellDownloadManagerDelegate::ChooseDownloadPath(...)
```

That is expected because this experiment does not install PDF navigation or
stream handling.

Experiment 3 and 4 behavior remained intact in the verification runs:

```text
[issue-792-exp3] pdf-component-extension-registered context=<ptr> enabled=1 inserted=1
[issue-792-exp4] pdf-resource-pak path=/Users/ryan/dev/termsurf/chromium/src/out/Default/gen/chrome/pdf_resources.pak found=1 loaded=1
[issue-792-exp4] pdf-resource-bytes id=21596 bytes=529 html_signature=1
```

During implementation, two early direct-extension smokes found real bugs that
were fixed before this result:

- empty template replacements caused a fatal `$i18n{textdirection}` check; fixed
  by skipping template replacement when the replacement map is empty and by
  using the non-fatal HTML replacement mode;
- a malformed/non-root extension subresource request hit a debug DCHECK inside
  Chromium's cross-renderer resource helper; fixed by delegating to that helper
  only for well-formed extension URLs whose root matches the extension being
  checked.

All final runs still show the known teardown `SEGV_ACCERR` after artifacts were
captured. This is the same cleanup crash recorded in earlier PDF experiments and
is not caused by the Experiment 5 resource-serving layer.

Bookkeeping status: Chromium branch commit, patch archive refresh,
`chromium/README.md` branch row, and the issue README status flip are deferred
until Claude after-review accepts this result.

## Conclusion

TermSurf can now serve the PDF component extension's `index.html` through the
canonical `chrome-extension://` URL-loader path, using a TermSurf-owned loader
that reads from `ui::ResourceBundle`. This proves the registered extension is no
longer just metadata plus bytes; Chromium can navigate to the extension URL and
receive the viewer shell as `text/html`.

The next experiment should address the first viewer-shell policy gap surfaced by
this run. The likely next layer is making the PDF extension's own scripts and
`chrome://resources` dependencies load under an appropriate, narrow security
model: either process-map insertion for the extension renderer, restoring the
needed manifest permissions/resources, and/or serving the required
`chrome://resources` assets. It should still avoid PDF navigation and stream
handoff until the viewer shell's static dependency graph loads cleanly.
