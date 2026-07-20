# libtermsurf_ladybird

This directory contains the C ABI wrapper that lets the Rust `ladybird`
profile-server binary call Ladybird's C++ embedding APIs.

The library has two build modes:

- `stub` (default): a standalone C stub that exports stable `ts_ladybird_*`
  symbols and keeps normal Ladybird builds cheap.
- `real`: a Ladybird-built C++ dylib that runs the first headless
  WebContent-backed lifecycle probe behind the C ABI.

## Intended Boundary

Later experiments should add a C ABI surface similar to the Chromium and WebKit
wrappers:

- process/runtime lifecycle;
- profile or website data store lifecycle;
- tab/view creation and destruction;
- navigation;
- resize and screen geometry;
- mouse, scroll, keyboard, focus, color-scheme, and GUI-active input;
- callbacks for tab readiness, render-surface identity, URL/title/loading state,
  cursor, target URL, JavaScript dialogs, console messages, HTTP auth, crashes,
  and shutdown;
- PDF capability/runtime asset checks (Ladybird pdf.js path in upstream Ladybird).

The Rust side should continue to own protobuf parsing, Unix-socket IPC,
profile-server process lifecycle, and protocol behavior. The C ABI should own
Ladybird-specific embedding and rendering details.

## Build

```bash
ladybird/libtermsurf_ladybird/build.sh
```

The output dylib is generated under `ladybird/libtermsurf_ladybird/build/`, which
is ignored by git.

Stub mode is the default. Real mode is opt-in because it requires the expensive
Ladybird checkout and build cache:

```bash
TERMSURF_LADYBIRD_BACKEND=real rust/ah-ladybirdd/libtermsurf_ladybird/build.sh
```

In real mode, `build.sh --clean` only removes the staged
`ladybird/libtermsurf_ladybird/build/` output. It does not remove
`forks/ladybird/Build/`.

## Exported Symbols

Both build modes export:

- `ts_ladybird_runtime_name`
- `ts_ladybird_runtime_version`
- `ts_ladybird_runtime_resource_root`
- `ts_ladybird_warmup`
- `ts_ladybird_initialize_runtime`
- `ts_ladybird_shutdown_runtime`
- `ts_ladybird_runtime_create`
- `ts_ladybird_runtime_destroy`
- `ts_ladybird_runtime_pump`
- `ts_ladybird_runtime_last_error`
- `ts_ladybird_view_create`
- `ts_ladybird_view_destroy`
- `ts_ladybird_view_load_url`
- `ts_ladybird_view_last_url`
- `ts_ladybird_view_did_finish_load`
- `ts_ladybird_view_did_crash`
- `ts_ladybird_view_navigation_action`
- `ts_ladybird_view_navigation_state`
- `ts_ladybird_view_render_surface_probe`

`ts_ladybird_runtime_name` returns `libtermsurf_ladybird-stub` specifically so
logs cannot be confused with a real embedded Ladybird runtime.

Real mode returns `libtermsurf_ladybird-ladybird`.

The runtime/view APIs are the first persistent ABI shape:

- one runtime handle per process, matching Astrohacker Terminal's
  one-profile-per-process engine model;
- one or more view handles owned by that runtime;
- explicit view destroy before runtime destroy;
- prompt event-loop pumping through `ts_ladybird_runtime_pump`;
- deterministic error reporting through `ts_ladybird_runtime_last_error`.

`ts_ladybird_initialize_runtime` and `ts_ladybird_shutdown_runtime` remain
exported for ABI continuity during the transition. New code should use
`ts_ladybird_runtime_create` and `ts_ladybird_runtime_destroy`.

The current warmup is still a lifecycle probe only. It creates a runtime,
creates a headless view, loads a deterministic `data:` URL, pumps until the load
callback, and destroys the handles. It is not visible rendering or full
TermSurf protocol parity.

## Semantic Navigation

Both backends expose a semantic Back/Forward history ABI. The real backend
accepts `ts_ladybird_view_navigation_action` with `"back"` or `"forward"` only
when Ladybird's corresponding native action is enabled and its session-history
traversal reports a started operation. `ts_ladybird_view_navigation_state`
reports native `can_go_back` and `can_go_forward` truth and fails closed after a
renderer crash. The stub preserves the ABI shape but always reports both
directions unavailable; it is not navigation proof.

The deterministic real-backend Forward smoke builds the issue branch, creates
two simultaneous Ladybird views, and proves a Back/Forward history round trip,
fresh-navigation clearing, same-document history, disabled and future-action
rejection, tab isolation, crash/recovery, and cleanup:

```bash
rust/ah-ladybirdd/libtermsurf_ladybird/smoke-test/run-forward-action-smoke.sh
```

Its fixture is local to this wrapper and has no Gecko dependency.

## Render-Surface Probe

Issue 26070112000884 Experiment 12 adds `ts_ladybird_view_render_surface_probe`. In stub
mode it validates the call and reports an explicit unsupported result: no
surface, no exportability, zero dimensions, and generation zero.

In real Ladybird-backed mode, `LibTermSurfLadybird` uses an inline
TermSurf-protocol-owned `HeadlessWebView` subclass to reach the protected
backing-store state that Ladybird's AppKit bridge also uses. The probe reports
whether the current view has a `Gfx::SharedImageBuffer`, the pixel dimensions of
that buffer, whether `on_ready_to_paint` has fired, whether
`m_client_state.has_usable_bitmap` is true, and a simple generation counter.

`can_export_shared_image=true` means a non-null buffer existed and
`Gfx::SharedImageBuffer::export_shared_image()` completed on the ABI owner
thread. It is a render-surface reachability signal, not proof that Ghostboard
has imported the Mach port or presented pixels. A later protocol/Ghostboard
experiment must validate the cross-process IOSurface/Mach-port transfer and
display path.

## Transition Plan

The next ABI step is to replace stub internals incrementally:

1. Prove Ladybird build/link consumption with a `HeadlessWebView` probe. Done in
   Issue 26070112000884 Experiment 5 with the in-tree `TestLadybirdHeadlessLifecycle`
   target.
2. Prove a live WebContent-backed `HeadlessWebView` under `Core::EventLoop`.
   Done in Issue 26070112000884 Experiment 5.
3. Prove deterministic headless navigation callbacks. Done in Issue 26070112000884
   Experiment 5 with a `data:` URL load-finish callback.
4. Move that deterministic lifecycle path behind the C ABI. Done in Issue 26070112000884
   Experiment 6 with the real `TermSurfLadybird` Ladybird CMake target.
5. Convert the one-shot lifecycle proof into persistent runtime and view
   handles. Done in Issue 26070112000884 Experiment 7.
6. Design a TermSurf protocol-specific `ViewImplementation` subclass that can
   expose a presentable backing store or IOSurface for Ghostboard. Done in Issue
   884 Experiment 12 through ABI reachability only.
7. Wire navigation, callbacks, input, PDF checks, and protocol behavior one
   message group at a time.

`HeadlessWebView` is a lifecycle probe target, not the final rendering strategy.
Ladybird's AppKit path uses a bridge with `paintable()` and CAMetalLayer
presentation; Astrohacker Terminal will need an equivalent presentable surface
boundary.
