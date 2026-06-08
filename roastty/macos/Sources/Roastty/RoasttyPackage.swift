import os
import SwiftUI
import RoasttyKit

// MARK: C Extensions

/// A command is fully self-contained so it is Sendable.
extension roastty_command_s: @unchecked @retroactive Sendable {}

/// A surface is sendable because it is just a reference type. Using the surface in parameters
/// may be unsafe but the value itself is safe to send across threads.
extension roastty_surface_t: @unchecked @retroactive Sendable {}

extension Roastty {
    // The user notification category identifier
    static let userNotificationCategory = "com.mitchellh.roastty.userNotification"

    // The user notification "Show" action
    static let userNotificationActionShow = "com.mitchellh.roastty.userNotification.Show"
}

// MARK: Build Info

extension Roastty {
    struct Info {
        var mode: roastty_build_mode_e
        var version: String
    }

    static var info: Info {
        let raw = roastty_info()
        let version = NSString(
            bytes: raw.version,
            length: Int(raw.version_len),
            encoding: NSUTF8StringEncoding
        ) ?? "unknown"

        return Info(mode: raw.build_mode, version: String(version))
    }
}

// MARK: General Helpers

extension Roastty {
    enum LaunchSource: String {
        case cli
        case app
        case zig_run
    }

    /// Returns the mechanism that launched the app. This is based on an env var so
    /// its up to the env var being set in the correct circumstance.
    static var launchSource: LaunchSource {
        guard let envValue = ProcessInfo.processInfo.environment["ROASTTY_MAC_LAUNCH_SOURCE"] else {
            // We default to the CLI because the app bundle always sets the
            // source. If its unset we assume we're in a CLI environment.
            return .cli
        }

        // If the env var is set but its unknown then we default back to the app.
        return LaunchSource(rawValue: envValue) ?? .app
    }
}

// MARK: Swift Types for C Types

extension Roastty {
    class AllocatedString {
        private let cString: roastty_string_s

        init(_ c: roastty_string_s) {
            self.cString = c
        }

        var string: String {
            guard let ptr = cString.ptr else { return "" }
            let data = Data(bytes: ptr, count: Int(cString.len))
            return String(data: data, encoding: .utf8) ?? ""
        }

        deinit {
            roastty_string_free(cString)
        }
    }
}

extension Roastty {
    enum SetFloatWIndow {
        case on
        case off
        case toggle

        static func from(_ c: roastty_action_float_window_e) -> Self? {
            switch c {
            case ROASTTY_FLOAT_WINDOW_ON:
                return .on

            case ROASTTY_FLOAT_WINDOW_OFF:
                return .off

            case ROASTTY_FLOAT_WINDOW_TOGGLE:
                return .toggle

            default:
                return nil
            }
        }
    }

    enum SetSecureInput {
        case on
        case off
        case toggle

        static func from(_ c: roastty_action_secure_input_e) -> Self? {
            switch c {
            case ROASTTY_SECURE_INPUT_ON:
                return .on

            case ROASTTY_SECURE_INPUT_OFF:
                return .off

            case ROASTTY_SECURE_INPUT_TOGGLE:
                return .toggle

            default:
                return nil
            }
        }
    }

    /// An enum that is used for the directions that a split focus event can change.
    enum SplitFocusDirection {
        case previous, next, up, down, left, right

        /// Initialize from a Roastty API enum.
        static func from(direction: roastty_action_goto_split_e) -> Self? {
            switch direction {
            case ROASTTY_GOTO_SPLIT_PREVIOUS:
                return .previous

            case ROASTTY_GOTO_SPLIT_NEXT:
                return .next

            case ROASTTY_GOTO_SPLIT_UP:
                return .up

            case ROASTTY_GOTO_SPLIT_DOWN:
                return .down

            case ROASTTY_GOTO_SPLIT_LEFT:
                return .left

            case ROASTTY_GOTO_SPLIT_RIGHT:
                return .right

            default:
                return nil
            }
        }

        func toNative() -> roastty_action_goto_split_e {
            switch self {
            case .previous:
                return ROASTTY_GOTO_SPLIT_PREVIOUS

            case .next:
                return ROASTTY_GOTO_SPLIT_NEXT

            case .up:
                return ROASTTY_GOTO_SPLIT_UP

            case .down:
                return ROASTTY_GOTO_SPLIT_DOWN

            case .left:
                return ROASTTY_GOTO_SPLIT_LEFT

            case .right:
                return ROASTTY_GOTO_SPLIT_RIGHT
            }
        }
    }

    /// Enum used for resizing splits. This is the direction the split divider will move.
    enum SplitResizeDirection {
        case up, down, left, right

        static func from(direction: roastty_action_resize_split_direction_e) -> Self? {
            switch direction {
            case ROASTTY_RESIZE_SPLIT_UP:
                return .up
            case ROASTTY_RESIZE_SPLIT_DOWN:
                return .down
            case ROASTTY_RESIZE_SPLIT_LEFT:
                return .left
            case ROASTTY_RESIZE_SPLIT_RIGHT:
                return .right
            default:
                return nil
            }
        }

        func toNative() -> roastty_action_resize_split_direction_e {
            switch self {
            case .up:
                return ROASTTY_RESIZE_SPLIT_UP
            case .down:
                return ROASTTY_RESIZE_SPLIT_DOWN
            case .left:
                return ROASTTY_RESIZE_SPLIT_LEFT
            case .right:
                return ROASTTY_RESIZE_SPLIT_RIGHT
            }
        }
    }
}

#if canImport(AppKit)
// MARK: SplitFocusDirection Extensions

extension Roastty.SplitFocusDirection {
    /// Convert to a SplitTree.FocusDirection for the given ViewType.
    func toSplitTreeFocusDirection<ViewType>() -> SplitTree<ViewType>.FocusDirection {
        switch self {
        case .previous:
            return .previous

        case .next:
            return .next

        case .up:
            return .spatial(.up)

        case .down:
            return .spatial(.down)

        case .left:
            return .spatial(.left)

        case .right:
            return .spatial(.right)
        }
    }
}
#endif

extension Roastty {
    /// The type of a clipboard request
    enum ClipboardRequest {
        /// A direct paste of clipboard contents
        case paste

        /// An application is attempting to read from the clipboard using OSC 52
        case osc_52_read

        /// An application is attempting to write to the clipboard using OSC 52
        case osc_52_write(OSPasteboard?)

        /// The text to show in the clipboard confirmation prompt for a given request type
        func text() -> String {
            switch self {
            case .paste:
                return """
                Pasting this text to the terminal may be dangerous as it looks like some commands may be executed.
                """
            case .osc_52_read:
                return """
                An application is attempting to read from the clipboard.
                The current clipboard contents are shown below.
                """
            case .osc_52_write:
                return """
                An application is attempting to write to the clipboard.
                The content to write is shown below.
                """
            }
        }

        static func from(request: roastty_clipboard_request_e) -> ClipboardRequest? {
            switch request {
            case ROASTTY_CLIPBOARD_REQUEST_PASTE:
                return .paste
            case ROASTTY_CLIPBOARD_REQUEST_OSC_52_READ:
                return .osc_52_read
            case ROASTTY_CLIPBOARD_REQUEST_OSC_52_WRITE:
                return .osc_52_write(nil)
            default:
                return nil
            }
        }
    }

    struct ClipboardContent {
        let mime: String
        let data: String

        static func from(content: roastty_clipboard_content_s) -> ClipboardContent? {
            guard let mimePtr = content.mime,
                  let dataPtr = content.data else {
                return nil
            }

            return ClipboardContent(
                mime: String(cString: mimePtr),
                data: String(cString: dataPtr)
            )
        }
    }

    /// Enum for the macos-window-buttons config option
    enum MacOSWindowButtons: String {
        case visible
        case hidden
    }

    /// Enum for the macos-titlebar-proxy-icon config option
    enum MacOSTitlebarProxyIcon: String {
        case visible
        case hidden
    }

    /// Enum for auto-update-channel config option
    enum AutoUpdateChannel: String {
        case tip
        case stable
    }
}

// MARK: Surface Notification

extension Notification.Name {
    /// Configuration change. If the object is nil then it is app-wide. Otherwise its surface-specific.
    static let roasttyConfigDidChange = Notification.Name("com.mitchellh.roastty.configDidChange")
    static let RoasttyConfigChangeKey = roasttyConfigDidChange.rawValue

    /// Color change. Object is the surface changing.
    static let roasttyColorDidChange = Notification.Name("com.mitchellh.roastty.roasttyColorDidChange")
    static let RoasttyColorChangeKey = roasttyColorDidChange.rawValue

    /// Goto tab. Has tab index in the userinfo.
    static let roasttyMoveTab = Notification.Name("com.mitchellh.roastty.moveTab")
    static let RoasttyMoveTabKey = roasttyMoveTab.rawValue

    /// Close tab
    static let roasttyCloseTab = Notification.Name("com.mitchellh.roastty.closeTab")

    /// Close other tabs
    static let roasttyCloseOtherTabs = Notification.Name("com.mitchellh.roastty.closeOtherTabs")

    /// Close tabs to the right of the focused tab
    static let roasttyCloseTabsOnTheRight = Notification.Name("com.mitchellh.roastty.closeTabsOnTheRight")

    /// Close window
    static let roasttyCloseWindow = Notification.Name("com.mitchellh.roastty.closeWindow")

    /// Resize the window to a default size.
    static let roasttyResetWindowSize = Notification.Name("com.mitchellh.roastty.resetWindowSize")

    /// Ring the bell
    static let roasttyBellDidRing = Notification.Name("com.mitchellh.roastty.roasttyBellDidRing")

    /// Readonly mode changed
    static let roasttyDidChangeReadonly = Notification.Name("com.mitchellh.roastty.didChangeReadonly")
    static let ReadonlyKey = roasttyDidChangeReadonly.rawValue + ".readonly"
    static let roasttyCommandPaletteDidToggle = Notification.Name("com.mitchellh.roastty.commandPaletteDidToggle")

    /// Toggle maximize of current window
    static let roasttyMaximizeDidToggle = Notification.Name("com.mitchellh.roastty.maximizeDidToggle")

    /// Notification sent when scrollbar updates
    static let roasttyDidUpdateScrollbar = Notification.Name("com.mitchellh.roastty.didUpdateScrollbar")
    static let ScrollbarKey = roasttyDidUpdateScrollbar.rawValue + ".scrollbar"

    /// Focus the search field
    static let roasttySearchFocus = Notification.Name("com.mitchellh.roastty.searchFocus")
}

// NOTE: I am moving all of these to Notification.Name extensions over time. This
// namespace was the old namespace.
extension Roastty.Notification {
    /// Used to pass a configuration along when creating a new tab/window/split.
    static let NewSurfaceConfigKey = "com.mitchellh.roastty.newSurfaceConfig"

    /// Posted when a new split is requested. The sending object will be the surface that had focus. The
    /// userdata has one key "direction" with the direction to split to.
    static let roasttyNewSplit = Notification.Name("com.mitchellh.roastty.newSplit")

    /// Close the calling surface.
    static let roasttyCloseSurface = Notification.Name("com.mitchellh.roastty.closeSurface")

    /// Focus previous/next split. Has a SplitFocusDirection in the userinfo.
    static let roasttyFocusSplit = Notification.Name("com.mitchellh.roastty.focusSplit")
    static let SplitDirectionKey = roasttyFocusSplit.rawValue

    /// Goto tab. Has tab index in the userinfo.
    static let roasttyGotoTab = Notification.Name("com.mitchellh.roastty.gotoTab")
    static let GotoTabKey = roasttyGotoTab.rawValue

    /// New tab. Has base surface config requested in userinfo.
    static let roasttyNewTab = Notification.Name("com.mitchellh.roastty.newTab")

    /// New window. Has base surface config requested in userinfo.
    static let roasttyNewWindow = Notification.Name("com.mitchellh.roastty.newWindow")

    /// Present terminal. Bring the surface's window to focus without activating the app.
    static let roasttyPresentTerminal = Notification.Name("com.mitchellh.roastty.presentTerminal")

    /// Toggle fullscreen of current window
    static let roasttyToggleFullscreen = Notification.Name("com.mitchellh.roastty.toggleFullscreen")
    static let FullscreenModeKey = roasttyToggleFullscreen.rawValue

    /// Notification sent to toggle split maximize/unmaximize.
    static let didToggleSplitZoom = Notification.Name("com.mitchellh.roastty.didToggleSplitZoom")

    /// Notification
    static let didReceiveInitialWindowFrame = Notification.Name("com.mitchellh.roastty.didReceiveInitialWindowFrame")
    static let FrameKey = "com.mitchellh.roastty.frame"

    /// Notification to render the inspector for a surface
    static let inspectorNeedsDisplay = Notification.Name("com.mitchellh.roastty.inspectorNeedsDisplay")

    /// Notification to show/hide the inspector
    static let didControlInspector = Notification.Name("com.mitchellh.roastty.didControlInspector")

    static let confirmClipboard = Notification.Name("com.mitchellh.roastty.confirmClipboard")
    static let ConfirmClipboardStrKey = confirmClipboard.rawValue + ".str"
    static let ConfirmClipboardStateKey = confirmClipboard.rawValue + ".state"
    static let ConfirmClipboardRequestKey = confirmClipboard.rawValue + ".request"

    /// Notification sent to the active split view to resize the split.
    static let didResizeSplit = Notification.Name("com.mitchellh.roastty.didResizeSplit")
    static let ResizeSplitDirectionKey = didResizeSplit.rawValue + ".direction"
    static let ResizeSplitAmountKey = didResizeSplit.rawValue + ".amount"

    /// Notification sent to the split root to equalize split sizes
    static let didEqualizeSplits = Notification.Name("com.mitchellh.roastty.didEqualizeSplits")

    /// Notification that renderer health changed
    static let didUpdateRendererHealth = Notification.Name("com.mitchellh.roastty.didUpdateRendererHealth")

    /// Notifications related to key sequences
    static let didContinueKeySequence = Notification.Name("com.mitchellh.roastty.didContinueKeySequence")
    static let didEndKeySequence = Notification.Name("com.mitchellh.roastty.didEndKeySequence")
    static let KeySequenceKey = didContinueKeySequence.rawValue + ".key"

    /// Notifications related to key tables
    static let didChangeKeyTable = Notification.Name("com.mitchellh.roastty.didChangeKeyTable")
    static let KeyTableKey = didChangeKeyTable.rawValue + ".action"
}

// Make the input enum hashable.
extension roastty_input_key_e: @retroactive Hashable {}
