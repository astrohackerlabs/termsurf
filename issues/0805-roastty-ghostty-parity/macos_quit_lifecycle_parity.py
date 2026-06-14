#!/usr/bin/env python3
"""Check macOS quit-after-last-window lifecycle parity for Issue 805."""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]

GHOSTTY_APP_DELEGATE = ROOT / "vendor/ghostty/macos/Sources/App/macOS/AppDelegate.swift"
ROASTTY_APP_DELEGATE = ROOT / "roastty/macos/Sources/App/macOS/AppDelegate.swift"
GHOSTTY_CONFIG_SWIFT = ROOT / "vendor/ghostty/macos/Sources/Ghostty/Ghostty.Config.swift"
ROASTTY_CONFIG_SWIFT = ROOT / "roastty/macos/Sources/Roastty/Roastty.Config.swift"
GHOSTTY_CONFIG_ZIG = ROOT / "vendor/ghostty/src/config/Config.zig"
GHOSTTY_GTK_APP = ROOT / "vendor/ghostty/src/apprt/gtk/class/application.zig"
ROASTTY_LIB = ROOT / "roastty/src/lib.rs"


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def normalize_names(text: str) -> str:
    return text.replace("Ghostty", "Roastty").replace("ghostty", "roastty")


def normalize_space(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


def extract_block(source: str, marker: str) -> str:
    start = source.find(marker)
    if start == -1:
        raise AssertionError(f"missing marker: {marker}")
    brace = source.find("{", start)
    if brace == -1:
        raise AssertionError(f"missing opening brace for marker: {marker}")

    depth = 0
    for index in range(brace, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start : index + 1]

    raise AssertionError(f"unterminated block for marker: {marker}")


def assert_renamed_equal(ghostty_source: str, roastty_source: str, marker: str) -> None:
    ghostty_block = normalize_space(normalize_names(extract_block(ghostty_source, marker)))
    roastty_marker = normalize_names(marker)
    roastty_block = normalize_space(extract_block(roastty_source, roastty_marker))
    if ghostty_block != roastty_block:
        raise AssertionError(
            f"Roastty block diverges from renamed Ghostty block for {marker!r}"
        )


def assert_roastty_config_get_exposes_quit_flag() -> None:
    source = read(ROASTTY_LIB)
    start = source.find('b"quit-after-last-window-closed" => {')
    end = source.find('b"auto-update-channel" => {', start)
    if start == -1 or end == -1:
        raise AssertionError("roastty_config_get is missing quit-after-last-window-closed")
    body = source[start:end]
    required = [
        "config_from_handle(config)",
        "output",
        ".cast::<bool>()",
        ".write(config.parsed.quit_after_last_window_closed)",
    ]
    missing = [needle for needle in required if needle not in body]
    if missing:
        raise AssertionError(
            "roastty_config_get quit-after-last-window-closed arm missing "
            + ", ".join(missing)
        )


def assert_delay_is_linux_only_for_macos_app() -> None:
    config_zig = read(GHOSTTY_CONFIG_ZIG)
    if "Only implemented on Linux." not in config_zig:
        raise AssertionError(
            "Pinned Ghostty no longer documents "
            "quit-after-last-window-closed-delay as Linux-only"
        )

    gtk_app = read(GHOSTTY_GTK_APP)
    if 'config.@"quit-after-last-window-closed-delay"' not in gtk_app:
        raise AssertionError(
            "Pinned Ghostty GTK app no longer consumes "
            "quit-after-last-window-closed-delay"
        )
    if "startQuitTimer" not in gtk_app or "handleQuitTimerExpired" not in gtk_app:
        raise AssertionError("Pinned Ghostty GTK quit timer markers are missing")

    macos_swift = "\n".join(
        path.read_text(encoding="utf-8")
        for root in [
            ROOT / "vendor/ghostty/macos/Sources",
            ROOT / "roastty/macos/Sources",
        ]
        for path in root.rglob("*.swift")
    )
    if "quit-after-last-window-closed-delay" in macos_swift:
        raise AssertionError(
            "A macOS Swift source now consumes quit-after-last-window-closed-delay"
        )


def main() -> int:
    ghostty_app_delegate = read(GHOSTTY_APP_DELEGATE)
    roastty_app_delegate = read(ROASTTY_APP_DELEGATE)
    ghostty_config_swift = read(GHOSTTY_CONFIG_SWIFT)
    roastty_config_swift = read(ROASTTY_CONFIG_SWIFT)

    assert_renamed_equal(
        ghostty_app_delegate,
        roastty_app_delegate,
        "func applicationShouldTerminateAfterLastWindowClosed",
    )
    assert_renamed_equal(
        ghostty_app_delegate,
        roastty_app_delegate,
        "private struct DerivedConfig",
    )
    assert_renamed_equal(
        ghostty_config_swift,
        roastty_config_swift,
        "var shouldQuitAfterLastWindowClosed",
    )
    assert_roastty_config_get_exposes_quit_flag()
    assert_delay_is_linux_only_for_macos_app()

    print("macos_quit_lifecycle_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
