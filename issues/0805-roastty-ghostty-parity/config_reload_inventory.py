#!/usr/bin/env python3
"""Inventory Roastty config reload coverage for Issue 805.

This is a bounded markdown/source inventory for CFG-222. It records pinned
Ghostty reload behaviors and only marks rows complete when existing Roastty
evidence proves the reload behavior at the same boundary.
"""

from __future__ import annotations

import argparse
import dataclasses
from collections import Counter
from pathlib import Path


@dataclasses.dataclass(frozen=True)
class ReloadRow:
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
    ReloadRow(
        id="RELOAD-001",
        behavior="irrelevant conditional changes return no new config",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::changeConditionalState`",
        roastty_reference="`roastty/src/config/mod.rs::change_conditional_state_with_theme_locations`",
        family="conditional config",
        status="Oracle complete",
        evidence=(
            "`config_conditional_theme_same_state_returns_none` and "
            "`config_conditional_theme_ignores_irrelevant_theme_state_change` "
            "prove Roastty returns `None` when the requested state is unchanged "
            "or when the changed key is not present in the conditional set."
        ),
        missing_evidence="None for irrelevant conditional state reload behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml config_conditional_theme`",
    ),
    ReloadRow(
        id="RELOAD-002",
        behavior="relevant theme conditional changes replay config and finalize a new config",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::changeConditionalState`, `finalize`",
        roastty_reference="`roastty/src/config/mod.rs::change_conditional_state_with_theme_locations`",
        family="conditional config",
        status="Oracle complete",
        evidence=(
            "`config_conditional_theme_change_reloads_theme_and_preserves_user_priority` "
            "and `config_conditional_theme_clone_can_reload_back_to_light` prove "
            "a relevant theme-state change rebuilds a finalized config through "
            "replay and can move dark and light directions."
        ),
        missing_evidence="None for relevant conditional theme reload behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml config_conditional_theme`",
    ),
    ReloadRow(
        id="RELOAD-003",
        behavior="conditional reload preserves replay entries without duplication",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::Replay`, `changeConditionalState`",
        roastty_reference="`roastty/src/config/mod.rs::ConfigReplayEntry`, `replay_into`",
        family="replay",
        status="Oracle complete",
        evidence=(
            "`config_conditional_theme_rebuild_preserves_replay_entries_without_duplication` "
            "proves replay entries remain equal before and after conditional "
            "rebuild, CLI priority is preserved, and the theme replay entry is "
            "not duplicated."
        ),
        missing_evidence="None for conditional reload replay preservation.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml config_conditional_theme_rebuild_preserves_replay_entries_without_duplication`",
    ),
    ReloadRow(
        id="RELOAD-004",
        behavior="theme reload applies theme values first and user config values on top",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadTheme`",
        roastty_reference="`roastty/src/config/mod.rs::load_theme_file`",
        family="theme reload",
        status="Oracle complete",
        evidence=(
            "`config_conditional_theme_change_reloads_theme_and_preserves_user_priority` "
            "proves theme-sourced foreground changes with the selected theme "
            "while user-configured background continues to override the theme."
        ),
        missing_evidence="None for theme-before-user reload priority.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml config_conditional_theme_change_reloads_theme_and_preserves_user_priority`",
    ),
    ReloadRow(
        id="RELOAD-005",
        behavior="theme reload failure reports failure while preserving window-theme conditional semantics",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadTheme`, `finalize`",
        roastty_reference="`roastty/src/config/mod.rs::finalize_theme`, `load_theme_file`",
        family="theme reload",
        status="Oracle complete",
        evidence=(
            "`config_theme_loading_reports_missing_resource`, "
            "`config_theme_loading_reports_not_file`, "
            "`config_theme_loading_reports_unreadable_file`, and "
            "`config_conditional_theme_marks_different_light_dark_as_relevant` "
            "prove failed theme loads are reported and light/dark theme "
            "selection still forces `window-theme = system` and marks theme as "
            "a relevant conditional key."
        ),
        missing_evidence="None for theme reload failure/finalization behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml config_theme_loading && cargo test --manifest-path roastty/Cargo.toml config_conditional_theme_marks_different_light_dark_as_relevant`",
    ),
    ReloadRow(
        id="RELOAD-006",
        behavior="app color-scheme changes update app conditional state and request a soft app reload",
        ghostty_reference="`vendor/ghostty/src/App.zig::setColorScheme` reload request",
        roastty_reference="`roastty/src/lib.rs::roastty_app_set_color_scheme`",
        family="app reload",
        status="Oracle complete",
        evidence=(
            "`app_set_color_scheme_updates_conditional_state_and_requests_reload` "
            "proves invalid/no-op scheme changes are ignored, light/dark "
            "changes update app conditional state, and the app target receives "
            "a soft reload request."
        ),
        missing_evidence="None for app soft reload request behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml app_set_color_scheme_updates_conditional_state_and_requests_reload`",
    ),
    ReloadRow(
        id="RELOAD-007",
        behavior="app config update applies app conditional state and propagates to surfaces",
        ghostty_reference="`vendor/ghostty/src/App.zig::updateConfig`",
        roastty_reference="`roastty/src/lib.rs::roastty_app_update_config`",
        family="app reload",
        status="Oracle complete",
        evidence=(
            "`app_set_color_scheme_updates_conditional_state_and_requests_reload` "
            "proves `roastty_app_update_config` applies the app dark-state "
            "conditional config after a soft reload request; key-remap and "
            "renderer update tests prove app config updates propagate to "
            "existing surfaces."
        ),
        missing_evidence="None for app-level conditional config propagation.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml app_set_color_scheme && cargo test --manifest-path roastty/Cargo.toml surface_key_remap_app_update_refreshes_existing_surface`",
    ),
    ReloadRow(
        id="RELOAD-008",
        behavior="new surfaces inherit app conditional state while preserving launch working directory",
        ghostty_reference="`vendor/ghostty/src/Surface.zig::init`",
        roastty_reference="`roastty/src/lib.rs::roastty_surface_new`",
        family="surface reload",
        status="Oracle complete",
        evidence=(
            "`surface_new_conditional_uses_app_state_and_preserves_working_directory` "
            "proves a new surface created after an app theme-state change uses "
            "the app conditional state and keeps its explicitly supplied "
            "working directory."
        ),
        missing_evidence="None for new-surface conditional inheritance behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml surface_new_conditional_uses_app_state_and_preserves_working_directory`",
    ),
    ReloadRow(
        id="RELOAD-009",
        behavior="surface color-scheme changes update surface conditional state and request a soft surface reload",
        ghostty_reference="`vendor/ghostty/src/Surface.zig::setColorScheme` reload request",
        roastty_reference="`roastty/src/lib.rs::roastty_surface_set_color_scheme`",
        family="surface reload",
        status="Oracle complete",
        evidence=(
            "`surface_set_color_scheme_updates_conditional_state_reloads_and_reports` "
            "proves invalid/no-op scheme changes are ignored, surface "
            "conditional state changes, and the surface target receives a soft "
            "reload request."
        ),
        missing_evidence="None for surface soft reload request behavior.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml surface_set_color_scheme_updates_conditional_state_reloads_and_reports`",
    ),
    ReloadRow(
        id="RELOAD-010",
        behavior="surface config update applies surface conditional state",
        ghostty_reference="`vendor/ghostty/src/Surface.zig::updateConfig` conditional branch",
        roastty_reference="`roastty/src/lib.rs::Surface::apply_config`",
        family="surface reload",
        status="Oracle complete",
        evidence=(
            "`surface_update_config_uses_surface_conditional_state` proves "
            "surface update uses the surface's own conditional state and does "
            "not accidentally follow later app-state changes."
        ),
        missing_evidence="None for surface-level conditional config application.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml surface_update_config_uses_surface_conditional_state`",
    ),
    ReloadRow(
        id="RELOAD-011",
        behavior="surface config update propagates reloadable terminal/runtime state",
        ghostty_reference="`vendor/ghostty/src/Surface.zig::updateConfig` renderer and termio change_config branches",
        roastty_reference="`roastty/src/lib.rs::Surface::apply_config`",
        family="surface reload",
        status="Oracle complete",
        evidence=(
            "`surface_apply_config_updates_palette_defaults`, "
            "`surface_apply_config_updates_generated_palette_defaults`, "
            "`surface_key_remap_surface_update_refreshes_existing_surface`, and "
            "live renderer update tests prove reloadable palette, key remap, "
            "renderer, mouse, selection, and paste-related fields are updated "
            "from config on surface reload."
        ),
        missing_evidence="None for the covered reloadable surface state.",
        guard_tier="Tier 1",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml surface_apply_config && cargo test --manifest-path roastty/Cargo.toml surface_key_remap_surface_update_refreshes_existing_surface`",
    ),
    ReloadRow(
        id="RELOAD-012",
        behavior="surface reload clears active key tables",
        ghostty_reference="`vendor/ghostty/src/Surface.zig::updateConfig` `deactivateAllKeyTables` branch",
        roastty_reference="`roastty/src/lib.rs::Surface::apply_config`",
        family="surface reload",
        status="Gap",
        evidence=(
            "Source audit found `Surface::apply_config` updates key remaps and "
            "keybind storage, but does not call `deactivate_all_key_tables` or "
            "otherwise clear `active_key_tables` during config update."
        ),
        missing_evidence=(
            "Add Roastty behavior matching pinned Ghostty: config update should "
            "clear active key tables and report the deactivation action when "
            "needed."
        ),
        guard_tier="Tier 1",
        guard_command="TBD by follow-up reload gap experiment.",
    ),
    ReloadRow(
        id="RELOAD-013",
        behavior="surface reload preserves manual font size but applies configured font size when unadjusted",
        ghostty_reference="`vendor/ghostty/src/Surface.zig::updateConfig` `setFontSize` branch",
        roastty_reference="`roastty/src/lib.rs::Surface::apply_config`",
        family="surface reload",
        status="Gap",
        evidence=(
            "Source audit found `Surface::apply_config` does not update "
            "`font_size_points` from `config.parsed.font_size` on reload and "
            "therefore cannot yet prove Ghostty's manual-adjustment preservation "
            "rule."
        ),
        missing_evidence=(
            "Add Roastty behavior matching pinned Ghostty: unadjusted surfaces "
            "should adopt the configured font size on reload, while manually "
            "adjusted surfaces keep their current font size."
        ),
        guard_tier="Tier 1",
        guard_command="TBD by follow-up reload gap experiment.",
    ),
    ReloadRow(
        id="RELOAD-014",
        behavior="hard reload action is distinct from soft reload and re-reads config",
        ghostty_reference="`vendor/ghostty/src/apprt/action.zig::ReloadConfig`, `vendor/ghostty/src/apprt/gtk/class/application.zig::reloadConfig`",
        roastty_reference="`roastty/src/lib.rs::RoasttyActionReloadConfig`, `roastty/macos/Sources/Roastty/Roastty.App.swift::reloadConfig`",
        family="app wrapper",
        status="Oracle complete",
        evidence=(
            "`RoasttyActionReloadConfig.soft` stores the soft flag, "
            "`Roastty.App.reloadConfig(soft:)` calls `roastty_app_update_config` "
            "with the existing config for soft reloads and constructs "
            "`Config(at: configPath)` for hard reloads, and "
            "`ConfigTests.reloadConfig` proves reloading config text updates a "
            "loaded Swift config object."
        ),
        missing_evidence="None for soft-vs-hard reload representation in the macOS wrapper.",
        guard_tier="Tier 2",
        guard_command="`cargo test --manifest-path roastty/Cargo.toml app_set_color_scheme_updates_conditional_state_and_requests_reload` plus macOS ConfigTests reloadConfig.",
    ),
]

EXPECTED_IDS = [
    "RELOAD-001",
    "RELOAD-002",
    "RELOAD-003",
    "RELOAD-004",
    "RELOAD-005",
    "RELOAD-006",
    "RELOAD-007",
    "RELOAD-008",
    "RELOAD-009",
    "RELOAD-010",
    "RELOAD-011",
    "RELOAD-012",
    "RELOAD-013",
    "RELOAD-014",
]


def validate_rows(rows: list[ReloadRow]) -> None:
    ids = [row.id for row in rows]
    if ids != EXPECTED_IDS:
        raise ValueError(f"reload row manifest mismatch: {ids!r}")

    duplicate_ids = [item for item, count in Counter(ids).items() if count > 1]
    if duplicate_ids:
        raise ValueError(f"duplicate reload row IDs: {duplicate_ids}")

    behaviors = [row.behavior for row in rows]
    duplicate_behaviors = [
        item for item, count in Counter(behaviors).items() if count > 1
    ]
    if duplicate_behaviors:
        raise ValueError(f"duplicate reload behavior names: {duplicate_behaviors}")

    valid_statuses = {
        "Oracle complete",
        "Audit covered",
        "Gap",
        "Intentional divergence",
        "Not applicable",
    }
    invalid_statuses = sorted({row.status for row in rows} - valid_statuses)
    if invalid_statuses:
        raise ValueError(f"invalid reload statuses: {invalid_statuses}")

    for row in rows:
        if not row.guard_tier or not row.guard_command:
            raise ValueError(f"missing guard field for {row.behavior}")
        if not row.ghostty_reference or not row.roastty_reference:
            raise ValueError(f"missing evidence anchor for {row.behavior}")
        if row.status == "Oracle complete" and row.missing_evidence.startswith("Add "):
            raise ValueError(f"oracle row has gap wording: {row.behavior}")
        if row.status == "Gap" and row.guard_command != "TBD by follow-up reload gap experiment.":
            raise ValueError(f"gap row has non-TBD guard: {row.behavior}")


def emit_inventory(rows: list[ReloadRow], output: Path) -> None:
    status_counts = Counter(row.status for row in rows)
    family_counts = Counter(row.family for row in rows)

    lines = [
        "# Config Reload Inventory",
        "",
        "Generated by `issues/0805-roastty-ghostty-parity/config_reload_inventory.py`",
        "for Issue 805 reload-facet experiments.",
        "",
        "## Counts",
        "",
        "| Category | Count |",
        "| --- | ---: |",
        f"| Reload rows | {len(rows)} |",
        f"| Oracle complete rows | {status_counts.get('Oracle complete', 0)} |",
        f"| Audit covered rows | {status_counts.get('Audit covered', 0)} |",
        f"| Gap rows | {status_counts.get('Gap', 0)} |",
        "",
        "## Reload Families",
        "",
        "| Reload family | Count |",
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


def update_cfg222(
    matrix: Path,
    reload_inventory_path: Path,
    closed_count: int,
    oracle_count: int,
    incomplete_count: int,
    gap_count: int,
) -> None:
    lines = matrix.read_text().splitlines()
    updated: list[str] = []
    found = False
    for line in lines:
        if line.startswith("| CFG-222 |"):
            found = True
            status = "Pass" if incomplete_count == 0 and gap_count == 0 else "Gap"
            notes = (
                f"Reload inventory coverage: {oracle_count} rows Oracle complete; "
                f"{closed_count} rows closed; "
                f"{incomplete_count} rows are incomplete and {gap_count} rows "
                "are reload gaps."
            )
            line = (
                "| CFG-222 | Config reload behavior | "
                "Ghostty reloads config and applies reloadable changes while "
                "preserving or rejecting non-reloadable state as designed. | "
                "Roastty reload behavior is inventoried by pinned Ghostty "
                "reload operations. | "
                f"{status} | Generated reload inventory plus matrix consistency "
                "assertion. | "
                f"`{reload_inventory_path}` | Tier 2 | "
                "`PYTHONDONTWRITEBYTECODE=1 python3 "
                "issues/0805-roastty-ghostty-parity/config_reload_inventory.py "
                "--output issues/0805-roastty-ghostty-parity/config-reload-inventory.md "
                "--matrix issues/0805-roastty-ghostty-parity/config-matrix.md` | "
                "Before closing Issue 805 and when config reload behavior changes. | "
                "CFG-222 only passes when every reload inventory row is "
                f"`Oracle complete`, `Not applicable`, or an accepted documented "
                f"divergence; audit coverage alone is insufficient. | Experiment 103 | {notes} |"
            )
        updated.append(line)

    if not found:
        raise ValueError("CFG-222 row not found in config matrix")

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
    update_cfg222(
        args.matrix,
        args.output,
        closed_count,
        oracle_count,
        incomplete_count,
        gap_count,
    )

    if incomplete_count == 0 and gap_count != 0:
        raise ValueError("CFG-222 cannot pass with reload gaps")

    print(f"reload_rows={len(rows)}")
    print(f"oracle_complete={oracle_count}")
    print(f"closed={closed_count}")
    print(f"audit_covered={audit_count}")
    print(f"incomplete={incomplete_count}")
    print(f"gap={gap_count}")
    print(f"cfg222={'Pass' if incomplete_count == 0 and gap_count == 0 else 'Gap'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
