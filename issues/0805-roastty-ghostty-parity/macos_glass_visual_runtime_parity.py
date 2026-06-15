#!/usr/bin/env python3
"""Guard macOS glass visual runtime parity for Issue 805 CFG-223."""

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
    )


def assert_sources_match_after_rename(ghostty_source: str, roastty_source: str) -> None:
    normalized = normalize_ghostty_to_roastty(ghostty_source)
    if normalized == roastty_source:
        return
    diff = "\n".join(
        difflib.unified_diff(
            normalized.splitlines(),
            roastty_source.splitlines(),
            fromfile="ghostty-normalized/TerminalViewContainer.swift",
            tofile="roastty/TerminalViewContainer.swift",
            lineterm="",
        )
    )
    raise AssertionError(
        "TerminalViewContainer.swift differs after expected Roastty renames:\n"
        + diff
    )


def main() -> int:
    ghostty_container = read(
        "vendor/ghostty/macos/Sources/Features/Terminal/TerminalViewContainer.swift"
    )
    roastty_container = read(
        "roastty/macos/Sources/Features/Terminal/TerminalViewContainer.swift"
    )
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    assert_sources_match_after_rename(ghostty_container, roastty_container)

    require_all(
        ghostty_container,
        [
            ("NSGlassEffectView", "Ghostty macOS glass view"),
            ("glassEffectView.tintColor = backgroundColor.withAlphaComponent(backgroundOpacity)", "Ghostty glass opacity tint"),
            ("glassEffectView.cornerRadius = cornerRadius ?? 0", "Ghostty glass corner radius"),
            ("case .macosGlassRegular:", "Ghostty regular glass config case"),
            ("case .macosGlassClear:", "Ghostty clear glass config case"),
            ("self.backgroundOpacity = config.backgroundOpacity", "Ghostty background opacity derivation"),
            ("preferredBackgroundColor ?? NSColor(config.backgroundColor)", "Ghostty preferred background color fallback"),
            ("effectView.updateTopInset(-themeFrameView.safeAreaInsets.top)", "Ghostty safe-area top inset update"),
            ("func updateGlassTintOverlay(isKeyWindow: Bool)", "Ghostty inactive tint update entry"),
        ],
    )
    require_all(
        roastty_container,
        [
            ("NSGlassEffectView", "Roastty macOS glass view"),
            ("glassEffectView.tintColor = backgroundColor.withAlphaComponent(backgroundOpacity)", "Roastty glass opacity tint"),
            ("glassEffectView.cornerRadius = cornerRadius ?? 0", "Roastty glass corner radius"),
            ("case .macosGlassRegular:", "Roastty regular glass config case"),
            ("case .macosGlassClear:", "Roastty clear glass config case"),
            ("self.backgroundOpacity = config.backgroundOpacity", "Roastty background opacity derivation"),
            ("preferredBackgroundColor ?? NSColor(config.backgroundColor)", "Roastty preferred background color fallback"),
            ("effectView.updateTopInset(-themeFrameView.safeAreaInsets.top)", "Roastty safe-area top inset update"),
            ("func updateGlassTintOverlay(isKeyWindow: Bool)", "Roastty inactive tint update entry"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-008B2B2B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-008B2B2B1 status"),
            ("macOS glass", "RUNTIME-008B2B2B1 behavior"),
            ("TerminalViewContainer.swift", "RUNTIME-008B2B2B1 source evidence"),
            ("NSGlassEffectView", "RUNTIME-008B2B2B1 glass marker"),
            ("backgroundOpacity", "RUNTIME-008B2B2B1 opacity evidence"),
            ("macos_glass_visual_runtime_parity.py", "RUNTIME-008B2B2B1 guard"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-008B2B2B2B")
    require_all(
        row_gap,
        [
            ("Gap", "RUNTIME-008B2B2B2B status"),
            ("non-glass compositor opacity", "RUNTIME-008B2B2B2B opacity gap"),
            ("GUI cursor pixels", "RUNTIME-008B2B2B2B GUI cursor gap"),
            ("custom shader output", "RUNTIME-008B2B2B2B custom shader gap"),
            ("broader GUI/pixel parity", "RUNTIME-008B2B2B2B GUI parity gap"),
            ("screenshot-level padding pixel proof", "RUNTIME-008B2B2B2B screenshot padding gap"),
        ],
    )
    if "RUNTIME-008B2B2B |" in runtime_inventory:
        raise AssertionError("old broad RUNTIME-008B2B2B row is still present")

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("58 rows Oracle complete", "CFG-223 oracle count"),
            ("60 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
