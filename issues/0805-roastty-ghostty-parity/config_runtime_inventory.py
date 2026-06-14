#!/usr/bin/env python3
"""Inventory Roastty config runtime/UI effect coverage for Issue 805.

This is a bounded markdown/source inventory for CFG-223. It records config
effects that must be proven in the running app/terminal/renderer surface and
keeps broad runtime/UI parity honest by requiring row-level evidence.
"""

from __future__ import annotations

import argparse
import dataclasses
from collections import Counter
from pathlib import Path


@dataclasses.dataclass(frozen=True)
class RuntimeRow:
    id: str
    behavior: str
    ghostty_reference: str
    roastty_reference: str
    family: str
    status: str
    evidence: str
    missing_evidence: str
    guard_tier: str
    guard_command: str


ROWS = [
    RuntimeRow(
        id="RUNTIME-001",
        behavior="app-level clipboard read/write policy effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` clipboard access fields; `vendor/ghostty/src/Surface.zig` clipboard actions",
        roastty_reference="`roastty/src/lib.rs` clipboard callbacks and app config fields",
        family="clipboard",
        status="Oracle complete",
        evidence=(
            "Clipboard callback tests cover read/write allow/deny/ask policy "
            "dispatch through the runtime callbacks, and app/surface update "
            "tests prove clipboard policies refresh existing surfaces."
        ),
        missing_evidence="None for clipboard read/write policy runtime dispatch.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml clipboard_read && cargo test --manifest-path roastty/Cargo.toml clipboard_write`",
    ),
    RuntimeRow(
        id="RUNTIME-002",
        behavior="clipboard copy/paste transformation effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` clipboard transform fields; `vendor/ghostty/src/Surface.zig` copy/paste paths",
        roastty_reference="`roastty/src/lib.rs::copy_to_clipboard`, `clipboard_paste_is_unsafe`, paste helpers",
        family="clipboard",
        status="Oracle complete",
        evidence=(
            "Clipboard and paste tests cover bracketed paste safety, paste "
            "protection, codepoint-map copy transformation, trimming trailing "
            "spaces, and selection-clear-on-copy behavior."
        ),
        missing_evidence="None for covered clipboard transform runtime behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml clipboard && cargo test --manifest-path roastty/Cargo.toml paste`",
    ),
    RuntimeRow(
        id="RUNTIME-003",
        behavior="selection behavior effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` selection fields; `vendor/ghostty/src/Surface.zig` selection and copy paths",
        roastty_reference="`roastty/src/lib.rs` selection gesture, selection read, and selection clear paths",
        family="selection",
        status="Oracle complete",
        evidence=(
            "`app_and_surface_update_config_sync_selection_behavior` plus "
            "selection gesture/read tests prove selection clear-on-typing, "
            "selection word character boundaries, copy-on-select-adjacent "
            "selection state, and selection runtime update behavior."
        ),
        missing_evidence="None for covered selection config runtime behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml selection`",
    ),
    RuntimeRow(
        id="RUNTIME-004",
        behavior="mouse reporting, click, cursor, and scroll effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` mouse/click fields; `vendor/ghostty/src/Surface.zig` mouse handlers",
        roastty_reference="`roastty/src/lib.rs` mouse handlers, selection gestures, and mouse encoder",
        family="mouse",
        status="Gap",
        evidence=(
            "Roastty has mouse reporting, scroll, selection gesture, and mouse "
            "encoder tests, but CFG-223 still needs one explicit row tying all "
            "config-driven click/cursor actions to pinned Ghostty behavior: "
            "`cursor-click-to-move`, `mouse-hide-while-typing`, "
            "`right-click-action`, and `middle-click-action` are not yet "
            "covered by this generated runtime inventory."
        ),
        missing_evidence=(
            "Add a focused runtime oracle for config-driven mouse/click/cursor "
            "effects, or split this row into smaller Oracle-complete rows."
        ),
        guard_tier="Tier 2",
        guard_command="TBD by future CFG-223 mouse runtime experiment.",
    ),
    RuntimeRow(
        id="RUNTIME-005",
        behavior="keyboard remap and keybind dispatch effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` key-remap/keybind fields; `vendor/ghostty/src/Surface.zig` key dispatch",
        roastty_reference="`roastty/src/lib.rs` key remap, keybind, and key table helpers",
        family="input",
        status="Oracle complete",
        evidence=(
            "Keybinding tests cover focused/global/all scope dispatch, key "
            "tables, key sequences, catch-all and unconsumed behavior; "
            "`surface_key_remap_*` tests prove remap affects binding detection, "
            "encoded input, and app/surface config updates."
        ),
        missing_evidence=(
            "None for key remap/keybind dispatch runtime behavior; "
            "command-palette UI dispatch remains tracked by `RUNTIME-011`."
        ),
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml surface_key_remap && cargo test --manifest-path roastty/Cargo.toml surface_key_table`",
    ),
    RuntimeRow(
        id="RUNTIME-006",
        behavior="color, palette, theme, and color-scheme runtime effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` colors/palette/theme fields; `vendor/ghostty/src/Surface.zig` config change rendering paths",
        roastty_reference="`roastty/src/lib.rs::derived_config_palette`, color scheme reload tests, renderer state",
        family="color",
        status="Oracle complete",
        evidence=(
            "`surface_apply_config_updates_palette_defaults`, "
            "`surface_apply_config_updates_generated_palette_defaults`, and "
            "color-scheme reload tests prove palette/generated-palette defaults "
            "and conditional theme/color-scheme runtime updates."
        ),
        missing_evidence="None for covered color/palette/theme runtime behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml surface_apply_config_updates_palette && cargo test --manifest-path roastty/Cargo.toml color_scheme`",
    ),
    RuntimeRow(
        id="RUNTIME-007",
        behavior="font selection, shaping, fallback, metrics, and font-size runtime effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` font fields; `vendor/ghostty/src/Surface.zig` font grid/runtime update paths",
        roastty_reference="`roastty/src/lib.rs` font-size runtime state; `roastty/src/font`",
        family="font",
        status="Gap",
        evidence=(
            "Experiment 105 proves reload font-size behavior, but CFG-223 still "
            "needs runtime proof for the broader font surface: configured font "
            "families, fallback, shaping, glyph metrics, feature/variation "
            "effects, and renderer-visible font changes."
        ),
        missing_evidence="Add focused font runtime/renderer oracles beyond parser/formatter/default coverage.",
        guard_tier="Tier 2",
        guard_command="TBD by future CFG-223 font runtime experiment.",
    ),
    RuntimeRow(
        id="RUNTIME-008",
        behavior="renderer presentation effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` renderer/window visual fields; `vendor/ghostty/src/Surface.zig` renderer config messages",
        roastty_reference="`roastty/src/lib.rs` live renderer, render state, present driver, and config update paths",
        family="renderer",
        status="Gap",
        evidence=(
            "Roastty has live renderer config-update and cursor blink tests, but "
            "CFG-223 still needs representative proof for config-driven "
            "opacity, blur, padding, cursor style, window padding color, vsync, "
            "custom shader, and other renderer-visible effects."
        ),
        missing_evidence="Add renderer/runtime or GUI smoke rows for visible config effects.",
        guard_tier="Tier 3",
        guard_command="TBD by future CFG-223 renderer runtime experiment.",
    ),
    RuntimeRow(
        id="RUNTIME-009",
        behavior="terminal behavior toggle effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` terminal behavior fields; `vendor/ghostty/src/termio` and `Surface.zig`",
        roastty_reference="`roastty/src/lib.rs` terminal/termio config use; `roastty/src/terminal`",
        family="terminal",
        status="Gap",
        evidence=(
            "`vt_kam_allowed_*` tests prove config-driven terminal key gating "
            "and live update behavior, but this row also covers scrollback, "
            "alternate screen, shell integration, terminfo, and title reporting, "
            "which do not yet have a generated CFG-223 runtime oracle."
        ),
        missing_evidence="Split VT KAM into a narrower pass row or add runtime oracles for the full terminal toggle scope.",
        guard_tier="Tier 2",
        guard_command="TBD by future CFG-223 terminal runtime experiment.",
    ),
    RuntimeRow(
        id="RUNTIME-010",
        behavior="PTY/process launch effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` command, working-directory, env, wait-after-command, and quit policy fields",
        roastty_reference="`roastty/src/lib.rs::start_termio`, inherited config, surface config launch fields",
        family="process",
        status="Gap",
        evidence=(
            "`first_surface_uses_app_initial_command`, "
            "`later_surface_after_close_ignores_app_initial_command`, and "
            "`surface_inherited_config_*` tests prove initial command and "
            "working-directory inheritance behavior, but this row also covers "
            "environment, wait-after-command, abnormal-command-exit-runtime, and "
            "quit policy effects, which do not yet have generated runtime proof."
        ),
        missing_evidence="Split proven command/working-directory behavior into narrower rows or add runtime proof for the full PTY/process scope.",
        guard_tier="Tier 2",
        guard_command="TBD by future CFG-223 PTY/process runtime experiment.",
    ),
    RuntimeRow(
        id="RUNTIME-011",
        behavior="macOS app/window/tab/split/menu and command palette UI effects",
        ghostty_reference="`vendor/ghostty/macos/Sources`; app/window/tab/split config-driven UI behavior",
        roastty_reference="`roastty/macos/Sources`; Roastty app wrapper and Swift UI",
        family="macOS app",
        status="Gap",
        evidence=(
            "Feature and walkthrough matrices only prove launch/cleanup and "
            "keyboard delivery. CFG-223 still needs real app walkthrough or "
            "focused macOS tests for config-driven windows, tabs, splits, "
            "menus, titlebar, fullscreen, quick terminal, and command palette UI."
        ),
        missing_evidence="Add focused macOS app walkthrough rows and GUI guards.",
        guard_tier="Tier 3",
        guard_command="TBD by future CFG-223 macOS app walkthrough experiment.",
    ),
    RuntimeRow(
        id="RUNTIME-012",
        behavior="notifications, bell, command-finish, and URL/link opening effects",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig` notification/bell/link fields; app runtime actions",
        roastty_reference="`roastty/src/lib.rs` bell/link/open-url actions; `roastty/macos/Sources` notification handling",
        family="notifications",
        status="Gap",
        evidence=(
            "Parser/formatter/default coverage exists for notification, bell, "
            "and link options, but CFG-223 still needs runtime proof for bell "
            "actions, command-finish notifications, app-notifications, URL "
            "opening, hover/cursor behavior, and context/menu link flows."
        ),
        missing_evidence="Add notification/bell/link runtime or GUI walkthrough guards.",
        guard_tier="Tier 3",
        guard_command="TBD by future CFG-223 notification/link runtime experiment.",
    ),
    RuntimeRow(
        id="RUNTIME-013",
        behavior="platform-specific or unsupported runtime effects",
        ghostty_reference="Pinned Ghostty GTK/Linux/platform-specific config runtime behavior",
        roastty_reference="Roastty macOS app and libroastty runtime",
        family="platform",
        status="Gap",
        evidence=(
            "Config parser/formatter rows list platform-specific settings, but "
            "CFG-223 still needs a runtime classification pass marking GTK/Linux "
            "effects not applicable to Roastty or proving macOS equivalents."
        ),
        missing_evidence="Classify each platform-specific runtime effect as Not applicable, divergence, or pass.",
        guard_tier="Tier 0",
        guard_command="TBD by future CFG-223 platform classification experiment.",
    ),
    RuntimeRow(
        id="RUNTIME-014",
        behavior="accepted runtime divergences cross-link",
        ghostty_reference="Pinned Ghostty runtime helpers and public ABI behavior",
        roastty_reference="`issues/0805-roastty-ghostty-parity/divergences.md`",
        family="divergence",
        status="Intentional divergence",
        evidence=(
            "`DIV-001` records `roastty_translate` identity behavior and "
            "`DIV-002` records unsupported benchmark CLI behavior, with ABI "
            "guards. These are accepted non-parity runtime outcomes."
        ),
        missing_evidence="None for currently accepted runtime divergences.",
        guard_tier="Tier 0",
        guard_command="Inspect `issues/0805-roastty-ghostty-parity/divergences.md` and run the ABI harness listed there.",
    ),
]

EXPECTED_IDS = [f"RUNTIME-{index:03d}" for index in range(1, 15)]


def validate_rows(rows: list[RuntimeRow]) -> None:
    ids = [row.id for row in rows]
    if ids != EXPECTED_IDS:
        raise ValueError(f"runtime row manifest mismatch: {ids!r}")

    duplicate_ids = [item for item, count in Counter(ids).items() if count > 1]
    if duplicate_ids:
        raise ValueError(f"duplicate runtime row IDs: {duplicate_ids}")

    behaviors = [row.behavior for row in rows]
    duplicate_behaviors = [
        item for item, count in Counter(behaviors).items() if count > 1
    ]
    if duplicate_behaviors:
        raise ValueError(f"duplicate runtime behavior names: {duplicate_behaviors}")

    valid_statuses = {
        "Oracle complete",
        "Audit covered",
        "Gap",
        "Intentional divergence",
        "Not applicable",
    }
    invalid_statuses = sorted({row.status for row in rows} - valid_statuses)
    if invalid_statuses:
        raise ValueError(f"invalid runtime statuses: {invalid_statuses}")

    for row in rows:
        if not row.guard_tier or not row.guard_command:
            raise ValueError(f"missing guard field for {row.id}")
        if not row.ghostty_reference or not row.roastty_reference:
            raise ValueError(f"missing evidence anchor for {row.id}")
        if row.status == "Gap" and not row.guard_command.startswith("TBD"):
            raise ValueError(f"gap row has non-TBD guard: {row.id}")


def emit_inventory(rows: list[RuntimeRow], output: Path) -> None:
    status_counts = Counter(row.status for row in rows)
    family_counts = Counter(row.family for row in rows)

    lines = [
        "# Config Runtime/UI Effects Inventory",
        "",
        "Generated by `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`",
        "for Issue 805 CFG-223 runtime/UI effect experiments.",
        "",
        "## Counts",
        "",
        "| Category | Count |",
        "| --- | ---: |",
        f"| Runtime rows | {len(rows)} |",
        f"| Oracle complete rows | {status_counts.get('Oracle complete', 0)} |",
        f"| Intentional divergence rows | {status_counts.get('Intentional divergence', 0)} |",
        f"| Not applicable rows | {status_counts.get('Not applicable', 0)} |",
        f"| Audit covered rows | {status_counts.get('Audit covered', 0)} |",
        f"| Gap rows | {status_counts.get('Gap', 0)} |",
        "",
        "## Runtime Families",
        "",
        "| Runtime family | Count |",
        "| --- | ---: |",
    ]
    for family, count in sorted(family_counts.items()):
        lines.append(f"| {family} | {count} |")

    lines.extend(["", "## Expected Row Manifest", ""])
    lines.extend(f"- `{row_id}`" for row_id in EXPECTED_IDS)

    lines.extend(
        [
            "",
            "## Rows",
            "",
            "| ID | Behavior | Ghostty reference | Roastty reference | Family | Status | Evidence | Missing evidence | Guard tier | Guard command |",
            "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
        ]
    )
    for row in rows:
        lines.append(
            f"| {row.id} | {row.behavior} | {row.ghostty_reference} | "
            f"{row.roastty_reference} | {row.family} | {row.status} | "
            f"{row.evidence} | {row.missing_evidence} | {row.guard_tier} | "
            f"{row.guard_command} |"
        )
    output.write_text("\n".join(lines) + "\n")


def update_cfg223(
    matrix: Path,
    runtime_inventory_path: Path,
    closed_count: int,
    oracle_count: int,
    incomplete_count: int,
    gap_count: int,
) -> None:
    lines = matrix.read_text().splitlines()
    updated: list[str] = []
    found = False
    for line in lines:
        if line.startswith("| CFG-223 |"):
            found = True
            status = "Pass" if incomplete_count == 0 and gap_count == 0 else "Gap"
            notes = (
                f"Runtime inventory coverage: {oracle_count} rows Oracle complete; "
                f"{closed_count} rows closed; {incomplete_count} rows are "
                f"incomplete and {gap_count} rows are runtime gaps."
            )
            line = (
                "| CFG-223 | Runtime and UI effects | "
                "Ghostty config options that affect app, renderer, input, font, "
                "terminal, and platform behavior produce equivalent runtime effects. | "
                "Roastty runtime/UI effects are inventoried by pinned Ghostty "
                "config-driven runtime domains. | "
                f"{status} | Generated runtime/UI inventory plus matrix consistency "
                "assertion. | "
                f"`{runtime_inventory_path}` | Tier 3 | "
                "`PYTHONDONTWRITEBYTECODE=1 python3 "
                "issues/0805-roastty-ghostty-parity/config_runtime_inventory.py "
                "--output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md "
                "--matrix issues/0805-roastty-ghostty-parity/config-matrix.md` | "
                "Before closing Issue 805 and when config-driven runtime behavior changes. | "
                "CFG-223 only passes when every runtime/UI inventory row is "
                "`Oracle complete`, `Not applicable`, or an accepted documented "
                f"divergence; audit coverage alone is insufficient. | Experiment 106 | {notes} |"
            )
        updated.append(line)

    if not found:
        raise ValueError("CFG-223 row not found in config matrix")

    matrix.write_text("\n".join(updated) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--matrix", type=Path, required=True)
    args = parser.parse_args()

    rows = list(ROWS)
    validate_rows(rows)
    emit_inventory(rows, args.output)

    complete_statuses = {"Oracle complete", "Intentional divergence", "Not applicable"}
    oracle_count = sum(row.status == "Oracle complete" for row in rows)
    closed_count = sum(row.status in complete_statuses for row in rows)
    incomplete_count = sum(row.status not in complete_statuses for row in rows)
    gap_count = sum(row.status == "Gap" for row in rows)
    audit_count = sum(row.status == "Audit covered" for row in rows)
    update_cfg223(
        args.matrix,
        args.output,
        closed_count,
        oracle_count,
        incomplete_count,
        gap_count,
    )

    print(f"runtime_rows={len(rows)}")
    print(f"oracle_complete={oracle_count}")
    print(f"closed={closed_count}")
    print(f"audit_covered={audit_count}")
    print(f"incomplete={incomplete_count}")
    print(f"gap={gap_count}")
    print(f"cfg223={'Pass' if incomplete_count == 0 and gap_count == 0 else 'Gap'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
