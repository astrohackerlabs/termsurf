#!/usr/bin/env python3
"""Inventory Roastty invalid-value diagnostic coverage for pinned config options.

This is a bounded markdown/source inventory for Issue 805 diagnostic-facet
experiments. It does not infer full parity from parser coverage; it records one
diagnostic row per canonical option and only marks rows complete when existing
evidence explicitly names diagnostic behavior.
"""

from __future__ import annotations

import argparse
import dataclasses
import re
from pathlib import Path


PARSER_ROW_RE = re.compile(r"^\| PARSE-\d+ \|")

BOOLEAN_DIAGNOSTIC_ORACLE_OPTIONS = {
    "background-image-repeat",
    "background-opacity-cells",
    "clipboard-paste-bracketed-safe",
    "clipboard-paste-protection",
    "clipboard-trim-trailing-spaces",
    "cursor-click-to-move",
    "desktop-notifications",
    "focus-follows-mouse",
    "font-thicken",
    "gtk-opengl-debug",
    "gtk-titlebar",
    "gtk-titlebar-hide-when-maximized",
    "gtk-wide-tabs",
    "initial-window",
    "link-url",
    "linux-cgroup-hard-fail",
    "macos-applescript",
    "macos-auto-secure-input",
    "macos-secure-input-indication",
    "macos-window-shadow",
    "maximize",
    "mouse-hide-while-typing",
    "mouse-reporting",
    "palette-generate",
    "palette-harmonious",
    "progress-style",
    "quick-terminal-autohide",
    "quit-after-last-window-closed",
    "selection-clear-on-copy",
    "selection-clear-on-typing",
    "split-inherit-working-directory",
    "tab-inherit-working-directory",
    "title-report",
    "vt-kam-allowed",
    "wait-after-command",
    "window-inherit-font-size",
    "window-inherit-working-directory",
    "window-step-resize",
    "window-vsync",
}


@dataclasses.dataclass(frozen=True)
class ParserInventoryRow:
    option: str
    parser_path: str
    family: str
    status: str
    evidence: str
    missing_evidence: str


@dataclasses.dataclass(frozen=True)
class DiagnosticRow:
    option: str
    parser_path: str
    parser_family: str
    diagnostic_family: str
    status: str
    evidence: str
    missing_evidence: str


def markdown_table_cells(line: str) -> list[str]:
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def extract_canonical_options(config_inventory: Path) -> list[str]:
    lines = config_inventory.read_text().splitlines()
    options: list[str] = []
    in_section = False
    for line in lines:
        if line == "## Ghostty Canonical Options":
            in_section = True
            continue
        if in_section and line.startswith("## "):
            break
        if in_section and line.startswith("- `") and line.endswith("`"):
            options.append(line[3:-1])
    return options


def parse_parser_inventory(path: Path) -> dict[str, ParserInventoryRow]:
    rows: dict[str, ParserInventoryRow] = {}
    for line in path.read_text().splitlines():
        if not PARSER_ROW_RE.match(line):
            continue
        cells = markdown_table_cells(line)
        option = cells[1].strip("`")
        rows[option] = ParserInventoryRow(
            option=option,
            parser_path=cells[2],
            family=cells[3],
            status=cells[4],
            evidence=cells[5],
            missing_evidence=cells[6],
        )
    return rows


def diagnostic_family(row: ParserInventoryRow) -> str:
    if row.family == "unsupported":
        return "not-implemented diagnostic"
    if row.family in {"path", "font", "key binding", "custom parse_cli"}:
        return "stateful parser diagnostic"
    if row.family in {"command palette", "window padding", "packed flags"}:
        return "structured value diagnostic"
    if row.family in {"boolean", "enum", "integer scalar", "float scalar", "string"}:
        return "scalar invalid-value diagnostic"
    if row.family == "color":
        return "color invalid-value diagnostic"
    if row.family == "duration":
        return "duration invalid-value diagnostic"
    if row.family == "working directory":
        return "required-value diagnostic"
    return "custom diagnostic"


def diagnostic_override_evidence(option: str, row: ParserInventoryRow) -> str | None:
    if option not in BOOLEAN_DIAGNOSTIC_ORACLE_OPTIONS:
        return None
    if row.family != "boolean":
        raise ValueError(
            f"{option} is listed in the Experiment 86 boolean diagnostic oracle "
            f"but parser family is {row.family!r}"
        )
    return (
        "Experiment 86 shared boolean diagnostic oracle covers exact upstream "
        "true/false tokens, bare true, empty reset, config-file invalid-value "
        "diagnostics with line/key/error, CLI invalid-value diagnostics with "
        "argument position/key/error, and invalid-value state retention; "
        "`roastty/src/config/mod.rs::config_boolean_diagnostic_family_oracle`"
    )


def build_rows(
    canonical_options: list[str],
    parser_rows: dict[str, ParserInventoryRow],
) -> tuple[list[DiagnosticRow], list[str], list[str]]:
    rows: list[DiagnosticRow] = []
    missing: list[str] = []
    for option in canonical_options:
        parser_row = parser_rows.get(option)
        if parser_row is None:
            missing.append(option)
            rows.append(
                DiagnosticRow(
                    option=option,
                    parser_path="missing parser inventory row",
                    parser_family="missing",
                    diagnostic_family="missing parser diagnostic",
                    status="Gap",
                    evidence="No parser inventory row found for this canonical option",
                    missing_evidence=(
                        "Add the parser row before diagnostic coverage can be audited."
                    ),
                )
            )
            continue

        override_evidence = diagnostic_override_evidence(option, parser_row)
        has_diagnostics = (
            override_evidence is not None or "diagnostic" in parser_row.evidence.lower()
        )
        rows.append(
            DiagnosticRow(
                option=option,
                parser_path=parser_row.parser_path,
                parser_family=parser_row.family,
                diagnostic_family=diagnostic_family(parser_row),
                status="Oracle complete" if has_diagnostics else "Audit covered",
                evidence=(
                    override_evidence
                    if override_evidence is not None
                    else parser_row.evidence
                    if "diagnostic" in parser_row.evidence.lower()
                    else "Parser row identified; diagnostic-specific proof still required"
                ),
                missing_evidence=(
                    "None for invalid-value diagnostic behavior."
                    if has_diagnostics
                    else (
                        "Needs explicit ConfigDiagnostic proof for invalid values, "
                        "line/key or CLI position behavior, and state retention "
                        "where applicable."
                    )
                ),
            )
        )

    extra = sorted(set(parser_rows) - set(canonical_options))
    missing_overrides = sorted(BOOLEAN_DIAGNOSTIC_ORACLE_OPTIONS - set(canonical_options))
    if missing_overrides:
        raise ValueError(
            "Experiment 86 boolean diagnostic oracle options missing from canonical "
            f"inventory: {', '.join(missing_overrides)}"
        )
    return rows, missing, extra


def emit_inventory(rows: list[DiagnosticRow], extra: list[str], output: Path) -> None:
    family_counts: dict[str, int] = {}
    status_counts: dict[str, int] = {}
    for row in rows:
        family_counts[row.diagnostic_family] = family_counts.get(row.diagnostic_family, 0) + 1
        status_counts[row.status] = status_counts.get(row.status, 0) + 1

    lines: list[str] = [
        "# Config Diagnostic Inventory",
        "",
        "Generated by `issues/0805-roastty-ghostty-parity/config_diagnostic_inventory.py`",
        "for Issue 805 diagnostic-facet experiments.",
        "",
        "## Counts",
        "",
        "| Category | Count |",
        "| --- | ---: |",
        f"| Canonical diagnostic rows | {len(rows)} |",
        f"| Oracle complete rows | {status_counts.get('Oracle complete', 0)} |",
        f"| Audit covered rows | {status_counts.get('Audit covered', 0)} |",
        f"| Gap rows | {status_counts.get('Gap', 0)} |",
        "",
        "## Diagnostic Families",
        "",
        "| Diagnostic family | Count |",
        "| --- | ---: |",
    ]
    for family, count in sorted(family_counts.items()):
        lines.append(f"| {family} | {count} |")

    lines.extend(["", "## Extra Parser Inventory Rows", ""])
    if extra:
        lines.extend(f"- `{option}`" for option in extra)
    else:
        lines.append("- None.")

    lines.extend(
        [
            "",
            "## Rows",
            "",
            "| ID | Canonical option | Parser path/helper | Parser family | Diagnostic family | Status | Evidence | Missing evidence |",
            "| --- | --- | --- | --- | --- | --- | --- | --- |",
        ]
    )
    for index, row in enumerate(rows, 1):
        lines.append(
            f"| DIAG-{index:03d} | `{row.option}` | {row.parser_path} | "
            f"{row.parser_family} | {row.diagnostic_family} | {row.status} | "
            f"{row.evidence} | {row.missing_evidence} |"
        )
    output.write_text("\n".join(lines) + "\n")


def update_cfg219(
    matrix: Path,
    diagnostic_inventory_path: Path,
    oracle_count: int,
    incomplete_count: int,
    gap_count: int,
) -> None:
    lines = matrix.read_text().splitlines()
    updated: list[str] = []
    for line in lines:
        if line.startswith("| CFG-219 |"):
            status = "Pass" if incomplete_count == 0 else "Gap"
            notes = (
                f"Diagnostic inventory coverage: {oracle_count} rows Oracle "
                f"complete; {incomplete_count} rows are not Oracle complete and "
                f"{gap_count} rows are diagnostic gaps."
            )
            line = (
                "| CFG-219 | Invalid-value diagnostics | Ghostty reports "
                "diagnostics for invalid config values with expected line/key "
                "behavior. | Roastty diagnostic behavior is inventoried per "
                f"canonical option, but full diagnostic parity is not proven. | {status} | "
                "Generated diagnostic-facet inventory plus matrix consistency assertion. | "
                f"`{diagnostic_inventory_path}` | Tier 1 | "
                "`PYTHONDONTWRITEBYTECODE=1 python3 "
                "issues/0805-roastty-ghostty-parity/config_diagnostic_inventory.py "
                "--config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md "
                "--parser-inventory issues/0805-roastty-ghostty-parity/config-parser-inventory.md "
                "--roastty roastty/src/config/mod.rs --output "
                "issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md "
                "--matrix issues/0805-roastty-ghostty-parity/config-matrix.md` | "
                "Before closing Issue 805 and when config parser diagnostics change. | "
                "CFG-219 only passes when every diagnostic inventory row is "
                f"`Oracle complete`; audit coverage alone is insufficient. | Experiment 85 | {notes} |"
            )
        updated.append(line)
    matrix.write_text("\n".join(updated) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--config-inventory", type=Path, required=True)
    parser.add_argument("--parser-inventory", type=Path, required=True)
    parser.add_argument("--roastty", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--matrix", type=Path, required=True)
    args = parser.parse_args()

    # `--roastty` is kept as an explicit dependency so this inventory has the
    # same command shape as the parser/formatter inventories and can grow source
    # checks without changing the matrix guard command.
    args.roastty.read_text()

    canonical_options = extract_canonical_options(args.config_inventory)
    parser_rows = parse_parser_inventory(args.parser_inventory)
    rows, missing, extra = build_rows(canonical_options, parser_rows)
    emit_inventory(rows, extra, args.output)

    oracle_count = sum(row.status == "Oracle complete" for row in rows)
    incomplete_count = sum(row.status != "Oracle complete" for row in rows)
    gap_count = sum(row.status == "Gap" for row in rows)
    update_cfg219(args.matrix, args.output, oracle_count, incomplete_count, gap_count)

    print(f"ghostty_canonical={len(canonical_options)}")
    print(f"diagnostic_rows={len(rows)}")
    print(f"missing_canonical_diagnostic_rows={len(missing)}")
    print(f"extra_diagnostic_rows={len(extra)}")
    print(f"oracle_complete={oracle_count}")
    print(f"audit_covered={sum(row.status == 'Audit covered' for row in rows)}")
    print(f"gap={gap_count}")
    if missing or extra:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
