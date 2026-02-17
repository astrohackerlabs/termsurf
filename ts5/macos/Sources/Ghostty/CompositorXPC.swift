// Copyright 2025 TermSurf
// Issue 506: XPC client that connects to the xpc-gateway daemon.
// Creates an anonymous listener and registers its endpoint so `web` processes
// can connect directly to the app for overlay messages.
//
// Issue 509: Server lifecycle management for Chromium Profile Server.
// Spawns servers, handles server_register/tab_ready/display_surface messages,
// imports IOSurface from Mach ports at 60fps.

import AppKit
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

    /// Maps pane UUID → current IOSurface (must retain to prevent ARC release).
    private var currentSurfaces: [UUID: IOSurface] = [:]

    /// Maps profile name → Chromium Profile Server process.
    private var serverProcesses: [String: Process] = [:]

    /// Maps profile name → server control connection (for sending create_tab).
    private var serverControlConnections: [String: xpc_connection_t] = [:]

    /// Maps pane UUID → (profile, url) pending server registration.
    private var pendingTabs: [UUID: (profile: String, url: String)] = [:]

    /// Maps pane UUID → profile name (for disconnect cleanup).
    private var paneProfiles: [UUID: String] = [:]

    /// Maps pane UUID → cached C surface pointer (for display_surface handler).
    private var cachedCSurfaces: [UUID: ghostty_surface_t] = [:]

    /// Maps pane UUID → pending pixel size for create_tab (Issue 509 Experiment 4).
    private var pendingPixelSizes: [UUID: (UInt64, UInt64)] = [:]

    /// Panes currently in browse mode (window intercepts keys).
    /// Absent or false = not browsing (keys pass through to terminal).
    private var paneBrowsing: [UUID: Bool] = [:]

    /// Maps pane UUID → web peer connection (for sending mode_changed back).
    private var webPeersForPane: [UUID: xpc_connection_t] = [:]

    /// Maps pane UUID → overlay geometry (grid coords + cell size in physical pixels).
    private var overlayGeometry: [UUID: (col: UInt32, row: UInt32,
        width: UInt32, height: UInt32, cellW: UInt32, cellH: UInt32)] = [:]

    /// Maps pane UUID → SurfaceView (for mouse hit-testing).
    private var paneSurfaceViews: [UUID: Ghostty.SurfaceView] = [:]

    /// Serial queue for all XPC state.
    private let xpcQueue = DispatchQueue(label: "com.termsurf.compositor.xpc")

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

        // Register local event monitor for Ctrl+Esc interception (Issue 513).
        // Must be registered before AppDelegate's monitor so it fires first.
        NSEvent.addLocalMonitorForEvents(matching: [.keyDown]) { [weak self] event in
            guard let self = self else { return event }

            // Only intercept Ctrl+Esc.
            guard event.keyCode == 0x35,
                  event.modifierFlags.contains(.control) else { return event }

            // Two-level focus check (from ts1 Issue 104):
            // 1. Our window must be the key window (covers inactive tabs too).
            guard let window = NSApp.keyWindow, window.isKeyWindow else { return event }
            // 2. The first responder must be a SurfaceView (the focused pane).
            guard let surfaceView = window.firstResponder
                    as? Ghostty.SurfaceView else { return event }
            let uuid = surfaceView.id

            // Check and update mode on the XPC queue (where all state lives).
            let consumed = self.xpcQueue.sync { () -> Bool in
                guard self.paneBrowsing[uuid] == true else { return false }
                self.paneBrowsing[uuid] = false
                self.sendModeChanged(paneUUID: uuid, browsing: false)
                fputs("[Compositor] Ctrl+Esc: exit browse for pane \(uuid)\n", stderr)
                return true
            }

            return consumed ? nil : event
        }

        // Register local event monitor for mouse clicks (Issue 514).
        // Routes clicks to the correct browsing pane via hit-testing.
        NSEvent.addLocalMonitorForEvents(matching: [
            .leftMouseDown, .leftMouseUp,
            .rightMouseDown, .rightMouseUp
        ]) { [weak self] event in
            guard let self = self else { return event }

            // Must have an active window.
            guard let window = event.window else { return event }

            // Hit-test against all panes with SurfaceViews.
            let windowLocation = event.locationInWindow

            // Check all browsing panes (must dispatch to XPC queue for state reads).
            let result: (uuid: UUID, relX: Double, relY: Double, scale: CGFloat)? = self.xpcQueue.sync {
                for (uuid, surfaceView) in self.paneSurfaceViews {
                    // Only intercept if the pane is in browse mode.
                    guard self.paneBrowsing[uuid] == true else { continue }

                    // Convert click to SurfaceView local coordinates.
                    let mouseInView = surfaceView.convert(windowLocation, from: nil)
                    guard surfaceView.bounds.contains(mouseInView) else { continue }

                    // Get overlay geometry for this pane.
                    guard let geo = self.overlayGeometry[uuid] else { continue }

                    // SurfaceView is NOT flipped (Y=0 at bottom). Flip to top-left origin.
                    let flippedY = surfaceView.bounds.height - mouseInView.y
                    let scale = surfaceView.window?.backingScaleFactor ?? 2.0

                    // Scale to physical pixels.
                    let physX = mouseInView.x * scale
                    let physY = flippedY * scale

                    // Compute overlay-relative physical coordinates.
                    let overlayOriginX = Double(geo.col) * Double(geo.cellW)
                    let overlayOriginY = Double(geo.row) * Double(geo.cellH)
                    let relPhysX = physX - overlayOriginX
                    let relPhysY = physY - overlayOriginY

                    // Hit test: is the click inside the overlay?
                    let overlayW = Double(geo.width) * Double(geo.cellW)
                    let overlayH = Double(geo.height) * Double(geo.cellH)
                    guard relPhysX >= 0, relPhysY >= 0,
                          relPhysX < overlayW, relPhysY < overlayH else { continue }

                    // Convert to logical pixels for Chromium.
                    let chromiumX = relPhysX / Double(scale)
                    let chromiumY = relPhysY / Double(scale)

                    return (uuid: uuid, relX: chromiumX, relY: chromiumY, scale: scale)
                }
                return nil
            }

            guard let hit = result else { return event }

            // Determine event type and button.
            let typeStr: String
            let buttonStr: String
            switch event.type {
            case .leftMouseDown:
                typeStr = "down"; buttonStr = "left"
            case .leftMouseUp:
                typeStr = "up"; buttonStr = "left"
            case .rightMouseDown:
                typeStr = "down"; buttonStr = "right"
            case .rightMouseUp:
                typeStr = "up"; buttonStr = "right"
            default:
                return event
            }

            // Map modifier flags (shift=1, ctrl=2, alt=4, cmd=8).
            var mods: UInt64 = 0
            if event.modifierFlags.contains(.shift)   { mods |= 1 }
            if event.modifierFlags.contains(.control)  { mods |= 2 }
            if event.modifierFlags.contains(.option)   { mods |= 4 }
            if event.modifierFlags.contains(.command)  { mods |= 8 }

            // Send mouse_event via XPC to the Chromium server.
            self.xpcQueue.async {
                guard let profile = self.paneProfiles[hit.uuid],
                      let controlConn = self.serverControlConnections[profile] else { return }

                let msg = xpc_dictionary_create(nil, nil, 0)
                xpc_dictionary_set_string(msg, "action", "mouse_event")
                xpc_dictionary_set_string(msg, "pane_id", hit.uuid.uuidString)
                xpc_dictionary_set_string(msg, "type", typeStr)
                xpc_dictionary_set_double(msg, "x", hit.relX)
                xpc_dictionary_set_double(msg, "y", hit.relY)
                xpc_dictionary_set_string(msg, "button", buttonStr)
                xpc_dictionary_set_int64(msg, "click_count", Int64(event.clickCount))
                xpc_dictionary_set_uint64(msg, "modifiers", mods)
                xpc_connection_send_message(controlConn, msg)
            }

            // Consume the event (prevent terminal from receiving it).
            return nil
        }

        // Step 1: Create anonymous listener for direct web connections.
        let listener = xpc_connection_create(nil, xpcQueue)
        anonymousListener = listener

        xpc_connection_set_event_handler(listener) { [weak self] peer in
            guard let self = self else { return }
            if xpc_get_type(peer) == XPC_TYPE_CONNECTION {
                let peerConn = peer as xpc_connection_t
                self.peers.append(peerConn)
                fputs("[Compositor] Peer connected (\(self.peers.count) total)\n", stderr)

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
                xpc_connection_set_target_queue(peerConn, xpcQueue)
                xpc_connection_resume(peerConn)
            } else if xpc_get_type(peer) == XPC_TYPE_ERROR {
                fputs("[Compositor] Anonymous listener error\n", stderr)
            }
        }
        xpc_connection_resume(listener)

        // Step 2: Connect to the gateway daemon as a client.
        let gateway = xpc_connection_create_mach_service(
            "com.termsurf.xpc-gateway",
            xpcQueue,
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
            handleSetOverlay(msg, from: peer)

        case "server_register":
            handleServerRegister(msg, from: peer)

        case "tab_ready":
            handleTabReady(msg, from: peer)

        case "display_surface":
            handleDisplaySurface(msg, from: peer)

        case "mode_changed":
            handleModeChanged(msg, from: peer)

        case "url_changed":
            handleUrlChanged(msg, from: peer)

        default:
            fputs("[Compositor] unknown action: \(action)\n", stderr)
        }
    }

    // MARK: - set_overlay (from web process)

    private func handleSetOverlay(_ msg: xpc_object_t, from peer: xpc_connection_t) {
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

        // Read initial browse mode and store web peer (Issue 513).
        let browsing = xpc_dictionary_get_bool(msg, "browsing")
        paneBrowsing[uuid] = browsing
        webPeersForPane[uuid] = peer

        // Check for URL field — if present, spawn or reuse Chromium server.
        let urlPtr = xpc_dictionary_get_string(msg, "url")
        if let urlPtr = urlPtr {
            let url = String(cString: urlPtr)
            let profilePtr = xpc_dictionary_get_string(msg, "profile")
            let profile = profilePtr.map { String(cString: $0) } ?? "default"

            // Track which profile this pane belongs to.
            paneProfiles[uuid] = profile

            // If this pane already has a cached surface (resize case), just update.
            if cachedCSurfaces[uuid] != nil {
                if let cSurface = cachedCSurfaces[uuid] {
                    ghostty_surface_set_overlay(cSurface, col, row, width, height)

                    var cellWidth: UInt32 = 0
                    var cellHeight: UInt32 = 0
                    ghostty_surface_get_cell_size(cSurface, &cellWidth, &cellHeight)
                    overlayGeometry[uuid] = (col: col, row: row, width: width,
                        height: height, cellW: cellWidth, cellH: cellHeight)
                    let pixelWidth = UInt64(width) * UInt64(cellWidth)
                    let pixelHeight = UInt64(height) * UInt64(cellHeight)

                    if let controlConn = serverControlConnections[profile] {
                        let msg = xpc_dictionary_create(nil, nil, 0)
                        xpc_dictionary_set_string(msg, "action", "resize")
                        xpc_dictionary_set_string(msg, "pane_id", paneIdStr)
                        xpc_dictionary_set_uint64(msg, "pixel_width", pixelWidth)
                        xpc_dictionary_set_uint64(msg, "pixel_height", pixelHeight)
                        xpc_connection_send_message(controlConn, msg)
                        fputs("[Compositor] resize \(pixelWidth)x\(pixelHeight) for pane \(paneIdStr)\n", stderr)
                    }
                }
                return
            }

            fputs("[Compositor] set_overlay with URL \(url) for pane \(paneIdStr) profile \(profile)\n", stderr)

            // Get the C surface pointer and SurfaceView from main (synchronous — safe from XPC queue).
            var cSurfaceOpt: ghostty_surface_t? = nil
            var surfaceViewOpt: Ghostty.SurfaceView? = nil
            DispatchQueue.main.sync { [weak self] in
                if let surface = self?.appDelegate?.findSurface(forUUID: uuid) {
                    cSurfaceOpt = surface.surface
                    surfaceViewOpt = surface
                }
            }

            guard let cSurface = cSurfaceOpt else {
                fputs("[Compositor] surface not found for pane \(paneIdStr)\n", stderr)
                return
            }

            // Cache the C surface pointer for display_surface handler.
            cachedCSurfaces[uuid] = cSurface

            // Cache the SurfaceView for mouse hit-testing (Issue 514).
            if let sv = surfaceViewOpt {
                paneSurfaceViews[uuid] = sv
            }

            // Set overlay grid coordinates (thread-safe via draw_mutex).
            ghostty_surface_set_overlay(cSurface, col, row, width, height)

            // Compute and store pixel dimensions for create_tab.
            var cellWidth: UInt32 = 0
            var cellHeight: UInt32 = 0
            ghostty_surface_get_cell_size(cSurface, &cellWidth, &cellHeight)
            overlayGeometry[uuid] = (col: col, row: row, width: width,
                height: height, cellW: cellWidth, cellH: cellHeight)
            let pixelWidth = UInt64(width) * UInt64(cellWidth)
            let pixelHeight = UInt64(height) * UInt64(cellHeight)
            pendingPixelSizes[uuid] = (pixelWidth, pixelHeight)

            if let controlConn = serverControlConnections[profile] {
                // Server already registered — send create_tab immediately.
                sendCreateTab(controlConn, paneId: paneIdStr, url: url, uuid: uuid)
            } else {
                // Store as pending (sent when server_register arrives).
                pendingTabs[uuid] = (profile: profile, url: url)

                if serverProcesses[profile] == nil {
                    // No server for this profile — spawn one.
                    spawnServer(forProfile: profile)
                }
                // Else: server spawned but not yet registered. pendingTabs will be
                // consumed when server_register arrives.
            }

        } else {
            // No URL — fall back to checkerboard (Issue 508 test path).
            DispatchQueue.main.async { [weak self] in
                guard let self = self,
                      let surface = self.appDelegate?.findSurface(forUUID: uuid),
                      let cSurface = surface.surface else {
                    fputs("[Compositor] surface not found for pane \(paneIdStr)\n", stderr)
                    return
                }
                ghostty_surface_set_overlay(cSurface, col, row, width, height)

                var cellWidth: UInt32 = 0
                var cellHeight: UInt32 = 0
                ghostty_surface_get_cell_size(cSurface, &cellWidth, &cellHeight)

                let pixelWidth = Int(width) * Int(cellWidth)
                let pixelHeight = Int(height) * Int(cellHeight)
                guard pixelWidth > 0 && pixelHeight > 0 else { return }

                if let existing = self.currentSurfaces[uuid],
                   IOSurfaceGetWidth(existing) == pixelWidth,
                   IOSurfaceGetHeight(existing) == pixelHeight {
                    fputs("[Compositor] Dimension cache hit, skipping rebuild\n", stderr)
                    return
                }

                guard let testSurface = IOSurface(properties: [
                    .width: pixelWidth,
                    .height: pixelHeight,
                    .bytesPerElement: 4,
                    .bytesPerRow: (pixelWidth * 4 + 15) & ~15,
                    .pixelFormat: 0x42475241  // 'BGRA'
                ] as [IOSurfacePropertyKey: Any]) else {
                    fputs("[Compositor] Failed to create IOSurface \(pixelWidth)x\(pixelHeight)\n", stderr)
                    return
                }

                testSurface.lock(options: [], seed: nil)
                let base = testSurface.baseAddress
                let bpr = testSurface.bytesPerRow
                let cw = Int(cellWidth)
                let ch = Int(cellHeight)
                for y in 0..<pixelHeight {
                    for x in 0..<pixelWidth {
                        let cellX = x / cw
                        let cellY = y / ch
                        let isLight = (cellX + cellY) % 2 == 0
                        let offset = y * bpr + x * 4
                        if isLight {
                            base.storeBytes(of: UInt32(0xFF_44_88_FF), toByteOffset: offset, as: UInt32.self)
                        } else {
                            base.storeBytes(of: UInt32(0xFF_22_22_22), toByteOffset: offset, as: UInt32.self)
                        }
                    }
                }
                testSurface.unlock(options: [], seed: nil)

                self.currentSurfaces[uuid] = testSurface
                let ptr = Unmanaged.passUnretained(testSurface).toOpaque()
                ghostty_surface_set_overlay_iosurface(cSurface, ptr)
                fputs("[Compositor] Checkerboard \(pixelWidth)x\(pixelHeight) for pane \(paneIdStr)\n", stderr)
            }
        }
    }

    // MARK: - server_register (from Chromium Profile Server)

    private func handleServerRegister(_ msg: xpc_object_t, from peer: xpc_connection_t) {
        guard let profilePtr = xpc_dictionary_get_string(msg, "profile") else {
            fputs("[Compositor] server_register missing profile\n", stderr)
            return
        }
        let profile = String(cString: profilePtr)

        fputs("[Compositor] server_register from profile \(profile)\n", stderr)

        // Store the control connection keyed by profile.
        serverControlConnections[profile] = peer

        // Flush all pending tabs for this profile.
        for (uuid, pending) in pendingTabs {
            if pending.profile == profile {
                sendCreateTab(peer, paneId: uuid.uuidString, url: pending.url, uuid: uuid)
            }
        }
        pendingTabs = pendingTabs.filter { $0.value.profile != profile }
    }

    private func sendCreateTab(_ controlConn: xpc_connection_t, paneId: String, url: String, uuid: UUID) {
        let pixelSize = pendingPixelSizes.removeValue(forKey: uuid)
        let msg = xpc_dictionary_create(nil, nil, 0)
        xpc_dictionary_set_string(msg, "action", "create_tab")
        xpc_dictionary_set_string(msg, "url", url)
        xpc_dictionary_set_string(msg, "pane_id", paneId)
        if let (pw, ph) = pixelSize {
            xpc_dictionary_set_uint64(msg, "pixel_width", pw)
            xpc_dictionary_set_uint64(msg, "pixel_height", ph)
        }
        xpc_connection_send_message(controlConn, msg)
        fputs("[Compositor] Sending create_tab url=\(url) pane_id=\(paneId) pixel=\(pixelSize?.0 ?? 0)x\(pixelSize?.1 ?? 0)\n", stderr)
    }

    // MARK: - tab_ready (from Chromium Profile Server per-tab connection)

    private func handleTabReady(_ msg: xpc_object_t, from peer: xpc_connection_t) {
        let tabIdPtr = xpc_dictionary_get_string(msg, "tab_id")
        let tabId = tabIdPtr.map { String(cString: $0) } ?? "unknown"
        fputs("[Compositor] tab_ready for tab \(tabId)\n", stderr)
    }

    // MARK: - display_surface (from Chromium Profile Server at 60fps)

    private func handleDisplaySurface(_ msg: xpc_object_t, from peer: xpc_connection_t) {
        // Extract pane_id.
        guard let paneIdPtr = xpc_dictionary_get_string(msg, "pane_id") else { return }
        let paneIdStr = String(cString: paneIdPtr)
        guard let uuid = UUID(uuidString: paneIdStr) else { return }

        // Extract IOSurface Mach port.
        let port = xpc_dictionary_copy_mach_send(msg, "iosurface_port")
        guard port != MACH_PORT_NULL else {
            fputs("[Compositor] display_surface: null Mach port\n", stderr)
            return
        }

        // Import IOSurface from Mach port.
        guard let ioSurface = IOSurfaceLookupFromMachPort(port) else {
            fputs("[Compositor] display_surface: IOSurfaceLookupFromMachPort failed\n", stderr)
            mach_port_deallocate(mach_task_self_, port)
            return
        }
        mach_port_deallocate(mach_task_self_, port)

        // Store the surface (ARC retains new, releases old).
        currentSurfaces[uuid] = ioSurface

        // Pass to the Zig renderer via the cached C surface pointer.
        guard let cSurface = cachedCSurfaces[uuid] else {
            fputs("[Compositor] display_surface: no cached cSurface for pane \(paneIdStr)\n", stderr)
            return
        }

        let ptr = Unmanaged.passUnretained(ioSurface).toOpaque()
        ghostty_surface_set_overlay_iosurface(cSurface, ptr)
    }

    // MARK: - Mode synchronization (Issue 513)

    private func handleModeChanged(_ msg: xpc_object_t, from peer: xpc_connection_t) {
        guard let paneIdPtr = xpc_dictionary_get_string(msg, "pane_id") else { return }
        let paneIdStr = String(cString: paneIdPtr)
        let browsing = xpc_dictionary_get_bool(msg, "browsing")
        guard let uuid = UUID(uuidString: paneIdStr) else { return }

        paneBrowsing[uuid] = browsing
        fputs("[Compositor] mode_changed from web: browsing=\(browsing) for pane \(paneIdStr)\n", stderr)
    }

    private func sendModeChanged(paneUUID: UUID, browsing: Bool) {
        guard let peer = webPeersForPane[paneUUID] else { return }
        let msg = xpc_dictionary_create(nil, nil, 0)
        xpc_dictionary_set_string(msg, "action", "mode_changed")
        xpc_dictionary_set_bool(msg, "browsing", browsing)
        xpc_connection_send_message(peer, msg)
    }

    // MARK: - URL synchronization (Issue 514)

    private func handleUrlChanged(_ msg: xpc_object_t, from peer: xpc_connection_t) {
        guard let paneIdPtr = xpc_dictionary_get_string(msg, "pane_id") else { return }
        let paneIdStr = String(cString: paneIdPtr)
        guard let uuid = UUID(uuidString: paneIdStr) else { return }

        guard let urlPtr = xpc_dictionary_get_string(msg, "url") else { return }
        let url = String(cString: urlPtr)

        fputs("[Compositor] url_changed: \(url) for pane \(paneIdStr)\n", stderr)

        // Forward to the web TUI peer for this pane.
        guard let webPeer = webPeersForPane[uuid] else { return }
        let fwd = xpc_dictionary_create(nil, nil, 0)
        xpc_dictionary_set_string(fwd, "action", "url_changed")
        xpc_dictionary_set_string(fwd, "url", url)
        xpc_connection_send_message(webPeer, fwd)
    }

    // MARK: - Server spawning

    private func spawnServer(forProfile profile: String) {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let serverPath = "\(home)/dev/termsurf/chromium/src/out/Default/Chromium Profile Server.app/Contents/MacOS/Chromium Profile Server"
        let profilePath = "\(home)/.config/termsurf/chromium-profiles/\(profile)"

        // Ensure profile directory exists.
        try? FileManager.default.createDirectory(
            atPath: profilePath,
            withIntermediateDirectories: true
        )

        let process = Process()
        process.executableURL = URL(fileURLWithPath: serverPath)
        process.arguments = [
            "--xpc-service=com.termsurf.xpc-gateway",
            "--user-data-dir=\(profilePath)",
            "--hidden"
        ]

        do {
            try process.run()
            serverProcesses[profile] = process
            fputs("[Compositor] Spawned server PID \(process.processIdentifier) for profile \(profile)\n", stderr)
        } catch {
            fputs("[Compositor] Failed to spawn server: \(error)\n", stderr)
        }
    }

    // MARK: - Disconnect handling

    private func handleDisconnect(_ peer: xpc_connection_t) {
        // Remove from peers list.
        peers.removeAll { $0 === peer }

        let peerId = ObjectIdentifier(peer as AnyObject)

        // Check if this is a web peer (has a pane mapping).
        if let uuid = peerPaneIds.removeValue(forKey: peerId) {
            fputs("[Compositor] Web process disconnected for pane \(uuid.uuidString)\n", stderr)

            let profile = paneProfiles.removeValue(forKey: uuid)

            // Clean up pane-level state.
            currentSurfaces.removeValue(forKey: uuid)
            pendingPixelSizes.removeValue(forKey: uuid)
            pendingTabs.removeValue(forKey: uuid)
            paneBrowsing.removeValue(forKey: uuid)
            webPeersForPane.removeValue(forKey: uuid)
            overlayGeometry.removeValue(forKey: uuid)
            paneSurfaceViews.removeValue(forKey: uuid)
            if let cSurface = cachedCSurfaces.removeValue(forKey: uuid) {
                ghostty_surface_clear_overlay(cSurface)
            }

            // If no other panes use this profile, kill the server.
            if let profile = profile {
                let otherPanesForProfile = paneProfiles.values.contains(where: { $0 == profile })
                if !otherPanesForProfile {
                    if let process = serverProcesses.removeValue(forKey: profile) {
                        process.terminate()
                        fputs("[Compositor] Terminated server PID \(process.processIdentifier) for profile \(profile)\n", stderr)
                    }
                    serverControlConnections.removeValue(forKey: profile)
                }
            }
        } else {
            // Server peer disconnected (control or tab connection) — log only.
            fputs("[Compositor] Server peer disconnected\n", stderr)
        }
    }
}
