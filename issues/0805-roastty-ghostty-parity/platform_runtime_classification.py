#!/usr/bin/env python3
"""Classify platform-prefixed config runtime effects for Issue 805."""

from __future__ import annotations

import argparse
import dataclasses
from collections import Counter
from pathlib import Path


VALID_STATUSES = {
    "Oracle complete",
    "Gap",
    "Intentional divergence",
    "Not applicable",
}


@dataclasses.dataclass(frozen=True)
class PlatformRow:
    option: str
    platform: str
    applicability: str
    status: str
    owner: str
    evidence: str
    guard_tier: str
    guard: str


def gtk_row(option: str, family: str) -> PlatformRow:
    return PlatformRow(
        option=option,
        platform="GTK",
        applicability="Not applicable to Roastty's macOS app/runtime.",
        status="Not applicable",
        owner="RUNTIME-013",
        evidence=(
            f"`{option}` configures Ghostty's GTK {family}. Roastty's shipped app "
            "surface for Issue 805 is the copied macOS Swift app, so there is no "
            "GTK runtime surface for this option to affect."
        ),
        guard_tier="Tier 0",
        guard="Regenerate this manifest from `config-inventory.md`.",
    )


def linux_row(option: str, family: str) -> PlatformRow:
    return PlatformRow(
        option=option,
        platform="Linux",
        applicability="Not applicable to Roastty's macOS app/runtime.",
        status="Not applicable",
        owner="RUNTIME-013",
        evidence=(
            f"`{option}` configures Ghostty's Linux {family}. Roastty's Issue 805 "
            "runtime target is macOS and has no Linux cgroup management layer."
        ),
        guard_tier="Tier 0",
        guard="Regenerate this manifest from `config-inventory.md`.",
    )


def macos_gap(option: str, family: str) -> PlatformRow:
    return PlatformRow(
        option=option,
        platform="macOS",
        applicability="Applicable to Roastty's copied macOS app.",
        status="Gap",
        owner="RUNTIME-011",
        evidence=(
            f"`{option}` affects macOS {family}. Parser/default/formatter coverage "
            "exists, but real macOS app/runtime behavior remains owned by the "
            "`RUNTIME-011` walkthrough row."
        ),
        guard_tier="Tier 3",
        guard="TBD by future CFG-223 macOS app walkthrough experiment.",
    )


ROWS: dict[str, PlatformRow] = {
    "gtk-custom-css": gtk_row("gtk-custom-css", "custom CSS loading"),
    "gtk-opengl-debug": gtk_row("gtk-opengl-debug", "OpenGL debug behavior"),
    "gtk-quick-terminal-layer": gtk_row("gtk-quick-terminal-layer", "quick terminal layer"),
    "gtk-quick-terminal-namespace": gtk_row(
        "gtk-quick-terminal-namespace", "quick terminal namespace"
    ),
    "gtk-single-instance": gtk_row("gtk-single-instance", "single-instance behavior"),
    "gtk-tabs-location": gtk_row("gtk-tabs-location", "tabs UI"),
    "gtk-titlebar": gtk_row("gtk-titlebar", "titlebar UI"),
    "gtk-titlebar-hide-when-maximized": gtk_row(
        "gtk-titlebar-hide-when-maximized", "titlebar UI"
    ),
    "gtk-titlebar-style": gtk_row("gtk-titlebar-style", "titlebar style"),
    "gtk-toolbar-style": gtk_row("gtk-toolbar-style", "toolbar style"),
    "gtk-wide-tabs": gtk_row("gtk-wide-tabs", "tabs UI"),
    "linux-cgroup": linux_row("linux-cgroup", "cgroup creation"),
    "linux-cgroup-hard-fail": linux_row("linux-cgroup-hard-fail", "cgroup error handling"),
    "linux-cgroup-memory-limit": linux_row("linux-cgroup-memory-limit", "cgroup memory limit"),
    "linux-cgroup-processes-limit": linux_row(
        "linux-cgroup-processes-limit", "cgroup process limit"
    ),
    "macos-applescript": macos_gap("macos-applescript", "AppleScript automation"),
    "macos-auto-secure-input": macos_gap("macos-auto-secure-input", "secure input"),
    "macos-custom-icon": macos_gap("macos-custom-icon", "app icon selection"),
    "macos-dock-drop-behavior": macos_gap("macos-dock-drop-behavior", "Dock drop handling"),
    "macos-hidden": macos_gap("macos-hidden", "app launch visibility"),
    "macos-icon": macos_gap("macos-icon", "app icon selection"),
    "macos-icon-frame": macos_gap("macos-icon-frame", "app icon rendering"),
    "macos-icon-ghost-color": macos_gap("macos-icon-ghost-color", "app icon rendering"),
    "macos-icon-screen-color": macos_gap("macos-icon-screen-color", "app icon rendering"),
    "macos-non-native-fullscreen": macos_gap("macos-non-native-fullscreen", "fullscreen UI"),
    "macos-option-as-alt": PlatformRow(
        option="macos-option-as-alt",
        platform="macOS",
        applicability="Applicable to Roastty key translation.",
        status="Oracle complete",
        owner="RUNTIME-005",
        evidence=(
            "`key_translation_mods_*` tests prove configured `macos-option-as-alt` "
            "drives macOS option-key translation and survives keyboard-change "
            "events."
        ),
        guard_tier="Tier 1",
        guard="`cargo test --manifest-path roastty/Cargo.toml key_translation_mods`",
    ),
    "macos-secure-input-indication": macos_gap(
        "macos-secure-input-indication", "secure input UI"
    ),
    "macos-shortcuts": macos_gap("macos-shortcuts", "native shortcut handling"),
    "macos-titlebar-proxy-icon": macos_gap(
        "macos-titlebar-proxy-icon", "titlebar proxy icon UI"
    ),
    "macos-titlebar-style": macos_gap("macos-titlebar-style", "titlebar UI"),
    "macos-window-buttons": macos_gap("macos-window-buttons", "window chrome"),
    "macos-window-shadow": macos_gap("macos-window-shadow", "window chrome"),
}


def extract_platform_options(config_inventory: Path) -> list[str]:
    options: list[str] = []
    in_canonical = False
    for line in config_inventory.read_text().splitlines():
        if line == "## Ghostty Canonical Options":
            in_canonical = True
            continue
        if in_canonical and line.startswith("## "):
            break
        if not in_canonical or not line.startswith("- `"):
            continue
        option = line.split("`", 2)[1]
        if option.startswith(("gtk-", "linux-", "macos-")):
            options.append(option)
    return sorted(set(options))


def validate_rows(options: list[str]) -> list[PlatformRow]:
    option_set = set(options)
    row_set = set(ROWS)
    missing = sorted(option_set - row_set)
    stale = sorted(row_set - option_set)
    if missing:
        raise ValueError(f"unclassified platform options: {missing}")
    if stale:
        raise ValueError(f"classification rows for non-canonical options: {stale}")

    rows = [ROWS[option] for option in options]
    invalid_statuses = sorted({row.status for row in rows} - VALID_STATUSES)
    if invalid_statuses:
        raise ValueError(f"invalid platform classification statuses: {invalid_statuses}")
    for row in rows:
        if row.status == "Gap" and not row.guard.startswith("TBD"):
            raise ValueError(f"gap row has non-TBD guard: {row.option}")
        if not row.owner or not row.evidence or not row.guard:
            raise ValueError(f"incomplete classification row: {row.option}")
    return rows


def emit(rows: list[PlatformRow], output: Path) -> None:
    status_counts = Counter(row.status for row in rows)
    platform_counts = Counter(row.platform for row in rows)
    lines = [
        "# Platform Runtime Classification",
        "",
        "Generated by `issues/0805-roastty-ghostty-parity/platform_runtime_classification.py`",
        "for Issue 805 `RUNTIME-013`.",
        "",
        "## Counts",
        "",
        "| Category | Count |",
        "| --- | ---: |",
        f"| Platform-prefixed options | {len(rows)} |",
    ]
    for status in sorted(status_counts):
        lines.append(f"| {status} rows | {status_counts[status]} |")
    for platform in sorted(platform_counts):
        lines.append(f"| {platform} rows | {platform_counts[platform]} |")

    lines.extend(
        [
            "",
            "## Rows",
            "",
            "| Option | Platform | Applicability | Status | Owner | Evidence | Guard tier | Guard |",
            "| --- | --- | --- | --- | --- | --- | --- | --- |",
        ]
    )
    for row in rows:
        lines.append(
            f"| `{row.option}` | {row.platform} | {row.applicability} | "
            f"{row.status} | {row.owner} | {row.evidence} | "
            f"{row.guard_tier} | {row.guard} |"
        )
    output.write_text("\n".join(lines) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--config-inventory", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    options = extract_platform_options(args.config_inventory)
    rows = validate_rows(options)
    emit(rows, args.output)

    status_counts = Counter(row.status for row in rows)
    print(f"platform_options={len(rows)}")
    for status in sorted(status_counts):
        key = status.lower().replace(" ", "_")
        print(f"{key}={status_counts[status]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
