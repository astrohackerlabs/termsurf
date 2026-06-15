#!/usr/bin/env python3
"""Guard link preview/context runtime parity for Issue 805 CFG-223."""

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
    ghostty_surface = read("vendor/ghostty/src/Surface.zig")
    roastty_config = read("roastty/src/config/mod.rs")
    roastty_lib = read("roastty/src/lib.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_surface,
        [
            ("const link = (try self.linkAtPos(pos))", "Ghostty hover link lookup"),
            ("self.config.link_previews == .true", "Ghostty regular link preview gate"),
            ("self.config.link_previews != .false", "Ghostty OSC8 preview gate"),
            (".@\"context-menu\" => {", "Ghostty context-menu branch"),
            ("if (try self.linkAtPos(pos)) |link|", "Ghostty context link selection lookup"),
            ("try self.setSelection(link.selection)", "Ghostty context selects link"),
            ("const sel = screen.selectWord", "Ghostty word fallback"),
            ("return false;", "Ghostty context menu remains unhandled"),
            ("fn linkAtPos", "Ghostty linkAtPos helper"),
            ("if (mouse_mods.equal(input.ctrlOrSuper(.{})))", "Ghostty OSC8 modifier gate"),
            ("return try self.linkAtPin(mouse_pin, mouse_mods)", "Ghostty configured link fallback"),
        ],
    )

    require_all(
        roastty_config,
        [
            ("pub(crate) enum LinkPreviews", "Roastty LinkPreviews enum"),
            ("pub(crate) fn previews_regular_link", "Roastty regular preview predicate"),
            ("matches!(self, LinkPreviews::True)", "Roastty regular preview gate"),
            ("pub(crate) fn previews_osc8_link", "Roastty OSC8 preview predicate"),
            ("!matches!(self, LinkPreviews::False)", "Roastty OSC8 preview gate"),
        ],
    )

    require_all(
        roastty_lib,
        [
            ("fn context_menu_action(&mut self) -> bool", "Roastty context menu action"),
            ("let mouse_mods = self.mouse_mods_with_capture()", "Roastty pre-lock mouse mods"),
            ("link_selection_at_viewport_cell(terminal, cell, mouse_mods)", "Roastty link selection before word fallback"),
            ("fn link_selection_at_viewport_cell", "Roastty link selection helper"),
            ("key_mods::ctrl_or_super(key_mods::Mods::new())", "Roastty OSC8 ctrl/super gate"),
            ("fn osc8_link_selection_at_viewport_cell", "Roastty OSC8 selection helper"),
            ("fn regex_link_selection_at_viewport_cell", "Roastty regex selection helper"),
            ("terminal.select_line(ref_, None, true)", "Roastty line-scoped link search"),
            (
                "terminal.selection_viewport_string_map(line, false)",
                "Roastty line-scoped string map",
            ),
            ("for link in &config.parsed_config.link", "Roastty configured link iteration"),
            ("input::link::Highlight::AlwaysMods(mods)", "Roastty modifier-gated configured links"),
            (".select_word(", "Roastty word fallback"),
            ("Some(false)", "Roastty containing selection preservation"),
            ("false\n    }\n\n    fn link_selection_at_viewport_cell", "Roastty context menu returns unhandled"),
            ("fn link_preview_context_runtime_gates_preview_by_link_kind", "Roastty preview gate test"),
            ("fn link_preview_context_runtime_context_menu_selects_regex_link", "Roastty regex context test"),
            ("fn link_preview_context_runtime_context_menu_regex_is_line_scoped", "Roastty line-scope regression test"),
            ("fn link_preview_context_runtime_context_menu_preserves_containing_link_selection", "Roastty preserve selection test"),
            ("fn link_preview_context_runtime_context_menu_selects_osc8_with_ctrl_or_super", "Roastty OSC8 context test"),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-012B2B2B2B2B1")
    require_all(
        row_complete,
        [
            ("Oracle complete", "complete row status"),
            ("link preview config predicates", "complete row preview predicate behavior"),
            ("context-menu selection", "complete row context behavior"),
            ("semantic prompt boundaries", "complete row line scope evidence"),
            ("link_preview_context_runtime", "complete row cargo guard"),
            ("link_preview_context_runtime_parity.py", "complete row Python guard"),
        ],
    )

    row_dispatch = require_row(runtime_inventory, "RUNTIME-012B2B2B2B2B2")
    require_all(
        row_dispatch,
        [
            ("Oracle complete", "dispatch row status"),
            ("link hover preview surface action dispatch", "dispatch row behavior"),
            ("Experiment 161", "dispatch row experiment"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-012B2B2B2B2B3")
    require_all(
        row_gap,
        [
            ("Gap", "remaining row status"),
            ("Actual OS banner/sound delivery", "remaining OS notification gap"),
            ("actual audio/dock/border/title GUI effects", "remaining bell GUI gap"),
            ("real app link hover/cursor UI", "remaining hover/cursor gap"),
            ("native link preview display", "remaining native preview gap"),
            ("native context/menu display", "remaining native context menu gap"),
            ("OS URL-opening flows", "remaining OS URL opening gap"),
        ],
    )

    old_gap_absent = "RUNTIME-012B2B2B2B2B | remaining notification/link/bell GUI effects"
    if old_gap_absent in runtime_inventory:
        raise AssertionError("old unsplit RUNTIME-012B2B2B2B2B gap row remains")

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("75 rows Oracle complete", "CFG-223 oracle count"),
            ("78 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("link_preview_context_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
