import AppKit

// MARK: Roastty Delegate

/// This implements the Roastty app delegate protocol which is used by the Roastty
/// APIs for app-global information.
extension AppDelegate: Roastty.Delegate {
    func roasttySurface(id: UUID) -> Roastty.SurfaceView? {
        for window in NSApp.windows {
            guard let controller = window.windowController as? BaseTerminalController else {
                continue
            }

            for surface in controller.surfaceTree where surface.id == id {
                return surface
            }
        }

        return nil
    }
}
