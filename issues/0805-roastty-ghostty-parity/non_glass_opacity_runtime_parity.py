#!/usr/bin/env python3
"""Guard copied macOS non-glass opacity parity for Issue 805 CFG-223."""

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
        .replace("libghostty", "libroastty")
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
        "vendor/ghostty/macos/Sources/Features/Terminal/Window Styles/TerminalWindow.swift",
        "roastty/macos/Sources/Features/Terminal/Window Styles/TerminalWindow.swift",
        "TerminalWindow.swift",
    )
    assert_sources_match_after_rename(
        "vendor/ghostty/macos/Sources/Features/Terminal/Window Styles/TransparentTitlebarTerminalWindow.swift",
        "roastty/macos/Sources/Features/Terminal/Window Styles/TransparentTitlebarTerminalWindow.swift",
        "TransparentTitlebarTerminalWindow.swift",
    )
    assert_sources_match_after_rename(
        "vendor/ghostty/macos/Sources/Features/QuickTerminal/QuickTerminalController.swift",
        "roastty/macos/Sources/Features/QuickTerminal/QuickTerminalController.swift",
        "QuickTerminalController.swift",
    )

    ghostty_window = read(
        "vendor/ghostty/macos/Sources/Features/Terminal/Window Styles/TerminalWindow.swift"
    )
    roastty_window = read(
        "roastty/macos/Sources/Features/Terminal/Window Styles/TerminalWindow.swift"
    )
    ghostty_titlebar = read(
        "vendor/ghostty/macos/Sources/Features/Terminal/Window Styles/TransparentTitlebarTerminalWindow.swift"
    )
    roastty_titlebar = read(
        "roastty/macos/Sources/Features/Terminal/Window Styles/TransparentTitlebarTerminalWindow.swift"
    )
    ghostty_quick = read(
        "vendor/ghostty/macos/Sources/Features/QuickTerminal/QuickTerminalController.swift"
    )
    roastty_quick = read(
        "roastty/macos/Sources/Features/QuickTerminal/QuickTerminalController.swift"
    )
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_window,
        [
            ("let forceOpaque = terminalController?.isBackgroundOpaque ?? false", "Ghostty opaque toggle"),
            ("!styleMask.contains(.fullScreen)", "Ghostty fullscreen opacity suppression"),
            ("!forceOpaque", "Ghostty forced opaque suppression"),
            ("surfaceConfig.backgroundOpacity < 1", "Ghostty opacity threshold"),
            ("surfaceConfig.backgroundBlur.isGlassStyle", "Ghostty glass branch condition"),
            ("isOpaque = false", "Ghostty transparent window mode"),
            ("backgroundColor = .white.withAlphaComponent(0.001)", "Ghostty transparent background workaround"),
            ("if !surfaceConfig.backgroundBlur.isGlassStyle", "Ghostty non-glass blur gate"),
            ("ghostty_set_window_background_blur", "Ghostty non-glass blur ABI"),
            ("isOpaque = true", "Ghostty opaque fallback"),
            ("backgroundColor.withAlphaComponent(1)", "Ghostty opaque fallback alpha"),
            ("var preferredBackgroundColor: NSColor?", "Ghostty preferred background color"),
            ("surface.derivedConfig.backgroundOpacity.clamped(to: 0.001...1)", "Ghostty surface alpha clamp"),
            ("derivedConfig.backgroundOpacity.clamped(to: 0.001...1)", "Ghostty window alpha clamp"),
            ("withAlphaComponent(alpha)", "Ghostty preferred alpha application"),
            ("self.backgroundOpacity = config.backgroundOpacity", "Ghostty derived opacity config"),
            ("self.backgroundBlur = config.backgroundBlur", "Ghostty derived blur config"),
        ],
    )
    require_all(
        roastty_window,
        [
            ("let forceOpaque = terminalController?.isBackgroundOpaque ?? false", "Roastty opaque toggle"),
            ("!styleMask.contains(.fullScreen)", "Roastty fullscreen opacity suppression"),
            ("!forceOpaque", "Roastty forced opaque suppression"),
            ("surfaceConfig.backgroundOpacity < 1", "Roastty opacity threshold"),
            ("surfaceConfig.backgroundBlur.isGlassStyle", "Roastty glass branch condition"),
            ("isOpaque = false", "Roastty transparent window mode"),
            ("backgroundColor = .white.withAlphaComponent(0.001)", "Roastty transparent background workaround"),
            ("if !surfaceConfig.backgroundBlur.isGlassStyle", "Roastty non-glass blur gate"),
            ("roastty_set_window_background_blur", "Roastty non-glass blur ABI"),
            ("isOpaque = true", "Roastty opaque fallback"),
            ("backgroundColor.withAlphaComponent(1)", "Roastty opaque fallback alpha"),
            ("var preferredBackgroundColor: NSColor?", "Roastty preferred background color"),
            ("surface.derivedConfig.backgroundOpacity.clamped(to: 0.001...1)", "Roastty surface alpha clamp"),
            ("derivedConfig.backgroundOpacity.clamped(to: 0.001...1)", "Roastty window alpha clamp"),
            ("withAlphaComponent(alpha)", "Roastty preferred alpha application"),
            ("self.backgroundOpacity = config.backgroundOpacity", "Roastty derived opacity config"),
            ("self.backgroundBlur = config.backgroundBlur", "Roastty derived blur config"),
        ],
    )

    require_all(
        ghostty_titlebar,
        [
            ("let isGlassStyle = derivedConfig.backgroundBlur.isGlassStyle", "Ghostty titlebar glass decision"),
            ("titlebarView.layer?.backgroundColor = (isGlassStyle && isTransparentTitlebar)", "Ghostty Tahoe titlebar branch"),
            ("? NSColor.clear.cgColor", "Ghostty glass titlebar clear"),
            (": preferredBackgroundColor?.cgColor", "Ghostty non-glass titlebar preferred color"),
            ("titlebarContainer.layer?.backgroundColor = preferredBackgroundColor?.cgColor", "Ghostty Ventura titlebar preferred color"),
        ],
    )
    require_all(
        roastty_titlebar,
        [
            ("let isGlassStyle = derivedConfig.backgroundBlur.isGlassStyle", "Roastty titlebar glass decision"),
            ("titlebarView.layer?.backgroundColor = (isGlassStyle && isTransparentTitlebar)", "Roastty Tahoe titlebar branch"),
            ("? NSColor.clear.cgColor", "Roastty glass titlebar clear"),
            (": preferredBackgroundColor?.cgColor", "Roastty non-glass titlebar preferred color"),
            ("titlebarContainer.layer?.backgroundColor = preferredBackgroundColor?.cgColor", "Roastty Ventura titlebar preferred color"),
        ],
    )

    require_all(
        ghostty_quick,
        [
            ("!isBackgroundOpaque", "Ghostty quick terminal opaque toggle"),
            ("self.derivedConfig.backgroundOpacity < 1", "Ghostty quick opacity threshold"),
            ("derivedConfig.backgroundBlur.isGlassStyle", "Ghostty quick glass condition"),
            ("window.isOpaque = false", "Ghostty quick transparent window mode"),
            ("window.backgroundColor = .white.withAlphaComponent(0.001)", "Ghostty quick transparent background workaround"),
            ("if !derivedConfig.backgroundBlur.isGlassStyle", "Ghostty quick non-glass blur gate"),
            ("ghostty_set_window_background_blur", "Ghostty quick non-glass blur ABI"),
            ("window.isOpaque = true", "Ghostty quick opaque fallback"),
            ("window.backgroundColor = .windowBackgroundColor", "Ghostty quick opaque background"),
            ("terminalViewContainer?.ghosttyConfigDidChange(ghostty.config, preferredBackgroundColor: nil)", "Ghostty quick container config sync"),
            ("let backgroundOpacity: Double", "Ghostty quick derived opacity field"),
            ("let backgroundBlur: Ghostty.Config.BackgroundBlur", "Ghostty quick derived blur field"),
            ("self.backgroundOpacity = config.backgroundOpacity", "Ghostty quick derived opacity config"),
            ("self.backgroundBlur = config.backgroundBlur", "Ghostty quick derived blur config"),
        ],
    )
    require_all(
        roastty_quick,
        [
            ("!isBackgroundOpaque", "Roastty quick terminal opaque toggle"),
            ("self.derivedConfig.backgroundOpacity < 1", "Roastty quick opacity threshold"),
            ("derivedConfig.backgroundBlur.isGlassStyle", "Roastty quick glass condition"),
            ("window.isOpaque = false", "Roastty quick transparent window mode"),
            ("window.backgroundColor = .white.withAlphaComponent(0.001)", "Roastty quick transparent background workaround"),
            ("if !derivedConfig.backgroundBlur.isGlassStyle", "Roastty quick non-glass blur gate"),
            ("roastty_set_window_background_blur", "Roastty quick non-glass blur ABI"),
            ("window.isOpaque = true", "Roastty quick opaque fallback"),
            ("window.backgroundColor = .windowBackgroundColor", "Roastty quick opaque background"),
            ("terminalViewContainer?.roasttyConfigDidChange(roastty.config, preferredBackgroundColor: nil)", "Roastty quick container config sync"),
            ("let backgroundOpacity: Double", "Roastty quick derived opacity field"),
            ("let backgroundBlur: Roastty.Config.BackgroundBlur", "Roastty quick derived blur field"),
            ("self.backgroundOpacity = config.backgroundOpacity", "Roastty quick derived opacity config"),
            ("self.backgroundBlur = config.backgroundBlur", "Roastty quick derived blur config"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-008B2B2B2A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-008B2B2B2A status"),
            ("TerminalWindow.swift", "RUNTIME-008B2B2B2A terminal window evidence"),
            ("TransparentTitlebarTerminalWindow.swift", "RUNTIME-008B2B2B2A titlebar evidence"),
            ("QuickTerminalController.swift", "RUNTIME-008B2B2B2A quick terminal evidence"),
            ("backgroundOpacity", "RUNTIME-008B2B2B2A opacity evidence"),
            ("backgroundBlur.isGlassStyle", "RUNTIME-008B2B2B2A non-glass gate"),
            ("non_glass_opacity_runtime_parity.py", "RUNTIME-008B2B2B2A guard"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-008B2B2B2B2B4")
    require_all(
        row_gap,
        [
            ("Oracle complete", "RUNTIME-008B2B2B2B2B4 status"),
                        ("scroll-to-bottom.output", "RUNTIME-008B2B2B2B2B concrete gap"),
        ],
    )
    if "RUNTIME-008B2B2B2 |" in runtime_inventory:
        raise AssertionError("old broad RUNTIME-008B2B2B2 row is still present")

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("89 rows Oracle complete", "CFG-223 oracle count"),
            ("92 rows closed", "CFG-223 closed count"),
            ("1 rows are incomplete", "CFG-223 incomplete count"),
            ("1 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("non_glass_opacity_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
