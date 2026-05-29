# Experiment 6: Register Extension Renderer Processes

## Description

Experiment 5 proved TermSurf can serve the PDF component extension's
`index.html` through Chromium's `chrome-extension://` URL-loader path. The
viewer shell now loads as an extension URL, but its static dependencies still
hit policy barriers:

```text
Not allowed to load local resource: chrome://resources/css/text_defaults_md.css
Loading the script 'chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/main.js' violates the following Content Security Policy directive...
Loading the script 'chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/pdf_viewer_wrapper.js' violates the following Content Security Policy directive...
```

The next narrow layer is to make TermSurf's browser process recognize the PDF
viewer renderer as an extension renderer. Electron does this in
`ElectronBrowserClient::SiteInstanceGotProcessAndSite()` by looking up the
extension for the `SiteInstance` URL and inserting the extension id/process id
into `extensions::ProcessMap`. app_shell does the same in
`ShellContentBrowserClient::SiteInstanceGotProcessAndSite()`.

TermSurf currently does not override this hook. That means the PDF viewer frame
can navigate to an extension URL and receive `index.html`, but later extension
resource and policy checks may not know that the renderer process belongs to the
PDF extension.

This experiment adds only that process-map registration layer plus the canonical
Electron/app_shell companion hooks that register `chrome-extension://` as a real
extension scheme, make extension URLs handled by the embedder, and keep
extension URLs process-per-site, then measures what changes. It does not wire
PDF navigation, `PdfNavigationThrottle`, `PdfViewerStreamManager`, guest-view,
MimeHandlerView, `--pdf-renderer`, `chrome://resources` serving, PDF viewer
private APIs, or stream handoff.

This experiment must receive Claude design review before implementation. After
implementation and result recording, Claude must review the completed output
before any next experiment is designed.

## Changes

1. Create the Chromium implementation branch.

   Start from the accepted Experiment 5 branch:

   ```bash
   git -C chromium/src checkout 148.0.7778.97-issue-792-exp5
   git -C chromium/src checkout -b 148.0.7778.97-issue-792-exp6
   ```

   Add the branch to `chromium/README.md` only after the branch builds and the
   result is accepted.

2. Add `TsBrowserClient::SiteInstanceGotProcessAndSite()`.

   Implement the same narrow pattern Electron and app_shell use:
   - get the `BrowserContext` from the `SiteInstance`;
   - skip off-the-record contexts;
   - get `extensions::ExtensionRegistry` for the context;
   - resolve the extension with
     `registry->enabled_extensions().GetExtensionOrAppByURL(site_instance->GetSiteURL())`;
   - if no extension matches, return without side effects;
   - if the `SiteInstance` security principal is sandboxed, return without side
     effects;
   - insert:

     ```cpp
     extensions::ProcessMap::Get(browser_context)
         ->Insert(extension->id(),
                  site_instance->GetProcess()->GetDeprecatedID());
     ```

   Scope this to extension URLs discovered by Chromium's registry. Do not
   special-case the PDF extension id unless Chromium's lookup fails and the
   result is recorded as Partial.

   Skipping off-the-record contexts matches Experiment 3's deliberate scoping:
   the PDF extension is only enabled in the regular context.

3. Add `TsBrowserClient::ShouldUseProcessPerSite()`.

   Implement Electron's narrower version, not app_shell's global `return true`:
   - if the effective URL is not `chrome-extension://`, return the base
     `ShellContentBrowserClient::ShouldUseProcessPerSite(...)` result;
   - if the effective URL is `chrome-extension://`, get
     `extensions::ExtensionRegistry` for the context;
   - return true only when
     `registry->enabled_extensions().GetByID(effective_url.GetHost())` returns
     an enabled extension.

   This companion hook is needed because without it Chromium may collapse the
   PDF viewer's `SiteInstance` site URL to bare `chrome-extension:`, which gives
   `SiteInstanceGotProcessAndSite()` no extension id to look up.

4. Add a TermSurf `ContentClient` scheme registration.

   TermSurf currently inherits content_shell's `ShellContentClient`, which does
   not register `chrome-extension://` as a standard extension scheme.
   extensions_shell and Chrome both register the extension scheme in
   `ContentClient::AddAdditionalSchemes()`. Without this, URL/origin code can
   treat `chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/index.html` as a
   scheme-only origin, producing the bare site URL `chrome-extension:` before
   `ShouldUseProcessPerSite()` or `SiteInstanceGotProcessAndSite()` get a useful
   extension id.

   Add a small `TsContentClient` that derives from content_shell's
   `ShellContentClient`, calls the base `AddAdditionalSchemes()`, then registers
   `extensions::kExtensionScheme` in the same scheme buckets used by
   extensions_shell:
   - `standard_schemes`;
   - `savable_schemes`;
   - `secure_schemes`;
   - `cors_enabled_schemes`;
   - `csp_bypassing_schemes`.

   Update `TsMainDelegate::CreateContentClient()` to return `TsContentClient`.
   Do not replace unrelated content_shell resource, GPU, utility, or renderer
   clients.

   Hold the `TsContentClient` through the inherited content-shell
   `content_client_` ownership slot and return its raw pointer, matching
   `ShellMainDelegate::CreateContentClient()`.

   Registering `csp_bypassing_schemes` matches extensions_shell's canonical
   scheme registration. It does not directly bypass the PDF extension page's own
   manifest CSP; that still depends on the downstream renderer/process and
   policy layers.

5. Add `TsBrowserClient::IsHandledURL()`.

   TermSurf inherits content_shell's handled-URL allowlist. content_shell does
   not include `chrome-extension://`; app_shell explicitly does. If the embedder
   does not report extension URLs as handled, Chromium may still reduce the PDF
   viewer site to the bare scheme before the process-per-site and process-map
   hooks can identify the extension id.

   Implement the narrow app_shell-compatible behavior:
   - if the URL scheme is `chrome-extension://`, return true;
   - otherwise delegate to `ShellContentBrowserClient::IsHandledURL(url)`.

   This hook is intentionally scheme-level, not PDF-extension-id-specific,
   because `IsHandledURL()` has no `BrowserContext` and app_shell treats the
   extension scheme as an embedder-handled internal scheme.

6. Add minimal diagnostics.

   Use Chromium `LOG(INFO)` lines with this exact prefix:

   ```text
   [issue-792-exp6]
   ```

   Required low-volume line when an extension process is inserted:

   ```text
   [issue-792-exp6] process-map-insert extension_id=<id> process_id=<id> site_url=<url>
   ```

   Add one low-volume line when `TsContentClient` registers the extension
   scheme:

   ```text
   [issue-792-exp6] extension-scheme-registered scheme=chrome-extension
   ```

   If Chromium's registry lookup fails for the direct PDF extension URL, log:

   ```text
   [issue-792-exp6] process-map-miss site_url=<url>
   ```

   Keep misses low-volume. A miss for ordinary `http://` pages is expected and
   should not be logged. Log `process-map-miss` only when
   `site_instance->GetSiteURL().SchemeIs(extensions::kExtensionScheme)` is true
   and the registry returned no extension.

   Add one low-volume line when `ShouldUseProcessPerSite()` recognizes an
   enabled extension:

   ```text
   [issue-792-exp6] process-per-site extension_id=<id> effective_url=<url>
   ```

   Add one low-volume line when `IsHandledURL()` handles an extension URL:

   ```text
   [issue-792-exp6] handled-url url=<url>
   ```

   Deduplicate this diagnostic so it emits at most once per process. Chromium
   can call `IsHandledURL()` many times for the same navigation, and this log is
   only meant to prove the hook is wired.

7. Do not widen the experiment.

   Forbidden in this experiment:
   - PDF navigation or MIME interception;
   - `PdfViewerStreamManager`;
   - guest-view or MimeHandlerView;
   - `--pdf-renderer`;
   - `chrome://resources` URL-loader work;
   - changing the PDF extension manifest;
   - restoring `web_accessible_resources`;
   - adding PDF viewer private APIs.

   If process-map insertion is not enough to unblock the viewer scripts, record
   exactly which policy/resource error remains and design the next experiment
   around that layer.

8. Build and archive only after verification.

   Build:

   ```bash
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   git -C chromium/src cl format --upstream=148.0.7778.97-issue-792-exp5 --full
   autoninja -C chromium/src/out/Default libtermsurf_chromium
   ```

   If the branch builds and verification passes or produces a useful Partial, do
   the full bookkeeping after Claude after-review accepts the result:
   - commit the Chromium branch;
   - regenerate `chromium/patches/issue-792/`;
   - add the new branch row to `chromium/README.md`;
   - update Experiment 6's line in `issues/0792-pdf-support/README.md` from
     `Designed` to the final status.

## Verification

1. Confirm starting state.

   ```bash
   git status --short
   git -C chromium/src status --short
   git -C chromium/src branch --show-current
   ```

   Chromium should start clean on `148.0.7778.97-issue-792-exp5`.

2. Build the branch.

   ```bash
   export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
   git -C chromium/src cl format --upstream=148.0.7778.97-issue-792-exp5 --full
   autoninja -C chromium/src/out/Default libtermsurf_chromium
   ```

3. Run the direct extension-resource smoke.

   Reuse the debug screenshot harness against:

   ```text
   chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/index.html
   ```

   Pass requires:
   - Experiment 5 still serves `index.html`:

     ```text
     [issue-792-exp5] bundle-resource-load path=index.html resource_id=21596 bytes=<n> mime=text/html ok=1
     ```

   - Experiment 6 inserts the PDF extension renderer process:

     ```text
     [issue-792-exp6] extension-scheme-registered scheme=chrome-extension
     [issue-792-exp6] handled-url url=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/index.html
     [issue-792-exp6] process-per-site extension_id=mhjfbmdgcfjbbpaeojofohoefgiehjai effective_url=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/
     [issue-792-exp6] process-map-insert extension_id=mhjfbmdgcfjbbpaeojofohoefgiehjai process_id=<id> site_url=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/
     ```

   - no `FATAL`, `NOTREACHED`, renderer IPC crash, or hang occurs before the
     screenshot artifact is captured.

   Compare the post-insert console/resource errors with Experiment 5's direct
   extension artifact:

   ```text
   logs/issue-792-exp5-extension-20260529-094357/
   ```

   Record whether the `main.js` and `pdf_viewer_wrapper.js` CSP errors are gone,
   changed, or unchanged. If they remain unchanged, process-map insertion was
   necessary foundation but not sufficient for viewer script loading.

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

   The PDF is still expected to take the default content_shell download path
   because this experiment does not install PDF navigation or stream handling. A
   browser crash, renderer IPC crash, or hang is a failure.

6. Run Claude review after recording the result.

   Provide Claude with the experiment file, Chromium diff, build output summary,
   runtime logs, screenshot artifact paths, and the recorded result. Fix all
   real findings before proceeding.

## Pass Criteria

- Chromium branch `148.0.7778.97-issue-792-exp6` builds `libtermsurf_chromium`.
- Direct navigation to the PDF component extension still serves `index.html`
  from `ui::ResourceBundle`.
- `TsContentClient` registers `chrome-extension://` as an extension-style
  scheme.
- `IsHandledURL()` recognizes `chrome-extension://` as an embedder-handled URL.
- `ShouldUseProcessPerSite()` recognizes the PDF extension URL.
- `SiteInstanceGotProcessAndSite()` inserts the PDF extension id/process id into
  `extensions::ProcessMap`.
- The direct extension smoke does not crash or hang.
- Normal HTML browsing still works through the debug TermSurf path.
- Loading `bitcoin.pdf` does not crash; rendering is not required.
- Claude reviews the completed result and agrees it is good enough to proceed.

## Partial Criteria

Partial if:

- scheme registration occurs, but `IsHandledURL()` still does not fire;
- the branch builds and the hook fires, but
  `GetExtensionOrAppByURL(site_instance->GetSiteURL())` returns no extension for
  the direct PDF extension URL;
- `IsHandledURL()` recognizes the extension URL, but `ShouldUseProcessPerSite()`
  still does not fire. In that case, the next likely missing hook is
  `GetEffectiveURL()`;
- `ShouldUseProcessPerSite()` recognizes the extension URL, but the
  `SiteInstance` still reports only bare `chrome-extension:`;
- process-map insertion succeeds, but the viewer scripts remain blocked by the
  same CSP/resource errors from Experiment 5;
- process-map insertion succeeds, but the next missing layer is clearly
  `chrome://resources` serving, manifest policy, `web_accessible_resources`, or
  viewer API binding.

## Failure Criteria

- The experiment changes PDF navigation, stream handling, guest-view,
  MimeHandlerView, or `--pdf-renderer`.
- The experiment changes the PDF extension manifest or restores
  `web_accessible_resources`.
- The experiment imports Chrome browser UI/resource stacks.
- The experiment changes TermSurf protocol, Wezboard, Roamium Rust, or webtui.
- The experiment regresses normal HTML browsing or reintroduces the extension
  renderer IPC crash.
- The experiment proceeds without Claude design review or ignores real Claude
  findings.

## Result

**Result:** Pass

Chromium branch `148.0.7778.97-issue-792-exp6` builds `libtermsurf_chromium`.

The direct extension-resource smoke used:

```text
logs/issue-792-exp6-extension-20260529-101127/
```

The log proves the PDF extension renderer process is now recognized as an
extension process:

```text
[issue-792-exp6] extension-scheme-registered scheme=chrome-extension
[issue-792-exp6] process-per-site extension_id=mhjfbmdgcfjbbpaeojofohoefgiehjai effective_url=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/
[issue-792-exp6] process-map-insert extension_id=mhjfbmdgcfjbbpaeojofohoefgiehjai process_id=5 site_url=chrome-extension://mhjfbmdgcfjbbpaeojofohoefgiehjai/
```

Experiment 5's resource serving also advanced beyond the prior script CSP
failure. `index.html`, `main.js`, `pdf_viewer_wrapper.js`, and `browser_api.js`
were all served from the PDF extension resource bundle:

```text
[issue-792-exp5] bundle-resource-load path=index.html resource_id=21596 bytes=529 mime=text/html ok=1
[issue-792-exp5] bundle-resource-load path=main.js resource_id=21592 bytes=2097 mime=text/javascript ok=1
[issue-792-exp5] bundle-resource-load path=pdf_viewer_wrapper.js resource_id=21599 bytes=267587 mime=text/javascript ok=1
[issue-792-exp5] bundle-resource-load path=browser_api.js resource_id=21591 bytes=7790 mime=text/javascript ok=1
```

The direct extension smoke no longer shows the Experiment 5 `main.js` and
`pdf_viewer_wrapper.js` CSP violations. The remaining direct-extension failures
are `chrome://resources/...` loads:

```text
Not allowed to load local resource: chrome://resources/css/text_defaults_md.css
Not allowed to load local resource: chrome://resources/js/assert.js
Not allowed to load local resource: chrome://resources/lit/v3_0/lit.rollup.js
Not allowed to load local resource: chrome://resources/js/load_time_data.js
Not allowed to load local resource: chrome://resources/mojo/mojo/public/js/bindings.js
```

`TsBrowserClient::IsHandledURL()` remained a defensive app_shell-compatible hook
but did not emit its diagnostic in the passing smoke. The load-bearing fix was
earlier: `TsContentClient::AddAdditionalSchemes()` registering
`chrome-extension://` as a real standard extension scheme. Once that happened,
Chromium preserved the extension id in the site URL and the process-map hooks
could fire. The Exp 6 iterations showed `AddAdditionalSchemes()` was sufficient
to engage Chromium's standard URL handling for this navigation path;
`IsHandledURL()` is kept as the app_shell-compatible companion hook for
resilience against other URL-classification paths, but it was not load-bearing
in the passing smoke.

Normal HTML regression smoke:

```text
logs/issue-792-exp6-html-20260529-101410/
```

The run used `http://localhost:9616/index.html`, reached `UrlChanged`,
`TitleChanged`, and `LoadingState`, and captured a non-empty screenshot
artifact.

PDF unchanged smoke:

```text
logs/issue-792-exp6-pdf-20260529-101422/
```

The run used `http://localhost:9616/bitcoin.pdf` and still reached the expected
content_shell download path:

```text
ShellDownloadManagerDelegate::ChooseDownloadPath(...)
```

This is expected because Experiment 6 does not implement PDF navigation,
streaming, or viewer handoff.

The known Roamium teardown `SEGV_ACCERR` recurred after screenshot/log capture
in the smokes. This is unchanged from prior experiments and remains out of scope
for the PDF infrastructure slice.

Bookkeeping status: Chromium branch commit, patch archive refresh, and
`chromium/README.md` branch row update completed after Claude accepted the
result.

## Conclusion

Experiment 6 proves that TermSurf now has the canonical extension renderer
process recognition path needed by the PDF viewer shell:

1. `chrome-extension://` is registered as a standard extension scheme.
2. Chromium keeps the PDF extension id in the site URL.
3. `ShouldUseProcessPerSite()` recognizes the PDF extension URL.
4. `SiteInstanceGotProcessAndSite()` inserts the extension id/process id into
   `extensions::ProcessMap`.

The next missing layer is no longer extension process recognition. The viewer
now advances far enough to request its shared WebUI dependencies, and those fail
because TermSurf does not yet serve or allow `chrome://resources/...` for the
PDF extension context. The next experiment should target `chrome://resources`
resource serving/access for the PDF viewer shell, while still avoiding PDF
navigation, streams, MimeHandlerView, and private APIs.
