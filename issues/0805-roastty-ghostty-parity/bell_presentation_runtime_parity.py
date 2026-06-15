#!/usr/bin/env python3
"""Guard copied macOS bell presentation parity for Issue 805 CFG-223."""

from __future__ import annotations

import difflib
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ISSUE = ROOT / "issues/0805-roastty-ghostty-parity"


def read(path: str) -> str:
    return (ROOT / path).read_text()


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise AssertionError(f"missing {label}: {needle!r}")


def require_regex(text: str, pattern: str, label: str) -> None:
    if re.search(pattern, text, flags=re.DOTALL) is None:
        raise AssertionError(f"missing {label}: {pattern!r}")


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
    return strip_ui_trace_hooks(
        source.replace("Ghostty", "Roastty")
        .replace("ghostty", "roastty")
        .replace("GHOSTTY", "ROASTTY")
        .replace("libghostty", "libroastty")
    )


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
        if "func appendUITestTrace(" in line:
            skip_helper = True
            helper_depth = line.count("{") - line.count("}")
            continue
        if skip_multiline_call:
            if line.strip() == ")":
                skip_multiline_call = False
            continue
        if "appendUITestTrace(" in line:
            if line.strip().endswith("("):
                skip_multiline_call = True
            continue
        stripped.append(line)
    result = "\n".join(stripped) + ("\n" if source.endswith("\n") else "")
    return result.replace("\n\n}\n\n/// Represents", "\n}\n\n/// Represents").replace("\n\n\n        private static func setInitialSize", "\n\n        private static func setInitialSize")


def assert_sources_match_after_rename(
    ghostty_path: str,
    roastty_path: str,
    label: str,
) -> None:
    ghostty_source = read(ghostty_path)
    roastty_source = strip_ui_trace_hooks(read(roastty_path))
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
        "vendor/ghostty/macos/Sources/App/macOS/AppDelegate.swift",
        "roastty/macos/Sources/App/macOS/AppDelegate.swift",
        "AppDelegate.swift",
    )
    assert_sources_match_after_rename(
        "vendor/ghostty/macos/Sources/Ghostty/Surface View/SurfaceView.swift",
        "roastty/macos/Sources/Roastty/Surface View/SurfaceView.swift",
        "SurfaceView.swift",
    )

    ghostty_controller = read(
        "vendor/ghostty/macos/Sources/Features/Terminal/BaseTerminalController.swift"
    )
    roastty_controller = read(
        "roastty/macos/Sources/Features/Terminal/BaseTerminalController.swift"
    )
    ghostty_app_delegate = read("vendor/ghostty/macos/Sources/App/macOS/AppDelegate.swift")
    roastty_app_delegate = read("roastty/macos/Sources/App/macOS/AppDelegate.swift")
    ghostty_surface_view = read(
        "vendor/ghostty/macos/Sources/Ghostty/Surface View/SurfaceView.swift"
    )
    roastty_surface_view = read(
        "roastty/macos/Sources/Roastty/Surface View/SurfaceView.swift"
    )
    ghostty_surface_appkit = read(
        "vendor/ghostty/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift"
    )
    roastty_surface_appkit = read(
        "roastty/macos/Sources/Roastty/Surface View/SurfaceView_AppKit.swift"
    )
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_controller,
        [
            ("titleSurface.$title", "Ghostty title publisher"),
            (".combineLatest(titleSurface.$bell)", "Ghostty title bell publisher"),
            ("computeTitle(title: $0, bell: $1)", "Ghostty title recompute callback"),
            ("if bell && ghostty.config.bellFeatures.contains(.title)", "Ghostty title bell gate"),
            ('result = "🔔 \\(result)"', "Ghostty bell title prefix"),
            ("private var bellStateCancellable: AnyCancellable?", "Ghostty bell cancellable"),
            ("setupBellNotificationPublisher()", "Ghostty bell publisher setup"),
            ("surfaceValuesPublisher(valueKeyPath: \\.bell, publisherKeyPath: \\.$bell)", "Ghostty aggregate bell source"),
            (".map { $0.values.contains(true) }", "Ghostty aggregate bell fold"),
            (".removeDuplicates()", "Ghostty duplicate suppression"),
            (".receive(on: DispatchQueue.main)", "Ghostty main queue delivery"),
            ("name: .terminalWindowBellDidChangeNotification", "Ghostty bell notification"),
            ("userInfo: [Notification.Name.terminalWindowHasBellKey: hasBell]", "Ghostty hasBell userInfo"),
            ("bell = false", "Ghostty close-time bell clear"),
            ("userInfo: [Notification.Name.terminalWindowHasBellKey: false]", "Ghostty close-time false userInfo"),
            ('Notification.Name("com.mitchellh.ghostty.terminalWindowBellDidChange")', "Ghostty notification name"),
        ],
    )
    require_all(
        roastty_controller,
        [
            ("titleSurface.$title", "Roastty title publisher"),
            (".combineLatest(titleSurface.$bell)", "Roastty title bell publisher"),
            ("computeTitle(title: $0, bell: $1)", "Roastty title recompute callback"),
            ("if bell && roastty.config.bellFeatures.contains(.title)", "Roastty title bell gate"),
            ('result = "🔔 \\(result)"', "Roastty bell title prefix"),
            ("private var bellStateCancellable: AnyCancellable?", "Roastty bell cancellable"),
            ("setupBellNotificationPublisher()", "Roastty bell publisher setup"),
            ("surfaceValuesPublisher(valueKeyPath: \\.bell, publisherKeyPath: \\.$bell)", "Roastty aggregate bell source"),
            (".map { $0.values.contains(true) }", "Roastty aggregate bell fold"),
            (".removeDuplicates()", "Roastty duplicate suppression"),
            (".receive(on: DispatchQueue.main)", "Roastty main queue delivery"),
            ("name: .terminalWindowBellDidChangeNotification", "Roastty bell notification"),
            ("userInfo: [Notification.Name.terminalWindowHasBellKey: hasBell]", "Roastty hasBell userInfo"),
            ("bell = false", "Roastty close-time bell clear"),
            ("userInfo: [Notification.Name.terminalWindowHasBellKey: false]", "Roastty close-time false userInfo"),
            ('Notification.Name("com.mitchellh.roastty.terminalWindowBellDidChange")', "Roastty notification name"),
        ],
    )
    require_regex(
        roastty_controller,
        r"titleSurface\.\$title\s*\.combineLatest\(titleSurface\.\$bell\)\s*\.map\s*\{[^}]*computeTitle\(title: \$0, bell: \$1\)",
        "Roastty focused title and bell recompute chain",
    )

    require_all(
        ghostty_app_delegate,
        [
            ("selector: #selector(ghosttyBellDidRing(_:))", "Ghostty bell action observer selector"),
            ("name: .ghosttyBellDidRing", "Ghostty bell action observer"),
            ("selector: #selector(terminalWindowHasBell(_:))", "Ghostty terminal bell observer selector"),
            ("name: .terminalWindowBellDidChangeNotification", "Ghostty terminal bell observer"),
            ("@objc private func ghosttyBellDidRing", "Ghostty bell action handler"),
            ("if ghostty.config.bellFeatures.contains(.system)", "Ghostty system bell gate"),
            ("NSSound.beep()", "Ghostty system bell"),
            ("if ghostty.config.bellFeatures.contains(.audio)", "Ghostty audio bell gate"),
            ("NSSound(contentsOfFile: configPath.path, byReference: false)", "Ghostty audio bell file"),
            ("sound.volume = ghostty.config.bellAudioVolume", "Ghostty audio volume"),
            ("sound.play()", "Ghostty audio playback"),
            ("if ghostty.config.bellFeatures.contains(.attention)", "Ghostty attention gate"),
            ("NSApp.requestUserAttention(.informationalRequest)", "Ghostty attention request"),
            ("private func syncDockBadge()", "Ghostty dock badge sync"),
            ("private func setDockBadge()", "Ghostty dock badge setter"),
            ("compactMap { $0.windowController as? BaseTerminalController }", "Ghostty terminal window count"),
            ("reduce(0) { $0 + ($1.bell ? 1 : 0) }", "Ghostty bell count"),
            ("let wantsBadge = ghostty.config.bellFeatures.contains(.attention) && bellCount > 0", "Ghostty dock badge attention gate"),
            ('bellCount > 99 ? "99+" : String(bellCount)', "Ghostty dock badge cap"),
        ],
    )
    require_all(
        roastty_app_delegate,
        [
            ("selector: #selector(roasttyBellDidRing(_:))", "Roastty bell action observer selector"),
            ("name: .roasttyBellDidRing", "Roastty bell action observer"),
            ("selector: #selector(terminalWindowHasBell(_:))", "Roastty terminal bell observer selector"),
            ("name: .terminalWindowBellDidChangeNotification", "Roastty terminal bell observer"),
            ("@objc private func roasttyBellDidRing", "Roastty bell action handler"),
            ("if roastty.config.bellFeatures.contains(.system)", "Roastty system bell gate"),
            ("NSSound.beep()", "Roastty system bell"),
            ("if roastty.config.bellFeatures.contains(.audio)", "Roastty audio bell gate"),
            ("NSSound(contentsOfFile: configPath.path, byReference: false)", "Roastty audio bell file"),
            ("sound.volume = roastty.config.bellAudioVolume", "Roastty audio volume"),
            ("sound.play()", "Roastty audio playback"),
            ("if roastty.config.bellFeatures.contains(.attention)", "Roastty attention gate"),
            ("NSApp.requestUserAttention(.informationalRequest)", "Roastty attention request"),
            ("private func syncDockBadge()", "Roastty dock badge sync"),
            ("private func setDockBadge()", "Roastty dock badge setter"),
            ("compactMap { $0.windowController as? BaseTerminalController }", "Roastty terminal window count"),
            ("reduce(0) { $0 + ($1.bell ? 1 : 0) }", "Roastty bell count"),
            ("let wantsBadge = roastty.config.bellFeatures.contains(.attention) && bellCount > 0", "Roastty dock badge attention gate"),
            ('bellCount > 99 ? "99+" : String(bellCount)', "Roastty dock badge cap"),
        ],
    )

    require_all(
        ghostty_surface_view,
        [
            ("if ghostty.config.bellFeatures.contains(.border)", "Ghostty bell border gate"),
            ("BellBorderOverlay(bell: surfaceView.bell)", "Ghostty bell border overlay call"),
            ("struct BellBorderOverlay: View", "Ghostty bell border overlay type"),
            (".opacity(bell ? 1.0 : 0.0)", "Ghostty bell border opacity"),
            (".animation(.easeInOut(duration: 0.3), value: bell)", "Ghostty bell border animation"),
        ],
    )
    require_all(
        roastty_surface_view,
        [
            ("if roastty.config.bellFeatures.contains(.border)", "Roastty bell border gate"),
            ("BellBorderOverlay(bell: surfaceView.bell)", "Roastty bell border overlay call"),
            ("struct BellBorderOverlay: View", "Roastty bell border overlay type"),
            (".opacity(bell ? 1.0 : 0.0)", "Roastty bell border opacity"),
            (".animation(.easeInOut(duration: 0.3), value: bell)", "Roastty bell border animation"),
        ],
    )
    require_all(
        ghostty_surface_appkit,
        [
            ("@Published private(set) var bell: Bool = false", "Ghostty surface bell state"),
            ("selector: #selector(ghosttyBellDidRing(_:))", "Ghostty surface bell observer selector"),
            ("name: .ghosttyBellDidRing", "Ghostty surface bell observer"),
            ("@objc private func ghosttyBellDidRing", "Ghostty surface bell handler"),
            ("bell = true", "Ghostty surface bell set"),
        ],
    )
    require_all(
        roastty_surface_appkit,
        [
            ("@Published private(set) var bell: Bool = false", "Roastty surface bell state"),
            ("selector: #selector(roasttyBellDidRing(_:))", "Roastty surface bell observer selector"),
            ("name: .roasttyBellDidRing", "Roastty surface bell observer"),
            ("@objc private func roasttyBellDidRing", "Roastty surface bell handler"),
            ("bell = true", "Roastty surface bell set"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-012B2B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-012B2B1 status"),
            ("bell presentation", "RUNTIME-012B2B1 behavior"),
            ("BaseTerminalController.swift", "RUNTIME-012B2B1 controller evidence"),
            ("AppDelegate.swift", "RUNTIME-012B2B1 app delegate evidence"),
            ("SurfaceView", "RUNTIME-012B2B1 surface evidence"),
            ("bell-features = system", "RUNTIME-012B2B1 system evidence"),
            ("bell-features = audio", "RUNTIME-012B2B1 audio evidence"),
            ("bell-features = attention", "RUNTIME-012B2B1 attention evidence"),
            ("bell-features = title", "RUNTIME-012B2B1 title evidence"),
            ("bell-features = border", "RUNTIME-012B2B1 border evidence"),
            ("bell_presentation_runtime_parity.py", "RUNTIME-012B2B1 guard"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-012B2B2B2B2B3C")
    require_all(
        row_gap,
        [
            ("Gap", "RUNTIME-012B2B2B2B2B3C status"),
            ("actual OS notification delivery/banner/sound", "RUNTIME-012B2B2B2B2B3C OS notification delivery gap"),
            ("audible bell output", "RUNTIME-012B2B2B2B2B3C actual GUI effect gap"),
            ("real OS cursor pixels", "RUNTIME-012B2B2B2B2B3C hover cursor gap"),
            ("native link preview display", "RUNTIME-012B2B2B2B2B3C link preview gap"),
            ("external Launch Services handler delivery", "RUNTIME-012B2B2B2B2B3C external URL-handler gap"),
        ],
    )
    if "RUNTIME-012B2B |" in runtime_inventory:
        raise AssertionError("old broad RUNTIME-012B2B row is still present")

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

    print("bell_presentation_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
