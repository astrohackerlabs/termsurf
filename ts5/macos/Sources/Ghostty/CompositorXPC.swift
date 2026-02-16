// Copyright 2025 TermSurf
// Issue 506: XPC client that connects to the xpc-gateway daemon.
// Creates an anonymous listener and registers its endpoint so `web` processes
// can connect directly to the app for overlay messages.

import Foundation
import GhosttyKit
import IOSurface
import os.log
import ServiceManagement

private let logger = Logger(subsystem: "com.termsurf.xpc-gateway", category: "xpc")

class CompositorXPC {
    static let shared = CompositorXPC()

    /// Connection to the xpc-gateway daemon (must be retained).
    private var gatewayConn: xpc_connection_t?

    /// Anonymous listener that accepts direct connections from `web` processes.
    private var anonymousListener: xpc_connection_t?

    /// Active peer connections (must be retained to prevent ARC release).
    private var peers: [xpc_connection_t] = []

    /// Maps peer connections to their pane UUID (for cleanup on disconnect).
    private var peerPaneIds: [ObjectIdentifier: UUID] = [:]

    /// Weak reference to the app delegate for surface lookup.
    private weak var appDelegate: GhosttyAppDelegate?

    /// Test IOSurface for Issue 507 (must be retained).
    private var testSurface: IOSurface?

    /// Current overlay dimensions in grid cells (to detect resize).
    private var overlayGridWidth: UInt32 = 0
    private var overlayGridHeight: UInt32 = 0

    private init() {}

    /// Connect to the xpc-gateway and register our anonymous listener endpoint.
    ///
    /// Call this once during app startup (e.g., in applicationDidFinishLaunching).
    func start(appDelegate: GhosttyAppDelegate) {
        self.appDelegate = appDelegate

        // Register the xpc-gateway LaunchAgent if not already registered.
        let gatewayService = SMAppService.agent(
            plistName: "com.termsurf.xpc-gateway.plist")
        switch gatewayService.status {
        case .notRegistered, .notFound:
            do {
                try gatewayService.register()
                fputs("[Compositor] Registered xpc-gateway LaunchAgent\n", stderr)
            } catch {
                fputs("[Compositor] Failed to register xpc-gateway: \(error)\n", stderr)
            }
        case .enabled:
            fputs("[Compositor] xpc-gateway LaunchAgent already registered\n", stderr)
        case .requiresApproval:
            fputs("[Compositor] xpc-gateway requires user approval in System Settings\n", stderr)
        @unknown default:
            break
        }

        logger.info("Connecting to xpc-gateway")

        let queue = DispatchQueue(label: "com.termsurf.compositor.xpc")

        // Step 1: Create anonymous listener for direct web connections.
        let listener = xpc_connection_create(nil, queue)
        anonymousListener = listener

        xpc_connection_set_event_handler(listener) { [weak self] peer in
            guard let self = self else { return }
            if xpc_get_type(peer) == XPC_TYPE_CONNECTION {
                let peerConn = peer as xpc_connection_t
                self.peers.append(peerConn)
                fputs("[Compositor] Web process connected (\(self.peers.count) total)\n", stderr)

                xpc_connection_set_event_handler(peerConn) { [weak self] event in
                    guard let self = self else { return }
                    if xpc_get_type(event) == XPC_TYPE_DICTIONARY {
                        self.handleMessage(event, from: peerConn)
                    } else if xpc_get_type(event) == XPC_TYPE_ERROR {
                        if event === XPC_ERROR_CONNECTION_INVALID {
                            self.handleDisconnect(peerConn)
                        } else {
                            fputs("[Compositor] XPC error\n", stderr)
                        }
                    }
                }
                xpc_connection_resume(peerConn)
            } else if xpc_get_type(peer) == XPC_TYPE_ERROR {
                fputs("[Compositor] Anonymous listener error\n", stderr)
            }
        }
        xpc_connection_resume(listener)

        // Step 2: Connect to the gateway daemon as a client.
        let gateway = xpc_connection_create_mach_service(
            "com.termsurf.xpc-gateway",
            queue,
            0)  // no LISTENER flag — we're a client

        gatewayConn = gateway

        xpc_connection_set_event_handler(gateway) { event in
            if xpc_get_type(event) == XPC_TYPE_ERROR {
                if event === XPC_ERROR_CONNECTION_INTERRUPTED {
                    fputs("[Compositor] Gateway connection interrupted\n", stderr)
                } else if event === XPC_ERROR_CONNECTION_INVALID {
                    fputs("[Compositor] Gateway connection invalid\n", stderr)
                }
            }
        }
        xpc_connection_resume(gateway)

        // Step 3: Register our anonymous listener endpoint with the gateway.
        let endpoint = xpc_endpoint_create(listener)
        let msg = xpc_dictionary_create(nil, nil, 0)
        xpc_dictionary_set_string(msg, "action", "register_app")
        xpc_dictionary_set_value(msg, "endpoint", endpoint)
        xpc_connection_send_message(gateway, msg)

        logger.info("Registered endpoint with xpc-gateway")
        fputs("[Compositor] Registered anonymous listener endpoint with xpc-gateway\n", stderr)
    }

    // MARK: - Message handling

    private func handleMessage(_ msg: xpc_object_t, from peer: xpc_connection_t) {
        guard let actionPtr = xpc_dictionary_get_string(msg, "action") else { return }
        let action = String(cString: actionPtr)

        switch action {
        case "set_overlay":
            guard let paneIdPtr = xpc_dictionary_get_string(msg, "pane_id") else {
                fputs("[Compositor] set_overlay missing pane_id\n", stderr)
                return
            }
            let paneIdStr = String(cString: paneIdPtr)
            guard let uuid = UUID(uuidString: paneIdStr) else {
                fputs("[Compositor] invalid pane_id: \(paneIdStr)\n", stderr)
                return
            }

            let col = UInt32(xpc_dictionary_get_uint64(msg, "col"))
            let row = UInt32(xpc_dictionary_get_uint64(msg, "row"))
            let width = UInt32(xpc_dictionary_get_uint64(msg, "width"))
            let height = UInt32(xpc_dictionary_get_uint64(msg, "height"))

            // Remember which pane this peer controls (for cleanup on disconnect).
            let peerId = ObjectIdentifier(peer as AnyObject)
            peerPaneIds[peerId] = uuid

            // Look up the surface and set the overlay.
            DispatchQueue.main.async { [weak self] in
                guard let self = self,
                      let surface = self.appDelegate?.findSurface(forUUID: uuid),
                      let cSurface = surface.surface else {
                    fputs("[Compositor] surface not found for pane \(paneIdStr)\n", stderr)
                    return
                }
                ghostty_surface_set_overlay(cSurface, col, row, width, height)

                // Recreate test IOSurface when overlay size changes (Experiment 2).
                if self.testSurface == nil || width != self.overlayGridWidth || height != self.overlayGridHeight {
                    var cellWidth: UInt32 = 0
                    var cellHeight: UInt32 = 0
                    ghostty_surface_get_cell_size(cSurface, &cellWidth, &cellHeight)

                    let pixelWidth = Int(width) * Int(cellWidth)
                    let pixelHeight = Int(height) * Int(cellHeight)
                    self.testSurface = Self.createCheckerboardSurface(
                        width: pixelWidth, height: pixelHeight,
                        cellWidth: Int(cellWidth), cellHeight: Int(cellHeight))
                    self.overlayGridWidth = width
                    self.overlayGridHeight = height
                    fputs("[Compositor] Created \(pixelWidth)x\(pixelHeight) checkerboard (cell \(cellWidth)x\(cellHeight))\n", stderr)
                }

                // Pass the IOSurface to the renderer (Issue 507).
                if let ioSurface = self.testSurface {
                    let ptr = Unmanaged.passUnretained(ioSurface).toOpaque()
                    ghostty_surface_set_overlay_iosurface(cSurface, ptr)
                }
            }

        default:
            fputs("[Compositor] unknown action: \(action)\n", stderr)
        }
    }

    // MARK: - Test IOSurface (Issue 507)

    /// Create a checkerboard IOSurface at the given pixel dimensions.
    /// Each checker cell is one terminal cell (cellWidth x cellHeight pixels).
    private static func createCheckerboardSurface(
        width: Int, height: Int,
        cellWidth: Int, cellHeight: Int
    ) -> IOSurface? {
        guard width > 0 && height > 0 else { return nil }
        guard let surface = IOSurface(properties: [
            .width: width,
            .height: height,
            .bytesPerElement: 4,
            .bytesPerRow: width * 4,
            .pixelFormat: 0x42475241  // 'BGRA'
        ] as [IOSurfacePropertyKey: Any]) else {
            fputs("[Compositor] Failed to create test IOSurface\n", stderr)
            return nil
        }

        surface.lock(options: [], seed: nil)
        let base = surface.baseAddress
        let bpr = surface.bytesPerRow
        for y in 0..<height {
            for x in 0..<width {
                let cellX = x / max(cellWidth, 1)
                let cellY = y / max(cellHeight, 1)
                let isLight = (cellX + cellY) % 2 == 0
                let offset = y * bpr + x * 4
                // BGRA byte order: B, G, R, A
                if isLight {
                    // Blue #4488FF → B=0xFF, G=0x88, R=0x44, A=0xFF
                    base.storeBytes(of: UInt32(0xFF_44_88_FF), toByteOffset: offset, as: UInt32.self)
                } else {
                    // Dark #222222 → B=0x22, G=0x22, R=0x22, A=0xFF
                    base.storeBytes(of: UInt32(0xFF_22_22_22), toByteOffset: offset, as: UInt32.self)
                }
            }
        }
        surface.unlock(options: [], seed: nil)

        return surface
    }

    // MARK: - Disconnect handling

    private func handleDisconnect(_ peer: xpc_connection_t) {
        fputs("[Compositor] Web process disconnected\n", stderr)

        // Remove from peers list.
        peers.removeAll { $0 === peer }

        // Clear overlay for the pane this peer was controlling.
        let peerId = ObjectIdentifier(peer as AnyObject)
        if let uuid = peerPaneIds.removeValue(forKey: peerId) {
            DispatchQueue.main.async { [weak self] in
                guard let self = self,
                      let surface = self.appDelegate?.findSurface(forUUID: uuid),
                      let cSurface = surface.surface else { return }
                ghostty_surface_clear_overlay(cSurface)
            }
        }
    }
}
