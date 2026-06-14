#!/usr/bin/env python3
"""Inventory Roastty config finalization coverage for Issue 805.

This is a bounded markdown/source inventory for CFG-220. It records the
post-parse behaviors performed by pinned Ghostty's `Config.finalize` and only
marks rows complete when existing Roastty evidence proves the finalized value or
report behavior for the relevant context.
"""

from __future__ import annotations

import argparse
import dataclasses
from collections import Counter
from pathlib import Path


@dataclasses.dataclass(frozen=True)
class FinalizationRow:
    behavior: str
    ghostty_reference: str
    roastty_reference: str
    family: str
    status: str
    evidence: str
    missing_evidence: str


ROWS = [
    FinalizationRow(
        behavior="theme loading and conditional theme state",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::finalize`, `loadTheme` branch",
        roastty_reference="`roastty/src/config/mod.rs::finalize_theme`",
        family="theme finalization",
        status="Oracle complete",
        evidence=(
            "Theme finalize tests cover absolute theme paths, user/resource name "
            "search, not-found and not-file reports, light/dark conditional "
            "theme selection, `window-theme = auto` conversion to `system`, "
            "conditional-set replay, malformed theme diagnostics, BOM handling, "
            "and preservation of scalar finalization when no theme is set."
        ),
        missing_evidence="None for theme finalization inventory behavior.",
    ),
    FinalizationRow(
        behavior="font-family inheritance",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::finalize`, font-family clone block",
        roastty_reference="`roastty/src/config/mod.rs::finalize_scalars` font-family block",
        family="font finalization",
        status="Oracle complete",
        evidence=(
            "`config_font_family_finalize_inherits_regular_family` proves "
            "non-empty regular family is cloned into missing italic and "
            "bold-italic families while preserving an explicitly configured "
            "bold family."
        ),
        missing_evidence="None for font-family inheritance finalization behavior.",
    ),
    FinalizationRow(
        behavior="empty term fallback",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::finalize`, empty `term` fallback",
        roastty_reference="`roastty/src/config/mod.rs::finalize_scalars` term block",
        family="scalar finalization",
        status="Oracle complete",
        evidence=(
            "`config_finalize_scalar_tail` proves an empty `term` finalizes to "
            "`xterm-roastty` and formats that finalized value."
        ),
        missing_evidence=(
            "None for Roastty app-name-normalized term fallback behavior; the "
            "value intentionally uses Roastty's terminal name instead of Ghostty's."
        ),
    ),
    FinalizationRow(
        behavior="working-directory default from probable CLI",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::finalize`, `probable_cli` working-directory default",
        roastty_reference="`roastty/src/config/mod.rs::finalize_working_directory`",
        family="working directory finalization",
        status="Oracle complete",
        evidence=(
            "`config_working_directory_finalize_defaults_from_probable_cli` "
            "proves CLI-context defaulting to `inherit` and desktop-context "
            "fallback through home lookup behavior."
        ),
        missing_evidence="None for probable-CLI working-directory default behavior.",
    ),
    FinalizationRow(
        behavior="command and home defaults",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::finalize`, shell environment and passwd lookup block",
        roastty_reference="`roastty/src/config/mod.rs::finalize_command_and_home`",
        family="command/home finalization",
        status="Oracle complete",
        evidence=(
            "`config_command_home_finalize_*` tests prove env-shell preference "
            "in probable CLI contexts, desktop passwd-shell fallback, explicit "
            "command preservation, empty shell/home handling, unset behavior, "
            "non-UTF-8 rejection, and missing-home inheritance."
        ),
        missing_evidence="None for command/home default finalization behavior.",
    ),
    FinalizationRow(
        behavior="working-directory tilde expansion",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::WorkingDirectory.finalize`",
        roastty_reference="`roastty/src/config/mod.rs::WorkingDirectory::finalize_with_home`",
        family="working directory finalization",
        status="Oracle complete",
        evidence=(
            "`working_directory_finalize_expands_tilde_slash_paths`, "
            "`working_directory_finalize_preserves_non_expandable_values`, and "
            "`config_working_directory_finalize_*` tests prove `~/` expansion "
            "and preservation of `~`, `~user`, absolute paths, inherit, and "
            "non-UTF-8 home inputs."
        ),
        missing_evidence="None for working-directory tilde finalization behavior.",
    ),
    FinalizationRow(
        behavior="GTK single-instance detection",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::finalize`, GTK app-runtime switch",
        roastty_reference="`roastty/src/config/mod.rs::finalize_gtk_single_instance`",
        family="app-runtime finalization",
        status="Oracle complete",
        evidence=(
            "`config_gtk_single_instance_finalize_*` tests prove non-GTK "
            "runtime leaves `detect` unchanged, GTK probable-CLI detection "
            "resolves to false, GTK desktop detection resolves to true, and "
            "explicit true/false values are preserved."
        ),
        missing_evidence="None for GTK single-instance finalization behavior.",
    ),
    FinalizationRow(
        behavior="click-repeat interval defaulting",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::finalize`, `internal_os.clickInterval() orelse 500`",
        roastty_reference="`roastty/src/config/mod.rs::finalize_scalars` click-repeat block",
        family="platform scalar finalization",
        status="Audit covered",
        evidence=(
            "`mouse_behavior_finalize_resolves_and_clamps` proves parser-level "
            "`0` finalizes to 500 and nonzero values are preserved in Roastty."
        ),
        missing_evidence=(
            "Needs explicit parity proof for app-runtime OS click interval "
            "defaulting, because pinned Ghostty uses `internal_os.clickInterval() "
            "orelse 500` outside tests while Roastty currently finalizes `0` to "
            "500 through a direct scalar block."
        ),
    ),
    FinalizationRow(
        behavior="mouse scroll multiplier clamps",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::finalize`, mouse-scroll clamp block",
        roastty_reference="`roastty/src/config/mod.rs::finalize_scalars` mouse-scroll clamp block",
        family="clamp finalization",
        status="Oracle complete",
        evidence=(
            "`mouse_behavior_finalize_resolves_and_clamps` proves precision and "
            "discrete multipliers clamp to the Ghostty 0.01..10000 range and "
            "preserve in-range values."
        ),
        missing_evidence="None for mouse-scroll clamp finalization behavior.",
    ),
    FinalizationRow(
        behavior="unfocused split opacity clamp",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::finalize`, split opacity clamp",
        roastty_reference="`roastty/src/config/mod.rs::finalize_scalars` unfocused split opacity clamp",
        family="clamp finalization",
        status="Oracle complete",
        evidence=(
            "`split_visual_config_defaults_parse_format_and_finalize` proves "
            "the default value and formatting, below-minimum parsed values "
            "clamping to 0.15 during finalization, above-maximum parsed values "
            "clamping to 1.0 during finalization, and config-file parsed "
            "out-of-range values clamping after finalization."
        ),
        missing_evidence="None for unfocused-split-opacity clamp finalization behavior.",
    ),
    FinalizationRow(
        behavior="minimum contrast clamp",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::finalize`, minimum contrast clamp",
        roastty_reference="`roastty/src/config/mod.rs::finalize_scalars` minimum contrast clamp",
        family="clamp finalization",
        status="Oracle complete",
        evidence=(
            "`config_finalize_scalar_tail` and "
            "`config_quit_delay_finalize_warning` prove low and high "
            "`minimum-contrast` values clamp to the Ghostty 1.0..21.0 range."
        ),
        missing_evidence="None for minimum-contrast clamp finalization behavior.",
    ),
    FinalizationRow(
        behavior="minimum window size clamp",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::finalize`, minimum window size block",
        roastty_reference="`roastty/src/config/mod.rs::finalize_scalars` window size block",
        family="clamp finalization",
        status="Oracle complete",
        evidence=(
            "`window_size_step_config_parse_format_reset_finalize_and_diagnose` "
            "proves zero width/height remain zero, values below the minimum "
            "finalize to 10x4, and in-range values are preserved."
        ),
        missing_evidence="None for minimum window size finalization behavior.",
    ),
    FinalizationRow(
        behavior="link-url default matcher pruning",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::finalize`, `link-url` pruning block",
        roastty_reference="`roastty/src/config/mod.rs::finalize_link_url`",
        family="link finalization",
        status="Oracle complete",
        evidence=(
            "`config_link_url_finalize` proves the default URL matcher exists, "
            "`link-url = true` preserves it, and `link-url = false` removes it "
            "during finalization."
        ),
        missing_evidence="None for link-url finalization behavior.",
    ),
    FinalizationRow(
        behavior="quit-after-last-window-closed-delay warning",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::finalize`, short quit-delay warning block",
        roastty_reference="`roastty/src/config/mod.rs::finalize_quit_delay_warning`",
        family="report finalization",
        status="Oracle complete",
        evidence=(
            "`config_quit_delay_finalize_warning` proves unset, short, exact "
            "threshold, and long quit-delay values produce the expected "
            "`ConfigFinalizeReport` warning behavior without mutating the value."
        ),
        missing_evidence="None for quit-delay warning finalization behavior.",
    ),
    FinalizationRow(
        behavior="auto-update-channel default",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::finalize`, release-channel default block",
        roastty_reference="`roastty/src/config/mod.rs::finalize_scalars` auto-update-channel block",
        family="build-config finalization",
        status="Audit covered",
        evidence=(
            "`config_finalize_scalar_tail` proves Roastty fills an unset "
            "`auto-update-channel` with `ReleaseChannel::Tip` and preserves an "
            "explicit value."
        ),
        missing_evidence=(
            "Needs explicit parity proof that Roastty's pinned build release "
            "channel is the correct app-name-normalized equivalent of Ghostty's "
            "`build_config.release_channel` for the compared build."
        ),
    ),
    FinalizationRow(
        behavior="faint opacity clamp",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::finalize`, faint opacity clamp",
        roastty_reference="`roastty/src/config/mod.rs::finalize_scalars` faint opacity clamp",
        family="clamp finalization",
        status="Oracle complete",
        evidence=(
            "`config_finalize_scalar_tail` proves high and low `faint-opacity` "
            "values clamp to the Ghostty 0.0..1.0 range."
        ),
        missing_evidence="None for faint-opacity clamp finalization behavior.",
    ),
    FinalizationRow(
        behavior="key-remap finalization",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::finalize`, key-remap finalization call",
        roastty_reference="`roastty/src/config/mod.rs::finalize_scalars` key-remap finalization call",
        family="key remap finalization",
        status="Oracle complete",
        evidence=(
            "Key remap parser oracle covers source expansion, target side "
            "defaults, and finalize ordering where right-sided mappings override "
            "generic expansions."
        ),
        missing_evidence="None for key-remap finalization behavior.",
    ),
]


def validate_rows(rows: list[FinalizationRow]) -> None:
    ids = [f"FINAL-{index:03d}" for index, _ in enumerate(rows, 1)]
    duplicate_ids = [item for item, count in Counter(ids).items() if count > 1]
    if duplicate_ids:
        raise ValueError(f"duplicate finalization row IDs: {duplicate_ids}")

    behaviors = [row.behavior for row in rows]
    duplicate_behaviors = [
        item for item, count in Counter(behaviors).items() if count > 1
    ]
    if duplicate_behaviors:
        raise ValueError(
            f"duplicate finalization behavior names: {duplicate_behaviors}"
        )

    valid_statuses = {"Oracle complete", "Audit covered", "Gap"}
    invalid_statuses = sorted({row.status for row in rows} - valid_statuses)
    if invalid_statuses:
        raise ValueError(f"invalid finalization statuses: {invalid_statuses}")


def emit_inventory(rows: list[FinalizationRow], output: Path) -> None:
    status_counts = Counter(row.status for row in rows)
    family_counts = Counter(row.family for row in rows)

    lines = [
        "# Config Finalization Inventory",
        "",
        "Generated by `issues/0805-roastty-ghostty-parity/config_finalization_inventory.py`",
        "for Issue 805 finalization-facet experiments.",
        "",
        "## Counts",
        "",
        "| Category | Count |",
        "| --- | ---: |",
        f"| Finalization rows | {len(rows)} |",
        f"| Oracle complete rows | {status_counts.get('Oracle complete', 0)} |",
        f"| Audit covered rows | {status_counts.get('Audit covered', 0)} |",
        f"| Gap rows | {status_counts.get('Gap', 0)} |",
        "",
        "## Finalization Families",
        "",
        "| Finalization family | Count |",
        "| --- | ---: |",
    ]
    for family, count in sorted(family_counts.items()):
        lines.append(f"| {family} | {count} |")

    lines.extend(
        [
            "",
            "## Rows",
            "",
            "| ID | Behavior | Ghostty reference | Roastty reference | Family | Status | Evidence | Missing evidence |",
            "| --- | --- | --- | --- | --- | --- | --- | --- |",
        ]
    )
    for index, row in enumerate(rows, 1):
        lines.append(
            f"| FINAL-{index:03d} | {row.behavior} | {row.ghostty_reference} | "
            f"{row.roastty_reference} | {row.family} | {row.status} | "
            f"{row.evidence} | {row.missing_evidence} |"
        )
    output.write_text("\n".join(lines) + "\n")


def update_cfg220(
    matrix: Path,
    finalization_inventory_path: Path,
    oracle_count: int,
    incomplete_count: int,
    gap_count: int,
) -> None:
    lines = matrix.read_text().splitlines()
    updated: list[str] = []
    for line in lines:
        if line.startswith("| CFG-220 |"):
            status = "Pass" if incomplete_count == 0 else "Gap"
            notes = (
                f"Finalization inventory coverage: {oracle_count} rows Oracle "
                f"complete; {incomplete_count} rows are not Oracle complete and "
                f"{gap_count} rows are finalization gaps."
            )
            line = (
                "| CFG-220 | Validation and finalization behavior | Ghostty "
                "finalizes config values using platform/runtime context, "
                "including validation and derived defaults. | Roastty "
                "finalization behavior is inventoried by pinned Ghostty "
                f"`Config.finalize` operation. | {status} | Generated "
                "finalization-facet inventory plus matrix consistency assertion. | "
                f"`{finalization_inventory_path}` | Tier 1 | "
                "`PYTHONDONTWRITEBYTECODE=1 python3 "
                "issues/0805-roastty-ghostty-parity/config_finalization_inventory.py "
                "--output issues/0805-roastty-ghostty-parity/config-finalization-inventory.md "
                "--matrix issues/0805-roastty-ghostty-parity/config-matrix.md` | "
                "Before closing Issue 805 and when config finalization changes. | "
                "CFG-220 only passes when every finalization inventory row is "
                f"`Oracle complete`; audit coverage alone is insufficient. | Experiment 95 | {notes} |"
            )
        updated.append(line)
    matrix.write_text("\n".join(updated) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--matrix", type=Path, required=True)
    args = parser.parse_args()

    rows = list(ROWS)
    validate_rows(rows)
    emit_inventory(rows, args.output)

    oracle_count = sum(row.status == "Oracle complete" for row in rows)
    incomplete_count = sum(row.status != "Oracle complete" for row in rows)
    gap_count = sum(row.status == "Gap" for row in rows)
    audit_count = sum(row.status == "Audit covered" for row in rows)
    update_cfg220(args.matrix, args.output, oracle_count, incomplete_count, gap_count)

    print(f"finalization_rows={len(rows)}")
    print(f"oracle_complete={oracle_count}")
    print(f"audit_covered={audit_count}")
    print(f"gap={gap_count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
