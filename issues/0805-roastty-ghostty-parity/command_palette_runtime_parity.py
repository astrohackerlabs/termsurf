#!/usr/bin/env python3
"""Guard command palette runtime parity for Issue 805 CFG-223."""

from __future__ import annotations

import difflib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ISSUE = ROOT / "issues/0805-roastty-ghostty-parity"


def read(path: str) -> str:
    return (ROOT / path).read_text()


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise AssertionError(f"missing {label}: {needle!r}")


def require_all(text: str, needles: list[tuple[str, str]]) -> None:
    for needle, label in needles:
        require(text, needle, label)


def require_row(markdown: str, row_id: str) -> str:
    for line in markdown.splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if cells and cells[0] == row_id:
            return line
    raise AssertionError(f"missing inventory row {row_id}")


def normalize_ghostty_to_roastty(source: str) -> str:
    return (
        source.replace("Ghostty", "Roastty")
        .replace("ghostty", "roastty")
        .replace("GHOSTTY", "ROASTTY")
        .replace("Update Roastty and Restart", "Update Ghostty and Restart")
    )


def assert_sources_match_after_rename(
    ghostty_path: str,
    roastty_path: str,
    label: str,
) -> None:
    ghostty_source = read(ghostty_path)
    roastty_source = read(roastty_path)
    normalized = normalize_ghostty_to_roastty(ghostty_source)
    if normalized == roastty_source:
        return
    diff = "\n".join(
        difflib.unified_diff(
            normalized.splitlines(),
            roastty_source.splitlines(),
            fromfile=f"ghostty-normalized/{label}",
            tofile=f"roastty/{label}",
            lineterm="",
        )
    )
    raise AssertionError(f"{label} differs after expected Roastty renames:\n{diff}")


def main() -> int:
    assert_sources_match_after_rename(
        "vendor/ghostty/macos/Sources/Features/Command Palette/CommandPalette.swift",
        "roastty/macos/Sources/Features/Command Palette/CommandPalette.swift",
        "CommandPalette.swift",
    )
    assert_sources_match_after_rename(
        "vendor/ghostty/macos/Sources/Features/App Intents/CommandPaletteIntent.swift",
        "roastty/macos/Sources/Features/App Intents/CommandPaletteIntent.swift",
        "CommandPaletteIntent.swift",
    )

    ghostty_terminal_palette = read(
        "vendor/ghostty/macos/Sources/Features/Command Palette/TerminalCommandPalette.swift"
    )
    roastty_terminal_palette = read(
        "roastty/macos/Sources/Features/Command Palette/TerminalCommandPalette.swift"
    )
    ghostty_app = read("vendor/ghostty/macos/Sources/Ghostty/Ghostty.App.swift")
    roastty_app = read("roastty/macos/Sources/Roastty/Roastty.App.swift")
    ghostty_controller = read(
        "vendor/ghostty/macos/Sources/Features/Terminal/BaseTerminalController.swift"
    )
    roastty_controller = read(
        "roastty/macos/Sources/Features/Terminal/BaseTerminalController.swift"
    )
    ghostty_surface = read(
        "vendor/ghostty/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift"
    )
    roastty_surface = read(
        "roastty/macos/Sources/Roastty/Surface View/SurfaceView_AppKit.swift"
    )
    roastty_config = read("roastty/macos/Sources/Roastty/Roastty.Config.swift")
    hosted_tests = read("roastty/macos/Tests/Roastty/CommandPaletteHostedTests.swift")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_terminal_palette,
        [
            ("CommandPaletteView(", "Ghostty overlay embeds shared palette view"),
            ("backgroundColor: ghosttyConfig.backgroundColor", "Ghostty palette background color"),
            ("surfaceView.window?.makeFirstResponder(surfaceView)", "Ghostty focus return"),
            ("updateOptions", "Ghostty update commands appear first"),
            ("jumpOptions + terminalOptions", "Ghostty jump and terminal options sorted together"),
            ("appDelegate.ghostty.config.commandPaletteEntries", "Ghostty command entries source"),
            (".filter(\\.isSupported)", "Ghostty unsupported command filter"),
            ("keyboardShortcut(for: c.action)?.keyList", "Ghostty shortcut symbols"),
            ("onAction(c.action)", "Ghostty command callback dispatch"),
            ("name: Ghostty.Notification.ghosttyPresentTerminal", "Ghostty focus jump notification"),
        ],
    )
    require_all(
        roastty_terminal_palette,
        [
            ("CommandPaletteView(", "Roastty overlay embeds shared palette view"),
            ("backgroundColor: roasttyConfig.backgroundColor", "Roastty palette background color"),
            ("surfaceView.window?.makeFirstResponder(surfaceView)", "Roastty focus return"),
            ("updateOptions", "Roastty update commands appear first"),
            ("jumpOptions + terminalOptions", "Roastty jump and terminal options sorted together"),
            ("Self.terminalCommandOptions(", "Roastty testable helper extraction"),
            ("commands: appDelegate.roastty.config.commandPaletteEntries", "Roastty command entries source"),
            ("static func terminalCommandOptions", "Roastty command option helper"),
            (".filter(\\.isSupported)", "Roastty unsupported command filter"),
            ("config.keyboardShortcut(for: c.action)?.keyList", "Roastty shortcut symbols"),
            ("onAction(c.action)", "Roastty command callback dispatch"),
            ("name: Roastty.Notification.roasttyPresentTerminal", "Roastty focus jump notification"),
        ],
    )

    require_all(
        ghostty_app,
        [
            ("case GHOSTTY_ACTION_TOGGLE_COMMAND_PALETTE:", "Ghostty toggle command action"),
            ("toggleCommandPalette(app, target: target)", "Ghostty toggle command dispatch"),
            ("name: .ghosttyCommandPaletteDidToggle", "Ghostty command palette notification"),
        ],
    )
    require_all(
        roastty_app,
        [
            ("case ROASTTY_ACTION_TOGGLE_COMMAND_PALETTE:", "Roastty toggle command action"),
            ("toggleCommandPalette(app, target: target)", "Roastty toggle command dispatch"),
            ("name: .roasttyCommandPaletteDidToggle", "Roastty command palette notification"),
        ],
    )
    require_all(
        ghostty_controller,
        [
            ("@Published var commandPaletteIsShowing: Bool = false", "Ghostty command palette state"),
            ("selector: #selector(ghosttyCommandPaletteDidToggle(_:))", "Ghostty toggle observer selector"),
            ("name: .ghosttyCommandPaletteDidToggle", "Ghostty toggle observer notification"),
            ("@objc private func ghosttyCommandPaletteDidToggle", "Ghostty notification handler"),
            ("toggleCommandPalette(nil)", "Ghostty notification toggles palette"),
            ("@IBAction func toggleCommandPalette", "Ghostty menu/action entry"),
            ("commandPaletteIsShowing.toggle()", "Ghostty state toggle"),
            ("focusedSurface?.resignFirstResponder()", "Ghostty first responder shield"),
        ],
    )
    require_all(
        roastty_controller,
        [
            ("@Published var commandPaletteIsShowing: Bool = false", "Roastty command palette state"),
            ("selector: #selector(roasttyCommandPaletteDidToggle(_:))", "Roastty toggle observer selector"),
            ("name: .roasttyCommandPaletteDidToggle", "Roastty toggle observer notification"),
            ("@objc private func roasttyCommandPaletteDidToggle", "Roastty notification handler"),
            ("toggleCommandPalette(nil)", "Roastty notification toggles palette"),
            ("@IBAction func toggleCommandPalette", "Roastty menu/action entry"),
            ("commandPaletteIsShowing.toggle()", "Roastty state toggle"),
            ("focusedSurface?.resignFirstResponder()", "Roastty first responder shield"),
        ],
    )
    require_all(
        ghostty_surface,
        [
            ("commandPaletteIsShowing == true", "Ghostty keyboard shielding state check"),
            ("are supposed to be handled by CommandPaletteView", "Ghostty keyboard shielding comment"),
        ],
    )
    require_all(
        roastty_surface,
        [
            ("commandPaletteIsShowing == true", "Roastty keyboard shielding state check"),
            ("are supposed to be handled by CommandPaletteView", "Roastty keyboard shielding comment"),
        ],
    )
    require_all(
        roastty_config,
        [
            ("var commandPaletteEntries: [Roastty.Command]", "Roastty Swift config command entries"),
            ('let key = "command-palette-entry"', "Roastty command-palette-entry key"),
            ("roastty_config_get(config, &v, key", "Roastty command entries ABI lookup"),
        ],
    )
    require_all(
        hosted_tests,
        [
            ("struct CommandPaletteHostedTests", "Roastty hosted command palette tests"),
            ("commandEntriesBuildSelectableOptions", "Roastty command option hosted test"),
            ("command-palette-entry = title:Hosted Clear", "Roastty custom entry fixture"),
            ("action:show_gtk_inspector", "Roastty unsupported action fixture"),
            ("TerminalCommandPaletteView.terminalCommandOptions", "Roastty helper exercised"),
            ('#expect(options.count == 1)', "Roastty unsupported command filtered"),
            ('#expect(option.symbols == ["⌘", "K"])', "Roastty shortcut symbols asserted"),
            ('#expect(performed == ["clear_screen"])', "Roastty command action asserted"),
            ("surfacePerformDispatchesBindingAction", "Roastty surface perform hosted test"),
            ('#expect(surface.perform(action: "clear_screen"))', "Roastty supported action dispatch"),
            ('#expect(!surface.perform(action: "definitely_not_a_command_palette_action"))', "Roastty invalid action rejection"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-011A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-011A status"),
            ("command palette", "RUNTIME-011A behavior"),
            ("CommandPaletteHostedTests", "RUNTIME-011A hosted test evidence"),
            ("command_palette_runtime_parity.py", "RUNTIME-011A guard"),
            ("command-palette-entry", "RUNTIME-011A config evidence"),
            ("commandPaletteIsShowing", "RUNTIME-011A state evidence"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-011B2B")
    require_all(
        row_gap,
        [
            ("Gap", "RUNTIME-011B2B status"),
            (
                "broader titlebar behavior",
                "RUNTIME-011B2B titlebar gap",
            ),
            ("screenshot/pixel evidence", "RUNTIME-011B2B screenshot/pixel gap"),
            (
                "broader split variants and interactions",
                "RUNTIME-011B2B broader split gap",
            ),
            (
                "focused fullscreen, command-palette, Quick Terminal, and right-split layout proof",
                "RUNTIME-011B2B narrowed GUI evidence",
            ),
        ],
    )
    if "RUNTIME-011 |" in runtime_inventory:
        raise AssertionError("old broad RUNTIME-011 row is still present")

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("75 rows Oracle complete", "CFG-223 oracle count"),
            ("78 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
