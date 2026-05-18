+++
status = "closed"
opened = "2026-02-14"
closed = "2026-02-14"
+++

# Issue 415: Swift Receiver

## Goal

Reimplement the Issue 414 receiver in Swift. Two hidden Chromium profile servers
send IOSurface frames via XPC to a Swift receiver that composites them side by
side in one Metal window at 60fps. The result must be identical to Issue 414
Experiment 7 — same sender binary, same XPC protocol, same visual output — but
with the receiver written entirely in Swift.

## Motivation

Ghostty's macOS shell is written in Swift. When we integrate browser panes into
Ghostty, the IOSurface import, Metal compositing, and XPC handling will live in
Swift code. This issue proves that the entire receiver side of the pipeline
works in Swift before we touch Ghostty.

Issue 414 proved the architecture in Objective-C++. This issue ports the
receiver to Swift, validating that:

1. Swift can receive XPC Mach ports via the C XPC API (`xpc/xpc.h`)
2. Swift can reconstruct IOSurfaces from Mach ports
   (`IOSurfaceLookupFromMachPort`)
3. Swift's Metal API can create textures from IOSurfaces
4. The full pipeline sustains 60fps with Retina quality in Swift

If any of these fail or introduce overhead, we need to know before starting
Ghostty integration.

## Chromium branch

Create a new branch `146.0.7650.0-issue-415` in the `termsurf-chromium`
submodule, forked from `146.0.7650.0-issue-414`. Every issue gets its own branch
of Chromium, even when no Chromium changes are expected. This keeps the history
clean and allows each issue's submodule pointer to be pinned independently.

```bash
cd ts4/termsurf-chromium/src
git checkout -b 146.0.7650.0-issue-415 146.0.7650.0-issue-414
```

The sender is unchanged from Issue 414. The same `One Profile.app` binary with
`--hidden`, `--xpc-service`, `--session-id`, and `--user-data-dir` flags works
identically — the sender has no knowledge of what language the receiver is
written in. No Chromium code changes are expected, but the branch exists for
completeness.

## Project structure

**Folder:** `ts4/two-profiles-swift/`

```
ts4/two-profiles-swift/
├── Package.swift              — Swift Package Manager manifest
├── Sources/
│   └── Receiver/
│       ├── main.swift         — Entry point, NSApplication setup
│       ├── AppDelegate.swift  — Window creation, Metal setup, CVDisplayLink
│       ├── XPCListener.swift  — XPC Mach service listener, message handling
│       ├── Renderer.swift     — Metal render pipeline, viewport compositing
│       └── Shaders.metal      — Vertex + fragment shaders (same as Issue 414)
└── com.termsurf.two-profiles-swift.plist  — Launchd agent
```

Swift Package Manager (SPM) is used instead of a Makefile. SPM handles the Metal
shader compilation automatically when the `.metal` file is in the Sources
directory.

The existing `ts4/two-profiles-receiver/` (Objective-C++) remains untouched.

## Launchd service

A new Mach service name: `com.termsurf.two-profiles-swift`. This avoids
conflicting with the existing `com.termsurf.two-profiles` service used by the
Objective-C++ receiver. Both can coexist — useful for side-by-side comparison.

The plist (`ts4/two-profiles-swift/com.termsurf.two-profiles-swift.plist`):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.termsurf.two-profiles-swift</string>
    <key>MachServices</key>
    <dict>
        <key>com.termsurf.two-profiles-swift</key>
        <true/>
    </dict>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/ryan/dev/termsurf/ts4/two-profiles-swift/.build/debug/Receiver</string>
    </array>
    <key>StandardOutPath</key>
    <string>/Users/ryan/dev/termsurf/logs/two-profiles-swift.log</string>
    <key>StandardErrorPath</key>
    <string>/Users/ryan/dev/termsurf/logs/two-profiles-swift.log</string>
</dict>
</plist>
```

## XPC in Swift

Swift does not have native XPC types. The C XPC API (`xpc/xpc.h`) is available
via automatic bridging. The key functions map directly:

| C API                                  | Swift usage                                                     |
| -------------------------------------- | --------------------------------------------------------------- |
| `xpc_connection_create_mach_service()` | Same, returns `xpc_connection_t` (bridged as class)             |
| `xpc_connection_set_event_handler()`   | Same, closure instead of block                                  |
| `xpc_connection_resume()`              | Same                                                            |
| `xpc_dictionary_get_string()`          | Returns `UnsafePointer<CChar>?`, wrap with `String(cString:)`   |
| `xpc_dictionary_copy_mach_send()`      | Returns `mach_port_t`                                           |
| `xpc_get_type()`                       | Compare with `XPC_TYPE_CONNECTION`, `XPC_TYPE_DICTIONARY`, etc. |

The main difference from C/ObjC: XPC objects in Swift are managed by ARC
automatically (same as ObjC++ in Issue 414). The lesson from Experiment 3
applies — store connections in strong properties or globals to prevent ARC
release.

## Metal in Swift

Swift has first-class Metal bindings. The API is cleaner than Objective-C:

```swift
let device = MTLCreateSystemDefaultDevice()!
let layer = CAMetalLayer()
layer.device = device
layer.pixelFormat = .bgra8Unorm_sRGB
layer.contentsScale = NSScreen.main!.backingScaleFactor

// Create texture from IOSurface
let descriptor = MTLTextureDescriptor.texture2DDescriptor(
    pixelFormat: .bgra8Unorm_sRGB,
    width: IOSurfaceGetWidth(surface),
    height: IOSurfaceGetHeight(surface),
    mipmapped: false)
descriptor.usage = .shaderRead
let texture = device.makeTexture(descriptor: descriptor,
                                  iosurface: surface,
                                  plane: 0)
```

## IOSurface in Swift

The IOSurface framework is available in Swift. The key functions:

```swift
import IOSurface

let surface = IOSurfaceLookupFromMachPort(port)  // Returns IOSurfaceRef?
let width = IOSurfaceGetWidth(surface!)
let height = IOSurfaceGetHeight(surface!)
// When done:
CFRelease(surface!)
mach_port_deallocate(mach_task_self_, port)
```

Note: `mach_task_self()` is a macro in C. In Swift, use `mach_task_self_` (the
underlying global variable).

## Design

### Architecture

Identical to Issue 414 Experiment 7:

```
Profile Server A (hidden)     Profile Server B (hidden)
--session-id=profile-a        --session-id=profile-b
IOSurface 1600x1200 @ 60fps   IOSurface 1600x1200 @ 60fps
        |                             |
        | XPC Mach port               | XPC Mach port
        v                             v
    +--------------------------------------+
    |       Swift Receiver (Metal)         |
    |  +----------------+----------------+ |
    |  |  Left viewport | Right viewport | |
    |  |  profile-a     | profile-b      | |
    |  |  60fps         | 60fps          | |
    |  +----------------+----------------+ |
    +--------------------------------------+
```

### Module breakdown

**`main.swift`** — Minimal entry point. Starts the XPC listener, creates
`NSApplication`, sets the delegate, runs the event loop. Same structure as Issue
414's `main()` function: XPC listener starts before `NSApp.run()`.

**`AppDelegate.swift`** — `NSApplicationDelegate`. Creates a 1600x600 window,
initializes the `Renderer` on the content view, starts a `CVDisplayLink` for
vsync-driven rendering.

**`XPCListener.swift`** — Encapsulates XPC state. Creates the Mach service
listener, accepts multiple peer connections (stored in an array to prevent ARC
release), dispatches `display_surface` messages to a callback. Maps `session_id`
to pane index (LEFT/RIGHT). Thread-safe handoff of IOSurfaces to the renderer
via a lock.

**`Renderer.swift`** — Metal rendering. Creates the device, command queue,
pipeline, sampler. Manages two texture slots (left/right). `render()` draws two
viewports with the fullscreen quad shader, same as Issue 414.

**`Shaders.metal`** — Identical to `ts4/two-profiles-receiver/shaders.metal`.
The vertex shader generates a fullscreen triangle strip from `vertex_id`. The
fragment shader samples the texture. No changes needed.

## Build and run

```bash
# Build
cd ts4/two-profiles-swift
swift build

# Load launchd plist
launchctl load ~/dev/termsurf/ts4/two-profiles-swift/com.termsurf.two-profiles-swift.plist

# Start profile servers (same binary as Issue 414)
cd ~/dev/termsurf/ts4/termsurf-chromium/src

out/Default/One\ Profile.app/Contents/MacOS/One\ Profile \
  --hidden \
  --xpc-service=com.termsurf.two-profiles-swift \
  --session-id=profile-a \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-a \
  http://localhost:9407 2>&1 &

out/Default/One\ Profile.app/Contents/MacOS/One\ Profile \
  --hidden \
  --xpc-service=com.termsurf.two-profiles-swift \
  --session-id=profile-b \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-b \
  http://localhost:9407 2>&1 &
```

Note the XPC service name is `com.termsurf.two-profiles-swift` (not
`com.termsurf.two-profiles`).

## Success criteria

Identical to Issue 414:

- Two panes in one window, each showing the spinning blue square
- Different localStorage identity in each pane (profile isolation)
- Both at 60fps sustained for 60+ seconds
- Pixel-perfect Retina quality (1600x1200 IOSurface, sRGB, 1:1 mapping)
- IOSurface transfer via XPC

Plus:

- Receiver written entirely in Swift (no Objective-C bridging headers, no
  `-import-objc-header`)
- Builds with `swift build` (Swift Package Manager)

## Potential issues

- **CVDisplayLink is deprecated.** Issue 414 used `CVDisplayLink`, which Apple
  deprecated in macOS 14 (Sonoma). It still compiles and runs, but the Swift
  receiver should use the replacement: `CADisplayLink` (macOS 14+). The API is
  cleaner in Swift — target/selector instead of a C callback — and aligns with
  the direction for Ghostty integration.

- **XPC type checking in Swift.** `xpc_get_type()` returns `xpc_type_t`, which
  compares with global constants (`XPC_TYPE_CONNECTION`, `XPC_TYPE_DICTIONARY`).
  These should be available via bridging but may need explicit import.

- **Metal shader compilation with SPM.** SPM should compile `.metal` files
  automatically when they're in the Sources directory. If not, compile manually
  with `xcrun metal` / `xcrun metallib` and bundle alongside the binary.

- **mach_task_self.** The C macro `mach_task_self()` is not available in Swift.
  Use the global `mach_task_self_` instead.

## Experiments

### Experiment 1: Single-pane Swift receiver

#### Hypothesis

Swift can receive an IOSurface Mach port via the C XPC API, reconstruct the
IOSurface, create a Metal texture from it, and render it in a window at 60fps
using `CADisplayLink`. If this works, every Swift-specific unknown is resolved
and Experiment 2 just adds the second pane.

#### Why single-pane first

The architecture is proven from Issue 414. The question is whether Swift can do
the same work. There are five independent unknowns:

1. **XPC C API bridging** — `xpc_connection_create_mach_service()`,
   `xpc_dictionary_copy_mach_send()`, `xpc_get_type()` comparisons
2. **IOSurface reconstruction** — `IOSurfaceLookupFromMachPort()` returning
   `IOSurfaceRef?` in Swift, `mach_task_self_` instead of `mach_task_self()`
3. **Metal texture from IOSurface** —
   `device.makeTexture(descriptor:iosurface:plane:)`
4. **CADisplayLink** — replaces the deprecated `CVDisplayLink` from Issue 414
5. **SPM Metal shader compilation** — `.metal` files in `Sources/` must be
   compiled automatically by `swift build`

If any of these fail, a single-pane setup makes debugging straightforward. Once
all five work together, adding the second pane is mechanical.

#### Design

##### Project setup

Create the SPM project at `ts4/two-profiles-swift/`:

**`Package.swift`:**

```swift
// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "TwoProfilesSwift",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "Receiver",
            path: "Sources/Receiver",
            linkerSettings: [
                .linkedFramework("Cocoa"),
                .linkedFramework("Metal"),
                .linkedFramework("QuartzCore"),
                .linkedFramework("IOSurface"),
            ]
        )
    ]
)
```

Platform is `.macOS(.v14)` because `CADisplayLink` requires macOS 14+.

##### Shaders

Copy `ts4/two-profiles-receiver/shaders.metal` to
`ts4/two-profiles-swift/Sources/Receiver/Shaders.metal`. Identical shader code —
fullscreen quad triangle strip, texture sampling. SPM should compile this
automatically. If not, fall back to manual `xcrun metal` / `xcrun metallib`.

##### Module structure (single-pane)

For Experiment 1, all code lives in `main.swift` — a single file, same as Issue
414's early experiments. If this works, Experiment 2 splits into the multi-file
layout described in the issue design (AppDelegate, XPCListener, Renderer).

**`main.swift`** does everything:

1. **XPC listener** — Create Mach service `com.termsurf.two-profiles-swift` with
   `xpc_connection_create_mach_service()` and the listener flag. Accept one peer
   connection (stored in a global to prevent ARC release). Handle
   `display_surface` messages by extracting the Mach port with
   `xpc_dictionary_copy_mach_send()`.

2. **IOSurface** — `IOSurfaceLookupFromMachPort(port)` returns `IOSurfaceRef?`.
   Store the latest surface in a global, protected by `NSLock`. Deallocate the
   Mach port with `mach_port_deallocate(mach_task_self_, port)`.

3. **NSApplication + window** — Create `NSApplication`, set activation policy to
   `.regular`, create an 800x600 window (one pane). Set up `CAMetalLayer` on the
   content view with `.bgra8Unorm_sRGB`, `contentsScale` = backing scale factor.

4. **Metal pipeline** — Create device, command queue, render pipeline with sRGB
   pixel format. Load the shader library from the SPM build output. SPM places
   compiled Metal libraries in the bundle or alongside the binary — the exact
   path may need discovery (try `Bundle.main`, then fall back to
   executable-relative path).

5. **CADisplayLink** — Create via
   `NSScreen.main!.displayLink(target:selector:)`. Add to `RunLoop.main` with
   `.common` mode. On each tick, grab the latest IOSurface, create a Metal
   texture, draw the fullscreen quad.

6. **FPS logging** — Count frames per second, log to stderr with IOSurface
   dimensions. Same format as Issue 414.

##### XPC details

```swift
let queue = DispatchQueue(label: "com.termsurf.two-profiles-swift.xpc")
let listener = xpc_connection_create_mach_service(
    "com.termsurf.two-profiles-swift",
    queue,
    UInt64(XPC_CONNECTION_MACH_SERVICE_LISTENER))

// Store globally to prevent ARC release
gListener = listener

xpc_connection_set_event_handler(listener) { peer in
    guard xpc_get_type(peer) == XPC_TYPE_CONNECTION else { return }
    let peerConn = peer as xpc_connection_t
    gPeer = peerConn  // strong reference

    xpc_connection_set_event_handler(peerConn) { event in
        guard xpc_get_type(event) == XPC_TYPE_DICTIONARY else { return }
        handleMessage(event)
    }
    xpc_connection_resume(peerConn)
}
xpc_connection_resume(listener)
```

Key Swift-isms to watch:

- `XPC_CONNECTION_MACH_SERVICE_LISTENER` may need casting to `UInt64`
- `xpc_get_type()` comparison with `XPC_TYPE_CONNECTION` — these are global
  constants, should be available via bridging
- `xpc_dictionary_get_string()` returns `UnsafePointer<CChar>?` — wrap with
  `String(cString:)`

##### CADisplayLink details

```swift
let displayLink = self.window.screen!.displayLink(
    target: self, selector: #selector(render))
displayLink.add(to: .main, forMode: .common)
```

Or if using `NSScreen`:

```swift
let displayLink = NSScreen.main!.displayLink(
    target: self, selector: #selector(render))
displayLink.add(to: .main, forMode: .common)
```

The render method is called on the main thread (unlike `CVDisplayLink` which
called back on a display link thread). This simplifies threading — no lock
needed for Metal state. The IOSurface handoff from the XPC queue still needs a
lock.

##### Shader library loading

SPM compiles `.metal` files into a `.metallib` and places it in the bundle's
resource directory. For a command-line executable (no `.app` bundle), SPM may
place it alongside the binary or in a `_Resources` directory. Try:

```swift
// Option 1: Bundle.module (SPM resource bundle)
let library = try device.makeDefaultLibrary(bundle: Bundle.module)

// Option 2: Executable-relative
let execURL = URL(fileURLWithPath: CommandLine.arguments[0])
    .deletingLastPathComponent()
let libURL = execURL.appendingPathComponent("Receiver_Receiver.metallib")
let library = try device.makeLibrary(URL: libURL)

// Option 3: Default library (may work if SPM sets up the bundle correctly)
let library = device.makeDefaultLibrary()
```

This is the most likely point of friction. If none of the automatic paths work,
compile manually:

```bash
xcrun metal -c Sources/Receiver/Shaders.metal -o Shaders.air
xcrun metallib Shaders.air -o .build/debug/Shaders.metallib
```

#### Launchd plist

Use the plist from the issue design (`com.termsurf.two-profiles-swift.plist`).
The binary path points to `.build/debug/Receiver`.

#### Build and run

```bash
# Build
cd ts4/two-profiles-swift
swift build

# Load launchd plist
launchctl load ~/dev/termsurf/ts4/two-profiles-swift/com.termsurf.two-profiles-swift.plist

# Start ONE profile server (single pane)
cd ~/dev/termsurf/ts4/termsurf-chromium/src
out/Default/One\ Profile.app/Contents/MacOS/One\ Profile \
  --hidden \
  --xpc-service=com.termsurf.two-profiles-swift \
  --session-id=profile-a \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-a \
  http://localhost:9407 2>&1 &
```

#### Success criteria

- One pane showing the spinning blue square with localStorage identity
- 60fps sustained for 60+ seconds
- Pixel-perfect Retina quality (1600x1200 IOSurface in 800x600 logical window)
- sRGB color correctness (blue square matches Chrome)
- Receiver is pure Swift — no bridging header, no Objective-C files
- Builds with `swift build`
- Uses `CADisplayLink` (not deprecated `CVDisplayLink`)

#### What could go wrong

- **SPM doesn't compile `.metal` files.** Fall back to manual `xcrun metal` /
  `xcrun metallib` and load from an explicit path.
- **`XPC_CONNECTION_MACH_SERVICE_LISTENER` not visible in Swift.** May need to
  use the raw integer value (`2`).
- **`CADisplayLink` doesn't fire when added to `RunLoop.main`.** May need to be
  added inside `applicationDidFinishLaunching` after the run loop is running.
- **`IOSurfaceLookupFromMachPort` not available in Swift.** The function is in
  the IOSurface C API — should bridge automatically, but if not, a tiny C shim
  file would be needed (which would violate the "pure Swift" goal — worth
  knowing early).

#### Result: PASSED

All five Swift-specific unknowns resolved. The Swift receiver renders a single
pane at 60fps with pixel-perfect Retina quality, using `CADisplayLink` and the C
XPC API.

**Logs (excerpt):**

```
[Receiver] Listening on com.termsurf.two-profiles-swift...
[Receiver] Profile server connected
[Receiver] Loaded shaders from .build/debug/shaders.metallib
[Receiver] Window and Metal pipeline ready
[Receiver] 75 frames (74.0 fps) | IOSurface 1600x1200
[Receiver] 60 frames (60.0 fps) | IOSurface 1600x1200
[Receiver] 61 frames (60.0 fps) | IOSurface 1600x1200
[Receiver] 60 frames (59.4 fps) | IOSurface 1600x1200
...sustained 60fps for 75+ seconds...
```

**Findings:**

1. **CFRetain/CFRelease unavailable in Swift.** Swift manages `IOSurfaceRef` via
   ARC automatically. No manual retain/release needed — just assign to variables
   and let ARC handle lifetimes.

2. **Pixel format naming.** The Swift enum name is `.bgra8Unorm_srgb` (lowercase
   `srgb`), not `.bgra8Unorm_sRGB` as in Objective-C.

3. **Texture usage.** `MTLTextureUsage.shaderRead` requires the explicit type
   prefix — Swift cannot infer the type from `.shaderRead` alone in this
   context.

4. **SPM does not compile `.metal` files.** SPM warns about unhandled `.metal`
   files and does not compile them. Fix: exclude from the target in
   `Package.swift`, compile manually with `xcrun metal` / `xcrun metallib`, and
   load via `device.makeLibrary(URL:)` from an executable-relative path.

5. **XPC C API bridges cleanly.** `xpc_connection_create_mach_service()`,
   `xpc_get_type()`, `xpc_dictionary_copy_mach_send()`, and the
   `XPC_TYPE_CONNECTION` / `XPC_TYPE_DICTIONARY` / `XPC_TYPE_ERROR` constants
   all work without issues. `XPC_CONNECTION_MACH_SERVICE_LISTENER` accepts
   `UInt64()` cast.

6. **CADisplayLink works at 60fps.** Created via
   `window.screen!.displayLink(target:selector:)`, added to `.main` run loop
   with `.common` mode. Fires on the main thread, simplifying Metal state
   access.

7. **`mach_task_self_` works.** The global variable replaces the C macro
   `mach_task_self()` as expected.

8. **IOSurfaceLookupFromMachPort bridges automatically.** Returns
   `IOSurfaceRef?` (ARC-managed), no C shim needed. Pure Swift throughout.

### Experiment 2: Two-pane Swift receiver

#### Hypothesis

Adding a second pane to the Swift receiver is mechanical — no new unknowns. The
same XPC, IOSurface, and Metal patterns from Experiment 1 extend to two panes
with viewport-based compositing. If this works, Issue 415 is complete: the Swift
receiver matches Issue 414 Experiment 7 in every way.

#### Design

Modify `main.swift` to support two simultaneous profile servers, identical to
Issue 414 Experiment 7's Objective-C++ receiver. The changes are:

##### 1. Pane enum and two IOSurface slots

Replace the single `gPendingSurface` / `gCurrentTexture` with arrays indexed by
pane:

```swift
enum Pane: Int {
    case left = 0
    case right = 1
    static let count = 2
}

var gPendingSurface: [IOSurfaceRef?] = [nil, nil]
var gCurrentTexture: [MTLTexture?] = [nil, nil]
var gFrameCount: [Int] = [0, 0]
```

##### 2. Session ID → pane mapping

Map `session_id` from the XPC message to a pane index. Same logic as Issue 414:

```swift
func paneForSession(_ sessionId: String?) -> Pane {
    if sessionId == "profile-b" { return .right }
    return .left
}
```

In `handleMessage`, extract `session_id` and route the IOSurface to the correct
slot.

##### 3. Multiple peer connections

Replace `gPeer: xpc_connection_t?` with an array:

```swift
var gPeers: [xpc_connection_t] = []
```

Each incoming connection is appended to the array (strong reference prevents ARC
release). The event handler for each peer is identical — `handleMessage` routes
by `session_id`, not by connection.

##### 4. Window size: 1600x600

Two 800x600 panes side by side:

```swift
let frame = NSRect(x: 100, y: 100, width: 1600, height: 600)
```

##### 5. Two-viewport rendering

In `render()`, draw two viewports like Issue 414 Experiment 7:

```swift
let halfW = drawableW / 2.0

// Left pane
if let tex = gCurrentTexture[Pane.left.rawValue] {
    let vp = MTLViewport(originX: 0, originY: 0,
                         width: halfW, height: drawableH,
                         znear: 0, zfar: 1)
    encoder.setViewport(vp)
    encoder.setFragmentTexture(tex, index: 0)
    encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4)
}

// Right pane
if let tex = gCurrentTexture[Pane.right.rawValue] {
    let vp = MTLViewport(originX: halfW, originY: 0,
                         width: halfW, height: drawableH,
                         znear: 0, zfar: 1)
    encoder.setViewport(vp)
    encoder.setFragmentTexture(tex, index: 0)
    encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4)
}
```

##### 6. Per-pane FPS logging

Log both pane frame counts on a single line, same format as Issue 414:

```
[Receiver] L: 60 (60.0 fps) R: 61 (60.0 fps) | IOSurface 1600x1200
```

#### Build and run

```bash
# Rebuild
cd ts4/two-profiles-swift
swift build
# Shader metallib is already compiled from Experiment 1

# Reload launchd plist (unload first to pick up new binary)
launchctl unload ~/dev/termsurf/ts4/two-profiles-swift/com.termsurf.two-profiles-swift.plist
launchctl load ~/dev/termsurf/ts4/two-profiles-swift/com.termsurf.two-profiles-swift.plist

# Start TWO profile servers
cd ~/dev/termsurf/ts4/termsurf-chromium/src

out/Default/One\ Profile.app/Contents/MacOS/One\ Profile \
  --hidden \
  --xpc-service=com.termsurf.two-profiles-swift \
  --session-id=profile-a \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-a \
  http://localhost:9407 2>&1 &

out/Default/One\ Profile.app/Contents/MacOS/One\ Profile \
  --hidden \
  --xpc-service=com.termsurf.two-profiles-swift \
  --session-id=profile-b \
  --user-data-dir=$HOME/.config/termsurf/poc/profile-b \
  http://localhost:9407 2>&1 &
```

#### Success criteria

- Two panes in one window, each showing the spinning blue square
- Different localStorage identity in each pane (profile isolation)
- Both at 60fps sustained for 60+ seconds
- Pixel-perfect Retina quality (1600x1200 IOSurface, sRGB, 1:1 mapping)
- Receiver is pure Swift, builds with `swift build`
- Matches Issue 414 Experiment 7 output exactly

#### Result: PASSED

The two-pane Swift receiver matches Issue 414 Experiment 7 exactly. Two profile
servers, two panes, both at 60fps, pixel-perfect Retina, pure Swift.

**Logs (excerpt):**

```
[Receiver] Listening on com.termsurf.two-profiles-swift...
[Receiver] Profile server connected (1 total)
[Receiver] Profile server connected (2 total)
[Receiver] Loaded shaders from .build/debug/shaders.metallib
[Receiver] Window and Metal pipeline ready
[Receiver] L: 74 (73.8 fps) R: 74 (73.8 fps) | IOSurface 1600x1200
[Receiver] L: 60 (60.0 fps) R: 60 (60.0 fps) | IOSurface 1600x1200
[Receiver] L: 61 (60.0 fps) R: 61 (60.0 fps) | IOSurface 1600x1200
...sustained 60fps on both panes for 60+ seconds...
```

Issue 415 is complete. All success criteria met:

- Two panes in one window, each showing the spinning blue square
- Different localStorage identity in each pane (profile isolation confirmed)
- Both at 60fps sustained
- Pixel-perfect Retina quality (1600x1200 IOSurface, sRGB, 1:1 mapping)
- IOSurface transfer via XPC Mach ports
- Receiver written entirely in Swift — no Objective-C bridging headers
- Builds with `swift build` (SPM)
- Uses `CADisplayLink` instead of deprecated `CVDisplayLink`
- Same sender binary as Issue 414, same XPC protocol, same visual output

## Conclusion

Issue 415 is complete. The entire receiver side of the IOSurface pipeline — XPC
Mach port reception, IOSurface reconstruction, Metal texture creation, and
viewport compositing — works natively in Swift with zero performance overhead
compared to the Objective-C++ implementation from Issue 414.

### Experiment summary

| # | Name                       | Result |
| - | -------------------------- | ------ |
| 1 | Single-pane Swift receiver | PASSED |
| 2 | Two-pane Swift receiver    | PASSED |

### Key findings

1. **The C XPC API bridges to Swift without friction.** All functions —
   `xpc_connection_create_mach_service()`, `xpc_dictionary_copy_mach_send()`,
   `xpc_get_type()` — work identically. The `XPC_TYPE_*` constants and
   `XPC_CONNECTION_MACH_SERVICE_LISTENER` flag are available via automatic
   bridging. No shims, no bridging headers.

2. **IOSurface is fully usable from Swift.** `IOSurfaceLookupFromMachPort()`
   returns an ARC-managed `IOSurfaceRef?`. `IOSurfaceGetWidth()` /
   `IOSurfaceGetHeight()` work as expected. `mach_task_self_` replaces the C
   macro. No manual `CFRetain` / `CFRelease` — Swift's ARC handles everything.

3. **Metal's Swift API is cleaner than Objective-C.** Creating textures from
   IOSurfaces via `device.makeTexture(descriptor:iosurface:plane:)` is
   straightforward. The main gotcha is naming: `.bgra8Unorm_srgb` (lowercase)
   instead of `MTLPixelFormatBGRA8Unorm_sRGB`, and `MTLTextureUsage.shaderRead`
   needs the explicit type prefix.

4. **CADisplayLink is a better fit than CVDisplayLink.** It fires on the main
   thread (simplifying Metal state access), uses target/selector instead of a C
   callback, and is the non-deprecated replacement. Created via
   `window.screen!.displayLink(target:selector:)`.

5. **SPM does not compile Metal shaders.** This was the biggest surprise. SPM
   warns about `.metal` files and ignores them. The workaround is simple:
   exclude from the target in `Package.swift`, compile with `xcrun metal` /
   `xcrun metallib`, and load via `device.makeLibrary(URL:)`. For Ghostty
   integration this won't matter — Ghostty uses Zig's build system, not SPM.

6. **Zero performance difference.** Both the Objective-C++ and Swift receivers
   sustain 60fps with identical IOSurface sizes (1600x1200). The Swift receiver
   introduces no measurable overhead.

### What this means for Ghostty integration

Every API in the browser-pane pipeline is proven to work in Swift:

- **XPC** — Ghostty's Swift shell can create Mach service listeners and receive
  IOSurface Mach ports from Chromium profile servers.
- **IOSurface** — Ghostty can reconstruct IOSurfaces from Mach ports and read
  their dimensions.
- **Metal** — Ghostty can create textures from IOSurfaces and composite them
  into its existing Metal render pipeline using viewports.
- **CADisplayLink** — Ghostty already has a render loop, but if needed,
  `CADisplayLink` provides vsync-driven callbacks on the main thread.

The receiver code is ~330 lines of Swift in a single file. The patterns are
simple enough to integrate into Ghostty's existing `MetalView` or equivalent
without architectural changes. The last unknown before Ghostty integration is
removed.
