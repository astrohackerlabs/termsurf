#![allow(dead_code)]
//! A persistent renderer that owns the CPU-side frame-rebuild state and drives
//! `rebuild_frame` from a live terminal across frames (Issue 801, Exp 840).
//!
//! This is the first renderer-integration slice. Metal presentation
//! (`draw_frame`/compositor), deriving the rebuild input from surface config/
//! state, wiring into `surface.draw()`, and clearing the terminal's dirty bits
//! are all later slices.

use crate::config::{Config, WindowPaddingColor};
use crate::font::run::Wide;
use crate::font::shared_grid::SharedGrid;
use crate::renderer::cell::{row_never_extend_bg_flags, Contents, Highlight, SelectionConfig};
use crate::renderer::cursor::Style as CursorStyle;
use crate::renderer::frame_rebuild::{
    FramePaddingExtendInput, FramePreparedFrameApplication, FramePreparedFrameError,
    FramePreparedPresentationInput, FramePreparedRebuildApplication, FramePreparedRebuildError,
    FramePreparedRebuildInput, FramePreparedRebuildTargets, FrameRebuildUniformInput,
    FrameSnapshotBlockCursorUniformInput, FrameSnapshotCursorOverlayInput,
    FrameSnapshotCursorUniformInput, FrameSnapshotRowFormatInput, FrameSnapshotTextOverlayInput,
    FrameTerminalSnapshot, RenderDirty,
};
use crate::renderer::metal::shaders::MetalUniforms;
use crate::renderer::size::GridSize;
use crate::renderer::state::Preedit;
use crate::terminal::color::{Palette, Rgb};
use crate::terminal::style::BoldColor;
use crate::terminal::terminal::{Terminal, TerminalColorKind};

/// Owns the persistent CPU-side frame-rebuild state across frames: the rebuilt
/// `Contents`, the `MetalUniforms`, the last-rendered grid (for resize
/// detection), and a scratch per-row dirty buffer the rebuild clears.
pub(crate) struct FrameRenderer {
    contents: Contents,
    uniforms: MetalUniforms,
    current_grid: GridSize,
    row_dirty: Vec<bool>,
}

impl FrameRenderer {
    /// Build a renderer over caller-supplied (config-derived) uniforms. The grid
    /// starts at 0×0 so the first `update_frame` is a full rebuild + resize.
    pub(crate) fn new(uniforms: MetalUniforms) -> Self {
        Self {
            contents: Contents::default(),
            uniforms,
            current_grid: GridSize {
                columns: 0,
                rows: 0,
            },
            row_dirty: Vec::new(),
        }
    }

    /// Collect a snapshot from the live terminal, rebuild this renderer's owned
    /// `contents`/`uniforms` against it, and advance `current_grid` on success so
    /// the next frame only resizes when the terminal grid actually changes. On a
    /// rebuild error the frame did not complete, so `current_grid` is left
    /// unchanged (the next frame re-resizes idempotently).
    pub(crate) fn update_frame(
        &mut self,
        terminal: &Terminal,
        grid: &mut SharedGrid,
        dirty: RenderDirty,
        preedit: Option<Preedit>,
        input: FramePreparedRebuildInput<'_>,
    ) -> Result<FramePreparedRebuildApplication, FramePreparedRebuildError> {
        let snapshot = FrameTerminalSnapshot::collect(terminal, self.current_grid, dirty, preedit);

        // Scratch dirty buffer the rebuild marks clean: a copy of the snapshot's
        // per-row dirty (length = terminal rows). Re-seeded every frame because the
        // dirty truth lives in the terminal and is re-snapshotted.
        self.row_dirty.clear();
        self.row_dirty.extend_from_slice(&snapshot.row_dirty);

        let app = snapshot.rebuild_frame(
            FramePreparedRebuildTargets {
                contents: &mut self.contents,
                grid,
                row_dirty: &mut self.row_dirty,
                uniforms: &mut self.uniforms,
            },
            input,
        )?;

        self.current_grid = snapshot.terminal_grid;
        Ok(app)
    }

    /// Collect a snapshot, rebuild this renderer's owned `contents`/`uniforms`,
    /// and present via Metal — the full frame, end to end (Issue 801, Exp 841).
    /// `current_grid` advances only on full success (both rebuild and present); a
    /// present-stage error leaves the rebuild's mutations in place but the grid
    /// unadvanced, so the next frame re-resizes idempotently.
    pub(crate) fn update_and_present_frame(
        &mut self,
        terminal: &Terminal,
        grid: &mut SharedGrid,
        dirty: RenderDirty,
        preedit: Option<Preedit>,
        input: FramePreparedRebuildInput<'_>,
        presentation: FramePreparedPresentationInput<'_>,
    ) -> Result<FramePreparedFrameApplication, FramePreparedFrameError> {
        let snapshot = FrameTerminalSnapshot::collect(terminal, self.current_grid, dirty, preedit);

        self.row_dirty.clear();
        self.row_dirty.extend_from_slice(&snapshot.row_dirty);

        let app = snapshot.rebuild_and_present_frame(
            FramePreparedRebuildTargets {
                contents: &mut self.contents,
                grid,
                row_dirty: &mut self.row_dirty,
                uniforms: &mut self.uniforms,
            },
            input,
            presentation,
        )?;

        self.current_grid = snapshot.terminal_grid;
        Ok(app)
    }

    /// Compose the full render input from `(terminal, config)` and rebuild a frame
    /// — the single entry point the live draw path uses (Issue 801, Exp 847).
    /// Builds the `FrameRenderState`, `FrameRenderKnobs`, and input internally and
    /// drives `update_frame`.
    pub(crate) fn render_frame(
        &mut self,
        terminal: &Terminal,
        grid: &mut SharedGrid,
        dirty: RenderDirty,
        preedit: Option<Preedit>,
        config: &Config,
    ) -> Result<FramePreparedRebuildApplication, FramePreparedRebuildError> {
        let state = FrameRenderState::from_terminal(terminal);
        let knobs = FrameRenderKnobs::from_config(config);
        let input = state.rebuild_input(&knobs);
        self.update_frame(terminal, grid, dirty, preedit, input)
    }

    /// The Metal-presenting variant of `render_frame` (Issue 801, Exp 847):
    /// compose the input from `(terminal, config)` and drive
    /// `update_and_present_frame`.
    pub(crate) fn render_and_present_frame(
        &mut self,
        terminal: &Terminal,
        grid: &mut SharedGrid,
        dirty: RenderDirty,
        preedit: Option<Preedit>,
        config: &Config,
        presentation: FramePreparedPresentationInput<'_>,
    ) -> Result<FramePreparedFrameApplication, FramePreparedFrameError> {
        let state = FrameRenderState::from_terminal(terminal);
        let knobs = FrameRenderKnobs::from_config(config);
        let input = state.rebuild_input(&knobs);
        self.update_and_present_frame(terminal, grid, dirty, preedit, input, presentation)
    }

    pub(crate) fn contents(&self) -> &Contents {
        &self.contents
    }

    pub(crate) fn uniforms(&self) -> &MetalUniforms {
        &self.uniforms
    }

    pub(crate) fn current_grid(&self) -> GridSize {
        self.current_grid
    }
}

/// The render knobs (Issue 801, Exp 842/846). `from_config` sources most from a
/// `Config`: `bold`, `background_opacity`, `padding_color`, and (Exp 845)
/// `thicken`/`thicken_strength`. Four fields have no `Config` option: `alpha` and
/// `overlay_alpha` are the faithful opaque `255`; `faint_opacity` and
/// `background_opacity_cells` are upstream-default placeholders awaiting their
/// config ports (`faint-opacity`, `background-opacity-cells`).
pub(crate) struct FrameRenderKnobs {
    pub(crate) bold: Option<BoldColor>,
    pub(crate) alpha: u8,
    pub(crate) faint_opacity: u8,
    pub(crate) thicken: bool,
    pub(crate) thicken_strength: u8,
    pub(crate) background_opacity_cells: bool,
    pub(crate) background_opacity: f64,
    pub(crate) padding_color: WindowPaddingColor,
    pub(crate) overlay_alpha: u8,
}

impl FrameRenderKnobs {
    /// Source the render knobs from a `Config`. Knobs without a config option yet
    /// take upstream-faithful default constants (Exp 846): `alpha`/`overlay_alpha`
    /// are opaque `255` (upstream hardcodes non-faint text alpha to 255);
    /// `faint_opacity` is `ceil(0.5 × 255) = 128` (the `faint-opacity` 0.5 default);
    /// `background_opacity_cells` is `false` (the `background-opacity-cells` default).
    pub(crate) fn from_config(config: &Config) -> Self {
        Self {
            bold: config.bold_color.map(|c| c.to_terminal()),
            alpha: 255,
            faint_opacity: 128,
            thicken: config.font_thicken,
            thicken_strength: config.font_thicken_strength,
            background_opacity_cells: false,
            background_opacity: config.background_opacity,
            padding_color: config.window_padding_color,
            overlay_alpha: 255,
        }
    }
}

/// Render data derived from the live terminal — the effective default fg/bg,
/// palette, cursor, and per-row never-extend flags — plus the dynamic buffers the
/// rebuild input borrows. The remaining stubs (until their own slices):
/// `highlights`/`link_ranges` empty and `selection_config` default.
pub(crate) struct FrameRenderState {
    default_fg: Rgb,
    default_bg: Rgb,
    palette: Palette,
    // `Some((style, color))` when the terminal cursor is visible; `None` otherwise.
    cursor: Option<(CursorStyle, Rgb)>,
    screen_fg: Rgb,
    highlights: Vec<Vec<Highlight>>,
    link_ranges: Vec<Vec<[u16; 2]>>,
    selection_config: SelectionConfig,
    row_never_extend: Vec<bool>,
}

impl FrameRenderState {
    /// Derive the effective colors and palette from the terminal, mirroring the
    /// existing GUI render path (`render_state_from_terminal`): background → black
    /// fallback, foreground → white fallback, the 256-entry palette. The dynamic
    /// buffers are empty/default placeholders for later slices.
    pub(crate) fn from_terminal(terminal: &Terminal) -> Self {
        let default_bg = terminal
            .color_effective(TerminalColorKind::Background)
            .map(|(r, g, b)| Rgb::new(r, g, b))
            .unwrap_or(Rgb::new(0, 0, 0));
        let default_fg = terminal
            .color_effective(TerminalColorKind::Foreground)
            .map(|(r, g, b)| Rgb::new(r, g, b))
            .unwrap_or(Rgb::new(0xff, 0xff, 0xff));
        let palette = terminal
            .palette_current()
            .map(|(r, g, b)| Rgb::new(r, g, b));

        // The cursor is derived only when visible: style mapped from the terminal
        // visual style, color from the effective cursor color with a foreground
        // fallback (mirrors the GUI render path / upstream `cursor_color`).
        let cursor = terminal.cursor_visible().then(|| {
            let style = CursorStyle::from_terminal(terminal.cursor_visual_style());
            let color = terminal
                .color_effective(TerminalColorKind::Cursor)
                .map(|(r, g, b)| Rgb::new(r, g, b))
                .unwrap_or(default_fg);
            (style, color)
        });

        // Per-row "never extend window padding into this row" flags, derived from
        // the shaped rows (a row with a default-background cell, a semantic prompt,
        // or a perfect-fit powerline cell never-extends). Note: this shapes the
        // rows a second time (the snapshot shapes them too); sharing one shaping is
        // a later refactor (Issue 801, Exp 846+).
        let rows = terminal.shape_run_options();
        let row_never_extend = row_never_extend_bg_flags(&rows, &palette, default_bg);

        Self {
            default_fg,
            default_bg,
            palette,
            cursor,
            screen_fg: default_fg,
            highlights: Vec::new(),
            link_ranges: Vec::new(),
            selection_config: SelectionConfig::default(),
            row_never_extend,
        }
    }

    /// Assemble a complete `FramePreparedRebuildInput` from the terminal-derived
    /// state and the caller-supplied knobs.
    pub(crate) fn rebuild_input<'a>(
        &'a self,
        knobs: &'a FrameRenderKnobs,
    ) -> FramePreparedRebuildInput<'a> {
        FramePreparedRebuildInput {
            row_format: FrameSnapshotRowFormatInput {
                highlights: &self.highlights,
                link_ranges: &self.link_ranges,
                selection_config: &self.selection_config,
                default_fg: self.default_fg,
                default_bg: self.default_bg,
                palette: &self.palette,
                bold: knobs.bold,
                alpha: knobs.alpha,
                faint_opacity: knobs.faint_opacity,
                thicken: knobs.thicken,
                thicken_strength: knobs.thicken_strength,
                background_opacity_cells: knobs.background_opacity_cells,
                background_opacity: knobs.background_opacity,
            },
            text_overlay: FrameSnapshotTextOverlayInput {
                cursor: self
                    .cursor
                    .map(|(style, color)| FrameSnapshotCursorOverlayInput {
                        style,
                        wide: false,
                        color,
                    }),
                screen_fg: self.screen_fg,
                alpha: knobs.overlay_alpha,
            },
            cursor_uniform: FrameSnapshotCursorUniformInput {
                // The block-cursor uniform is the under-cursor recolor; it applies
                // only to the Block style (other styles render as the overlay only).
                block_cursor: self
                    .cursor
                    .filter(|(style, _)| matches!(style, CursorStyle::Block))
                    .map(|(_, color)| FrameSnapshotBlockCursorUniformInput {
                        wide: Wide::Narrow,
                        color,
                    }),
            },
            rebuild_uniform: FrameRebuildUniformInput {
                padding_color: knobs.padding_color,
            },
            padding_extend: FramePaddingExtendInput {
                padding_color: knobs.padding_color,
                row_never_extend: &self.row_never_extend,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WindowPaddingColor;
    use crate::font::run::Wide;
    use crate::renderer::cell::{Highlight, SelectionConfig};
    use crate::renderer::cursor::Style as CursorStyle;
    use crate::renderer::frame_rebuild::{
        FramePaddingExtendInput, FrameRebuildUniformInput, FrameSnapshotBlockCursorUniformInput,
        FrameSnapshotCursorOverlayInput, FrameSnapshotCursorUniformInput,
        FrameSnapshotRowFormatInput, FrameSnapshotTextOverlayInput,
    };
    use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
    use crate::terminal::style::BoldColor;

    use crate::font::atlas::{Atlas, Format};
    use crate::renderer::metal::api::{MetalPixelFormat, MetalResourceOptions, MetalStorageMode};
    use crate::renderer::metal::compositor::{MetalFrameCompositor, MetalFrameCompositorOptions};
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_metal::{MTLCreateSystemDefaultDevice, MTLDevice};

    fn metal_device() -> Option<Retained<ProtocolObject<dyn MTLDevice>>> {
        MTLCreateSystemDefaultDevice()
    }

    fn metal_compositor(
        device: Retained<ProtocolObject<dyn MTLDevice>>,
        width: usize,
        height: usize,
        grayscale: &Atlas,
        color: &Atlas,
    ) -> MetalFrameCompositor {
        MetalFrameCompositor::new(MetalFrameCompositorOptions {
            device,
            width,
            height,
            pixel_format: MetalPixelFormat::Bgra8Unorm,
            storage_mode: MetalStorageMode::Shared,
            resource_options: MetalResourceOptions::image(MetalStorageMode::Shared),
            grayscale_atlas: grayscale,
            color_atlas: color,
        })
        .expect("compositor should be created")
    }

    fn grid(columns: u16, rows: u16) -> GridSize {
        GridSize { columns, rows }
    }

    fn terminal(columns: u16, rows: u16) -> Terminal {
        let mut terminal = Terminal::init(columns, rows, None).expect("terminal");
        terminal.next_slice(b"A").expect("terminal input");
        terminal
    }

    fn menlo_grid() -> SharedGrid {
        use crate::font::codepoint_resolver::CodepointResolver;
        use crate::font::collection::Collection;
        use crate::font::face::coretext::Face;
        use crate::font::Style;

        let mut collection = Collection::new();
        collection
            .add(Face::new("Menlo", 32.0), Style::Regular, false)
            .unwrap();
        collection.update_metrics().unwrap();
        let metrics = *collection.metrics().unwrap();
        SharedGrid::new(CodepointResolver::new(collection), metrics)
    }

    fn uniforms() -> MetalUniforms {
        MetalUniforms::test_with_grid([2, 3], [4, 5], [6.0, 7.0], [0.0; 4], 9, [1, 2, 3, 4])
    }

    /// Build a full rebuild input borrowing the caller-owned scratch data. The
    /// `row_never_extend` length is the only lever the error test needs (`&[]`
    /// makes `refine_padding_extend_rows` reject).
    fn frame_input<'a>(
        highlights: &'a [Vec<Highlight>],
        link_ranges: &'a [Vec<[u16; 2]>],
        selection_config: &'a SelectionConfig,
        row_never_extend: &'a [bool],
    ) -> FramePreparedRebuildInput<'a> {
        FramePreparedRebuildInput {
            row_format: FrameSnapshotRowFormatInput {
                highlights,
                link_ranges,
                selection_config,
                default_fg: Rgb::new(210, 211, 212),
                default_bg: Rgb::new(10, 11, 12),
                palette: &DEFAULT_PALETTE,
                bold: Some(BoldColor::Color(Rgb::new(1, 2, 3))),
                alpha: 230,
                faint_opacity: 99,
                thicken: true,
                thicken_strength: 77,
                background_opacity_cells: true,
                background_opacity: 0.42,
            },
            text_overlay: FrameSnapshotTextOverlayInput {
                cursor: Some(FrameSnapshotCursorOverlayInput {
                    style: CursorStyle::Underline,
                    wide: true,
                    color: Rgb::new(3, 4, 5),
                }),
                screen_fg: Rgb::new(40, 41, 42),
                alpha: 219,
            },
            cursor_uniform: FrameSnapshotCursorUniformInput {
                block_cursor: Some(FrameSnapshotBlockCursorUniformInput {
                    wide: Wide::Wide,
                    color: Rgb::new(11, 12, 13),
                }),
            },
            rebuild_uniform: FrameRebuildUniformInput {
                padding_color: WindowPaddingColor::Extend,
            },
            padding_extend: FramePaddingExtendInput {
                padding_color: WindowPaddingColor::Extend,
                row_never_extend,
            },
        }
    }

    #[test]
    fn first_frame_is_full_rebuild_and_resize() {
        let term = terminal(4, 3);
        let mut shared = menlo_grid();
        let mut renderer = FrameRenderer::new(uniforms());
        let highlights = Vec::new();
        let links = Vec::new();
        let selection = SelectionConfig::default();
        let never = [false, false, false, false];

        let app = renderer
            .update_frame(
                &term,
                &mut shared,
                RenderDirty::Partial,
                None,
                frame_input(&highlights, &links, &selection, &never),
            )
            .expect("first frame");

        // 0x0 -> 4x3 is a grid change, so a full rebuild + resize.
        assert!(app.rows.reset_contents);
        assert_eq!(app.rows.rebuilt_rows, vec![0, 1, 2]);
        assert_eq!(renderer.current_grid(), grid(4, 3));
        assert_eq!(renderer.contents().size(), grid(4, 3));
    }

    #[test]
    fn second_frame_with_clean_terminal_is_partial_without_resize() {
        let mut term = terminal(4, 3);
        let mut shared = menlo_grid();
        let mut renderer = FrameRenderer::new(uniforms());
        let highlights = Vec::new();
        let links = Vec::new();
        let selection = SelectionConfig::default();
        let never = [false, false, false, false];

        renderer
            .update_frame(
                &term,
                &mut shared,
                RenderDirty::Partial,
                None,
                frame_input(&highlights, &links, &selection, &never),
            )
            .expect("first frame");

        // No terminal change, dirty cleared -> no rows to rebuild, no resize.
        term.clear_dirty_for_tests();
        let app = renderer
            .update_frame(
                &term,
                &mut shared,
                RenderDirty::Partial,
                None,
                frame_input(&highlights, &links, &selection, &never),
            )
            .expect("second frame");

        assert!(!app.rows.reset_contents);
        assert!(app.rows.rebuilt_rows.is_empty());
        assert_eq!(renderer.current_grid(), grid(4, 3));
    }

    #[test]
    fn resize_is_detected_and_advances_current_grid() {
        let mut shared = menlo_grid();
        let mut renderer = FrameRenderer::new(uniforms());
        let highlights = Vec::new();
        let links = Vec::new();
        let selection = SelectionConfig::default();
        let never = [false, false, false, false];

        let term3 = terminal(4, 3);
        renderer
            .update_frame(
                &term3,
                &mut shared,
                RenderDirty::Partial,
                None,
                frame_input(&highlights, &links, &selection, &never),
            )
            .expect("first frame");
        assert_eq!(renderer.current_grid(), grid(4, 3));

        // A differently-sized terminal -> grid change -> resize + full rebuild.
        let term2 = terminal(4, 2);
        let app = renderer
            .update_frame(
                &term2,
                &mut shared,
                RenderDirty::Partial,
                None,
                frame_input(&highlights, &links, &selection, &never),
            )
            .expect("resize frame");

        assert!(app.rows.reset_contents);
        assert_eq!(renderer.current_grid(), grid(4, 2));
        assert_eq!(renderer.contents().size(), grid(4, 2));
    }

    #[test]
    fn rebuild_error_does_not_advance_current_grid() {
        let term = terminal(4, 3);
        let mut shared = menlo_grid();
        let mut renderer = FrameRenderer::new(uniforms());
        let highlights = Vec::new();
        let links = Vec::new();
        let selection = SelectionConfig::default();
        // Too short: under Extend, refine_padding_extend_rows validates a
        // row_never_extend index the empty slice does not cover.
        let never: [bool; 0] = [];

        let err = renderer
            .update_frame(
                &term,
                &mut shared,
                RenderDirty::Partial,
                None,
                frame_input(&highlights, &links, &selection, &never),
            )
            .expect_err("padding extend should fail");

        assert!(matches!(err, FramePreparedRebuildError::PaddingExtend(_)));
        // The frame did not complete, so the grid is not advanced off 0x0.
        assert_eq!(renderer.current_grid(), grid(0, 0));
    }

    #[test]
    fn update_and_present_rebuilds_and_presents() {
        let Some(device) = metal_device() else {
            return;
        };
        let term = terminal(4, 3);
        let mut shared = menlo_grid();
        let grayscale = Atlas::new(8, Format::Grayscale);
        let color = Atlas::new(8, Format::Bgra);
        let mut compositor = metal_compositor(device, 8, 6, &grayscale, &color);
        let mut renderer = FrameRenderer::new(uniforms());
        let highlights = Vec::new();
        let links = Vec::new();
        let selection = SelectionConfig::default();
        let never = [false, false, false, false];

        let app = renderer
            .update_and_present_frame(
                &term,
                &mut shared,
                RenderDirty::Partial,
                None,
                frame_input(&highlights, &links, &selection, &never),
                FramePreparedPresentationInput {
                    compositor: &mut compositor,
                    width: 8,
                    height: 6,
                    contents_scale: 1.0,
                    grayscale_atlas: &grayscale,
                    color_atlas: &color,
                },
            )
            .expect("update and present");

        assert_eq!(app.rebuild.rows.rebuilt_rows, vec![0, 1, 2]);
        assert_eq!(renderer.current_grid(), grid(4, 3));
        assert_eq!(app.present.presentation.width, 8);
        assert_eq!(app.present.presentation.height, 6);
    }

    #[test]
    fn update_and_present_second_frame_is_partial_and_still_presents() {
        let Some(device) = metal_device() else {
            return;
        };
        let mut term = terminal(4, 3);
        let mut shared = menlo_grid();
        let grayscale = Atlas::new(8, Format::Grayscale);
        let color = Atlas::new(8, Format::Bgra);
        let mut compositor = metal_compositor(device, 8, 6, &grayscale, &color);
        let mut renderer = FrameRenderer::new(uniforms());
        let highlights = Vec::new();
        let links = Vec::new();
        let selection = SelectionConfig::default();
        let never = [false, false, false, false];

        renderer
            .update_and_present_frame(
                &term,
                &mut shared,
                RenderDirty::Partial,
                None,
                frame_input(&highlights, &links, &selection, &never),
                FramePreparedPresentationInput {
                    compositor: &mut compositor,
                    width: 8,
                    height: 6,
                    contents_scale: 1.0,
                    grayscale_atlas: &grayscale,
                    color_atlas: &color,
                },
            )
            .expect("first frame");

        term.clear_dirty_for_tests();
        let app = renderer
            .update_and_present_frame(
                &term,
                &mut shared,
                RenderDirty::Partial,
                None,
                frame_input(&highlights, &links, &selection, &never),
                FramePreparedPresentationInput {
                    compositor: &mut compositor,
                    width: 8,
                    height: 6,
                    contents_scale: 1.0,
                    grayscale_atlas: &grayscale,
                    color_atlas: &color,
                },
            )
            .expect("second frame");

        assert!(!app.rebuild.rows.reset_contents);
        assert!(app.rebuild.rows.rebuilt_rows.is_empty());
        assert_eq!(renderer.current_grid(), grid(4, 3));
        assert_eq!(app.present.presentation.width, 8);
    }

    #[test]
    fn update_and_present_rebuild_error_skips_present_and_grid() {
        let Some(device) = metal_device() else {
            return;
        };
        let term = terminal(4, 3);
        let mut shared = menlo_grid();
        let grayscale = Atlas::new(8, Format::Grayscale);
        let color = Atlas::new(8, Format::Bgra);
        let mut compositor = metal_compositor(device, 8, 6, &grayscale, &color);
        let mut renderer = FrameRenderer::new(uniforms());
        let highlights = Vec::new();
        let links = Vec::new();
        let selection = SelectionConfig::default();
        let never: [bool; 0] = [];

        let err = renderer
            .update_and_present_frame(
                &term,
                &mut shared,
                RenderDirty::Partial,
                None,
                frame_input(&highlights, &links, &selection, &never),
                FramePreparedPresentationInput {
                    compositor: &mut compositor,
                    width: 8,
                    height: 6,
                    contents_scale: 1.0,
                    grayscale_atlas: &grayscale,
                    color_atlas: &color,
                },
            )
            .expect_err("rebuild should fail before present");

        // A rebuild-side error, not a present error, and the grid is unchanged.
        assert!(matches!(
            err,
            FramePreparedFrameError::Rebuild(FramePreparedRebuildError::PaddingExtend(_))
        ));
        assert_eq!(renderer.current_grid(), grid(0, 0));
    }

    #[test]
    fn update_and_present_present_error_does_not_advance_grid_then_self_heals() {
        let Some(device) = metal_device() else {
            return;
        };
        let term = terminal(4, 3);
        let mut shared = menlo_grid();
        let grayscale = Atlas::new(8, Format::Grayscale);
        let color = Atlas::new(8, Format::Bgra);
        let mut compositor = metal_compositor(device, 8, 6, &grayscale, &color);
        let mut renderer = FrameRenderer::new(uniforms());
        let highlights = Vec::new();
        let links = Vec::new();
        let selection = SelectionConfig::default();
        let never = [false, false, false, false];

        // Invalid contents_scale makes the compositor reject *after* the rebuild
        // already mutated the owned contents/uniforms.
        let err = renderer
            .update_and_present_frame(
                &term,
                &mut shared,
                RenderDirty::Partial,
                None,
                frame_input(&highlights, &links, &selection, &never),
                FramePreparedPresentationInput {
                    compositor: &mut compositor,
                    width: 8,
                    height: 6,
                    contents_scale: 0.0,
                    grayscale_atlas: &grayscale,
                    color_atlas: &color,
                },
            )
            .expect_err("invalid scale should fail at present");

        assert!(matches!(err, FramePreparedFrameError::Present(_)));
        // The frame did not present, so the grid stays at 0x0.
        assert_eq!(renderer.current_grid(), grid(0, 0));

        // A valid frame self-heals: stale grid -> full re-resize, presents.
        let app = renderer
            .update_and_present_frame(
                &term,
                &mut shared,
                RenderDirty::Partial,
                None,
                frame_input(&highlights, &links, &selection, &never),
                FramePreparedPresentationInput {
                    compositor: &mut compositor,
                    width: 8,
                    height: 6,
                    contents_scale: 1.0,
                    grayscale_atlas: &grayscale,
                    color_atlas: &color,
                },
            )
            .expect("self-heal frame");

        assert!(app.rebuild.rows.reset_contents);
        assert_eq!(renderer.current_grid(), grid(4, 3));
        assert_eq!(app.present.presentation.width, 8);
    }

    fn render_knobs() -> FrameRenderKnobs {
        FrameRenderKnobs {
            bold: Some(BoldColor::Color(Rgb::new(1, 2, 3))),
            alpha: 230,
            faint_opacity: 99,
            thicken: true,
            thicken_strength: 77,
            background_opacity_cells: true,
            background_opacity: 0.42,
            padding_color: WindowPaddingColor::Extend,
            overlay_alpha: 219,
        }
    }

    #[test]
    fn render_state_derives_colors_and_palette_from_terminal() {
        let term = terminal(4, 3);
        let state = FrameRenderState::from_terminal(&term);

        // Faithful extraction: the derived fields equal the terminal's effective
        // colors / palette (with the GUI path's black/white fallbacks).
        let expected_bg = term
            .color_effective(TerminalColorKind::Background)
            .map(|(r, g, b)| Rgb::new(r, g, b))
            .unwrap_or(Rgb::new(0, 0, 0));
        let expected_fg = term
            .color_effective(TerminalColorKind::Foreground)
            .map(|(r, g, b)| Rgb::new(r, g, b))
            .unwrap_or(Rgb::new(0xff, 0xff, 0xff));
        assert_eq!(state.default_bg, expected_bg);
        assert_eq!(state.default_fg, expected_fg);
        // A fresh terminal carries the default palette.
        assert_eq!(state.palette, DEFAULT_PALETTE);
        // row_never_extend is sized to the terminal rows.
        assert_eq!(state.row_never_extend.len(), 3);
    }

    #[test]
    fn render_state_rebuild_input_drives_a_frame() {
        let term = terminal(4, 3);
        let mut shared = menlo_grid();
        let mut renderer = FrameRenderer::new(uniforms());
        let state = FrameRenderState::from_terminal(&term);
        let knobs = render_knobs();

        let app = renderer
            .update_frame(
                &term,
                &mut shared,
                RenderDirty::Partial,
                None,
                state.rebuild_input(&knobs),
            )
            .expect("update frame from derived input");

        // The terminal-derived, assembled input rebuilds a full frame.
        assert!(app.rows.reset_contents);
        assert_eq!(app.rows.rebuilt_rows, vec![0, 1, 2]);
        assert_eq!(renderer.current_grid(), grid(4, 3));
    }

    #[test]
    fn render_state_derives_visible_block_cursor_overlay() {
        let term = terminal(4, 3); // default: cursor visible, Block style
        let state = FrameRenderState::from_terminal(&term);

        let (style, color) = state.cursor.expect("visible cursor");
        assert!(matches!(style, CursorStyle::Block));
        // No OSC-12 set, so the cursor color is the default-foreground fallback.
        assert_eq!(color, state.default_fg);

        let knobs = render_knobs();
        let input = state.rebuild_input(&knobs);
        let overlay = input.text_overlay.cursor.expect("overlay cursor");
        assert!(matches!(overlay.style, CursorStyle::Block));
        assert_eq!(overlay.color, state.default_fg);
        assert_eq!(input.text_overlay.screen_fg, state.default_fg);
    }

    #[test]
    fn render_state_cursor_color_comes_from_osc12() {
        let mut term = terminal(4, 3);
        // OSC 12: set the cursor color to ab/cd/ef.
        term.next_slice(b"\x1b]12;rgb:ab/cd/ef\x07")
            .expect("osc12 cursor color");
        let state = FrameRenderState::from_terminal(&term);

        let (_, color) = state.cursor.expect("visible cursor");
        assert_eq!(color, Rgb::new(0xab, 0xcd, 0xef));
        assert_ne!(color, state.default_fg);
    }

    #[test]
    fn render_state_block_sets_uniform_underline_does_not() {
        let knobs = render_knobs();

        // Block (default) → the block-cursor uniform is set.
        let block = terminal(4, 3);
        let block_state = FrameRenderState::from_terminal(&block);
        let block_input = block_state.rebuild_input(&knobs);
        assert!(block_input.cursor_uniform.block_cursor.is_some());

        // Underline (DECSCUSR 4) → no block uniform, but the overlay cursor stays.
        let mut underline = terminal(4, 3);
        underline
            .next_slice(b"\x1b[4 q")
            .expect("decscusr underline");
        let underline_state = FrameRenderState::from_terminal(&underline);
        let underline_input = underline_state.rebuild_input(&knobs);
        assert!(underline_input.cursor_uniform.block_cursor.is_none());
        assert!(underline_input.text_overlay.cursor.is_some());
    }

    #[test]
    fn render_state_hidden_cursor_has_no_overlay_or_uniform() {
        let mut term = terminal(4, 3);
        term.next_slice(b"\x1b[?25l").expect("hide cursor");
        let state = FrameRenderState::from_terminal(&term);

        assert!(state.cursor.is_none());
        let knobs = render_knobs();
        let input = state.rebuild_input(&knobs);
        assert!(input.text_overlay.cursor.is_none());
        assert!(input.cursor_uniform.block_cursor.is_none());
    }

    #[test]
    fn render_state_row_never_extend_matches_helper() {
        let term = terminal(4, 3);
        let state = FrameRenderState::from_terminal(&term);

        let expected =
            row_never_extend_bg_flags(&term.shape_run_options(), &state.palette, state.default_bg);
        assert_eq!(state.row_never_extend, expected);
        assert_eq!(state.row_never_extend.len(), 3);
    }

    #[test]
    fn render_state_default_terminal_never_extends_every_row() {
        let term = terminal(4, 3);
        let state = FrameRenderState::from_terminal(&term);

        // A blank cell is a default-background codepoint cell, so every row of a
        // default terminal never-extends (the all-false stub was behaviorally wrong).
        assert_eq!(state.row_never_extend, vec![true, true, true]);
    }

    #[test]
    fn render_state_non_default_background_row_may_extend() {
        let mut term = terminal(4, 3);
        // Row 1 (0-based): cursor to row 2 col 1, palette-red background, fill all
        // four columns so no cell carries the default background.
        term.next_slice(b"\x1b[2;1H\x1b[41mBBBB")
            .expect("red background row");
        let state = FrameRenderState::from_terminal(&term);

        // Row 1's cells all have a non-default background → it may extend (false);
        // the default-background rows around it never-extend (true).
        assert!(!state.row_never_extend[1]);
        assert!(state.row_never_extend[0]);
        assert!(state.row_never_extend[2]);
    }

    #[test]
    fn from_config_defaults_flow_through() {
        let knobs = FrameRenderKnobs::from_config(&Config::default());

        // Config-sourced defaults.
        assert!(knobs.bold.is_none());
        assert!(!knobs.thicken);
        assert_eq!(knobs.thicken_strength, 255);
        assert_eq!(knobs.background_opacity, 1.0);
        assert!(matches!(
            knobs.padding_color,
            WindowPaddingColor::Background
        ));
        // Upstream-faithful constants (no config option yet).
        assert_eq!(knobs.alpha, 255);
        assert_eq!(knobs.overlay_alpha, 255);
        assert_eq!(knobs.faint_opacity, 128);
        assert!(!knobs.background_opacity_cells);
    }

    #[test]
    fn from_config_sources_config_values() {
        let mut cfg = Config::default();
        cfg.set("font-thicken", Some("true")).unwrap();
        cfg.set("font-thicken-strength", Some("200")).unwrap();
        cfg.set("background-opacity", Some("0.7")).unwrap();
        cfg.set("bold-color", Some("bright")).unwrap();

        let knobs = FrameRenderKnobs::from_config(&cfg);
        assert!(knobs.thicken);
        assert_eq!(knobs.thicken_strength, 200);
        assert_eq!(knobs.background_opacity, 0.7);
        assert!(matches!(knobs.bold, Some(BoldColor::Bright)));
    }

    #[test]
    fn from_config_knobs_drive_a_frame() {
        let term = terminal(4, 3);
        let mut shared = menlo_grid();
        let mut renderer = FrameRenderer::new(uniforms());
        let state = FrameRenderState::from_terminal(&term);
        let knobs = FrameRenderKnobs::from_config(&Config::default());

        let app = renderer
            .update_frame(
                &term,
                &mut shared,
                RenderDirty::Partial,
                None,
                state.rebuild_input(&knobs),
            )
            .expect("update frame from config-sourced knobs");

        assert!(app.rows.reset_contents);
        assert_eq!(app.rows.rebuilt_rows, vec![0, 1, 2]);
        assert_eq!(renderer.current_grid(), grid(4, 3));
    }

    #[test]
    fn render_frame_rebuilds_from_terminal_and_config() {
        let term = terminal(4, 3);
        let mut shared = menlo_grid();
        let mut renderer = FrameRenderer::new(uniforms());

        let app = renderer
            .render_frame(
                &term,
                &mut shared,
                RenderDirty::Partial,
                None,
                &Config::default(),
            )
            .expect("render frame from terminal+config");

        assert!(app.rows.reset_contents);
        assert_eq!(app.rows.rebuilt_rows, vec![0, 1, 2]);
        assert_eq!(renderer.current_grid(), grid(4, 3));
    }

    #[test]
    fn render_frame_equals_hand_wired_path() {
        let term = terminal(4, 3);
        let config = Config::default();

        // Composed path.
        let mut shared_a = menlo_grid();
        let mut renderer_a = FrameRenderer::new(uniforms());
        let app_a = renderer_a
            .render_frame(&term, &mut shared_a, RenderDirty::Partial, None, &config)
            .expect("render_frame");

        // The explicit four-step hand-wired path on an equivalent fresh renderer.
        let mut shared_b = menlo_grid();
        let mut renderer_b = FrameRenderer::new(uniforms());
        let state = FrameRenderState::from_terminal(&term);
        let knobs = FrameRenderKnobs::from_config(&config);
        let app_b = renderer_b
            .update_frame(
                &term,
                &mut shared_b,
                RenderDirty::Partial,
                None,
                state.rebuild_input(&knobs),
            )
            .expect("update_frame");

        assert_eq!(app_a.rows.reset_contents, app_b.rows.reset_contents);
        assert_eq!(app_a.rows.rebuilt_rows, app_b.rows.rebuilt_rows);
        assert_eq!(renderer_a.current_grid(), renderer_b.current_grid());
    }

    #[test]
    fn render_and_present_frame_presents() {
        let Some(device) = metal_device() else {
            return;
        };
        let term = terminal(4, 3);
        let mut shared = menlo_grid();
        let grayscale = Atlas::new(8, Format::Grayscale);
        let color = Atlas::new(8, Format::Bgra);
        let mut compositor = metal_compositor(device, 8, 6, &grayscale, &color);
        let mut renderer = FrameRenderer::new(uniforms());

        let app = renderer
            .render_and_present_frame(
                &term,
                &mut shared,
                RenderDirty::Partial,
                None,
                &Config::default(),
                FramePreparedPresentationInput {
                    compositor: &mut compositor,
                    width: 8,
                    height: 6,
                    contents_scale: 1.0,
                    grayscale_atlas: &grayscale,
                    color_atlas: &color,
                },
            )
            .expect("render and present from terminal+config");

        assert_eq!(app.rebuild.rows.rebuilt_rows, vec![0, 1, 2]);
        assert_eq!(renderer.current_grid(), grid(4, 3));
        assert_eq!(app.present.presentation.width, 8);
    }
}
