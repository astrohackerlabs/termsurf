#!/usr/bin/env python3
"""Audit the residual terminal-runtime CFG-223 row for Issue 805."""

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


def require_row_complete(markdown: str, row_id: str, needles: list[str]) -> None:
    row = require_row(markdown, row_id)
    require(row, "Oracle complete", f"{row_id} complete status")
    for needle in needles:
        require(row, needle, f"{row_id} evidence {needle}")


def main() -> int:
    ghostty_termio = read("vendor/ghostty/src/termio/Termio.zig")
    ghostty_stream = read("vendor/ghostty/src/termio/stream_handler.zig")
    inventory_source = read("issues/0805-roastty-ghostty-parity/config_runtime_inventory.py")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_termio,
        [
            ("pub const DerivedConfig = struct {", "Ghostty DerivedConfig"),
            ("palette: terminalpkg.color.Palette", "DerivedConfig palette"),
            ("image_storage_limit: usize", "DerivedConfig image storage"),
            ("cursor_style: terminalpkg.CursorStyle", "DerivedConfig cursor style"),
            ("cursor_blink: ?bool", "DerivedConfig cursor blink"),
            ("cursor_color: ?configpkg.Config.TerminalColor", "DerivedConfig cursor color"),
            ("foreground: configpkg.Config.Color", "DerivedConfig foreground"),
            ("background: configpkg.Config.Color", "DerivedConfig background"),
            ("osc_color_report_format: configpkg.Config.OSCColorReportFormat", "DerivedConfig OSC format"),
            ("clipboard_write: configpkg.ClipboardAccess", "DerivedConfig clipboard write"),
            ("enquiry_response: []const u8", "DerivedConfig enquiry response"),
            ("conditional_state: configpkg.ConditionalState", "DerivedConfig conditional state"),
            ('opts.full_config.@"grapheme-width-method"', "direct grapheme config use"),
            ('opts.full_config.@"scrollback-limit"', "direct scrollback config use"),
            ("opts.config.cursor_blink orelse true", "cursor blink default mode"),
            (".kitty_image_storage_limit = opts.config.image_storage_limit", "image limit init"),
            ("self.terminal.colors.palette.changeDefault(config.palette)", "palette live update"),
            ("self.terminal.colors.background.default = config.background.toTerminalRGB()", "background live update"),
            ("self.terminal.colors.foreground.default = config.foreground.toTerminalRGB()", "foreground live update"),
            ("self.terminal.colors.cursor.default = cursor:", "cursor color live update"),
            ("try self.terminal.setKittyGraphicsSizeLimit(self.alloc, config.image_storage_limit)", "image limit live update"),
            ("self.terminal.setKittyGraphicsLoadingLimits(.all)", "image media live update"),
            ("pub fn colorSchemeReportLocked", "color-scheme report handler"),
            ("self.config.conditional_state.theme", "conditional theme color-scheme source"),
            ('.light => "\\x1B[?997;2n"', "light color-scheme report"),
            ('.dark => "\\x1B[?997;1n"', "dark color-scheme report"),
        ],
    )

    require_all(
        ghostty_stream,
        [
            ("self.osc_color_report_format = config.osc_color_report_format", "OSC format changeConfig"),
            ("self.clipboard_write = config.clipboard_write", "clipboard write changeConfig"),
            ("self.enquiry_response = config.enquiry_response", "enquiry response changeConfig"),
            ("self.default_cursor_style = config.cursor_style", "cursor style changeConfig"),
            ("self.default_cursor_blink = config.cursor_blink", "cursor blink changeConfig"),
            ("if (self.default_cursor) self.setCursorStyle(.default)", "default cursor refresh"),
            ("self.messageWriter(.{ .color_scheme_report = .{ .force = false } });", "color scheme report update"),
            ("fn deviceAttributes", "device attributes handler"),
            (".write_stable = if (self.clipboard_write != .deny)", "clipboard DA capability"),
            ("pub fn enquiry", "ENQ handler"),
            ("self.messageWriter(try termio.Message.writeReq(self.alloc, self.enquiry_response))", "ENQ write"),
            ("fn colorOperation", "OSC color operation handler"),
            ("if (self.osc_color_report_format == .none) break :report", "OSC format none gate"),
        ],
    )

    require_row_complete(
        runtime_inventory,
        "RUNTIME-006",
        ["color, palette", "theme", "color-scheme"],
    )
    require_row_complete(runtime_inventory, "RUNTIME-009B1", ["scrollback-limit"])
    require_row_complete(runtime_inventory, "RUNTIME-009B2B1", ["shell-integration"])
    require_row_complete(runtime_inventory, "RUNTIME-009B2B2A", ["surface-title"])
    require_row_complete(runtime_inventory, "RUNTIME-009B2B2B1", ["stored-PWD title fallback"])
    require_row_complete(runtime_inventory, "RUNTIME-009B2B2B2", ["OSC 7 local PWD"])
    require_row_complete(runtime_inventory, "RUNTIME-009B2B2B3A", ["nonzero scrollback byte quota"])
    require_row_complete(runtime_inventory, "RUNTIME-009B2B2B3B1", ["shell-specific startup rewrite"])
    require_row_complete(runtime_inventory, "RUNTIME-009B2B2B3B2A", ["OSC 7 query"])
    require_row_complete(runtime_inventory, "RUNTIME-009B2B2B3B2B1", ["enquiry-response"])
    require_row_complete(runtime_inventory, "RUNTIME-009B2B2B3B2B2A", ["osc-color-report-format"])
    require_row_complete(runtime_inventory, "RUNTIME-009B2B2B3B2B2B1", ["clipboard-write"])
    require_row_complete(runtime_inventory, "RUNTIME-009B2B2B3B2B2B2A", ["cursor-style"])
    require_row_complete(runtime_inventory, "RUNTIME-009B2B2B3B2B2B2B1", ["image-storage-limit"])
    require_row_complete(runtime_inventory, "RUNTIME-009B2B2B3B2B2B2B2", ["grapheme-width-method"])

    residual = require_row(runtime_inventory, "RUNTIME-009B2B2B3B2B2B2B3")
    require_all(
        residual,
        [
            ("Oracle complete", "residual row status"),
            ("terminal-runtime residual audit", "residual audit evidence"),
            ("DerivedConfig", "DerivedConfig evidence"),
            ("no remaining pinned Ghostty config-driven terminal-runtime fields", "closure claim"),
        ],
    )

    require_all(
        inventory_source,
        [
            ('id="RUNTIME-009B2B2B3B2B2B2B3"', "source residual row"),
            ('status="Oracle complete"', "source residual complete status"),
            ("terminal_runtime_residual_audit.py", "source guard command"),
            ('id="RUNTIME-007B2B2B2B2"', "font gap remains tracked"),
            ("RUNTIME-008B2B2B2B2B", "renderer gap remains tracked"),
            ("RUNTIME-011B2B", "macOS gap remains tracked"),
            ("RUNTIME-012B2B2B2B2B3", "notification/link GUI gap remains tracked"),
        ],
    )

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("Runtime and UI effects", "CFG-223 row"),
            ("Gap", "CFG-223 remains open"),
            ("80 rows Oracle complete", "CFG-223 oracle count"),
            ("83 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("terminal_runtime_residual_audit=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
