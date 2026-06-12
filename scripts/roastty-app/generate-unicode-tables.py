#!/usr/bin/env python3
"""Generate Roastty Unicode tables from the pinned Ghostty checkout."""

from __future__ import annotations

import argparse
import difflib
import re
import os
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
PROPS_OUT = ROOT / "roastty/src/unicode/tables.rs"
GRAPHEME_OUT = ROOT / "roastty/src/unicode/grapheme_table.rs"

GB_NAMES = [
    "other",
    "prepend",
    "regional_indicator",
    "spacing_mark",
    "l",
    "v",
    "t",
    "lv",
    "lvt",
    "zwj",
    "zwnj",
    "extended_pictographic",
    "emoji_modifier_base",
    "emoji_modifier",
    "indic_conjunct_break_extend",
    "indic_conjunct_break_linker",
    "indic_conjunct_break_consonant",
]

STATE_NAMES = [
    "default",
    "regional_indicator",
    "extended_pictographic",
    "indic_conjunct_break_consonant",
    "indic_conjunct_break_linker",
]

GB_RUST = {
    "other": "Other",
    "prepend": "Prepend",
    "regional_indicator": "RegionalIndicator",
    "spacing_mark": "SpacingMark",
    "l": "L",
    "v": "V",
    "t": "T",
    "lv": "Lv",
    "lvt": "Lvt",
    "zwj": "Zwj",
    "zwnj": "Zwnj",
    "extended_pictographic": "ExtendedPictographic",
    "emoji_modifier_base": "EmojiModifierBase",
    "emoji_modifier": "EmojiModifier",
    "indic_conjunct_break_extend": "IndicConjunctBreakExtend",
    "indic_conjunct_break_linker": "IndicConjunctBreakLinker",
    "indic_conjunct_break_consonant": "IndicConjunctBreakConsonant",
}


@dataclass(frozen=True)
class Props:
    width: int
    width_zero_in_grapheme: bool
    grapheme_break: str
    emoji_vs_base: bool


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--generate", action="store_true")
    mode.add_argument("--check", action="store_true")
    args = parser.parse_args()

    outputs = render_outputs()

    if args.generate:
        for path, text in outputs:
            path.write_text(text)
        return 0

    ok = True
    for path, expected in outputs:
        actual = path.read_text() if path.exists() else ""
        if actual != expected:
            ok = False
            print(f"{path} is out of date", file=sys.stderr)
            for line in difflib.unified_diff(
                actual.splitlines(),
                expected.splitlines(),
                fromfile=str(path),
                tofile=f"{path} (generated)",
                lineterm="",
                n=3,
            ):
                print(line, file=sys.stderr)
    return 0 if ok else 1


def render_outputs() -> list[tuple[Path, str]]:
    rendered = [
        (PROPS_OUT, render_props()),
        (GRAPHEME_OUT, render_grapheme()),
    ]
    with tempfile.TemporaryDirectory() as temp_dir:
        formatted = []
        temp_root = Path(temp_dir)
        for path, text in rendered:
            temp_path = temp_root / path.name
            temp_path.write_text(text)
            rustfmt(temp_path)
            formatted.append((path, temp_path.read_text()))
        return formatted


def rustfmt(path: Path) -> None:
    try:
        subprocess.run(["rustfmt", str(path)], check=True)
    except FileNotFoundError as error:
        raise SystemExit("rustfmt is required to generate Unicode tables") from error


def find_props_zig() -> Path:
    candidates = sorted(
        ROOT.glob("vendor/ghostty/.zig-cache/o/*/props.zig"),
        key=lambda p: p.stat().st_mtime,
        reverse=True,
    )
    for path in candidates:
        text = path.read_text()
        if "pub const stage1" in text and "pub const stage3" in text:
            return path
    raise SystemExit(
        "could not find generated Ghostty props.zig; build Ghostty or run its Unicode generator first"
    )


def parse_array(text: str, name: str) -> list[int]:
    match = re.search(rf"pub const {name}: \[(\d+)\]u16 = \.\{{(.*?)\}};", text, re.S)
    if not match:
        raise SystemExit(f"could not parse {name}")
    values = [int(v) for v in re.findall(r"\d+", match.group(2))]
    expected = int(match.group(1))
    if len(values) != expected:
        raise SystemExit(f"{name} length mismatch: parsed {len(values)}, expected {expected}")
    return values


def parse_stage3(text: str) -> list[Props]:
    match = re.search(r"pub const stage3: \[(\d+)\]Elem = \.\{(.*?)\};", text, re.S)
    if not match:
        raise SystemExit("could not parse stage3")
    body = match.group(2)
    entries = []
    for entry in re.finditer(r"\.\{\s*\.width=\s*(\d+),\s*\.width_zero_in_grapheme=\s*(true|false),\s*\.grapheme_break=\s*\.([a-z_]+),\s*\.emoji_vs_base=\s*(true|false),\s*\}", body, re.S):
        gb = entry.group(3)
        if gb not in GB_RUST:
            raise SystemExit(f"unknown grapheme break property {gb}")
        entries.append(
            Props(
                width=int(entry.group(1)),
                width_zero_in_grapheme=entry.group(2) == "true",
                grapheme_break=gb,
                emoji_vs_base=entry.group(4) == "true",
            )
        )
    expected = int(match.group(1))
    if len(entries) != expected:
        raise SystemExit(f"stage3 length mismatch: parsed {len(entries)}, expected {expected}")
    return entries


def render_props() -> str:
    source = find_props_zig()
    text = source.read_text()
    stage1 = parse_array(text, "stage1")
    stage2 = parse_array(text, "stage2")
    stage3 = parse_stage3(text)
    lines = [
        "// This file is auto-generated by scripts/roastty-app/generate-unicode-tables.py.",
        "// Do not edit by hand.",
        "",
        "use super::{GraphemeBreak, Properties};",
        "",
        f"pub(crate) const STAGE1: [u16; {len(stage1)}] = [",
    ]
    lines.extend(format_int_array(stage1))
    lines += [
        "];",
        "",
        f"pub(crate) const STAGE2: [u16; {len(stage2)}] = [",
    ]
    lines.extend(format_int_array(stage2))
    lines += [
        "];",
        "",
        f"pub(crate) const STAGE3: [Properties; {len(stage3)}] = [",
    ]
    for prop in stage3:
        lines.extend(
            [
                "    Properties {",
                f"        width: {prop.width},",
                f"        width_zero_in_grapheme: {str(prop.width_zero_in_grapheme).lower()},",
                f"        grapheme_break: GraphemeBreak::{GB_RUST[prop.grapheme_break]},",
                f"        emoji_vs_base: {str(prop.emoji_vs_base).lower()},",
                "    },",
            ]
        )
    lines += [
        "];",
        "",
        f"pub(crate) const WIDTH_STAGE3: [u8; {len(stage3)}] = [",
    ]
    lines.extend(format_int_array([prop.width for prop in stage3]))
    lines += ["];"]
    return "\n".join(lines) + "\n"


def format_int_array(values: list[int]) -> list[str]:
    lines = []
    for i in range(0, len(values), 16):
        chunk = ", ".join(str(v) for v in values[i : i + 16])
        lines.append(f"    {chunk},")
    return lines


def render_grapheme() -> str:
    values = render_ghostty_grapheme_values()
    lines = [
        "// This file is auto-generated by scripts/roastty-app/generate-unicode-tables.py.",
        "// Do not edit by hand.",
        "",
        f"pub(crate) const BREAK_TRANSITIONS: [u8; {len(values)}] = [",
    ]
    lines.extend(format_int_array(values))
    lines += ["];"]
    return "\n".join(lines) + "\n"


def render_ghostty_grapheme_values() -> list[int]:
    with tempfile.TemporaryDirectory(prefix="roastty-unicode-grapheme-") as temp_dir:
        temp_root = Path(temp_dir)
        output = subprocess.run(
            [
                find_zig_015(),
                "build",
                "--build-file",
                str(write_grapheme_dump_build(temp_root)),
                "--cache-dir",
                str(temp_root / ".zig-cache"),
                "--global-cache-dir",
                str(temp_root / ".zig-cache-global"),
            ],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        ).stdout

    values = []
    expected = [
        (state, gb1, gb2)
        for state in STATE_NAMES
        for gb1 in GB_NAMES
        for gb2 in GB_NAMES
    ]
    lines = [line for line in output.splitlines() if line]
    if len(lines) != len(expected):
        raise SystemExit(
            f"Ghostty grapheme dump length mismatch: got {len(lines)}, expected {len(expected)}"
        )
    for line, expected_key in zip(lines, expected):
        state, gb1, gb2, result, next_state = line.split(",")
        if (state, gb1, gb2) != expected_key:
            raise SystemExit(
                "Ghostty grapheme dump order mismatch: "
                f"got {(state, gb1, gb2)}, expected {expected_key}"
            )
        values.append((1 if result == "true" else 0) | (int(next_state) << 1))
    return values


def find_zig_015() -> str:
    candidates = [
        os.environ.get("TERMSURF_ZIG_015"),
        "/opt/homebrew/opt/zig@0.15/bin/zig",
        "zig",
    ]
    for candidate in candidates:
        if not candidate:
            continue
        try:
            version = subprocess.run(
                [candidate, "version"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            ).stdout.strip()
        except (FileNotFoundError, subprocess.CalledProcessError):
            continue
        if version.startswith("0.15."):
            return candidate
    raise SystemExit(
        "Zig 0.15.x is required to verify Ghostty grapheme transitions; "
        "install zig@0.15 or set TERMSURF_ZIG_015"
    )


def find_uucode_dir() -> Path:
    candidates = sorted(ROOT.glob("vendor/ghostty/zig-pkg/uucode-*"))
    if not candidates:
        raise SystemExit("could not find Ghostty uucode package under vendor/ghostty/zig-pkg")
    return candidates[0]


def write_grapheme_dump_build(temp_root: Path) -> Path:
    (temp_root / "uucode").symlink_to(find_uucode_dir(), target_is_directory=True)
    (temp_root / "build.zig.zon").write_text(
        """.{
    .name = .check,
    .version = "0.0.0",
    .fingerprint = 0x3c8eac13413e0f3a,
    .minimum_zig_version = "0.15.2",
    .dependencies = .{
        .uucode = .{ .path = "uucode" },
    },
    .paths = .{""},
}
"""
    )
    (temp_root / "build.zig").write_text(
        r'''const std = @import("std");

const build_config =
\\const config = @import("config.zig");
\\const config_x = @import("config.x.zig");
\\pub const tables = [_]config.Table{
\\    .{
\\        .extensions = &.{ config_x.grapheme_break_no_control },
\\        .fields = &config._resolveFields(config_x, &.{ "grapheme_break", "grapheme_break_no_control" }, &.{ "grapheme_break_no_control" }),
\\    },
\\};
;

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});
    const exe = b.addExecutable(.{
        .name = "dump",
        .root_module = b.createModule(.{
            .root_source_file = b.path("dump.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });
    const uucode = b.dependency("uucode", .{
        .target = target,
        .optimize = optimize,
        .@"build_config.zig" = build_config,
    });
    exe.root_module.addImport("uucode", uucode.module("uucode"));
    const run = b.addRunArtifact(exe);
    b.default_step.dependOn(&run.step);
}
'''
    )
    (temp_root / "dump.zig").write_text(
        r'''const std = @import("std");
const uucode = @import("uucode");

pub fn main() !void {
    @setEvalBranchQuota(10_000);
    var out_buf: [4096]u8 = undefined;
    var out_writer = std.fs.File.stdout().writer(&out_buf);
    const out = &out_writer.interface;
    const states = @typeInfo(uucode.grapheme.BreakState).@"enum".fields;
    const fields = @typeInfo(uucode.x.types.GraphemeBreakNoControl).@"enum".fields;
    inline for (states) |state_field| {
        inline for (fields) |field1| {
            inline for (fields) |field2| {
                var state: uucode.grapheme.BreakState = @enumFromInt(state_field.value);
                const result = uucode.x.grapheme.computeGraphemeBreakNoControl(
                    @field(uucode.x.types.GraphemeBreakNoControl, field1.name),
                    @field(uucode.x.types.GraphemeBreakNoControl, field2.name),
                    &state,
                );
                try out.print("{s},{s},{s},{},{}\n", .{
                    state_field.name,
                    field1.name,
                    field2.name,
                    result,
                    @intFromEnum(state),
                });
            }
        }
    }
    try out.flush();
}
'''
    )
    return temp_root / "build.zig"


if __name__ == "__main__":
    raise SystemExit(main())
