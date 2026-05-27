+++
status = "open"
opened = "2026-04-11"
+++

# Issue 776: PDF files show blank white screen instead of rendering

## Goal

Opening a local PDF file via `web file.pdf` should render the PDF content, not a
blank white screen.

## Background

When a user runs `web file.pdf` (or any local PDF path), the browser pane opens
but displays a blank white screen instead of rendering the PDF. The navigation
appears to work (no error is shown), but no PDF content is visible.

Chromium normally has a built-in PDF viewer (PDFium) that renders PDFs inline.
The blank screen suggests either:

1. The PDF viewer plugin is not enabled or not included in the Chromium
   build/configuration used by Roamium.
2. The `file://` URL scheme is not being handled correctly for PDF MIME types.
3. The PDF viewer requires UI chrome (toolbar, scroll handling) that isn't
   available in the embedded context.
4. Content Security Policy or sandboxing restrictions are blocking the PDF
   plugin from loading.

## Analysis

Possible areas to investigate:

- **Chromium PDF plugin registration** — Check whether `libtermsurf_chromium`
  enables the PDF viewer plugin (`chrome_pdf`) during browser initialization.
  Headless or minimal embeddings often skip plugin registration.
- **MIME type handling** — Verify that `file://` URLs with `.pdf` extension are
  being served with `application/pdf` MIME type and routed to the PDF viewer.
- **Plugin process sandboxing** — The PDF viewer runs in a utility process;
  sandbox restrictions in the TermSurf build may prevent it from launching.
- **Alternative approach** — If enabling the built-in viewer is complex,
  consider whether PDF.js (Mozilla's JavaScript PDF renderer) could work as a
  fallback.

## Experiments

### Experiment 1: Probe Chromium PDF Viewer Plumbing

#### Description

The likely cause is not `webtui` path handling. TermSurf's Chromium embedder is
built on Content Shell, and Content Shell does not wire Chromium's PDF viewer
pipeline. Electron's embedder shows the required pieces explicitly:

- PDF build deps and resources;
- PDF plugin registration through `ContentClient::AddPlugins()`;
- PDF component-extension/resource loading;
- PDF stream routing for the PDF viewer;
- renderer-side creation of the internal PDF plugin with
  `pdf::CreateInternalPlugin()`.

TermSurf currently overrides only `CreateContentBrowserClient()` in
`TsMainDelegate`. It does not provide a TermSurf `ContentClient`, does not
provide a TermSurf renderer client, and `libtermsurf_chromium/BUILD.gn` has no
PDF deps. Therefore the PDF navigation can succeed while no viewer exists to
render the document.

This experiment is a scoped PDF-rendering probe with automated screenshot
capture and agent visual triage. It should add the smallest TermSurf
`ContentClient` / renderer-client hooks needed to make Chromium's built-in PDF
path available, then run TermSurf end-to-end and take a screenshot of a real PDF
loaded in the browser pane.

The primary question for this experiment is user-visible:

```text
Does the PDF visibly render in the TermSurf browser pane?
```

The experiment should not require manual app interaction for verification. It
will capture screenshots automatically, then the agent will inspect those
artifacts visually. The screenshot script does not need to implement OCR or
image classification; it only needs to produce deterministic artifacts. If a
screenshot shows recognizable PDF content, the experiment passes. If the
screenshot is still blank or white, the result should record the artifact and
only then use logs to decide the next experiment.

Use the vendored Bitcoin whitepaper fixture:

```text
test-html/public/bitcoin.pdf
```

The primary automated run should open it through the local test server:

```text
http://localhost:9616/bitcoin.pdf
```

That avoids network flakiness while still exercising Chromium's browser PDF
rendering path. If the HTTP fixture renders, also run a second screenshot check
against the same file through `file://` to verify the original local-file goal:

```text
file:///Users/ryan/dev/termsurf/test-html/public/bitcoin.pdf
```

This experiment commits to the expensive product direction: in-pane PDF viewing
inside the TermSurf browser overlay. A simpler fallback would be to hand PDFs to
macOS Preview or another external viewer, but that breaks the in-terminal
experience. If the Chromium pipeline proves far larger than expected in later
experiments, that product trade-off can be revisited explicitly.

#### Changes

1. Precondition checks:
   - Verify the current Chromium GN args do not disable PDF/plugin/extension
     support:
     ```bash
     cd chromium/src
     gn args out/Default --list | rg 'enable_pdf|enable_extensions|enable_plugins'
     ```
     Expected: `enable_pdf`, `enable_extensions`, and `enable_plugins` are
     enabled under defaults or are not overridden to `false`.
   - After adding the minimal PDF deps, run
     `gn check out/Default //content/libtermsurf_chromium`. `testonly = true` is
     not expected to block the listed deps; if GN proves otherwise, stop and
     record the exact dependency error instead of silently changing the target's
     test-only status.
2. Create a new Chromium branch for this issue:
   - start from the most relevant current branch, currently
     `148.0.7778.97-issue-778`;
   - create `148.0.7778.97-issue-776`;
   - add it to the Branches table in `chromium/README.md`.
3. Update `chromium/src/content/libtermsurf_chromium/BUILD.gn` to include only
   the minimal PDF deps needed for this plumbing probe:
   - `//pdf`;
   - `//pdf:features`;
   - `//components/pdf/common:constants`;
   - `//components/pdf/common:util`;
   - `//components/pdf/renderer`. Do not add `//components/pdf/browser`,
     extension, MimeHandlerView, or stream-manager deps in this experiment
     unless a compile error proves one of them is needed by the plumbing probe
     itself.

   Do not include `//chrome/browser/resources/pdf:resources` in the first build
   attempt. If the plugin is created but the screenshot remains blank, record
   "plugin created, viewer resources or MimeHandlerView missing" as an expected
   Partial outcome. A compile error is not expected to reveal this runtime
   missing-resource layer.

4. Add a TermSurf content client, for example `ts_content_client.{cc,h}`,
   derived from `ShellContentClient`.
   - Override `AddPlugins()`.
   - Register the internal PDF plugin using the Chrome/Electron pattern:
     `pdf::kInternalPluginMimeType`, extension `"pdf"`, description
     `"Portable Document Format"`, and
     `content::WebPluginInfo::PLUGIN_TYPE_BROWSER_INTERNAL_PLUGIN`.
   - Keep all existing `ShellContentClient` behavior by inheriting from it.
   - Add a temporary `LOG(INFO)` line confirming PDF plugin registration.
5. Update `TsMainDelegate` to override `CreateContentClient()` and return the
   new TermSurf content client.
6. Add a TermSurf renderer client, for example `ts_renderer_client.{cc,h}`,
   derived from `ShellContentRendererClient`.
   - Override `OverrideCreatePlugin()`.
   - First preserve the existing Shell behavior, including surface-embed plugin
     handling.
   - If `params.mime_type.Utf8() == pdf::kInternalPluginMimeType`, call
     `pdf::CreateInternalPlugin(std::move(params), render_frame, {})`.
   - Return `true` only when a plugin was actually created. If PDF creation
     returns `nullptr`, let the normal Shell fallback continue or fail visibly;
     do not silently synthesize a blank page.
   - Add temporary `LOG(INFO)` lines that report each PDF MIME type seen by
     `OverrideCreatePlugin()` and whether `pdf::CreateInternalPlugin()` returned
     a plugin or `nullptr`.
7. Override `IsPluginHandledExternally()` in the TermSurf renderer client.
   - Log when `mime_type == pdf::kPDFMimeType` or
     `pdf::kInternalPluginMimeType`.
   - Do not try to implement MimeHandlerView in this experiment.
   - The method exists on Chromium 148's `ContentRendererClient`; implement the
     override directly. If the implementation reveals that useful behavior
     requires broad extensions infrastructure, record that explicitly in the
     result instead of silently widening scope.
8. Update `TsMainDelegate` to override `CreateContentRendererClient()` and
   return the new TermSurf renderer client.
9. Do not wire the PDF component-extension, MimeHandlerView, GuestView, or
   streams-private path in this experiment. Instead, document them as the likely
   next layer if plugin registration succeeds but PDFs still do not render. The
   future implementation should be based on Electron's:
   - `shell/browser/extensions/electron_extension_system.cc`;
   - `shell/browser/extensions/electron_component_extension_resource_manager.cc`;
   - `shell/browser/extensions/api/streams_private/streams_private_api.cc`;
   - Chromium patch equivalent for redirecting
     `plugin_response_interceptor_url_loader_throttle.cc` to the embedder's
     streams-private implementation.
10. Do not change:
    - `webtui`;
    - Roamium's Rust IPC;
    - `termsurf.proto`;
    - Wezboard overlay positioning or input forwarding.

11. Build `libtermsurf_chromium` with `autoninja`, rebuild Roamium against the
    new library, and run the diagnostic verification. Regenerate and commit the
    Issue 776 Chromium patch archive on Pass or Partial. Skip archiving only for
    a Fail result that leaves the Chromium branch in an incoherent state, and
    document why the archive was skipped.

12. Add an automated visual PDF smoke-test command or script.

    The automation should:
    - create an isolated log/state directory under `logs/issue-776-exp1-*`;
    - launch debug Wezboard directly from `wezboard/target/debug/wezboard-gui`;
    - launch debug `webtui/target/debug/web` inside that Wezboard session using
      the repo-built Roamium binary:

      ```text
      /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium
      ```

    - ensure the local test server is running and serves
      `test-html/public/bitcoin.pdf`;
    - open:

      ```text
      http://localhost:9616/bitcoin.pdf
      ```

    - wait for the browser pane to settle;
    - capture a macOS screenshot with `screencapture`;
    - save the screenshot under `logs/issue-776-exp1-*`;
    - print the screenshot path and any relevant log paths.

    Prefer a direct startup-command or command-line launch path if Wezboard
    exposes one, because it avoids keyboard focus flakiness. If no reliable
    direct launch path exists, the automation may use macOS GUI automation, such
    as AppleScript/System Events, to type the debug `web` command into the newly
    launched Wezboard window. This is acceptable for this experiment because the
    target is an end-to-end visual smoke test of the actual GUI path.

    If AppleScript/System Events are used, Accessibility permission is also a
    precondition. Screen Recording permission is required for `screencapture`;
    Accessibility permission is required for typed GUI automation.

    The automation must not rely on installed stable TermSurf binaries. It must
    use debug Wezboard, debug `web`, and the repo-built Roamium/Chromium path.

#### Non-Negotiable Invariants

- Local PDF rendering is fixed in Chromium/Roamium, not by special-casing PDF
  paths in `webtui`.
- Normal HTML navigation, local file navigation, link clicks, scrolling, and
  keyboard input remain unchanged.
- The fix must not introduce a second browser surface or a separate native
  window for PDFs; PDFs should render in the existing browser overlay.
- The PDF viewer must not require changes to the TermSurf protobuf protocol.
- The implementation must preserve Content Shell behavior unrelated to PDFs.
- Experiment 1 must not quietly grow into an extensions/MimeHandlerView port. If
  that layer is required, stop with a Partial result and design Experiment 2
  around it.
- The Chromium changes must live on the `148.0.7778.97-issue-776` branch. On
  Pass or Partial, they must be archived under `chromium/patches/issue-776/`. On
  an incoherent Fail, the result must explain why archiving was skipped.

#### Verification

This experiment's verification uses automated capture plus agent visual triage.
Manual inspection by the user is not required to decide whether the experiment
passes. The automation produces screenshots and logs; the agent classifies the
artifact by visual inspection.

1. Confirm screenshot capture is available before running the full test:

   ```bash
   screencapture -x /private/tmp/termsurf-screenshot-permission-test.png
   test -s /private/tmp/termsurf-screenshot-permission-test.png
   ```

   If this fails, stop and record the permission problem. The experiment cannot
   be visually verified until macOS Screen Recording permission is granted to
   the process running the test.

   If the automation uses AppleScript/System Events to type into Wezboard,
   verify Accessibility permission as well. If Accessibility permission is not
   available, either use a non-typed launch path or stop and record the
   automation permission problem.

2. Record the precondition results:
   - `enable_pdf`, `enable_extensions`, and `enable_plugins` state;
   - whether `gn check out/Default //content/libtermsurf_chromium` passes after
     the minimal deps are added.

3. Build Chromium:

   ```bash
   cd chromium/src
   export PATH="$(cd ../depot_tools && pwd):$PATH"
   autoninja -C out/Default libtermsurf_chromium
   ```

4. Build Wezboard, Roamium, and webtui in debug mode:

   ```bash
   cd /Users/ryan/dev/termsurf
   ./scripts/build.sh wezboard
   ./scripts/build.sh roamium
   ./scripts/build.sh webtui
   ```

5. Run the automated visual PDF smoke test.

   Start the local test server if it is not already running. The fixture is:

   ```text
   /Users/ryan/dev/termsurf/test-html/public/bitcoin.pdf
   ```

   The test must launch debug Wezboard and debug `web`, explicitly passing the
   repo-built Roamium binary:

   ```bash
   /Users/ryan/dev/termsurf/webtui/target/debug/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     http://localhost:9616/bitcoin.pdf
   ```

   The test then waits for load and captures a screenshot.

6. If the HTTP fixture renders, run a second screenshot check against the local
   file URL:

   ```bash
   /Users/ryan/dev/termsurf/webtui/target/debug/web \
     --browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium \
     file:///Users/ryan/dev/termsurf/test-html/public/bitcoin.pdf
   ```

   This verifies the original `web file.pdf` class of bug after the renderer
   path is proven to work.

7. Inspect the screenshot artifacts.

   The automated result should classify the screenshot as one of:
   - **rendered PDF** — visible Bitcoin whitepaper content is present, such as
     the title `Bitcoin: A Peer-to-Peer Electronic Cash System`, author text, or
     body paragraphs;
   - **blank/white** — the browser pane is blank or shows a white PDF surface
     without recognizable document content;
   - **navigation/error page** — TermSurf reached an error page, network error,
     or non-PDF page instead of the PDF;
   - **automation failure** — Wezboard, `web`, Roamium, network access, or
     screenshot capture failed before the visual state could be determined.

   A human-readable screenshot path must be recorded in the experiment result.

8. If and only if the screenshot is blank/white or an error page, collect the
   logs and record a failure-layer table:

   | Layer                                                       | Result                 |
   | ----------------------------------------------------------- | ---------------------- |
   | PDF build flags enabled                                     | yes/no                 |
   | Minimal PDF deps compile                                    | yes/no                 |
   | PDF plugin registered in `AddPlugins()`                     | yes/no                 |
   | `OverrideCreatePlugin()` sees internal PDF MIME type        | yes/no                 |
   | `CreateInternalPlugin()` returns a plugin                   | yes/no/null-not-called |
   | `IsPluginHandledExternally()` sees top-level PDF            | yes/no                 |
   | Plugin created but viewer resources/MimeHandlerView missing | yes/no                 |
   | Evidence that MimeHandlerView/extension layer is next       | yes/no                 |
   | Evidence that PDF stream routing is next                    | yes/no                 |
   | Evidence that plugin/utility sandboxing is next             | yes/no                 |

   Copy this table into the result with concrete `yes`, `no`, or
   `null-not-called` values instead of reconstructing the diagnosis from memory
   after the run.

#### Pass Criteria

The automated screenshot for the local HTTP fixture shows recognizable Bitcoin
PDF content rendered inside the existing TermSurf browser overlay.

The pass decision is visual. It does not require proving every PDF plumbing
layer, and it does not require manual interaction beyond granting screenshot and
automation permissions before the run.

If the HTTP fixture renders but the `file://` fixture does not, the experiment
still passes for browser PDF rendering and should open a follow-up focused on
local-file MIME/path handling.

#### Partial Criteria

The screenshot is still blank/white or reaches an error page, but the automated
run succeeds and logs identify the first missing layer in dependency order:

1. PDF build flags/deps;
2. plugin registration;
3. renderer plugin creation;
4. top-level PDF external plugin handling;
5. MimeHandlerView/extension infrastructure;
6. PDF stream routing.

If multiple layers are missing, the next experiment should target the earliest
missing layer in this list.

#### Failure Criteria

- The automated test cannot launch the correct debug Wezboard/debug `web`/repo
  Roamium path.
- The automated test uses installed stable TermSurf binaries instead of repo
  debug binaries.
- Screenshot capture fails after permission has been granted.
- The experiment makes PDFs blank or crashes without producing a screenshot or
  enough logs to identify the first missing layer.
- The fix only handles PDFs by opening an external app or native window.
- The fix adds PDF-specific behavior to `webtui` while the Chromium viewer
  remains unwired.
- Normal HTML or local file navigation regresses.
- Roamium crashes when opening a PDF.
- The experiment quietly starts porting the extension/MimeHandlerView stack
  instead of stopping at the scoped plumbing probe.

**Result:** Partial

Experiment 1 implemented the scoped Chromium PDF plumbing probe and automated
visual smoke test. The Chromium branch `148.0.7778.97-issue-776` now adds a
TermSurf `ContentClient`, registers the internal Chromium PDF plugin, adds a
TermSurf renderer client, and logs PDF-related renderer hooks. The automation
script `scripts/test-issue-776-pdf.sh` launches debug Wezboard, debug `web`, and
the repo-built Roamium binary, then captures a screenshot of the vendored
Bitcoin PDF served by the local test server.

The automated run succeeded, but the PDF did not render. Screenshot artifact:

```text
/Users/ryan/dev/termsurf/logs/issue-776-exp1-20260527-075821/pdf-smoke.png
```

Visual classification: `blank/white`. The browser pane reached
`http://localhost:9616/bitcoin.pdf`, but no recognizable Bitcoin whitepaper
content appeared.

Build and verification notes:

- `autoninja -C out/Default libtermsurf_chromium` succeeded after adding the
  minimal PDF deps.
- `./scripts/build.sh wezboard`, `./scripts/build.sh roamium`, and
  `./scripts/build.sh webtui` succeeded in debug mode.
- The GN PDF/plugin/extension precondition did not show an override disabling
  PDF, plugins, or extensions.
- `gn check out/Default //content/libtermsurf_chromium` is not usable as a clean
  PDF-dependency signal yet because the target already has private-header
  dependency errors unrelated to this experiment.

Failure-layer table:

| Layer                                                       | Result          |
| ----------------------------------------------------------- | --------------- |
| PDF build flags enabled                                     | yes             |
| Minimal PDF deps compile                                    | yes             |
| PDF plugin registered in `AddPlugins()`                     | yes             |
| `OverrideCreatePlugin()` sees internal PDF MIME type        | no              |
| `CreateInternalPlugin()` returns a plugin                   | null-not-called |
| `IsPluginHandledExternally()` sees top-level PDF            | no              |
| Plugin created but viewer resources/MimeHandlerView missing | no              |
| Evidence that MimeHandlerView/extension layer is next       | likely          |
| Evidence that PDF stream routing is next                    | unknown         |
| Evidence that plugin/utility sandboxing is next             | no              |

Key log evidence:

```text
[issue-776] registered internal PDF plugin mime=application/x-google-chrome-pdf
Not implemented reached in content::ShellDownloadManagerDelegate::ChooseDownloadPath(...)
```

No `OverrideCreatePlugin()` or `IsPluginHandledExternally()` PDF log lines were
emitted. That means the top-level PDF navigation never reached the renderer
plugin creation path. Content Shell treated the PDF as a download before the
minimal renderer hook could create the internal PDF plugin.

The automation cleanup also produced a Roamium crash after the screenshot was
captured and the test process was torn down. That is a residual cleanup risk,
not the first PDF-rendering failure layer, because the screenshot and relevant
logs were already captured.

#### Conclusion

Experiment 1 proved that minimal renderer-side PDF plugin plumbing is
insufficient for top-level PDF navigation in TermSurf's Content Shell-based
embedder. Registering the internal PDF plugin works, but loading
`http://localhost:9616/bitcoin.pdf` goes through Content Shell's download path
and never reaches `IsPluginHandledExternally()` or `OverrideCreatePlugin()`.

The next experiment should target browser-side top-level PDF handling rather
than adding more renderer-only hooks. The likely missing layer is the
Chrome/Electron PDF navigation stack: PDF navigation interception,
MimeHandlerView/component-extension infrastructure, and eventually PDF stream
routing. Do not continue Experiment 2 as another renderer-client-only patch.

### Experiment 2: Route Top-Level PDFs to a PDF Wrapper

#### Description

Experiment 1 proved that renderer-side plugin registration is not reached for a
top-level PDF navigation. The important log line was:

```text
Not implemented reached in content::ShellDownloadManagerDelegate::ChooseDownloadPath(...)
```

That means Content Shell classifies the top-level PDF as a download before Blink
creates a plugin element and before `TsRendererClient` can call
`pdf::CreateInternalPlugin()`.

This experiment should add the smallest browser-side top-level PDF route in
TermSurf's Chromium embedder:

1. Detect a main-frame `application/pdf` response in
   `TsBrowserClient::CreateThrottlesForNavigation()`.
2. Cancel that navigation before it becomes a download.
3. Navigate the same tab to a small generated HTML wrapper that embeds the
   original PDF URL with Chromium's internal PDF plugin MIME type.

The wrapper approach is deliberately a probe, not a full Chrome PDF viewer port.
It tests whether TermSurf can reach the already-added renderer plugin path
without first porting MimeHandlerView, GuestView, the PDF component extension,
or streams-private. If this works, it may be enough for basic in-pane PDF
viewing. If it fails, the logs should prove exactly which PDF viewer substrate
is still missing.

Important constraint: a `data:` wrapper has an opaque origin. It may not be able
to embed an HTTP PDF, and it is not expected to embed a `file://` PDF under
default Chromium policy. Therefore HTTP-only success is not a full Issue 776
Pass. Issue 776's original goal is `web file.pdf`; this experiment only Passes
if both the HTTP fixture and the `file://` fixture render. If HTTP renders but
`file://` does not, record Partial and design the next experiment around a
non-opaque wrapper origin or a real MimeHandlerView path.

The generated wrapper should be minimal and visible only for top-level PDF
documents. A representative wrapper is:

```html
<!doctype html>
<meta charset="utf-8" />
<style>
  html,
  body,
  embed {
    margin: 0;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: #f8d7da;
  }
</style>
<div id="termsurf-pdf-wrapper-marker">TermSurf PDF wrapper</div>
<embed src="{original_pdf_url}" type="application/x-google-chrome-pdf" />
```

The non-white background and marker are diagnostic scaffolding. They distinguish
"wrapper rendered but plugin is empty" from "wrapper did not render." The
experiment passes only if the automated screenshots show recognizable Bitcoin
PDF content for both HTTP and `file://`. Reaching `OverrideCreatePlugin()` or
`CreateInternalPlugin()` is useful evidence, but it is only a Partial result if
either screenshot remains blank or the `file://` case fails.

#### Changes

1. Create a new Chromium experiment branch from the current issue branch:
   `148.0.7778.97-issue-776-exp2`.
   - Do not commit directly to `148.0.7778.97-issue-776`.
   - Add the new branch to the Branches table in `chromium/README.md`.
   - Add a one-line note to `chromium/README.md` explaining that
     `{version}-issue-{N}-exp{M}` branches are allowed for follow-up Chromium
     experiments within an already-open issue.
   - Archive the branch under `chromium/patches/issue-776-exp2/` on Pass or
     Partial.

2. Add a browser-side PDF navigation throttle in `content/libtermsurf_chromium`,
   for example `ts_pdf_navigation_throttle.{cc,h}`.
   - Derive from `content::NavigationThrottle`.
   - Implement `WillProcessResponse()`.
   - Use `navigation_handle()->GetMimeType()` as the primary MIME signal because
     `file://` response headers may be synthetic or incomplete. Also log the
     MIME type reported by `GetResponseHeaders()` when headers exist, so the
     result can compare the two.
   - Only act when all of these are true:
     - the response MIME type is `pdf::kPDFMimeType` (`application/pdf`);
     - `navigation_handle()->IsInPrimaryMainFrame()` is true;
     - the target URL is not already the generated wrapper URL, as identified by
       a real sentinel rather than substring matching;
     - the URL scheme is ordinary browser content (`http`, `https`, or `file`)
       for this experiment.
   - Log every PDF decision with an `[issue-776-exp2]` prefix: URL, MIME type,
     main-frame status, and action.

3. When the throttle sees a top-level PDF response, cancel the current
   navigation and schedule a same-tab navigation to the generated wrapper.
   - Use `content::WebContents` from the navigation handle.
   - Preserve the existing tab; do not create a new window.
   - First probe a `data:text/html;charset=utf-8,...` URL for the wrapper,
     because it is the smallest implementation. Treat it as a probe, not the
     promised final architecture.
   - Include a deterministic wrapper sentinel, for example
     `termsurf-pdf-wrapper=1`, either in the generated URL or in a synthetic
     wrapper scheme. Use that sentinel for recursion prevention.
   - Store the original PDF URL in the wrapper's `<embed src=...>`.
   - Escape the original PDF URL for HTML attribute context before inserting it
     into the wrapper. URLs containing `&`, `"`, `'`, `<`, or `>` must not break
     the generated markup.
   - Set the `<embed>` type to `pdf::kInternalPluginMimeType`.
   - PostTask the wrapper navigation. Do not synchronously navigate the same
     `WebContents` from inside `WillProcessResponse()`, because the current
     navigation is still unwinding.
   - Return `content::NavigationThrottle::CANCEL_AND_IGNORE` after posting the
     wrapper navigation. Do not use `CANCEL`, `BLOCK_RESPONSE`, or an
     unspecified "closest" result.

4. Wire the throttle from `TsBrowserClient`.
   - Add a `CreateThrottlesForNavigation()` override to
     `ts_browser_client.{cc,h}`.
   - Call `ShellContentBrowserClient::CreateThrottlesForNavigation(registry)`
     first to preserve Content Shell behavior.
   - Then add the TermSurf PDF throttle.
   - Do not patch `content/shell/browser/shell_download_manager_delegate.cc` as
     the primary fix. That file should only be used as a diagnostic reference.

5. Keep Experiment 1's PDF plugin registration and renderer-client hooks.
   - Do not remove `TsContentClient`.
   - Do not remove `TsRendererClient`.
   - Keep the `[issue-776]` renderer/plugin logs for this experiment because
     they prove whether the wrapper reaches plugin creation.
   - Extend the renderer logs, if practical, so `OverrideCreatePlugin()` records
     the document URL/origin and PDF MIME type when it is asked to create a PDF
     plugin. This helps distinguish "plugin registered globally" from "plugin
     available in this document context."

6. Do not implement MimeHandlerView, GuestView, component-extension loading, or
   streams-private in this experiment.
   - If the wrapper reaches `CreateInternalPlugin()` but the screenshot is still
     blank, record Partial and design Experiment 3 around the missing substrate.
   - Do not silently widen this experiment into a partial Chrome PDF viewer
     port.
   - If the `data:` wrapper reaches the expected cross-origin or opaque-origin
     blocker, record Partial. Do not spend this experiment inventing a synthetic
     `ts-pdf://` or `chrome-untrusted://` wrapper unless the code is already
     trivial. That should be a separately designed experiment.

7. Harden the screenshot automation enough to trust the artifact.
   - Extend `scripts/test-issue-776-pdf.sh` so the log records the launched
     Wezboard PID and the command used for debug `web`.
   - Keep launching debug Wezboard, debug `web`, and repo-built Roamium only.
   - After foregrounding the process, capture the screenshot as before.
   - Record in the result whether the screenshot visibly shows the target URL
     `http://localhost:9616/bitcoin.pdf` or a generated PDF wrapper URL in the
     `web` URL bar. This does not replace the visual PDF-content check; it only
     proves the screenshot belongs to the intended run.

8. Build and run:
   - `autoninja -C out/Default libtermsurf_chromium`;
   - `./scripts/build.sh roamium`;
   - `./scripts/build.sh wezboard`;
   - `./scripts/build.sh webtui`;
   - `./scripts/test-issue-776-pdf.sh`.

9. Confirm whether the teardown crash seen in Experiment 1 recurs.
   - If Roamium crashes after the screenshot is captured, attach the crash stack
     or log excerpt to the result.
   - If Roamium crashes before the screenshot is captured, treat the experiment
     as Fail unless the logs still prove a clear next layer.

#### Non-Negotiable Invariants

- The fix remains Chromium/Roamium-side. Do not add PDF-specific logic to
  `webtui`.
- The existing browser pane is used. Do not open a native PDF window or an
  external app.
- The TermSurf protobuf protocol is unchanged.
- Normal HTML navigation, link clicks, scrolling, keyboard input, and local file
  navigation remain unchanged.
- The throttle only intercepts main-frame PDF responses. It must not hijack
  images, downloads, HTML pages, JavaScript resources, CSS, or arbitrary binary
  downloads.
- The implementation must not create a second tab or a second Chromium window.
- The generated wrapper must not recurse on itself.
- A `data:` wrapper is only a probe. HTTP-only rendering is Partial, not Pass,
  because the issue goal is local PDF rendering via `web file.pdf`.
- Experiment 2 must not quietly turn into a MimeHandlerView/component-extension
  port.

#### Verification

1. Confirm the branch and patch target:
   - Chromium branch: `148.0.7778.97-issue-776-exp2`;
   - eventual patch archive: `chromium/patches/issue-776-exp2/`.

2. Build the Chromium and Rust debug targets listed in the Changes section.

3. Run the automated PDF smoke test:

   ```bash
   ./scripts/test-issue-776-pdf.sh
   ```

4. Inspect the screenshot artifact.
   - Pass visual state: recognizable Bitcoin whitepaper content appears in the
     TermSurf browser pane.
   - Partial visual state: the page is still blank/white, but logs identify the
     next missing layer.
   - Partial visual state: the wrapper's diagnostic marker or non-white
     background appears, but no PDF content appears.
   - Fail visual state: the automation captured the wrong app/window, launched
     installed binaries, crashed before a screenshot, or regressed normal
     navigation.

5. Inspect logs and record these values in the result:

   | Layer                                                            | Result |
   | ---------------------------------------------------------------- | ------ |
   | `TsPdfNavigationThrottle` sees `application/pdf` main-frame URL  | yes/no |
   | Throttle cancels top-level PDF before download path              | yes/no |
   | `ShellDownloadManagerDelegate::ChooseDownloadPath` no longer hit | yes/no |
   | Generated wrapper navigation starts in the same tab              | yes/no |
   | Wrapper marker/background is visible in screenshot               | yes/no |
   | `<embed>` appears to be blocked by wrapper origin                | yes/no |
   | `OverrideCreatePlugin()` sees internal PDF MIME type             | yes/no |
   | `CreateInternalPlugin()` returns a plugin                        | yes/no |
   | Screenshot shows recognizable PDF content                        | yes/no |

6. If the HTTP fixture renders, run the same automation against the local file
   URL. This is required before recording Pass:

   ```bash
   ./scripts/test-issue-776-pdf.sh \
     file:///Users/ryan/dev/termsurf/test-html/public/bitcoin.pdf
   ```

   If the HTTP fixture renders but this `file://` fixture does not, record
   Partial. The original issue is not solved.

7. Run a normal HTML smoke test after the PDF test:
   - open `https://example.com`;
   - click a link if available;
   - type in a text field on a simple test page;
   - confirm no download prompt, blank page, or navigation loop occurs.

8. Run a non-PDF binary/download smoke test after the PDF test.
   - Serve or open a non-PDF binary fixture, such as a `.zip` or `.bin`, from
     the local test server.
   - Confirm the PDF throttle does not intercept it as a PDF.

#### Pass Criteria

The automated screenshot for both fixtures shows recognizable Bitcoin PDF
content rendered inside the existing TermSurf browser overlay:

- `http://localhost:9616/bitcoin.pdf`;
- `file:///Users/ryan/dev/termsurf/test-html/public/bitcoin.pdf`.

Normal HTML navigation still works, and a non-PDF binary/download fixture is not
intercepted by the PDF throttle.

#### Partial Criteria

The throttle prevents the Content Shell download path and reaches a later PDF
layer, but the screenshot still does not show PDF content. The result must name
the first missing layer, such as:

- HTTP fixture renders but `file://` does not render;
- wrapper navigation starts but `<embed>` does not create a plugin;
- `data:` wrapper origin blocks the PDF `<embed>`;
- `CreateInternalPlugin()` returns `nullptr`;
- plugin is created but the viewer surface is blank;
- plugin is created but PDF resource fetching is blocked by origin or file
  access;
- plugin is created but MimeHandlerView/component-extension/streams-private is
  required after all.

#### Failure Criteria

- The screenshot automation is not trustworthy enough to identify the visible
  app state.
- The implementation does not intercept top-level PDF navigation before the
  Content Shell download path.
- The implementation intercepts non-PDF resources or normal HTML pages.
- The implementation creates a second tab/window or opens an external PDF app.
- The implementation adds PDF-specific behavior to `webtui`.
- The generated wrapper loops indefinitely.
- Roamium crashes while loading the PDF, before teardown.
- Normal HTML navigation regresses.
