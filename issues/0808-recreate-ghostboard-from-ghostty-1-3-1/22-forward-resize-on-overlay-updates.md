# Experiment 22: Forward Resize on Overlay Updates

## Description

Experiment 21 completed the first normal browser readiness path:
`SetOverlay -> spawn browser -> ServerRegister -> CreateTab -> TabReady -> BrowserReady`.
The next small parity step is to keep the browser surface dimensions in sync
when the TUI updates an existing overlay.

Wezboard treats a repeated `SetOverlay` for an existing pane as a resize/update
path. It updates pane geometry and, once the pane has a browser `tab_id`, sends
`Resize` to the matched browser server with the tab id and pixel dimensions.
Fresh Ghostboard currently updates pane geometry but returns without notifying
the browser. That leaves Roamium with stale surface dimensions after terminal
layout changes.

This experiment will implement only that state-backed resize forwarding path. It
will not create native overlay UI, attach CALayerHost, forward input, route
navigation, create DevTools panes, or change `webtui`, `roamium`, or the
protocol schema.

## Changes

- `ghostboard/src/apprt/termsurf.zig`
  - add a small snapshot type for a pending `Resize` message containing browser
    fd, tab id, and pixel width/height;
  - when `handleSetOverlay` updates an existing pane, preserve the latest TUI fd
    and pane geometry as it does today;
  - if that existing pane has a nonzero `tab_id` and its matched server has an
    attached browser fd, snapshot the resize fields under `state_mutex`;
  - release `state_mutex` before sending the length-prefixed `Resize` protobuf
    to the browser fd;
  - compute pixel dimensions from the updated grid dimensions using the same
    fallback cell metrics currently used for `CreateTab`;
  - set the current screen fields to `0.0`, matching Wezboard's existing resize
    fallback path until Ghostboard has real overlay/window metric integration;
  - log successful resize sends with pane id, tab id, and pixel dimensions;
  - keep new-pane `SetOverlay`, browser spawning, `CreateTab`, `TabReady`,
    `BrowserReady`, native overlay presentation, navigation forwarding, and
    input forwarding otherwise unchanged.

No changes will be made to `webtui`, `roamium`, `proto/termsurf.proto`,
branding, app config paths, icon assets, Xcode project files, CLI install
behavior, DevTools pane creation, CALayerHost overlay presentation, navigation
forwarding, or input forwarding.

## Verification

Pass criteria:

- `zig fmt src/apprt/termsurf.zig src/main_c.zig src/build/SharedDeps.zig`
  passes inside `ghostboard/`.
- The native GhosttyKit framework build passes:
  `zig build -Demit-xcframework=true -Dxcframework-target=native -Demit-macos-app=false`.
- The macOS app build passes:
  `macos/build.nu --scheme Ghostty --configuration Debug --action build`.
- Runtime harness launches `TermSurf.app`, connects to `TERMSURF_SOCKET`, and
  sends
  `SetOverlay(browser=/absolute/temp/helper, profile=default, pane_id=pane-a, width=80, height=24)`
  from a TUI socket.
- The spawned helper connects back with `ServerRegister`, receives `CreateTab`,
  and sends `TabReady(pane-a, 42)`.
- The TUI socket receives `BrowserReady` with the real helper listen socket,
  proving the Experiment 21 path still works.
- The harness sends a second `SetOverlay` for the same `pane_id=pane-a` with a
  different `width` and `height`.
- The helper receives a `Resize` message after the second `SetOverlay`.
- The decoded `Resize` has:
  - `tab_id = 42`;
  - `pixel_width = updated width * fallback_cell_width`;
  - `pixel_height = updated height * fallback_cell_height`;
  - `screen_x = 0.0`;
  - `screen_y = 0.0`;
  - `screen_width = 0.0`;
  - `screen_height = 0.0`;
  - `screen_scale = 0.0`.
- A repeated `SetOverlay` before `TabReady` does not send `Resize`, because the
  pane does not yet have a browser tab id.
- App logs include `Resize: pane_id=pane-a tab_id=42 pixel=...x...`.
- The runtime harness still verifies shutdown cleanup removes the socket file
  and leaves no stale `TermSurf.app/Contents/MacOS/termsurf` process.
- `git diff --check` is clean.

Fail criteria:

- New-pane `SetOverlay` sends `Resize` before `CreateTab`/`TabReady`.
- A repeated `SetOverlay` before `TabReady` sends `Resize`.
- The `Resize` is sent to the TUI socket instead of the browser server socket.
- The `Resize` has the wrong tab id or stale pixel dimensions.
- Duplicate `SetOverlay` creates a duplicate pane or duplicate server.
- Existing
  `SetOverlay -> spawn -> ServerRegister -> CreateTab -> TabReady -> BrowserReady`
  behavior regresses.
- The implementation adds CALayerHost overlay presentation, navigation
  forwarding, input forwarding, DevTools pane creation, or changes `webtui`,
  `roamium`, or the protocol schema in this experiment.

## Design Review

Fresh-context adversarial design review returned **APPROVED** with no required,
optional, or nit findings.

## Result

**Result:** Pass

Implemented state-backed `Resize` forwarding for repeated `SetOverlay` updates
in `ghostboard/src/apprt/termsurf.zig`.

The implementation now:

- snapshots a browser fd, pane id, tab id, and updated pixel dimensions when a
  repeated `SetOverlay` updates an existing pane;
- skips resize forwarding until the pane has a nonzero browser `tab_id`;
- skips resize forwarding unless the matched server has an attached browser fd;
- releases `state_mutex` before writing the `Resize` protobuf to the browser fd;
- sends `Resize` with `tab_id`, `pixel_width`, and `pixel_height`;
- leaves the screen fields at their proto3 default zero values, matching the
  Wezboard fallback path until Ghostboard has real overlay/window metric
  integration;
- logs successful sends as `Resize: pane_id=... tab_id=... pixel=...x...`.

Verification passed:

- `zig fmt src/apprt/termsurf.zig src/main_c.zig src/build/SharedDeps.zig`
  passed: `logs/ghostboard-exp22-zig-fmt-20260616-after-review.log`.
- Native GhosttyKit framework build passed:
  `logs/ghostboard-exp22-zig-native-xcframework-20260616-after-review.log`.
- macOS app build passed on the serial rerun after the framework existed:
  `logs/ghostboard-exp22-macos-build-debug-20260616-rerun.log`.
- Runtime harness passed:
  `logs/ghostboard-exp22-runtime-harness-20260616-final.log`.
- Runtime app log: `logs/ghostboard-exp22-runtime-app-20260616-final.log`.
- `git diff --check` passed.

Observed successful runtime checks:

```text
PASS: helper path is longer than old 64-byte browser limit
PASS: child wrote TERMSURF_SOCKET
PASS: socket path is under TMPDIR/termsurf
PASS: socket exists while app is running
PASS: pre-TabReady duplicate SetOverlay did not emit Resize
PASS: TUI socket received BrowserReady with real listen socket
PASS: BrowserReady browser_socket is not GUI socket
PASS: helper received Resize after second SetOverlay
PASS: Resize tab_id and pixel dimensions match updated overlay
PASS: Resize screen fallback fields are zero
PASS: fresh TUI client received HelloReply
PASS: app exited after SIGTERM
PASS: socket file removed after shutdown
PASS: app log contains Resize
PASS: no CaContext emitted
runtime verification passed
```

The runtime harness verified that the helper browser received `Resize` only
after `TabReady` and after a repeated `SetOverlay`, with:

- `tab_id = 42`;
- `pixel_width = 1000`;
- `pixel_height = 600`;
- proto3-default zero screen fields.

The app log contains:

```text
Resize: pane_id=pane-a tab_id=42 pixel=1000x600
```

## Conclusion

Ghostboard now keeps browser surface dimensions synchronized for the first
state-backed resize path. This gives Roamium updated pixel dimensions when the
TUI changes an existing overlay, while native overlay presentation,
screen-coordinate metrics, navigation forwarding, DevTools overlay creation, and
input forwarding remain for later experiments.

## Result Review

Fresh-context adversarial result review initially returned **CHANGES REQUIRED**
with one required finding: the first formatter and native build logs were empty,
so those referenced logs did not independently prove the commands ran or exited
successfully.

Required finding accepted and fixed: the formatter and native build were rerun
with wrappers that record the command, working directory, and `exit_status: 0`
inside the logs:

- `logs/ghostboard-exp22-zig-fmt-20260616-after-review.log`;
- `logs/ghostboard-exp22-zig-native-xcframework-20260616-after-review.log`.

Fresh-context adversarial re-review returned **APPROVED**. The reviewer
confirmed the replacement logs are nonempty and record the command, working
directory, and `exit_status: 0` for both the formatter and native framework
build. No new required findings were introduced by the fix.
