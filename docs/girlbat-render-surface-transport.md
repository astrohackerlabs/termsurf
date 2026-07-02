# Girlbat Render-Surface Transport

Issue 884 Experiment 13 audits the transport boundary between Girlbat's Ladybird
render surface and Ghostboard. Experiment 12 proved ABI-local access to a
presentable `Gfx::SharedImageBuffer`; this document records why that surface
cannot simply be represented as a protobuf integer and what the next rendering
experiment should do instead.

The audit is enforced by:

```bash
scripts/audit-girlbat-render-transport.py
```

## Source Facts

The current TermSurf wire format is plain length-prefixed protobuf bytes over a
Unix socket:

- `girlbat/src/ipc.rs`, `roamium/src/ipc.rs`, and `surfari/src/ipc.rs` write a
  4-byte little-endian length followed by `TermSurfMessage` bytes.
- `ghostboard/src/apprt/termsurf.zig` reads the same frame shape with
  `std.posix.read` and writes it with `std.posix.write`.
- The TermSurf IPC paths do not currently use `sendmsg`, `recvmsg`,
  `SCM_RIGHTS`, Mach-port attachments, or another side channel.

The legacy rendering protocol message is CAContext-specific:

- `proto/termsurf.proto` has `CaContext ca_context = 14`.
- `proto/termsurf.proto` also has metadata-only
  `RenderSurface render_surface = 43` for the Girlbat side-channel path.
- `ghostboard/src/apprt/termsurf.zig` rejects a `CaContext` with
  `ca_context_id == 0` and presents through the CAContext overlay path.
- The current highest `TermSurfMessage.oneof` field is `render_surface = 43`;
  field 44 is available for the next backward-compatible message.

Ladybird's macOS render-surface sharing path is IOSurface/Mach-port based:

- `vendor/ladybird/Libraries/LibCore/IOSurface.cpp` creates IOSurfaces without
  `kIOSurfaceIsGlobal`.
- The same file exports and imports surfaces with `IOSurfaceCreateMachPort` and
  `IOSurfaceLookupFromMachPort`.
- `vendor/ladybird/Libraries/LibGfx/SharedImage.cpp` encodes macOS
  `Gfx::SharedImage` values as `Attachment::from_mach_port(..., MoveSend)`.
- `vendor/ladybird/Libraries/LibGfx/SharedImageBuffer.cpp` implements
  `export_shared_image()` with `m_iosurface_handle.create_mach_port()`.

Ghostboard already has a presentation primitive but not an import path:

- `ghostboard/src/renderer/metal/IOSurfaceLayer.zig` can set a CALayer's
  contents to an `IOSurface`.
- `ghostboard/pkg/macos/iosurface/iosurface.zig` can create and inspect
  IOSurfaces, but it does not expose `IOSurfaceLookup` or
  `IOSurfaceLookupFromMachPort`.
- No TermSurf protocol receiver maps a Girlbat render-surface event to this
  IOSurface presentation layer.

## IOSurface ID Probe

The macOS SDK exposes `IOSurfaceID`, `IOSurfaceGetID`, and `IOSurfaceLookup`,
but that is not enough for Girlbat. The SDK also documents `kIOSurfaceIsGlobal`
as the option that allows lookup by ID, and marks global surfaces as deprecated
because they are insecure.

The audit script compiles and runs a small two-process probe:

1. create a non-global IOSurface like Ladybird does;
2. get its `IOSurfaceID`;
3. `exec` a separate process that calls `IOSurfaceLookup(id)`;
4. require that lookup to fail;
5. repeat with an intentionally global IOSurface and require lookup to succeed.

The probe result is:

```text
PASS iosurface_id_probe_non_global_lookup_rejected
PASS iosurface_id_probe_global_lookup_succeeded
```

That proves an ID-only protobuf field is not viable for Ladybird's current
non-global IOSurfaces. The render-surface transport must carry or make available
the Mach send right, or use an equivalent OS transport that preserves the same
authority.

## Transport Options

### Reuse `CaContext`

Rejected. `CaContext.ca_context_id` means a remote Core Animation context ID for
Roamium and Surfari. Girlbat's Ladybird surface is an IOSurface/Mach-port
surface with different import, lifetime, and ownership semantics. Reusing
`CaContext` would make the protocol ambiguous and risk existing engines.

### Send An IOSurface ID In Protobuf

Rejected for Ladybird's current surfaces. ID lookup only works for global
IOSurfaces, and Ladybird does not create global IOSurfaces. Making Ladybird
create global IOSurfaces would adopt a deprecated insecure sharing mode and
would diverge from Ladybird's own `SharedImage` transport.

### Use Unix-Socket Ancillary Data Directly

Not selected. `SCM_RIGHTS` over an `AF_UNIX` socket transfers file descriptors,
not Mach send rights. Ladybird's macOS `SharedImage` transport is a Mach-port
attachment. This path is only viable if a later spike proves a correct
Mach-port-to-file-descriptor conversion or changes the surface representation to
an fd-backed object, neither of which is true today.

### Add A Mach-Port Or XPC Transport

Chosen next direction. The next experiment should be a focused transport spike
that proves how Girlbat can transfer the IOSurface Mach send right to
Ghostboard. Plausible implementation directions include:

- a small Mach/XPC side channel for render-surface attachments;
- adapting a narrow piece of Ladybird's `LibIPC` Mach-port attachment mechanism;
- another macOS-specific bridge that moves the send right without making the
  IOSurface global.

The spike should avoid broad protocol work until it proves one concrete
transport end to end.

## Future Protocol Shape

After the Mach-port/XPC transport was proven, `termsurf.proto` gained a new
metadata message instead of reusing `CaContext`. The next available oneof field
is 44.

The current metadata message carries:

- `tab_id`;
- `pixel_width`;
- `pixel_height`;
- `generation`;
- `bytes_per_row`;
- `pixel_format`;
- `attachment_id`.

The Mach send right itself should not be represented as a plain integer in
protobuf unless the chosen transport explicitly makes that integer meaningful in
the receiving task.

`attachment_id = 0` means metadata-only: no real attachment match has been
proven. Nonzero `attachment_id` values mean Ghostboard imported a side-channel
attachment with the same correlation key and matching dimensions, bytes per row,
pixel format, and generation.

Issue 884 Experiment 20 proves the first nonzero case for Girlbat. The real
Ladybird ABI exports an IOSurface Mach send right from the headless
`TermSurfWebView`, Girlbat sends it over the render side channel with
`attachment_id = 1`, and the smoke path verifies Ghostboard/probe-side import
with matching metadata.

Issue 884 Experiment 21 then makes that matched attachment structurally
presentable in Ghostboard. Ghostboard retains the imported receive result, a
matched nonzero `RenderSurface.attachment_id` calls the IOSurface-specific
AppKit bridge, Swift retains the `IOSurfaceRef` before async dispatch, and the
AppKit path attaches a normal `CALayer` whose `contents` is that IOSurface. This
is still a one-shot, per-server proof. It does not yet prove continuous per-tab
frame delivery or screenshot/readback-backed visual correctness.

Issue 884 Experiment 35 proves that the structural path also works through the
normal runtime route: a repo Debug Ghostboard app launches WebTUI with
`web --browser girlbat`, resolves Girlbat by name through
`TERMSURF_GIRLBAT_PATH`, spawns Girlbat with a render side-channel, loads a
normal local HTTP page, receives matched nonzero `RenderSurface` metadata, and
logs AppKit `presented_iosurface_pixels`. This remains structural presentation
evidence only; it is not screenshot/readback-backed visual correctness and it is
not continuous frame streaming.

## Lifetime And Stale Frames

Girlbat must keep the exported surface alive until Ghostboard has imported or
replaced it. Ghostboard must validate dimensions and generation before
presenting a frame:

- `generation` lets Ghostboard ignore stale frames after resize or navigation.
- `pixel_width` and `pixel_height` let Ghostboard reject wrong-size surfaces,
  matching the existing `IOSurfaceLayer.setSurfaceCallback()` behavior.
- Surface lifetime must be tied to the tab/view generation, not only the
  transient protobuf message.

## Next Experiment

Issue 884 Experiment 14 proved the first concrete Mach-port transport spike with
`scripts/test-girlbat-iosurface-mach-port-transport.py`:

- a parent process registers a bootstrap control Mach port;
- the child process looks up that control port and sends the parent a
  child-owned receive port;
- the parent creates a non-global IOSurface after the child starts;
- the parent sends `IOSurfaceCreateMachPort(surface)` to the child with a
  `MACH_MSG_PORT_DESCRIPTOR` and `MACH_MSG_TYPE_MOVE_SEND`;
- the child imports the surface with `IOSurfaceLookupFromMachPort` and verifies
  width, height, bytes per row, and pixel format.

The bootstrap registration in that probe is test scaffolding. It proves the Mach
send-right transfer, but it is not a commitment to use `bootstrap_register` in
Ghostboard's production integration.

Issue 884 Experiment 15 then proved the spawned-process topology that matches
Ghostboard's browser launch model:

- a parent process registers a per-process side-channel service;
- the parent spawns a fresh child executable and passes the service name as a
  command-line argument;
- the child looks up the service, creates its own render-surface receive port,
  and sends a send right for that port back to the parent;
- the parent creates a non-global IOSurface and sends its Mach send right to the
  spawned child;
- the child imports the surface with `IOSurfaceLookupFromMachPort` and verifies
  metadata.

That makes the likely TermSurf integration shape concrete: Ghostboard can create
a per-browser-process render side channel when spawning Girlbat, pass the
side-channel service/token as an argument, and keep the existing protobuf Unix
socket for control messages. Future render-surface protobuf metadata should
carry tab/generation/dimensions and a correlation token; the Mach side channel
should carry the actual IOSurface send right.

The bootstrap registration in the probes is test scaffolding. It proves the
side-channel topology, but the production implementation may use a different
Mach/XPC bootstrap mechanism if that is more robust inside Ghostboard's runtime
environment.

Issue 884 Experiment 16 adds the first production launch contract for that
topology:

- Ghostboard builds browser spawn argv through a testable helper.
- Only Girlbat receives
  `--render-surface-service=com.termsurf.girlbat.render.<pid>.<profile>.<browser>`.
- The service token is bounded, reverse-DNS-like, and sanitizes profile/browser
  components to avoid embedding raw filesystem or socket paths.
- Roamium and Surfari do not receive the argument and continue using the
  existing `CaContext` rendering path.
- Girlbat parses the argument and records whether the render side-channel is
  configured during startup, but it does not connect to the channel yet.

Experiment 17 used that launch contract to connect the first production
side-channel handshake, Experiment 18 proved deterministic IOSurface transfer,
Experiment 19 added the `RenderSurface` metadata seam, Experiment 20 proved a
real Ladybird frame attachment can be exported, imported, and matched by a
nonzero `attachment_id`, Experiment 21 structurally presents that attachment
through the AppKit IOSurface overlay path, and Experiment 35 proves the same
path from a normal HTTP page in the Ghostboard/WebTUI runtime.

The next implementation should expand the one-shot proof into continuous per-tab
frame delivery and add screenshot/readback evidence before TermSurf treats
Girlbat rendering as visually correct on screen.

Experiment 22 adds a related control-path proof, not a transport proof. Girlbat
can now receive `Resize` and apply the requested dimensions through the Ladybird
headless viewport ABI. The experiment does not prove that resize alone generates
a new side-channel attachment, presents a resized frame, or produces
screenshot/readback-backed visual correctness.

Experiment 24 records that Girlbat's non-use of legacy `CaContext` is
intentional. `RenderSurface` remains the authoritative Girlbat metadata path,
with the side channel carrying the actual IOSurface authority. A Girlbat
`CaContext` emission would be a regression against this transport design.

Issue 884 Experiment 17 wires the first production side-channel handshake:

- `render-channel/termsurf_render_channel.h` and
  `render-channel/termsurf_render_channel.c` define the shared C ABI for the
  bootstrap handshake.
- Ghostboard links that shim and calls `tsrc_register_service()` for Girlbat
  only, before spawning the browser process.
- Girlbat links the same shim and calls `tsrc_child_connect_and_send()` when
  `--render-surface-service=...` is present.
- Ghostboard receives the child render-channel send right on a detached bounded
  thread via `tsrc_wait_for_child_port()`, records it only if the server still
  matches the same profile/browser/PID/service tuple, and deallocates stale or
  discarded rights.
- Roamium and Surfari stay on the existing `CaContext` path and do not register
  or consume render side-channel state.

The current shim still uses `bootstrap_register`, so the earlier warning remains
relevant: this is a first production-code handshake, not final proof that
bootstrap registration is the permanent distribution mechanism. The shared shim
probe proves the chosen mechanism in a normal local process context and
Ghostboard's test suite proves the shim links into Ghostboard. A later installed
app smoke test should confirm the same mechanism from the signed `TermSurf.app`
runtime before relying on it for user-visible rendering.

Experiment 18 sent a test IOSurface Mach send right over the established channel
and added the Ghostboard import primitive needed to convert that right into an
`IOSurface`.

Issue 884 Experiment 18 proves that bidirectional test-surface transfer:

- the Experiment 17 bootstrap still gives Ghostboard a send right to Girlbat's
  child render-channel receive port;
- Ghostboard then allocates a new Ghostboard-owned surface receive port, inserts
  a send right, and sends that send right to Girlbat using
  `TSRC_SURFACE_RECEIVER_MESSAGE_ID`;
- Girlbat receives that Ghostboard surface send right, creates a deterministic
  `16x16` BGRA IOSurface, creates an IOSurface Mach send right, and sends it
  back to Ghostboard with `TSRC_TEST_SURFACE_MESSAGE_ID`;
- Ghostboard receives the IOSurface send right, imports it with
  `IOSurfaceLookupFromMachPort`, records both reported and imported metadata,
  releases the imported `IOSurfaceRef`, deallocates the received IOSurface send
  right, and destroys the Ghostboard-owned receive right with
  `tsrc_destroy_receive_port()`.

The directionality was correct for rendering: Girlbat could deliver IOSurface
authority to Ghostboard. That proof deliberately sent a deterministic test
surface instead of a real Ladybird frame.

Issue 884 Experiment 19 adds only protobuf metadata/routing. Issue 884
Experiment 20 then uses the same side channel as the attachment path for one
real Girlbat frame surface and connects that attachment to nonzero
`RenderSurface.attachment_id` metadata. Issue 884 Experiment 21 stores the
retained imported surface and routes the matched attachment to an IOSurface
AppKit overlay. The existing `CaContext` path remains the Roamium/Surfari path
and should not be reused for Girlbat IOSurface frames.
