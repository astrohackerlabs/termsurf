import Cocoa
import CoreGraphics
import Carbon
import OSLog
import RoasttyKit

// Manages the event tap to monitor global events, currently only used for
// global keybindings.
class GlobalEventTap {
    static let shared = GlobalEventTap()

    struct InstalledTap {
        var invalidate: () -> Void
        var setEnabled: (Bool) -> Void
    }

    protocol RetryTimer: AnyObject {
        func invalidate()
    }

    struct Dependencies {
        var createEventTap: (CGEventMask) -> InstalledTap?
        var scheduleRetry: (@escaping () -> Void) -> RetryTimer

        static let live = Dependencies(
            createEventTap: { eventMask in
                // Try to create it
                guard let eventTap = CGEvent.tapCreate(
                    tap: .cgSessionEventTap,
                    place: .headInsertEventTap,
                    options: .defaultTap,
                    eventsOfInterest: eventMask,
                    callback: cgEventFlagsChangedHandler(proxy:type:cgEvent:userInfo:),
                    userInfo: nil
                ) else {
                    return nil
                }

                // Attach our event tap to the main run loop. Note if you don't
                // do this then the event tap will block every
                CFRunLoopAddSource(
                    CFRunLoopGetMain(),
                    CFMachPortCreateRunLoopSource(nil, eventTap, 0),
                    .commonModes
                )

                return InstalledTap(
                    invalidate: { CFMachPortInvalidate(eventTap) },
                    setEnabled: { CGEvent.tapEnable(tap: eventTap, enable: $0) }
                )
            },
            scheduleRetry: { retry in
                Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { _ in
                    retry()
                }
            }
        )
    }

    fileprivate static let logger = Logger(
        subsystem: Bundle.main.bundleIdentifier!,
        category: String(describing: GlobalEventTap.self)
    )

    // The event tap used for global event listening. This is non-nil if it is
    // created.
    fileprivate var eventTap: InstalledTap?

    // This is the timer used to retry enabling the global event tap if we
    // don't have permissions.
    private var enableTimer: RetryTimer?

    private let dependencies: Dependencies

    var isEventTapInstalled: Bool { eventTap != nil }
    var isRetryPending: Bool { enableTimer != nil }

    // Internal init so hosted tests can inject non-permissioned dependencies.
    init(dependencies: Dependencies = .live) {
        self.dependencies = dependencies
    }

    deinit {
        disable()
    }

    // Enable the global event tap. This is safe to call if it is already enabled.
    // If enabling fails due to permissions, this will start a timer to retry since
    // accessibility permissions take affect immediately.
    func enable() {
        if eventTap != nil {
            // Already enabled
            return
        }

        // If we are already trying to enable, then stop the timer and restart it.
        if let enableTimer {
            enableTimer.invalidate()
            self.enableTimer = nil
        }

        // Try to enable the event tap immediately. If this succeeds then we're done!
        if tryEnable() {
            return
        }

        // Failed, probably due to permissions. The permissions dialog should've
        // popped up. We retry on a timer since once the permissions are granted
        // then they take affect immediately.
        enableTimer = dependencies.scheduleRetry {
            _ = self.tryEnable()
        }
    }

    // Disable the global event tap. This is safe to call if it is already disabled.
    func disable() {
        // Stop our enable timer if it is on
        if let enableTimer {
            enableTimer.invalidate()
            self.enableTimer = nil
        }

        // Stop our event tap
        if let eventTap {
            Self.logger.debug("invalidating event tap mach port")
            eventTap.invalidate()
            self.eventTap = nil
        }
    }

    // Try to enable the global event type, returns false if it fails.
    private func tryEnable() -> Bool {
        // The events we care about
        let eventMask = [
            CGEventType.keyDown
        ].reduce(CGEventMask(0), { $0 | (1 << $1.rawValue)})

        guard let eventTap = dependencies.createEventTap(eventMask) else {
            // Return false if creation failed. This is usually because we don't have
            // Accessibility permissions but can probably be other reasons I don't
            // know about.
            Self.logger.debug("creating global event tap failed, missing permissions?")
            return false
        }

        // Store our event tap
        self.eventTap = eventTap

        // If we have an enable timer we always want to disable it
        if let enableTimer {
            enableTimer.invalidate()
            self.enableTimer = nil
        }

        Self.logger.info("global event tap enabled for global keybinds")
        return true
    }
}

extension Timer: GlobalEventTap.RetryTimer {}

func globalEventTapHandleKeyEvent(
    type: CGEventType,
    cgEvent: CGEvent,
    appIsActive: Bool,
    roastty: roastty_app_t?
) -> Bool {
    guard type == .keyDown else { return false }
    guard !appIsActive else { return false }
    guard let roastty else { return false }
    guard let event: NSEvent = .init(cgEvent: cgEvent) else { return false }

    let key_ev = event.roasttyKeyEvent(ROASTTY_ACTION_PRESS)
    if roastty_app_key(roastty, key_ev) {
        GlobalEventTap.logger.info("global key event handled event=\(event)")
        return true
    }

    return false
}

private func cgEventFlagsChangedHandler(
    proxy: CGEventTapProxy,
    type: CGEventType,
    cgEvent: CGEvent,
    userInfo: UnsafeMutableRawPointer?
) -> Unmanaged<CGEvent>? {
    let result = Unmanaged.passUnretained(cgEvent)

    // macOS disables the event tap if the callback is too slow or for other
    // internal reasons. When that happens it sends this event type. We need
    // to re-enable the tap or it stays dead forever.
    if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
        GlobalEventTap.logger.warning("global event tap was disabled by the system, re-enabling")
        if let eventTap = GlobalEventTap.shared.eventTap {
            eventTap.setEnabled(true)
        }
        return result
    }

    // We need an app delegate to get the Roastty app instance
    let appDelegate = NSApplication.shared.delegate as? AppDelegate
    if globalEventTapHandleKeyEvent(
        type: type,
        cgEvent: cgEvent,
        appIsActive: NSApp.isActive,
        roastty: appDelegate?.roastty.app
    ) {
        return nil
    }

    return result
}
