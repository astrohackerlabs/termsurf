#!/usr/bin/env python3
"""Guard app-notifications platform classification for Issue 805 CFG-223."""

from __future__ import annotations

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


def require_absent(text: str, needle: str, label: str) -> None:
    if needle in text:
        raise AssertionError(f"unexpected {label}: {needle!r}")


def require_row(markdown: str, row_id: str) -> str:
    for line in markdown.splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if cells and cells[0] == row_id:
            return line
    raise AssertionError(f"missing inventory row {row_id}")


def main() -> int:
    ghostty_config = read("vendor/ghostty/src/config/Config.zig")
    ghostty_gtk_window = read("vendor/ghostty/src/apprt/gtk/class/window.zig")
    roastty_config = read("roastty/src/config/mod.rs")
    runtime_inventory_source = read("issues/0805-roastty-ghostty-parity/config_runtime_inventory.py")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    ghostty_macos_text = "\n".join(
        path.read_text()
        for path in sorted((ROOT / "vendor/ghostty/macos/Sources").rglob("*.swift"))
    )
    roastty_macos_text = "\n".join(
        path.read_text()
        for path in sorted((ROOT / "roastty/macos/Sources").rglob("*.swift"))
    )
    roastty_non_config_runtime_text = "\n".join(
        path.read_text(errors="ignore")
        for path in sorted((ROOT / "roastty/src").rglob("*.rs"))
        if path.as_posix() != (ROOT / "roastty/src/config/mod.rs").as_posix()
    )
    ghostty_non_gtk_runtime_text = "\n".join(
        path.read_text(errors="ignore")
        for path in sorted((ROOT / "vendor/ghostty/src").rglob("*.zig"))
        if "src/apprt/gtk" not in path.as_posix()
        and path.as_posix() != (ROOT / "vendor/ghostty/src/config/Config.zig").as_posix()
    )

    require_all(
        ghostty_config,
        [
            ("@\"app-notifications\": AppNotifications = .{}", "Ghostty app-notifications field"),
            ("This configuration only applies to GTK.", "Ghostty GTK-only documentation"),
            ("pub const AppNotifications = packed struct", "Ghostty packed struct"),
            ("@\"clipboard-copy\": bool = true", "Ghostty clipboard-copy flag"),
            ("@\"config-reload\": bool = true", "Ghostty config-reload flag"),
        ],
    )
    require_all(
        ghostty_gtk_window,
        [
            ("config.@\"app-notifications\".@\"config-reload\"", "Ghostty GTK config-reload consumer"),
            ("config.@\"app-notifications\".@\"clipboard-copy\"", "Ghostty GTK clipboard-copy consumer"),
        ],
    )
    require_absent(
        ghostty_macos_text,
        "app-notifications",
        "Ghostty macOS app-notifications runtime consumer",
    )
    require_absent(
        ghostty_non_gtk_runtime_text,
        "app-notifications",
        "Ghostty non-GTK app-notifications runtime consumer",
    )

    require_all(
        roastty_config,
        [
            ("pub app_notifications: AppNotifications", "Roastty app-notifications config field"),
            ("fn app_notifications_config_parse_format_reset_and_diagnose", "Roastty parser/formatter test"),
            ("app-notifications = clipboard-copy,config-reload", "Roastty default formatter evidence"),
            ("app-notifications = no-clipboard-copy,no-config-reload", "Roastty false formatter evidence"),
        ],
    )
    require_absent(
        roastty_macos_text,
        "app-notifications",
        "Roastty macOS app-notifications runtime consumer",
    )
    for needle in ("app-notifications", "app_notifications", "AppNotifications"):
        require_absent(
            roastty_non_config_runtime_text,
            needle,
            "Roastty non-config Rust app-notifications runtime consumer",
        )

    row_na = require_row(runtime_inventory, "RUNTIME-012B2B2B2B1")
    require_all(
        row_na,
        [
            ("Not applicable", "RUNTIME-012B2B2B2B1 status"),
            ("app-notifications GTK-only runtime effects", "RUNTIME-012B2B2B2B1 behavior"),
            ("This configuration only applies to GTK", "RUNTIME-012B2B2B1 GTK-only evidence"),
            ("src/apprt/gtk", "RUNTIME-012B2B2B2B1 GTK consumer evidence"),
            ("app_notifications_platform_runtime_parity.py", "RUNTIME-012B2B2B2B1 guard evidence"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-012B2B2B2B2B3")
    require_all(
        row_gap,
        [
            ("Gap", "RUNTIME-012B2B2B2B2B3 status"),
            ("Actual OS banner/sound delivery", "remaining OS notification gap"),
            ("actual audio/dock/border/title GUI effects", "remaining bell GUI gap"),
            ("hover/cursor UI", "remaining hover cursor gap"),
            ("native link preview display", "remaining link preview gap"),
            ("native context/menu display", "remaining context menu gap"),
        ],
    )
    if "Add app-notification" in row_gap or "app-notifications still need" in row_gap:
        raise AssertionError("remaining notification/link/bell gap still lists app-notifications")

    require_all(
        runtime_inventory_source,
        [
            ('id="RUNTIME-012B2B2B2B1"', "source not-applicable row"),
            ('status="Not applicable"', "source not-applicable status"),
            ('id="RUNTIME-012B2B2B2B2B3"', "source reduced gap row"),
        ],
    )

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("80 rows Oracle complete", "CFG-223 oracle count"),
            ("83 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("app_notifications_platform_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
