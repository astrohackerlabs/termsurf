import CoreGraphics
import RoasttyKit
import Testing
@testable import Roastty

struct GlobalEventTapTests {
    @Test func inactiveGlobalBindingSuppressesKeyDown() throws {
        let app = try TestRoasttyApp(configText: "keybind = global:a=ignore")
        let event = try keyEvent(keyCode: 0)

        #expect(globalEventTapHandleKeyEvent(
            type: .keyDown,
            cgEvent: event,
            appIsActive: false,
            roastty: app.app
        ))
    }

    @Test func activeAppDoesNotSuppressGlobalBinding() throws {
        let app = try TestRoasttyApp(configText: "keybind = global:a=ignore")
        let event = try keyEvent(keyCode: 0)

        #expect(!globalEventTapHandleKeyEvent(
            type: .keyDown,
            cgEvent: event,
            appIsActive: true,
            roastty: app.app
        ))
    }

    @Test func inactiveNonGlobalBindingDoesNotSuppressKeyDown() throws {
        let app = try TestRoasttyApp(configText: "keybind = a=ignore")
        let event = try keyEvent(keyCode: 0)

        #expect(!globalEventTapHandleKeyEvent(
            type: .keyDown,
            cgEvent: event,
            appIsActive: false,
            roastty: app.app
        ))
    }

    @Test func nonKeyDownEventPassesThrough() throws {
        let app = try TestRoasttyApp(configText: "keybind = global:a=ignore")
        let event = try keyEvent(keyCode: 0)

        #expect(!globalEventTapHandleKeyEvent(
            type: .flagsChanged,
            cgEvent: event,
            appIsActive: false,
            roastty: app.app
        ))
    }

    private func keyEvent(keyCode: CGKeyCode) throws -> CGEvent {
        guard let event = CGEvent(
            keyboardEventSource: nil,
            virtualKey: keyCode,
            keyDown: true
        ) else {
            throw TestError.cgEventCreationFailed
        }
        return event
    }

    private enum TestError: Error {
        case appCreationFailed
        case cgEventCreationFailed
    }

    private final class TestRoasttyApp {
        let config: TemporaryConfig
        let app: roastty_app_t

        init(configText: String) throws {
            let config = try TemporaryConfig(configText)
            guard let rawConfig = config.config else {
                throw TestError.appCreationFailed
            }
            guard let app = roastty_app_new(nil, rawConfig) else {
                throw TestError.appCreationFailed
            }

            self.config = config
            self.app = app
        }

        deinit {
            roastty_app_free(app)
        }
    }
}
