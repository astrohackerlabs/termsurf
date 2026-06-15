#!/usr/bin/env python3
"""Guard window padding layout runtime parity for Issue 805 CFG-223."""

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
    ghostty_size = read("vendor/ghostty/src/renderer/size.zig")
    roastty_size = read("roastty/src/renderer/size.rs")
    roastty_lib = read("roastty/src/lib.rs")
    runtime_inventory = (ISSUE / "config-runtime-inventory.md").read_text()
    config_matrix = (ISSUE / "config-matrix.md").read_text()

    require_all(
        ghostty_surface,
        [
            (
                ".window_padding_top = config.@\"window-padding-y\".top_left,",
                "Ghostty derived top padding",
            ),
            (
                ".window_padding_bottom = config.@\"window-padding-y\".bottom_right,",
                "Ghostty derived bottom padding",
            ),
            (
                ".window_padding_left = config.@\"window-padding-x\".top_left,",
                "Ghostty derived left padding",
            ),
            (
                ".window_padding_right = config.@\"window-padding-x\".bottom_right,",
                "Ghostty derived right padding",
            ),
            (
                "fn scaledPadding(self: *const DerivedConfig, x_dpi: f32, y_dpi: f32) rendererpkg.Padding",
                "Ghostty scaledPadding helper",
            ),
            (
                "break :padding_top @intFromFloat(@floor(padding_top * y_dpi / 72));",
                "Ghostty top y-DPI scaling",
            ),
            (
                "break :padding_left @intFromFloat(@floor(padding_left * x_dpi / 72));",
                "Ghostty left x-DPI scaling",
            ),
            (
                "size.balancePadding(explicit, derived_config.window_padding_balance);",
                "Ghostty init balance application",
            ),
            ("self.balancePaddingIfNeeded();", "Ghostty resize balance call"),
            (
                "self.size.padding = self.config.scaledPadding(x_dpi, y_dpi);",
                "Ghostty content-scale unbalanced padding update",
            ),
            (
                "self.queueIo(.{ .resize = self.size }, .unlocked);",
                "Ghostty padded resize reaches IO",
            ),
        ],
    )
    require_all(
        ghostty_size,
        [
            ("pub fn grid(self: Size) GridSize", "Ghostty Size.grid"),
            ("return self.screen.subPadding(self.padding);", "Ghostty terminal size"),
            ("pub fn balancePadding(", "Ghostty balancePadding"),
            ("const max_top = (explicit.left + explicit.right + self.cell.width) / 2;", "Ghostty top cap"),
            ("pub fn balanced(screen: ScreenSize, grid: GridSize, cell: CellSize) Padding", "Ghostty balanced padding"),
        ],
    )

    require_all(
        roastty_size,
        [
            (
                "pub(crate) fn from_config(",
                "Roastty config-derived renderer Size helper",
            ),
            (
                "let x_dpi = x_scale.max(0.0) * 72.0;",
                "Roastty x scale to DPI",
            ),
            (
                "let y_dpi = y_scale.max(0.0) * 72.0;",
                "Roastty y scale to DPI",
            ),
            (
                "let explicit = Padding::scaled_from_config(config, x_dpi, y_dpi);",
                "Roastty scaled explicit padding",
            ),
            (
                "config::WindowPaddingBalance::True =>",
                "Roastty true balance branch",
            ),
            (
                "config::WindowPaddingBalance::Equal =>",
                "Roastty equal balance branch",
            ),
            (
                "top: scaled(config.window_padding_y.top_left, y_dpi)",
                "Roastty top y-DPI scaling",
            ),
            (
                "left: scaled(config.window_padding_x.top_left, x_dpi)",
                "Roastty left x-DPI scaling",
            ),
            (
                "fn window_padding_layout_runtime_asymmetric_scale_uses_axes_independently",
                "Roastty asymmetric scale test",
            ),
            (
                "fn window_padding_layout_runtime_balance_true_uses_ghostty_top_cap",
                "Roastty balance true test",
            ),
            (
                "fn window_padding_layout_runtime_balance_equal_centers_grid",
                "Roastty balance equal test",
            ),
        ],
    )
    require_all(
        roastty_lib,
        [
            ("renderer_padding: renderer::size::Padding", "Roastty surface padding state"),
            (
                "fn recompute_renderer_size_from_config(",
                "Roastty surface padded size helper",
            ),
            (
                "renderer::size::Size::from_config(",
                "Roastty surface uses renderer Size helper",
            ),
            (
                "Self::sanitized_scale(self.scale_factor_x)",
                "Roastty x scale passed to helper",
            ),
            (
                "Self::sanitized_scale(self.scale_factor_y)",
                "Roastty y scale passed to helper",
            ),
            ("self.size.columns = grid.columns;", "Roastty surface columns from padded grid"),
            ("self.size.rows = grid.rows;", "Roastty surface rows from padded grid"),
            (
                "fn resize_pty_to_current_size(&mut self, report_in_band_size: bool)",
                "Roastty shared PTY resize helper",
            ),
            (
                "self.recompute_renderer_size_from_config(&config);",
                "Roastty startup/present recompute marker",
            ),
            ("self.recompute_renderer_size();", "Roastty resize/content-scale recompute marker"),
            (
                "surface.resize_pty_to_current_size(false);",
                "Roastty content-scale PTY resize marker",
            ),
            (
                "render_size.unwrap_or(renderer::size::Size",
                "Roastty update_screen padded Size fallback marker",
            ),
            (
                "frame_renderer.update_screen(",
                "Roastty active live renderer update_screen",
            ),
            (
                "top: self.renderer_padding.top",
                "Roastty mouse geometry padding top",
            ),
            (
                "fn window_padding_layout_runtime_surface_grid_uses_asymmetric_scaled_padding",
                "Roastty surface asymmetric runtime test",
            ),
            (
                "fn window_padding_layout_runtime_set_size_recomputes_padded_pty_grid",
                "Roastty set_size padded PTY test",
            ),
            (
                "fn window_padding_layout_runtime_content_scale_updates_unbalanced_padding",
                "Roastty content-scale padding test",
            ),
        ],
    )

    row_complete = require_row(runtime_inventory, "RUNTIME-008B2B2A")
    require_all(
        row_complete,
        [
            ("Oracle complete", "RUNTIME-008B2B2A status"),
            ("window-padding-x", "RUNTIME-008B2B2A x padding behavior"),
            ("window-padding-y", "RUNTIME-008B2B2A y padding behavior"),
            ("window-padding-balance", "RUNTIME-008B2B2A balance behavior"),
            ("independent X/Y scale", "RUNTIME-008B2B2A asymmetric evidence"),
            ("padded rows/columns", "RUNTIME-008B2B2A PTY evidence"),
            ("window_padding_layout_runtime_parity.py", "RUNTIME-008B2B2A guard"),
        ],
    )

    row_gap = require_row(runtime_inventory, "RUNTIME-008B2B2B2B2B")
    require_all(
        row_gap,
        [
            ("Gap", "RUNTIME-008B2B2B2B2B status"),
            ("GUI cursor pixels", "RUNTIME-008B2B2B2B2B cursor gap"),
            ("screenshot-level padding pixel proof", "RUNTIME-008B2B2B2B2B screenshot gap"),
        ],
    )
    if 'id="RUNTIME-008B2B2",' in read(
        "issues/0805-roastty-ghostty-parity/config_runtime_inventory.py"
    ):
        raise AssertionError("old broad RUNTIME-008B2B2 row is still present")

    cfg223 = require_row(config_matrix, "CFG-223")
    require_all(
        cfg223,
        [
            ("Runtime and UI effects", "CFG-223 row"),
            ("Gap", "CFG-223 status"),
            ("73 rows Oracle complete", "CFG-223 oracle count"),
            ("76 rows closed", "CFG-223 closed count"),
            ("4 rows are incomplete", "CFG-223 incomplete count"),
            ("4 rows are runtime gaps", "CFG-223 gap count"),
        ],
    )

    print("window_padding_layout_runtime_parity=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
