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

- **CVDisplayLink in Swift.** `CVDisplayLink` is deprecated in macOS 15. Swift
  may require a different approach — `CADisplayLink` (macOS 14+) or
  `NSView.displayLink(target:selector:)`. If `CVDisplayLink` works, use it for
  consistency with Issue 414. If not, try `CADisplayLink`.

- **XPC type checking in Swift.** `xpc_get_type()` returns `xpc_type_t`, which
  compares with global constants (`XPC_TYPE_CONNECTION`, `XPC_TYPE_DICTIONARY`).
  These should be available via bridging but may need explicit import.

- **Metal shader compilation with SPM.** SPM should compile `.metal` files
  automatically when they're in the Sources directory. If not, compile manually
  with `xcrun metal` / `xcrun metallib` and bundle alongside the binary.

- **mach_task_self.** The C macro `mach_task_self()` is not available in Swift.
  Use the global `mach_task_self_` instead.

## What this unlocks

A working Swift receiver proves that every API in the pipeline (XPC, IOSurface,
Metal) works natively in Swift. This removes the last unknown before Ghostty
integration — we know the receiver code can be written in the same language as
Ghostty's macOS shell, using the same Metal rendering patterns.
