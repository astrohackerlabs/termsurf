import AppKit

extension Notification.Name {
    /// Distributed Notification for DockTilePlugin to update icon
    ///
    /// Roastty -> DockTilePlugin
    static let roasttyIconDidChange = Notification.Name("com.mitchellh.roastty.iconDidChange")
}
