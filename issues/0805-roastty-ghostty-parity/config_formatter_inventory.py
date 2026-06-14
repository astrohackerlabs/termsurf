#!/usr/bin/env python3
"""Inventory Roastty formatter dispatch for pinned Ghostty config options.

This is a bounded source scanner for Issue 805 formatter-facet experiments. It
is not a Rust or Zig parser; it reads Roastty's `Config::format_config`
`EntryFormatter::new(...)` calls and maps them to pinned Ghostty canonical
options.
"""

from __future__ import annotations

import argparse
import dataclasses
import re
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

import config_inventory

ENTRY_FORMATTER_RE = re.compile(r'EntryFormatter::new\(\s*"(?P<key>[^"]+)"', re.DOTALL)
PRIMITIVE_ORACLE_TEST = "primitive_config_formatter_family_oracle"
OPTIONAL_SCALAR_ORACLE_TEST = "optional_scalar_config_formatter_family_oracle"
OPTIONAL_COLOR_ORACLE_TEST = "optional_color_config_formatter_family_oracle"
OPTIONAL_PATH_ORACLE_TEST = "optional_path_config_formatter_family_oracle"
OPTIONAL_COMMAND_ORACLE_TEST = "optional_command_config_formatter_family_oracle"
OPTIONAL_VALUE_ORACLE_TEST = "optional_value_config_formatter_family_oracle"
FONT_SCALAR_ORACLE_TEST = "font_scalar_config_formatter_family_oracle"
FONT_REPEATABLE_STRING_ORACLE_TEST = "font_repeatable_string_config_formatter_family_oracle"
FONT_STYLE_ORACLE_TEST = "font_style_config_formatter_family_oracle"
FONT_VARIATION_ORACLE_TEST = "font_variation_config_formatter_family_oracle"
CODEPOINT_MAP_ORACLE_TEST = "codepoint_map_config_formatter_family_oracle"
FONT_SHAPING_BREAK_ORACLE_TEST = "font_shaping_break_config_formatter_family_oracle"
KEYWORD_ENUM_ORACLE_TEST = "keyword_enum_config_formatter_family_oracle"
CLIPBOARD_ACCESS_ORACLE_TEST = "clipboard_access_config_formatter_family_oracle"
DIRECT_COLOR_ORACLE_TEST = "direct_color_config_formatter_family_oracle"
CLICK_ACTION_ORACLE_TEST = "click_action_config_formatter_family_oracle"
WINDOW_ENUM_ORACLE_TEST = "window_enum_config_formatter_family_oracle"
METRIC_MODIFIER_ORACLE_TEST = "metric_modifier_config_formatter_family_oracle"
WINDOW_PADDING_ORACLE_TEST = "window_padding_config_formatter_family_oracle"
REPEATABLE_PATH_ORACLE_TEST = "repeatable_path_config_formatter_family_oracle"
COLOR_KEYWORD_ORACLE_TEST = "color_keyword_config_formatter_family_oracle"
KEY_REMAP_ORACLE_TEST = "key_remap_config_formatter_family_oracle"
KEYBIND_ORACLE_TEST = "keybind_config_formatter_family_oracle"
LINK_NO_OUTPUT_ORACLE_TEST = "link_no_output_config_formatter_oracle"
COMMAND_PALETTE_ORACLE_TEST = "command_palette_entry_config_parse_format_reset_and_diagnose"
PRIMITIVE_FAMILIES = {"boolean", "integer", "float", "string"}
REPEATABLE_PATH_OPTIONS = {"config-file", "custom-shader", "gtk-custom-css"}
OPTIONAL_PATH_OPTIONS = {"background-image", "bell-audio-path"}
OPTIONAL_COMMAND_OPTIONS = {"command", "initial-command"}
FONT_SCALAR_OPTIONS = {
    "adjust-font-baseline",
    "font-size",
    "font-thicken",
    "font-thicken-strength",
    "window-inherit-font-size",
    "window-title-font-family",
}
FONT_REPEATABLE_STRING_OPTIONS = {
    "font-family",
    "font-family-bold",
    "font-family-italic",
    "font-family-bold-italic",
    "font-feature",
}
FONT_STYLE_OPTIONS = {
    "font-style",
    "font-style-bold",
    "font-style-italic",
    "font-style-bold-italic",
    "font-synthetic-style",
}
FONT_VARIATION_OPTIONS = {
    "font-variation",
    "font-variation-bold",
    "font-variation-italic",
    "font-variation-bold-italic",
}
CODEPOINT_MAP_OPTIONS = {
    "font-codepoint-map",
    "clipboard-codepoint-map",
}
FONT_SHAPING_BREAK_OPTIONS = {"font-shaping-break"}
KEYWORD_ENUM_OPTIONS = {
    "alpha-blending",
    "cursor-style",
    "mouse-shift-capture",
    "scrollbar",
}
CLIPBOARD_ACCESS_OPTIONS = {
    "clipboard-read",
    "clipboard-write",
}
DIRECT_COLOR_OPTIONS = {
    "background",
    "foreground",
    "search-foreground",
    "search-background",
    "search-selected-foreground",
    "search-selected-background",
}
CLICK_ACTION_OPTIONS = {
    "copy-on-select",
    "right-click-action",
    "middle-click-action",
}
WINDOW_ENUM_OPTIONS = {
    "window-theme",
    "window-save-state",
    "window-new-tab-position",
    "window-show-tab-bar",
}
OPTIONAL_COLOR_OPTIONS = {
    "bold-color",
    "cursor-color",
    "cursor-text",
    "macos-icon-ghost-color",
    "selection-background",
    "selection-foreground",
    "split-divider-color",
    "unfocused-split-fill",
    "window-titlebar-background",
    "window-titlebar-foreground",
}

NO_OUTPUT_FORMATTERS = {
    "link": (
        "Pinned Ghostty `RepeatableLink.formatEntry` intentionally emits no "
        "output because `link` cannot currently be set."
    ),
}


@dataclasses.dataclass(frozen=True)
class FormatterCall:
    key: str
    text: str
    line: int


@dataclasses.dataclass(frozen=True)
class FormatterRow:
    option: str
    formatter_path: str
    family: str
    status: str
    evidence: str
    missing_evidence: str
    source_line: int | None


def extract_format_config_body(path: Path) -> tuple[str, int]:
    text = path.read_text()
    marker = "    pub(crate) fn format_config"
    start = text.index(marker)
    end = text.index("    /// Set one config field", start)
    line = text[:start].count("\n") + 1
    return text[start:end], line


def statement_around(body: str, match_start: int) -> str:
    start = body.rfind(";", 0, match_start)
    start = 0 if start == -1 else start + 1
    end = body.find(";", match_start)
    end = len(body) if end == -1 else end + 1
    return body[start:end].strip()


def extract_formatter_calls(path: Path) -> list[FormatterCall]:
    body, base_line = extract_format_config_body(path)
    calls: list[FormatterCall] = []
    for match in ENTRY_FORMATTER_RE.finditer(body):
        calls.append(
            FormatterCall(
                key=match.group("key"),
                text=statement_around(body, match.start()),
                line=base_line + body[: match.start()].count("\n"),
            )
        )
    return calls


def formatter_path(text: str) -> str:
    checks = [
        "entry_optional",
        "entry_bool",
        "entry_int",
        "entry_float",
        "entry_str",
        "entry_void",
        ".format_entry",
        "format_metric_modifier",
    ]
    found = [check for check in checks if check in text]
    if found:
        return ", ".join(found)
    return "custom inline formatter"


def formatter_family(option: str, path_text: str, call_text: str) -> str:
    if "keybind" in call_text:
        return "key binding"
    if "key-remap" in call_text:
        return "key remap"
    if "command_palette_entry" in call_text:
        return "command palette"
    if option in REPEATABLE_PATH_OPTIONS:
        return "repeatable path"
    if option in OPTIONAL_PATH_OPTIONS:
        return "optional path"
    if option in OPTIONAL_COMMAND_OPTIONS:
        return "optional command"
    if option in OPTIONAL_COLOR_OPTIONS:
        return "optional color"
    if option in FONT_SCALAR_OPTIONS:
        return "font scalar"
    if option in FONT_REPEATABLE_STRING_OPTIONS:
        return "font repeatable string"
    if option in FONT_STYLE_OPTIONS:
        return "font style"
    if option in FONT_VARIATION_OPTIONS:
        return "font variation"
    if option in CODEPOINT_MAP_OPTIONS:
        return "codepoint map"
    if option in FONT_SHAPING_BREAK_OPTIONS:
        return "font shaping break"
    if option in KEYWORD_ENUM_OPTIONS:
        return "keyword enum"
    if option in CLIPBOARD_ACCESS_OPTIONS:
        return "clipboard access"
    if option in DIRECT_COLOR_OPTIONS:
        return "direct color"
    if option in CLICK_ACTION_OPTIONS:
        return "click action"
    if option in WINDOW_ENUM_OPTIONS:
        return "window enum"
    if "font_" in call_text or "Font" in call_text:
        return "font"
    if "window_padding" in call_text:
        return "window padding"
    if "format_metric_modifier" in call_text:
        return "metric modifier"
    if "entry_optional" in path_text and any(
        helper in path_text for helper in ("entry_bool", "entry_int", "entry_str")
    ):
        return "optional scalar"
    if "entry_optional" in path_text:
        return "optional value"
    if "entry_bool" in path_text:
        return "boolean"
    if "entry_int" in path_text:
        return "integer"
    if "entry_float" in path_text:
        return "float"
    if "entry_str" in path_text:
        return "string"
    if "entry_void" in path_text:
        return "void"
    if "Color" in call_text or "color" in call_text:
        return "color"
    if ".format_entry" in path_text:
        return "custom format_entry"
    return "custom inline"


def emit_inventory(rows: list[FormatterRow], extra: list[str], output: Path) -> None:
    family_counts: dict[str, int] = {}
    status_counts: dict[str, int] = {}
    for row in rows:
        family_counts[row.family] = family_counts.get(row.family, 0) + 1
        status_counts[row.status] = status_counts.get(row.status, 0) + 1

    lines: list[str] = [
        "# Config Formatter Inventory",
        "",
        "Generated by `issues/0805-roastty-ghostty-parity/config_formatter_inventory.py`",
        "for Issue 805 formatter-facet experiments.",
        "",
        "## Counts",
        "",
        "| Category | Count |",
        "| --- | ---: |",
        f"| Canonical formatter rows | {len(rows)} |",
        f"| Oracle complete rows | {status_counts.get('Oracle complete', 0)} |",
        f"| Audit covered rows | {status_counts.get('Audit covered', 0)} |",
        f"| Gap rows | {status_counts.get('Gap', 0)} |",
        f"| Intentional no-output rows | {sum(row.family == 'no-output' for row in rows)} |",
        "",
        "## Formatter Families",
        "",
        "| Formatter family | Count |",
        "| --- | ---: |",
    ]
    for family, count in sorted(family_counts.items()):
        lines.append(f"| {family} | {count} |")

    lines.extend(
        [
            "",
            "## Extra Roastty Formatter Entries",
            "",
        ]
    )
    if extra:
        lines.extend(f"- `{key}`" for key in extra)
    else:
        lines.append("- None.")

    lines.extend(
        [
            "",
            "## Rows",
            "",
            "| ID | Canonical option | Formatter path/helper | Formatter family | Status | Evidence | Missing evidence |",
            "| --- | --- | --- | --- | --- | --- | --- |",
        ]
    )
    for index, row in enumerate(rows, 1):
        source = (
            f"`roastty/src/config/mod.rs:{row.source_line}`"
            if row.source_line is not None
            else "canonical no-output"
        )
        lines.append(
            f"| FORMAT-{index:03d} | `{row.option}` | {row.formatter_path} | "
            f"{row.family} | {row.status} | {row.evidence}; {source} | "
            f"{row.missing_evidence} |"
        )
    output.write_text("\n".join(lines) + "\n")


def update_cfg218(
    matrix: Path,
    formatter_inventory_path: Path,
    oracle_count: int,
    incomplete_count: int,
    gap_count: int,
    owner_experiment: int,
) -> None:
    lines = matrix.read_text().splitlines()
    updated: list[str] = []
    for line in lines:
        if line.startswith("| CFG-218 |"):
            status = "Pass" if incomplete_count == 0 else "Gap"
            notes = (
                "All formatter rows are Oracle complete."
                if incomplete_count == 0
                else (
                    f"Experiment {owner_experiment} inventories formatter coverage: "
                    f"{oracle_count} rows Oracle complete; {incomplete_count} rows "
                    f"are not Oracle complete and {gap_count} rows are formatter gaps."
                )
            )
            line = (
                "| CFG-218 | Non-default formatter behavior and order | Ghostty "
                "formats configured non-default values and repeatable values with "
                "stable text and ordering. | Roastty formatter dispatch is "
                f"inventoried per canonical option, but full upstream-derived "
                f"formatter oracles are not complete. | {status} | Generated "
                "formatter-facet inventory plus matrix consistency assertion. | "
                f"`{formatter_inventory_path}` | Tier 1 | "
                "`PYTHONDONTWRITEBYTECODE=1 python3 "
                "issues/0805-roastty-ghostty-parity/config_formatter_inventory.py "
                "--upstream vendor/ghostty/src/config/Config.zig "
                "--upstream-formatter-file vendor/ghostty/src/config/formatter_file.zig "
                "--upstream-formatter vendor/ghostty/src/config/formatter.zig "
                "--roastty roastty/src/config/mod.rs --config-inventory "
                "issues/0805-roastty-ghostty-parity/config-inventory.md --output "
                "issues/0805-roastty-ghostty-parity/config-formatter-inventory.md "
                "--matrix issues/0805-roastty-ghostty-parity/config-matrix.md` | "
                "Before closing Issue 805 and when config formatter dispatch changes. | "
                "CFG-218 only passes when every formatter inventory row is "
                "`Oracle complete`; audit coverage alone is insufficient. | "
                f"Experiment {owner_experiment} | {notes} |"
            )
        updated.append(line)
    matrix.write_text("\n".join(updated) + "\n")


def build_rows(
    upstream: list[str],
    calls: list[FormatterCall],
    primitive_oracle_present: bool,
    optional_scalar_oracle_present: bool,
    optional_color_oracle_present: bool,
    optional_path_oracle_present: bool,
    optional_command_oracle_present: bool,
    optional_value_oracle_present: bool,
    font_scalar_oracle_present: bool,
    font_repeatable_string_oracle_present: bool,
    font_style_oracle_present: bool,
    font_variation_oracle_present: bool,
    codepoint_map_oracle_present: bool,
    font_shaping_break_oracle_present: bool,
    keyword_enum_oracle_present: bool,
    clipboard_access_oracle_present: bool,
    direct_color_oracle_present: bool,
    click_action_oracle_present: bool,
    window_enum_oracle_present: bool,
    metric_modifier_oracle_present: bool,
    window_padding_oracle_present: bool,
    repeatable_path_oracle_present: bool,
    color_keyword_oracle_present: bool,
    key_remap_oracle_present: bool,
    keybind_oracle_present: bool,
    link_no_output_oracle_present: bool,
    command_palette_oracle_present: bool,
) -> tuple[list[FormatterRow], list[str], list[str]]:
    call_by_key = {call.key: call for call in calls}
    canonical = set(upstream)
    rows: list[FormatterRow] = []
    missing: list[str] = []

    for option in upstream:
        if option in NO_OUTPUT_FORMATTERS:
            status = "Oracle complete" if link_no_output_oracle_present else "Audit covered"
            missing_evidence = (
                "None for canonical no-output formatter rows."
                if link_no_output_oracle_present
                else (
                    "Non-default formatter parity for this no-output canonical "
                    "option is not yet independently proven."
                )
            )
            rows.append(
                FormatterRow(
                    option=option,
                    formatter_path="canonical no-output formatter",
                    family="no-output",
                    status=status,
                    evidence=NO_OUTPUT_FORMATTERS[option],
                    missing_evidence=missing_evidence,
                    source_line=None,
                )
            )
            continue

        call = call_by_key.get(option)
        if call is None:
            rows.append(
                FormatterRow(
                    option=option,
                    formatter_path="missing formatter dispatch",
                    family="missing",
                    status="Gap",
                    evidence="No `Config::format_config` formatter entry found",
                    missing_evidence=(
                        "Add or identify the formatter dispatch before formatter "
                        "parity can be audited."
                    ),
                    source_line=None,
                )
            )
            missing.append(option)
            continue

        path_text = formatter_path(call.text)
        family = formatter_family(option, path_text, call.text)
        status = "Audit covered"
        evidence = "Formatter dispatch path identified; non-default oracle still required"
        missing_evidence = (
            "Full non-default value formatting, repeatable/empty forms where "
            "applicable, and order are not yet proven."
        )
        if primitive_oracle_present and family in PRIMITIVE_FAMILIES:
            status = "Oracle complete"
            evidence = (
                "Primitive formatter oracle covers direct boolean, integer, "
                "float, and string config rows using Ghostty-compatible "
                "true/false, decimal integer, shortest float, lowercase nan, "
                "byte-preserving string text, and representative order checks"
            )
            missing_evidence = "None for direct primitive formatter rows."
        elif optional_scalar_oracle_present and family == "optional scalar":
            status = "Oracle complete"
            evidence = (
                "Optional scalar formatter oracle covers optional None void "
                "output, optional bool output, signed and unsigned integer "
                "output, byte-preserving string output, macos option-as-alt "
                "keyword string output, raw-empty resets, and representative "
                "order checks"
            )
            missing_evidence = "None for optional scalar formatter rows."
        elif optional_color_oracle_present and family == "optional color":
            status = "Oracle complete"
            evidence = (
                "Optional color formatter oracle covers optional void output, "
                "lowercase hex output, named-color normalization, terminal color "
                "sentinel output, bright bold color output, raw-empty resets, "
                "and representative order checks"
            )
            missing_evidence = "None for optional color formatter rows."
        elif optional_path_oracle_present and family == "optional path":
            status = "Oracle complete"
            evidence = (
                "Optional path formatter oracle covers optional void output, "
                "required path output, optional `?path` output, quoted literal "
                "`?path` output, parsed-empty no-op behavior, raw-empty resets, "
                "embedded NUL path output, and representative order checks"
            )
            missing_evidence = "None for optional path formatter rows."
        elif optional_command_oracle_present and family == "optional command":
            status = "Oracle complete"
            evidence = (
                "Optional command formatter oracle covers optional void output, "
                "shell command output, explicit shell-prefix normalization, "
                "direct command output, direct empty payload output, raw-empty "
                "resets, and representative order checks"
            )
            missing_evidence = "None for optional command formatter rows."
        elif optional_value_oracle_present and family == "optional value":
            status = "Oracle complete"
            evidence = (
                "Optional value formatter oracle covers optional void output, "
                "enum keyword output, comma-joined color-list output, "
                "decomposed duration output, single-name and light/dark theme "
                "output, working-directory keyword/path output, raw-empty "
                "resets, and representative order checks"
            )
            missing_evidence = "None for optional value formatter rows."
        elif font_scalar_oracle_present and family == "font scalar":
            status = "Oracle complete"
            evidence = (
                "Font scalar formatter oracle covers optional metric modifier "
                "void, absolute, percent, and raw-empty reset output; font-size "
                "float output; font-thicken boolean output; font-thicken-strength "
                "integer output; window-inherit-font-size boolean output; "
                "window-title-font-family optional void, string, raw-empty "
                "reset, byte-preserving string output, and representative "
                "order checks"
            )
            missing_evidence = "None for font scalar formatter rows."
        elif font_repeatable_string_oracle_present and family == "font repeatable string":
            status = "Oracle complete"
            evidence = (
                "Font repeatable string formatter oracle covers empty-list "
                "void output, multiple formatted lines in insertion order, "
                "raw-empty resets, byte-preserving font-family and font-feature "
                "string output, and representative order checks"
            )
            missing_evidence = "None for font repeatable string formatter rows."
        elif font_style_oracle_present and family == "font style":
            status = "Oracle complete"
            evidence = (
                "Font style formatter oracle covers FontStyle default, false, "
                "named style, whitespace-preserving named style, raw-empty "
                "reset output, FontSyntheticStyle default all-flags output, "
                "disabled all-flags output, mixed `[no-]flag` output, "
                "raw-empty reset output, and representative order checks"
            )
            missing_evidence = "None for font style formatter rows."
        elif font_variation_oracle_present and family == "font variation":
            status = "Oracle complete"
            evidence = (
                "Font variation formatter oracle covers empty-list void output, "
                "multiple `axis=value` lines in insertion order, decimal, "
                "negative, normalized hexadecimal-float, infinity, nan output, "
                "raw-empty resets, and representative order checks"
            )
            missing_evidence = "None for font variation formatter rows."
        elif codepoint_map_oracle_present and family == "codepoint map":
            status = "Oracle complete"
            evidence = (
                "Codepoint map formatter oracle covers empty-map void output, "
                "single-codepoint and range-key output, uppercase zero-padded "
                "hex formatting, font descriptor family output, clipboard "
                "codepoint replacement output, clipboard string replacement "
                "output, empty-string replacement output, raw-empty resets, "
                "and representative order checks"
            )
            missing_evidence = "None for codepoint map formatter rows."
        elif font_shaping_break_oracle_present and family == "font shaping break":
            status = "Oracle complete"
            evidence = (
                "Font shaping break formatter oracle covers default cursor "
                "output, no-cursor output, standalone boolean parsing feeding "
                "formatter output, raw-empty reset output, and representative "
                "order checks"
            )
            missing_evidence = "None for font shaping break formatter rows."
        elif keyword_enum_oracle_present and family == "keyword enum":
            status = "Oracle complete"
            evidence = (
                "Keyword enum formatter oracle covers every keyword for "
                "alpha-blending, cursor-style, mouse-shift-capture, and "
                "scrollbar; direct enum formatter output; non-default "
                "Config::set plus format_config output; raw-empty resets; "
                "and representative order checks"
            )
            missing_evidence = "None for keyword enum formatter rows."
        elif clipboard_access_oracle_present and family == "clipboard access":
            status = "Oracle complete"
            evidence = (
                "Clipboard access formatter oracle covers allow, deny, and ask "
                "keywords; direct enum formatter output; clipboard-read and "
                "clipboard-write Config::set plus format_config output; "
                "raw-empty resets to their distinct defaults; and representative "
                "order checks"
            )
            missing_evidence = "None for clipboard access formatter rows."
        elif direct_color_oracle_present and family == "direct color":
            status = "Oracle complete"
            evidence = (
                "Direct color formatter oracle covers background and foreground "
                "Color output; search TerminalColor explicit color, "
                "cell-foreground, and cell-background output; raw-empty resets "
                "for all six direct color rows; and representative order checks"
            )
            missing_evidence = "None for direct color formatter rows."
        elif click_action_oracle_present and family == "click action":
            status = "Oracle complete"
            evidence = (
                "Click action formatter oracle covers every CopyOnSelect, "
                "RightClickAction, and MiddleClickAction keyword; direct enum "
                "formatter output; representative Config::set plus "
                "format_config output; raw-empty resets to defaults; and "
                "representative order checks"
            )
            missing_evidence = "None for click action formatter rows."
        elif window_enum_oracle_present and family == "window enum":
            status = "Oracle complete"
            evidence = (
                "Window enum formatter oracle covers every WindowTheme, "
                "WindowSaveState, WindowNewTabPosition, and WindowShowTabBar "
                "keyword; direct enum formatter output; representative "
                "Config::set plus format_config output; raw-empty resets to "
                "defaults; and representative order checks"
            )
            missing_evidence = "None for window enum formatter rows."
        elif metric_modifier_oracle_present and family == "metric modifier":
            status = "Oracle complete"
            evidence = (
                "Metric modifier formatter oracle covers non-font adjust rows, "
                "absolute decimal output, percent output as `(stored - 1) * 100%`, "
                "clamped negative percents, infinity, nan, empty optional output, "
                "and representative order checks"
            )
            missing_evidence = "None for metric modifier formatter rows."
        elif window_padding_oracle_present and family == "window padding":
            status = "Oracle complete"
            evidence = (
                "Window padding formatter oracle covers single-value and two-value "
                "padding output, every padding balance keyword, every padding color "
                "keyword, empty resets, and representative order checks"
            )
            missing_evidence = "None for window padding formatter rows."
        elif repeatable_path_oracle_present and family == "repeatable path":
            status = "Oracle complete"
            evidence = (
                "Repeatable path formatter oracle covers empty-list void output, "
                "required paths, optional paths with `?`, quoted literal `?path` "
                "values, raw-empty resets, and representative order checks"
            )
            missing_evidence = "None for repeatable path formatter rows."
        elif color_keyword_oracle_present and family == "color":
            status = "Oracle complete"
            evidence = (
                "Color keyword formatter oracle covers osc color report keywords, "
                "window colorspace keywords, empty resets, and representative order checks"
            )
            missing_evidence = "None for color keyword formatter rows."
        elif key_remap_oracle_present and family == "key remap":
            status = "Oracle complete"
            evidence = (
                "Key remap formatter oracle covers empty output, normalized direct "
                "modifier remaps, side-specific remaps, alias normalization, bare "
                "and raw-empty resets, and representative order checks"
            )
            missing_evidence = "None for key remap formatter rows."
        elif keybind_oracle_present and family == "key binding":
            status = "Oracle complete"
            evidence = (
                "Keybind formatter oracle covers empty output, default reset, "
                "direct root bindings, chained actions, root key sequences, table "
                "bindings, Ghostty-compatible cleared-table silence, slash key "
                "disambiguation, flag-prefix normalization, exact formatted lines, "
                "and representative order checks"
            )
            missing_evidence = "None for key binding formatter rows."
        elif command_palette_oracle_present and family == "command palette":
            status = "Oracle complete"
            evidence = (
                "Command palette formatter oracle covers default entries, clear "
                "void output, custom entries, quoted comma values, shorthand "
                "actions, reset behavior, diagnostics, and exact formatted output"
            )
            missing_evidence = "None for command palette formatter rows."
        rows.append(
            FormatterRow(
                option=option,
                formatter_path=f"`{path_text}`",
                family=family,
                status=status,
                evidence=evidence,
                missing_evidence=missing_evidence,
                source_line=call.line,
            )
        )

    extra = sorted(call.key for call in calls if call.key not in canonical)
    return rows, missing, extra


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--upstream", required=True, type=Path)
    parser.add_argument("--upstream-formatter-file", required=True, type=Path)
    parser.add_argument("--upstream-formatter", required=True, type=Path)
    parser.add_argument("--roastty", required=True, type=Path)
    parser.add_argument("--config-inventory", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--matrix", required=True, type=Path)
    args = parser.parse_args()

    for path in [
        args.upstream,
        args.upstream_formatter_file,
        args.upstream_formatter,
        args.roastty,
        args.config_inventory,
    ]:
        if not path.exists():
            raise FileNotFoundError(path)

    upstream, _aliases, _internal = config_inventory.extract_ghostty(args.upstream)
    calls = extract_formatter_calls(args.roastty)
    roastty_source = args.roastty.read_text()
    primitive_oracle_present = PRIMITIVE_ORACLE_TEST in roastty_source
    optional_scalar_oracle_present = OPTIONAL_SCALAR_ORACLE_TEST in roastty_source
    optional_color_oracle_present = OPTIONAL_COLOR_ORACLE_TEST in roastty_source
    optional_path_oracle_present = OPTIONAL_PATH_ORACLE_TEST in roastty_source
    optional_command_oracle_present = OPTIONAL_COMMAND_ORACLE_TEST in roastty_source
    optional_value_oracle_present = OPTIONAL_VALUE_ORACLE_TEST in roastty_source
    font_scalar_oracle_present = FONT_SCALAR_ORACLE_TEST in roastty_source
    font_repeatable_string_oracle_present = (
        FONT_REPEATABLE_STRING_ORACLE_TEST in roastty_source
    )
    font_style_oracle_present = FONT_STYLE_ORACLE_TEST in roastty_source
    font_variation_oracle_present = FONT_VARIATION_ORACLE_TEST in roastty_source
    codepoint_map_oracle_present = CODEPOINT_MAP_ORACLE_TEST in roastty_source
    font_shaping_break_oracle_present = FONT_SHAPING_BREAK_ORACLE_TEST in roastty_source
    keyword_enum_oracle_present = KEYWORD_ENUM_ORACLE_TEST in roastty_source
    clipboard_access_oracle_present = CLIPBOARD_ACCESS_ORACLE_TEST in roastty_source
    direct_color_oracle_present = DIRECT_COLOR_ORACLE_TEST in roastty_source
    click_action_oracle_present = CLICK_ACTION_ORACLE_TEST in roastty_source
    window_enum_oracle_present = WINDOW_ENUM_ORACLE_TEST in roastty_source
    metric_modifier_oracle_present = METRIC_MODIFIER_ORACLE_TEST in roastty_source
    window_padding_oracle_present = WINDOW_PADDING_ORACLE_TEST in roastty_source
    repeatable_path_oracle_present = REPEATABLE_PATH_ORACLE_TEST in roastty_source
    color_keyword_oracle_present = COLOR_KEYWORD_ORACLE_TEST in roastty_source
    key_remap_oracle_present = KEY_REMAP_ORACLE_TEST in roastty_source
    keybind_oracle_present = KEYBIND_ORACLE_TEST in roastty_source
    link_no_output_oracle_present = LINK_NO_OUTPUT_ORACLE_TEST in roastty_source
    command_palette_oracle_present = COMMAND_PALETTE_ORACLE_TEST in roastty_source
    rows, missing, extra = build_rows(
        upstream,
        calls,
        primitive_oracle_present,
        optional_scalar_oracle_present,
        optional_color_oracle_present,
        optional_path_oracle_present,
        optional_command_oracle_present,
        optional_value_oracle_present,
        font_scalar_oracle_present,
        font_repeatable_string_oracle_present,
        font_style_oracle_present,
        font_variation_oracle_present,
        codepoint_map_oracle_present,
        font_shaping_break_oracle_present,
        keyword_enum_oracle_present,
        clipboard_access_oracle_present,
        direct_color_oracle_present,
        click_action_oracle_present,
        window_enum_oracle_present,
        metric_modifier_oracle_present,
        window_padding_oracle_present,
        repeatable_path_oracle_present,
        color_keyword_oracle_present,
        key_remap_oracle_present,
        keybind_oracle_present,
        link_no_output_oracle_present,
        command_palette_oracle_present,
    )
    emit_inventory(rows, extra, args.output)

    incomplete = [row for row in rows if row.status != "Oracle complete"]
    oracle_count = sum(row.status == "Oracle complete" for row in rows)
    gap_count = sum(row.status == "Gap" for row in rows)
    owner_experiment = (
        75
        if window_enum_oracle_present
        else 74
        if click_action_oracle_present
        else 73
        if direct_color_oracle_present
        else 72
        if clipboard_access_oracle_present
        else 71
        if keyword_enum_oracle_present
        else 70
        if font_shaping_break_oracle_present
        else 69
        if codepoint_map_oracle_present
        else 68
        if font_variation_oracle_present
        else 67
        if font_style_oracle_present
        else 66
        if font_repeatable_string_oracle_present
        else 65
        if font_scalar_oracle_present
        else 64
        if optional_value_oracle_present
        else 63
        if optional_command_oracle_present
        else 62
        if optional_path_oracle_present
        else 61
        if optional_color_oracle_present
        else 60
        if optional_scalar_oracle_present
        else 59
        if keybind_oracle_present
        else 58
        if command_palette_oracle_present
        else 57
        if link_no_output_oracle_present
        else 56
        if key_remap_oracle_present
        else 55
        if color_keyword_oracle_present
        else 54
        if repeatable_path_oracle_present
        else 53
        if window_padding_oracle_present
        else 52
        if metric_modifier_oracle_present
        else 51
        if primitive_oracle_present
        else 50
    )
    update_cfg218(args.matrix, args.output, oracle_count, len(incomplete), gap_count, owner_experiment)

    print(f"ghostty_canonical={len(upstream)}")
    print(f"roastty_formatter_rows={len(rows)}")
    print(f"missing_canonical_formatter_rows={len(missing)}")
    print(f"extra_formatter_rows={len(extra)}")
    print(f"oracle_complete={oracle_count}")
    print(f"audit_covered={sum(row.status == 'Audit covered' for row in rows)}")
    print(f"gap={gap_count}")
    print(f"no_output_rows={sum(row.family == 'no-output' for row in rows)}")
    if missing or extra:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
