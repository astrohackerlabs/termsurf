import Testing
@testable import Ghostty
import AppKit
import SwiftUI

@Suite
struct ConfigTests {
    private func expectColor(_ color: Color?, red: Int, green: Int, blue: Int) throws {
        let color = try #require(color)
        let nsColor = try #require(NSColor(color).usingColorSpace(.sRGB))
        #expect(Int((nsColor.redComponent * 255).rounded()) == red)
        #expect(Int((nsColor.greenComponent * 255).rounded()) == green)
        #expect(Int((nsColor.blueComponent * 255).rounded()) == blue)
    }

    // MARK: - Boolean Properties

    @Test func initialWindowDefaultsToTrue() throws {
        let config = try TemporaryConfig("")
        #expect(config.initialWindow == true)
    }

    @Test func initialWindowSetToFalse() throws {
        let config = try TemporaryConfig("initial-window = false")
        #expect(config.initialWindow == false)
    }

    @Test func quitAfterLastWindowClosedDefaultsToFalse() throws {
        let config = try TemporaryConfig("")
        #expect(config.shouldQuitAfterLastWindowClosed == false)
    }

    @Test func quitAfterLastWindowClosedSetToTrue() throws {
        let config = try TemporaryConfig("quit-after-last-window-closed = true")
        #expect(config.shouldQuitAfterLastWindowClosed == true)
    }

    @Test func windowStepResizeDefaultsToFalse() throws {
        let config = try TemporaryConfig("")
        #expect(config.windowStepResize == false)
    }

    @Test func focusFollowsMouseDefaultsToFalse() throws {
        let config = try TemporaryConfig("")
        #expect(config.focusFollowsMouse == false)
    }

    @Test func focusFollowsMouseSetToTrue() throws {
        let config = try TemporaryConfig("focus-follows-mouse = true")
        #expect(config.focusFollowsMouse == true)
    }

    @Test func windowDecorationsDefaultsToTrue() throws {
        let config = try TemporaryConfig("")
        #expect(config.windowDecorations == true)
    }

    @Test func windowDecorationsNone() throws {
        let config = try TemporaryConfig("window-decoration = none")
        #expect(config.windowDecorations == false)
    }

    @Test func macosWindowShadowDefaultsToTrue() throws {
        let config = try TemporaryConfig("")
        #expect(config.macosWindowShadow == true)
    }

    @Test func maximizeDefaultsToFalse() throws {
        let config = try TemporaryConfig("")
        #expect(config.maximize == false)
    }

    @Test func maximizeSetToTrue() throws {
        let config = try TemporaryConfig("maximize = true")
        #expect(config.maximize == true)
    }

    // MARK: - String / Optional String Properties

    @Test func titleDefaultsToNil() throws {
        let config = try TemporaryConfig("")
        #expect(config.title == nil)
    }

    @Test func titleSetToCustomValue() throws {
        let config = try TemporaryConfig("title = My Terminal")
        #expect(config.title == "My Terminal")
    }

    @Test func windowTitleFontFamilyDefaultsToNil() throws {
        let config = try TemporaryConfig("")
        #expect(config.windowTitleFontFamily == nil)
    }

    @Test func windowTitleFontFamilySetToValue() throws {
        let config = try TemporaryConfig("window-title-font-family = Menlo")
        #expect(config.windowTitleFontFamily == "Menlo")
    }

    // MARK: - Enum Properties

    @Test func macosTitlebarStyleDefaultsToTransparent() throws {
        let config = try TemporaryConfig("")
        #expect(config.macosTitlebarStyle == .transparent)
    }

    @Test(arguments: [
        ("native", Ghostty.Config.MacOSTitlebarStyle.native),
        ("transparent", Ghostty.Config.MacOSTitlebarStyle.transparent),
        ("tabs", Ghostty.Config.MacOSTitlebarStyle.tabs),
        ("hidden", Ghostty.Config.MacOSTitlebarStyle.hidden),
    ])
    func macosTitlebarStyleValues(raw: String, expected: Ghostty.Config.MacOSTitlebarStyle) throws {
        let config = try TemporaryConfig("macos-titlebar-style = \(raw)")
        #expect(config.macosTitlebarStyle == expected)
    }

    @Test func resizeOverlayDefaultsToAfterFirst() throws {
        let config = try TemporaryConfig("")
        #expect(config.resizeOverlay == .after_first)
    }

    @Test(arguments: [
        ("always", Ghostty.Config.ResizeOverlay.always),
        ("never", Ghostty.Config.ResizeOverlay.never),
        ("after-first", Ghostty.Config.ResizeOverlay.after_first),
    ])
    func resizeOverlayValues(raw: String, expected: Ghostty.Config.ResizeOverlay) throws {
        let config = try TemporaryConfig("resize-overlay = \(raw)")
        #expect(config.resizeOverlay == expected)
    }

    @Test func resizeOverlayPositionDefaultsToCenter() throws {
        let config = try TemporaryConfig("")
        #expect(config.resizeOverlayPosition == .center)
    }

    @Test func macosIconDefaultsToOfficial() throws {
        let config = try TemporaryConfig("")
        #expect(config.macosIcon == .official)
    }

    @Test func macosIconFrameDefaultsToAluminum() throws {
        let config = try TemporaryConfig("")
        #expect(config.macosIconFrame == .aluminum)
    }

    @Test func macosWindowButtonsDefaultsToVisible() throws {
        let config = try TemporaryConfig("")
        #expect(config.macosWindowButtons == .visible)
    }

    @Test func scrollbarDefaultsToSystem() throws {
        let config = try TemporaryConfig("")
        #expect(config.scrollbar == .system)
    }

    @Test func scrollbarSetToNever() throws {
        let config = try TemporaryConfig("scrollbar = never")
        #expect(config.scrollbar == .never)
    }

    // MARK: - Numeric Properties

    @Test func backgroundOpacityDefaultsToOne() throws {
        let config = try TemporaryConfig("")
        #expect(config.backgroundOpacity == 1.0)
    }

    @Test func backgroundOpacitySetToCustom() throws {
        let config = try TemporaryConfig("background-opacity = 0.5")
        #expect(config.backgroundOpacity == 0.5)
    }

    @Test func splitBorderWidthDefaultsToZero() throws {
        let config = try TemporaryConfig("")
        #expect(config.splitBorderWidth == 0)
    }

    @Test func windowPositionDefaultsToNil() throws {
        let config = try TemporaryConfig("")
        #expect(config.windowPositionX == nil)
        #expect(config.windowPositionY == nil)
    }

    // MARK: - Split Border Colors

    @Test func splitBorderColorsDeriveFromTokyoNightPalette() throws {
        let config = try TemporaryConfig("""
        background = #1a1b26
        palette = 6=#7dcfff
        palette = 8=#414868
        palette = 14=#7dcfff
        """)

        try expectColor(config.focusedSplitBorderColor, red: 0x7d, green: 0xcf, blue: 0xff)
        try expectColor(config.unfocusedSplitBorderColor, red: 0x41, green: 0x48, blue: 0x68)
        #expect(config.splitBorderWidth == 0)
    }

    @Test func splitBorderColorOverridesWinIndependently() throws {
        let config = try TemporaryConfig("""
        background = #1a1b26
        palette = 6=#7dcfff
        palette = 8=#414868
        focused-split-border-color = #112233
        unfocused-split-border-color = #445566
        """)

        try expectColor(config.focusedSplitBorderColor, red: 0x11, green: 0x22, blue: 0x33)
        try expectColor(config.unfocusedSplitBorderColor, red: 0x44, green: 0x55, blue: 0x66)
    }

    @Test func focusedSplitBorderUsesFallbackWhenPaletteSixContrastIsLow() throws {
        let config = try TemporaryConfig("""
        background = #f4f4f4
        palette = 4=#003e8a
        palette = 6=#7cc4df
        palette = 8=#888888
        palette = 12=#807d7c
        palette = 14=#7cc4df
        """)

        try expectColor(config.focusedSplitBorderColor, red: 0x00, green: 0x3e, blue: 0x8a)
    }

    // MARK: - Config Loading

    @Test func loadedIsTrueForValidConfig() throws {
        let config = try TemporaryConfig("")
        #expect(config.loaded == true)
    }

    @Test func unfinalizedConfigIsLoaded() throws {
        let config = try TemporaryConfig("", finalize: false)
        #expect(config.loaded == true)
    }

    @Test func reloadConfig() throws {
        let config = try TemporaryConfig("background-opacity = 0.5")
        #expect(config.backgroundOpacity == 0.5)

        try config.reload("background-opacity = 0.7")
        #expect(config.backgroundOpacity == 0.7)
    }

    @Test func defaultConfigIsLoaded() throws {
        let config = try TemporaryConfig("")
        #expect(config.optionalAutoUpdateChannel != nil) // release or tip
        let config1 = try TemporaryConfig("", finalize: false)
        #expect(config1.optionalAutoUpdateChannel == nil)
    }

    @Test func errorsEmptyForValidConfig() throws {
        let config = try TemporaryConfig("")
        #expect(config.errors.isEmpty)
    }

    @Test func errorsReportedForInvalidConfig() throws {
        let config = try TemporaryConfig("not-a-real-key = value")
        #expect(!config.errors.isEmpty)
    }

    // MARK: - Multiple Config Lines

    @Test func multipleConfigValues() throws {
        let config = try TemporaryConfig("""
        initial-window = false
        quit-after-last-window-closed = true
        maximize = true
        focus-follows-mouse = true
        """)
        #expect(config.initialWindow == false)
        #expect(config.shouldQuitAfterLastWindowClosed == true)
        #expect(config.maximize == true)
        #expect(config.focusFollowsMouse == true)
    }

    // MARK: - Keybind

    @Test
    func uppercasedLetterShouldBeNormalized() async throws {
        let config = try TemporaryConfig("""
        keybind=cmd+L=goto_split:left
        """)
        let shortcut = try #require(config.keyboardShortcut(for: "goto_split:left"))
        #expect(shortcut == .init("l", modifiers: [.command]))

        let config2 = try TemporaryConfig("""
        keybind=cmd+Ä=goto_split:left
        """)
        let shortcut2 = try #require(config2.keyboardShortcut(for: "goto_split:left"))
        #expect(shortcut2 == .init("ä", modifiers: [.command]))
    }

    @Test
    func emptyConfigShouldBeHaveDefaultShortcut() async throws {
        let config = try TemporaryConfig("")
        let newWindow = try #require(config.keyboardShortcut(for: "new_window"))
        #expect(newWindow == .init("n", modifiers: [.command]))
        let gotoToNextSplit = try #require(config.keyboardShortcut(for: "goto_split:next"))
        #expect(gotoToNextSplit == .init("]", modifiers: [.command]))
    }
}
