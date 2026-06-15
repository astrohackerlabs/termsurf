#!/usr/bin/env python3
"""Guard image-storage-limit runtime parity for Issue 805 CFG-223."""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ISSUE = ROOT / "issues/0805-roastty-ghostty-parity"


def read(path: str) -> str:
    return (ROOT / path).read_text()


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise AssertionError(f"missing {label}: {needle!r}")


def require_all(text: str, needles: list[tuple[str, str]]) -> None:
    for needle, label in needles:
        require(text, needle, label)


def require_row(markdown: str, row_id: str) -> str:
    for line in markdown.splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if cells and cells[0] == row_id:
            return line
    raise AssertionError(f"missing inventory row {row_id}")


def main() -> int:
    ghostty_config = read("vendor/ghostty/src/config/Config.zig")
    ghostty_termio = read("vendor/ghostty/src/termio/Termio.zig")
    roastty_config = read("roastty/src/config/mod.rs")
    roastty_terminal = read("roastty/src/terminal/terminal.rs")
    roastty_termio = read("roastty/src/termio.rs")
    roastty_lib = read("roastty/src/lib.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_config,
        [
            ('@"image-storage-limit": u32 = 320 * 1000 * 1000', "Ghostty config field"),
            ("The total amount of bytes that can be used for image data", "Ghostty image limit documentation"),
        ],
    )
    require_all(
        ghostty_termio,
        [
            ("image_storage_limit: usize", "Ghostty derived config field"),
            ('config.@"image-storage-limit"', "Ghostty parsed config handoff"),
            (
                ".kitty_image_storage_limit = opts.config.image_storage_limit",
                "Ghostty terminal init image limit",
            ),
            (
                "self.terminal.setKittyGraphicsSizeLimit",
                "Ghostty live image limit update",
            ),
            (
                "config.image_storage_limit",
                "Ghostty live image limit config value",
            ),
            (
                "self.terminal.setKittyGraphicsLoadingLimits(.all)",
                "Ghostty live image loading limit reset",
            ),
        ],
    )

    require_all(
        roastty_config,
        [
            ("pub image_storage_limit: u32", "Roastty parsed config field"),
            ('"image-storage-limit"', "Roastty config key"),
            ("parse_u32_scalar_field", "Roastty integer parser helper"),
        ],
    )
    require_all(
        roastty_terminal,
        [
            ("pub(crate) fn kitty_image_storage_limit", "Roastty terminal limit getter"),
            ("pub(crate) fn set_kitty_image_storage_limit", "Roastty terminal limit setter"),
            ("pub(crate) fn set_kitty_image_medium", "Roastty terminal medium setter"),
            ("terminal_stream_kitty_graphics_config_applies_to_screens_and_resets", "Roastty direct terminal kitty config test"),
        ],
    )
    require_all(
        roastty_termio,
        [
            ("pub(crate) image_storage_limit: usize", "Roastty Termio spawn option"),
            ("image_storage_limit: crate::terminal::kitty::graphics_storage::DEFAULT_TOTAL_LIMIT", "Roastty Termio default limit"),
            ("terminal.set_kitty_image_storage_limit(options.image_storage_limit)", "Roastty Termio startup limit handoff"),
            ("KittyImageMedium::File", "Roastty Termio file medium startup"),
            ("KittyImageMedium::TemporaryFile", "Roastty Termio temp-file medium startup"),
            ("KittyImageMedium::SharedMemory", "Roastty Termio shared-memory medium startup"),
            ("termio_image_storage_limit_runtime_spawn_options_reach_terminal", "Roastty Termio runtime test"),
        ],
    )
    require_all(
        roastty_lib,
        [
            ("image_storage_limit: config.image_storage_limit as usize", "Roastty startup config handoff"),
            ("terminal.set_kitty_image_storage_limit(parsed.image_storage_limit as usize)", "Roastty live config limit update"),
            ("terminal.set_kitty_image_medium(KittyImageMedium::File, true)", "Roastty live file medium reset"),
            ("terminal.set_kitty_image_medium(KittyImageMedium::TemporaryFile, true)", "Roastty live temp-file medium reset"),
            ("terminal.set_kitty_image_medium(KittyImageMedium::SharedMemory, true)", "Roastty live shared-memory medium reset"),
            ("surface_image_storage_limit_runtime_startup_and_update", "Roastty surface runtime test"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-009B2B2B3B2B2B2B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-009B2B2B3B2B2B2B1 status"),
            ("`image-storage-limit` kitty graphics storage quota startup and live update effects", "image limit behavior"),
            ("termio_image_storage_limit_runtime_spawn_options_reach_terminal", "Termio evidence"),
            ("surface_image_storage_limit_runtime_startup_and_update", "surface update evidence"),
            ("image_storage_limit_runtime_parity.py", "static parity guard evidence"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-009B2B2B3B2B2B2B3")
    require_all(
        row_gap,
        [
            ("Oracle complete", "RUNTIME-009B2B2B3B2B2B2B3 status"),
            ("terminal-runtime residual audit", "terminal residual audit"),
        ],
    )

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("Runtime and UI effects", "CFG-223 row"),
            ("Gap", "CFG-223 status"),
            ("75 rows Oracle complete", "CFG-223 oracle count"),
            ("78 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("image_storage_limit_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
