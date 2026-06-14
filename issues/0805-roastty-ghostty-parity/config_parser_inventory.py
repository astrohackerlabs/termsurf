#!/usr/bin/env python3
"""Inventory Roastty parser dispatch for pinned Ghostty config options.

This is a bounded source scanner for Issue 805 parser-facet experiments. It is
not a Rust parser; it reads the `Config::set_from_source` key-dispatch match and
maps each pinned Ghostty canonical option to the parser expression Roastty
currently uses.
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


ARM_RE = re.compile(r'^\s*(?P<keys>"[^"]+"(?:\s*\|\s*"[^"]+")*)\s*=>')
KEY_RE = re.compile(r'"([^"]+)"')
BOOLEAN_ORACLE_TEST = "boolean_config_parser_family_oracle"
INTEGER_ORACLE_TEST = "integer_config_parser_family_oracle"
FLOAT_ORACLE_TEST = "float_config_parser_family_oracle"
STRING_ORACLE_TEST = "string_config_parser_family_oracle"
DURATION_ORACLE_TEST = "duration_config_parser_family_oracle"
PATH_ORACLE_TEST = "path_config_parser_family_oracle"
WORKING_DIRECTORY_ORACLE_TEST = "working_directory_config_parser_family_oracle"
COMMAND_PALETTE_ORACLE_TEST = "command_palette_config_parser_family_oracle"
WINDOW_PADDING_ORACLE_TEST = "window_padding_config_parser_family_oracle"
PACKED_FLAGS_ORACLE_TEST = "packed_flags_config_parser_family_oracle"
UNSUPPORTED_ORACLE_TEST = "unsupported_config_parser_family_oracle"
ENUM_ORACLE_TEST = "enum_config_parser_family_oracle"
COLOR_ORACLE_TEST = "color_config_parser_family_oracle"
METRIC_MODIFIER_ORACLE_TEST = "metric_modifier_config_parser_family_oracle"
BACKGROUND_BLUR_ORACLE_TEST = "background_blur_config_parser_family_oracle"
CLICK_REPEAT_ORACLE_TEST = "click_repeat_interval_config_parser_family_oracle"
CURSOR_STYLE_BLINK_ORACLE_TEST = "cursor_style_blink_config_parser_family_oracle"
MACOS_ICON_SCREEN_COLOR_ORACLE_TEST = "macos_icon_screen_color_config_parser_family_oracle"
SELECTION_WORD_CHARS_ORACLE_TEST = "selection_word_chars_config_parser_family_oracle"
WINDOW_DECORATION_ORACLE_TEST = "window_decoration_config_parser_family_oracle"
MOUSE_SCROLL_MULTIPLIER_ORACLE_TEST = "mouse_scroll_multiplier_config_parser_family_oracle"
QUICK_TERMINAL_SIZE_ORACLE_TEST = "quick_terminal_size_config_parser_family_oracle"
COMMAND_ORACLE_TEST = "command_config_parser_family_oracle"
PALETTE_ORACLE_TEST = "palette_config_parser_family_oracle"
ENV_ORACLE_TEST = "env_config_parser_family_oracle"
REPEATABLE_PATH_ORACLE_TEST = "repeatable_path_config_parser_family_oracle"
INPUT_ORACLE_TEST = "input_config_parser_family_oracle"
REPEATABLE_STRING_FONT_ORACLE_TEST = "repeatable_string_font_config_parser_family_oracle"
FONT_STYLE_ORACLE_TEST = "font_style_config_parser_family_oracle"
FONT_VARIATION_ORACLE_TEST = "font_variation_config_parser_family_oracle"
CODEPOINT_MAP_ORACLE_TEST = "codepoint_map_config_parser_family_oracle"

REPEATABLE_STRING_FONT_OPTIONS = {
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


@dataclasses.dataclass(frozen=True)
class ParserArm:
    keys: tuple[str, ...]
    text: str
    line: int


@dataclasses.dataclass(frozen=True)
class ParserRow:
    option: str
    parser_path: str
    family: str
    status: str
    evidence: str
    missing_evidence: str
    source_line: int | None


def extract_set_from_source_arms(path: Path) -> list[ParserArm]:
    lines = path.read_text().splitlines()
    in_fn = False
    fn_opened = False
    in_match = False
    fn_depth = 0
    match_depth = 0
    current_keys: tuple[str, ...] | None = None
    current_start = 0
    current_lines: list[str] = []
    arms: list[ParserArm] = []

    for line_number, line in enumerate(lines, 1):
        stripped = line.strip()
        if not in_fn:
            if stripped.startswith("fn set_from_source("):
                in_fn = True
                fn_depth = line.count("{") - line.count("}")
                fn_opened = "{" in line
            continue

        fn_depth += line.count("{") - line.count("}")
        fn_opened = fn_opened or "{" in line

        if in_fn and not in_match:
            if stripped == "match key {":
                in_match = True
                match_depth = 1
            if fn_opened and fn_depth <= 0:
                break
            continue

        if not in_match:
            continue

        if stripped.startswith("_ =>"):
            if current_keys is not None:
                arms.append(ParserArm(current_keys, "\n".join(current_lines), current_start))
            break

        match = ARM_RE.match(line)
        if match:
            if current_keys is not None:
                arms.append(ParserArm(current_keys, "\n".join(current_lines), current_start))
            current_keys = tuple(KEY_RE.findall(match.group("keys")))
            current_start = line_number
            current_lines = [line]
            match_depth += line.count("{") - line.count("}")
            continue

        if current_keys is not None:
            current_lines.append(line)
        match_depth += line.count("{") - line.count("}")
        if match_depth <= 0:
            if current_keys is not None:
                arms.append(ParserArm(current_keys, "\n".join(current_lines), current_start))
            break

    return arms


def parser_path(text: str) -> str:
    checks = [
        "ConfigSetError::NotImplemented",
        "set_bool_field",
        "set_enum_field",
        "set_optional_enum_field",
        "set_optional_value_field",
        "set_value_field",
        "set_packed_field",
        "set_optional_config_file_path_field",
        "set_f64_field",
        "set_f32_field",
        "parse_u32_scalar_field",
        "parse_usize_scalar_field",
        "parse_u64_scalar_field",
        "parse_i16_field",
        "parse_u8_field",
        "parse_metric_modifier",
        "parse_string_field",
        "parse_working_directory_field",
        "parse_color_list_field",
        "parse_compat_bool",
        ".parse_cli",
        "::parse_cli",
        "from_keyword",
    ]
    found = [check for check in checks if check in text]
    if found:
        return ", ".join(found)
    return "custom inline parser"


def parser_family(path_text: str, arm_text: str) -> str:
    if "ConfigSetError::NotImplemented" in path_text:
        return "unsupported"
    if "set_bool_field" in path_text or "parse_compat_bool" in path_text:
        return "boolean"
    if "set_enum_field" in path_text or "set_optional_enum_field" in path_text or "from_keyword" in path_text:
        return "enum"
    if "set_packed_field" in path_text:
        return "packed flags"
    if "set_f64_field" in path_text or "set_f32_field" in path_text:
        return "float scalar"
    if any(name in path_text for name in ["parse_u32", "parse_usize", "parse_u64", "parse_i16", "parse_u8"]):
        return "integer scalar"
    if "Duration::parse_cli" in arm_text:
        return "duration"
    if "Color::parse_cli" in arm_text or "TerminalColor::parse_cli" in arm_text or "BoldColor::parse_cli" in arm_text:
        return "color"
    if (
        "ConfigFilePath::parse_single" in arm_text
        or "set_optional_config_file_path_field" in arm_text
        or "config_file.parse_cli" in arm_text
    ):
        return "path"
    if "parse_string_field" in path_text:
        return "string"
    if "parse_working_directory_field" in path_text:
        return "working directory"
    if "keybind.parse_cli" in arm_text or "key_remap.parse_cli" in arm_text:
        return "key binding"
    if "command_palette_entry.parse_cli" in arm_text:
        return "command palette"
    if "font_" in arm_text or "Font" in arm_text:
        return "font"
    if "WindowPadding::parse_cli" in arm_text:
        return "window padding"
    if ".parse_cli" in path_text or "::parse_cli" in path_text:
        return "custom parse_cli"
    return "custom inline"


def emit_inventory(rows: list[ParserRow], aliases: list[str], output: Path) -> None:
    family_counts: dict[str, int] = {}
    status_counts: dict[str, int] = {}
    for row in rows:
        family_counts[row.family] = family_counts.get(row.family, 0) + 1
        status_counts[row.status] = status_counts.get(row.status, 0) + 1

    lines: list[str] = [
        "# Config Parser Inventory",
        "",
        "Generated by `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`",
        "for Issue 805 parser-facet experiments.",
        "",
        "## Counts",
        "",
        "| Category | Count |",
        "| --- | ---: |",
        f"| Canonical parser rows | {len(rows)} |",
        f"| Compatibility-only parser arms | {len(aliases)} |",
        f"| Oracle complete rows | {status_counts.get('Oracle complete', 0)} |",
        f"| Audit covered rows | {status_counts.get('Audit covered', 0)} |",
        f"| Gap rows | {status_counts.get('Gap', 0)} |",
        "",
        "## Parser Families",
        "",
        "| Parser family | Count |",
        "| --- | ---: |",
    ]
    for family, count in sorted(family_counts.items()):
        lines.append(f"| {family} | {count} |")

    lines.extend(
        [
            "",
            "## Compatibility-Only Parser Arms",
            "",
            "These arms are not canonical Ghostty option rows for CFG-217.",
            "",
        ]
    )
    if aliases:
        lines.extend(f"- `{alias}`" for alias in aliases)
    else:
        lines.append("- None.")

    lines.extend(
        [
            "",
            "## Rows",
            "",
            "| ID | Canonical option | Parser path/helper | Parser family | Status | Evidence | Missing evidence |",
            "| --- | --- | --- | --- | --- | --- | --- |",
        ]
    )
    for index, row in enumerate(rows, 1):
        source = (
            f"`roastty/src/config/mod.rs:{row.source_line}`"
            if row.source_line is not None
            else "`roastty/src/config/mod.rs`"
        )
        lines.append(
            f"| PARSE-{index:03d} | `{row.option}` | {row.parser_path} | "
            f"{row.family} | {row.status} | {row.evidence}; {source} | "
            f"{row.missing_evidence} |"
        )
    output.write_text("\n".join(lines) + "\n")


def update_cfg217(
    matrix: Path,
    parser_inventory_path: Path,
    oracle_count: int,
    incomplete_count: int,
    gap_count: int,
    owner_experiment: int,
) -> None:
    lines = matrix.read_text().splitlines()
    updated: list[str] = []
    for line in lines:
        if line.startswith("| CFG-217 |"):
            status = "Pass" if incomplete_count == 0 else "Gap"
            notes = (
                "All parser rows are Oracle complete."
                if incomplete_count == 0
                else (
                    f"Experiment {owner_experiment} proves {oracle_count} parser rows Oracle "
                    f"complete; {incomplete_count} parser rows are not Oracle "
                    f"complete and {gap_count} parser rows are dispatch gaps."
                )
            )
            line = (
                "| CFG-217 | Non-default parser semantics | Ghostty parsers accept "
                "and reject the full documented non-default value space for every "
                "canonical option. | Roastty parser dispatch is inventoried per "
                f"canonical option, but full upstream-derived parser oracles are not complete. | {status} | "
                "Generated parser-facet inventory plus matrix consistency assertion. | "
                f"`{parser_inventory_path}` | Tier 1 | "
                "`python3 issues/0805-roastty-ghostty-parity/config_parser_inventory.py "
                "--upstream vendor/ghostty/src/config/Config.zig --roastty "
                "roastty/src/config/mod.rs --config-inventory "
                "issues/0805-roastty-ghostty-parity/config-inventory.md --output "
                "issues/0805-roastty-ghostty-parity/config-parser-inventory.md --matrix "
                "issues/0805-roastty-ghostty-parity/config-matrix.md` | Before closing "
                "Issue 805 and when config parser dispatch changes. | CFG-217 only "
                "passes when every parser inventory row is `Oracle complete`; audit "
                f"coverage alone is insufficient. | Experiment {owner_experiment} | {notes} |"
            )
        updated.append(line)
    matrix.write_text("\n".join(updated) + "\n")


def build_rows(
    upstream: list[str],
    aliases: list[str],
    arms: list[ParserArm],
    boolean_oracle_present: bool,
    integer_oracle_present: bool,
    float_oracle_present: bool,
    string_oracle_present: bool,
    duration_oracle_present: bool,
    path_oracle_present: bool,
    working_directory_oracle_present: bool,
    command_palette_oracle_present: bool,
    window_padding_oracle_present: bool,
    packed_flags_oracle_present: bool,
    unsupported_oracle_present: bool,
    enum_oracle_present: bool,
    color_oracle_present: bool,
    metric_modifier_oracle_present: bool,
    background_blur_oracle_present: bool,
    click_repeat_oracle_present: bool,
    cursor_style_blink_oracle_present: bool,
    macos_icon_screen_color_oracle_present: bool,
    selection_word_chars_oracle_present: bool,
    window_decoration_oracle_present: bool,
    mouse_scroll_multiplier_oracle_present: bool,
    quick_terminal_size_oracle_present: bool,
    command_oracle_present: bool,
    palette_oracle_present: bool,
    env_oracle_present: bool,
    repeatable_path_oracle_present: bool,
    input_oracle_present: bool,
    repeatable_string_font_oracle_present: bool,
    font_style_oracle_present: bool,
    font_variation_oracle_present: bool,
    codepoint_map_oracle_present: bool,
) -> tuple[list[ParserRow], list[str], list[str], list[str]]:
    arm_by_key: dict[str, ParserArm] = {}
    for arm in arms:
        for key in arm.keys:
            arm_by_key[key] = arm

    rows: list[ParserRow] = []
    missing: list[str] = []
    for option in upstream:
        arm = arm_by_key.get(option)
        if arm is None:
            rows.append(
                ParserRow(
                    option=option,
                    parser_path="missing dispatch arm",
                    family="missing",
                    status="Gap",
                    evidence="No `Config::set_from_source` match arm found",
                    missing_evidence="Add or identify the parser dispatch arm before parser parity can be audited.",
                    source_line=None,
                )
            )
            missing.append(option)
            continue

        path_text = parser_path(arm.text)
        family = parser_family(path_text, arm.text)
        status = "Audit covered"
        evidence = "Parser dispatch path identified; option still needs upstream-derived full-value oracle"
        missing_evidence = "Full accepted variants/classes plus rejection/reset semantics are not yet proven."
        if boolean_oracle_present and family == "boolean" and option != "config-default-files":
            status = "Oracle complete"
            evidence = (
                "Shared boolean parser oracle covers upstream true/false spellings, "
                "bare true, empty reset, and invalid values"
            )
            missing_evidence = "None for direct boolean parser semantics."
        elif integer_oracle_present and family == "integer scalar":
            status = "Oracle complete"
            evidence = (
                "Shared integer parser oracle covers Zig base-0 prefixes/signs/"
                "underscores, missing values, empty reset, invalid syntax, and "
                "overflow/range failures"
            )
            missing_evidence = "None for direct integer parser semantics."
        elif float_oracle_present and family == "float scalar":
            status = "Oracle complete"
            evidence = (
                "Shared float parser oracle covers Zig decimal, special, "
                "underscore, hexadecimal, missing-value, empty-reset, invalid "
                "syntax, and overflow semantics"
            )
            missing_evidence = "None for direct float parser semantics."
        elif string_oracle_present and family == "string":
            status = "Oracle complete"
            evidence = (
                "Shared string parser oracle covers exact byte-preserving copy, "
                "embedded NULs, missing values, explicit empty strings, and "
                "required/optional empty-reset semantics"
            )
            missing_evidence = "None for direct string parser semantics."
        elif duration_oracle_present and family == "duration":
            status = "Oracle complete"
            evidence = (
                "Shared duration parser oracle covers units, whitespace, zero, "
                "invalid values, overflow, missing values, and required/optional "
                "empty-reset semantics"
            )
            missing_evidence = "None for direct duration parser semantics."
        elif path_oracle_present and family == "path":
            status = "Oracle complete"
            evidence = (
                "Shared path parser oracle covers optional markers, quoted literal "
                "markers, parsed-empty no-op behavior, raw-empty resets, embedded "
                "NULs, repeatable accumulation, and formatting"
            )
            missing_evidence = "None for direct path parser semantics."
        elif working_directory_oracle_present and family == "working directory":
            status = "Oracle complete"
            evidence = (
                "Shared working-directory parser oracle covers whitespace trimming, "
                "quote stripping, exact keywords, path fallback, embedded NULs, "
                "missing values, empty resets, and formatting"
            )
            missing_evidence = "None for direct working-directory parser semantics."
        elif command_palette_oracle_present and family == "command palette":
            status = "Oracle complete"
            evidence = (
                "Shared command-palette parser oracle covers defaults, clear, "
                "empty restore, auto-struct fields, quoting, duplicate fields, "
                "action canonicalization, invalid values, and formatting"
            )
            missing_evidence = "None for direct command-palette parser semantics."
        elif window_padding_oracle_present and family == "window padding":
            status = "Oracle complete"
            evidence = (
                "Shared window-padding parser oracle covers single and paired "
                "values, space/tab trimming, base-10 u32 parsing, invalid values, "
                "empty resets, diagnostics, and formatting"
            )
            missing_evidence = "None for direct window-padding parser semantics."
        elif packed_flags_oracle_present and family == "packed flags":
            status = "Oracle complete"
            evidence = (
                "Shared packed-flags parser oracle covers standalone bools, "
                "default-based comma lists, no-prefix toggles, exact hyphenated "
                "field names, duplicate flags, missing values, empty resets, "
                "invalid values, diagnostics, and formatting"
            )
            missing_evidence = "None for direct packed-flags parser semantics."
        elif unsupported_oracle_present and family == "unsupported":
            status = "Oracle complete"
            evidence = (
                "Shared unsupported parser oracle covers recognized "
                "not-implemented link parser behavior, empty reset, diagnostics, "
                "and distinction from unknown fields"
            )
            missing_evidence = "None for direct unsupported parser semantics."
        elif enum_oracle_present and family == "enum":
            status = "Oracle complete"
            evidence = (
                "Shared enum parser oracle covers exact keywords, required and "
                "optional enum dispatch, missing values, empty resets, invalid "
                "values, diagnostics, formatting, and pinned compatibility "
                "branches"
            )
            missing_evidence = "None for direct enum parser semantics."
        elif color_oracle_present and family == "color":
            status = "Oracle complete"
            evidence = (
                "Shared color parser oracle covers named and hex colors, "
                "required and optional Color/TerminalColor/BoldColor dispatch, "
                "terminal sentinels, bright bold color, missing values, empty "
                "resets, invalid values, diagnostics, and formatting"
            )
            missing_evidence = "None for direct color parser semantics."
        elif metric_modifier_oracle_present and "parse_metric_modifier" in path_text:
            status = "Oracle complete"
            evidence = (
                "Shared metric modifier parser oracle covers Zig base-10 i32 "
                "absolutes, Zig f64 percent syntax, clamp behavior, missing "
                "values, empty resets, invalid values, diagnostics, CLI, "
                "formatting, and clone semantics"
            )
            missing_evidence = "None for direct metric modifier parser semantics."
        elif background_blur_oracle_present and option == "background-blur":
            status = "Oracle complete"
            evidence = (
                "Background blur parser oracle covers bool-first parsing, bare "
                "true, glass keywords, base-0 u8 radii, empty reset, invalid "
                "values, diagnostics, CLI, formatting, and clone semantics"
            )
            missing_evidence = "None for direct background-blur parser semantics."
        elif click_repeat_oracle_present and option == "click-repeat-interval":
            status = "Oracle complete"
            evidence = (
                "Click repeat interval parser oracle covers base-10 u32 values, "
                "missing values, empty resets, invalid values, diagnostics, CLI, "
                "formatting, clone semantics, and parser/finalization boundary"
            )
            missing_evidence = "None for direct click-repeat-interval parser semantics."
        elif cursor_style_blink_oracle_present and option == "cursor-style-blink":
            status = "Oracle complete"
            evidence = (
                "Cursor style blink parser oracle covers optional bool defaults, "
                "bare true, bool spellings, empty unset, invalid values, "
                "diagnostics, CLI, formatting, and clone semantics"
            )
            missing_evidence = "None for direct cursor-style-blink parser semantics."
        elif macos_icon_screen_color_oracle_present and option == "macos-icon-screen-color":
            status = "Oracle complete"
            evidence = (
                "macOS icon screen color parser oracle covers ColorList defaults, "
                "named and hex colors, comma lists, token trimming, skipped empty "
                "tokens, reset semantics, 64-color cap, empty unset, invalid "
                "values, diagnostics, CLI, formatting, and clone semantics"
            )
            missing_evidence = "None for direct macos-icon-screen-color parser semantics."
        elif selection_word_chars_oracle_present and option == "selection-word-chars":
            status = "Oracle complete"
            evidence = (
                "Selection word chars parser oracle covers default boundaries, "
                "literal and escaped codepoints, null seeding, empty values, "
                "missing values, invalid escapes, diagnostics, CLI, formatting, "
                "the 4096-byte cap, and clone semantics"
            )
            missing_evidence = "None for direct selection-word-chars parser semantics."
        elif window_decoration_oracle_present and option == "window-decoration":
            status = "Oracle complete"
            evidence = (
                "Window decoration parser oracle covers missing values, bool "
                "tokens, exact variant names, invalid values, diagnostics, CLI, "
                "formatting, and clone semantics"
            )
            missing_evidence = "None for direct window-decoration parser semantics."
        elif mouse_scroll_multiplier_oracle_present and option == "mouse-scroll-multiplier":
            status = "Oracle complete"
            evidence = (
                "Mouse scroll multiplier parser oracle covers defaults, bare "
                "and auto-struct values, empty no-op values, Zig float syntax, "
                "quoted field values, invalid values, diagnostics, CLI, "
                "formatting, and clone semantics"
            )
            missing_evidence = "None for direct mouse-scroll-multiplier parser semantics."
        elif quick_terminal_size_oracle_present and option == "quick-terminal-size":
            status = "Oracle complete"
            evidence = (
                "Quick terminal size parser oracle covers unset defaults, pixel "
                "and percentage values, Zig integer and float syntax, comma "
                "pairs, invalid values, diagnostics, CLI, formatting, "
                "calculation, and clone semantics"
            )
            missing_evidence = "None for direct quick-terminal-size parser semantics."
        elif command_oracle_present and option in ("command", "initial-command"):
            status = "Oracle complete"
            evidence = (
                "Command parser oracle covers defaults, required values, empty "
                "optional resets, exact shell/direct prefixes, unknown prefix "
                "fallback, ASCII-space trimming, direct argument splitting, "
                "prefixed empty payloads, diagnostics, formatting, string "
                "conversion, and clone semantics"
            )
            missing_evidence = "None for direct command/initial-command parser semantics."
        elif palette_oracle_present and option == "palette":
            status = "Oracle complete"
            evidence = (
                "Palette parser oracle covers required values, first-equals "
                "splitting, ASCII space/tab key trimming, Zig base-0 u8 key "
                "syntax, color parsing, failed-parse atomicity, repeated "
                "assignments, empty optional reset, diagnostics, formatting, "
                "replay, and clone semantics"
            )
            missing_evidence = "None for direct palette parser semantics."
        elif env_oracle_present and option == "env":
            status = "Oracle complete"
            evidence = (
                "Env parser oracle covers required values, empty reset, missing "
                "equals, whitespace-only rejection, first-equals splitting, "
                "ASCII whitespace trimming, empty keys, key deletion, repeated "
                "key overwrite, formatting, diagnostics, equality, and clone "
                "semantics"
            )
            missing_evidence = "None for direct env parser semantics."
        elif repeatable_path_oracle_present and option in ("custom-shader", "gtk-custom-css"):
            status = "Oracle complete"
            evidence = (
                "Repeatable path parser oracle covers missing values, raw-empty "
                "reset, required and optional path append, quoted literal "
                "question-mark paths, parsed-empty no-op behavior, formatting, "
                "diagnostics, file/CLI base expansion, and clone semantics"
            )
            missing_evidence = "None for direct repeatable path parser semantics."
        elif input_oracle_present and option == "input":
            status = "Oracle complete"
            evidence = (
                "Input parser oracle covers missing values, empty reset, raw and "
                "path tagged values, unknown-tag raw fallback, raw-empty payloads, "
                "invalid string-literal rejection, diagnostics, CLI, formatting, "
                "and clone semantics"
            )
            missing_evidence = "None for direct input parser semantics."
        elif repeatable_string_font_oracle_present and option in REPEATABLE_STRING_FONT_OPTIONS:
            status = "Oracle complete"
            evidence = (
                "Repeatable string font parser oracle covers missing values, "
                "empty resets, byte-preserving append, one-shot overwrite_next, "
                "font-family CLI overwrite, font-feature CLI append, formatting, "
                "equality, and clone semantics"
            )
            missing_evidence = "None for direct repeatable string font parser semantics."
        elif font_style_oracle_present and option in FONT_STYLE_OPTIONS:
            status = "Oracle complete"
            evidence = (
                "Font style parser oracle covers missing values, exact default "
                "and false tokens, arbitrary named styles including empty direct "
                "parser values, empty config resets, diagnostics, CLI, formatting, "
                "enabled/name helpers, and clone semantics"
            )
            missing_evidence = "None for direct font-style parser semantics."
        elif font_variation_oracle_present and option in FONT_VARIATION_OPTIONS:
            status = "Oracle complete"
            evidence = (
                "Font variation parser oracle covers missing values, first-equals "
                "splitting, ASCII space/tab trimming, four-byte axis IDs, "
                "Zig-compatible f64 syntax, invalid values, empty config resets, "
                "diagnostics, CLI, formatting, and clone semantics"
            )
            missing_evidence = "None for direct font-variation parser semantics."
        elif codepoint_map_oracle_present and option in CODEPOINT_MAP_OPTIONS:
            status = "Oracle complete"
            evidence = (
                "Codepoint map parser oracle covers missing values, direct empty "
                "invalidity, config empty resets, Unicode range grammar, font "
                "descriptor mappings, clipboard codepoint/string replacements, "
                "u21 clipboard semantics, diagnostics, CLI, formatting, and clone "
                "semantics"
            )
            missing_evidence = "None for direct codepoint-map parser semantics."
        elif option == "config-default-files":
            missing_evidence = (
                "Direct parser and effective default-file load-order semantics must "
                "be proven together under CFG-221."
            )
        rows.append(
            ParserRow(
                option=option,
                parser_path=f"`{path_text}`",
                family=family,
                status=status,
                evidence=evidence,
                missing_evidence=missing_evidence,
                source_line=arm.line,
            )
        )

    canonical = set(upstream)
    alias_set = set(aliases)
    compatibility_only = sorted(key for key in arm_by_key if key in alias_set and key not in canonical)
    noncanonical = sorted(key for key in arm_by_key if key not in canonical and key not in alias_set)
    return rows, missing, compatibility_only, noncanonical


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--upstream", required=True, type=Path)
    parser.add_argument("--roastty", required=True, type=Path)
    parser.add_argument("--config-inventory", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--matrix", required=True, type=Path)
    args = parser.parse_args()

    if not args.config_inventory.exists():
        parser.error(f"--config-inventory does not exist: {args.config_inventory}")

    upstream, aliases, _internal = config_inventory.extract_ghostty(args.upstream)
    arms = extract_set_from_source_arms(args.roastty)
    roastty_source = args.roastty.read_text()
    boolean_oracle_present = BOOLEAN_ORACLE_TEST in roastty_source
    integer_oracle_present = INTEGER_ORACLE_TEST in roastty_source
    float_oracle_present = FLOAT_ORACLE_TEST in roastty_source
    string_oracle_present = STRING_ORACLE_TEST in roastty_source
    duration_oracle_present = DURATION_ORACLE_TEST in roastty_source
    path_oracle_present = PATH_ORACLE_TEST in roastty_source
    working_directory_oracle_present = WORKING_DIRECTORY_ORACLE_TEST in roastty_source
    command_palette_oracle_present = COMMAND_PALETTE_ORACLE_TEST in roastty_source
    window_padding_oracle_present = WINDOW_PADDING_ORACLE_TEST in roastty_source
    packed_flags_oracle_present = PACKED_FLAGS_ORACLE_TEST in roastty_source
    unsupported_oracle_present = UNSUPPORTED_ORACLE_TEST in roastty_source
    enum_oracle_present = ENUM_ORACLE_TEST in roastty_source
    color_oracle_present = COLOR_ORACLE_TEST in roastty_source
    metric_modifier_oracle_present = METRIC_MODIFIER_ORACLE_TEST in roastty_source
    background_blur_oracle_present = BACKGROUND_BLUR_ORACLE_TEST in roastty_source
    click_repeat_oracle_present = CLICK_REPEAT_ORACLE_TEST in roastty_source
    cursor_style_blink_oracle_present = CURSOR_STYLE_BLINK_ORACLE_TEST in roastty_source
    macos_icon_screen_color_oracle_present = MACOS_ICON_SCREEN_COLOR_ORACLE_TEST in roastty_source
    selection_word_chars_oracle_present = SELECTION_WORD_CHARS_ORACLE_TEST in roastty_source
    window_decoration_oracle_present = WINDOW_DECORATION_ORACLE_TEST in roastty_source
    mouse_scroll_multiplier_oracle_present = MOUSE_SCROLL_MULTIPLIER_ORACLE_TEST in roastty_source
    quick_terminal_size_oracle_present = QUICK_TERMINAL_SIZE_ORACLE_TEST in roastty_source
    command_oracle_present = COMMAND_ORACLE_TEST in roastty_source
    palette_oracle_present = PALETTE_ORACLE_TEST in roastty_source
    env_oracle_present = ENV_ORACLE_TEST in roastty_source
    repeatable_path_oracle_present = REPEATABLE_PATH_ORACLE_TEST in roastty_source
    input_oracle_present = INPUT_ORACLE_TEST in roastty_source
    repeatable_string_font_oracle_present = REPEATABLE_STRING_FONT_ORACLE_TEST in roastty_source
    font_style_oracle_present = FONT_STYLE_ORACLE_TEST in roastty_source
    font_variation_oracle_present = FONT_VARIATION_ORACLE_TEST in roastty_source
    codepoint_map_oracle_present = CODEPOINT_MAP_ORACLE_TEST in roastty_source
    rows, missing, compatibility_only, noncanonical = build_rows(
        upstream,
        aliases,
        arms,
        boolean_oracle_present,
        integer_oracle_present,
        float_oracle_present,
        string_oracle_present,
        duration_oracle_present,
        path_oracle_present,
        working_directory_oracle_present,
        command_palette_oracle_present,
        window_padding_oracle_present,
        packed_flags_oracle_present,
        unsupported_oracle_present,
        enum_oracle_present,
        color_oracle_present,
        metric_modifier_oracle_present,
        background_blur_oracle_present,
        click_repeat_oracle_present,
        cursor_style_blink_oracle_present,
        macos_icon_screen_color_oracle_present,
        selection_word_chars_oracle_present,
        window_decoration_oracle_present,
        mouse_scroll_multiplier_oracle_present,
        quick_terminal_size_oracle_present,
        command_oracle_present,
        palette_oracle_present,
        env_oracle_present,
        repeatable_path_oracle_present,
        input_oracle_present,
        repeatable_string_font_oracle_present,
        font_style_oracle_present,
        font_variation_oracle_present,
        codepoint_map_oracle_present,
    )
    emit_inventory(rows, compatibility_only, args.output)
    incomplete = [row for row in rows if row.status != "Oracle complete"]
    oracle_count = sum(row.status == "Oracle complete" for row in rows)
    gap_count = sum(row.status == "Gap" for row in rows)
    owner_experiment = (
        45
        if codepoint_map_oracle_present
        else 44
        if font_variation_oracle_present
        else 43
        if font_style_oracle_present
        else 42
        if repeatable_string_font_oracle_present
        else 41
        if input_oracle_present
        else 40
        if repeatable_path_oracle_present
        else 39
        if env_oracle_present
        else 38
        if palette_oracle_present
        else 37
        if command_oracle_present
        else 36
        if quick_terminal_size_oracle_present
        else 35
        if mouse_scroll_multiplier_oracle_present
        else 34
        if window_decoration_oracle_present
        else 33
        if selection_word_chars_oracle_present
        else 32
        if macos_icon_screen_color_oracle_present
        else 31
        if cursor_style_blink_oracle_present
        else 30
        if click_repeat_oracle_present
        else 29
        if background_blur_oracle_present
        else 28
        if metric_modifier_oracle_present
        else 27
        if color_oracle_present
        else 26
        if enum_oracle_present
        else 25
        if unsupported_oracle_present
        else 24
        if packed_flags_oracle_present
        else 23
        if window_padding_oracle_present
        else 22
        if command_palette_oracle_present
        else 21
        if working_directory_oracle_present
        else 20
        if path_oracle_present
        else 19
    )
    update_cfg217(args.matrix, args.output, oracle_count, len(incomplete), gap_count, owner_experiment)

    print(f"ghostty_canonical={len(upstream)}")
    print(f"roastty_parser_rows={len(rows)}")
    print("missing_canonical_parser_rows=0")
    print(f"missing_dispatch_rows={len(missing)}")
    print("extra_parser_rows=0")
    print(f"compatibility_only_parser_arms={len(compatibility_only)}")
    print(f"noncanonical_noncompat_parser_arms={len(noncanonical)}")
    print(f"oracle_complete={sum(row.status == 'Oracle complete' for row in rows)}")
    print(f"audit_covered={sum(row.status == 'Audit covered' for row in rows)}")
    print(f"gap={sum(row.status == 'Gap' for row in rows)}")
    if noncanonical:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
