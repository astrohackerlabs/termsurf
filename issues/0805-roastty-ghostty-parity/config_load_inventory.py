#!/usr/bin/env python3
"""Inventory Roastty config source precedence/load coverage for Issue 805.

This is a bounded markdown/source inventory for CFG-221. It records the pinned
Ghostty load pipeline operations and only marks rows complete when existing
Roastty evidence proves source order, state mutation, diagnostics/errors,
repeatable behavior, and path-base behavior where relevant.
"""

from __future__ import annotations

import argparse
import dataclasses
from collections import Counter
from pathlib import Path


@dataclasses.dataclass(frozen=True)
class LoadRow:
    behavior: str
    ghostty_reference: str
    roastty_reference: str
    family: str
    status: str
    evidence: str
    missing_evidence: str


ROWS = [
    LoadRow(
        behavior="full load pipeline order",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::load`",
        roastty_reference="Roastty load pieces are implemented separately in `roastty/src/config/mod.rs`",
        family="pipeline",
        status="Audit covered",
        evidence=(
            "Roastty has separate default-file, CLI, recursive-file, and "
            "finalize entry points with focused tests for each stage."
        ),
        missing_evidence=(
            "Needs an end-to-end load pipeline oracle proving default files, CLI "
            "args, recursive config files, and finalization run in the pinned "
            "Ghostty order."
        ),
    ),
    LoadRow(
        behavior="config-file reader parsing and BOM skipping",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadFile`, `loadReader`",
        roastty_reference="`roastty/src/config/mod.rs::load_file`, `load_str`",
        family="file load",
        status="Oracle complete",
        evidence=(
            "`config_load_str_applies_lines_and_collects_diagnostics` and "
            "`config_load_file_reads_and_skips_bom` prove line parsing, comments, "
            "blank lines, quoted values, per-line diagnostics, continued loading, "
            "file reads, UTF-8 BOM skipping, and missing-file error behavior."
        ),
        missing_evidence="None for file reader/BOM load behavior.",
    ),
    LoadRow(
        behavior="config-file path base expansion after file load",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadReader`, `expandPaths`",
        roastty_reference="`roastty/src/config/mod.rs::load_file`, `expand_config_file_paths_from_base`",
        family="path base",
        status="Oracle complete",
        evidence=(
            "`config_file_load_expands_paths_relative_to_config_file`, "
            "`config_file_load_relative_config_path_uses_canonical_parent_base`, "
            "and `bell_audio_path_expands_from_default_and_recursive_file_bases` "
            "prove config-file and path-valued entries expand relative to the "
            "loading file/default/recursive source base."
        ),
        missing_evidence="None for config-file path-base expansion behavior.",
    ),
    LoadRow(
        behavior="optional file three-way behavior",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadOptionalFile`",
        roastty_reference="`roastty/src/config/mod.rs::load_optional_file`",
        family="optional file",
        status="Oracle complete",
        evidence=(
            "`config_load_optional_file_three_way_action` proves Loaded, "
            "NotFound, and Error outcomes and confirms loaded file state applies."
        ),
        missing_evidence="None for optional-file three-way behavior.",
    ),
    LoadRow(
        behavior="default XDG and platform file load order",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadDefaultFiles`",
        roastty_reference="`roastty/src/config/mod.rs::load_default_files_from_paths`",
        family="default files",
        status="Oracle complete",
        evidence=(
            "`config_load_default_files_applies_candidates_in_order` proves legacy "
            "XDG, preferred XDG, legacy app support, and preferred app support "
            "load in order and later values win."
        ),
        missing_evidence="None for default-file order behavior.",
    ),
    LoadRow(
        behavior="default file duplicate reporting",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadDefaultFiles` duplicate warning branches",
        roastty_reference="`roastty/src/config/mod.rs::DefaultConfigLoadReport` duplicate fields",
        family="default files",
        status="Oracle complete",
        evidence=(
            "`config_load_default_files_applies_candidates_in_order`, "
            "`config_load_default_files_deduplicates_equal_app_support_paths`, "
            "`config_load_default_files_reports_xdg_error_duplicates`, and "
            "`config_load_default_files_reports_app_support_error_duplicates` "
            "prove duplicate XDG/app-support reporting and same-path app support "
            "deduplication."
        ),
        missing_evidence="None for default-file duplicate reporting behavior.",
    ),
    LoadRow(
        behavior="default file errors and diagnostics continue loading",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadDefaultFiles`, `loadOptionalFile`",
        roastty_reference="`roastty/src/config/mod.rs::load_default_files_from_paths`",
        family="default files",
        status="Oracle complete",
        evidence=(
            "`config_load_default_files_reports_errors_and_diagnostics_without_aborting` "
            "proves non-not-found errors are reported, later files still load, "
            "line diagnostics are retained, and valid later lines still apply."
        ),
        missing_evidence="None for default-file error/diagnostic continuation behavior.",
    ),
    LoadRow(
        behavior="default template creation when no default file exists",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadDefaultFiles`, `writeConfigTemplate`",
        roastty_reference="`roastty/src/config/mod.rs::load_default_files_from_paths`, `write_default_config_template`",
        family="default files",
        status="Oracle complete",
        evidence=(
            "`config_load_default_files_creates_app_support_template_when_all_missing`, "
            "`config_load_default_files_creates_xdg_template_without_app_support_target`, "
            "`config_load_default_files_suppresses_template_when_candidate_loaded_or_errors`, "
            "and `config_load_default_files_records_template_creation_errors_without_aborting` "
            "prove target selection, pinned template text with substituted path, "
            "suppression when any default candidate loads or errors, and nonfatal "
            "creation error recording."
        ),
        missing_evidence="None for default config template creation behavior.",
    ),
    LoadRow(
        behavior="CLI diagnostics and good-argument continuation",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadCliArgs`, `loadIter`",
        roastty_reference="`roastty/src/config/mod.rs::set_cli_args_from_base`",
        family="CLI",
        status="Oracle complete",
        evidence=(
            "`config_set_cli_args_applies_and_collects_diagnostics` proves good "
            "CLI args apply, non-flag/unknown/invalid args produce positional "
            "diagnostics, and later/good args continue."
        ),
        missing_evidence="None for CLI diagnostic continuation behavior.",
    ),
    LoadRow(
        behavior="CLI repeatable font overwrite behavior",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadCliArgs` font-family overwrite block",
        roastty_reference="`roastty/src/config/mod.rs::set_cli_args_from_base` font overwrite block",
        family="CLI",
        status="Oracle complete",
        evidence=(
            "`config_replay_preserves_cli_repeatable_overwrite`, "
            "`config_replay_preserves_separate_cli_repeatable_overwrites`, and "
            "font parser/formatter oracles prove CLI font-family batches replace "
            "prior file/default values while preserving multiple CLI values in a "
            "batch."
        ),
        missing_evidence="None for CLI repeatable font overwrite behavior.",
    ),
    LoadRow(
        behavior="CLI config-default-files discard/replay behavior",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadCliArgs` replay discard block",
        roastty_reference="`roastty/src/config/mod.rs::set_cli_args_from_base` replay discard block",
        family="CLI",
        status="Oracle complete",
        evidence=(
            "Experiment 49 `config_default_files_parser_family_oracle` proves "
            "file-sourced `config-default-files` has no effect, CLI false "
            "discards previously loaded default-file values, and CLI true/empty "
            "preserves them."
        ),
        missing_evidence="None for config-default-files load discard behavior.",
    ),
    LoadRow(
        behavior="CLI path base expansion",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadCliArgs`, `expandPaths(cwd)`",
        roastty_reference="`roastty/src/config/mod.rs::set_cli_args_from_base`, `expand_config_file_paths_from_base`",
        family="path base",
        status="Oracle complete",
        evidence=(
            "`config_path_cli_expands_relative_optional_absolute_home_and_missing` "
            "and path-family oracles prove CLI path values expand relative to the "
            "provided base/current directory, preserve absolute paths, expand "
            "home paths, and handle optional missing paths."
        ),
        missing_evidence="None for CLI path-base expansion behavior.",
    ),
    LoadRow(
        behavior="recursive config-file order and newly appended files",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadRecursiveFiles` while-loop",
        roastty_reference="`roastty/src/config/mod.rs::load_recursive_files_from_config`",
        family="recursive config-file",
        status="Oracle complete",
        evidence=(
            "`config_recursive_loads_children_after_parent_and_grandchildren_in_order` "
            "proves recursive files load after the parent, newly appended "
            "grandchildren are visited by the while-loop, and later recursive "
            "values win."
        ),
        missing_evidence="None for recursive config-file order behavior.",
    ),
    LoadRow(
        behavior="recursive optional and required missing file behavior",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadRecursiveFiles` open-file branch",
        roastty_reference="`roastty/src/config/mod.rs::load_recursive_files_from_config`",
        family="recursive config-file",
        status="Oracle complete",
        evidence=(
            "`config_recursive_suppresses_optional_missing_and_reports_required_missing` "
            "proves optional missing recursive files are suppressed while required "
            "missing files are reported."
        ),
        missing_evidence="None for recursive missing-file behavior.",
    ),
    LoadRow(
        behavior="recursive non-file diagnostics",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadRecursiveFiles` file-type branch",
        roastty_reference="`roastty/src/config/mod.rs::load_recursive_files_from_config`",
        family="recursive config-file",
        status="Oracle complete",
        evidence=(
            "`config_recursive_reports_relative_and_non_file_errors` proves "
            "relative recursive paths and non-file recursive paths are reported "
            "without loading."
        ),
        missing_evidence="None for recursive non-file diagnostics behavior.",
    ),
    LoadRow(
        behavior="recursive cycle diagnostics",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadRecursiveFiles` loaded-map branch",
        roastty_reference="`roastty/src/config/mod.rs::load_recursive_files_from_config`",
        family="recursive config-file",
        status="Oracle complete",
        evidence=(
            "`config_recursive_reports_cycles_and_loads_once` proves repeated "
            "recursive paths load once and report cycles."
        ),
        missing_evidence="None for recursive cycle behavior.",
    ),
    LoadRow(
        behavior="recursive replay placement before initial command suffix",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadRecursiveFiles` replay-suffix branch before `-e`",
        roastty_reference="No Roastty `-e` replay-suffix placement implementation found in `roastty/src/config/mod.rs`",
        family="recursive config-file",
        status="Gap",
        evidence=(
            "Pinned Ghostty moves recursive `config-file` replay steps before "
            "the `-e`/initial-command suffix so recursive config-file entries do "
            "not become command arguments during replay."
        ),
        missing_evidence=(
            "Needs Roastty implementation/proof for recursive replay placement "
            "relative to initial command suffix, or an intentional divergence."
        ),
    ),
    LoadRow(
        behavior="replay preservation for theme and conditional rebuilds",
        ghostty_reference="`vendor/ghostty/src/config/Config.zig::loadTheme`, `changeConditionalState`",
        roastty_reference="`roastty/src/config/mod.rs::load_theme_file`, `change_conditional_state_with_theme_locations`",
        family="replay",
        status="Oracle complete",
        evidence=(
            "`config_replay_records_file_and_cli_successes_in_order`, "
            "`config_replay_into_fresh_config_reconstructs_values_without_duplication`, "
            "`config_theme_loading_preserves_user_replay_entries`, and "
            "`config_conditional_theme_rebuild_preserves_replay_entries_without_duplication` "
            "prove replay entries preserve file/CLI order and remain stable "
            "through theme and conditional rebuild paths."
        ),
        missing_evidence="None for theme/conditional replay preservation behavior.",
    ),
]

EXPECTED_IDS = [
    "LOAD-001",
    "LOAD-002",
    "LOAD-003",
    "LOAD-004",
    "LOAD-005",
    "LOAD-006",
    "LOAD-007",
    "LOAD-008",
    "LOAD-009",
    "LOAD-010",
    "LOAD-011",
    "LOAD-012",
    "LOAD-013",
    "LOAD-014",
    "LOAD-015",
    "LOAD-016",
    "LOAD-017",
    "LOAD-018",
]


def validate_rows(rows: list[LoadRow]) -> None:
    ids = [f"LOAD-{index:03d}" for index, _ in enumerate(rows, 1)]
    if ids != EXPECTED_IDS:
        raise ValueError(f"load row manifest mismatch: {ids!r}")

    duplicate_ids = [item for item, count in Counter(ids).items() if count > 1]
    if duplicate_ids:
        raise ValueError(f"duplicate load row IDs: {duplicate_ids}")

    behaviors = [row.behavior for row in rows]
    duplicate_behaviors = [
        item for item, count in Counter(behaviors).items() if count > 1
    ]
    if duplicate_behaviors:
        raise ValueError(f"duplicate load behavior names: {duplicate_behaviors}")

    valid_statuses = {"Oracle complete", "Audit covered", "Gap"}
    invalid_statuses = sorted({row.status for row in rows} - valid_statuses)
    if invalid_statuses:
        raise ValueError(f"invalid load statuses: {invalid_statuses}")


def emit_inventory(rows: list[LoadRow], output: Path) -> None:
    status_counts = Counter(row.status for row in rows)
    family_counts = Counter(row.family for row in rows)

    lines = [
        "# Config Load Inventory",
        "",
        "Generated by `issues/0805-roastty-ghostty-parity/config_load_inventory.py`",
        "for Issue 805 source-precedence/load-facet experiments.",
        "",
        "## Counts",
        "",
        "| Category | Count |",
        "| --- | ---: |",
        f"| Load rows | {len(rows)} |",
        f"| Oracle complete rows | {status_counts.get('Oracle complete', 0)} |",
        f"| Audit covered rows | {status_counts.get('Audit covered', 0)} |",
        f"| Gap rows | {status_counts.get('Gap', 0)} |",
        "",
        "## Load Families",
        "",
        "| Load family | Count |",
        "| --- | ---: |",
    ]
    for family, count in sorted(family_counts.items()):
        lines.append(f"| {family} | {count} |")

    lines.extend(
        [
            "",
            "## Expected Row Manifest",
            "",
        ]
    )
    lines.extend(f"- `{row_id}`" for row_id in EXPECTED_IDS)

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
            f"| LOAD-{index:03d} | {row.behavior} | {row.ghostty_reference} | "
            f"{row.roastty_reference} | {row.family} | {row.status} | "
            f"{row.evidence} | {row.missing_evidence} |"
        )
    output.write_text("\n".join(lines) + "\n")


def update_cfg221(
    matrix: Path,
    load_inventory_path: Path,
    oracle_count: int,
    incomplete_count: int,
    gap_count: int,
) -> None:
    lines = matrix.read_text().splitlines()
    updated: list[str] = []
    for line in lines:
        if line.startswith("| CFG-221 |"):
            status = "Pass" if incomplete_count == 0 else "Gap"
            notes = (
                f"Load inventory coverage: {oracle_count} rows Oracle complete; "
                f"{incomplete_count} rows are not Oracle complete and {gap_count} "
                "rows are load gaps."
            )
            line = (
                "| CFG-221 | Config source precedence and repeated-file load semantics | "
                "Ghostty applies defaults, config files, CLI args, repeatable values, "
                "and repeated config-file loads with documented precedence. | "
                "Roastty source precedence and load behavior is inventoried by "
                "pinned Ghostty load-pipeline operation. | "
                f"{status} | Generated load inventory plus matrix consistency "
                "assertion. | "
                f"`{load_inventory_path}` | Tier 2 | "
                "`PYTHONDONTWRITEBYTECODE=1 python3 "
                "issues/0805-roastty-ghostty-parity/config_load_inventory.py "
                "--output issues/0805-roastty-ghostty-parity/config-load-inventory.md "
                "--matrix issues/0805-roastty-ghostty-parity/config-matrix.md` | "
                "Before closing Issue 805 and when config load/source precedence changes. | "
                "CFG-221 only passes when every load inventory row is "
                f"`Oracle complete`; audit coverage alone is insufficient. | Experiment 99 | {notes} |"
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
    update_cfg221(args.matrix, args.output, oracle_count, incomplete_count, gap_count)

    print(f"load_rows={len(rows)}")
    print(f"oracle_complete={oracle_count}")
    print(f"audit_covered={audit_count}")
    print(f"gap={gap_count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
