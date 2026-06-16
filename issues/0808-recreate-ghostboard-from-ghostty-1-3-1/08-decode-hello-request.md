# Experiment 8: Decode HelloRequest

## Description

Experiment 7 proved that `TermSurf.app` starts a PID-scoped Unix socket, exports
`TERMSURF_SOCKET` into terminal sessions, accepts a local client, and cleans the
socket up on controlled shutdown.

The next smallest protocol step is to make that socket understand the TermSurf
wire framing and one safe request/reply message: `HelloRequest` from a TUI and
`HelloReply` from the GUI. This proves the new Ghostboard-derived app can read
length-prefixed protobuf messages and write length-prefixed protobuf replies
without yet launching Roamium, rendering overlays, forwarding input, or changing
`webtui`/`roamium`.

This experiment should use the current protocol definition in
`proto/termsurf.proto`. Ghostboard Legacy can be used as a reference for the
protobuf-c approach and length-prefixed send helper, but its generated protocol
files are historical and must not be treated as authoritative unless they are
regenerated from the current `proto/termsurf.proto`.

## Changes

- `ghostboard/src/protobuf/` — add the current TermSurf protobuf-c runtime and
  generated C sources needed by Zig:
  - `protobuf-c.c` / `protobuf-c.h`;
  - `protobuf-c/protobuf-c.h`, or an equivalent generated-header-compatible
    include layout, because generated protobuf-c headers conventionally include
    `<protobuf-c/protobuf-c.h>`;
  - `termsurf.pb-c.c` / `termsurf.pb-c.h` generated from the current
    `proto/termsurf.proto`;
  - a short note or generation command in the experiment result describing how
    the generated files were produced.
- `ghostboard/src/build/SharedDeps.zig` — wire the protobuf-c include path and C
  sources into Ghostboard's shared build dependencies, following the narrow
  pattern used by Ghostboard Legacy.
- `ghostboard/src/apprt/termsurf.zig` — replace the immediate close-on-accept
  behavior with per-client handling that:
  - reads the TermSurf wire format: 4-byte little-endian length prefix followed
    by serialized `TermSurfMessage`;
  - rejects clearly invalid frame sizes instead of allocating unbounded memory;
  - handles partial reads correctly;
  - decodes `TermSurfMessage` with protobuf-c;
  - logs the decoded message type;
  - replies to `HelloRequest` with a length-prefixed `HelloReply`;
  - keeps all other message types out of scope except for logging and
    disconnect/error handling;
  - continues to clean up client fds and the listening socket during shutdown.
- `ghostboard/src/main_c.zig` or a small dedicated C import boundary — expose
  the generated protobuf-c declarations to Zig without changing existing Ghostty
  C ABI names.
- Issue docs — record the result and update the experiment index.

This experiment intentionally does not:

- implement `SetOverlay`, `Navigate`, `SetColorScheme`, query messages, browser
  registration, tab lifecycle, browser launch, overlays, input forwarding, or
  direct TUI-browser routing;
- modify `webtui` or `roamium`;
- change the TermSurf protocol schema;
- change app naming, config paths, icons, or CLI install/emit behavior.

## Verification

1. Confirm the generated `termsurf.pb-c.*` files correspond to the current
   `proto/termsurf.proto`, not stale Ghostboard Legacy output. The result should
   record the exact generation command or source.
2. Run Zig formatting on edited Zig files.
3. If Swift files are edited, run SwiftLint:

   ```bash
   cd ghostboard
   swiftlint lint --strict --fix
   swiftlint lint --strict
   ```

4. Format edited markdown.
5. Build the native GhosttyKit framework:

   ```bash
   cd ghostboard
   zig build -Demit-xcframework=true -Dxcframework-target=native -Demit-macos-app=false
   ```

6. Build the macOS app:

   ```bash
   cd ghostboard
   macos/build.nu --scheme Ghostty --configuration Debug --action build
   ```

7. Launch `TermSurf.app` with the same deterministic temporary config style used
   by Experiment 7 so the initial terminal child inherits `TERMSURF_SOCKET`.
8. From a local harness, connect to the inherited socket and send a manually
   encoded current-protocol `HelloRequest` frame:

   ```text
   09 00 00 00                         # payload length = 9
   ba 01 06                            # TermSurfMessage field 23, len 6
   0a 04 65 78 70 38                   # HelloRequest pane_id = "exp8"
   ```

9. Verify the app replies with a valid length-prefixed `HelloReply` frame. For
   an empty reply body, the expected payload is:

   ```text
   c2 01 00                            # TermSurfMessage field 24, len 0
   ```

   The harness should decode at least the wrapper field number and length so the
   pass condition is not just "some bytes arrived".

10. Repeat the same `HelloRequest`/`HelloReply` check with deliberately partial
    writes. At minimum, split the frame across multiple writes so the listener
    receives:
    - only part of the 4-byte length prefix first;
    - the rest of the prefix later;
    - the protobuf payload in multiple small chunks, ideally one byte at a time
      or similarly small slices.

    The pass condition is the same valid `HelloReply`. This prevents a
    full-buffer happy-path implementation from passing.

11. Test invalid frame-size handling by sending a length prefix greater than the
    implementation's documented maximum frame size and no valid payload. Verify:
    - the app logs that the frame was rejected;
    - the client connection is closed or otherwise rejected;
    - the app process remains alive;
    - a subsequent fresh valid `HelloRequest` on a new connection still gets a
      valid `HelloReply`;
    - shutdown still removes the socket file.
12. Verify the app log records:
    - the socket listener path;
    - accepted client connection;
    - decoded `HelloRequest`;
    - sent `HelloReply`.
13. Verify a client can disconnect after the reply without crashing or hanging
    the app.
14. Terminate the app and verify the socket file is removed.
15. Confirm the diff did not touch `webtui`, `roamium`, browser launch, overlay,
    input forwarding, app identity, config paths, icons, `build.zig`, or CLI
    install/emit behavior.

Pass criteria:

- The app still builds and launches as `TermSurf.app`.
- The GUI socket still starts before the first terminal session and
  `TERMSURF_SOCKET` still propagates.
- The listener accepts a client that sends a valid length-prefixed current
  `HelloRequest`.
- The listener handles a valid `HelloRequest` split across partial writes.
- The listener rejects an oversized frame without killing the app or breaking a
  later valid client.
- The app decodes that message as `HelloRequest`.
- The app sends a valid length-prefixed `HelloReply`.
- Client disconnect and app shutdown clean up without stale processes or socket
  files.
- Scope remains limited to protobuf framing plus `HelloRequest`/`HelloReply`.

Fail criteria:

- The protobuf files are stale or cannot be traced to the current
  `proto/termsurf.proto`.
- The app accepts raw connections but cannot decode the length-prefixed
  `TermSurfMessage`.
- The app reads only full-buffer happy paths and fails on partial frames.
- The app sends bytes that are not a valid `HelloReply` wrapper.
- The app leaks client fds, app processes, or socket files after the test.
- The experiment expands into browser launch, overlays, input, `webtui`,
  `roamium`, or unrelated build/identity behavior.

## Notes

If this experiment passes, the next experiment can implement another small
request/reply surface such as `QueryTabsRequest` or introduce connection
classification for TUI versus browser engine clients. Browser process launch
should still wait until framing and basic request/reply handling are boring.

## Design Review

Fresh-context adversarial design review initially returned `CHANGES REQUIRED`.

Required findings accepted and fixed:

- The protobuf-c file layout did not mention the nested
  `protobuf-c/protobuf-c.h` include path that generated protobuf-c headers
  conventionally use. The design now requires that path or an equivalent
  generated-header-compatible include layout.
- The verification required partial-read handling, but only tested a full-frame
  happy path. The design now requires a deliberately partial-write harness case
  that splits both the length prefix and payload.
- The verification required invalid frame-size rejection, but had no concrete
  oversized-frame test. The design now requires an oversized-frame test that
  verifies rejection, app survival, a later valid `HelloRequest`, and socket
  cleanup.

Re-review returned `APPROVED`. The reviewer confirmed the three required
findings were resolved, `git diff --check` was clean, the README still links
Experiment 8 as `Designed`, and the plan commit had not yet been made.
