# Girlbat Rendering Strategy

Issue 884 needs Girlbat to render a normal Ladybird page inside Ghostboard. The
current Girlbat implementation is still headless: it can create tabs, navigate,
query state, and emit headless `TabReady`/`UrlChanged`/`LoadingState` events,
but it does not expose a render surface.

This document records the render-surface audit from Issue 884 Experiment 11 and
the ABI reachability update from Experiment 12. The follow-on transport audit is
recorded in
[girlbat-render-surface-transport.md](girlbat-render-surface-transport.md).

## Audited Source

- Ladybird checkout: `vendor/ladybird`
- Branch: `a80d01fc-issue-0884-persistent-ladybird-abi-handles`
- Commit: `abba707e84f9ba7398ab3d031fa4178b25a70aba`
- Audit command: `scripts/audit-girlbat-render-surface.py`

The audit script matches stable source symbols and reports line numbers as
evidence. Line numbers are not treated as the source of truth.

## Source Facts

Ladybird's AppKit bridge already exposes a presentable image-buffer concept:

- [LadybirdWebViewBridge.h](../vendor/ladybird/UI/AppKit/Interface/LadybirdWebViewBridge.h)
  declares `struct Paintable` with
  `Gfx::SharedImageBuffer const* shared_image_buffer` and
  `Optional<Paintable> paintable()`.
- [LadybirdWebViewBridge.cpp](../vendor/ladybird/UI/AppKit/Interface/LadybirdWebViewBridge.cpp)
  returns `m_client_state.front_bitmap.shared_image_buffer` when the current
  client state has a usable bitmap, or `m_backup_shared_image_buffer` while
  preserving the prior frame during process changes.

The image buffer is IOSurface-backed on macOS:

- [SharedImageBuffer.h](../vendor/ladybird/Libraries/LibGfx/SharedImageBuffer.h)
  exposes `iosurface_handle()` under `AK_OS_MACOS` and `export_shared_image()`.
- [IOSurface.cpp](../vendor/ladybird/Libraries/LibCore/IOSurface.cpp) exposes
  `IOSurfaceHandle::create_mach_port()` using `IOSurfaceCreateMachPort`.
- [SharedImage.cpp](../vendor/ladybird/Libraries/LibGfx/SharedImage.cpp) encodes
  macOS `Gfx::SharedImage` values as Mach-port attachments.
- [WebContentViewMac.mm](../vendor/ladybird/UI/Qt/WebContentViewMac.mm) imports
  the shared image buffer's IOSurface into Metal with
  `newTextureWithDescriptor:iosurface:plane:`.

The current headless Girlbat path cannot reach that surface through a public
API:

- [HeadlessWebView.h](../vendor/ladybird/Libraries/LibWebView/HeadlessWebView.h)
  exposes lifecycle and viewport helpers, but no `paintable()` or equivalent
  public render-surface accessor.
- [ViewImplementation.h](../vendor/ladybird/Libraries/LibWebView/ViewImplementation.h)
  owns the relevant client state, including the front/backup shared image
  buffers, but that state is not publicly accessible through `HeadlessWebView`.

Ghostboard currently consumes Core Animation contexts, not IOSurfaces from the
TermSurf protocol:

- [termsurf.proto](../proto/termsurf.proto) defines `CaContext` with
  `ca_context_id`, `pixel_width`, and `pixel_height`.
- [termsurf.zig](../ghostboard/src/apprt/termsurf.zig) rejects a `CaContext`
  whose `ca_context_id` is zero, records the context ID on the pane, and
  presents the overlay through the CAContext-oriented path.
- `termsurf.proto` currently has no IOSurface, Mach-port, or generic render
  surface message.

## Strategy Options

### Overload `CaContext`

Rejected. `CaContext.ca_context_id` means a remote Core Animation context ID in
Ghostboard, Roamium, and Surfari. An IOSurface/Mach-port render surface has
different ownership and import semantics. Reusing `CaContext` would make the
wire protocol ambiguous and would risk regressions in existing engines.

### Patch Ladybird To Create A CAContext

Not the next step. Ladybird's current macOS rendering model already produces
IOSurface-backed shared image buffers, and both Qt and AppKit paths consume that
shape. A CAContext bridge would be a larger Ladybird-specific rendering patch
that works against the existing upstream direction.

### In-Process AppKit `NSView` Overlay

Not the next step. TermSurf's engine architecture is one browser profile server
process speaking protobuf/Unix sockets to Ghostboard. Embedding a Ladybird
`NSView` directly into Ghostboard would cross that process boundary in the wrong
direction and would not generalize cleanly to other terminal frontends.

### New IOSurface/Mach-Port Render Surface Message

Chosen direction. Girlbat should expose a render surface compatible with
Ladybird's native macOS buffer model: IOSurface-backed `Gfx::SharedImageBuffer`
exported through a Mach-port transfer or an equivalent transport. Ghostboard
should gain a receiver/import path distinct from `handleCaContext()`.

This keeps `CaContext` semantics stable for Roamium and Surfari while allowing
Girlbat to use Ladybird's native rendering primitive honestly.

## Headless Reachability Decision

There are three possible reachability outcomes:

1. Existing public API: current `HeadlessWebView` exposes a presentable
   `SharedImageBuffer`/IOSurface directly.
2. Girlbat-owned subclass: `libtermsurf_ladybird` can define a TermSurf-specific
   `ViewImplementation` subclass that exposes the protected client-state image
   buffer without patching upstream Ladybird.
3. Vendor patch: Ladybird itself needs an issue-specific branch and patch to
   expose the render surface.

The audit rules out outcome 1. `HeadlessWebView` has no public render-surface
accessor.

The likely next implementation should test outcome 2 first: create a
TermSurf-specific `ViewImplementation` subclass inside
`libtermsurf_ladybird`/the Ladybird CMake target, modeled on the AppKit
`WebViewBridge` but without AppKit UI. That subclass should expose a `paintable`
or `export_shared_image` style method through the C ABI. If Ladybird's protected
client-state shape or build boundaries prevent that, then the next experiment
must switch to outcome 3 and use an issue-specific Ladybird branch, patch
archive, and README update.

## Protocol Direction

The next protocol work should add a fresh backward-compatible
`TermSurfMessage.oneof` field, not reinterpret field 14 `CaContext`. The current
highest field number is 42, so the likely next field is 43.

The new message should represent the surface shape explicitly. The exact fields
should be designed in the implementation experiment, but the likely data is:

- `tab_id`;
- pixel width and height;
- IOSurface/Mach-port transfer identity or a transport-specific attachment
  mechanism;
- a generation or frame identifier so Ghostboard can ignore stale frames;
- possibly bytes-per-row/pixel-format metadata if Ghostboard cannot obtain it
  from IOSurface APIs.

Issue 884 Experiment 13 made that transport choice sharper:

- ID-only protobuf is rejected for Ladybird's current non-global IOSurfaces. A
  two-process probe shows that `IOSurfaceLookup(id)` fails for non-global
  surfaces and succeeds only for intentionally global surfaces.
- Unix-socket `SCM_RIGHTS` is not direct Mach-port transfer; it passes file
  descriptors, while Ladybird exports macOS `SharedImage` values as Mach send
  rights.
- The next rendering step should therefore be a focused Mach-port/XPC transport
  spike before the final `RenderSurface` message is added.

The design must preserve existing Roamium and Surfari behavior. Those engines
should keep using `CaContext`.

## Ghostboard Scope

Ghostboard needs a path separate from `handleCaContext()`:

- decode the new render-surface message;
- map `tab_id` back to the pane using the existing server/profile/browser tab
  lookup model;
- import or look up the IOSurface/Mach-port surface;
- present it through a new overlay snapshot/presentation path that does not
  require a CAContext ID;
- keep the existing CAContext path unchanged for Roamium and Surfari.

The current Ghostboard code already has IOSurface-oriented renderer support in
the macOS/Metal stack, but the TermSurf protocol receiver currently reaches that
code only through the CAContext overlay path. The new path should be tested
without regressing CAContext overlays.

## Next Experiment

Issue 884 Experiment 12 proved the first concrete render-surface boundary:

- `LibTermSurfLadybird` can define a TermSurf-owned `HeadlessWebView` subclass
  in the existing Ladybird target without patching core `LibWebView`.
- The subclass can reach the same protected backing-store state used by
  Ladybird's AppKit bridge.
- The C ABI can report a presentable `Gfx::SharedImageBuffer` after loading a
  deterministic page.
- The positive real-mode smoke observed a surface with `800x600` pixel
  dimensions, `can_export_shared_image=true`, `ready_to_paint_seen=true`,
  `has_usable_bitmap=true`, and generation `1`.

This confirms outcome 2 from the reachability decision: a Girlbat-owned subclass
is enough for initial surface access. A deeper Ladybird patch is not required
for this specific boundary.

`can_export_shared_image=true` means that a non-null buffer existed and
`Gfx::SharedImageBuffer::export_shared_image()` completed on the ABI owner
thread. It does not prove that Ghostboard can receive, import, or present the
Mach port yet.

Experiment 13 then audited the transport boundary and rejected ID-only protobuf.
Experiment 14 proved a standalone Mach-message transport: a parent process can
send a non-global IOSurface Mach send right to a child process, and the child
can import it with `IOSurfaceLookupFromMachPort`. Experiment 15 proved the same
basic side-channel topology across a spawned child executable, matching
Ghostboard's browser launch model. Experiment 16 then wired the first production
launch contract: Ghostboard passes a bounded `--render-surface-service=...`
token only to spawned Girlbat processes, and Girlbat parses that token without
yet connecting a real render side channel. Experiment 17 added the first
production-code handshake using a shared C shim: Ghostboard registers the
Girlbat-only side-channel service, Girlbat sends back a render-channel port, and
Ghostboard records that port without changing protobuf or `CaContext`.

Experiment 18 makes the channel bidirectional in the direction rendering needs.
Ghostboard sends Girlbat a Ghostboard-owned surface receive port over the
Experiment 17 child port, and Girlbat sends a deterministic test IOSurface Mach
send right back to Ghostboard. Ghostboard imports that right with
`IOSurfaceLookupFromMachPort` and validates reported/imported dimensions, bytes
per row, pixel format, and generation. This proves the attachment transport and
import seam for a test surface while leaving visible rendering and protobuf
metadata for a later experiment.

Experiment 19 adds the first TermSurf `RenderSurface` metadata seam:

- `RenderSurface render_surface = 43` carries tab identity, dimensions, bytes
  per row, pixel format, generation, and `attachment_id`;
- `attachment_id = 0` means metadata-only, with no proven side-channel
  attachment correlation;
- nonzero `attachment_id` values mean a side-channel attachment with matching
  metadata was imported;
- Ghostboard routes and stores the metadata for Girlbat panes while leaving
  Roamium/Surfari `CaContext` behavior unchanged.

Experiment 20 connects the metadata seam to one real Ladybird frame attachment:

1. The Ladybird-backed ABI exports a real IOSurface Mach send right from
   `TermSurfWebView`.
2. Girlbat carries that send right over the render side channel with a nonzero
   `attachment_id`.
3. Ghostboard accepts nonzero `RenderSurface` metadata only when it has imported
   the matching side-channel attachment.
4. The existing length-prefixed protobuf stream and Roamium/Surfari `CaContext`
   behavior remain unchanged.

Experiment 22 adds headless viewport resize control through the Ladybird C ABI.
Girlbat now routes `Resize` to `ts_ladybird_view_resize`, which calls
`HeadlessWebView::reset_viewport_size()`. This is a viewport-control proof only:
it does not prove that resizing emits a fresh presentable frame, that Ghostboard
presents the resized frame, or that screenshot/readback visual correctness
exists.

Experiment 24 makes the legacy `CaContext` decision explicit in the protobuf
coverage matrix. For Girlbat, emitting `CaContext` would be a protocol
regression because Ladybird uses the `RenderSurface` metadata message plus the
IOSurface/Mach-port side channel. Roamium and Surfari keep using the existing
`CaContext` path.

Experiment 21 wires that matched IOSurface into Ghostboard's AppKit overlay
presentation path structurally:

1. Ghostboard keeps the imported `IOSurfaceRef` alive as an owned
   `tsrc_received_surface_t` on the render side-channel state.
2. A matched nonzero `RenderSurface.attachment_id` creates an IOSurface overlay
   snapshot instead of only storing metadata.
3. Swift retains the opaque `IOSurfaceRef` before async main-queue dispatch and
   presents it as normal `CALayer.contents`, mutually exclusive with the
   Roamium/Surfari `CALayerHost` path.
4. The deterministic AppKit smoke verifies an imported Mach-send-right surface,
   a nonzero attached layer, contents identity, and release-on-clear.

This is structural presentation evidence, not a full on-screen visual
correctness claim. Screenshot/readback evidence and continuous per-tab frame
delivery remain incomplete.

Experiment 35 proves that this structural presentation path works from the
normal runtime route as well: WebTUI runs `web --browser girlbat` inside Debug
Ghostboard, Ghostboard resolves the named Girlbat executable through
`TERMSURF_GIRLBAT_PATH`, Girlbat loads a normal local HTTP page, emits matched
nonzero `RenderSurface` metadata, and AppKit logs `presented_iosurface_pixels`.
This still does not prove screenshot/readback visual correctness or continuous
frame streaming.
