#!/usr/bin/env python3
"""Guard copied macOS link-hover banner plumbing for Issue 805 CFG-223."""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ISSUE = ROOT / "issues/0805-roastty-ghostty-parity"


RENAMES = (
    ("GhosttyKit", "RoasttyKit"),
    ("Ghostty", "Roastty"),
    ("ghostty", "roastty"),
    ("GHOSTTY", "ROASTTY"),
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
    return result.replace("\n\n\n            switch action.kind", "\n\n            switch action.kind").replace("\n        }\n\n\n        private static func undo", "\n        }\n\n        private static func undo").replace("\n\n}\n\n/// Represents", "\n}\n\n/// Represents").replace("\n\n\n        private static func setInitialSize", "\n\n        private static func setInitialSize")


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise AssertionError(f"missing {label}: {needle!r}")


def require_all(text: str, needles: list[tuple[str, str]]) -> None:
    for needle, label in needles:
        require(text, needle, label)


def require_normalized_match(ghostty_path: str, roastty_path: str) -> str:
    ghostty = normalize_ghostty_swift(read(ghostty_path))
    roastty = strip_ui_trace_hooks(read(roastty_path))
    if ghostty != roastty:
        raise AssertionError(f"normalized Swift mismatch: {ghostty_path} != {roastty_path}")
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

    os_surface = require_normalized_match(
        "vendor/ghostty/macos/Sources/Ghostty/Surface View/OSSurfaceView.swift",
        "roastty/macos/Sources/Roastty/Surface View/OSSurfaceView.swift",
    )
    surface_view = require_normalized_match(
        "vendor/ghostty/macos/Sources/Ghostty/Surface View/SurfaceView.swift",
        "roastty/macos/Sources/Roastty/Surface View/SurfaceView.swift",
    )
    hover_banner = require_normalized_match(
        "vendor/ghostty/macos/Sources/Helpers/URLHoverBanner.swift",
        "roastty/macos/Sources/Helpers/URLHoverBanner.swift",
    )
    app = require_normalized_match(
        "vendor/ghostty/macos/Sources/Ghostty/Ghostty.App.swift",
        "roastty/macos/Sources/Roastty/Roastty.App.swift",
    )

    require_all(
        os_surface,
        [
            ("@Published var hoverUrl: String?", "Roastty hoverUrl published state"),
        ],
    )
    require_all(
        surface_view,
        [
            ("if let url = surfaceView.hoverUrl", "Roastty hoverUrl view gate"),
            ("URLHoverBanner(url: url)", "Roastty URLHoverBanner render"),
        ],
    )
    require_all(
        hover_banner,
        [
            ("struct URLHoverBanner: View", "Roastty URLHoverBanner type"),
            ("Text(verbatim: url)", "Roastty literal URL text"),
            ("truncationMode(.middle)", "Roastty URL middle truncation"),
            ("isHoveringURLLeft ? 1 : 0", "Roastty left/right opacity switch"),
            (".onHover(perform:", "Roastty banner side hover handling"),
        ],
    )
    require_all(
        app,
        [
            ("setMouseOverLink(app, target: target, v: action.action.mouse_over_link)", "Roastty action dispatch"),
            ("private static func setMouseOverLink", "Roastty setMouseOverLink handler"),
            ("case ROASTTY_TARGET_SURFACE", "Roastty surface target branch"),
            ("guard v.len > 0 else", "Roastty clear-hover branch"),
            ("surfaceView.hoverUrl = nil", "Roastty clear hover URL"),
            ("Data(bytes: v.url!, count: v.len)", "Roastty hover URL byte copy"),
            ("surfaceView.hoverUrl = String(data: buffer, encoding: .utf8)", "Roastty hover URL decode"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-012B2B2B2B2A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-012B2B2B2B2A status"),
            ("copied macOS link-hover banner plumbing", "RUNTIME-012B2B2B2B2A behavior"),
            ("OSSurfaceView.swift", "RUNTIME-012B2B2B2B2A surface view evidence"),
            ("hoverUrl", "RUNTIME-012B2B2B2B2A hoverUrl evidence"),
            ("URLHoverBanner", "RUNTIME-012B2B2B2B2A banner evidence"),
            ("setMouseOverLink", "RUNTIME-012B2B2B2B2A action handler evidence"),
            ("macos_link_hover_banner_runtime_parity.py", "RUNTIME-012B2B2B2B2A guard evidence"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-012B2B2B2B2B3C")
    require_all(
        row_gap,
        [
            ("Gap", "RUNTIME-012B2B2B2B2B3C status"),
            ("actual OS notification delivery/banner/sound", "remaining OS notification gap"),
            ("audible bell output", "remaining bell GUI gap"),
            ("real OS cursor pixels", "remaining real hover UI gap"),
            ("native link preview display", "remaining link preview gap"),
            ("external Launch Services handler delivery", "remaining external URL-handler gap"),
        ],
    )

    require_all(
        inventory_source,
        [
            ('id="RUNTIME-012B2B2B2B2A"', "source link-hover row"),
            ('id="RUNTIME-012B2B2B2B2B3C"', "source reduced gap row"),
            ("macos_link_hover_banner_runtime_parity.py", "source guard command"),
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

    print("macos_link_hover_banner_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
