import RoasttyKit
import Testing
@testable import Roastty

struct CommandPaletteHostedTests {
    @Test func commandEntriesBuildSelectableOptions() throws {
        let config = try TemporaryConfig("""
        command-palette-entry = clear
        command-palette-entry = title:Hosted Clear,description:Clear from hosted test,action:clear_screen
        command-palette-entry = title:GTK Inspector,description:Unsupported on macOS,action:show_gtk_inspector
        keybind = super+k=clear_screen
        """)

        let commands = config.commandPaletteEntries
        #expect(commands.count == 2)

        var performed: [String] = []
        let options = TerminalCommandPaletteView.terminalCommandOptions(
            commands: commands,
            config: config
        ) { action in
            performed.append(action)
        }

        #expect(options.count == 1)
        let option = try #require(options.first)
        #expect(option.title == "Hosted Clear")
        #expect(option.description == "Clear from hosted test")
        #expect(option.symbols == ["⌘", "K"])

        option.action()

        #expect(performed == ["clear_screen"])
    }

    @MainActor
    @Test func surfacePerformDispatchesBindingAction() throws {
        let app = try TestRoasttyApp()
        let surfaceView = Roastty.SurfaceView(app.app)
        let surface = try #require(surfaceView.surfaceModel)

        #expect(surface.perform(action: "clear_screen"))
        #expect(!surface.perform(action: "definitely_not_a_command_palette_action"))
    }

    private final class TestRoasttyApp {
        let config: TemporaryConfig
        let app: roastty_app_t

        init() throws {
            let config = try TemporaryConfig("")
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

    private enum TestError: Error {
        case appCreationFailed
    }
}
