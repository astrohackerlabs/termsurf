+++
status = "open"
opened = "2026-03-16"
+++

# Issue 756: Surfari — WebKit engine for TermSurf

## Goal

Build Surfari, the WebKit-based browser engine for TermSurf. Surfari proves that
the TermSurf protocol is truly engine-agnostic by running a second browser
engine alongside Roamium (Chromium) in the same terminal window.

## Background

### Why WebKit first

TermSurf claims to be a protocol that works with any engine, but it currently
only runs Chromium. Shipping a second engine proves the architecture. WebKit is
the best candidate because:

- **Smallest embedding surface.** WebKit's embedding API is simpler than
  Chromium's Content API.
- **Prior art.** TermSurf 1.x (ts1) already implemented a full WKWebView
  integration with Ghostty, including navigation, DevTools, profiles, downloads,
  and console capture (Issue 108). We know the API well.
- **Cross-platform.** WebKit builds on macOS, Linux (WebKitGTK, WPE), and can be
  built on Windows. This matters for TermSurf's cross-platform goals.

### Why we must build WebKit from source

TermSurf 1.x used the system `WebKit.framework` (WKWebView) on macOS. This
worked for a prototype but hit serious limitations documented across Issues
100–108:

**macOS-only.** WKWebView is an Apple framework. It does not exist on Linux or
Windows. TermSurf needs to run on all three platforms. Building WebKit from
source gives us WebKitGTK on Linux and a custom build on Windows.

**No console capture API.** WKWebView has no native API for capturing console
output (Issue 102). We had to inject JavaScript at document start to override
`console.log`, `console.error`, `console.warn`, and `console.info`, then
serialize objects with `JSON.stringify()`. No structured console API exists.

**Broken HTTP headers.** WKWebView does not send the
`Upgrade-Insecure-Requests: 1` header that Safari sends (Issue 108). Google and
other sites serve simplified/mobile layouts as a result. Apple has an open radar
(rdar://50057283) with no fix. The workaround — intercepting navigation and
injecting the header manually — only works for top-level requests, not XHR/fetch
or subresources.

**No native target="\_blank" handling.** WKWebView drops links requesting new
windows (Issue 107). The workaround loads them in the same webview, but loses
the original page context. OAuth flows relying on popups do not work.

**Private API dependency for DevTools.** Enabling Web Inspector requires
`setValue(true, forKey: "developerExtrasEnabled")` — an undocumented private
configuration key (Issue 108). The `_WKInspector` API uses a leading underscore,
indicating it is not public.

**Key event interception hacks.** WKWebView captures all key events when focused
(Issue 104). We had to use `NSEvent.addLocalMonitorForEvents` to intercept
Ctrl+C globally before WKWebView sees it. WKWebView's `performKeyEquivalent`
claims cmd+c/x/v (returns `true`) but does not actually execute the operations —
we had to convert to `NSApp.sendAction` to trigger menu actions.

**No process-level control.** The system framework runs WebContent processes
that we cannot configure, inspect, or control. Building from source gives us
access to `WebPageProxy`, `WebProcessPool`, compositor internals, and the
ability to add our own C API hooks.

Building WebKit from source eliminates all of these limitations. We get full
control over the engine, the same way we have full control over Chromium via our
fork.

### Architecture

Surfari follows the same pattern as Roamium:

```
libtermsurf_webkit (Objective-C/C++, C API)
        ↓ links
    Surfari (Rust binary, ~400 lines)
        ↓ connects via Unix socket
    Ghostboard (GUI)
```

- `libtermsurf_webkit` — Objective-C/C++ library wrapping WebKit's internal
  APIs, exporting C functions with `ts_*` signatures matching the same API as
  `libtermsurf_chromium`.
- Surfari — Rust binary handling Unix socket IPC, protobuf parsing, and process
  lifecycle. Almost entirely reusable from Roamium.

### Guiding strategy

Surfari should keep the same process architecture as Roamium, but WebKit has a
different natural embedding seam than Chromium. Chromium gave us `content_shell`
as the practical proof that an embedder can own the browser process and expose a
surface. For WebKit, the equivalent starting point is MiniBrowser, especially
the macOS WebKit2 implementation under `Tools/MiniBrowser/mac`.

The production shape should be:

```
WebKit MiniBrowser/WKWebView/WebKit2 embedding
        ↓ wrapped by
libtermsurf_webkit (Objective-C++ implementation, C ABI)
        ↓ called by
Surfari (Rust process, shared Roamium-style IPC/protobuf lifecycle)
```

The C ABI should look as much like `libtermsurf_chromium` as possible:
initialize the engine, create and destroy browser views, navigate, resize,
focus, forward mouse/keyboard/wheel input, report state changes, expose
compositing handles, and shut down cleanly. Internally, the macOS implementation
will be Objective-C++/Cocoa because the supported WebKit embedding API is
`WKWebView`.

MiniBrowser is the WebKit analogue to Chromium `content_shell`: it shows the
smallest supported browser embedder. WebKitTestRunner is the automation and test
harness reference: it shows how WebKit injects input, toggles runtime flags, and
drives browser behavior under test. WebKitGTK and WPE are cross-platform
embedding references, but they are not the primary macOS implementation path.

Putting `WKWebView` directly inside Ghostboard is useful only as a temporary
prototype if we need to validate terminal overlay behavior quickly. It is not
the production Surfari architecture because it collapses the engine into the GUI
process and breaks the one-engine-process-per-profile model that keeps TermSurf
uniform across Chromium, WebKit, Gecko, and future engines.

The first deep WebKit question is compositor export, not basic embedding.
Surfari should first try to use WebKit's existing `RemoteLayerTree`,
`LayerHostingContext`, and `HostingContext` machinery to expose a renderable
surface to Ghostboard. Only if that path cannot satisfy the TermSurf compositor
contract should we patch lower-level WebKit internals.

### End-to-end completion checklist

This checklist tracks the full Surfari strategy from proof-of-concept through
production TermSurf integration. Items must be checked off as they are finished,
and the experiment that proves each item should be linked or referenced in the
surrounding issue text.

- [x] Shallow clone WebKit into `webkit/src` and prove the checkout builds on
      macOS.
- [x] Prove WebKit content can be hosted outside its original WebKit process
      boundary by exporting a WebKit render surface or hosting context and
      displaying it in a separate host window/process.
- [x] Confirm the hosted WebKit surface resizes correctly, animates, scrolls,
      survives navigation, and remains stable across repeated show/hide cycles.
- [x] Establish WebKit branch and patch management analogous to Chromium:
      issue-specific branches, documented upstream commit ancestry, build
      commands, and a clear record of each TermSurf patch.
- [x] Create `libtermsurf_webkit` with a C ABI backed by Objective-C++/Cocoa on
      macOS.
- [ ] Implement the core `libtermsurf_webkit` API: initialize, shutdown, create
      and destroy browser views, navigate, reload, stop, resize, focus, forward
      mouse/keyboard/wheel input, report browser state, and expose compositing
      handles.
- [x] Create the Surfari Rust binary by reusing the Roamium-style Unix socket,
      protobuf dispatch, process lifecycle, and profile management code
      (Experiment 15).
- [x] Run Surfari outside Ghostboard with a small test driver or harness and
      prove the Rust process can drive WebKit through `libtermsurf_webkit`
      (Experiment 16).
- [x] Audit Surfari against Roamium and the existing TermSurf protobuf messages;
      mark every message supported, unsupported, or requiring a protocol
      extension (Experiment 17).
- [ ] Modify `termsurf.proto` only where WebKit exposes a real browser
      capability that the current protocol cannot express.
- [ ] Integrate Surfari with Ghostboard engine launching, profile selection,
      socket routing, and overlay hosting.
- [ ] Test Surfari inside the real TermSurf app with navigation, keyboard input,
      click, drag, scroll, resize, pane resize, split panes, tab switching,
      window switching, focus changes, shutdown, restart, profile isolation, and
      crash handling.
- [ ] Add focused regression guards for behavior that is proven to work, keeping
      the tests small enough that the suite remains practical to run.
- [ ] Re-run the full Ghostboard/Roamium feature matrix against
      Ghostboard/Surfari and document any engine-specific differences.

### Compositing: the key open question

Chromium uses CALayerHost for zero-copy GPU compositing. A `CAContext` ID
crosses the process boundary over a Unix socket, and Ghostboard creates a
`CALayerHost` with that ID. This is the core rendering mechanism.

WebKit's rendering is tied to its view hierarchy. The key question: how do we
get WebKit's rendered content into Ghostboard's window across the process
boundary?

**Approach A: Reparent the WKWebView into Ghostboard's window.** The Surfari
process creates the WKWebView, then the GUI reparents it into the terminal
window as a subview. macOS does not support cross-process NSView reparenting.
**Not viable.**

**Approach B: Create the WKWebView in the GUI process.** Ghostboard creates the
WKWebView directly. Surfari becomes a thin control process. This breaks the
architectural principle that the engine process owns its rendering surface. It
also would not work cross-platform — on Linux there is no NSView, so the engine
must own its rendering surface. **Architecturally wrong.**

**Approach C: Extract a CAContext from WebKit's layer tree.** WebKit has an
internal layer tree. We create a `CAContext` from the root layer and export the
`contextId` to Ghostboard, just like Chromium. This requires a small
modification to WebKit (~20 lines in `LayerHostingContext.mm` or similar).
**Architecturally consistent with Roamium.** Same CALayerHost pattern, same
process model, same Ghostboard code path. Requires building WebKit from source
(which we already need for the reasons above).

**Approach D: Off-screen rendering.** Render into a bitmap and send pixels. This
is the CEF approach we already rejected for performance. **Not viable.**

Approach C is the recommended path. It keeps the architecture uniform across all
engines and reuses the existing Ghostboard compositing code.

### Multi-profile

WebKit supports multiple profiles in one process via `WKWebsiteDataStore`.
However, TermSurf uses one-process-per-profile for architectural uniformity
across all engines (Chromium requires it; Gecko and Ladybird require it).
Surfari will follow the same model.

```objc
WKWebsiteDataStore *store = [WKWebsiteDataStore
    dataStoreForIdentifier:[[NSUUID alloc] init]];
config.websiteDataStore = store;
```

### Input handling

If using Approach C (separate process), input events are forwarded over the Unix
socket just like Roamium. WebKit's `WebPageProxy` has methods for injecting
events: `handleMouseEvent`, `handleWheelEvent`, `handleKeyboardEvent`.

### DevTools

WebKit uses Web Inspector, not Chrome DevTools. With a source build, we have
full access to the inspector APIs without private API hacks:

```objc
[webView._inspector show];
[webView._inspector attachRight];
```

### Prior work from ts1

Issue 108 documented a full WKWebView integration in TermSurf 1.x including:
navigation delegates, download handling, console capture via JS injection,
profile isolation, DevTools, HTTP auth, dialogs, file uploads, and crash
recovery. Much of this code can inform Surfari's implementation.

### What needs to happen

1. Build WebKit from source on macOS (research build system, dependencies)
2. Determine the exact compositing approach — find where in WebKit's source to
   extract a CAContext ID (Approach C)
3. Create `libtermsurf_webkit` (C library with `ts_*` API)
4. Create the Surfari Rust binary (fork from Roamium, adapt)
5. Update Ghostboard to launch Surfari processes for WebKit profiles
6. Update the `web` TUI to support `--browser webkit` (or similar)
7. Test with all existing protocol messages

## Experiments

- [Experiment 1: Shallow clone and build WebKit](01-shallow-clone-and-build-webkit.md)
  — **Pass**
- [Experiment 2: Prove WebKit hosting context export](02-webkit-hosting-context-proof.md)
  — **Pass**
- [Experiment 3: Stress hosted WebKit surface lifecycle](03-hosted-surface-lifecycle.md)
  — **Pass**
- [Experiment 4: Establish WebKit branch workflow](04-webkit-branch-workflow.md)
  — **Pass**
- [Experiment 5: Create initial libtermsurf_webkit ABI](05-initial-libtermsurf-webkit-abi.md)
  — **Pass**
- [Experiment 6: Implement core WebKit input API](06-core-webkit-input-api.md) —
  **Partial**
- [Experiment 7: Resolve WebKit focus semantics](07-webkit-focus-semantics.md) —
  **Pass**
- [Experiment 8: Implement WebKit browser state callbacks](08-webkit-browser-state-callbacks.md)
  — **Partial**
- [Experiment 9: Implement WebKit HTTP auth callbacks](09-webkit-http-auth.md) —
  **Pass**
- [Experiment 10: Implement WebKit target URL hover callbacks](10-webkit-target-url-hover.md)
  — **Pass**
- [Experiment 11: Implement WebKit cursor callbacks](11-webkit-cursor-callbacks.md)
  — **Partial**
- [Experiment 12: Hook WebKit cursor changes](12-webkit-cursor-hook.md) —
  **Pass**
- [Experiment 13: Implement WebKit console messages](13-webkit-console-messages.md)
  — **Pass**
- [Experiment 14: Implement WebKit renderer crash callbacks](14-webkit-renderer-crash.md)
  — **Pass**
- [Experiment 15: Stand up the Surfari Rust binary](15-surfari-rust-binary.md) —
  **Pass**
- [Experiment 16: Prove Surfari fake-GUI IPC](16-surfari-fake-gui-ipc.md) —
  **Pass**
- [Experiment 17: Audit Surfari protocol parity](17-surfari-protocol-parity-audit.md)
  — **Pass**
- [Experiment 18: Implement Surfari DevTools Path](18-surfari-devtools-path.md)
  — **Pass**
