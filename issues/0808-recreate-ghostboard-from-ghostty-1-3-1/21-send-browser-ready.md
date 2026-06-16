# Experiment 21: Send BrowserReady After TabReady

## Description

Experiment 20 gave Ghostboard a real browser listen socket path when it spawns
an absolute browser executable for `SetOverlay`. Experiments 15 and 16 already
send `CreateTab` to a browser connection and record `TabReady`. The next parity
step is to notify the originating TUI that its browser tab is ready.

Wezboard sends `BrowserReady` after `TabReady` for a normal browser pane. The
message contains:

- `pane_id` from `TabReady`;
- `tab_id` from `TabReady`;
- `browser_socket` from the matched server's listen socket;
- `browser` from the pane's browser spec.

This experiment will implement that same state-backed notification in
Ghostboard. It must use the real listen socket stored in Experiment 20; it must
not fabricate a socket path, use the GUI socket as the browser socket, or send
`BrowserReady` before `TabReady`.

## Changes

- `ghostboard/src/apprt/termsurf.zig`
  - extend `PaneState` with the originating TUI fd from the `SetOverlay`
    connection;
  - pass the current client fd into `handleSetOverlay`;
  - preserve that TUI fd when updating an existing pane;
  - after `TabReady` records `tab_id` and lookup state, find the pane's server;
  - if the server has a nonempty listen socket and the pane has a live TUI fd,
    snapshot the pane id, tab id, browser, listen socket, and TUI fd under
    `state_mutex`;
  - release `state_mutex` before writing the length-prefixed `BrowserReady`
    protobuf to the TUI fd;
  - log the successful `BrowserReady` send with pane id, tab id, socket, and
    browser;
  - leave browser direct-client routing, overlay presentation, navigation, and
    input forwarding out of scope.

No changes will be made to `webtui`, `roamium`, `proto/termsurf.proto`,
branding, app config paths, icon assets, Xcode project files, CLI install
behavior, direct browser-client routing, CALayerHost overlay presentation,
navigation forwarding, or input forwarding.

## Verification

Pass criteria:

- `zig fmt src/apprt/termsurf.zig src/main_c.zig src/build/SharedDeps.zig`
  passes inside `ghostboard/`.
- The native GhosttyKit framework build passes:
  `zig build -Demit-xcframework=true -Dxcframework-target=native -Demit-macos-app=false`.
- The macOS app build passes:
  `macos/build.nu --scheme Ghostty --configuration Debug --action build`.
- Runtime harness launches `TermSurf.app`, connects to `TERMSURF_SOCKET`, and
  sends `SetOverlay(browser=/absolute/temp/helper, profile=default)` from a TUI
  socket.
- The spawned helper connects back with `ServerRegister`, receives `CreateTab`,
  and sends `TabReady(pane-a, 42)`.
- The TUI socket receives `BrowserReady` after `TabReady`.
- The decoded `BrowserReady` has:
  - `pane_id = "pane-a"`;
  - `tab_id = 42`;
  - `browser_socket` equal to the `--listen-socket` argument passed to the
    helper;
  - `browser` equal to the absolute helper browser spec used by `SetOverlay`.
- App logs include
  `BrowserReady: pane_id=pane-a tab_id=42 socket=... browser=...`.
- The runtime harness also sends a normal TUI `HelloRequest` on a fresh socket
  and receives `HelloReply`, proving existing request/reply behavior still
  works.
- The harness verifies shutdown cleanup still removes the socket file and leaves
  no stale `TermSurf.app/Contents/MacOS/termsurf` process.
- `git diff --check` is clean.

Fail criteria:

- `BrowserReady` is sent before `TabReady`.
- `BrowserReady.browser_socket` is empty, fabricated, or equal to the GUI
  socket.
- `BrowserReady` is sent to the browser socket instead of the originating TUI
  socket.
- `BrowserReady` contains the wrong pane id, tab id, browser socket, or browser
  value.
- Existing `SetOverlay -> spawn -> ServerRegister -> CreateTab -> TabReady`
  behavior regresses.
- The implementation adds direct browser-client routing, CALayerHost overlay
  presentation, navigation forwarding, input forwarding, or changes `webtui`,
  `roamium`, or the protocol schema in this experiment.

## Design Review

Fresh-context adversarial design review returned **APPROVED** with no required
findings.

Optional finding accepted and fixed: the design now requires snapshotting the
`BrowserReady` fields under `state_mutex`, then releasing the lock before
writing to the TUI fd.

Nit accepted and fixed: the expected app log check now includes the browser
value because the design requires logging it.
