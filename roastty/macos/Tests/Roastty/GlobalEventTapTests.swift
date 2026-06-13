import CoreGraphics
import RoasttyKit
import Testing
@testable import Roastty

struct GlobalEventTapTests {
    @Test func enableSuccessInstallsTapWithoutRetry() {
        let harness = TapHarness(results: [TestInstalledTap().asInstalledTap()])

        harness.tap.enable()

        let expectedEventMask = Self.keyDownEventMask
        #expect(harness.tap.isEventTapInstalled)
        #expect(!harness.tap.isRetryPending)
        #expect(harness.createCount == 1)
        #expect(harness.eventMasks == [expectedEventMask])
        #expect(harness.retryTimer == nil)
    }

    @Test func repeatedEnableDoesNotReinstallTap() {
        let harness = TapHarness(results: [TestInstalledTap().asInstalledTap()])

        harness.tap.enable()
        harness.tap.enable()

        #expect(harness.tap.isEventTapInstalled)
        #expect(harness.createCount == 1)
        #expect(!harness.tap.isRetryPending)
    }

    @Test func enableFailureSchedulesRetry() {
        let harness = TapHarness(results: [nil])

        harness.tap.enable()

        #expect(!harness.tap.isEventTapInstalled)
        #expect(harness.tap.isRetryPending)
        #expect(harness.createCount == 1)
        #expect(harness.retryTimer != nil)
        #expect(harness.retryTimer?.invalidated == false)
    }

    @Test func retrySuccessInstallsTapAndCancelsRetry() throws {
        let harness = TapHarness(results: [nil, TestInstalledTap().asInstalledTap()])

        harness.tap.enable()
        let retryTimer = try #require(harness.retryTimer)
        retryTimer.fire()

        #expect(harness.tap.isEventTapInstalled)
        #expect(!harness.tap.isRetryPending)
        #expect(harness.createCount == 2)
        #expect(retryTimer.invalidated)
    }

    @Test func disableCancelsPendingRetry() throws {
        let harness = TapHarness(results: [nil])

        harness.tap.enable()
        let retryTimer = try #require(harness.retryTimer)
        harness.tap.disable()

        #expect(!harness.tap.isEventTapInstalled)
        #expect(!harness.tap.isRetryPending)
        #expect(retryTimer.invalidated)
    }

    @Test func disableInvalidatesInstalledTap() throws {
        let installedTap = TestInstalledTap()
        let harness = TapHarness(results: [installedTap.asInstalledTap()])

        harness.tap.enable()
        harness.tap.disable()

        #expect(!harness.tap.isEventTapInstalled)
        #expect(!harness.tap.isRetryPending)
        #expect(installedTap.invalidated)
    }

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

    private static var keyDownEventMask: CGEventMask {
        [CGEventType.keyDown].reduce(CGEventMask(0), { $0 | (1 << $1.rawValue)})
    }

    private enum TestError: Error {
        case appCreationFailed
        case cgEventCreationFailed
    }

    private final class TapHarness {
        var createCount = 0
        var eventMasks: [CGEventMask] = []
        var retryTimer: TestRetryTimer?
        private var results: [GlobalEventTap.InstalledTap?]

        init(results: [GlobalEventTap.InstalledTap?]) {
            self.results = results
        }

        lazy var tap = GlobalEventTap(dependencies: GlobalEventTap.Dependencies(
                createEventTap: { [self] eventMask in
                    createCount += 1
                    eventMasks.append(eventMask)
                    return self.results.isEmpty ? nil : self.results.removeFirst()
                },
                scheduleRetry: { [self] retry in
                    let timer = TestRetryTimer(retry)
                    retryTimer = timer
                    return timer
                }
        ))
    }

    private final class TestInstalledTap {
        var invalidated = false
        var enabledValues: [Bool] = []

        func asInstalledTap() -> GlobalEventTap.InstalledTap {
            GlobalEventTap.InstalledTap(
                invalidate: { self.invalidated = true },
                setEnabled: { self.enabledValues.append($0) }
            )
        }
    }

    private final class TestRetryTimer: GlobalEventTap.RetryTimer {
        var invalidated = false
        private let retry: () -> Void

        init(_ retry: @escaping () -> Void) {
            self.retry = retry
        }

        func invalidate() {
            invalidated = true
        }

        func fire() {
            retry()
        }
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
