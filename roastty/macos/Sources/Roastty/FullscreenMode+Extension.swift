import RoasttyKit

extension FullscreenMode {
    /// Initialize from a Roastty fullscreen action.
    static func from(roastty: roastty_action_fullscreen_e) -> Self? {
        return switch roastty {
        case ROASTTY_FULLSCREEN_NATIVE:
                .native

        case ROASTTY_FULLSCREEN_MACOS_NON_NATIVE:
                .nonNative

        case ROASTTY_FULLSCREEN_MACOS_NON_NATIVE_VISIBLE_MENU:
                .nonNativeVisibleMenu

        case ROASTTY_FULLSCREEN_MACOS_NON_NATIVE_PADDED_NOTCH:
                .nonNativePaddedNotch

        default:
            nil
        }
    }
}
