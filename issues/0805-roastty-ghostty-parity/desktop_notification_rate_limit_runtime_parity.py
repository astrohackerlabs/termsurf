#!/usr/bin/env python3
"""Guard desktop notification rate-limit parity for Issue 805 CFG-223."""

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


def require_row(markdown: str, row_id: str) -> str:
    for line in markdown.splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if cells and cells[0] == row_id:
            return line
    raise AssertionError(f"missing inventory row {row_id}")


def main() -> int:
    ghostty_app = read("vendor/ghostty/src/App.zig")
    ghostty_surface = read("vendor/ghostty/src/Surface.zig")
    roastty_lib = read("roastty/src/lib.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_app,
        [
            ("last_notification_time: ?std.time.Instant = null", "Ghostty app notification time state"),
            ("last_notification_digest: u64 = 0", "Ghostty app notification digest state"),
        ],
    )
    require_all(
        ghostty_surface,
        [
            ("fn showDesktopNotification", "Ghostty notification helper"),
            ("const hash_algorithm = std.hash.Wyhash", "Ghostty Wyhash algorithm"),
            ("const now = try std.time.Instant.now()", "Ghostty notification instant"),
            ("now.since(last) < 1 * std.time.ns_per_s", "Ghostty one-second throttle"),
            ("hash.update(title)", "Ghostty title digest input"),
            ("hash.update(body)", "Ghostty body digest input"),
            ("last_notification_digest == new_digest", "Ghostty identical digest check"),
            ("now.since(last) < 5 * std.time.ns_per_s", "Ghostty five-second identical throttle"),
            ("self.app.last_notification_time = now", "Ghostty state time update"),
            ("self.app.last_notification_digest = new_digest", "Ghostty state digest update"),
            (".desktop_notification", "Ghostty app action dispatch"),
        ],
    )

    require_all(
        roastty_lib,
        [
            (
                "const DESKTOP_NOTIFICATION_RATE_LIMIT: std::time::Duration = std::time::Duration::from_secs(1)",
                "Roastty one-second limit",
            ),
            (
                "const DESKTOP_NOTIFICATION_IDENTICAL_RATE_LIMIT: std::time::Duration =\n    std::time::Duration::from_secs(5)",
                "Roastty five-second identical limit",
            ),
            ("last_notification_time: Option<std::time::Instant>", "Roastty app notification time state"),
            ("last_notification_identity: Vec<u8>", "Roastty app notification identity state"),
            ("fn allow_desktop_notification_at", "Roastty explicit-time limiter"),
            ("now.duration_since(last) < DESKTOP_NOTIFICATION_RATE_LIMIT", "Roastty one-second check"),
            ("self.last_notification_identity == identity", "Roastty identical identity check"),
            (
                "now.duration_since(last) < DESKTOP_NOTIFICATION_IDENTICAL_RATE_LIMIT",
                "Roastty five-second check",
            ),
            ("self.last_notification_time = Some(now)", "Roastty time state update"),
            ("self.last_notification_identity.extend_from_slice(identity)", "Roastty identity state update"),
            ("fn perform_desktop_notification_at", "Roastty explicit-time dispatch helper"),
            ("if !self.desktop_notifications", "Roastty config-disabled gate before limiter"),
            ("nul_terminated_truncated(&notification.title", "Roastty title truncation before identity"),
            ("nul_terminated_truncated(&notification.body", "Roastty body truncation before identity"),
            ("identity.extend_from_slice(&title[..title.len().saturating_sub(1)])", "Roastty title identity input"),
            ("identity.extend_from_slice(&body[..body.len().saturating_sub(1)])", "Roastty body identity input"),
            ("app.allow_desktop_notification_at(now, &identity)", "Roastty app-level limiter call"),
            ("ROASTTY_ACTION_DESKTOP_NOTIFICATION", "Roastty notification action dispatch"),
        ],
    )

    require_all(
        roastty_lib,
        [
            (
                "surface_desktop_notification_runtime_suppresses_config_disabled_action",
                "Roastty config-disabled test",
            ),
            ("last_notification_time.is_none()", "Roastty config-disabled no state update"),
            (
                "surface_desktop_notification_runtime_rate_limits_without_sleeping",
                "Roastty deterministic rate-limit test",
            ),
            ("Duration::from_millis(999)", "Roastty one-second suppressed offset"),
            ("Duration::from_millis(1001)", "Roastty one-second allowed offset"),
            ("Duration::from_secs(4)", "Roastty identical suppressed offset"),
            ("Duration::from_millis(5001)", "Roastty identical allowed offset"),
            ("b\"abc\".to_vec()", "Roastty delimiterless identity assertion"),
            (
                "surface_desktop_notification_runtime_uses_delimiterless_identity",
                "Roastty delimiterless identity test",
            ),
            (
                "surface_desktop_notification_runtime_rate_limit_is_app_level",
                "Roastty cross-surface app-level test",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-012B2B2B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-012B2B2B1 status"),
            ("desktop notification rate limiting", "RUNTIME-012B2B2B1 behavior"),
            ("last_notification_time", "RUNTIME-012B2B2B1 Ghostty time evidence"),
            ("last_notification_digest", "RUNTIME-012B2B2B1 Ghostty digest evidence"),
            ("one-second", "RUNTIME-012B2B2B1 one-second evidence"),
            ("five-second", "RUNTIME-012B2B2B1 five-second evidence"),
            ("delimiterless", "RUNTIME-012B2B2B1 delimiterless identity evidence"),
            ("desktop_notification_rate_limit_runtime_parity.py", "RUNTIME-012B2B2B1 guard evidence"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-012B2B2B2B2B3")
    require_all(
        row_gap,
        [
            ("Gap", "RUNTIME-012B2B2B2B2B3 status"),
            ("Actual OS banner/sound delivery", "remaining OS delivery gap"),
            ("actual audio/dock/border/title GUI effects", "remaining bell GUI gap"),
            ("hover/cursor UI", "remaining hover cursor gap"),
            ("native link preview display", "remaining link preview gap"),
            ("native context/menu display", "remaining context menu gap"),
        ],
    )
    if "RUNTIME-012B2B2B |" in runtime_inventory:
        raise AssertionError("old broad RUNTIME-012B2B2B row is still present")

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("73 rows Oracle complete", "CFG-223 oracle count"),
            ("76 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("desktop_notification_rate_limit_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
