import Cocoa
import RoasttyKit

extension Roastty {
    /// A comparable event.
    struct ComparableKeyEvent: Equatable {
        let keyCode: UInt16
        let flags: NSEvent.ModifierFlags

        init(event: NSEvent) {
            self.keyCode = event.keyCode
            self.flags = event.modifierFlags
        }
    }
}
