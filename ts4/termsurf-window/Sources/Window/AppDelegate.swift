import AppKit
import IOSurface
import XIPC

class AppDelegate: NSObject, NSApplicationDelegate {
    var window: NSWindow!
    var metalView: MetalView!

    func applicationDidFinishLaunching(_ notification: Notification) {
        let windowRect = NSRect(x: 100, y: 100, width: 800, height: 600)
        window = NSWindow(
            contentRect: windowRect,
            styleMask: [.titled, .closable, .resizable, .miniaturizable],
            backing: .buffered,
            defer: false
        )
        window.title = "TermSurf"
        window.minSize = NSSize(width: 400, height: 300)

        metalView = MetalView(frame: windowRect)
        window.contentView = metalView

        window.center()
        window.makeKeyAndOrderFront(nil)

        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)

        let context = Unmanaged.passUnretained(self).toOpaque()
        let callback: xipc_frame_callback = { port, width, height, context in
            guard let context = context else { return }
            let delegate = Unmanaged<AppDelegate>.fromOpaque(context).takeUnretainedValue()
            delegate.handleFrame(port: port, width: width, height: height)
        }

        // Connect to the Rust terminal service (blue)
        NSLog("[Window] Connecting to com.termsurf.ts4.terminal")
        xipc_connect("com.termsurf.ts4.terminal", callback, context)

        // Connect to the C++ browser service (green)
        NSLog("[Window] Connecting to com.termsurf.ts4.browser")
        xipc_connect("com.termsurf.ts4.browser", callback, context)
    }

    private func handleFrame(port: mach_port_t, width: UInt32, height: UInt32) {
        NSLog("[Window] Received frame: port=%u, %ux%u", port, width, height)

        guard let surfacePtr = xipc_import_iosurface(port) else {
            NSLog("[Window] Failed to import IOSurface")
            return
        }

        // IOSurfaceLookupFromMachPort returns +1 retained reference
        let ioSurface = Unmanaged<IOSurface>.fromOpaque(surfacePtr).takeRetainedValue()

        NSLog("[Window] IOSurface imported: %dx%d",
              IOSurfaceGetWidth(ioSurface), IOSurfaceGetHeight(ioSurface))

        metalView.setExternalSurface(ioSurface)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        return true
    }
}
