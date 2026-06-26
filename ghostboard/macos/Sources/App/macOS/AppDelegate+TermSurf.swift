import AppKit
import Darwin
import GhosttyKit

func termsurfGeometryTraceEnabled() -> Bool {
    guard let value = ProcessInfo.processInfo.environment["TERMSURF_GEOMETRY_TRACE"] else { return false }
    return value != "0" && value.lowercased() != "false"
}

func termsurfGeometryScenario() -> String {
    ProcessInfo.processInfo.environment["TERMSURF_GEOMETRY_SCENARIO"] ?? "unknown"
}

func termsurfGeometryIdentity(
    paneID: String,
    browserTabID: String = "unknown:appkit-bridge",
    surfaceID: String = "unknown:bridge-before-surface",
    windowID: String = "unknown:bridge-before-surface",
    selectedTabID: String = "unknown:bridge-before-surface"
) -> String {
    "window_id:\(windowID) surface_id:\(surfaceID) selected_tab_id:\(selectedTabID) pane_id:\(paneID) browser_tab_id:\(browserTabID)"
}

func termsurfLogGeometry(_ message: String) {
    guard termsurfGeometryTraceEnabled() else { return }
    let line = "TermSurf geometry \(message)"
    AppDelegate.logger.info("\(line)")
    fputs("\(line)\n", stderr)
}

func termsurfRunActivationProbeIfRequested() {
    let environment = ProcessInfo.processInfo.environment
    guard environment["TERMSURF_ISSUE834_GHOSTBOARD_ACTIVATION_PROBE"] != nil else {
        return
    }

    let row = environment["TERMSURF_ISSUE834_ACTIVATION_PROBE_ROW"] ?? "ghostboard-gui"
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
        termsurfPublishActivationProbePidIfRequested(process: "ghostboard")
        guard termsurfWaitForActivationProbeWatcherIfRequested() else {
            termsurfTraceActivationProbe(row: row, phase: "refuse", attempts: "watcher-not-ready")
            return
        }

        let attempts = "setActivationPolicyRegular,activateIgnoringOtherApps"
        termsurfTraceActivationProbe(row: row, phase: "before-activation", attempts: "none")
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
        NSApp.unhide(nil)
        NSApp.arrangeInFront(nil)
        termsurfTraceActivationProbe(row: row, phase: "after-activation", attempts: attempts)

        let alert = NSAlert()
        alert.messageText = "TermSurf Issue 834 Activation Probe"
        alert.informativeText = row
        alert.addButton(withTitle: "OK")
        alert.window.title = "TermSurf Issue 834 Activation Probe"
        termsurfTraceActivationProbe(row: row, phase: "before-alert", attempts: attempts)
        let response = alert.runModal()
        termsurfTraceActivationProbe(
            row: row,
            phase: "after-alert",
            attempts: "\(attempts),response:\(response.rawValue)")
    }
}

private func termsurfPublishActivationProbePidIfRequested(process: String) {
    let environment = ProcessInfo.processInfo.environment
    guard let path = environment["TERMSURF_ISSUE834_ACTIVATION_PROBE_TRACKED_PIDS_FILE"], !path.isEmpty else {
        return
    }

    let url = URL(fileURLWithPath: path)
    try? FileManager.default.createDirectory(
        at: url.deletingLastPathComponent(),
        withIntermediateDirectories: true)
    let text = "\(process) \(getpid())\n"
    try? Data(text.utf8).write(to: url)
}

private func termsurfWaitForActivationProbeWatcherIfRequested() -> Bool {
    let environment = ProcessInfo.processInfo.environment
    guard let path = environment["TERMSURF_ISSUE834_ACTIVATION_PROBE_WATCHER_READY_FILE"], !path.isEmpty else {
        return true
    }

    for _ in 0..<100 {
        if FileManager.default.fileExists(atPath: path) {
            return true
        }
        RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.1))
    }
    return false
}

private func termsurfTraceActivationProbe(row: String, phase: String, attempts: String) {
    let environment = ProcessInfo.processInfo.environment
    guard let path = environment["TERMSURF_ISSUE834_ACTIVATION_PROBE_TRACE_FILE"], !path.isEmpty else {
        return
    }

    let line = "termsurf-activation-probe process=ghostboard row=\(row) phase=\(phase) \(termsurfActivationProbeState(attempts: attempts))"
    let url = URL(fileURLWithPath: path)
    try? FileManager.default.createDirectory(
        at: url.deletingLastPathComponent(),
        withIntermediateDirectories: true)
    let data = Data("\(line)\n".utf8)
    if FileManager.default.fileExists(atPath: path),
       let handle = try? FileHandle(forWritingTo: url) {
        _ = try? handle.seekToEnd()
        _ = try? handle.write(contentsOf: data)
        _ = try? handle.close()
    } else {
        try? data.write(to: url)
    }
    AppDelegate.logger.info("\(line)")
    fputs("\(line)\n", stderr)
}

private func termsurfActivationProbeState(attempts: String) -> String {
    let bundleID = Bundle.main.bundleIdentifier ?? ""
    return [
        "pid=\(getpid())",
        "ppid=\(getppid())",
        "bundle_id=\(bundleID)",
        "executable=\(Bundle.main.executablePath ?? "")",
        "activation_policy=\(NSApp.activationPolicy().rawValue)",
        "app_active=\(NSApp.isActive ? 1 : 0)",
        "key_window=\(termsurfDescribeWindow(NSApp.keyWindow))",
        "main_window=\(termsurfDescribeWindow(NSApp.mainWindow))",
        "attempts=\(attempts)",
        "windows={\(NSApp.windows.map(termsurfDescribeWindow).joined(separator: "|"))}",
    ].joined(separator: " ")
}

private func termsurfDescribeWindow(_ window: NSWindow?) -> String {
    guard let window else { return "nil" }
    return "\(type(of: window)):\(Unmanaged.passUnretained(window).toOpaque())" +
        ":title={\(window.title)}" +
        ":visible=\(window.isVisible ? 1 : 0)" +
        ":key=\(window.isKeyWindow ? 1 : 0)" +
        ":main=\(window.isMainWindow ? 1 : 0)" +
        ":can_key=\(window.canBecomeKey ? 1 : 0)" +
        ":can_main=\(window.canBecomeMain ? 1 : 0)"
}

@_cdecl("termsurf_clear_overlay")
func termsurf_clear_overlay(_ paneIDPointer: UnsafePointer<CChar>?) {
    guard let paneIDPointer else {
        termsurfLogOverlay("TermSurf overlay clear rejected: missing pane id")
        return
    }

    let paneID = String(cString: paneIDPointer)
    termsurfLogOverlay("TermSurf overlay clear request pane_id=\(paneID)")
    termsurfLogGeometry(
        "layer=bridge event=clear_request scenario=\(termsurfGeometryScenario()) identity=\(termsurfGeometryIdentity(paneID: paneID)) visible=false note=received-zig-clear")

    DispatchQueue.main.async {
        guard let appDelegate = NSApplication.shared.delegate as? AppDelegate else {
            termsurfLogOverlay("TermSurf overlay clear rejected: missing app delegate")
            termsurfLogGeometry(
                "layer=bridge event=clear_rejected scenario=\(termsurfGeometryScenario()) identity=\(termsurfGeometryIdentity(paneID: paneID)) visible=unknown note=missing-app-delegate")
            return
        }
        guard let uuid = UUID(uuidString: paneID) else {
            termsurfLogOverlay("TermSurf overlay clear rejected: invalid pane id \(paneID)")
            termsurfLogGeometry(
                "layer=bridge event=clear_rejected scenario=\(termsurfGeometryScenario()) identity=\(termsurfGeometryIdentity(paneID: paneID)) visible=unknown note=invalid-pane-id")
            return
        }
        guard let target = appDelegate.findSurface(forUUID: uuid) else {
            termsurfLogOverlay("TermSurf overlay clear rejected: no surface for pane id \(paneID)")
            termsurfLogGeometry(
                "layer=bridge event=clear_rejected scenario=\(termsurfGeometryScenario()) identity=\(termsurfGeometryIdentity(paneID: paneID)) visible=unknown note=no-surface")
            return
        }
        termsurfLogGeometry(
            "layer=bridge event=clear_target_found scenario=\(termsurfGeometryScenario()) identity=\(target.termSurfGeometryIdentity(browserTabID: "unknown:zig-clear")) visible=false note=dispatching-clear-to-surface")

        target.clearTermSurfOverlay()
    }
}

@_cdecl("termsurf_set_cursor")
func termsurf_set_cursor(_ paneIDPointer: UnsafePointer<CChar>?, _ cursorType: Int64) {
    guard let paneIDPointer else {
        termsurfLogOverlay("TermSurf cursor rejected: missing pane id")
        return
    }

    let paneID = String(cString: paneIDPointer)
    termsurfLogOverlay("TermSurf cursor request pane_id=\(paneID) cursor_type=\(cursorType)")

    DispatchQueue.main.async {
        guard let appDelegate = NSApplication.shared.delegate as? AppDelegate else {
            termsurfLogOverlay("TermSurf cursor rejected: missing app delegate")
            return
        }
        guard let uuid = UUID(uuidString: paneID) else {
            termsurfLogOverlay("TermSurf cursor rejected: invalid pane id \(paneID)")
            return
        }
        guard let target = appDelegate.findSurface(forUUID: uuid) else {
            termsurfLogOverlay("TermSurf cursor rejected: no surface for pane id \(paneID)")
            return
        }

        target.setTermSurfCursor(type: cursorType)
    }
}

@_cdecl("termsurf_set_title")
func termsurf_set_title(
    _ paneIDPointer: UnsafePointer<CChar>?,
    _ titlePointer: UnsafePointer<CChar>?
) {
    guard let paneIDPointer, let titlePointer else {
        termsurfLogOverlay("TermSurf title rejected: missing C string")
        return
    }

    let paneID = String(cString: paneIDPointer)
    let title = String(cString: titlePointer)
    termsurfLogOverlay("TermSurf title request pane_id=\(paneID) title=\(title)")

    DispatchQueue.main.async {
        guard let appDelegate = NSApplication.shared.delegate as? AppDelegate else {
            termsurfLogOverlay("TermSurf title rejected: missing app delegate")
            return
        }
        guard let uuid = UUID(uuidString: paneID) else {
            termsurfLogOverlay("TermSurf title rejected: invalid pane id \(paneID)")
            return
        }
        guard let target = appDelegate.findSurface(forUUID: uuid) else {
            termsurfLogOverlay("TermSurf title rejected: no surface for pane id \(paneID)")
            return
        }

        target.setTitle(title)
        termsurfLogOverlay("TermSurf title applied pane_id=\(paneID) title=\(title)")
    }
}

@_cdecl("termsurf_present_overlay")
// swiftlint:disable:next function_parameter_count
func termsurf_present_overlay(
    _ paneIDPointer: UnsafePointer<CChar>?,
    _ contextID: UInt64,
    _ col: UInt64,
    _ row: UInt64,
    _ width: UInt64,
    _ height: UInt64,
    _ pixelWidth: UInt64,
    _ pixelHeight: UInt64
) {
    guard let paneIDPointer else {
        termsurfLogOverlay("TermSurf overlay rejected: missing pane id")
        return
    }

    let paneID = String(cString: paneIDPointer)
    termsurfLogOverlay(
        "TermSurf overlay request pane_id=\(paneID) context_id=\(contextID) grid=\(width)x\(height)+\(col)+\(row) pixel=\(pixelWidth)x\(pixelHeight)")
    termsurfLogGeometry(
        "layer=bridge event=present_request scenario=\(termsurfGeometryScenario()) identity=\(termsurfGeometryIdentity(paneID: paneID)) grid=\(width)x\(height)+\(col)+\(row) browser_pixel=\(pixelWidth)x\(pixelHeight) context_id=\(contextID) visible=unknown note=received-zig-present")

    DispatchQueue.main.async {
        guard let appDelegate = NSApplication.shared.delegate as? AppDelegate else {
            termsurfLogOverlay("TermSurf overlay rejected: missing app delegate")
            termsurfLogGeometry(
                "layer=bridge event=present_rejected scenario=\(termsurfGeometryScenario()) identity=\(termsurfGeometryIdentity(paneID: paneID)) grid=\(width)x\(height)+\(col)+\(row) browser_pixel=\(pixelWidth)x\(pixelHeight) context_id=\(contextID) visible=unknown note=missing-app-delegate")
            return
        }
        guard let uuid = UUID(uuidString: paneID) else {
            termsurfLogOverlay("TermSurf overlay rejected: invalid pane id \(paneID)")
            termsurfLogGeometry(
                "layer=bridge event=present_rejected scenario=\(termsurfGeometryScenario()) identity=\(termsurfGeometryIdentity(paneID: paneID)) grid=\(width)x\(height)+\(col)+\(row) browser_pixel=\(pixelWidth)x\(pixelHeight) context_id=\(contextID) visible=unknown note=invalid-pane-id")
            return
        }
        guard let target = appDelegate.findSurface(forUUID: uuid) else {
            termsurfLogOverlay("TermSurf overlay rejected: no surface for pane id \(paneID)")
            termsurfLogGeometry(
                "layer=bridge event=present_rejected scenario=\(termsurfGeometryScenario()) identity=\(termsurfGeometryIdentity(paneID: paneID)) grid=\(width)x\(height)+\(col)+\(row) browser_pixel=\(pixelWidth)x\(pixelHeight) context_id=\(contextID) visible=unknown note=no-surface")
            return
        }
        termsurfLogGeometry(
            "layer=bridge event=present_target_found scenario=\(termsurfGeometryScenario()) identity=\(target.termSurfGeometryIdentity(browserTabID: "unknown:zig-present")) grid=\(width)x\(height)+\(col)+\(row) browser_pixel=\(pixelWidth)x\(pixelHeight) context_id=\(contextID) visible=unknown note=dispatching-present-to-surface")

        target.presentTermSurfOverlay(
            contextID: contextID,
            col: col,
            row: row,
            width: width,
            height: height,
            pixelWidth: pixelWidth,
            pixelHeight: pixelHeight)
    }
}

@_cdecl("termsurf_open_split")
func termsurf_open_split(
    _ paneIDPointer: UnsafePointer<CChar>?,
    _ directionPointer: UnsafePointer<CChar>?,
    _ commandPointer: UnsafePointer<CChar>?
) {
    guard let paneIDPointer, let directionPointer, let commandPointer else {
        termsurfLogOpenSplit("TermSurf OpenSplit rejected: missing C string")
        return
    }

    let paneID = String(cString: paneIDPointer)
    let direction = String(cString: directionPointer)
    let command = String(cString: commandPointer)

    termsurfLogOpenSplit("TermSurf OpenSplit request pane_id=\(paneID) direction=\(direction)")

    DispatchQueue.main.async {
        guard let appDelegate = NSApplication.shared.delegate as? AppDelegate else {
            termsurfLogOpenSplit("TermSurf OpenSplit rejected: missing app delegate")
            return
        }
        guard let uuid = UUID(uuidString: paneID) else {
            termsurfLogOpenSplit("TermSurf OpenSplit rejected: invalid pane id \(paneID)")
            return
        }
        guard let target = appDelegate.findSurface(forUUID: uuid) else {
            termsurfLogOpenSplit("TermSurf OpenSplit rejected: no surface for pane id \(paneID)")
            return
        }
        guard let splitDirection = termsurfSplitDirection(direction) else {
            termsurfLogOpenSplit("TermSurf OpenSplit rejected: invalid direction \(direction)")
            return
        }
        guard let surface = target.surface else {
            termsurfLogOpenSplit("TermSurf OpenSplit rejected: target surface is unavailable")
            return
        }
        guard let controller = target.window?.windowController as? BaseTerminalController else {
            termsurfLogOpenSplit("TermSurf OpenSplit rejected: target has no terminal controller")
            return
        }

        var config = Ghostty.SurfaceConfiguration(
            from: ghostty_surface_inherited_config(surface, GHOSTTY_SURFACE_CONTEXT_SPLIT))
        config.command = command

        guard controller.newSplit(at: target, direction: splitDirection, baseConfig: config) != nil else {
            termsurfLogOpenSplit("TermSurf OpenSplit rejected: split creation failed")
            return
        }

        termsurfLogOpenSplit("TermSurf OpenSplit created split pane_id=\(paneID) direction=\(direction)")
    }
}

private func termsurfLogOpenSplit(_ message: String) {
    AppDelegate.logger.info("\(message)")
    fputs("\(message)\n", stderr)
}

private func termsurfLogOverlay(_ message: String) {
    AppDelegate.logger.info("\(message)")
    fputs("\(message)\n", stderr)
}

private func termsurfSplitDirection(_ direction: String) -> SplitTree<Ghostty.SurfaceView>.NewDirection? {
    switch direction {
    case "right":
        return .right
    case "left":
        return .left
    case "down":
        return .down
    case "up":
        return .up
    default:
        return nil
    }
}
