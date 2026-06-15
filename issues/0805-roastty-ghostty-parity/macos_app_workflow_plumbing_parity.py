#!/usr/bin/env python3
"""Guard copied macOS app workflow plumbing for Issue 805 CFG-223."""

from __future__ import annotations

import difflib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ISSUE = ROOT / "issues/0805-roastty-ghostty-parity"

RENAMES = (
    ("GhosttyKit", "RoasttyKit"),
    ("Ghostty", "Roastty"),
    ("ghostty", "roastty"),
    ("GHOSTTY", "ROASTTY"),
    ("libghostty", "libroastty"),
)


def read(path: str) -> str:
    return (ROOT / path).read_text()


def normalize_ghostty_swift(text: str) -> str:
    for old, new in RENAMES:
        text = text.replace(old, new)
    return text


def strip_ui_trace_hooks(source: str) -> str:
    lines = source.splitlines()
    stripped: list[str] = []
    skip_multiline_call = False
    skip_helper = False
    helper_depth = 0
    for line in lines:
        if skip_helper:
            helper_depth += line.count("{") - line.count("}")
            if helper_depth <= 0:
                skip_helper = False
            continue
        if (
            "func appendUITestTrace(" in line
            or "func appendUITestKeyTrace(" in line
            or "static func openURLForUITest(" in line
            or 'if let expected = ProcessInfo.processInfo.environment["ROASTTY_UI_TEST_RECORD_OPEN_URL_PATH"]' in line
            or 'if ProcessInfo.processInfo.environment["ROASTTY_UI_TEST_SUPPRESS_OPEN_URL"] == "1"' in line
            or "func showUITestContextMenu(" in line
        ):
            skip_helper = True
            helper_depth = line.count("{") - line.count("}")
            continue
        if skip_multiline_call:
            if line.strip() == ")":
                skip_multiline_call = False
            continue
        if "appendUITestTrace(" in line or "appendUITestKeyTrace(" in line:
            if line.strip().endswith("("):
                skip_multiline_call = True
            continue
        stripped.append(line)
    result = "\n".join(stripped) + ("\n" if source.endswith("\n") else "")
    return result.replace("\n\n\n            switch action.kind", "\n\n            switch action.kind").replace(
        "\n        }\n\n\n        private static func undo",
        "\n        }\n\n        private static func undo",
    ).replace("\n\n}\n\n/// Represents", "\n}\n\n/// Represents").replace(
        "\n\n\n        private static func setInitialSize",
        "\n\n        private static func setInitialSize",
    )


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise AssertionError(f"missing {label}: {needle!r}")


def require_all(text: str, needles: list[tuple[str, str]]) -> None:
    for needle, label in needles:
        require(text, needle, label)


def assert_equal(left: str, right: str, label: str) -> None:
    if left == right:
        return
    diff = "\n".join(
        difflib.unified_diff(
            left.splitlines(),
            right.splitlines(),
            fromfile=f"ghostty-normalized/{label}",
            tofile=f"roastty/{label}",
            lineterm="",
        )
    )
    raise AssertionError(f"{label} differs after expected Roastty renames:\n{diff}")


def require_normalized_match(ghostty_path: str, roastty_path: str) -> str:
    roastty = strip_ui_trace_hooks(read(roastty_path))
    assert_equal(normalize_ghostty_swift(read(ghostty_path)), roastty, roastty_path)
    return roastty


def require_row(markdown: str, row_id: str) -> str:
    for line in markdown.splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if cells and cells[0] == row_id:
            return line
    raise AssertionError(f"missing inventory row {row_id}")


def main() -> int:
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()
    inventory_source = read("issues/0805-roastty-ghostty-parity/config_runtime_inventory.py")
    command_palette_guard = read(
        "issues/0805-roastty-ghostty-parity/command_palette_runtime_parity.py"
    )
    terminal_residual_guard = read(
        "issues/0805-roastty-ghostty-parity/terminal_runtime_residual_audit.py"
    )

    terminal_controller = require_normalized_match(
        "vendor/ghostty/macos/Sources/Features/Terminal/TerminalController.swift",
        "roastty/macos/Sources/Features/Terminal/TerminalController.swift",
    )
    terminal_window = require_normalized_match(
        "vendor/ghostty/macos/Sources/Features/Terminal/Window Styles/TerminalWindow.swift",
        "roastty/macos/Sources/Features/Terminal/Window Styles/TerminalWindow.swift",
    )
    split_tree = require_normalized_match(
        "vendor/ghostty/macos/Sources/Features/Splits/SplitTree.swift",
        "roastty/macos/Sources/Features/Splits/SplitTree.swift",
    )
    split_view = require_normalized_match(
        "vendor/ghostty/macos/Sources/Features/Splits/SplitView.swift",
        "roastty/macos/Sources/Features/Splits/SplitView.swift",
    )
    split_view_divider = require_normalized_match(
        "vendor/ghostty/macos/Sources/Features/Splits/SplitView.Divider.swift",
        "roastty/macos/Sources/Features/Splits/SplitView.Divider.swift",
    )
    terminal_split_tree_view = require_normalized_match(
        "vendor/ghostty/macos/Sources/Features/Splits/TerminalSplitTreeView.swift",
        "roastty/macos/Sources/Features/Splits/TerminalSplitTreeView.swift",
    )
    quick_terminal_controller = require_normalized_match(
        "vendor/ghostty/macos/Sources/Features/QuickTerminal/QuickTerminalController.swift",
        "roastty/macos/Sources/Features/QuickTerminal/QuickTerminalController.swift",
    )
    quick_terminal_intent = require_normalized_match(
        "vendor/ghostty/macos/Sources/Features/App Intents/QuickTerminalIntent.swift",
        "roastty/macos/Sources/Features/App Intents/QuickTerminalIntent.swift",
    )
    app_delegate = require_normalized_match(
        "vendor/ghostty/macos/Sources/App/macOS/AppDelegate.swift",
        "roastty/macos/Sources/App/macOS/AppDelegate.swift",
    )
    config_bridge = require_normalized_match(
        "vendor/ghostty/macos/Sources/Ghostty/Ghostty.Config.swift",
        "roastty/macos/Sources/Roastty/Roastty.Config.swift",
    )
    app_bridge = require_normalized_match(
        "vendor/ghostty/macos/Sources/Ghostty/Ghostty.App.swift",
        "roastty/macos/Sources/Roastty/Roastty.App.swift",
    )
    package_bridge = require_normalized_match(
        "vendor/ghostty/macos/Sources/Ghostty/GhosttyPackage.swift",
        "roastty/macos/Sources/Roastty/RoasttyPackage.swift",
    )
    fullscreen_mode = require_normalized_match(
        "vendor/ghostty/macos/Sources/Ghostty/FullscreenMode+Extension.swift",
        "roastty/macos/Sources/Roastty/FullscreenMode+Extension.swift",
    )

    base_controller = read("roastty/macos/Sources/Features/Terminal/BaseTerminalController.swift")
    split_tree_tests = read("roastty/macos/Tests/Splits/SplitTreeTests.swift")
    split_drop_tests = read("roastty/macos/Tests/Splits/TerminalSplitDropZoneTests.swift")

    require_all(
        terminal_controller,
        [
            ("static func newWindow(", "new window factory"),
            ("static func newTab(", "new tab factory"),
            ("@IBAction func newWindow", "new window menu action"),
            ("@IBAction func newTab", "new tab menu action"),
            ("@IBAction func closeTab", "close tab menu action"),
            ("@IBAction func closeOtherTabs", "close other tabs action"),
            ("@IBAction func closeTabsOnTheRight", "close right tabs action"),
            ("override func newWindowForTab", "native tab detach action"),
            ("@IBAction func toggleRoasttyFullScreen", "surface fullscreen action"),
            ("@objc private func onToggleFullscreen", "fullscreen notification action"),
            ("toggleFullscreen(mode: fullscreenMode)", "fullscreen mode dispatch"),
            ("macosTitlebarStyle", "titlebar style config"),
            ("macosWindowButtons", "window buttons config"),
        ],
    )
    require_all(
        terminal_window,
        [
            ("configureTabContextMenuIfNeeded", "tab context menu customization"),
            ("Close Tabs to the Right", "close-right menu item"),
            ("Rename Tab...", "rename tab context item"),
            ("makeTabColorPaletteView", "tab color context palette"),
            ("TabTitleEditorDelegate", "inline tab title editor delegate"),
            ("macosTitlebarStyle", "titlebar style derived config"),
            ("macosWindowButtons", "window buttons derived config"),
            ("resetZoomAccessory", "split zoom titlebar reset accessory"),
        ],
    )
    require_all(
        base_controller,
        [
            ("func toggleFullscreen(mode: FullscreenMode)", "base fullscreen toggle"),
            ("@IBAction func splitRight", "split right action"),
            ("@IBAction func splitLeft", "split left action"),
            ("@IBAction func splitDown", "split down action"),
            ("@IBAction func splitUp", "split up action"),
            ("@IBAction func splitZoom", "split zoom action"),
            ("@IBAction func equalizeSplits", "equalize splits action"),
            ("@IBAction func moveSplitDividerUp", "move split divider up action"),
            ("@IBAction func moveSplitDividerDown", "move split divider down action"),
            ("@IBAction func moveSplitDividerLeft", "move split divider left action"),
            ("@IBAction func moveSplitDividerRight", "move split divider right action"),
            ("private func splitMoveFocus", "split focus helper"),
            ("private func splitDidDrop", "split drop delegate"),
            ("private func splitDidResize", "split resize delegate"),
        ],
    )
    require_all(
        split_tree,
        [
            ("struct SplitTree", "split tree type"),
            ("func inserting(view:", "split insert helper"),
            ("func removing", "split remove helper"),
            ("func focusTarget", "split focus target helper"),
            ("func equalized", "split equalize helper"),
            ("func resizing", "split resize helper"),
        ],
    )
    require_all(
        split_view,
        [
            ("struct SplitView", "split view type"),
            ("let onEqualize: () -> Void", "split equalize callback"),
            ("private func dragGesture", "split drag resize callback"),
        ],
    )
    require_all(
        split_view_divider,
        [
            ("struct Divider: View", "split divider view"),
            ("@Binding var split: CGFloat", "split divider binding"),
            ("accessibilityAdjustableAction", "split divider accessibility resize"),
        ],
    )
    require_all(
        terminal_split_tree_view,
        [
            ("struct TerminalSplitTreeView", "terminal split tree view"),
            ("TerminalSplitDropZone", "drop-zone type"),
            ("func calculate(at", "drop-zone calculation"),
            ("onDrop", "drop callback"),
        ],
    )
    require_all(
        quick_terminal_controller,
        [
            ("class QuickTerminalController: BaseTerminalController", "quick terminal controller"),
            ("override var windowNibName", "quick terminal nib"),
            ("QuickTerminalWindow", "quick terminal window type"),
            ("saveScreenState(exitFullscreen: true)", "quick terminal state save"),
            ("private func animateWindowIn", "quick terminal show animation"),
            ("private func animateWindowOut", "quick terminal hide animation"),
            ("@IBAction func newTab", "quick terminal new tab action"),
            ("@IBAction func toggleRoasttyFullScreen", "quick terminal fullscreen action"),
            ("@objc private func onToggleFullscreen", "quick terminal fullscreen notification"),
            ("static let quickTerminalDidChangeVisibility", "quick terminal visibility notification"),
        ],
    )
    require_all(
        quick_terminal_intent,
        [
            ("struct QuickTerminalIntent", "quick terminal app intent"),
            ("let c = delegate.quickController", "quick terminal intent controller lookup"),
            ("c.animateIn()", "quick terminal intent show dispatch"),
            ("TerminalEntity($0)", "quick terminal intent terminal return"),
        ],
    )
    require_all(
        app_delegate,
        [
            ("@IBOutlet private var menuQuickTerminal", "quick terminal menu item"),
            ("var quickController: QuickTerminalController", "quick terminal controller accessor"),
            ("@IBAction func newWindow", "app delegate new window action"),
            ("@IBAction func newTab", "app delegate new tab action"),
            ("@IBAction func toggleQuickTerminal", "quick terminal menu action"),
            ("syncMenuShortcut(config, action: \"toggle_quick_terminal\"", "quick terminal shortcut sync"),
            ("private enum QuickTerminalState", "quick terminal state enum"),
        ],
    )
    require_all(
        config_bridge,
        [
            ("var windowFullscreen: FullscreenMode?", "window fullscreen config"),
            ("var windowFullscreenMode: FullscreenMode", "window fullscreen mode config"),
            ("var macosWindowButtons", "macOS window buttons config"),
            ("var macosTitlebarStyle", "macOS titlebar style config"),
            ("var quickTerminalPosition", "quick terminal position config"),
            ("var quickTerminalScreen", "quick terminal screen config"),
            ("var quickTerminalSpaceBehavior", "quick terminal space behavior config"),
            ("var quickTerminalSize", "quick terminal size config"),
        ],
    )
    require_all(
        app_bridge,
        [
            ("func newTab(surface: roastty_surface_t)", "surface new-tab callback"),
            ("func newWindow(surface: roastty_surface_t)", "surface new-window callback"),
            ("func split(surface: roastty_surface_t", "surface split callback"),
            ("func splitMoveFocus(surface: roastty_surface_t", "surface split focus callback"),
            ("func splitResize(surface: roastty_surface_t", "surface split resize callback"),
            ("func splitEqualize(surface: roastty_surface_t)", "surface split equalize callback"),
            ("func splitToggleZoom(surface: roastty_surface_t)", "surface split zoom callback"),
            ("func toggleFullscreen(surface: roastty_surface_t)", "surface fullscreen callback"),
            ("private static func newWindow", "action new-window handler"),
            ("private static func newTab", "action new-tab handler"),
            ("private static func closeTab", "action close-tab handler"),
            ("private static func moveTab", "action move-tab handler"),
            ("private static func gotoTab", "action goto-tab handler"),
            ("private static func resizeSplit", "action resize-split handler"),
            ("private static func equalizeSplits", "action equalize-split handler"),
            ("private static func toggleQuickTerminal", "action quick-terminal handler"),
        ],
    )
    require_all(
        package_bridge,
        [
            ("static let roasttyToggleFullscreen", "fullscreen notification name"),
            ("static let FullscreenModeKey", "fullscreen notification mode key"),
        ],
    )
    require_all(
        fullscreen_mode,
        [
            ("static func from(roastty:", "fullscreen ABI enum bridge"),
            ("ROASTTY_FULLSCREEN_NATIVE", "native fullscreen bridge"),
            ("ROASTTY_FULLSCREEN_MACOS_NON_NATIVE", "non-native fullscreen bridge"),
            ("ROASTTY_FULLSCREEN_MACOS_NON_NATIVE_VISIBLE_MENU", "visible-menu fullscreen bridge"),
            ("ROASTTY_FULLSCREEN_MACOS_NON_NATIVE_PADDED_NOTCH", "padded-notch fullscreen bridge"),
        ],
    )
    require_all(
        split_tree_tests,
        [
            ("struct SplitTreeTests", "split tree test suite"),
            ("focusTargetShouldFindNextFocusedNode", "split focus test"),
            ("equalizedAdjustsRatioByLeafCount", "split equalize test"),
            ("resizingAdjustsRatio", "split resize test"),
            ("encodingAndDecodingPreservesTree", "split state coding test"),
        ],
    )
    require_all(
        split_drop_tests,
        [
            ("struct TerminalSplitDropZoneTests", "drop-zone test suite"),
            ("topEdge", "top drop-zone test"),
            ("bottomEdge", "bottom drop-zone test"),
            ("leftEdge", "left drop-zone test"),
            ("rightEdge", "right drop-zone test"),
            ("centerSelectsLeft", "center tie-break test"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-011B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-011B1 status"),
            ("copied macOS workflow plumbing", "RUNTIME-011B1 behavior"),
            ("TerminalController.swift", "terminal controller evidence"),
            ("TerminalWindow.swift", "terminal window evidence"),
            ("QuickTerminalController.swift", "quick terminal evidence"),
            ("SplitTreeTests", "split tests evidence"),
            ("macos_app_workflow_plumbing_parity.py", "RUNTIME-011B1 guard"),
        ],
    )

    row_macos_residual = require_row(runtime_inventory, "RUNTIME-011B2B")
    require_all(
        row_macos_residual,
        [
            ("Oracle complete", "RUNTIME-011B2B status"),
            ("macOS walkthrough residual row", "RUNTIME-011B2B evidence"),
            ("Experiment 185", "RUNTIME-011B2B experiment"),
            ("macos_walkthrough_residual_parity.py", "RUNTIME-011B2B guard"),
        ],
    )

    if "| RUNTIME-011B " in runtime_inventory:
        raise AssertionError("old broad RUNTIME-011B row is still present")

    require_all(
        inventory_source,
        [
            ('id="RUNTIME-011B1"', "source workflow-plumbing row"),
            ('id="RUNTIME-011B2B"', "source macOS residual row"),
            ("Experiment 185 closes the macOS walkthrough residual row", "source macOS residual closure"),
            ("macos_app_workflow_plumbing_parity.py", "source guard command"),
        ],
    )
    require_all(
        command_palette_guard,
        [
            ('require_row(runtime_inventory, "RUNTIME-011B2B")', "command palette gap id update"),
            ("89 rows Oracle complete", "command palette CFG-223 oracle count"),
            ("92 rows closed", "command palette CFG-223 closed count"),
        ],
    )
    require_all(
        terminal_residual_guard,
        [
            ('("RUNTIME-011B2B", "macOS residual row remains tracked")', "terminal residual macOS id update"),
            ("89 rows Oracle complete", "terminal residual CFG-223 oracle count"),
            ("92 rows closed", "terminal residual CFG-223 closed count"),
        ],
    )

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

    print("macos_app_workflow_plumbing_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
