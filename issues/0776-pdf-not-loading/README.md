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
capture and agent visual classification. It should add the smallest TermSurf
`ContentClient` / renderer-client hooks needed to make Chromium's built-in PDF
path available, then run TermSurf end-to-end and take a screenshot of a real PDF
loaded in the browser pane.

The primary question for this experiment is user-visible:

```text
Does the PDF visibly render in the TermSurf browser pane?
```

The experiment should not require manual app interaction for verification. It
will capture screenshots automatically, then the agent will inspect those
artifacts. If a screenshot shows recognizable PDF content, the experiment
passes. If the screenshot is still blank or white, the result should record the
artifact and only then use logs to decide the next experiment.

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
   - Check whether `libtermsurf_chromium`'s `testonly = true` blocks the
     proposed deps. Run `gn check out/Default //content/libtermsurf_chromium`.
     If a needed PDF dep is rejected because it is not test-only-compatible,
     stop and record that as the result; do not silently remove
     `testonly = true` without a separate design.
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
   - If implementing the override requires broad extensions infrastructure,
     record that explicitly in the result instead of silently widening scope.
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

    The automation may use macOS GUI automation, such as AppleScript/System
    Events, to type the debug `web` command into the newly launched Wezboard
    window. This is acceptable for this experiment because the target is an
    end-to-end visual smoke test of the actual GUI path.

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
- The Chromium changes must live on the `148.0.7778.97-issue-776` branch and be
  archived under `chromium/patches/issue-776/`.

#### Verification

This experiment's verification uses automated capture plus agent visual
classification. Manual inspection by the user is not required to decide whether
the experiment passes.

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
   - whether `testonly = true` blocks the minimal PDF deps;
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
   | `IsPluginHandledExternally()` sees top-level PDF            | yes/no/not-available   |
   | Plugin created but viewer resources/MimeHandlerView missing | yes/no                 |
   | Evidence that MimeHandlerView/extension layer is next       | yes/no                 |
   | Evidence that PDF stream routing is next                    | yes/no                 |
   | Evidence that plugin/utility sandboxing is next             | yes/no                 |

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
