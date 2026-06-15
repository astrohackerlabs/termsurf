#!/usr/bin/env python3
"""Guard copied macOS user-notification parity for Issue 805 CFG-223."""

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


def assert_sources_match_after_rename(
    ghostty_path: str,
    roastty_path: str,
    label: str,
) -> None:
    assert_equal(
        normalize_ghostty_to_roastty(read(ghostty_path)),
        read(roastty_path),
        label,
    )


def line_start_for(source: str, needle: str) -> int:
    index = source.find(needle)
    if index == -1:
        raise AssertionError(f"missing block start: {needle!r}")
    return source.rfind("\n", 0, index) + 1


def previous_declaration_start(source: str, needle: str) -> int:
    index = source.find(needle)
    if index == -1:
        raise AssertionError(f"missing declaration marker: {needle!r}")
    prefix = source[:index]
    matches = list(re.finditer(r"(?m)^\s*(?:@\w+\s+)*(?:\w+\s+)*func\s+", prefix))
    if not matches:
        raise AssertionError(f"missing function declaration before {needle!r}")
    return matches[-1].start()


def previous_keyword_start(source: str, needle: str, keyword: str) -> int:
    index = source.find(needle)
    if index == -1:
        raise AssertionError(f"missing declaration marker: {needle!r}")
    prefix = source[:index]
    matches = list(re.finditer(rf"(?m)^(?:\s*){re.escape(keyword)}\b", prefix))
    if not matches:
        raise AssertionError(f"missing {keyword} declaration before {needle!r}")
    return matches[-1].start()


def balanced_block(source: str, start: int) -> str:
    brace = source.find("{", start)
    if brace == -1:
        raise AssertionError(f"missing opening brace after offset {start}")
    depth = 0
    in_string = False
    escaped = False
    for index in range(brace, len(source)):
        char = source[index]
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            continue
        if char == '"':
            in_string = True
            continue
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                end = index + 1
                while end < len(source) and source[end] in " \t":
                    end += 1
                if end < len(source) and source[end] == "\n":
                    end += 1
                return source[start:end]
    raise AssertionError(f"unbalanced Swift block after offset {start}")


def declaration_line(source: str, marker: str) -> str:
    start = line_start_for(source, marker)
    end = source.find("\n", start)
    if end == -1:
        end = len(source)
    return source[start:end]


def notification_lifecycle_slice(source: str) -> str:
    parts = [
        declaration_line(source, "var notificationIdentifiers: Set<String> = []"),
        balanced_block(source, previous_keyword_start(source, "removeDeliveredNotifications(withIdentifiers: identifiers)", "deinit")),
        balanced_block(source, previous_declaration_start(source, "self.notificationIdentifiers = []")),
        balanced_block(source, line_start_for(source, "func showUserNotification(")),
        balanced_block(source, line_start_for(source, "func handleUserNotification(")),
    ]
    return "\n".join(part.strip() for part in parts)


def main() -> int:
    assert_sources_match_after_rename(
        "vendor/ghostty/macos/Sources/App/macOS/AppDelegate.swift",
        "roastty/macos/Sources/App/macOS/AppDelegate.swift",
        "AppDelegate.swift",
    )
    assert_sources_match_after_rename(
        "vendor/ghostty/macos/Sources/Ghostty/Ghostty.App.swift",
        "roastty/macos/Sources/Roastty/Roastty.App.swift",
        "Roastty.App.swift",
    )
    assert_sources_match_after_rename(
        "vendor/ghostty/macos/Sources/Ghostty/GhosttyPackage.swift",
        "roastty/macos/Sources/Roastty/RoasttyPackage.swift",
        "RoasttyPackage.swift",
    )

    ghostty_surface_appkit = read(
        "vendor/ghostty/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift"
    )
    roastty_surface_appkit = read(
        "roastty/macos/Sources/Roastty/Surface View/SurfaceView_AppKit.swift"
    )
    assert_equal(
        normalize_ghostty_to_roastty(notification_lifecycle_slice(ghostty_surface_appkit)),
        notification_lifecycle_slice(roastty_surface_appkit),
        "SurfaceView_AppKit.swift notification lifecycle",
    )

    roastty_surface_slice = notification_lifecycle_slice(roastty_surface_appkit)
    require_all(
        roastty_surface_slice,
        [
            ("var notificationIdentifiers: Set<String> = []", "identifier tracking field"),
            ("removeDeliveredNotifications(withIdentifiers: identifiers)", "deinit cleanup"),
            ("self.notificationIdentifiers = []", "focus cleanup"),
            ("UNMutableNotificationContent()", "notification content"),
            ("content.title = title", "notification title"),
            ("content.subtitle = self.title", "notification subtitle"),
            ("content.body = body", "notification body"),
            ("content.sound = UNNotificationSound.default", "default notification sound"),
            ("content.categoryIdentifier = Roastty.userNotificationCategory", "notification category"),
            ('"surface": self.id.uuidString', "surface UUID userInfo"),
            ('"requireFocus": requireFocus', "requireFocus userInfo"),
            ("UNNotificationRequest(", "notification request"),
            ("identifier: uuid", "notification request identifier"),
            ("UNUserNotificationCenter.current().add(request)", "notification delivery"),
            ("notificationIdentifiers.insert(uuid)", "post-delivery identifier tracking"),
            ("if focused", "focused-surface delayed cleanup gate"),
            ("Task.sleep(for: .seconds(3))", "focused-surface delayed cleanup"),
            ("window?.makeKeyAndOrderFront(self)", "click brings window forward"),
            ("Roastty.moveFocus(to: self)", "click focuses surface"),
        ],
    )

    roastty_app = read("roastty/macos/Sources/Roastty/Roastty.App.swift")
    require_all(
        roastty_app,
        [
            ("func shouldPresentNotification(notification: UNNotification) -> Bool", "foreground presentation gate"),
            ('guard let uuidString = userInfo["surface"] as? String', "surface UUID lookup"),
            ('let requireFocus = userInfo["requireFocus"] as? Bool ?? true', "requireFocus lookup"),
            ("return !window.isKeyWindow || !surface.focused", "foreground focus/window suppression"),
            ("ROASTTY_ACTION_DESKTOP_NOTIFICATION", "desktop notification action dispatch"),
            ("showDesktopNotification(app, target: target, n: action.action.desktop_notification)", "action payload forwarding"),
            ("requestAuthorization(options: [.alert, .sound])", "authorization request"),
            ("settings.authorizationStatus == .authorized", "authorized settings gate"),
            ("surfaceView.showUserNotification(", "surface delivery call"),
            ("func handleUserNotification(response: UNNotificationResponse)", "response routing"),
            ("surface.handleUserNotification(notification: response.notification, focus: true)", "show action focus"),
            ("surface.handleUserNotification(notification: response.notification, focus: false)", "dismiss action no-focus"),
        ],
    )

    roastty_app_delegate = read("roastty/macos/Sources/App/macOS/AppDelegate.swift")
    require_all(
        roastty_app_delegate,
        [
            ("UNNotificationAction(identifier: Roastty.userNotificationActionShow", "show action registration"),
            ("UNNotificationCategory(", "category registration"),
            ("identifier: Roastty.userNotificationCategory", "category identifier registration"),
            ("center.delegate = self", "notification delegate install"),
            ("removeAllDeliveredNotifications()", "termination cleanup"),
            ("roastty.handleUserNotification(response: didReceive)", "response callback"),
            ("roastty.shouldPresentNotification(notification: willPresent)", "foreground callback"),
            ("let options: UNNotificationPresentationOptions = shouldPresent ? [.banner, .sound] : []", "foreground presentation options"),
            ("withCompletionHandler(options)", "foreground completion callback"),
        ],
    )

    roastty_package = read("roastty/macos/Sources/Roastty/RoasttyPackage.swift")
    require_all(
        roastty_package,
        [
            ('userNotificationCategory = "com.mitchellh.roastty.userNotification"', "category identifier"),
            ('userNotificationActionShow = "com.mitchellh.roastty.userNotification.Show"', "action identifier"),
        ],
    )

    ghostty_surface = read("vendor/ghostty/src/Surface.zig")
    require_all(
        ghostty_surface,
        [
            ("last_notification_time", "Ghostty notification rate limit time state"),
            ("last_notification_digest", "Ghostty notification rate limit digest state"),
            ("const hash_algorithm = std.hash.Wyhash", "Ghostty notification digest algorithm"),
            ("hash.update(title)", "Ghostty notification digest title"),
            ("hash.update(body)", "Ghostty notification digest body"),
            ("now.since(last) < 1 * std.time.ns_per_s", "Ghostty one-second notification throttle"),
            ("now.since(last) < 5 * std.time.ns_per_s", "Ghostty duplicate notification throttle"),
        ],
    )

    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    row_complete = require_row(runtime_inventory, "RUNTIME-012B2B2A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-012B2B2A status"),
            ("macOS user-notification presentation", "RUNTIME-012B2B2A behavior"),
            ("AppDelegate.swift", "app delegate evidence"),
            ("Roastty.App.swift", "app evidence"),
            ("RoasttyPackage.swift", "package evidence"),
            ("SurfaceView_AppKit.swift", "surface evidence"),
            ("identifier tracking", "identifier evidence"),
            ("requireFocus", "requireFocus evidence"),
            ("click-to-focus", "click focus evidence"),
            ("macos_user_notification_runtime_parity.py", "static guard evidence"),
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
    if "RUNTIME-012B2B2 |" in runtime_inventory:
        raise AssertionError("old broad RUNTIME-012B2B2 row is still present")

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

    print("macos_user_notification_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
