#!/usr/bin/env python3
"""Check title-report runtime wiring for Issue 805."""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]

GHOSTTY_CONFIG = ROOT / "vendor/ghostty/src/config/Config.zig"
GHOSTTY_SURFACE = ROOT / "vendor/ghostty/src/Surface.zig"
ROASTTY_CONFIG = ROOT / "roastty/src/config/mod.rs"
ROASTTY_TERMINAL = ROOT / "roastty/src/terminal/terminal.rs"
ROASTTY_TERMIO = ROOT / "roastty/src/termio.rs"
ROASTTY_LIB = ROOT / "roastty/src/lib.rs"


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def main() -> int:
    ghostty_config = read(GHOSTTY_CONFIG)
    ghostty_surface = read(GHOSTTY_SURFACE)
    roastty_config = read(ROASTTY_CONFIG)
    roastty_terminal = read(ROASTTY_TERMINAL)
    roastty_termio = read(ROASTTY_TERMIO)
    roastty_lib = read(ROASTTY_LIB)

    require(
        '@"title-report": bool = false' in ghostty_config,
        "Pinned Ghostty title-report default is no longer false",
    )
    require(
        'if (!self.config.title_report)' in ghostty_surface
        and "report_title requested, but disabled via config" in ghostty_surface,
        "Pinned Ghostty Surface title-report gate is missing",
    )
    require(
        '"\\x1b]l{s}\\x1b\\\\"' in ghostty_surface
        and "self.rt_surface.getTitle()" in ghostty_surface,
        "Pinned Ghostty Surface CSI 21t report-title response changed",
    )

    require(
        "pub title_report: bool" in roastty_config
        and "title_report: false" in roastty_config
        and '"title-report" => self.title_report = set_bool_field' in roastty_config,
        "Roastty config title-report field/default/parser wiring is missing",
    )
    require(
        "title_report: bool" in roastty_terminal
        and "pub(crate) fn set_title_report(&mut self, enabled: bool)" in roastty_terminal
        and "self.title_report = enabled;" in roastty_terminal,
        "Roastty terminal title-report state or setter is missing",
    )
    require(
        re.search(
            r"if request == size_report::Request::Csi21T \{\s*if !\*self\.title_report \{\s*return;\s*\}",
            roastty_terminal,
            flags=re.DOTALL,
        )
        is not None,
        "Roastty terminal CSI 21t branch is not gated by title_report",
    )
    require(
        "pub(crate) title_report: bool" in roastty_termio
        and "title_report: false" in roastty_termio
        and "title_report: options.title_report" in roastty_termio,
        "Roastty TermioSpawnOptions title-report startup wiring is missing",
    )
    require(
        "title_report: config.title_report" in roastty_lib,
        "Roastty surface startup does not pass title_report through TermioSpawnOptions",
    )
    require(
        "terminal.set_title_report(parsed.title_report);" in roastty_lib,
        "Roastty Surface::apply_config does not refresh terminal title-report gate",
    )
    require(
        "config_title_report_runtime_startup_and_update_gate" in roastty_lib
        and "terminal_stream_title_report_disabled_by_default" in roastty_terminal,
        "Roastty title-report runtime guards are missing",
    )

    print("title_report_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
