import AppKit
import AppIntents
import RoasttyKit

struct FocusTerminalIntent: AppIntent {
    static var title: LocalizedStringResource = "Focus Terminal"
    static var description = IntentDescription("Move focus to an existing terminal.")

    @Parameter(
        title: "Terminal",
        description: "The terminal to focus.",
    )
    var terminal: TerminalEntity

#if compiler(>=6.2)
    @available(macOS 26.0, *)
    static var supportedModes: IntentModes = .background
#endif

    @MainActor
    func perform() async throws -> some IntentResult {
        guard await requestIntentPermission() else {
            throw RoasttyIntentError.permissionDenied
        }

        guard let surfaceView = terminal.surfaceView else {
            throw RoasttyIntentError.surfaceNotFound
        }

        guard let controller = surfaceView.window?.windowController as? BaseTerminalController else {
            return .result()
        }

        controller.focusSurface(surfaceView)
        return .result()
    }
}
