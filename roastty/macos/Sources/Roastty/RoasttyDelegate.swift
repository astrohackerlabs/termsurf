import Foundation

extension Roastty {
    /// This is a delegate that should be applied to your global app delegate for RoasttyKit
    /// to perform app-global operations.
    protocol Delegate {
        /// Look up a surface within the application by ID.
        func roasttySurface(id: UUID) -> SurfaceView?
    }
}
