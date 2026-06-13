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
    FrameCustomShaderInput, FramePaddingExtendInput, FramePreparedFrameApplication,
    FramePreparedFrameError, FramePreparedPresentationInput, FramePreparedRebuildApplication,
    FramePreparedRebuildError, FramePreparedRebuildInput, FramePreparedRebuildTargets,
    FrameRebuildUniformInput, FrameSnapshotBlockCursorUniformInput,
    FrameSnapshotCursorOverlayInput, FrameSnapshotCursorUniformInput, FrameSnapshotRowFormatInput,
    FrameSnapshotTextOverlayInput, FrameTerminalSnapshot, RenderDirty,
};
use crate::renderer::image::{BackgroundImageState, ImageState};
use crate::renderer::metal::pipeline::MetalPipeline;
use crate::renderer::metal::shaders::MetalUniforms;
use crate::renderer::metal::texture::MetalTexture;
use crate::renderer::shadertoy::CustomShaderUniforms;
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

    pub(crate) fn update_and_present_frame_with_images(
        &mut self,
        terminal: &Terminal,
        grid: &mut SharedGrid,
        images: &mut ImageState<MetalTexture>,
        background: &mut BackgroundImageState<MetalTexture>,
        dirty: RenderDirty,
        preedit: Option<Preedit>,
        input: FramePreparedRebuildInput<'_>,
        presentation: FramePreparedPresentationInput<'_>,
    ) -> Result<FramePreparedFrameApplication, FramePreparedFrameError> {
        let snapshot = FrameTerminalSnapshot::collect(terminal, self.current_grid, dirty, preedit);

        self.row_dirty.clear();
        self.row_dirty.extend_from_slice(&snapshot.row_dirty);

        let app = snapshot.rebuild_and_present_frame_with_images(
            FramePreparedRebuildTargets {
                contents: &mut self.contents,
                grid,
                row_dirty: &mut self.row_dirty,
                uniforms: &mut self.uniforms,
            },
            input,
            images,
            background,
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

    pub(crate) fn render_and_present_frame_with_images(
        &mut self,
        terminal: &Terminal,
        grid: &mut SharedGrid,
        images: &mut ImageState<MetalTexture>,
        background: &mut BackgroundImageState<MetalTexture>,
        dirty: RenderDirty,
        preedit: Option<Preedit>,
        config: &Config,
        presentation: FramePreparedPresentationInput<'_>,
    ) -> Result<FramePreparedFrameApplication, FramePreparedFrameError> {
        self.render_and_present_frame_with_images_and_link_ranges(
            terminal,
            grid,
            images,
            background,
            Vec::new(),
            dirty,
            preedit,
            config,
            presentation,
        )
    }

    pub(crate) fn render_and_present_frame_with_images_and_link_ranges(
        &mut self,
        terminal: &Terminal,
        grid: &mut SharedGrid,
        images: &mut ImageState<MetalTexture>,
        background: &mut BackgroundImageState<MetalTexture>,
        link_ranges: Vec<Vec<[u16; 2]>>,
        dirty: RenderDirty,
        preedit: Option<Preedit>,
        config: &Config,
        presentation: FramePreparedPresentationInput<'_>,
    ) -> Result<FramePreparedFrameApplication, FramePreparedFrameError> {
        self.render_and_present_frame_with_images_and_link_ranges_and_cursor_options(
            terminal,
            grid,
            images,
            background,
            link_ranges,
            dirty,
            preedit,
            config,
            presentation,
            FrameCursorOptions::default(),
        )
    }

    pub(crate) fn render_and_present_frame_with_images_and_link_ranges_and_cursor_options(
        &mut self,
        terminal: &Terminal,
        grid: &mut SharedGrid,
        images: &mut ImageState<MetalTexture>,
        background: &mut BackgroundImageState<MetalTexture>,
        link_ranges: Vec<Vec<[u16; 2]>>,
        dirty: RenderDirty,
        preedit: Option<Preedit>,
        config: &Config,
        presentation: FramePreparedPresentationInput<'_>,
        cursor_options: FrameCursorOptions,
    ) -> Result<FramePreparedFrameApplication, FramePreparedFrameError> {
        background.update_from_config(config);
        let mut state =
            FrameRenderState::from_terminal_with_cursor_options(terminal, cursor_options);
        state.link_ranges = link_ranges;
        let knobs = FrameRenderKnobs::from_config(config);
        let input = state.rebuild_input(&knobs);
        self.update_and_present_frame_with_images(
            terminal,
            grid,
            images,
            background,
            dirty,
            preedit,
            input,
            presentation,
        )
    }

    pub(crate) fn render_and_present_frame_with_images_and_custom_shaders(
        &mut self,
        terminal: &Terminal,
        grid: &mut SharedGrid,
        images: &mut ImageState<MetalTexture>,
        background: &mut BackgroundImageState<MetalTexture>,
        custom_uniforms: &mut CustomShaderUniforms,
        custom_input: FrameCustomShaderInput,
        custom_pipelines: &[&MetalPipeline],
        dirty: RenderDirty,
        preedit: Option<Preedit>,
        config: &Config,
        presentation: FramePreparedPresentationInput<'_>,
    ) -> Result<FramePreparedFrameApplication, FramePreparedFrameError> {
        self.render_and_present_frame_with_images_and_custom_shaders_and_link_ranges(
            terminal,
            grid,
            images,
            background,
            custom_uniforms,
            custom_input,
            custom_pipelines,
            Vec::new(),
            dirty,
            preedit,
            config,
            presentation,
        )
    }

    pub(crate) fn render_and_present_frame_with_images_and_custom_shaders_and_link_ranges(
        &mut self,
        terminal: &Terminal,
        grid: &mut SharedGrid,
        images: &mut ImageState<MetalTexture>,
        background: &mut BackgroundImageState<MetalTexture>,
        custom_uniforms: &mut CustomShaderUniforms,
        custom_input: FrameCustomShaderInput,
        custom_pipelines: &[&MetalPipeline],
        link_ranges: Vec<Vec<[u16; 2]>>,
        dirty: RenderDirty,
        preedit: Option<Preedit>,
        config: &Config,
        presentation: FramePreparedPresentationInput<'_>,
    ) -> Result<FramePreparedFrameApplication, FramePreparedFrameError> {
        self.render_and_present_frame_with_images_and_custom_shaders_and_link_ranges_and_cursor_options(
            terminal,
            grid,
            images,
            background,
            custom_uniforms,
            custom_input,
            custom_pipelines,
            link_ranges,
            dirty,
            preedit,
            config,
            presentation,
            FrameCursorOptions::default(),
        )
    }

    pub(crate) fn render_and_present_frame_with_images_and_custom_shaders_and_link_ranges_and_cursor_options(
        &mut self,
        terminal: &Terminal,
        grid: &mut SharedGrid,
        images: &mut ImageState<MetalTexture>,
        background: &mut BackgroundImageState<MetalTexture>,
        custom_uniforms: &mut CustomShaderUniforms,
        custom_input: FrameCustomShaderInput,
        custom_pipelines: &[&MetalPipeline],
        link_ranges: Vec<Vec<[u16; 2]>>,
        dirty: RenderDirty,
        preedit: Option<Preedit>,
        config: &Config,
        presentation: FramePreparedPresentationInput<'_>,
        cursor_options: FrameCursorOptions,
    ) -> Result<FramePreparedFrameApplication, FramePreparedFrameError> {
        background.update_from_config(config);
        let mut state =
            FrameRenderState::from_terminal_with_cursor_options(terminal, cursor_options);
        state.link_ranges = link_ranges;
        state.update_custom_shader_uniforms_from_state(custom_uniforms);
        let knobs = FrameRenderKnobs::from_config(config);
        let input = state.rebuild_input(&knobs);
        self.update_and_present_frame_with_images_and_custom_shaders(
            terminal,
            grid,
            images,
            background,
            custom_uniforms,
            custom_input,
            custom_pipelines,
            dirty,
            preedit,
            input,
            presentation,
        )
    }

    /// Drive the screen-size + font-grid uniforms from the live surface (Issue 802 / Exp 18):
    /// the orthographic `projection_matrix`, `screen_size`, and the cell pixel size. The rebuild
    /// updates `grid_size`/contents on top but touches none of these, so without this call the
    /// projection is identity and glyphs render off-screen.
    pub(crate) fn update_screen(
        &mut self,
        size: crate::renderer::size::Size,
        grid: GridSize,
        metrics: &crate::font::metrics::Metrics,
    ) {
        self.uniforms.update_screen_size(size, grid);
        self.uniforms.update_font_grid(metrics);
    }

    pub(crate) fn update_and_present_frame_with_images_and_custom_shaders(
        &mut self,
        terminal: &Terminal,
        grid: &mut SharedGrid,
        images: &mut ImageState<MetalTexture>,
        background: &mut BackgroundImageState<MetalTexture>,
        custom_uniforms: &mut CustomShaderUniforms,
        custom_input: FrameCustomShaderInput,
        custom_pipelines: &[&MetalPipeline],
        dirty: RenderDirty,
        preedit: Option<Preedit>,
        input: FramePreparedRebuildInput<'_>,
        presentation: FramePreparedPresentationInput<'_>,
    ) -> Result<FramePreparedFrameApplication, FramePreparedFrameError> {
        let snapshot = FrameTerminalSnapshot::collect(terminal, self.current_grid, dirty, preedit);

        self.row_dirty.clear();
        self.row_dirty.extend_from_slice(&snapshot.row_dirty);

        let app = snapshot.rebuild_and_present_frame_with_images_and_custom_shaders(
            FramePreparedRebuildTargets {
                contents: &mut self.contents,
                grid,
                row_dirty: &mut self.row_dirty,
                uniforms: &mut self.uniforms,
            },
            input,
            images,
            background,
            custom_uniforms,
            custom_input,
            custom_pipelines,
            presentation,
        )?;

        self.current_grid = snapshot.terminal_grid;
        Ok(app)
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

/// The render knobs (Issue 801, Exp 842/846/848). `from_config` sources most from
/// a `Config`: `bold`, `background_opacity`, `padding_color`, `thicken`/
/// `thicken_strength` (Exp 845), and `faint_opacity`/`background_opacity_cells`
/// (Exp 848), and `cursor_opacity` (Exp 60). Only `alpha` and `overlay_alpha`
/// have no `Config` option — they are the faithful opaque `255` (upstream
/// hardcodes non-faint text alpha to 255).
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
    pub(crate) cursor_overlay_alpha: u8,
    pub(crate) selection_config: SelectionConfig,
}

impl FrameRenderKnobs {
    /// Source the render knobs from a `Config`. `faint_opacity` converts the f64
    /// `faint-opacity` to the `u8` knob — clamped to `[0, 1]` at this use site
    /// (roastty has no config finalize step) and `ceil(x × 255)` (matching upstream
    /// `generic.zig`). `cursor_opacity` uses the same clamp-and-ceil conversion
    /// for the cursor overlay. Only `alpha`/`overlay_alpha` are constants: the
    /// faithful opaque `255` (upstream hardcodes non-faint text alpha to 255).
    pub(crate) fn from_config(config: &Config) -> Self {
        Self {
            bold: config.bold_color.map(|c| c.to_terminal()),
            alpha: 255,
            faint_opacity: (config.faint_opacity.clamp(0.0, 1.0) * 255.0).ceil() as u8,
            thicken: config.font_thicken,
            thicken_strength: config.font_thicken_strength,
            background_opacity_cells: config.background_opacity_cells,
            background_opacity: config.background_opacity,
            padding_color: config.window_padding_color,
            overlay_alpha: 255,
            cursor_overlay_alpha: (config.cursor_opacity.clamp(0.0, 1.0) * 255.0).ceil() as u8,
            selection_config: SelectionConfig::from_config(config),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameCursorOptions {
    pub(crate) focused: bool,
    pub(crate) blink_visible: bool,
}

impl Default for FrameCursorOptions {
    fn default() -> Self {
        Self {
            focused: true,
            blink_visible: true,
        }
    }
}

/// Render data derived from the live terminal — the effective default fg/bg,
/// palette, cursor, and per-row never-extend flags — plus the dynamic buffers the
/// rebuild input borrows. The remaining stubs (until their own slices):
/// `highlights`/`link_ranges` empty.
pub(crate) struct FrameRenderState {
    default_fg: Rgb,
    default_bg: Rgb,
    palette: Palette,
    // `Some((style, color))` when the terminal cursor is visible; `None` otherwise.
    cursor: Option<(CursorStyle, Rgb)>,
    screen_fg: Rgb,
    highlights: Vec<Vec<Highlight>>,
    link_ranges: Vec<Vec<[u16; 2]>>,
    row_never_extend: Vec<bool>,
}

impl FrameRenderState {
    /// Derive the effective colors and palette from the terminal, mirroring the
    /// existing GUI render path (`render_state_from_terminal`): background → black
    /// fallback, foreground → white fallback, the 256-entry palette. The dynamic
    /// buffers are empty/default placeholders for later slices.
    pub(crate) fn from_terminal(terminal: &Terminal) -> Self {
        Self::from_terminal_with_cursor_options(terminal, FrameCursorOptions::default())
    }

    pub(crate) fn from_terminal_with_cursor_options(
        terminal: &Terminal,
        cursor_options: FrameCursorOptions,
    ) -> Self {
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

        // The cursor is derived only when the renderer should draw it. This mirrors
        // upstream `renderer/cursor.zig` for the live inputs Roastty currently has:
        // viewport presence, terminal visibility, focus hollowing, and blink-visible state.
        let cursor = cursor_style_from_terminal(terminal, cursor_options).map(|style| {
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
                selection_config: &knobs.selection_config,
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
                        alpha: knobs.cursor_overlay_alpha,
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

    pub(crate) fn update_custom_shader_uniforms_from_state(
        &self,
        uniforms: &mut CustomShaderUniforms,
    ) {
        uniforms.update_palette(&self.palette);
        uniforms.update_state_colors(
            self.default_bg,
            self.default_fg,
            self.cursor.map(|(_, color)| color),
            None,
            None,
            None,
        );
        let (visible, style) = self
            .cursor
            .map(|(style, _)| (true, style))
            .unwrap_or((false, CursorStyle::Block));
        uniforms.update_cursor_style(visible, style);
    }
}

fn cursor_style_from_terminal(
    terminal: &Terminal,
    options: FrameCursorOptions,
) -> Option<CursorStyle> {
    terminal.cursor_viewport_position()?;

    if !terminal.cursor_visible() {
        return None;
    }

    if !options.focused {
        return Some(CursorStyle::BlockHollow);
    }

    if terminal.cursor_blinking() && !options.blink_visible {
        return None;
    }

    Some(CursorStyle::from_terminal(terminal.cursor_visual_style()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WindowPaddingColor;
    use crate::font::run::Wide;
    use crate::renderer::cell::SelectionColor;
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

    fn first_pixel(bytes: &[u8]) -> [u8; 4] {
        [bytes[0], bytes[1], bytes[2], bytes[3]]
    }

    fn pixel_at(bytes: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
        let offset = (y * width + x) * 4;
        [
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]
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

    fn temp_image_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "roastty-frame-renderer-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).expect("create temp image dir");
        dir
    }

    fn write_png(path: &std::path::Path, rgba: [u8; 4]) {
        image::save_buffer_with_format(
            path,
            &rgba,
            1,
            1,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .expect("write png");
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
                    alpha: 211,
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
            cursor_overlay_alpha: 211,
            selection_config: SelectionConfig::default(),
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
        assert_eq!(overlay.alpha, knobs.cursor_overlay_alpha);
        assert_eq!(input.text_overlay.screen_fg, state.default_fg);
        assert_eq!(input.text_overlay.alpha, knobs.overlay_alpha);
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
    fn cursor_blink_render_state_hides_focused_blinking_cursor_when_not_visible() {
        let term = terminal(4, 3);
        let state = FrameRenderState::from_terminal_with_cursor_options(
            &term,
            FrameCursorOptions {
                focused: true,
                blink_visible: false,
            },
        );

        assert!(state.cursor.is_none());
        let knobs = render_knobs();
        let input = state.rebuild_input(&knobs);
        assert!(input.text_overlay.cursor.is_none());
        assert!(input.cursor_uniform.block_cursor.is_none());
    }

    #[test]
    fn cursor_blink_render_state_shows_focused_blinking_cursor_when_visible() {
        let term = terminal(4, 3);
        let state = FrameRenderState::from_terminal_with_cursor_options(
            &term,
            FrameCursorOptions {
                focused: true,
                blink_visible: true,
            },
        );

        let (style, _) = state.cursor.expect("visible cursor");
        assert!(matches!(style, CursorStyle::Block));
    }

    #[test]
    fn cursor_blink_render_state_unfocused_cursor_is_hollow_even_when_blink_hidden() {
        let term = terminal(4, 3);
        let state = FrameRenderState::from_terminal_with_cursor_options(
            &term,
            FrameCursorOptions {
                focused: false,
                blink_visible: false,
            },
        );

        let (style, _) = state.cursor.expect("unfocused cursor");
        assert!(matches!(style, CursorStyle::BlockHollow));
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
        assert_eq!(knobs.cursor_overlay_alpha, 255);
        assert_eq!(knobs.faint_opacity, 128);
        assert!(!knobs.background_opacity_cells);
        assert_eq!(knobs.selection_config, SelectionConfig::default());
    }

    #[test]
    fn from_config_sources_config_values() {
        let mut cfg = Config::default();
        cfg.set("font-thicken", Some("true")).unwrap();
        cfg.set("font-thicken-strength", Some("200")).unwrap();
        cfg.set("background-opacity", Some("0.7")).unwrap();
        cfg.set("bold-color", Some("bright")).unwrap();
        cfg.set("search-background", Some("#010203")).unwrap();
        cfg.set("search-foreground", Some("cell-background"))
            .unwrap();

        let knobs = FrameRenderKnobs::from_config(&cfg);
        assert!(knobs.thicken);
        assert_eq!(knobs.thicken_strength, 200);
        assert_eq!(knobs.background_opacity, 0.7);
        assert!(matches!(knobs.bold, Some(BoldColor::Bright)));
        assert_eq!(
            knobs.selection_config.search_background,
            SelectionColor::Color(Rgb::new(1, 2, 3))
        );
        assert_eq!(
            knobs.selection_config.search_foreground,
            SelectionColor::CellBackground
        );
    }

    #[test]
    fn from_config_sources_opacity_options() {
        let mut cfg = Config::default();
        cfg.set("background-opacity-cells", Some("true")).unwrap();
        cfg.set("faint-opacity", Some("0.0")).unwrap();
        let knobs = FrameRenderKnobs::from_config(&cfg);
        assert!(knobs.background_opacity_cells);
        assert_eq!(knobs.faint_opacity, 0);

        // 1.0 → 255 (ceil), and an out-of-range raw value is clamped at this use site.
        cfg.set("faint-opacity", Some("1.0")).unwrap();
        assert_eq!(FrameRenderKnobs::from_config(&cfg).faint_opacity, 255);
        cfg.set("faint-opacity", Some("2.0")).unwrap();
        assert_eq!(FrameRenderKnobs::from_config(&cfg).faint_opacity, 255);
    }

    #[test]
    fn cursor_opacity_clamps_to_cursor_overlay_alpha_only() {
        let mut cfg = Config::default();
        cfg.set("cursor-opacity", Some("0.5")).unwrap();
        let knobs = FrameRenderKnobs::from_config(&cfg);
        assert_eq!(knobs.cursor_overlay_alpha, 128);
        assert_eq!(knobs.overlay_alpha, 255);
        assert_eq!(knobs.alpha, 255);

        cfg.set("cursor-opacity", Some("-1.0")).unwrap();
        assert_eq!(FrameRenderKnobs::from_config(&cfg).cursor_overlay_alpha, 0);
        cfg.set("cursor-opacity", Some("2.0")).unwrap();
        assert_eq!(
            FrameRenderKnobs::from_config(&cfg).cursor_overlay_alpha,
            255
        );
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
                },
            )
            .expect("render and present from terminal+config");

        assert_eq!(app.rebuild.rows.rebuilt_rows, vec![0, 1, 2]);
        assert_eq!(renderer.current_grid(), grid(4, 3));
        assert_eq!(app.present.presentation.width, 8);
    }

    #[test]
    fn live_kitty_image_frame_renderer_presents_terminal_placement_and_unloads() {
        let Some(device) = metal_device() else {
            return;
        };
        let mut shared = menlo_grid();
        let cell = shared.cell_size();
        let (cw, ch) = (cell.width as usize, cell.height as usize);
        let mut term = Terminal::init(1, 1, None).expect("terminal");
        term.set_kitty_cell_metrics_for_tests(1, 1);
        term.next_slice(b"\x1b[?25l\x1b_Ga=T,f=32,s=1,v=1,i=7,p=4,C=1;/wAA/w==\x1b\\")
            .expect("hide cursor + transmit and display image");
        let placements = crate::kitty_render_placement_snapshots(&term);
        assert_eq!(placements.len(), 1);

        let mut images = ImageState::<MetalTexture>::default();
        images.update_kitty_from_render_placements(&placements);
        let mut compositor = MetalFrameCompositor::new(MetalFrameCompositorOptions {
            device,
            width: cw,
            height: ch,
            pixel_format: MetalPixelFormat::Bgra8Unorm,
            storage_mode: MetalStorageMode::Shared,
            resource_options: MetalResourceOptions::image(MetalStorageMode::Shared),
            grayscale_atlas: &shared.atlas_grayscale,
            color_atlas: &shared.atlas_color,
        })
        .expect("compositor");

        use crate::renderer::size::{CellSize, Padding, ScreenSize, Size};
        let mut u = MetalUniforms::test_with_grid(
            [cw as u16, ch as u16],
            [1, 1],
            [cw as f32, ch as f32],
            [0.0; 4],
            0,
            [0, 0, 0, 255],
        );
        u.update_screen_size(
            Size {
                screen: ScreenSize {
                    width: cw as u32,
                    height: ch as u32,
                },
                cell: CellSize {
                    width: cw as u32,
                    height: ch as u32,
                },
                padding: Padding {
                    top: 0,
                    bottom: 0,
                    right: 0,
                    left: 0,
                },
            },
            GridSize {
                columns: 1,
                rows: 1,
            },
        );
        let mut renderer = FrameRenderer::new(u);
        let mut background = BackgroundImageState::<MetalTexture>::default();
        renderer
            .render_and_present_frame_with_images(
                &term,
                &mut shared,
                &mut images,
                &mut background,
                RenderDirty::Full,
                None,
                &Config::default(),
                FramePreparedPresentationInput {
                    compositor: &mut compositor,
                    width: cw,
                    height: ch,
                    contents_scale: 1.0,
                },
            )
            .expect("image frame should present");
        assert_eq!(first_pixel(&compositor.target_bytes()), [0, 0, 255, 255]);

        let mut empty = Terminal::init(1, 1, None).expect("empty terminal");
        empty.set_kitty_cell_metrics_for_tests(1, 1);
        empty
            .next_slice(b"\x1b[?25l")
            .expect("hide cursor on empty terminal");
        let empty_placements = crate::kitty_render_placement_snapshots(&empty);
        assert!(empty_placements.is_empty());
        images.update_kitty_from_render_placements(&empty_placements);
        renderer
            .render_and_present_frame_with_images(
                &empty,
                &mut shared,
                &mut images,
                &mut background,
                RenderDirty::Full,
                None,
                &Config::default(),
                FramePreparedPresentationInput {
                    compositor: &mut compositor,
                    width: cw,
                    height: ch,
                    contents_scale: 1.0,
                },
            )
            .expect("empty frame should present");
        assert_eq!(first_pixel(&compositor.target_bytes()), [0, 0, 0, 255]);
    }

    #[test]
    fn live_background_image_frame_renderer_presents_config_path_and_unloads() {
        let Some(device) = metal_device() else {
            return;
        };
        let mut shared = menlo_grid();
        let cell = shared.cell_size();
        let (cw, ch) = (cell.width as usize, cell.height as usize);
        let width = cw * 2;
        let mut term = Terminal::init(1, 1, None).expect("terminal");
        term.next_slice(b"\x1b[?25l").expect("hide cursor");
        let mut compositor = MetalFrameCompositor::new(MetalFrameCompositorOptions {
            device,
            width,
            height: ch,
            pixel_format: MetalPixelFormat::Bgra8Unorm,
            storage_mode: MetalStorageMode::Shared,
            resource_options: MetalResourceOptions::image(MetalStorageMode::Shared),
            grayscale_atlas: &shared.atlas_grayscale,
            color_atlas: &shared.atlas_color,
        })
        .expect("compositor");

        use crate::renderer::size::{CellSize, Padding, ScreenSize, Size};
        let mut u = MetalUniforms::test_with_grid(
            [width as u16, ch as u16],
            [1, 1],
            [cw as f32, ch as f32],
            [0.0; 4],
            0,
            [0, 0, 0, 255],
        );
        u.update_screen_size(
            Size {
                screen: ScreenSize {
                    width: width as u32,
                    height: ch as u32,
                },
                cell: CellSize {
                    width: cw as u32,
                    height: ch as u32,
                },
                padding: Padding::default(),
            },
            GridSize {
                columns: 1,
                rows: 1,
            },
        );

        let dir = temp_image_dir("background-image");
        let path = dir.join("bg.png");
        write_png(&path, [255, 0, 0, 255]);
        let mut config = Config::default();
        config
            .set("background-image", Some(&path.to_string_lossy()))
            .expect("set background image");
        config.bg_image_fit = crate::config::BackgroundImageFit::Stretch;
        let mut renderer = FrameRenderer::new(u);
        let mut images = ImageState::<MetalTexture>::default();
        let mut background = BackgroundImageState::<MetalTexture>::default();

        renderer
            .render_and_present_frame_with_images(
                &term,
                &mut shared,
                &mut images,
                &mut background,
                RenderDirty::Full,
                None,
                &config,
                FramePreparedPresentationInput {
                    compositor: &mut compositor,
                    width,
                    height: ch,
                    contents_scale: 1.0,
                },
            )
            .expect("background image frame should present");
        let image_pixel = pixel_at(&compositor.target_bytes(), width, cw, 0);
        assert_eq!(image_pixel[3], 255);
        assert!(
            image_pixel[2] > 0,
            "background image should add red contribution: {image_pixel:?}"
        );

        renderer
            .render_and_present_frame_with_images(
                &term,
                &mut shared,
                &mut images,
                &mut background,
                RenderDirty::Full,
                None,
                &Config::default(),
                FramePreparedPresentationInput {
                    compositor: &mut compositor,
                    width,
                    height: ch,
                    contents_scale: 1.0,
                },
            )
            .expect("reset background frame should present");
        assert_eq!(
            pixel_at(&compositor.target_bytes(), width, cw, 0),
            [0, 0, 0, 255]
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Issue 802 / Exp 17: the present must sample the **grid's** rasterized atlas, so glyph
    /// pixels actually reach the GPU target. This reads back the rendered target and asserts it
    /// is **non-uniform** — i.e. the glyph drew foreground pixels over the background. With the
    /// pre-Exp-17 bug (present sampled a separate empty atlas), the target would be a uniform
    /// background and this fails. The cursor is hidden (`\x1b[?25l`) so the only source of
    /// variation is the glyph itself.
    #[test]
    fn present_samples_grid_atlas_so_glyphs_reach_the_target() {
        let Some(device) = metal_device() else {
            return;
        };
        let mut shared = menlo_grid();
        let cell = shared.cell_size();
        let (cw, ch) = (cell.width as usize, cell.height as usize);
        // 2x1 terminal, cursor hidden, two visible glyphs; target sized exactly to the grid so
        // any non-background pixel must come from a rendered glyph (no padding region).
        let mut term = Terminal::init(2, 1, None).expect("terminal");
        term.next_slice(b"\x1b[?25l\x1b[91mWW")
            .expect("hide cursor + bright fg + write");
        let (w, h) = (cw * 2, ch);
        let mut compositor = MetalFrameCompositor::new(MetalFrameCompositorOptions {
            device,
            width: w,
            height: h,
            pixel_format: MetalPixelFormat::Bgra8Unorm,
            storage_mode: MetalStorageMode::Shared,
            resource_options: MetalResourceOptions::image(MetalStorageMode::Shared),
            grayscale_atlas: &shared.atlas_grayscale,
            color_atlas: &shared.atlas_color,
        })
        .expect("compositor");
        // Uniforms must match the grid + target (the rebuild updates grid_size but NOT the
        // projection/screen_size/cell_size): set the ortho projection so the grid maps onto the
        // target, black bg so the glyph stands out.
        use crate::renderer::size::{CellSize, Padding, ScreenSize, Size};
        let mut u = MetalUniforms::test_with_grid(
            [w as u16, h as u16],
            [2, 1],
            [cw as f32, ch as f32],
            [0.0; 4],
            0,
            [0, 0, 0, 255],
        );
        u.update_screen_size(
            Size {
                screen: ScreenSize {
                    width: w as u32,
                    height: h as u32,
                },
                cell: CellSize {
                    width: cw as u32,
                    height: ch as u32,
                },
                padding: Padding {
                    top: 0,
                    bottom: 0,
                    right: 0,
                    left: 0,
                },
            },
            GridSize {
                columns: 2,
                rows: 1,
            },
        );
        let mut renderer = FrameRenderer::new(u);
        renderer
            .render_and_present_frame(
                &term,
                &mut shared,
                RenderDirty::Full,
                None,
                &Config::default(),
                FramePreparedPresentationInput {
                    compositor: &mut compositor,
                    width: w,
                    height: h,
                    contents_scale: 1.0,
                },
            )
            .expect("render and present");

        let bytes = compositor.target_bytes();
        // Assert a specific bright-foreground glyph pixel reached the GPU — the target is BGRA,
        // the bg is pure black, and the glyph fg is bright red (`\x1b[91m`), so a high red
        // channel (byte index 2) can only come from a rendered glyph. This is stronger than a
        // bare "non-uniform" check: it survives later setup changes (padding, a visible cursor,
        // per-cell bg) that could otherwise make a uniform-vs-not test pass without a glyph.
        let glyph_px = bytes.chunks(4).filter(|px| px[2] > 100).count();
        assert!(
            glyph_px > 0,
            "no bright-foreground glyph pixel reached the GPU target — the present sampled an \
             empty atlas instead of the grid's rasterized one",
        );
    }

    /// Issue 802 / Exp 18: `update_screen` drives the geometry uniforms the rebuild never sets —
    /// the ortho `projection_matrix`, `screen_size`, and the `cell_size` (from the font metrics).
    /// Without these the projection is identity and glyphs render off-screen. Headless (no GPU).
    #[test]
    fn update_screen_sets_projection_screen_and_cell() {
        use crate::renderer::metal::shaders::ortho2d;
        use crate::renderer::size::{CellSize, Padding, ScreenSize, Size};
        let shared = menlo_grid();
        let mut renderer = FrameRenderer::new(uniforms());
        renderer.update_screen(
            Size {
                screen: ScreenSize {
                    width: 200,
                    height: 100,
                },
                cell: CellSize {
                    width: 10,
                    height: 20,
                },
                padding: Padding::default(),
            },
            GridSize {
                columns: 20,
                rows: 5,
            },
            &shared.metrics,
        );
        let u = renderer.uniforms();
        assert_eq!(u.screen_size, [200.0, 100.0]);
        // cell_size comes from update_font_grid (the metrics), not the Size.cell argument.
        assert_eq!(
            u.cell_size,
            [
                shared.metrics.cell_width as f32,
                shared.metrics.cell_height as f32
            ]
        );
        // padding 0 → terminal == screen → ortho2d(0, 200, 100, 0); not the identity it starts as.
        assert_eq!(u.projection_matrix, ortho2d(0.0, 200.0, 100.0, 0.0));
    }
}
