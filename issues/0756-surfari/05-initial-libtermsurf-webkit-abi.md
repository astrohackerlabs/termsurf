# Experiment 5: Create initial libtermsurf_webkit ABI

## Description

Experiments 1-4 proved that WebKit builds locally, that WebKit content can be
hosted across a process boundary, that the hosted surface survives key lifecycle
events, and that WebKit source changes have a branch/patch workflow. The next
step is to turn the proof harness into the first production-shaped library
boundary: `libtermsurf_webkit`.

This experiment should create a buildable macOS `libtermsurf_webkit` C ABI
scaffold backed by Objective-C++/Cocoa. It should prove that a C caller can
initialize the library, create a browser context, create a WebKit-backed browser
view, receive the important callbacks, navigate, resize, destroy the view, and
shut down cleanly.

The scope is intentionally smaller than full Surfari. This experiment should not
create the Surfari Rust binary, modify Ghostboard, modify `termsurf.proto`,
implement every input path, or patch WebKit source. It should establish the
library shape and prove the first working browser-view lifecycle through the C
ABI.

## Changes

- Create a new tracked library directory, likely `surfari/libtermsurf_webkit/`,
  with:
  - a public C header declaring opaque `ts_browser_context_t` and
    `ts_web_contents_t` handles;
  - a public C ABI compatible with the current Roamium FFI names in
    `roamium/src/ffi.rs`;
  - an Objective-C++ implementation that owns Cocoa/WebKit objects behind opaque
    C handles;
  - a local build script for macOS development;
  - a smoke-test executable or harness that calls the C ABI directly.
- Link the library and smoke test against the locally source-built WebKit
  products under `webkit/src/WebKitBuild/Debug`, not accidentally against only
  `/System/Library/Frameworks/WebKit.framework`.
- Implement the initial working subset:
  - callback registration for `ts_set_on_initialized`, `ts_set_on_tab_ready`,
    `ts_set_on_ca_context_id`, `ts_set_on_url_changed`,
    `ts_set_on_loading_state`, and `ts_set_on_title_changed`;
  - exact export of `ts_content_main`, which initializes Cocoa on the main
    thread and fires the initialized callback;
  - `ts_post_task`;
  - `ts_quit`;
  - `ts_create_browser_context`;
  - `ts_create_incognito_browser_context`;
  - `ts_destroy_browser_context`;
  - `ts_create_web_contents`;
  - `ts_destroy_web_contents`;
  - `ts_load_url`;
  - `ts_set_view_size`.
- Export the remaining Roamium-compatible symbols as explicit unsupported stubs
  only if needed to make the ABI complete for a future Surfari link. Any
  unsupported stub must be documented in the experiment result and should not be
  claimed as implemented behavior.
- Reuse the proven compositor hook from the proof harness: create a `CAContext`,
  assign the `WKWebView` layer, and fire `ts_set_on_ca_context_id` with the
  exported context ID and current pixel size.
- Add deterministic local test content for the smoke test if the existing
  `surfari-proofs/hosting-context/test-content/` files are not suitable.
- Update `webkit/README.md` or a new Surfari README only if needed to document
  how to build the initial library.
- Do not modify `webkit/src` in this experiment. If the first ABI slice needs a
  WebKit source patch, record **Partial**, archive the reason, and design the
  next experiment around that patch.

## Verification

Start from a clean TermSurf repo root:

```bash
git status --short
git -C webkit/src status --short
git -C webkit/src rev-parse HEAD
git -C webkit/src rev-parse --abbrev-ref HEAD
git -C webkit/src rev-parse --is-shallow-repository
find webkit/src/WebKitBuild/Debug -maxdepth 2 \
  \( -name 'WebKit.framework' -o -name 'JavaScriptCore.framework' \) -print
```

Then build the library and smoke test with the new documented command, expected
to be something like:

```bash
surfari/libtermsurf_webkit/build.sh
```

The build must produce a dynamic library named `libtermsurf_webkit.dylib` and a
smoke-test binary. Verify exported symbols:

```bash
nm -gU surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib | rg ' _ts_'
```

Run the smoke test with logs in the repo `logs/` directory. The exact command
must be recorded in the result, but it should prove:

- the library initializes and fires `ts_set_on_initialized`;
- a persistent and an incognito browser context can be created and destroyed;
- a WebKit-backed web contents can be created through `ts_create_web_contents`;
- `ts_set_on_tab_ready` fires with a nonzero tab ID;
- `ts_set_on_ca_context_id` fires with a nonzero context ID and the expected
  size;
- `ts_set_on_loading_state` reports loading transitions;
- `ts_set_on_url_changed` reports the loaded URL;
- `ts_set_on_title_changed` reports the page title;
- `ts_load_url` can navigate from one deterministic local page to another;
- `ts_set_view_size` resizes the `WKWebView` and causes the exported context
  callback or smoke-test observation to reflect the new size;
- `ts_destroy_web_contents`, `ts_destroy_browser_context`, and `ts_quit` shut
  down cleanly;
- `webkit/src` remains clean and on `webkit-1452a439-issue-756`.

Use `otool` or an equivalent check to verify the library and smoke-test link
paths use the local WebKit build products under `webkit/src/WebKitBuild/Debug`:

```bash
otool -L surfari/libtermsurf_webkit/build/libtermsurf_webkit.dylib | rg 'WebKit|JavaScriptCore'
otool -L surfari/libtermsurf_webkit/build/smoke-test | rg 'WebKit|JavaScriptCore|libtermsurf_webkit'
```

**Pass** = the library and smoke test build, the dynamic library exports the
expected `ts_*` symbols, the smoke test proves the initial WebKit browser-view
lifecycle through the C ABI, the binary links against the local source-built
WebKit products, unsupported stubs are explicitly documented, and `webkit/src`
remains unchanged.

**Partial** = the library builds but one or more required lifecycle callbacks,
source-built WebKit linkage checks, resize behavior, or shutdown behavior is
missing. The result must identify the exact missing behavior and the next
experiment needed.

**Fail** = the initial library cannot be built or cannot create a usable WebKit
view through the C ABI.

Before recording the result, capture:

```bash
git status --short
git -C webkit/src status --short
git -C webkit/src rev-parse HEAD
git -C webkit/src rev-parse --abbrev-ref HEAD
git -C webkit/src rev-parse --is-shallow-repository
```

The TermSurf worktree must contain only the intended library, harness, docs, and
issue changes plus ignored `logs/` and build output.

## Design Review

An adversarial Codex subagent reviewed the design with fresh context.

**Verdict:** Approved.

Required findings: none.

Optional findings accepted and fixed:

- The design originally allowed "`ts_content_main` or equivalent"; this was
  tightened to require the exact `ts_content_main` export for Roamium ABI
  compatibility.
- The local WebKit linkage check originally showed only the dylib; it now also
  checks the smoke-test binary.
