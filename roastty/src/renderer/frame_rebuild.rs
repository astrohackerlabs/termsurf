#![allow(dead_code)]
// Frame rebuild planning is consumed by later renderer integration slices.

//! Renderer frame rebuild planning.
//!
//! Faithful value-level port of the front half of upstream
//! `renderer/generic.zig`'s `rebuildCells`: decide whether the cell contents
//! grid must resize, whether the rebuild is full or row-level, which rows should
//! be rebuilt/cleared/marked clean, and whether preedit text masks the cursor
//! row. Actual terminal row formatting, glyph emission, cursor drawing, and
//! `Contents` mutation remain later integration work.

use crate::config::WindowPaddingColor;
use crate::font::atlas::Atlas;
use crate::font::codepoint_resolver::ResolverRenderError;
use crate::font::run::{shape_row_cached, RunOptions, Wide};
use crate::font::shared_grid::SharedGrid;
use crate::renderer::cell::{
    add_cursor, add_preedit, rebuild_bg_row, rebuild_row, Contents, Highlight, SelectionConfig,
};
use crate::renderer::cursor::Style as CursorStyle;
use crate::renderer::image::ImageState;
use crate::renderer::metal::compositor::{
    MetalFrameCompositor, MetalFrameCompositorError, MetalFrameInput, MetalFramePresentation,
};
use crate::renderer::metal::shaders::MetalUniforms;
use crate::renderer::metal::texture::MetalTexture;
use crate::renderer::shader::CellTextVertex;
use crate::renderer::shadertoy::CustomShaderUniforms;
use crate::renderer::size::{GridSize, Unit};
use crate::renderer::state::{Preedit, PreeditRange};
use crate::terminal::color::{Palette, Rgb};
use crate::terminal::point::Coordinate;
use crate::terminal::style::BoldColor;
use crate::terminal::terminal::Terminal;
use std::collections::HashSet;

/// Terminal render-state dirty mode. Mirrors upstream
/// `terminal.RenderState.Dirty`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderDirty {
    Clean,
    Partial,
    Full,
}

/// Input to the frame rebuild planner. `row_dirty` is indexed by viewport row
/// after any terminal-state/search/link updates have already run.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameRebuildInput<'a> {
    pub(crate) current_grid: GridSize,
    pub(crate) terminal_grid: GridSize,
    pub(crate) dirty: RenderDirty,
    pub(crate) row_dirty: &'a [bool],
    pub(crate) preedit: Option<&'a Preedit>,
    pub(crate) cursor_viewport: Option<Coordinate>,
}

#[derive(Debug, Clone)]
pub(crate) struct FrameTerminalSnapshot {
    pub(crate) current_grid: GridSize,
    pub(crate) terminal_grid: GridSize,
    pub(crate) dirty: RenderDirty,
    pub(crate) row_dirty: Vec<bool>,
    pub(crate) rows: Vec<RunOptions>,
    pub(crate) preedit: Option<Preedit>,
    pub(crate) cursor_viewport: Option<Coordinate>,
}

impl FrameTerminalSnapshot {
    pub(crate) fn collect(
        terminal: &Terminal,
        current_grid: GridSize,
        dirty: RenderDirty,
        preedit: Option<Preedit>,
    ) -> Self {
        let terminal_grid = GridSize {
            columns: terminal.columns(),
            rows: terminal.rows(),
        };
        let row_dirty = terminal
            .render_rows_snapshot()
            .into_iter()
            .map(|row| row.dirty)
            .collect();
        let rows = terminal.shape_run_options();
        // Issue 802 / Exp 24: the cursor's VIEWPORT position (None when scrolled off-viewport),
        // so the cursor block isn't drawn on a scrollback history row. Still bounded by the render
        // grid (defensive, matching the prior `cursor_viewport` check).
        let cursor_viewport = terminal
            .cursor_viewport_position()
            .map(|(x, y)| Coordinate::new(x, u32::from(y)))
            .filter(|c| c.x < terminal_grid.columns && c.y < u32::from(terminal_grid.rows));

        Self {
            current_grid,
            terminal_grid,
            dirty,
            row_dirty,
            rows,
            preedit,
            cursor_viewport,
        }
    }

    pub(crate) fn rebuild_input(&self) -> FrameRebuildInput<'_> {
        FrameRebuildInput {
            current_grid: self.current_grid,
            terminal_grid: self.terminal_grid,
            dirty: self.dirty,
            row_dirty: &self.row_dirty,
            preedit: self.preedit.as_ref(),
            cursor_viewport: self.cursor_viewport,
        }
    }

    pub(crate) fn build_plan(&self) -> Result<FrameRebuildPlan, FrameRebuildPlanError> {
        FrameRebuildPlan::build(self.rebuild_input())
    }

    pub(crate) fn row_format_input<'a>(
        &'a self,
        input: FrameSnapshotRowFormatInput<'a>,
    ) -> FrameRowFormatInput<'a> {
        FrameRowFormatInput {
            rows: &self.rows,
            highlights: input.highlights,
            link_ranges: input.link_ranges,
            selection_config: input.selection_config,
            default_fg: input.default_fg,
            default_bg: input.default_bg,
            palette: input.palette,
            bold: input.bold,
            alpha: input.alpha,
            faint_opacity: input.faint_opacity,
            thicken: input.thicken,
            thicken_strength: input.thicken_strength,
            background_opacity_cells: input.background_opacity_cells,
            background_opacity: input.background_opacity,
        }
    }

    pub(crate) fn text_overlay_input(
        &self,
        input: FrameSnapshotTextOverlayInput,
    ) -> FrameTextOverlayInput<'_> {
        let cursor = cursor_grid_pos(self.cursor_viewport).and_then(|grid_pos| {
            input.cursor.map(|cursor| FrameCursorOverlay {
                grid_pos,
                style: cursor.style,
                wide: cursor.wide,
                color: cursor.color,
                alpha: cursor.alpha,
            })
        });

        FrameTextOverlayInput {
            preedit: self.preedit.as_ref(),
            cursor,
            screen_fg: input.screen_fg,
            alpha: input.alpha,
        }
    }

    pub(crate) fn cursor_uniform_input(
        &self,
        input: FrameSnapshotCursorUniformInput,
    ) -> FrameCursorUniformInput {
        let block_cursor = cursor_grid_pos(self.cursor_viewport).and_then(|grid_pos| {
            input.block_cursor.map(|cursor| FrameBlockCursorUniform {
                grid_pos,
                wide: cursor.wide,
                color: cursor.color,
            })
        });

        FrameCursorUniformInput {
            preedit_active: self.preedit.is_some(),
            block_cursor,
        }
    }

    /// Compose the prepared frame rebuild: build the plan from this snapshot and
    /// run the five rebuild drivers in dependency order, stopping before Metal
    /// presentation / renderer-thread orchestration (Issue 801, Exp 838).
    ///
    /// Ordering: `format_rows` → `draw_text_overlays` → `apply_rebuild_uniforms`
    /// → `refine_padding_extend_rows` (refines the padding-extend the rebuild
    /// uniforms reset, so it must run after) → `apply_cursor_uniforms` (disjoint
    /// uniform fields, order-independent). Fail-fast: the first failing stage
    /// returns its error and later stages do not run; earlier stages' mutations
    /// have already landed in `targets`, exactly as if hand-sequenced.
    pub(crate) fn rebuild_frame(
        &self,
        targets: FramePreparedRebuildTargets<'_>,
        input: FramePreparedRebuildInput<'_>,
    ) -> Result<FramePreparedRebuildApplication, FramePreparedRebuildError> {
        let plan = self.build_plan()?;
        self.run_rebuild_stages(&plan, targets, input)
    }

    /// Run the five rebuild drivers against a pre-built plan. Shared by
    /// `rebuild_frame` and `rebuild_and_present_frame` so the plan is built once
    /// and reused for presentation (Issue 801, Exp 839).
    fn run_rebuild_stages(
        &self,
        plan: &FrameRebuildPlan,
        targets: FramePreparedRebuildTargets<'_>,
        input: FramePreparedRebuildInput<'_>,
    ) -> Result<FramePreparedRebuildApplication, FramePreparedRebuildError> {
        let FramePreparedRebuildTargets {
            contents,
            grid,
            row_dirty,
            uniforms,
        } = targets;

        let rows = plan.format_rows(
            contents,
            grid,
            row_dirty,
            self.row_format_input(input.row_format),
        )?;
        let text_overlays =
            plan.draw_text_overlays(contents, grid, self.text_overlay_input(input.text_overlay))?;
        let rebuild_uniforms = plan.apply_rebuild_uniforms(uniforms, input.rebuild_uniform)?;
        let padding_extend = plan.refine_padding_extend_rows(uniforms, input.padding_extend)?;
        let cursor_uniforms =
            plan.apply_cursor_uniforms(uniforms, self.cursor_uniform_input(input.cursor_uniform))?;

        Ok(FramePreparedRebuildApplication {
            rows,
            text_overlays,
            rebuild_uniforms,
            padding_extend,
            cursor_uniforms,
        })
    }

    /// Compose the full prepared frame: rebuild (the five stages) then present via
    /// `present_metal_frame`, reusing the single plan so presentation validates
    /// against the same `effective_grid` the rebuild targeted (Issue 801, Exp
    /// 839). Stops at presentation; no renderer-thread orchestration.
    pub(crate) fn rebuild_and_present_frame(
        &self,
        targets: FramePreparedRebuildTargets<'_>,
        input: FramePreparedRebuildInput<'_>,
        presentation: FramePreparedPresentationInput<'_>,
    ) -> Result<FramePreparedFrameApplication, FramePreparedFrameError> {
        let plan = self.build_plan().map_err(FramePreparedRebuildError::from)?;

        let rebuild = self.run_rebuild_stages(
            &plan,
            FramePreparedRebuildTargets {
                contents: &mut *targets.contents,
                grid: &mut *targets.grid,
                row_dirty: &mut *targets.row_dirty,
                uniforms: &mut *targets.uniforms,
            },
            input,
        )?;

        let present = plan.present_metal_frame(
            presentation.compositor,
            FrameMetalPresentationInput {
                width: presentation.width,
                height: presentation.height,
                contents_scale: presentation.contents_scale,
                uniforms: targets.uniforms,
                contents: targets.contents,
                // Sample the SharedGrid's OWN atlases — the ones the rebuild just rasterized
                // glyphs into (Issue 802 / Exp 17). The rebuild's `&mut grid` borrow has ended,
                // so this immutable re-borrow is sound; presentation no longer carries atlases.
                grayscale_atlas: &targets.grid.atlas_grayscale,
                color_atlas: &targets.grid.atlas_color,
            },
        )?;

        Ok(FramePreparedFrameApplication { rebuild, present })
    }

    pub(crate) fn rebuild_and_present_frame_with_images(
        &self,
        targets: FramePreparedRebuildTargets<'_>,
        input: FramePreparedRebuildInput<'_>,
        images: &mut ImageState<MetalTexture>,
        presentation: FramePreparedPresentationInput<'_>,
    ) -> Result<FramePreparedFrameApplication, FramePreparedFrameError> {
        let plan = self.build_plan().map_err(FramePreparedRebuildError::from)?;

        let rebuild = self.run_rebuild_stages(
            &plan,
            FramePreparedRebuildTargets {
                contents: &mut *targets.contents,
                grid: &mut *targets.grid,
                row_dirty: &mut *targets.row_dirty,
                uniforms: &mut *targets.uniforms,
            },
            input,
        )?;

        let present = plan.present_metal_frame_with_images(
            presentation.compositor,
            images,
            FrameMetalPresentationInput {
                width: presentation.width,
                height: presentation.height,
                contents_scale: presentation.contents_scale,
                uniforms: targets.uniforms,
                contents: targets.contents,
                grayscale_atlas: &targets.grid.atlas_grayscale,
                color_atlas: &targets.grid.atlas_color,
            },
        )?;

        Ok(FramePreparedFrameApplication { rebuild, present })
    }
}

/// Mutable render targets the prepared frame rebuild sequence writes into.
pub(crate) struct FramePreparedRebuildTargets<'a> {
    pub(crate) contents: &'a mut Contents,
    pub(crate) grid: &'a mut SharedGrid,
    pub(crate) row_dirty: &'a mut [bool],
    pub(crate) uniforms: &'a mut MetalUniforms,
}

/// Caller-supplied inputs for the prepared frame rebuild sequence: the
/// snapshot-adapter inputs (827/828) plus the two drivers whose inputs are not
/// snapshot-derived (rebuild uniforms, padding extend).
pub(crate) struct FramePreparedRebuildInput<'a> {
    pub(crate) row_format: FrameSnapshotRowFormatInput<'a>,
    pub(crate) text_overlay: FrameSnapshotTextOverlayInput,
    pub(crate) cursor_uniform: FrameSnapshotCursorUniformInput,
    pub(crate) rebuild_uniform: FrameRebuildUniformInput,
    pub(crate) padding_extend: FramePaddingExtendInput<'a>,
}

/// Per-stage application results of one prepared frame rebuild.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FramePreparedRebuildApplication {
    pub(crate) rows: FrameRowRebuildApplication<FrameRowRenderError>,
    pub(crate) text_overlays: FrameTextOverlayApplication,
    pub(crate) rebuild_uniforms: FrameRebuildUniformApplication,
    pub(crate) padding_extend: FramePaddingExtendApplication,
    pub(crate) cursor_uniforms: FrameCursorUniformApplication,
}

/// Error from one prepared frame rebuild stage, wrapping that stage's own error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FramePreparedRebuildError {
    Plan(FrameRebuildPlanError),
    FormatRows(FrameRowFormatValidationError),
    TextOverlays(FrameTextOverlayError),
    RebuildUniforms(FrameRebuildUniformValidationError),
    PaddingExtend(FramePaddingExtendValidationError),
    CursorUniforms(FrameCursorUniformValidationError),
}

impl From<FrameRebuildPlanError> for FramePreparedRebuildError {
    fn from(error: FrameRebuildPlanError) -> Self {
        Self::Plan(error)
    }
}

impl From<FrameRowFormatValidationError> for FramePreparedRebuildError {
    fn from(error: FrameRowFormatValidationError) -> Self {
        Self::FormatRows(error)
    }
}

impl From<FrameTextOverlayError> for FramePreparedRebuildError {
    fn from(error: FrameTextOverlayError) -> Self {
        Self::TextOverlays(error)
    }
}

impl From<FrameRebuildUniformValidationError> for FramePreparedRebuildError {
    fn from(error: FrameRebuildUniformValidationError) -> Self {
        Self::RebuildUniforms(error)
    }
}

impl From<FramePaddingExtendValidationError> for FramePreparedRebuildError {
    fn from(error: FramePaddingExtendValidationError) -> Self {
        Self::PaddingExtend(error)
    }
}

impl From<FrameCursorUniformValidationError> for FramePreparedRebuildError {
    fn from(error: FrameCursorUniformValidationError) -> Self {
        Self::CursorUniforms(error)
    }
}

/// Presentation-only inputs for `rebuild_and_present_frame` (the Metal compositor,
/// atlases, and drawable dimensions the rebuild stages do not supply).
pub(crate) struct FramePreparedPresentationInput<'a> {
    pub(crate) compositor: &'a mut MetalFrameCompositor,
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) contents_scale: f64,
}

/// The result of one full prepared frame: the rebuild applications plus the Metal
/// presentation application.
#[derive(Debug)]
pub(crate) struct FramePreparedFrameApplication {
    pub(crate) rebuild: FramePreparedRebuildApplication,
    pub(crate) present: FrameMetalPresentationApplication,
}

/// Error from one full prepared frame: a rebuild-stage failure (including the
/// plan build) or the Metal presentation. `Debug`-only because
/// `FrameMetalPresentationError` derives only `Debug`.
#[derive(Debug)]
pub(crate) enum FramePreparedFrameError {
    Rebuild(FramePreparedRebuildError),
    Present(FrameMetalPresentationError),
}

impl From<FramePreparedRebuildError> for FramePreparedFrameError {
    fn from(error: FramePreparedRebuildError) -> Self {
        Self::Rebuild(error)
    }
}

impl From<FrameMetalPresentationError> for FramePreparedFrameError {
    fn from(error: FrameMetalPresentationError) -> Self {
        Self::Present(error)
    }
}

/// The preedit placement planned for this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FramePreeditRange {
    pub(crate) row: Unit,
    pub(crate) range: PreeditRange,
}

/// The value-level plan that a future `rebuildCells` integration can apply to
/// `Contents` and terminal row-dirty flags before formatting rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameRebuildPlan {
    pub(crate) grid_changed: bool,
    pub(crate) resize_to: Option<GridSize>,
    pub(crate) effective_grid: GridSize,
    pub(crate) full_rebuild: bool,
    pub(crate) row_count: Unit,
    pub(crate) rows_to_rebuild: Vec<Unit>,
    pub(crate) reset_contents: bool,
    pub(crate) clear_rows: Vec<Unit>,
    pub(crate) rows_to_mark_clean: Vec<Unit>,
    pub(crate) preedit_range: Option<FramePreeditRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameRebuildPlanError {
    DirtyRowsTooShort { needed: usize, actual: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameRebuildApplyError {
    ContentsGridMismatch {
        expected: GridSize,
        actual: GridSize,
    },
    ResizeGridMismatch {
        resize_to: GridSize,
        effective_grid: GridSize,
    },
    ClearRowOutOfBounds {
        row: Unit,
        rows: Unit,
    },
    DirtyRowsTooShort {
        needed: usize,
        actual: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameRebuildApplication {
    pub(crate) resized_to: Option<GridSize>,
    pub(crate) reset_contents: bool,
    pub(crate) cleared_rows: Vec<Unit>,
    pub(crate) marked_clean_rows: Vec<Unit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameRowRebuildValidationError {
    Apply(FrameRebuildApplyError),
    RebuildRowOutOfBounds { row: Unit, rows: Unit },
    DuplicateRebuildRow { row: Unit },
    DuplicateClearRow { row: Unit },
    DuplicateMarkCleanRow { row: Unit },
    ClearRowNotRebuilt { row: Unit },
    MarkCleanRowNotRebuilt { row: Unit },
}

impl From<FrameRebuildApplyError> for FrameRowRebuildValidationError {
    fn from(error: FrameRebuildApplyError) -> Self {
        Self::Apply(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameRowRebuildFailure<E> {
    pub(crate) row: Unit,
    pub(crate) error: E,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameRowRebuildApplication<E> {
    pub(crate) resized_to: Option<GridSize>,
    pub(crate) reset_contents: bool,
    pub(crate) cleared_rows: Vec<Unit>,
    pub(crate) marked_clean_rows: Vec<Unit>,
    pub(crate) rebuilt_rows: Vec<Unit>,
    pub(crate) failed_rows: Vec<FrameRowRebuildFailure<E>>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameRowFormatInput<'a> {
    pub(crate) rows: &'a [RunOptions],
    pub(crate) highlights: &'a [Vec<Highlight>],
    pub(crate) link_ranges: &'a [Vec<[u16; 2]>],
    pub(crate) selection_config: &'a SelectionConfig,
    pub(crate) default_fg: Rgb,
    pub(crate) default_bg: Rgb,
    pub(crate) palette: &'a Palette,
    pub(crate) bold: Option<BoldColor>,
    pub(crate) alpha: u8,
    pub(crate) faint_opacity: u8,
    pub(crate) thicken: bool,
    pub(crate) thicken_strength: u8,
    pub(crate) background_opacity_cells: bool,
    pub(crate) background_opacity: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameSnapshotRowFormatInput<'a> {
    pub(crate) highlights: &'a [Vec<Highlight>],
    pub(crate) link_ranges: &'a [Vec<[u16; 2]>],
    pub(crate) selection_config: &'a SelectionConfig,
    pub(crate) default_fg: Rgb,
    pub(crate) default_bg: Rgb,
    pub(crate) palette: &'a Palette,
    pub(crate) bold: Option<BoldColor>,
    pub(crate) alpha: u8,
    pub(crate) faint_opacity: u8,
    pub(crate) thicken: bool,
    pub(crate) thicken_strength: u8,
    pub(crate) background_opacity_cells: bool,
    pub(crate) background_opacity: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameRowFormatValidationError {
    Driver(FrameRowRebuildValidationError),
    MissingRow {
        row: Unit,
        rows: usize,
    },
    RowWidthMismatch {
        row: Unit,
        expected: Unit,
        actual: usize,
    },
}

impl From<FrameRowRebuildValidationError> for FrameRowFormatValidationError {
    fn from(error: FrameRowRebuildValidationError) -> Self {
        Self::Driver(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameRowRenderError {
    Render(ResolverRenderError),
}

impl From<ResolverRenderError> for FrameRowRenderError {
    fn from(error: ResolverRenderError) -> Self {
        Self::Render(error)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameCursorOverlay {
    pub(crate) grid_pos: [u16; 2],
    pub(crate) style: CursorStyle,
    pub(crate) wide: bool,
    pub(crate) color: Rgb,
    pub(crate) alpha: u8,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameSnapshotCursorOverlayInput {
    pub(crate) style: CursorStyle,
    pub(crate) wide: bool,
    pub(crate) color: Rgb,
    pub(crate) alpha: u8,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameSnapshotTextOverlayInput {
    pub(crate) cursor: Option<FrameSnapshotCursorOverlayInput>,
    pub(crate) screen_fg: Rgb,
    pub(crate) alpha: u8,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameTextOverlayInput<'a> {
    pub(crate) preedit: Option<&'a Preedit>,
    pub(crate) cursor: Option<FrameCursorOverlay>,
    pub(crate) screen_fg: Rgb,
    pub(crate) alpha: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameTextOverlayValidationError {
    ContentsGridMismatch {
        expected: GridSize,
        actual: GridSize,
    },
    CursorOutOfBounds {
        grid_pos: [u16; 2],
        size: GridSize,
    },
    WideCursorOutOfBounds {
        grid_pos: [u16; 2],
        columns: Unit,
    },
    PreeditRowOutOfBounds {
        row: Unit,
        rows: Unit,
    },
    PreeditRangeInvalid {
        range: PreeditRange,
    },
    PreeditRangeOutOfBounds {
        range: PreeditRange,
        columns: Unit,
    },
    MissingPreedit,
    PreeditOffsetOutOfBounds {
        cp_offset: usize,
        codepoints: usize,
    },
    PreeditWidthMismatch {
        expected: Unit,
        actual: Unit,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameTextOverlayRenderError {
    Render(ResolverRenderError),
}

impl From<ResolverRenderError> for FrameTextOverlayRenderError {
    fn from(error: ResolverRenderError) -> Self {
        Self::Render(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameTextOverlayError {
    Validation(FrameTextOverlayValidationError),
    Render(FrameTextOverlayRenderError),
}

impl From<FrameTextOverlayValidationError> for FrameTextOverlayError {
    fn from(error: FrameTextOverlayValidationError) -> Self {
        Self::Validation(error)
    }
}

impl From<FrameTextOverlayRenderError> for FrameTextOverlayError {
    fn from(error: FrameTextOverlayRenderError) -> Self {
        Self::Render(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameTextOverlayApplication {
    pub(crate) cursor_cleared: bool,
    pub(crate) cursor_drawn: Option<CursorStyle>,
    pub(crate) preedit_drawn: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameBlockCursorUniform {
    pub(crate) grid_pos: [u16; 2],
    pub(crate) wide: Wide,
    pub(crate) color: Rgb,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameSnapshotBlockCursorUniformInput {
    pub(crate) wide: Wide,
    pub(crate) color: Rgb,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameSnapshotCursorUniformInput {
    pub(crate) block_cursor: Option<FrameSnapshotBlockCursorUniformInput>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameCursorUniformInput {
    pub(crate) preedit_active: bool,
    pub(crate) block_cursor: Option<FrameBlockCursorUniform>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameCursorUniformValidationError {
    CursorOutOfBounds { grid_pos: [u16; 2], size: GridSize },
    WideCursorOutOfBounds { grid_pos: [u16; 2], columns: Unit },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameCursorUniformApplication {
    pub(crate) cursor_cleared: bool,
    pub(crate) block_cursor_applied: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameCustomShaderInput {
    pub(crate) time_secs: f32,
    pub(crate) time_delta_secs: f32,
    pub(crate) screen_size: [u32; 2],
    pub(crate) cell_size: [u32; 2],
    pub(crate) padding: [u32; 2],
    pub(crate) cursor: Option<CellTextVertex>,
    pub(crate) focused: bool,
    pub(crate) focus_changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameCustomShaderValidationError {
    ZeroCellSizeWithCursor { cell_size: [u32; 2] },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameCustomShaderApplication {
    pub(crate) frame_updated: bool,
    pub(crate) cursor_supplied: bool,
    pub(crate) focus_changed_consumed: bool,
    pub(crate) next_focus_changed: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct FrameMetalPresentationInput<'a> {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) contents_scale: f64,
    pub(crate) uniforms: &'a MetalUniforms,
    pub(crate) contents: &'a Contents,
    pub(crate) grayscale_atlas: &'a Atlas,
    pub(crate) color_atlas: &'a Atlas,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameMetalPresentationValidationError {
    ZeroDimensions {
        width: usize,
        height: usize,
    },
    ContentsGridMismatch {
        expected: GridSize,
        actual: GridSize,
    },
    UniformGridMismatch {
        expected: [u16; 2],
        actual: [u16; 2],
    },
}

#[derive(Debug)]
pub(crate) enum FrameMetalPresentationError {
    Validation(FrameMetalPresentationValidationError),
    Compositor(MetalFrameCompositorError),
}

impl From<FrameMetalPresentationValidationError> for FrameMetalPresentationError {
    fn from(error: FrameMetalPresentationValidationError) -> Self {
        Self::Validation(error)
    }
}

impl From<MetalFrameCompositorError> for FrameMetalPresentationError {
    fn from(error: MetalFrameCompositorError) -> Self {
        Self::Compositor(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameMetalPresentationApplication {
    pub(crate) presentation: MetalFramePresentation,
    pub(crate) foreground_drawn: bool,
    pub(crate) target_reallocated: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameRebuildUniformInput {
    pub(crate) padding_color: WindowPaddingColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameRebuildUniformValidationError {
    ResizeGridMismatch {
        resize_to: GridSize,
        effective_grid: GridSize,
    },
    ResizeWithoutFullRebuild {
        resize_to: GridSize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameRebuildUniformApplication {
    pub(crate) grid_size_updated: bool,
    pub(crate) padding_extend_mutated: bool,
    pub(crate) effective_grid: GridSize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FramePaddingExtendInput<'a> {
    pub(crate) padding_color: WindowPaddingColor,
    pub(crate) row_never_extend: &'a [bool],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FramePaddingExtendValidationError {
    DuplicateRebuildRow { row: Unit },
    RebuildRowOutOfBounds { row: Unit, rows: Unit },
    MissingRowNeverExtend { row: Unit, rows: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FramePaddingExtendApplication {
    pub(crate) refined_rows: Vec<Unit>,
    pub(crate) padding_extend_mutated: bool,
}

impl FrameRebuildPlan {
    pub(crate) fn build(input: FrameRebuildInput<'_>) -> Result<Self, FrameRebuildPlanError> {
        let needed_dirty_rows = input.terminal_grid.rows as usize;
        if input.row_dirty.len() < needed_dirty_rows {
            return Err(FrameRebuildPlanError::DirtyRowsTooShort {
                needed: needed_dirty_rows,
                actual: input.row_dirty.len(),
            });
        }

        let grid_changed = input.current_grid != input.terminal_grid;
        let resize_to = grid_changed.then_some(input.terminal_grid);
        let effective_grid = resize_to.unwrap_or(input.current_grid);
        let row_count = input.terminal_grid.rows.min(effective_grid.rows);
        let full_rebuild = matches!(input.dirty, RenderDirty::Full) || grid_changed;

        let rows_to_rebuild: Vec<Unit> = if full_rebuild {
            (0..row_count).collect()
        } else {
            input
                .row_dirty
                .iter()
                .take(row_count as usize)
                .enumerate()
                .filter_map(|(row, dirty)| dirty.then_some(row as Unit))
                .collect()
        };

        let reset_contents = full_rebuild;
        let clear_rows = if full_rebuild {
            Vec::new()
        } else {
            rows_to_rebuild.clone()
        };
        let rows_to_mark_clean = rows_to_rebuild.clone();
        let preedit_range = plan_preedit_range(input, row_count, &rows_to_rebuild);

        Ok(Self {
            grid_changed,
            resize_to,
            effective_grid,
            full_rebuild,
            row_count,
            rows_to_rebuild,
            reset_contents,
            clear_rows,
            rows_to_mark_clean,
            preedit_range,
        })
    }

    pub(crate) fn apply_to_contents(
        &self,
        contents: &mut Contents,
        row_dirty: &mut [bool],
    ) -> Result<FrameRebuildApplication, FrameRebuildApplyError> {
        self.validate_application(contents, row_dirty)?;

        if let Some(size) = self.resize_to {
            contents.resize(size);
        }
        if self.reset_contents {
            contents.reset();
        }
        for row in &self.clear_rows {
            contents.clear(*row);
        }
        for row in &self.rows_to_mark_clean {
            row_dirty[*row as usize] = false;
        }

        Ok(FrameRebuildApplication {
            resized_to: self.resize_to,
            reset_contents: self.reset_contents,
            cleared_rows: self.clear_rows.clone(),
            marked_clean_rows: self.rows_to_mark_clean.clone(),
        })
    }

    pub(crate) fn drive_row_rebuilds<E>(
        &self,
        contents: &mut Contents,
        row_dirty: &mut [bool],
        mut rebuild_row: impl FnMut(&mut Contents, Unit) -> Result<(), E>,
    ) -> Result<FrameRowRebuildApplication<E>, FrameRowRebuildValidationError> {
        self.validate_row_rebuild_application(contents, row_dirty)?;

        if let Some(size) = self.resize_to {
            contents.resize(size);
        }
        if self.reset_contents {
            contents.reset();
        }

        let clear_rows: HashSet<Unit> = self.clear_rows.iter().copied().collect();
        let mark_clean_rows: HashSet<Unit> = self.rows_to_mark_clean.iter().copied().collect();
        let mut rebuilt_rows = Vec::new();
        let mut failed_rows = Vec::new();

        for row in &self.rows_to_rebuild {
            if clear_rows.contains(row) {
                contents.clear(*row);
            }
            if mark_clean_rows.contains(row) {
                row_dirty[*row as usize] = false;
            }

            match rebuild_row(contents, *row) {
                Ok(()) => rebuilt_rows.push(*row),
                Err(error) => {
                    contents.clear(*row);
                    failed_rows.push(FrameRowRebuildFailure { row: *row, error });
                }
            }
        }

        Ok(FrameRowRebuildApplication {
            resized_to: self.resize_to,
            reset_contents: self.reset_contents,
            cleared_rows: self.clear_rows.clone(),
            marked_clean_rows: self.rows_to_mark_clean.clone(),
            rebuilt_rows,
            failed_rows,
        })
    }

    pub(crate) fn format_rows(
        &self,
        contents: &mut Contents,
        grid: &mut SharedGrid,
        row_dirty: &mut [bool],
        input: FrameRowFormatInput<'_>,
    ) -> Result<FrameRowRebuildApplication<FrameRowRenderError>, FrameRowFormatValidationError>
    {
        self.validate_format_rows_input(&input)?;

        self.drive_row_rebuilds(contents, row_dirty, |contents, row| {
            let opts = &input.rows[row as usize];
            let row_highlights = input
                .highlights
                .get(row as usize)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let row_links = input
                .link_ranges
                .get(row as usize)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let row_preedit = self
                .preedit_range
                .filter(|range| range.row == row)
                .map(|range| [range.range.start, range.range.end]);

            rebuild_bg_row(
                contents,
                row,
                &opts.cells,
                opts.selection,
                row_highlights,
                input.selection_config,
                input.default_fg,
                input.default_bg,
                input.palette,
                input.bold,
                input.alpha,
                row_preedit,
                input.background_opacity_cells,
                input.background_opacity,
            );

            let runs = shape_row_cached(opts, &mut grid.resolver, &mut grid.shaper_cache);
            rebuild_row(
                contents,
                grid,
                row,
                &runs,
                &opts.cells,
                opts.selection,
                row_highlights,
                input.selection_config,
                input.default_fg,
                input.default_bg,
                input.palette,
                input.bold,
                input.alpha,
                input.faint_opacity,
                input.thicken,
                input.thicken_strength,
                row_links,
                row_preedit,
            )
            .map_err(FrameRowRenderError::from)
        })
        .map_err(FrameRowFormatValidationError::from)
    }

    pub(crate) fn draw_text_overlays(
        &self,
        contents: &mut Contents,
        grid: &mut SharedGrid,
        input: FrameTextOverlayInput<'_>,
    ) -> Result<FrameTextOverlayApplication, FrameTextOverlayError> {
        self.validate_text_overlay_input(contents, &input)?;

        contents.set_cursor(None, None);

        let mut cursor_drawn = None;
        let mut preedit_drawn = false;

        if let Some(preedit) = input.preedit {
            if let Some(range) = self.preedit_range {
                add_preedit(
                    contents,
                    grid,
                    preedit,
                    range.range,
                    range.row,
                    self.effective_grid.columns,
                    rgb_array(input.screen_fg),
                )
                .map_err(FrameTextOverlayRenderError::from)?;
                preedit_drawn = true;
            }

            return Ok(FrameTextOverlayApplication {
                cursor_cleared: true,
                cursor_drawn,
                preedit_drawn,
            });
        }

        if let Some(cursor) = input.cursor {
            add_cursor(
                contents,
                grid,
                cursor.grid_pos,
                cursor.style,
                cursor.wide,
                rgb_array(cursor.color),
                cursor.alpha,
            )
            .map_err(FrameTextOverlayRenderError::from)?;
            if contents.get_cursor_glyph().is_some() {
                cursor_drawn = Some(cursor.style);
            }
        }

        Ok(FrameTextOverlayApplication {
            cursor_cleared: true,
            cursor_drawn,
            preedit_drawn,
        })
    }

    pub(crate) fn apply_cursor_uniforms(
        &self,
        uniforms: &mut MetalUniforms,
        input: FrameCursorUniformInput,
    ) -> Result<FrameCursorUniformApplication, FrameCursorUniformValidationError> {
        self.validate_cursor_uniform_input(input)?;

        uniforms.clear_cursor();

        if input.preedit_active {
            return Ok(FrameCursorUniformApplication {
                cursor_cleared: true,
                block_cursor_applied: false,
            });
        }

        let mut block_cursor_applied = false;
        if let Some(cursor) = input.block_cursor {
            let [x, y] = cursor.grid_pos;
            uniforms.update_block_cursor(x, y, cursor.wide, cursor.color);
            block_cursor_applied = true;
        }

        Ok(FrameCursorUniformApplication {
            cursor_cleared: true,
            block_cursor_applied,
        })
    }

    pub(crate) fn apply_custom_shader_frame(
        &self,
        uniforms: &mut CustomShaderUniforms,
        input: FrameCustomShaderInput,
    ) -> Result<FrameCustomShaderApplication, FrameCustomShaderValidationError> {
        self.validate_custom_shader_input(input)?;

        uniforms.update_for_frame(
            input.time_secs,
            input.time_delta_secs,
            input.screen_size[0],
            input.screen_size[1],
        );
        uniforms.update_cursor(
            input.cursor,
            input.cell_size[0],
            input.cell_size[1],
            input.padding[0],
            input.padding[1],
        );
        let next_focus_changed = uniforms.update_focus(input.focused, input.focus_changed);

        Ok(FrameCustomShaderApplication {
            frame_updated: true,
            cursor_supplied: input.cursor.is_some(),
            focus_changed_consumed: input.focus_changed && input.focused && !next_focus_changed,
            next_focus_changed,
        })
    }

    pub(crate) fn present_metal_frame(
        &self,
        compositor: &mut MetalFrameCompositor,
        input: FrameMetalPresentationInput<'_>,
    ) -> Result<FrameMetalPresentationApplication, FrameMetalPresentationError> {
        self.validate_metal_presentation_input(&input)?;

        let presentation = compositor.draw_frame(MetalFrameInput {
            width: input.width,
            height: input.height,
            contents_scale: input.contents_scale,
            uniforms: input.uniforms,
            contents: input.contents,
            grayscale_atlas: input.grayscale_atlas,
            color_atlas: input.color_atlas,
        })?;

        Ok(FrameMetalPresentationApplication {
            foreground_drawn: presentation.fg_count > 0,
            target_reallocated: presentation.target_reallocated,
            presentation,
        })
    }

    pub(crate) fn present_metal_frame_with_images(
        &self,
        compositor: &mut MetalFrameCompositor,
        images: &mut ImageState<MetalTexture>,
        input: FrameMetalPresentationInput<'_>,
    ) -> Result<FrameMetalPresentationApplication, FrameMetalPresentationError> {
        self.validate_metal_presentation_input(&input)?;

        let presentation = compositor.draw_frame_with_images(
            MetalFrameInput {
                width: input.width,
                height: input.height,
                contents_scale: input.contents_scale,
                uniforms: input.uniforms,
                contents: input.contents,
                grayscale_atlas: input.grayscale_atlas,
                color_atlas: input.color_atlas,
            },
            images,
        )?;

        Ok(FrameMetalPresentationApplication {
            foreground_drawn: presentation.fg_count > 0,
            target_reallocated: presentation.target_reallocated,
            presentation,
        })
    }

    pub(crate) fn apply_rebuild_uniforms(
        &self,
        uniforms: &mut MetalUniforms,
        input: FrameRebuildUniformInput,
    ) -> Result<FrameRebuildUniformApplication, FrameRebuildUniformValidationError> {
        self.validate_rebuild_uniforms()?;

        let mut grid_size_updated = false;
        let mut padding_extend_mutated = false;

        if self.resize_to.is_some() {
            uniforms.update_grid_size(self.effective_grid);
            grid_size_updated = true;
        }

        if self.full_rebuild {
            uniforms.reset_padding_extend(input.padding_color);
            padding_extend_mutated = matches!(
                input.padding_color,
                WindowPaddingColor::Extend | WindowPaddingColor::ExtendAlways
            );
        }

        Ok(FrameRebuildUniformApplication {
            grid_size_updated,
            padding_extend_mutated,
            effective_grid: self.effective_grid,
        })
    }

    pub(crate) fn refine_padding_extend_rows(
        &self,
        uniforms: &mut MetalUniforms,
        input: FramePaddingExtendInput<'_>,
    ) -> Result<FramePaddingExtendApplication, FramePaddingExtendValidationError> {
        self.validate_padding_extend_rows(&input)?;

        let before = uniforms.padding_extend;
        let mut refined_rows = Vec::new();

        if input.padding_color == WindowPaddingColor::Extend && self.effective_grid.rows > 0 {
            let last_row = self.effective_grid.rows - 1;

            for row in &self.rows_to_rebuild {
                let is_first_row = *row == 0;
                let is_last_row = *row == last_row;
                if !is_first_row && !is_last_row {
                    continue;
                }

                let never_extend = input.row_never_extend[*row as usize];
                uniforms.refine_padding_extend(
                    input.padding_color,
                    is_first_row,
                    is_last_row,
                    never_extend,
                );
                refined_rows.push(*row);
            }
        }

        Ok(FramePaddingExtendApplication {
            refined_rows,
            padding_extend_mutated: before != uniforms.padding_extend,
        })
    }

    fn validate_application(
        &self,
        contents: &Contents,
        row_dirty: &[bool],
    ) -> Result<(), FrameRebuildApplyError> {
        if let Some(resize_to) = self.resize_to {
            if resize_to != self.effective_grid {
                return Err(FrameRebuildApplyError::ResizeGridMismatch {
                    resize_to,
                    effective_grid: self.effective_grid,
                });
            }
        } else if contents.size() != self.effective_grid {
            return Err(FrameRebuildApplyError::ContentsGridMismatch {
                expected: self.effective_grid,
                actual: contents.size(),
            });
        }

        for row in &self.clear_rows {
            if *row >= self.effective_grid.rows {
                return Err(FrameRebuildApplyError::ClearRowOutOfBounds {
                    row: *row,
                    rows: self.effective_grid.rows,
                });
            }
        }

        let needed_dirty_rows = self
            .rows_to_mark_clean
            .iter()
            .copied()
            .max()
            .map_or(0, |row| row as usize + 1);
        if row_dirty.len() < needed_dirty_rows {
            return Err(FrameRebuildApplyError::DirtyRowsTooShort {
                needed: needed_dirty_rows,
                actual: row_dirty.len(),
            });
        }

        Ok(())
    }

    fn validate_format_rows_input(
        &self,
        input: &FrameRowFormatInput<'_>,
    ) -> Result<(), FrameRowFormatValidationError> {
        let expected = self.effective_grid.columns;
        for row in &self.rows_to_rebuild {
            let Some(opts) = input.rows.get(*row as usize) else {
                return Err(FrameRowFormatValidationError::MissingRow {
                    row: *row,
                    rows: input.rows.len(),
                });
            };
            if opts.cells.len() != expected as usize {
                return Err(FrameRowFormatValidationError::RowWidthMismatch {
                    row: *row,
                    expected,
                    actual: opts.cells.len(),
                });
            }
        }
        Ok(())
    }

    fn validate_rebuild_uniforms(&self) -> Result<(), FrameRebuildUniformValidationError> {
        if let Some(resize_to) = self.resize_to {
            if resize_to != self.effective_grid {
                return Err(FrameRebuildUniformValidationError::ResizeGridMismatch {
                    resize_to,
                    effective_grid: self.effective_grid,
                });
            }
            if !self.full_rebuild {
                return Err(
                    FrameRebuildUniformValidationError::ResizeWithoutFullRebuild { resize_to },
                );
            }
        }

        Ok(())
    }

    fn validate_padding_extend_rows(
        &self,
        input: &FramePaddingExtendInput<'_>,
    ) -> Result<(), FramePaddingExtendValidationError> {
        let mut seen = HashSet::new();
        for row in &self.rows_to_rebuild {
            if !seen.insert(*row) {
                return Err(FramePaddingExtendValidationError::DuplicateRebuildRow { row: *row });
            }
            if *row >= self.effective_grid.rows {
                return Err(FramePaddingExtendValidationError::RebuildRowOutOfBounds {
                    row: *row,
                    rows: self.effective_grid.rows,
                });
            }
        }

        if input.padding_color == WindowPaddingColor::Extend && self.effective_grid.rows > 0 {
            let last_row = self.effective_grid.rows - 1;
            for row in &self.rows_to_rebuild {
                if (*row == 0 || *row == last_row)
                    && input.row_never_extend.get(*row as usize).is_none()
                {
                    return Err(FramePaddingExtendValidationError::MissingRowNeverExtend {
                        row: *row,
                        rows: input.row_never_extend.len(),
                    });
                }
            }
        }

        Ok(())
    }

    fn validate_metal_presentation_input(
        &self,
        input: &FrameMetalPresentationInput<'_>,
    ) -> Result<(), FrameMetalPresentationValidationError> {
        if input.width == 0 || input.height == 0 {
            return Err(FrameMetalPresentationValidationError::ZeroDimensions {
                width: input.width,
                height: input.height,
            });
        }

        if input.contents.size() != self.effective_grid {
            return Err(
                FrameMetalPresentationValidationError::ContentsGridMismatch {
                    expected: self.effective_grid,
                    actual: input.contents.size(),
                },
            );
        }

        let expected = [self.effective_grid.columns, self.effective_grid.rows];
        if input.uniforms.grid_size != expected {
            return Err(FrameMetalPresentationValidationError::UniformGridMismatch {
                expected,
                actual: input.uniforms.grid_size,
            });
        }

        Ok(())
    }

    fn validate_custom_shader_input(
        &self,
        input: FrameCustomShaderInput,
    ) -> Result<(), FrameCustomShaderValidationError> {
        if input.cursor.is_some() && (input.cell_size[0] == 0 || input.cell_size[1] == 0) {
            return Err(FrameCustomShaderValidationError::ZeroCellSizeWithCursor {
                cell_size: input.cell_size,
            });
        }

        Ok(())
    }

    fn validate_cursor_uniform_input(
        &self,
        input: FrameCursorUniformInput,
    ) -> Result<(), FrameCursorUniformValidationError> {
        if let Some(cursor) = input.block_cursor {
            let [x, y] = cursor.grid_pos;
            if x >= self.effective_grid.columns || y >= self.effective_grid.rows {
                return Err(FrameCursorUniformValidationError::CursorOutOfBounds {
                    grid_pos: cursor.grid_pos,
                    size: self.effective_grid,
                });
            }
            if matches!(cursor.wide, Wide::Wide) && x + 1 >= self.effective_grid.columns {
                return Err(FrameCursorUniformValidationError::WideCursorOutOfBounds {
                    grid_pos: cursor.grid_pos,
                    columns: self.effective_grid.columns,
                });
            }
        }

        Ok(())
    }

    fn validate_text_overlay_input(
        &self,
        contents: &Contents,
        input: &FrameTextOverlayInput<'_>,
    ) -> Result<(), FrameTextOverlayValidationError> {
        if contents.size() != self.effective_grid {
            return Err(FrameTextOverlayValidationError::ContentsGridMismatch {
                expected: self.effective_grid,
                actual: contents.size(),
            });
        }

        if let Some(cursor) = input.cursor {
            let [x, y] = cursor.grid_pos;
            if x >= self.effective_grid.columns || y >= self.effective_grid.rows {
                return Err(FrameTextOverlayValidationError::CursorOutOfBounds {
                    grid_pos: cursor.grid_pos,
                    size: self.effective_grid,
                });
            }
            if cursor.wide && x + 1 >= self.effective_grid.columns {
                return Err(FrameTextOverlayValidationError::WideCursorOutOfBounds {
                    grid_pos: cursor.grid_pos,
                    columns: self.effective_grid.columns,
                });
            }
        }

        if let Some(preedit_range) = self.preedit_range {
            if preedit_range.row >= self.effective_grid.rows {
                return Err(FrameTextOverlayValidationError::PreeditRowOutOfBounds {
                    row: preedit_range.row,
                    rows: self.effective_grid.rows,
                });
            }

            let range = preedit_range.range;
            if range.start > range.end {
                return Err(FrameTextOverlayValidationError::PreeditRangeInvalid { range });
            }
            if range.end >= self.effective_grid.columns {
                return Err(FrameTextOverlayValidationError::PreeditRangeOutOfBounds {
                    range,
                    columns: self.effective_grid.columns,
                });
            }

            let Some(preedit) = input.preedit else {
                return Err(FrameTextOverlayValidationError::MissingPreedit);
            };
            if range.cp_offset > preedit.codepoints.len() {
                return Err(FrameTextOverlayValidationError::PreeditOffsetOutOfBounds {
                    cp_offset: range.cp_offset,
                    codepoints: preedit.codepoints.len(),
                });
            }

            let codepoints = &preedit.codepoints[range.cp_offset..];
            let actual = preedit_width(codepoints);
            let expected = range.end - range.start + 1;
            if actual < expected {
                return Err(FrameTextOverlayValidationError::PreeditWidthMismatch {
                    expected,
                    actual,
                });
            }

            let mut x = range.start;
            for cp in codepoints {
                let width = if cp.wide { 2 } else { 1 };
                let end = x + width - 1;
                let allowed_final_wide =
                    cp.wide && x == range.end && range.end + 1 == self.effective_grid.columns;
                if x > range.end || (end > range.end && !allowed_final_wide) {
                    return Err(FrameTextOverlayValidationError::PreeditWidthMismatch {
                        expected,
                        actual,
                    });
                }
                x = x.saturating_add(width);
            }
        }

        Ok(())
    }

    fn validate_row_rebuild_application(
        &self,
        contents: &Contents,
        row_dirty: &[bool],
    ) -> Result<(), FrameRowRebuildValidationError> {
        self.validate_application(contents, row_dirty)?;
        validate_unique_rows(&self.rows_to_rebuild, |row| {
            FrameRowRebuildValidationError::DuplicateRebuildRow { row }
        })?;
        validate_unique_rows(&self.clear_rows, |row| {
            FrameRowRebuildValidationError::DuplicateClearRow { row }
        })?;
        validate_unique_rows(&self.rows_to_mark_clean, |row| {
            FrameRowRebuildValidationError::DuplicateMarkCleanRow { row }
        })?;

        for row in &self.rows_to_rebuild {
            if *row >= self.effective_grid.rows {
                return Err(FrameRowRebuildValidationError::RebuildRowOutOfBounds {
                    row: *row,
                    rows: self.effective_grid.rows,
                });
            }
        }

        let rebuild_rows: HashSet<Unit> = self.rows_to_rebuild.iter().copied().collect();
        for row in &self.clear_rows {
            if !rebuild_rows.contains(row) {
                return Err(FrameRowRebuildValidationError::ClearRowNotRebuilt { row: *row });
            }
        }
        for row in &self.rows_to_mark_clean {
            if !rebuild_rows.contains(row) {
                return Err(FrameRowRebuildValidationError::MarkCleanRowNotRebuilt { row: *row });
            }
        }

        Ok(())
    }
}

fn rgb_array(rgb: Rgb) -> [u8; 3] {
    [rgb.r, rgb.g, rgb.b]
}

fn preedit_width(codepoints: &[crate::renderer::state::Codepoint]) -> Unit {
    codepoints
        .iter()
        .map(|cp| if cp.wide { 2 } else { 1 })
        .sum()
}

fn cursor_grid_pos(cursor: Option<Coordinate>) -> Option<[u16; 2]> {
    let cursor = cursor?;
    Some([cursor.x, u16::try_from(cursor.y).ok()?])
}

fn validate_unique_rows(
    rows: &[Unit],
    duplicate: impl Fn(Unit) -> FrameRowRebuildValidationError,
) -> Result<(), FrameRowRebuildValidationError> {
    let mut seen = HashSet::new();
    for row in rows {
        if !seen.insert(*row) {
            return Err(duplicate(*row));
        }
    }
    Ok(())
}

fn plan_preedit_range(
    input: FrameRebuildInput<'_>,
    row_count: Unit,
    rows_to_rebuild: &[Unit],
) -> Option<FramePreeditRange> {
    let preedit = input.preedit?;
    if preedit.codepoints.is_empty() {
        return None;
    }
    let cursor = input.cursor_viewport?;
    let row = Unit::try_from(cursor.y).ok()?;
    if row >= row_count || cursor.x >= input.terminal_grid.columns {
        return None;
    }
    if input.terminal_grid.columns == 0 {
        return None;
    }
    if !rows_to_rebuild.contains(&row) {
        return None;
    }

    Some(FramePreeditRange {
        row,
        range: preedit.range(cursor.x, input.terminal_grid.columns - 1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::atlas::{Atlas, Format};
    use crate::renderer::cell::{Contents, Key, SelectionColor};
    use crate::renderer::cursor::Style as CursorStyle;
    use crate::renderer::metal::api::{MetalPixelFormat, MetalResourceOptions, MetalStorageMode};
    use crate::renderer::metal::compositor::{MetalFrameCompositorOptions, MetalFramePresentation};
    use crate::renderer::metal::shaders::{ortho2d, MetalUniforms};
    use crate::renderer::shader::{CellBg, CellTextAtlas, CellTextFlags, CellTextVertex};
    use crate::renderer::state::Codepoint;
    use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
    use crate::terminal::style::{Color, Style as TermStyle};
    use crate::terminal::terminal::Terminal;
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_metal::{MTLCreateSystemDefaultDevice, MTLDevice};

    fn grid(columns: Unit, rows: Unit) -> GridSize {
        GridSize { columns, rows }
    }

    fn preedit(widths: &[bool]) -> Preedit {
        Preedit {
            codepoints: widths
                .iter()
                .map(|wide| Codepoint {
                    codepoint: 'x' as u32,
                    wide: *wide,
                })
                .collect(),
        }
    }

    fn input<'a>(
        current_grid: GridSize,
        terminal_grid: GridSize,
        dirty: RenderDirty,
        row_dirty: &'a [bool],
    ) -> FrameRebuildInput<'a> {
        FrameRebuildInput {
            current_grid,
            terminal_grid,
            dirty,
            row_dirty,
            preedit: None,
            cursor_viewport: None,
        }
    }

    fn terminal(columns: Unit, rows: Unit) -> Terminal {
        Terminal::init(columns, rows, None).expect("terminal")
    }

    fn write_terminal(terminal: &mut Terminal, input: &[u8]) {
        terminal.next_slice(input).expect("terminal input");
    }

    fn dummy_vertex(row: Unit, marker: u8) -> CellTextVertex {
        CellTextVertex {
            glyph_pos: [marker as u32, 0],
            glyph_size: [1, 1],
            bearings: [0, 0],
            grid_pos: [0, row],
            color: [marker, marker, marker, 255],
            atlas: CellTextAtlas::Grayscale,
            flags: CellTextFlags::new(false, false),
            _padding: [0, 0],
        }
    }

    fn contents_with_rows(size: GridSize) -> Contents {
        let mut contents = Contents::default();
        contents.resize(size);
        for row in 0..size.rows {
            for col in 0..size.columns {
                *contents.bg_cell_mut(row as usize, col as usize) =
                    CellBg([row as u8 + 1, col as u8 + 1, 7, 255]);
            }
            contents.add(Key::Text, dummy_vertex(row, row as u8 + 10));
        }
        contents
    }

    #[test]
    fn terminal_snapshot_clean_terminal_builds_empty_partial_plan() {
        let mut terminal = terminal(4, 2);
        terminal.clear_dirty_for_tests();

        let snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 2), RenderDirty::Partial, None);
        let plan = snapshot.build_plan().expect("plan");

        assert_eq!(snapshot.current_grid, grid(4, 2));
        assert_eq!(snapshot.terminal_grid, grid(4, 2));
        assert_eq!(snapshot.row_dirty, vec![false, false]);
        assert_eq!(snapshot.rows.len(), 2);
        assert_eq!(plan.rows_to_rebuild, Vec::<Unit>::new());
        assert_eq!(plan.clear_rows, Vec::<Unit>::new());
        assert_eq!(plan.rows_to_mark_clean, Vec::<Unit>::new());
    }

    #[test]
    fn terminal_snapshot_dirty_rows_drive_partial_rebuild() {
        let mut terminal = terminal(4, 3);
        terminal.clear_dirty_for_tests();
        terminal.set_cursor_position_for_tests(0, 1);
        write_terminal(&mut terminal, b"A");

        let snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 3), RenderDirty::Partial, None);
        let plan = snapshot.build_plan().expect("plan");

        assert_eq!(snapshot.row_dirty, vec![false, true, false]);
        assert_eq!(plan.full_rebuild, false);
        assert_eq!(plan.rows_to_rebuild, vec![1]);
        assert_eq!(plan.clear_rows, vec![1]);
        assert_eq!(plan.rows_to_mark_clean, vec![1]);
    }

    #[test]
    fn terminal_snapshot_full_dirty_rebuilds_all_rows_when_row_flags_clean() {
        let mut terminal = terminal(4, 3);
        terminal.clear_dirty_for_tests();

        let snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 3), RenderDirty::Full, None);
        let plan = snapshot.build_plan().expect("plan");

        assert_eq!(snapshot.row_dirty, vec![false, false, false]);
        assert_eq!(plan.full_rebuild, true);
        assert_eq!(plan.rows_to_rebuild, vec![0, 1, 2]);
        assert_eq!(plan.clear_rows, Vec::<Unit>::new());
        assert_eq!(plan.rows_to_mark_clean, vec![0, 1, 2]);
    }

    #[test]
    fn terminal_snapshot_grid_mismatch_plans_resize_full_rebuild() {
        let mut terminal = terminal(4, 2);
        terminal.clear_dirty_for_tests();

        let snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(2, 1), RenderDirty::Partial, None);
        let plan = snapshot.build_plan().expect("plan");

        assert_eq!(snapshot.terminal_grid, grid(4, 2));
        assert_eq!(plan.grid_changed, true);
        assert_eq!(plan.resize_to, Some(grid(4, 2)));
        assert_eq!(plan.full_rebuild, true);
        assert_eq!(plan.rows_to_rebuild, vec![0, 1]);
    }

    #[test]
    fn terminal_snapshot_captures_cursor_only_inside_terminal_grid() {
        let mut terminal = terminal(4, 3);
        terminal.set_cursor_position_for_tests(2, 1);

        let snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 3), RenderDirty::Partial, None);

        assert_eq!(snapshot.cursor_viewport, Some(Coordinate::new(2, 1)));
    }

    #[test]
    fn terminal_snapshot_owns_preedit_and_feeds_rebuild_input() {
        let mut terminal = terminal(4, 2);
        terminal.set_cursor_position_for_tests(1, 0);
        let preedit = preedit(&[false, false]);

        let snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 2), RenderDirty::Full, Some(preedit));
        let input = snapshot.rebuild_input();
        let plan = snapshot.build_plan().expect("plan");

        assert_eq!(input.preedit.expect("preedit").codepoints.len(), 2);
        assert_eq!(
            plan.preedit_range,
            Some(FramePreeditRange {
                row: 0,
                range: PreeditRange {
                    start: 1,
                    end: 2,
                    cp_offset: 0,
                },
            })
        );
    }

    #[test]
    fn terminal_snapshot_rows_match_terminal_shape_run_options() {
        let mut terminal = terminal(5, 2);
        write_terminal(&mut terminal, b"AB\nCD");

        let expected = terminal.shape_run_options();
        let snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(5, 2), RenderDirty::Partial, None);

        assert_eq!(snapshot.rows, expected);
        assert_eq!(snapshot.rows.len(), 2);
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

    fn run_cell(cp: u32, bg: Color) -> crate::font::run::RunCell {
        crate::font::run::RunCell {
            codepoint: cp,
            graphemes: vec![],
            style: TermStyle {
                bg_color: bg,
                ..TermStyle::default()
            },
            explicit_bg: Color::None,
            style_id: 0,
            wide: crate::font::run::Wide::Narrow,
            is_empty: cp == 0,
            is_codepoint: cp != 0,
        }
    }

    fn row(text: &[u8], bg: Color) -> RunOptions {
        RunOptions {
            cells: text.iter().map(|cp| run_cell(u32::from(*cp), bg)).collect(),
            ..Default::default()
        }
    }

    fn format_input<'a>(rows: &'a [RunOptions]) -> FrameRowFormatInput<'a> {
        FrameRowFormatInput {
            rows,
            highlights: &[],
            link_ranges: &[],
            selection_config: Box::leak(Box::new(SelectionConfig::default())),
            default_fg: Rgb::new(200, 200, 200),
            default_bg: Rgb::new(0, 0, 0),
            palette: &DEFAULT_PALETTE,
            bold: None,
            alpha: 255,
            faint_opacity: 128,
            thicken: false,
            thicken_strength: 255,
            background_opacity_cells: false,
            background_opacity: 1.0,
        }
    }

    fn snapshot_format_input<'a>(
        highlights: &'a [Vec<Highlight>],
        link_ranges: &'a [Vec<[u16; 2]>],
        selection_config: &'a SelectionConfig,
    ) -> FrameSnapshotRowFormatInput<'a> {
        FrameSnapshotRowFormatInput {
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
        }
    }

    #[test]
    fn snapshot_row_format_input_borrows_snapshot_rows_by_identity() {
        let mut terminal = terminal(4, 2);
        write_terminal(&mut terminal, b"AB");
        let snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 2), RenderDirty::Partial, None);
        let highlights = Vec::new();
        let links = Vec::new();
        let selection_config = SelectionConfig::default();

        let input = snapshot.row_format_input(snapshot_format_input(
            &highlights,
            &links,
            &selection_config,
        ));

        assert!(std::ptr::eq(input.rows, snapshot.rows.as_slice()));
    }

    #[test]
    fn snapshot_row_format_input_threads_renderer_options() {
        let terminal = terminal(3, 1);
        let snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(3, 1), RenderDirty::Partial, None);
        let highlights = vec![vec![Highlight {
            range: [0, 1],
            tag: crate::renderer::cell::HighlightTag::SearchMatch,
        }]];
        let links = vec![vec![[1, 2]]];
        let selection_config = SelectionConfig {
            search_background: SelectionColor::Color(Rgb::new(4, 5, 6)),
            search_foreground: SelectionColor::Color(Rgb::new(7, 8, 9)),
            ..SelectionConfig::default()
        };

        let input = snapshot.row_format_input(snapshot_format_input(
            &highlights,
            &links,
            &selection_config,
        ));

        assert!(std::ptr::eq(input.rows, snapshot.rows.as_slice()));
        assert!(std::ptr::eq(input.highlights, highlights.as_slice()));
        assert!(std::ptr::eq(input.link_ranges, links.as_slice()));
        assert!(std::ptr::eq(input.selection_config, &selection_config));
        assert!(std::ptr::eq(input.palette, &DEFAULT_PALETTE));
        assert_eq!(input.default_fg, Rgb::new(210, 211, 212));
        assert_eq!(input.default_bg, Rgb::new(10, 11, 12));
        assert_eq!(input.bold, Some(BoldColor::Color(Rgb::new(1, 2, 3))));
        assert_eq!(input.alpha, 230);
        assert_eq!(input.faint_opacity, 99);
        assert_eq!(input.thicken, true);
        assert_eq!(input.thicken_strength, 77);
        assert_eq!(input.background_opacity_cells, true);
        assert_eq!(input.background_opacity, 0.42);
    }

    #[test]
    fn snapshot_row_format_input_feeds_live_terminal_row_formatting() {
        let mut terminal = terminal(4, 3);
        terminal.clear_dirty_for_tests();
        terminal.set_cursor_position_for_tests(0, 1);
        write_terminal(&mut terminal, b"A");
        let snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 3), RenderDirty::Partial, None);
        let plan = snapshot.build_plan().expect("plan");
        let highlights = Vec::new();
        let links = Vec::new();
        let selection_config = SelectionConfig::default();
        let mut contents = contents_with_rows(grid(4, 3));
        let preserved_bg = *contents.bg_cell(0, 0);
        let mut row_dirty = snapshot.row_dirty.clone();
        let mut grid = menlo_grid();

        let applied = plan
            .format_rows(
                &mut contents,
                &mut grid,
                &mut row_dirty,
                snapshot.row_format_input(snapshot_format_input(
                    &highlights,
                    &links,
                    &selection_config,
                )),
            )
            .expect("format rows");

        assert_eq!(applied.rebuilt_rows, vec![1]);
        assert!(applied.failed_rows.is_empty());
        assert_eq!(row_dirty, vec![false, false, false]);
        assert_eq!(*contents.bg_cell(0, 0), preserved_bg);
        assert!(contents.fg_rows()[2]
            .iter()
            .any(|vertex| vertex.grid_pos == [0, 1]));
    }

    #[test]
    fn full_dirty_rebuilds_all_rows_and_resets_contents() {
        let plan = FrameRebuildPlan::build(input(
            grid(4, 3),
            grid(4, 3),
            RenderDirty::Full,
            &[false, true, false],
        ))
        .expect("plan");

        assert!(!plan.grid_changed);
        assert_eq!(plan.resize_to, None);
        assert_eq!(plan.effective_grid, grid(4, 3));
        assert!(plan.full_rebuild);
        assert_eq!(plan.row_count, 3);
        assert_eq!(plan.rows_to_rebuild, vec![0, 1, 2]);
        assert!(plan.reset_contents);
        assert!(plan.clear_rows.is_empty());
        assert_eq!(plan.rows_to_mark_clean, vec![0, 1, 2]);
    }

    #[test]
    fn partial_rebuilds_only_dirty_rows_and_clears_them() {
        let plan = FrameRebuildPlan::build(input(
            grid(4, 4),
            grid(4, 4),
            RenderDirty::Partial,
            &[false, true, false, true],
        ))
        .expect("plan");

        assert!(!plan.full_rebuild);
        assert_eq!(plan.rows_to_rebuild, vec![1, 3]);
        assert!(!plan.reset_contents);
        assert_eq!(plan.clear_rows, vec![1, 3]);
        assert_eq!(plan.rows_to_mark_clean, vec![1, 3]);
    }

    #[test]
    fn clean_still_rebuilds_dirty_rows() {
        let plan = FrameRebuildPlan::build(input(
            grid(4, 3),
            grid(4, 3),
            RenderDirty::Clean,
            &[false, true, false],
        ))
        .expect("plan");

        assert!(!plan.full_rebuild);
        assert_eq!(plan.rows_to_rebuild, vec![1]);
        assert_eq!(plan.clear_rows, vec![1]);
        assert_eq!(plan.rows_to_mark_clean, vec![1]);
    }

    #[test]
    fn grid_growth_uses_post_resize_rows() {
        let plan = FrameRebuildPlan::build(input(
            grid(4, 2),
            grid(4, 5),
            RenderDirty::Clean,
            &[false, false, false, false, false],
        ))
        .expect("plan");

        assert!(plan.grid_changed);
        assert_eq!(plan.resize_to, Some(grid(4, 5)));
        assert_eq!(plan.effective_grid, grid(4, 5));
        assert!(plan.full_rebuild);
        assert_eq!(plan.row_count, 5);
        assert_eq!(plan.rows_to_rebuild, vec![0, 1, 2, 3, 4]);
        assert!(plan.reset_contents);
    }

    #[test]
    fn grid_shrink_uses_post_resize_rows() {
        let plan = FrameRebuildPlan::build(input(
            grid(4, 5),
            grid(4, 2),
            RenderDirty::Partial,
            &[false, false],
        ))
        .expect("plan");

        assert!(plan.grid_changed);
        assert_eq!(plan.resize_to, Some(grid(4, 2)));
        assert_eq!(plan.effective_grid, grid(4, 2));
        assert_eq!(plan.row_count, 2);
        assert_eq!(plan.rows_to_rebuild, vec![0, 1]);
    }

    #[test]
    fn row_count_clamps_to_effective_grid_rows() {
        let plan = FrameRebuildPlan::build(input(
            grid(4, 2),
            grid(4, 3),
            RenderDirty::Full,
            &[false, false, false],
        ))
        .expect("plan");

        assert_eq!(plan.row_count, 3);
        assert_eq!(plan.rows_to_rebuild, vec![0, 1, 2]);
    }

    #[test]
    fn short_dirty_slice_errors() {
        let err = FrameRebuildPlan::build(input(
            grid(4, 3),
            grid(4, 3),
            RenderDirty::Full,
            &[false, false],
        ))
        .expect_err("short row dirty slice should error");

        assert_eq!(
            err,
            FrameRebuildPlanError::DirtyRowsTooShort {
                needed: 3,
                actual: 2,
            }
        );
    }

    #[test]
    fn extra_dirty_flags_are_ignored() {
        let plan = FrameRebuildPlan::build(input(
            grid(4, 2),
            grid(4, 2),
            RenderDirty::Clean,
            &[false, true, true],
        ))
        .expect("plan");

        assert_eq!(plan.rows_to_rebuild, vec![1]);
    }

    #[test]
    fn zero_sized_grids_plan_no_rows_or_preedit() {
        let p = preedit(&[false]);
        let plan = FrameRebuildPlan::build(FrameRebuildInput {
            current_grid: grid(0, 0),
            terminal_grid: grid(0, 0),
            dirty: RenderDirty::Full,
            row_dirty: &[],
            preedit: Some(&p),
            cursor_viewport: Some(Coordinate::new(0, 0)),
        })
        .expect("plan");

        assert_eq!(plan.row_count, 0);
        assert!(plan.rows_to_rebuild.is_empty());
        assert_eq!(plan.preedit_range, None);
    }

    #[test]
    fn preedit_range_is_planned_for_rebuilt_cursor_row() {
        let p = preedit(&[false, true]);
        let plan = FrameRebuildPlan::build(FrameRebuildInput {
            current_grid: grid(4, 3),
            terminal_grid: grid(4, 3),
            dirty: RenderDirty::Partial,
            row_dirty: &[false, true, false],
            preedit: Some(&p),
            cursor_viewport: Some(Coordinate::new(2, 1)),
        })
        .expect("plan");

        assert_eq!(
            plan.preedit_range,
            Some(FramePreeditRange {
                row: 1,
                range: PreeditRange {
                    start: 1,
                    end: 3,
                    cp_offset: 0,
                },
            })
        );
    }

    #[test]
    fn preedit_range_is_planned_on_full_rebuild_even_when_row_clean() {
        let p = preedit(&[false]);
        let plan = FrameRebuildPlan::build(FrameRebuildInput {
            current_grid: grid(4, 2),
            terminal_grid: grid(4, 2),
            dirty: RenderDirty::Full,
            row_dirty: &[false, false],
            preedit: Some(&p),
            cursor_viewport: Some(Coordinate::new(1, 1)),
        })
        .expect("plan");

        assert!(plan.preedit_range.is_some());
    }

    #[test]
    fn preedit_range_is_skipped_when_partial_cursor_row_clean() {
        let p = preedit(&[false]);
        let plan = FrameRebuildPlan::build(FrameRebuildInput {
            current_grid: grid(4, 2),
            terminal_grid: grid(4, 2),
            dirty: RenderDirty::Partial,
            row_dirty: &[true, false],
            preedit: Some(&p),
            cursor_viewport: Some(Coordinate::new(1, 1)),
        })
        .expect("plan");

        assert_eq!(plan.rows_to_rebuild, vec![0]);
        assert_eq!(plan.preedit_range, None);
    }

    #[test]
    fn preedit_range_is_skipped_when_cursor_outside_viewport() {
        let p = preedit(&[false]);
        let plan = FrameRebuildPlan::build(FrameRebuildInput {
            current_grid: grid(4, 2),
            terminal_grid: grid(4, 2),
            dirty: RenderDirty::Full,
            row_dirty: &[false, false],
            preedit: Some(&p),
            cursor_viewport: Some(Coordinate::new(4, 2)),
        })
        .expect("plan");

        assert_eq!(plan.preedit_range, None);
    }

    #[test]
    fn apply_resizes_before_reset_and_reports_post_size() {
        let plan = FrameRebuildPlan::build(input(
            grid(2, 1),
            grid(3, 2),
            RenderDirty::Clean,
            &[true, true],
        ))
        .expect("plan");
        let mut contents = contents_with_rows(grid(2, 1));
        let mut row_dirty = vec![true, true];

        let applied = plan
            .apply_to_contents(&mut contents, &mut row_dirty)
            .expect("apply plan");

        assert_eq!(contents.size(), grid(3, 2));
        assert_eq!(applied.resized_to, Some(grid(3, 2)));
        assert!(applied.reset_contents);
        assert_eq!(applied.marked_clean_rows, vec![0, 1]);
        assert_eq!(row_dirty, vec![false, false]);
        for row in 0..2 {
            for col in 0..3 {
                assert_eq!(*contents.bg_cell(row, col), CellBg([0, 0, 0, 0]));
            }
        }
    }

    #[test]
    fn apply_full_rebuild_resets_cursor_reserved_lists() {
        let plan = FrameRebuildPlan::build(input(
            grid(2, 2),
            grid(2, 2),
            RenderDirty::Full,
            &[true, true],
        ))
        .expect("plan");
        let mut contents = contents_with_rows(grid(2, 2));
        contents.set_cursor(Some(dummy_vertex(0, 99)), Some(CursorStyle::Block));
        assert!(contents.get_cursor_glyph().is_some());
        let mut row_dirty = vec![true, true];

        plan.apply_to_contents(&mut contents, &mut row_dirty)
            .expect("apply plan");

        assert_eq!(contents.get_cursor_glyph(), None);
        assert!(contents.fg_rows().iter().all(Vec::is_empty));
        assert_eq!(row_dirty, vec![false, false]);
    }

    #[test]
    fn apply_partial_clears_only_planned_rows() {
        let plan = FrameRebuildPlan::build(input(
            grid(2, 3),
            grid(2, 3),
            RenderDirty::Partial,
            &[false, true, false],
        ))
        .expect("plan");
        let mut contents = contents_with_rows(grid(2, 3));
        let preserved_bg = *contents.bg_cell(0, 0);
        let preserved_fg = contents.fg_rows()[1].clone();
        let mut row_dirty = vec![false, true, false];

        let applied = plan
            .apply_to_contents(&mut contents, &mut row_dirty)
            .expect("apply plan");

        assert_eq!(applied.cleared_rows, vec![1]);
        assert_eq!(applied.marked_clean_rows, vec![1]);
        assert_eq!(*contents.bg_cell(0, 0), preserved_bg);
        assert_eq!(contents.fg_rows()[1], preserved_fg);
        assert_eq!(*contents.bg_cell(1, 0), CellBg([0, 0, 0, 0]));
        assert!(contents.fg_rows()[2].is_empty());
        assert_eq!(row_dirty, vec![false, false, false]);
    }

    #[test]
    fn apply_clean_empty_plan_is_noop() {
        let plan = FrameRebuildPlan::build(input(
            grid(2, 2),
            grid(2, 2),
            RenderDirty::Clean,
            &[false, false],
        ))
        .expect("plan");
        let mut contents = contents_with_rows(grid(2, 2));
        let bg = contents.bg_cells().to_vec();
        let fg = contents.fg_rows().to_vec();
        let mut row_dirty = vec![false, false];

        let applied = plan
            .apply_to_contents(&mut contents, &mut row_dirty)
            .expect("apply plan");

        assert_eq!(applied.resized_to, None);
        assert!(!applied.reset_contents);
        assert!(applied.cleared_rows.is_empty());
        assert!(applied.marked_clean_rows.is_empty());
        assert_eq!(contents.bg_cells(), bg);
        assert_eq!(contents.fg_rows(), fg);
        assert_eq!(row_dirty, vec![false, false]);
    }

    #[test]
    fn apply_rejects_short_dirty_slice_without_mutation() {
        let plan = FrameRebuildPlan::build(input(
            grid(2, 3),
            grid(2, 3),
            RenderDirty::Full,
            &[true, true, true],
        ))
        .expect("plan");
        let mut contents = contents_with_rows(grid(2, 3));
        let bg = contents.bg_cells().to_vec();
        let fg = contents.fg_rows().to_vec();
        let mut row_dirty = vec![true, true];

        let err = plan
            .apply_to_contents(&mut contents, &mut row_dirty)
            .expect_err("short dirty slice should error before mutation");

        assert_eq!(
            err,
            FrameRebuildApplyError::DirtyRowsTooShort {
                needed: 3,
                actual: 2,
            }
        );
        assert_eq!(contents.bg_cells(), bg);
        assert_eq!(contents.fg_rows(), fg);
        assert_eq!(row_dirty, vec![true, true]);
    }

    #[test]
    fn apply_rejects_out_of_bounds_clear_row_without_mutation() {
        let mut plan = FrameRebuildPlan::build(input(
            grid(2, 2),
            grid(2, 2),
            RenderDirty::Partial,
            &[false, true],
        ))
        .expect("plan");
        plan.clear_rows = vec![2];
        let mut contents = contents_with_rows(grid(2, 2));
        let bg = contents.bg_cells().to_vec();
        let fg = contents.fg_rows().to_vec();
        let mut row_dirty = vec![false, true];

        let err = plan
            .apply_to_contents(&mut contents, &mut row_dirty)
            .expect_err("invalid clear row should error before mutation");

        assert_eq!(
            err,
            FrameRebuildApplyError::ClearRowOutOfBounds { row: 2, rows: 2 }
        );
        assert_eq!(contents.bg_cells(), bg);
        assert_eq!(contents.fg_rows(), fg);
        assert_eq!(row_dirty, vec![false, true]);
    }

    #[test]
    fn apply_rejects_grid_mismatch_without_mutation() {
        let plan = FrameRebuildPlan::build(input(
            grid(2, 2),
            grid(2, 2),
            RenderDirty::Clean,
            &[false, false],
        ))
        .expect("plan");
        let mut contents = contents_with_rows(grid(3, 2));
        let mut row_dirty = vec![false, false];

        let err = plan
            .apply_to_contents(&mut contents, &mut row_dirty)
            .expect_err("contents grid mismatch should error");

        assert_eq!(
            err,
            FrameRebuildApplyError::ContentsGridMismatch {
                expected: grid(2, 2),
                actual: grid(3, 2),
            }
        );
        assert_eq!(contents.size(), grid(3, 2));
    }

    #[test]
    fn apply_rejects_resize_effective_grid_mismatch_without_mutation() {
        let mut plan = FrameRebuildPlan::build(input(
            grid(2, 2),
            grid(3, 2),
            RenderDirty::Clean,
            &[false, false],
        ))
        .expect("plan");
        plan.effective_grid = grid(4, 2);
        let mut contents = contents_with_rows(grid(2, 2));
        let mut row_dirty = vec![false, false];

        let err = plan
            .apply_to_contents(&mut contents, &mut row_dirty)
            .expect_err("resize/effective mismatch should error");

        assert_eq!(
            err,
            FrameRebuildApplyError::ResizeGridMismatch {
                resize_to: grid(3, 2),
                effective_grid: grid(4, 2),
            }
        );
        assert_eq!(contents.size(), grid(2, 2));
    }

    #[test]
    fn drive_partial_rows_clears_marks_and_rebuilds_each_row() {
        let plan = FrameRebuildPlan::build(input(
            grid(2, 3),
            grid(2, 3),
            RenderDirty::Partial,
            &[false, true, true],
        ))
        .expect("plan");
        let mut contents = contents_with_rows(grid(2, 3));
        let mut row_dirty = vec![false, true, true];
        let mut observed = Vec::new();

        let applied = plan
            .drive_row_rebuilds(&mut contents, &mut row_dirty, |contents, row| {
                observed.push(row);
                assert_eq!(*contents.bg_cell(row as usize, 0), CellBg([0, 0, 0, 0]));
                *contents.bg_cell_mut(row as usize, 0) = CellBg([row as u8 + 20, 1, 1, 255]);
                Ok::<(), &'static str>(())
            })
            .expect("drive row rebuilds");

        assert_eq!(observed, vec![1, 2]);
        assert_eq!(applied.cleared_rows, vec![1, 2]);
        assert_eq!(applied.marked_clean_rows, vec![1, 2]);
        assert_eq!(applied.rebuilt_rows, vec![1, 2]);
        assert!(applied.failed_rows.is_empty());
        assert_eq!(row_dirty, vec![false, false, false]);
        assert_eq!(*contents.bg_cell(1, 0), CellBg([21, 1, 1, 255]));
        assert_eq!(*contents.bg_cell(2, 0), CellBg([22, 1, 1, 255]));
    }

    #[test]
    fn drive_full_rebuild_resets_once_and_rebuilds_all_rows() {
        let plan = FrameRebuildPlan::build(input(
            grid(2, 2),
            grid(2, 2),
            RenderDirty::Full,
            &[true, true],
        ))
        .expect("plan");
        let mut contents = contents_with_rows(grid(2, 2));
        let mut row_dirty = vec![true, true];
        let mut observed = Vec::new();

        let applied = plan
            .drive_row_rebuilds(&mut contents, &mut row_dirty, |contents, row| {
                observed.push(row);
                assert_eq!(*contents.bg_cell(row as usize, 0), CellBg([0, 0, 0, 0]));
                *contents.bg_cell_mut(row as usize, 0) = CellBg([row as u8 + 30, 2, 2, 255]);
                Ok::<(), &'static str>(())
            })
            .expect("drive row rebuilds");

        assert_eq!(observed, vec![0, 1]);
        assert!(applied.reset_contents);
        assert!(applied.cleared_rows.is_empty());
        assert_eq!(applied.marked_clean_rows, vec![0, 1]);
        assert_eq!(applied.rebuilt_rows, vec![0, 1]);
        assert_eq!(row_dirty, vec![false, false]);
        assert_eq!(*contents.bg_cell(0, 0), CellBg([30, 2, 2, 255]));
        assert_eq!(*contents.bg_cell(1, 0), CellBg([31, 2, 2, 255]));
    }

    #[test]
    fn drive_callback_errors_clear_failed_row_and_continue() {
        let plan = FrameRebuildPlan::build(input(
            grid(2, 3),
            grid(2, 3),
            RenderDirty::Partial,
            &[true, true, true],
        ))
        .expect("plan");
        let mut contents = contents_with_rows(grid(2, 3));
        let mut row_dirty = vec![true, true, true];
        let mut observed = Vec::new();

        let applied = plan
            .drive_row_rebuilds(&mut contents, &mut row_dirty, |contents, row| {
                observed.push(row);
                *contents.bg_cell_mut(row as usize, 0) = CellBg([row as u8 + 40, 3, 3, 255]);
                if row == 1 {
                    Err("row failed")
                } else {
                    Ok(())
                }
            })
            .expect("drive row rebuilds");

        assert_eq!(observed, vec![0, 1, 2]);
        assert_eq!(applied.rebuilt_rows, vec![0, 2]);
        assert_eq!(
            applied.failed_rows,
            vec![FrameRowRebuildFailure {
                row: 1,
                error: "row failed",
            }]
        );
        assert_eq!(*contents.bg_cell(0, 0), CellBg([40, 3, 3, 255]));
        assert_eq!(*contents.bg_cell(1, 0), CellBg([0, 0, 0, 0]));
        assert_eq!(*contents.bg_cell(2, 0), CellBg([42, 3, 3, 255]));
        assert_eq!(row_dirty, vec![false, false, false]);
    }

    #[test]
    fn drive_resize_happens_before_callback() {
        let plan = FrameRebuildPlan::build(input(
            grid(1, 1),
            grid(2, 2),
            RenderDirty::Clean,
            &[false, false],
        ))
        .expect("plan");
        let mut contents = contents_with_rows(grid(1, 1));
        let mut row_dirty = vec![false, false];
        let mut sizes = Vec::new();

        let applied = plan
            .drive_row_rebuilds(&mut contents, &mut row_dirty, |contents, row| {
                sizes.push((row, contents.size()));
                Ok::<(), &'static str>(())
            })
            .expect("drive row rebuilds");

        assert_eq!(applied.resized_to, Some(grid(2, 2)));
        assert_eq!(contents.size(), grid(2, 2));
        assert_eq!(sizes, vec![(0, grid(2, 2)), (1, grid(2, 2))]);
    }

    #[test]
    fn drive_clean_empty_plan_is_noop() {
        let plan = FrameRebuildPlan::build(input(
            grid(2, 2),
            grid(2, 2),
            RenderDirty::Clean,
            &[false, false],
        ))
        .expect("plan");
        let mut contents = contents_with_rows(grid(2, 2));
        let bg = contents.bg_cells().to_vec();
        let fg = contents.fg_rows().to_vec();
        let mut row_dirty = vec![false, false];

        let applied = plan
            .drive_row_rebuilds(&mut contents, &mut row_dirty, |_contents, _row| {
                panic!("empty plan should not invoke callback");
                #[allow(unreachable_code)]
                Ok::<(), &'static str>(())
            })
            .expect("drive row rebuilds");

        assert_eq!(applied.resized_to, None);
        assert!(!applied.reset_contents);
        assert!(applied.cleared_rows.is_empty());
        assert!(applied.marked_clean_rows.is_empty());
        assert!(applied.rebuilt_rows.is_empty());
        assert!(applied.failed_rows.is_empty());
        assert_eq!(contents.bg_cells(), bg);
        assert_eq!(contents.fg_rows(), fg);
        assert_eq!(row_dirty, vec![false, false]);
    }

    #[test]
    fn drive_rejects_out_of_bounds_rebuild_row_without_mutation() {
        let mut plan = FrameRebuildPlan::build(input(
            grid(2, 2),
            grid(2, 2),
            RenderDirty::Partial,
            &[false, true],
        ))
        .expect("plan");
        plan.rows_to_rebuild = vec![2];
        plan.clear_rows.clear();
        plan.rows_to_mark_clean.clear();
        let mut contents = contents_with_rows(grid(2, 2));
        let bg = contents.bg_cells().to_vec();
        let fg = contents.fg_rows().to_vec();
        let mut row_dirty = vec![false, true];

        let err = plan
            .drive_row_rebuilds(&mut contents, &mut row_dirty, |_contents, _row| {
                Ok::<(), &'static str>(())
            })
            .expect_err("invalid rebuild row should error before mutation");

        assert_eq!(
            err,
            FrameRowRebuildValidationError::RebuildRowOutOfBounds { row: 2, rows: 2 }
        );
        assert_eq!(contents.bg_cells(), bg);
        assert_eq!(contents.fg_rows(), fg);
        assert_eq!(row_dirty, vec![false, true]);
    }

    #[test]
    fn drive_rejects_duplicate_rows_without_mutation() {
        let mut plan = FrameRebuildPlan::build(input(
            grid(2, 2),
            grid(2, 2),
            RenderDirty::Partial,
            &[false, true],
        ))
        .expect("plan");
        plan.rows_to_rebuild = vec![1, 1];
        let mut contents = contents_with_rows(grid(2, 2));
        let mut row_dirty = vec![false, true];

        let err = plan
            .drive_row_rebuilds(&mut contents, &mut row_dirty, |_contents, _row| {
                Ok::<(), &'static str>(())
            })
            .expect_err("duplicate rebuild rows should error");

        assert_eq!(
            err,
            FrameRowRebuildValidationError::DuplicateRebuildRow { row: 1 }
        );
        assert_eq!(row_dirty, vec![false, true]);
    }

    #[test]
    fn drive_rejects_duplicate_clear_rows_without_mutation() {
        let mut plan = FrameRebuildPlan::build(input(
            grid(2, 2),
            grid(2, 2),
            RenderDirty::Partial,
            &[false, true],
        ))
        .expect("plan");
        plan.clear_rows = vec![1, 1];
        let mut contents = contents_with_rows(grid(2, 2));
        let bg = contents.bg_cells().to_vec();
        let fg = contents.fg_rows().to_vec();
        let mut row_dirty = vec![false, true];

        let err = plan
            .drive_row_rebuilds(&mut contents, &mut row_dirty, |_contents, _row| {
                Ok::<(), &'static str>(())
            })
            .expect_err("duplicate clear rows should error");

        assert_eq!(
            err,
            FrameRowRebuildValidationError::DuplicateClearRow { row: 1 }
        );
        assert_eq!(contents.bg_cells(), bg);
        assert_eq!(contents.fg_rows(), fg);
        assert_eq!(row_dirty, vec![false, true]);
    }

    #[test]
    fn drive_rejects_duplicate_mark_clean_rows_without_mutation() {
        let mut plan = FrameRebuildPlan::build(input(
            grid(2, 2),
            grid(2, 2),
            RenderDirty::Partial,
            &[false, true],
        ))
        .expect("plan");
        plan.rows_to_mark_clean = vec![1, 1];
        let mut contents = contents_with_rows(grid(2, 2));
        let bg = contents.bg_cells().to_vec();
        let fg = contents.fg_rows().to_vec();
        let mut row_dirty = vec![false, true];

        let err = plan
            .drive_row_rebuilds(&mut contents, &mut row_dirty, |_contents, _row| {
                Ok::<(), &'static str>(())
            })
            .expect_err("duplicate mark-clean rows should error");

        assert_eq!(
            err,
            FrameRowRebuildValidationError::DuplicateMarkCleanRow { row: 1 }
        );
        assert_eq!(contents.bg_cells(), bg);
        assert_eq!(contents.fg_rows(), fg);
        assert_eq!(row_dirty, vec![false, true]);
    }

    #[test]
    fn drive_rejects_clear_or_mark_rows_outside_rebuild_set() {
        let mut plan = FrameRebuildPlan::build(input(
            grid(2, 3),
            grid(2, 3),
            RenderDirty::Partial,
            &[true, true, false],
        ))
        .expect("plan");
        plan.clear_rows = vec![0, 2];
        let mut contents = contents_with_rows(grid(2, 3));
        let mut row_dirty = vec![true, true, false];

        let err = plan
            .drive_row_rebuilds(&mut contents, &mut row_dirty, |_contents, _row| {
                Ok::<(), &'static str>(())
            })
            .expect_err("clear row outside rebuild set should error");

        assert_eq!(
            err,
            FrameRowRebuildValidationError::ClearRowNotRebuilt { row: 2 }
        );

        let mut plan = FrameRebuildPlan::build(input(
            grid(2, 3),
            grid(2, 3),
            RenderDirty::Partial,
            &[true, true, false],
        ))
        .expect("plan");
        plan.rows_to_mark_clean = vec![0, 2];
        let err = plan
            .drive_row_rebuilds(&mut contents, &mut row_dirty, |_contents, _row| {
                Ok::<(), &'static str>(())
            })
            .expect_err("mark-clean row outside rebuild set should error");

        assert_eq!(
            err,
            FrameRowRebuildValidationError::MarkCleanRowNotRebuilt { row: 2 }
        );
        assert_eq!(row_dirty, vec![true, true, false]);
    }

    #[test]
    fn drive_wraps_apply_validation_errors_without_mutation() {
        let plan = FrameRebuildPlan::build(input(
            grid(2, 3),
            grid(2, 3),
            RenderDirty::Full,
            &[true, true, true],
        ))
        .expect("plan");
        let mut contents = contents_with_rows(grid(2, 3));
        let bg = contents.bg_cells().to_vec();
        let fg = contents.fg_rows().to_vec();
        let mut row_dirty = vec![true, true];

        let err = plan
            .drive_row_rebuilds(&mut contents, &mut row_dirty, |_contents, _row| {
                Ok::<(), &'static str>(())
            })
            .expect_err("short dirty slice should error before mutation");

        assert_eq!(
            err,
            FrameRowRebuildValidationError::Apply(FrameRebuildApplyError::DirtyRowsTooShort {
                needed: 3,
                actual: 2,
            })
        );
        assert_eq!(contents.bg_cells(), bg);
        assert_eq!(contents.fg_rows(), fg);
        assert_eq!(row_dirty, vec![true, true]);
    }

    #[test]
    fn format_rows_partial_formats_only_planned_rows() {
        let mut plan = FrameRebuildPlan::build(input(
            grid(2, 2),
            grid(2, 2),
            RenderDirty::Partial,
            &[false, true],
        ))
        .expect("plan");
        let rows = vec![row(b"AA", Color::Palette(1)), row(b"BC", Color::Palette(2))];
        let mut contents = contents_with_rows(grid(2, 2));
        let preserved_bg = *contents.bg_cell(0, 0);
        let preserved_fg = contents.fg_rows()[1].clone();
        let mut row_dirty = vec![false, true];
        let mut grid = menlo_grid();

        let applied = plan
            .format_rows(
                &mut contents,
                &mut grid,
                &mut row_dirty,
                format_input(&rows),
            )
            .expect("format rows");

        assert_eq!(applied.rebuilt_rows, vec![1]);
        assert!(applied.failed_rows.is_empty());
        assert_eq!(row_dirty, vec![false, false]);
        assert_eq!(*contents.bg_cell(0, 0), preserved_bg);
        assert_eq!(contents.fg_rows()[1], preserved_fg);
        let p2 = DEFAULT_PALETTE[2];
        assert_eq!(*contents.bg_cell(1, 0), CellBg([p2.r, p2.g, p2.b, 255]));
        assert_eq!(contents.fg_rows()[2].len(), 2);
        assert_eq!(contents.fg_rows()[2][0].grid_pos, [0, 1]);

        // Keep the stale-plan fields mutable inside this module; no mutation here.
        plan.rows_to_rebuild = vec![1];
    }

    #[test]
    fn format_rows_full_rebuild_formats_every_row_after_reset() {
        let plan = FrameRebuildPlan::build(input(
            grid(2, 2),
            grid(2, 2),
            RenderDirty::Full,
            &[true, true],
        ))
        .expect("plan");
        let rows = vec![row(b"AB", Color::Palette(1)), row(b"CD", Color::Palette(2))];
        let mut contents = contents_with_rows(grid(2, 2));
        let mut row_dirty = vec![true, true];
        let mut grid = menlo_grid();

        let applied = plan
            .format_rows(
                &mut contents,
                &mut grid,
                &mut row_dirty,
                format_input(&rows),
            )
            .expect("format rows");

        assert!(applied.reset_contents);
        assert_eq!(applied.rebuilt_rows, vec![0, 1]);
        assert_eq!(row_dirty, vec![false, false]);
        assert_eq!(contents.fg_rows()[1].len(), 2);
        assert_eq!(contents.fg_rows()[2].len(), 2);
        assert_eq!(contents.fg_rows()[1][0].grid_pos, [0, 0]);
        assert_eq!(contents.fg_rows()[2][0].grid_pos, [0, 1]);
    }

    #[test]
    fn format_rows_rejects_missing_row_without_mutation() {
        let plan = FrameRebuildPlan::build(input(
            grid(2, 2),
            grid(2, 2),
            RenderDirty::Full,
            &[true, true],
        ))
        .expect("plan");
        let rows = vec![row(b"AB", Color::Palette(1))];
        let mut contents = contents_with_rows(grid(2, 2));
        let bg = contents.bg_cells().to_vec();
        let fg = contents.fg_rows().to_vec();
        let mut row_dirty = vec![true, true];
        let mut grid = menlo_grid();

        let err = plan
            .format_rows(
                &mut contents,
                &mut grid,
                &mut row_dirty,
                format_input(&rows),
            )
            .expect_err("missing row should validate before mutation");

        assert_eq!(
            err,
            FrameRowFormatValidationError::MissingRow { row: 1, rows: 1 }
        );
        assert_eq!(contents.bg_cells(), bg);
        assert_eq!(contents.fg_rows(), fg);
        assert_eq!(row_dirty, vec![true, true]);
    }

    #[test]
    fn format_rows_rejects_wrong_row_width_without_mutation() {
        let plan =
            FrameRebuildPlan::build(input(grid(2, 1), grid(2, 1), RenderDirty::Full, &[true]))
                .expect("plan");
        let rows = vec![row(b"ABC", Color::Palette(1))];
        let mut contents = contents_with_rows(grid(2, 1));
        let bg = contents.bg_cells().to_vec();
        let fg = contents.fg_rows().to_vec();
        let mut row_dirty = vec![true];
        let mut grid = menlo_grid();

        let err = plan
            .format_rows(
                &mut contents,
                &mut grid,
                &mut row_dirty,
                format_input(&rows),
            )
            .expect_err("wrong width should validate before mutation");

        assert_eq!(
            err,
            FrameRowFormatValidationError::RowWidthMismatch {
                row: 0,
                expected: 2,
                actual: 3,
            }
        );
        assert_eq!(contents.bg_cells(), bg);
        assert_eq!(contents.fg_rows(), fg);
        assert_eq!(row_dirty, vec![true]);
    }

    #[test]
    fn format_rows_threads_highlights_links_and_plan_preedit() {
        let p = preedit(&[false]);
        let plan = FrameRebuildPlan::build(FrameRebuildInput {
            current_grid: grid(2, 1),
            terminal_grid: grid(2, 1),
            dirty: RenderDirty::Full,
            row_dirty: &[true],
            preedit: Some(&p),
            cursor_viewport: Some(Coordinate::new(0, 0)),
        })
        .expect("plan");
        let rows = vec![row(b"AB", Color::Palette(1))];
        let highlights = vec![vec![Highlight {
            range: [1, 1],
            tag: crate::renderer::cell::HighlightTag::SearchMatch,
        }]];
        let links = vec![vec![[1, 1]]];
        let selection_config = SelectionConfig {
            search_background: SelectionColor::Color(Rgb::new(1, 2, 3)),
            search_foreground: SelectionColor::Color(Rgb::new(4, 5, 6)),
            ..SelectionConfig::default()
        };
        let mut input = format_input(&rows);
        input.highlights = &highlights;
        input.link_ranges = &links;
        input.selection_config = &selection_config;
        let mut contents = Contents::default();
        contents.resize(grid(2, 1));
        let mut row_dirty = vec![true];
        let mut grid = menlo_grid();

        plan.format_rows(&mut contents, &mut grid, &mut row_dirty, input)
            .expect("format rows");

        // Column 0 is under the plan-owned preedit mask, so it is transparent and
        // has no foreground. Column 1 uses search colors and link underline.
        assert_eq!(*contents.bg_cell(0, 0), CellBg([0, 0, 0, 0]));
        assert!(contents.fg_rows()[1].iter().all(|v| v.grid_pos[0] != 0));
        assert_eq!(*contents.bg_cell(0, 1), CellBg([1, 2, 3, 255]));
        assert!(contents.fg_rows()[1].iter().any(|v| v.grid_pos == [1, 0]));
        assert!(contents.fg_rows()[1].len() >= 2);
        assert_eq!(row_dirty, vec![false]);
    }

    #[test]
    fn format_rows_wraps_driver_validation_without_mutation() {
        let mut plan =
            FrameRebuildPlan::build(input(grid(2, 1), grid(2, 1), RenderDirty::Full, &[true]))
                .expect("plan");
        plan.rows_to_rebuild = vec![0, 0];
        let rows = vec![row(b"AB", Color::Palette(1))];
        let mut contents = contents_with_rows(grid(2, 1));
        let bg = contents.bg_cells().to_vec();
        let fg = contents.fg_rows().to_vec();
        let mut row_dirty = vec![true];
        let mut grid = menlo_grid();

        let err = plan
            .format_rows(
                &mut contents,
                &mut grid,
                &mut row_dirty,
                format_input(&rows),
            )
            .expect_err("driver validation should propagate");

        assert_eq!(
            err,
            FrameRowFormatValidationError::Driver(
                FrameRowRebuildValidationError::DuplicateRebuildRow { row: 0 }
            )
        );
        assert_eq!(contents.bg_cells(), bg);
        assert_eq!(contents.fg_rows(), fg);
        assert_eq!(row_dirty, vec![true]);
    }

    fn text_overlay_input(
        preedit: Option<&Preedit>,
        cursor: Option<FrameCursorOverlay>,
    ) -> FrameTextOverlayInput<'_> {
        FrameTextOverlayInput {
            preedit,
            cursor,
            screen_fg: Rgb::new(220, 221, 222),
            alpha: 255,
        }
    }

    fn cursor_overlay(style: CursorStyle, grid_pos: [u16; 2]) -> FrameCursorOverlay {
        FrameCursorOverlay {
            grid_pos,
            style,
            wide: false,
            color: Rgb::new(7, 8, 9),
            alpha: 255,
        }
    }

    fn snapshot_cursor_overlay_input() -> FrameSnapshotCursorOverlayInput {
        FrameSnapshotCursorOverlayInput {
            style: CursorStyle::Underline,
            wide: true,
            color: Rgb::new(3, 4, 5),
            alpha: 219,
        }
    }

    fn snapshot_text_overlay_input(
        cursor: Option<FrameSnapshotCursorOverlayInput>,
    ) -> FrameSnapshotTextOverlayInput {
        FrameSnapshotTextOverlayInput {
            cursor,
            screen_fg: Rgb::new(40, 41, 42),
            alpha: 219,
        }
    }

    #[test]
    fn snapshot_overlay_text_input_borrows_preedit_and_threads_fields() {
        let mut terminal = terminal(4, 2);
        terminal.set_cursor_position_for_tests(2, 1);
        let snapshot = FrameTerminalSnapshot::collect(
            &terminal,
            grid(4, 2),
            RenderDirty::Full,
            Some(preedit(&[false])),
        );

        let input = snapshot.text_overlay_input(snapshot_text_overlay_input(Some(
            snapshot_cursor_overlay_input(),
        )));

        assert!(std::ptr::eq(
            input.preedit.expect("preedit"),
            snapshot.preedit.as_ref().expect("snapshot preedit")
        ));
        assert_eq!(input.screen_fg, Rgb::new(40, 41, 42));
        assert_eq!(input.alpha, 219);
        let cursor = input.cursor.expect("cursor");
        assert_eq!(cursor.grid_pos, [2, 1]);
        assert_eq!(cursor.style, CursorStyle::Underline);
        assert_eq!(cursor.wide, true);
        assert_eq!(cursor.color, Rgb::new(3, 4, 5));
        assert_eq!(cursor.alpha, 219);
    }

    #[test]
    fn snapshot_overlay_text_input_omits_cursor_without_snapshot_cursor() {
        let terminal = terminal(4, 2);
        let mut snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 2), RenderDirty::Full, None);
        snapshot.cursor_viewport = None;

        let input = snapshot.text_overlay_input(snapshot_text_overlay_input(Some(
            snapshot_cursor_overlay_input(),
        )));

        assert_eq!(input.preedit, None);
        assert!(input.cursor.is_none());
    }

    #[test]
    fn snapshot_overlay_text_input_feeds_cursor_driver() {
        let mut terminal = terminal(4, 2);
        terminal.set_cursor_position_for_tests(1, 0);
        let snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 2), RenderDirty::Full, None);
        let plan = snapshot.build_plan().expect("plan");
        let mut contents = Contents::default();
        contents.resize(grid(4, 2));
        let mut shared = menlo_grid();

        let applied = plan
            .draw_text_overlays(
                &mut contents,
                &mut shared,
                snapshot.text_overlay_input(snapshot_text_overlay_input(Some(
                    FrameSnapshotCursorOverlayInput {
                        style: CursorStyle::Block,
                        wide: false,
                        color: Rgb::new(7, 8, 9),
                        alpha: 203,
                    },
                ))),
            )
            .expect("draw overlays");

        assert!(applied.cursor_cleared);
        assert_eq!(applied.cursor_drawn, Some(CursorStyle::Block));
        assert_eq!(applied.preedit_drawn, false);
    }

    #[test]
    fn snapshot_overlay_text_input_feeds_preedit_suppression_driver() {
        let mut terminal = terminal(4, 2);
        terminal.set_cursor_position_for_tests(1, 0);
        let snapshot = FrameTerminalSnapshot::collect(
            &terminal,
            grid(4, 2),
            RenderDirty::Full,
            Some(preedit(&[false])),
        );
        let plan = snapshot.build_plan().expect("plan");
        let mut contents = Contents::default();
        contents.resize(grid(4, 2));
        let mut shared = menlo_grid();

        let applied = plan
            .draw_text_overlays(
                &mut contents,
                &mut shared,
                snapshot.text_overlay_input(snapshot_text_overlay_input(Some(
                    snapshot_cursor_overlay_input(),
                ))),
            )
            .expect("draw overlays");

        assert!(applied.cursor_cleared);
        assert_eq!(applied.cursor_drawn, None);
        assert!(applied.preedit_drawn);
    }

    fn plan_for_grid(size: GridSize) -> FrameRebuildPlan {
        FrameRebuildPlan::build(input(
            size,
            size,
            RenderDirty::Full,
            &vec![true; size.rows as usize],
        ))
        .expect("plan")
    }

    #[test]
    fn draw_text_overlays_clears_stale_cursor_without_overlay() {
        let plan = plan_for_grid(grid(3, 2));
        let mut contents = contents_with_rows(grid(3, 2));
        contents.set_cursor(Some(dummy_vertex(0, 99)), Some(CursorStyle::Block));
        let mut shared = menlo_grid();

        let applied = plan
            .draw_text_overlays(&mut contents, &mut shared, text_overlay_input(None, None))
            .expect("draw overlays");

        assert_eq!(
            applied,
            FrameTextOverlayApplication {
                cursor_cleared: true,
                cursor_drawn: None,
                preedit_drawn: false,
            }
        );
        assert_eq!(contents.get_cursor_glyph(), None);
    }

    #[test]
    fn draw_text_overlays_draws_cursor_overlay() {
        let plan = plan_for_grid(grid(4, 2));
        let mut contents = Contents::default();
        contents.resize(grid(4, 2));
        let mut shared = menlo_grid();
        let cursor = cursor_overlay(CursorStyle::Bar, [1, 0]);

        let applied = plan
            .draw_text_overlays(
                &mut contents,
                &mut shared,
                text_overlay_input(None, Some(cursor)),
            )
            .expect("draw overlays");

        assert_eq!(applied.cursor_drawn, Some(CursorStyle::Bar));
        let glyph = contents.get_cursor_glyph().expect("cursor glyph");
        assert_eq!(glyph.grid_pos, [1, 0]);
        assert!(contents.fg_rows()[0].is_empty());
        assert_eq!(contents.fg_rows()[3].len(), 1);
    }

    #[test]
    fn draw_text_overlays_places_block_cursor_in_first_reserved_row() {
        let plan = plan_for_grid(grid(4, 2));
        let mut contents = Contents::default();
        contents.resize(grid(4, 2));
        let mut shared = menlo_grid();
        let cursor = cursor_overlay(CursorStyle::Block, [2, 1]);

        plan.draw_text_overlays(
            &mut contents,
            &mut shared,
            text_overlay_input(None, Some(cursor)),
        )
        .expect("draw overlays");

        assert_eq!(contents.fg_rows()[0].len(), 1);
        assert!(contents.fg_rows()[3].is_empty());
        assert_eq!(contents.fg_rows()[0][0].grid_pos, [2, 1]);
    }

    #[test]
    fn draw_text_overlays_draws_preedit_and_suppresses_cursor() {
        let p = Preedit {
            codepoints: vec![
                Codepoint {
                    codepoint: 'A' as u32,
                    wide: false,
                },
                Codepoint {
                    codepoint: 'B' as u32,
                    wide: false,
                },
            ],
        };
        let mut plan = plan_for_grid(grid(4, 1));
        plan.preedit_range = Some(FramePreeditRange {
            row: 0,
            range: PreeditRange {
                start: 1,
                end: 2,
                cp_offset: 0,
            },
        });
        let mut contents = Contents::default();
        contents.resize(grid(4, 1));
        contents.set_cursor(Some(dummy_vertex(0, 88)), Some(CursorStyle::Block));
        let mut shared = menlo_grid();

        let applied = plan
            .draw_text_overlays(
                &mut contents,
                &mut shared,
                text_overlay_input(Some(&p), Some(cursor_overlay(CursorStyle::Bar, [0, 0]))),
            )
            .expect("draw overlays");

        assert_eq!(
            applied,
            FrameTextOverlayApplication {
                cursor_cleared: true,
                cursor_drawn: None,
                preedit_drawn: true,
            }
        );
        assert_eq!(contents.get_cursor_glyph(), None);
        let cols: Vec<u16> = contents.fg_rows()[1]
            .iter()
            .map(|vertex| vertex.grid_pos[0])
            .collect();
        assert_eq!(cols, [1, 1, 2, 2]);
    }

    #[test]
    fn draw_text_overlays_active_preedit_without_range_still_suppresses_cursor() {
        let p = preedit(&[false]);
        let plan = plan_for_grid(grid(3, 1));
        let mut contents = Contents::default();
        contents.resize(grid(3, 1));
        contents.set_cursor(Some(dummy_vertex(0, 77)), Some(CursorStyle::Block));
        let mut shared = menlo_grid();

        let applied = plan
            .draw_text_overlays(
                &mut contents,
                &mut shared,
                text_overlay_input(Some(&p), Some(cursor_overlay(CursorStyle::Bar, [0, 0]))),
            )
            .expect("draw overlays");

        assert_eq!(applied.cursor_drawn, None);
        assert!(!applied.preedit_drawn);
        assert_eq!(contents.get_cursor_glyph(), None);
        assert!(contents.fg_rows()[1].is_empty());
    }

    #[test]
    fn draw_text_overlays_accepts_plan_generated_wide_preedit_at_edge() {
        let p = preedit(&[true]);
        let plan = FrameRebuildPlan::build(FrameRebuildInput {
            current_grid: grid(1, 1),
            terminal_grid: grid(1, 1),
            dirty: RenderDirty::Full,
            row_dirty: &[true],
            preedit: Some(&p),
            cursor_viewport: Some(Coordinate::new(0, 0)),
        })
        .expect("plan");
        assert_eq!(
            plan.preedit_range,
            Some(FramePreeditRange {
                row: 0,
                range: PreeditRange {
                    start: 0,
                    end: 0,
                    cp_offset: 0,
                },
            })
        );
        let mut contents = Contents::default();
        contents.resize(grid(1, 1));
        contents.set_cursor(Some(dummy_vertex(0, 76)), Some(CursorStyle::Block));
        let mut shared = menlo_grid();

        let applied = plan
            .draw_text_overlays(
                &mut contents,
                &mut shared,
                text_overlay_input(Some(&p), Some(cursor_overlay(CursorStyle::Bar, [0, 0]))),
            )
            .expect("wide edge preedit should draw best-effort");

        assert!(applied.preedit_drawn);
        assert_eq!(applied.cursor_drawn, None);
        assert_eq!(contents.get_cursor_glyph(), None);
        assert_eq!(contents.fg_rows()[1].len(), 2);
    }

    #[test]
    fn empty_plan_generated_preedit_suppresses_cursor_without_range() {
        let p = Preedit::default();
        let plan = FrameRebuildPlan::build(FrameRebuildInput {
            current_grid: grid(2, 1),
            terminal_grid: grid(2, 1),
            dirty: RenderDirty::Full,
            row_dirty: &[true],
            preedit: Some(&p),
            cursor_viewport: Some(Coordinate::new(0, 0)),
        })
        .expect("plan");
        assert_eq!(plan.preedit_range, None);
        let mut contents = Contents::default();
        contents.resize(grid(2, 1));
        contents.set_cursor(Some(dummy_vertex(0, 75)), Some(CursorStyle::Block));
        let mut shared = menlo_grid();

        let applied = plan
            .draw_text_overlays(
                &mut contents,
                &mut shared,
                text_overlay_input(Some(&p), Some(cursor_overlay(CursorStyle::Bar, [0, 0]))),
            )
            .expect("empty preedit should suppress cursor");

        assert_eq!(applied.cursor_drawn, None);
        assert!(!applied.preedit_drawn);
        assert_eq!(contents.get_cursor_glyph(), None);
        assert!(contents.fg_rows()[1].is_empty());
    }

    #[test]
    fn draw_text_overlays_rejects_contents_grid_mismatch_without_mutation() {
        let plan = plan_for_grid(grid(2, 1));
        let mut contents = Contents::default();
        contents.resize(grid(3, 1));
        contents.set_cursor(Some(dummy_vertex(0, 66)), Some(CursorStyle::Block));
        let fg = contents.fg_rows().to_vec();
        let mut shared = menlo_grid();

        let err = plan
            .draw_text_overlays(&mut contents, &mut shared, text_overlay_input(None, None))
            .expect_err("contents mismatch should reject before mutation");

        assert_eq!(
            err,
            FrameTextOverlayError::Validation(
                FrameTextOverlayValidationError::ContentsGridMismatch {
                    expected: grid(2, 1),
                    actual: grid(3, 1),
                }
            )
        );
        assert_eq!(contents.fg_rows(), fg);
        assert!(contents.get_cursor_glyph().is_some());
    }

    #[test]
    fn draw_text_overlays_rejects_wide_cursor_extent_without_mutation() {
        let plan = plan_for_grid(grid(2, 1));
        let mut contents = Contents::default();
        contents.resize(grid(2, 1));
        contents.set_cursor(Some(dummy_vertex(0, 55)), Some(CursorStyle::Block));
        let fg = contents.fg_rows().to_vec();
        let mut shared = menlo_grid();
        let mut cursor = cursor_overlay(CursorStyle::Block, [1, 0]);
        cursor.wide = true;

        let err = plan
            .draw_text_overlays(
                &mut contents,
                &mut shared,
                text_overlay_input(None, Some(cursor)),
            )
            .expect_err("wide cursor should reject before mutation");

        assert_eq!(
            err,
            FrameTextOverlayError::Validation(
                FrameTextOverlayValidationError::WideCursorOutOfBounds {
                    grid_pos: [1, 0],
                    columns: 2,
                }
            )
        );
        assert_eq!(contents.fg_rows(), fg);
    }

    #[test]
    fn draw_text_overlays_rejects_invalid_preedit_payload_without_mutation() {
        let p = preedit(&[false]);
        let mut plan = plan_for_grid(grid(3, 1));
        plan.preedit_range = Some(FramePreeditRange {
            row: 0,
            range: PreeditRange {
                start: 0,
                end: 1,
                cp_offset: 0,
            },
        });
        let mut contents = Contents::default();
        contents.resize(grid(3, 1));
        contents.set_cursor(Some(dummy_vertex(0, 44)), Some(CursorStyle::Block));
        let fg = contents.fg_rows().to_vec();
        let mut shared = menlo_grid();

        let err = plan
            .draw_text_overlays(
                &mut contents,
                &mut shared,
                text_overlay_input(Some(&p), None),
            )
            .expect_err("payload width should reject before mutation");

        assert_eq!(
            err,
            FrameTextOverlayError::Validation(
                FrameTextOverlayValidationError::PreeditWidthMismatch {
                    expected: 2,
                    actual: 1,
                }
            )
        );
        assert_eq!(contents.fg_rows(), fg);
        assert!(contents.get_cursor_glyph().is_some());
    }

    #[test]
    fn draw_text_overlays_rejects_too_long_preedit_payload_without_mutation() {
        let p = preedit(&[false, false]);
        let mut plan = plan_for_grid(grid(2, 1));
        plan.preedit_range = Some(FramePreeditRange {
            row: 0,
            range: PreeditRange {
                start: 0,
                end: 0,
                cp_offset: 0,
            },
        });
        let mut contents = Contents::default();
        contents.resize(grid(2, 1));
        contents.set_cursor(Some(dummy_vertex(0, 43)), Some(CursorStyle::Block));
        let fg = contents.fg_rows().to_vec();
        let mut shared = menlo_grid();

        let err = plan
            .draw_text_overlays(
                &mut contents,
                &mut shared,
                text_overlay_input(Some(&p), None),
            )
            .expect_err("too-long payload should reject before mutation");

        assert_eq!(
            err,
            FrameTextOverlayError::Validation(
                FrameTextOverlayValidationError::PreeditWidthMismatch {
                    expected: 1,
                    actual: 2,
                }
            )
        );
        assert_eq!(contents.fg_rows(), fg);
        assert!(contents.get_cursor_glyph().is_some());
    }

    #[test]
    fn draw_text_overlays_rejects_missing_preedit_payload_without_mutation() {
        let mut plan = plan_for_grid(grid(3, 1));
        plan.preedit_range = Some(FramePreeditRange {
            row: 0,
            range: PreeditRange {
                start: 0,
                end: 0,
                cp_offset: 0,
            },
        });
        let mut contents = Contents::default();
        contents.resize(grid(3, 1));
        contents.set_cursor(Some(dummy_vertex(0, 33)), Some(CursorStyle::Block));
        let fg = contents.fg_rows().to_vec();
        let mut shared = menlo_grid();

        let err = plan
            .draw_text_overlays(&mut contents, &mut shared, text_overlay_input(None, None))
            .expect_err("missing preedit should reject before mutation");

        assert_eq!(
            err,
            FrameTextOverlayError::Validation(FrameTextOverlayValidationError::MissingPreedit)
        );
        assert_eq!(contents.fg_rows(), fg);
    }

    fn cursor_uniform_input(
        preedit_active: bool,
        block_cursor: Option<FrameBlockCursorUniform>,
    ) -> FrameCursorUniformInput {
        FrameCursorUniformInput {
            preedit_active,
            block_cursor,
        }
    }

    fn block_cursor_uniform(grid_pos: [u16; 2], wide: Wide, color: Rgb) -> FrameBlockCursorUniform {
        FrameBlockCursorUniform {
            grid_pos,
            wide,
            color,
        }
    }

    fn snapshot_block_cursor_uniform_input() -> FrameSnapshotBlockCursorUniformInput {
        FrameSnapshotBlockCursorUniformInput {
            wide: Wide::Wide,
            color: Rgb::new(11, 12, 13),
        }
    }

    fn snapshot_cursor_uniform_input(
        block_cursor: Option<FrameSnapshotBlockCursorUniformInput>,
    ) -> FrameSnapshotCursorUniformInput {
        FrameSnapshotCursorUniformInput { block_cursor }
    }

    #[test]
    fn snapshot_overlay_cursor_uniform_input_derives_preedit_and_threads_cursor() {
        let mut terminal = terminal(4, 2);
        terminal.set_cursor_position_for_tests(2, 1);
        let snapshot = FrameTerminalSnapshot::collect(
            &terminal,
            grid(4, 2),
            RenderDirty::Full,
            Some(preedit(&[false])),
        );

        let input = snapshot.cursor_uniform_input(snapshot_cursor_uniform_input(Some(
            snapshot_block_cursor_uniform_input(),
        )));

        assert!(input.preedit_active);
        let cursor = input.block_cursor.expect("block cursor");
        assert_eq!(cursor.grid_pos, [2, 1]);
        assert_eq!(cursor.wide, Wide::Wide);
        assert_eq!(cursor.color, Rgb::new(11, 12, 13));
    }

    #[test]
    fn snapshot_overlay_cursor_uniform_omits_cursor_without_snapshot_cursor() {
        let terminal = terminal(4, 2);
        let mut snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 2), RenderDirty::Full, None);
        snapshot.cursor_viewport = None;

        let input = snapshot.cursor_uniform_input(snapshot_cursor_uniform_input(Some(
            snapshot_block_cursor_uniform_input(),
        )));

        assert_eq!(input.preedit_active, false);
        assert!(input.block_cursor.is_none());
    }

    #[test]
    fn snapshot_overlay_cursor_uniform_feeds_block_driver() {
        let mut terminal = terminal(4, 2);
        terminal.set_cursor_position_for_tests(1, 0);
        let snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 2), RenderDirty::Full, None);
        let plan = snapshot.build_plan().expect("plan");
        let mut uniforms = uniforms_with_stale_cursor();

        let applied = plan
            .apply_cursor_uniforms(
                &mut uniforms,
                snapshot.cursor_uniform_input(snapshot_cursor_uniform_input(Some(
                    snapshot_block_cursor_uniform_input(),
                ))),
            )
            .expect("cursor uniforms");

        assert!(applied.cursor_cleared);
        assert!(applied.block_cursor_applied);
        assert_eq!(uniforms.cursor_pos, [1, 0]);
        assert_eq!(uniforms.cursor_color, [11, 12, 13, 255]);
        assert!(uniforms.bools.cursor_wide);
    }

    #[test]
    fn snapshot_overlay_cursor_uniform_feeds_preedit_suppression_driver() {
        let mut terminal = terminal(4, 2);
        terminal.set_cursor_position_for_tests(1, 0);
        let snapshot = FrameTerminalSnapshot::collect(
            &terminal,
            grid(4, 2),
            RenderDirty::Full,
            Some(preedit(&[false])),
        );
        let plan = snapshot.build_plan().expect("plan");
        let mut uniforms = uniforms_with_stale_cursor();

        let applied = plan
            .apply_cursor_uniforms(
                &mut uniforms,
                snapshot.cursor_uniform_input(snapshot_cursor_uniform_input(Some(
                    snapshot_block_cursor_uniform_input(),
                ))),
            )
            .expect("cursor uniforms");

        assert!(applied.cursor_cleared);
        assert_eq!(applied.block_cursor_applied, false);
        assert_eq!(uniforms.cursor_pos, [u16::MAX, u16::MAX]);
    }

    fn uniforms_with_stale_cursor() -> MetalUniforms {
        let mut uniforms =
            MetalUniforms::test_with_grid([2, 3], [4, 5], [6.0, 7.0], [0.0; 4], 0, [1, 2, 3, 4]);
        uniforms.cursor_pos = [1, 2];
        uniforms.cursor_color = [9, 9, 9, 9];
        uniforms.bools.cursor_wide = true;
        uniforms
    }

    #[test]
    fn apply_cursor_uniforms_clears_position_without_cursor() {
        let plan = plan_for_grid(grid(4, 2));
        let mut uniforms = uniforms_with_stale_cursor();

        let applied = plan
            .apply_cursor_uniforms(&mut uniforms, cursor_uniform_input(false, None))
            .expect("apply cursor uniforms");

        assert_eq!(
            applied,
            FrameCursorUniformApplication {
                cursor_cleared: true,
                block_cursor_applied: false,
            }
        );
        assert_eq!(uniforms.cursor_pos, [u16::MAX, u16::MAX]);
        assert_eq!(uniforms.cursor_color, [9, 9, 9, 9]);
        assert!(uniforms.bools.cursor_wide);
    }

    #[test]
    fn apply_cursor_uniforms_preedit_suppresses_block_cursor() {
        let plan = plan_for_grid(grid(4, 2));
        let mut uniforms = uniforms_with_stale_cursor();
        let cursor = block_cursor_uniform([1, 0], Wide::Narrow, Rgb::new(10, 20, 30));

        let applied = plan
            .apply_cursor_uniforms(&mut uniforms, cursor_uniform_input(true, Some(cursor)))
            .expect("apply cursor uniforms");

        assert_eq!(
            applied,
            FrameCursorUniformApplication {
                cursor_cleared: true,
                block_cursor_applied: false,
            }
        );
        assert_eq!(uniforms.cursor_pos, [u16::MAX, u16::MAX]);
        assert_eq!(uniforms.cursor_color, [9, 9, 9, 9]);
        assert!(uniforms.bools.cursor_wide);
    }

    #[test]
    fn apply_cursor_uniforms_sets_block_cursor() {
        let plan = plan_for_grid(grid(5, 3));
        let mut uniforms = uniforms_with_stale_cursor();
        let cursor = block_cursor_uniform([4, 2], Wide::SpacerTail, Rgb::new(10, 20, 30));

        let applied = plan
            .apply_cursor_uniforms(&mut uniforms, cursor_uniform_input(false, Some(cursor)))
            .expect("apply cursor uniforms");

        assert_eq!(
            applied,
            FrameCursorUniformApplication {
                cursor_cleared: true,
                block_cursor_applied: true,
            }
        );
        assert_eq!(uniforms.cursor_pos, [3, 2]);
        assert!(uniforms.bools.cursor_wide);
        assert_eq!(uniforms.cursor_color, [10, 20, 30, 255]);
    }

    #[test]
    fn apply_cursor_uniforms_keeps_spacer_tail_zero_backstep() {
        let plan = plan_for_grid(grid(2, 1));
        let mut uniforms = uniforms_with_stale_cursor();
        let cursor = block_cursor_uniform([0, 0], Wide::SpacerTail, Rgb::new(1, 2, 3));

        plan.apply_cursor_uniforms(&mut uniforms, cursor_uniform_input(false, Some(cursor)))
            .expect("apply cursor uniforms");

        assert_eq!(uniforms.cursor_pos, [0, 0]);
        assert!(uniforms.bools.cursor_wide);
        assert_eq!(uniforms.cursor_color, [1, 2, 3, 255]);
    }

    #[test]
    fn apply_cursor_uniforms_rejects_out_of_bounds_without_mutation() {
        let plan = plan_for_grid(grid(2, 1));
        let mut uniforms = uniforms_with_stale_cursor();
        let before = uniforms;
        let cursor = block_cursor_uniform([2, 0], Wide::Narrow, Rgb::new(10, 20, 30));

        let err = plan
            .apply_cursor_uniforms(&mut uniforms, cursor_uniform_input(false, Some(cursor)))
            .expect_err("out-of-bounds cursor should reject");

        assert_eq!(
            err,
            FrameCursorUniformValidationError::CursorOutOfBounds {
                grid_pos: [2, 0],
                size: grid(2, 1),
            }
        );
        assert_eq!(uniforms.cursor_pos, before.cursor_pos);
        assert_eq!(uniforms.cursor_color, before.cursor_color);
        assert_eq!(uniforms.bools.cursor_wide, before.bools.cursor_wide);
    }

    #[test]
    fn apply_cursor_uniforms_rejects_wide_last_column_without_mutation() {
        let plan = plan_for_grid(grid(2, 1));
        let mut uniforms = uniforms_with_stale_cursor();
        let before = uniforms;
        let cursor = block_cursor_uniform([1, 0], Wide::Wide, Rgb::new(10, 20, 30));

        let err = plan
            .apply_cursor_uniforms(&mut uniforms, cursor_uniform_input(false, Some(cursor)))
            .expect_err("wide cursor should reject");

        assert_eq!(
            err,
            FrameCursorUniformValidationError::WideCursorOutOfBounds {
                grid_pos: [1, 0],
                columns: 2,
            }
        );
        assert_eq!(uniforms.cursor_pos, before.cursor_pos);
        assert_eq!(uniforms.cursor_color, before.cursor_color);
        assert_eq!(uniforms.bools.cursor_wide, before.bools.cursor_wide);
    }

    fn custom_shader_input(cursor: Option<CellTextVertex>) -> FrameCustomShaderInput {
        FrameCustomShaderInput {
            time_secs: 1.5,
            time_delta_secs: 0.25,
            screen_size: [80, 24],
            cell_size: [8, 16],
            padding: [4, 5],
            cursor,
            focused: true,
            focus_changed: false,
        }
    }

    fn custom_cursor_vertex() -> CellTextVertex {
        CellTextVertex {
            glyph_pos: [0, 0],
            glyph_size: [6, 7],
            bearings: [1, 2],
            grid_pos: [2, 3],
            color: [64, 128, 255, 128],
            atlas: CellTextAtlas::Grayscale,
            flags: CellTextFlags::new(false, true),
            _padding: [0, 0],
        }
    }

    #[test]
    fn apply_custom_shader_frame_updates_time_and_resolution() {
        let plan = plan_for_grid(grid(2, 1));
        let mut uniforms = CustomShaderUniforms::new();

        let applied = plan
            .apply_custom_shader_frame(&mut uniforms, custom_shader_input(None))
            .expect("apply custom shader frame");

        assert_eq!(
            applied,
            FrameCustomShaderApplication {
                frame_updated: true,
                cursor_supplied: false,
                focus_changed_consumed: false,
                next_focus_changed: false,
            }
        );
        assert_eq!(uniforms.time, 1.5);
        assert_eq!(uniforms.time_delta, 0.25);
        assert_eq!(uniforms.frame, 1);
        assert_eq!(uniforms.resolution, [80.0, 24.0, 1.0]);
        assert_eq!(uniforms.channel_resolution[0], [80.0, 24.0, 1.0, 0.0]);
    }

    #[test]
    fn apply_custom_shader_frame_updates_cursor_rect_and_color() {
        let plan = plan_for_grid(grid(2, 1));
        let mut uniforms = CustomShaderUniforms::new();
        let cursor = custom_cursor_vertex();

        let applied = plan
            .apply_custom_shader_frame(&mut uniforms, custom_shader_input(Some(cursor)))
            .expect("apply custom shader frame");

        assert_eq!(applied.cursor_supplied, true);
        assert_eq!(uniforms.current_cursor, [21.0, 74.0, 6.0, 7.0]);
        assert_eq!(
            uniforms.current_cursor_color,
            [64.0 / 255.0, 128.0 / 255.0, 1.0, 128.0 / 255.0]
        );
        assert_eq!(uniforms.previous_cursor, [0.0; 4]);
        assert_eq!(uniforms.previous_cursor_color, [0.0; 4]);
        assert_eq!(uniforms.cursor_change_time, 1.5);
    }

    #[test]
    fn apply_custom_shader_frame_without_cursor_preserves_cursor_fields() {
        let plan = plan_for_grid(grid(2, 1));
        let mut uniforms = CustomShaderUniforms::new();
        uniforms.current_cursor = [1.0, 2.0, 3.0, 4.0];
        uniforms.current_cursor_color = [0.1, 0.2, 0.3, 0.4];
        uniforms.previous_cursor = [5.0, 6.0, 7.0, 8.0];
        uniforms.previous_cursor_color = [0.5, 0.6, 0.7, 0.8];
        uniforms.cursor_change_time = 9.0;

        plan.apply_custom_shader_frame(&mut uniforms, custom_shader_input(None))
            .expect("apply custom shader frame");

        assert_eq!(uniforms.current_cursor, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(uniforms.current_cursor_color, [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(uniforms.previous_cursor, [5.0, 6.0, 7.0, 8.0]);
        assert_eq!(uniforms.previous_cursor_color, [0.5, 0.6, 0.7, 0.8]);
        assert_eq!(uniforms.cursor_change_time, 9.0);
        assert_eq!(uniforms.time, 1.5);
    }

    #[test]
    fn apply_custom_shader_frame_unchanged_cursor_does_not_restamp_change_time() {
        let plan = plan_for_grid(grid(2, 1));
        let mut uniforms = CustomShaderUniforms::new();
        let cursor = custom_cursor_vertex();

        plan.apply_custom_shader_frame(&mut uniforms, custom_shader_input(Some(cursor)))
            .expect("first frame");
        assert_eq!(uniforms.cursor_change_time, 1.5);
        let previous_cursor = uniforms.previous_cursor;
        let previous_color = uniforms.previous_cursor_color;

        let mut input = custom_shader_input(Some(cursor));
        input.time_secs = 3.0;
        input.time_delta_secs = 1.5;
        plan.apply_custom_shader_frame(&mut uniforms, input)
            .expect("second frame");

        assert_eq!(uniforms.time, 3.0);
        assert_eq!(uniforms.cursor_change_time, 1.5);
        assert_eq!(uniforms.previous_cursor, previous_cursor);
        assert_eq!(uniforms.previous_cursor_color, previous_color);
    }

    #[test]
    fn apply_custom_shader_frame_consumes_focus_changed_when_focused() {
        let plan = plan_for_grid(grid(2, 1));
        let mut uniforms = CustomShaderUniforms::new();
        let mut input = custom_shader_input(None);
        input.time_secs = 4.0;
        input.focused = true;
        input.focus_changed = true;

        let applied = plan
            .apply_custom_shader_frame(&mut uniforms, input)
            .expect("apply custom shader frame");

        assert_eq!(uniforms.focus, 1);
        assert_eq!(uniforms.time_focus, 4.0);
        assert!(applied.focus_changed_consumed);
        assert!(!applied.next_focus_changed);
    }

    #[test]
    fn apply_custom_shader_frame_preserves_focus_changed_when_unfocused() {
        let plan = plan_for_grid(grid(2, 1));
        let mut uniforms = CustomShaderUniforms::new();
        uniforms.time_focus = 2.0;
        let mut input = custom_shader_input(None);
        input.time_secs = 4.0;
        input.focused = false;
        input.focus_changed = true;

        let applied = plan
            .apply_custom_shader_frame(&mut uniforms, input)
            .expect("apply custom shader frame");

        assert_eq!(uniforms.focus, 0);
        assert_eq!(uniforms.time_focus, 2.0);
        assert!(!applied.focus_changed_consumed);
        assert!(applied.next_focus_changed);
    }

    #[test]
    fn apply_custom_shader_frame_allows_zero_screen_size() {
        let plan = plan_for_grid(grid(2, 1));
        let mut uniforms = CustomShaderUniforms::new();
        let mut input = custom_shader_input(None);
        input.screen_size = [0, 0];

        plan.apply_custom_shader_frame(&mut uniforms, input)
            .expect("zero screen size is a valid prepared input");

        assert_eq!(uniforms.resolution, [0.0, 0.0, 1.0]);
        assert_eq!(uniforms.channel_resolution[0], [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(uniforms.frame, 1);
    }

    #[test]
    fn apply_custom_shader_frame_allows_zero_cell_size_without_cursor() {
        let plan = plan_for_grid(grid(2, 1));
        let mut uniforms = CustomShaderUniforms::new();
        let mut input = custom_shader_input(None);
        input.cell_size = [0, 0];
        input.focus_changed = true;

        let applied = plan
            .apply_custom_shader_frame(&mut uniforms, input)
            .expect("zero cell size without cursor is valid");

        assert_eq!(uniforms.time, 1.5);
        assert_eq!(uniforms.frame, 1);
        assert!(applied.focus_changed_consumed);
        assert_eq!(uniforms.current_cursor, [0.0; 4]);
    }

    #[test]
    fn apply_custom_shader_frame_rejects_zero_cell_size_with_cursor_without_mutation() {
        let plan = plan_for_grid(grid(2, 1));
        let mut uniforms = CustomShaderUniforms::new();
        uniforms.time = 8.0;
        uniforms.frame = 4;
        uniforms.current_cursor = [1.0, 2.0, 3.0, 4.0];
        let before = uniforms;
        let mut input = custom_shader_input(Some(custom_cursor_vertex()));
        input.cell_size = [0, 16];

        let err = plan
            .apply_custom_shader_frame(&mut uniforms, input)
            .expect_err("zero cell size with cursor should reject before mutation");

        assert_eq!(
            err,
            FrameCustomShaderValidationError::ZeroCellSizeWithCursor { cell_size: [0, 16] }
        );
        assert_eq!(uniforms.time, before.time);
        assert_eq!(uniforms.frame, before.frame);
        assert_eq!(uniforms.current_cursor, before.current_cursor);
        assert_eq!(uniforms.resolution, before.resolution);
    }

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

    fn metal_uniforms(screen_size: [u16; 2], grid_size: [u16; 2]) -> MetalUniforms {
        let mut uniforms = MetalUniforms::test_with_grid(
            screen_size,
            grid_size,
            [
                screen_size[0] as f32 / grid_size[0].max(1) as f32,
                screen_size[1] as f32 / grid_size[1].max(1) as f32,
            ],
            [0.0; 4],
            0,
            [0, 0, 0, 0],
        );
        uniforms.projection_matrix =
            ortho2d(0.0, screen_size[0] as f32, screen_size[1] as f32, 0.0);
        uniforms.cursor_pos = [u16::MAX, u16::MAX];
        uniforms
    }

    fn metal_presentation_input<'a>(
        width: usize,
        height: usize,
        contents_scale: f64,
        uniforms: &'a MetalUniforms,
        contents: &'a Contents,
        grayscale: &'a Atlas,
        color: &'a Atlas,
    ) -> FrameMetalPresentationInput<'a> {
        FrameMetalPresentationInput {
            width,
            height,
            contents_scale,
            uniforms,
            contents,
            grayscale_atlas: grayscale,
            color_atlas: color,
        }
    }

    fn assert_validation_error(
        error: FrameMetalPresentationError,
        expected: FrameMetalPresentationValidationError,
    ) {
        match error {
            FrameMetalPresentationError::Validation(actual) => assert_eq!(actual, expected),
            FrameMetalPresentationError::Compositor(other) => {
                panic!("expected validation error, got compositor error: {other:?}")
            }
        }
    }

    fn assert_compositor_invalid_scale(error: FrameMetalPresentationError) {
        match error {
            FrameMetalPresentationError::Compositor(
                MetalFrameCompositorError::InvalidContentsScale,
            ) => {}
            other => panic!("expected invalid contents scale, got {other:?}"),
        }
    }

    #[test]
    fn present_metal_frame_rejects_zero_dimensions_before_compositor() {
        let grayscale = Atlas::new(8, Format::Grayscale);
        let color = Atlas::new(8, Format::Bgra);
        let plan = plan_for_grid(grid(1, 1));
        let contents = contents_with_rows(grid(1, 1));
        let uniforms = metal_uniforms([2, 2], [1, 1]);

        let err = plan
            .validate_metal_presentation_input(&metal_presentation_input(
                0,
                2,
                f64::NAN,
                &uniforms,
                &contents,
                &grayscale,
                &color,
            ))
            .expect_err("zero width should reject before compositor scale validation");

        assert_eq!(
            err,
            FrameMetalPresentationValidationError::ZeroDimensions {
                width: 0,
                height: 2,
            },
        );
    }

    #[test]
    fn present_metal_frame_rejects_contents_grid_mismatch_before_compositor() {
        let grayscale = Atlas::new(8, Format::Grayscale);
        let color = Atlas::new(8, Format::Bgra);
        let plan = plan_for_grid(grid(1, 1));
        let contents = contents_with_rows(grid(2, 1));
        let uniforms = metal_uniforms([2, 2], [1, 1]);

        let err = plan
            .validate_metal_presentation_input(&metal_presentation_input(
                2,
                2,
                f64::NAN,
                &uniforms,
                &contents,
                &grayscale,
                &color,
            ))
            .expect_err("contents mismatch should reject before compositor scale validation");

        assert_eq!(
            err,
            FrameMetalPresentationValidationError::ContentsGridMismatch {
                expected: grid(1, 1),
                actual: grid(2, 1),
            },
        );
    }

    #[test]
    fn present_metal_frame_rejects_uniform_grid_mismatch_before_compositor() {
        let grayscale = Atlas::new(8, Format::Grayscale);
        let color = Atlas::new(8, Format::Bgra);
        let plan = plan_for_grid(grid(1, 1));
        let contents = contents_with_rows(grid(1, 1));
        let uniforms = metal_uniforms([2, 2], [2, 1]);

        let err = plan
            .validate_metal_presentation_input(&metal_presentation_input(
                2,
                2,
                f64::NAN,
                &uniforms,
                &contents,
                &grayscale,
                &color,
            ))
            .expect_err("uniform mismatch should reject before compositor scale validation");

        assert_eq!(
            err,
            FrameMetalPresentationValidationError::UniformGridMismatch {
                expected: [1, 1],
                actual: [2, 1],
            },
        );
    }

    #[test]
    fn present_metal_frame_propagates_invalid_contents_scale_from_compositor() {
        let Some(device) = metal_device() else {
            return;
        };
        let grayscale = Atlas::new(8, Format::Grayscale);
        let color = Atlas::new(8, Format::Bgra);
        let mut compositor = metal_compositor(device, 2, 2, &grayscale, &color);
        let plan = plan_for_grid(grid(1, 1));
        let contents = contents_with_rows(grid(1, 1));
        let uniforms = metal_uniforms([2, 2], [1, 1]);

        let err = plan
            .present_metal_frame(
                &mut compositor,
                metal_presentation_input(2, 2, 0.0, &uniforms, &contents, &grayscale, &color),
            )
            .expect_err("invalid scale should come from compositor");

        assert_compositor_invalid_scale(err);
    }

    #[test]
    fn present_metal_frame_presents_background_only_frame() {
        let Some(device) = metal_device() else {
            return;
        };
        let grayscale = Atlas::new(8, Format::Grayscale);
        let color = Atlas::new(8, Format::Bgra);
        let mut compositor = metal_compositor(device, 2, 2, &grayscale, &color);
        let plan = plan_for_grid(grid(1, 1));
        let mut contents = Contents::default();
        contents.resize(grid(1, 1));
        *contents.bg_cell_mut(0, 0) = CellBg([0, 255, 0, 255]);
        let uniforms = metal_uniforms([2, 2], [1, 1]);

        let applied = plan
            .present_metal_frame(
                &mut compositor,
                metal_presentation_input(2, 2, 1.0, &uniforms, &contents, &grayscale, &color),
            )
            .expect("frame should present");

        assert_eq!(
            applied.presentation,
            MetalFramePresentation {
                fg_count: 0,
                mode: applied.presentation.mode,
                width: 2,
                height: 2,
                target_reallocated: false,
            }
        );
        assert!(!applied.foreground_drawn);
        assert!(!applied.target_reallocated);
    }

    #[test]
    fn present_metal_frame_reports_foreground_count() {
        let Some(device) = metal_device() else {
            return;
        };
        let mut grayscale = Atlas::new(8, Format::Grayscale);
        let region = grayscale.reserve(2, 2).expect("reserve glyph region");
        grayscale.set(region, &[255, 255, 255, 255]);
        let color = Atlas::new(8, Format::Bgra);
        let mut compositor = metal_compositor(device, 2, 2, &grayscale, &color);
        let plan = plan_for_grid(grid(1, 1));
        let mut contents = Contents::default();
        contents.resize(grid(1, 1));
        contents.add(
            Key::Text,
            CellTextVertex {
                glyph_pos: [region.x, region.y],
                glyph_size: [2, 2],
                bearings: [0, 2],
                grid_pos: [0, 0],
                color: [255, 0, 0, 255],
                atlas: CellTextAtlas::Grayscale,
                flags: CellTextFlags::new(false, false),
                _padding: [0, 0],
            },
        );
        let uniforms = metal_uniforms([2, 2], [1, 1]);

        let applied = plan
            .present_metal_frame(
                &mut compositor,
                metal_presentation_input(2, 2, 1.0, &uniforms, &contents, &grayscale, &color),
            )
            .expect("frame should present");

        assert_eq!(applied.presentation.fg_count, 1);
        assert!(applied.foreground_drawn);
        assert!(!applied.target_reallocated);
    }

    #[test]
    fn present_metal_frame_reports_target_reallocation() {
        let Some(device) = metal_device() else {
            return;
        };
        let grayscale = Atlas::new(8, Format::Grayscale);
        let color = Atlas::new(8, Format::Bgra);
        let mut compositor = metal_compositor(device, 2, 2, &grayscale, &color);
        let plan = plan_for_grid(grid(1, 1));
        let mut contents = Contents::default();
        contents.resize(grid(1, 1));
        let uniforms = metal_uniforms([4, 4], [1, 1]);

        let applied = plan
            .present_metal_frame(
                &mut compositor,
                metal_presentation_input(4, 4, 1.0, &uniforms, &contents, &grayscale, &color),
            )
            .expect("resized frame should present");

        assert_eq!(applied.presentation.width, 4);
        assert_eq!(applied.presentation.height, 4);
        assert!(applied.presentation.target_reallocated);
        assert!(applied.target_reallocated);
    }

    fn rebuild_uniform_input(padding_color: WindowPaddingColor) -> FrameRebuildUniformInput {
        FrameRebuildUniformInput { padding_color }
    }

    fn rebuild_uniforms() -> MetalUniforms {
        MetalUniforms::test_with_grid([2, 3], [4, 5], [6.0, 7.0], [0.0; 4], 9, [1, 2, 3, 4])
    }

    #[test]
    fn apply_rebuild_uniforms_full_rebuild_resets_padding_only() {
        let plan =
            FrameRebuildPlan::build(input(grid(4, 5), grid(4, 5), RenderDirty::Full, &[true; 5]))
                .expect("plan");
        let mut uniforms = rebuild_uniforms();

        let applied = plan
            .apply_rebuild_uniforms(
                &mut uniforms,
                rebuild_uniform_input(WindowPaddingColor::Extend),
            )
            .expect("apply rebuild uniforms");

        assert_eq!(
            applied,
            FrameRebuildUniformApplication {
                grid_size_updated: false,
                padding_extend_mutated: true,
                effective_grid: grid(4, 5),
            }
        );
        assert_eq!(uniforms.grid_size, [4, 5]);
        assert_eq!(uniforms.padding_extend, 15);
        assert_eq!(uniforms.bg_color, [1, 2, 3, 4]);
    }

    #[test]
    fn apply_rebuild_uniforms_resize_full_rebuild_updates_grid_then_padding() {
        let plan = FrameRebuildPlan::build(input(
            grid(4, 5),
            grid(7, 3),
            RenderDirty::Clean,
            &[true; 3],
        ))
        .expect("plan");
        assert!(plan.full_rebuild);
        let mut uniforms = rebuild_uniforms();
        uniforms.padding_extend = 0;

        let applied = plan
            .apply_rebuild_uniforms(
                &mut uniforms,
                rebuild_uniform_input(WindowPaddingColor::ExtendAlways),
            )
            .expect("apply rebuild uniforms");

        assert_eq!(
            applied,
            FrameRebuildUniformApplication {
                grid_size_updated: true,
                padding_extend_mutated: true,
                effective_grid: grid(7, 3),
            }
        );
        assert_eq!(uniforms.grid_size, [7, 3]);
        assert_eq!(uniforms.padding_extend, 15);
    }

    #[test]
    fn apply_rebuild_uniforms_clean_partial_no_resize_is_noop() {
        let plan = FrameRebuildPlan::build(input(
            grid(4, 5),
            grid(4, 5),
            RenderDirty::Partial,
            &[false; 5],
        ))
        .expect("plan");
        let mut uniforms = rebuild_uniforms();
        let before = uniforms;

        let applied = plan
            .apply_rebuild_uniforms(
                &mut uniforms,
                rebuild_uniform_input(WindowPaddingColor::Extend),
            )
            .expect("apply rebuild uniforms");

        assert_eq!(
            applied,
            FrameRebuildUniformApplication {
                grid_size_updated: false,
                padding_extend_mutated: false,
                effective_grid: grid(4, 5),
            }
        );
        assert_eq!(uniforms.grid_size, before.grid_size);
        assert_eq!(uniforms.padding_extend, before.padding_extend);
        assert_eq!(uniforms.bg_color, before.bg_color);
    }

    #[test]
    fn apply_rebuild_uniforms_background_padding_reports_no_mutation() {
        let plan =
            FrameRebuildPlan::build(input(grid(4, 5), grid(4, 5), RenderDirty::Full, &[true; 5]))
                .expect("plan");
        let mut uniforms = rebuild_uniforms();
        uniforms.padding_extend = 9;

        let applied = plan
            .apply_rebuild_uniforms(
                &mut uniforms,
                rebuild_uniform_input(WindowPaddingColor::Background),
            )
            .expect("apply rebuild uniforms");

        assert_eq!(
            applied,
            FrameRebuildUniformApplication {
                grid_size_updated: false,
                padding_extend_mutated: false,
                effective_grid: grid(4, 5),
            }
        );
        assert_eq!(uniforms.padding_extend, 9);
    }

    #[test]
    fn apply_rebuild_uniforms_rejects_resize_effective_mismatch_without_mutation() {
        let mut plan = FrameRebuildPlan::build(input(
            grid(4, 5),
            grid(7, 3),
            RenderDirty::Clean,
            &[true; 3],
        ))
        .expect("plan");
        plan.resize_to = Some(grid(6, 3));
        let mut uniforms = rebuild_uniforms();
        let before = uniforms;

        let err = plan
            .apply_rebuild_uniforms(
                &mut uniforms,
                rebuild_uniform_input(WindowPaddingColor::Extend),
            )
            .expect_err("malformed resize should reject");

        assert_eq!(
            err,
            FrameRebuildUniformValidationError::ResizeGridMismatch {
                resize_to: grid(6, 3),
                effective_grid: grid(7, 3),
            }
        );
        assert_eq!(uniforms.grid_size, before.grid_size);
        assert_eq!(uniforms.padding_extend, before.padding_extend);
    }

    #[test]
    fn apply_rebuild_uniforms_rejects_resize_without_full_rebuild_without_mutation() {
        let mut plan = FrameRebuildPlan::build(input(
            grid(4, 5),
            grid(7, 3),
            RenderDirty::Clean,
            &[true; 3],
        ))
        .expect("plan");
        plan.full_rebuild = false;
        let mut uniforms = rebuild_uniforms();
        let before = uniforms;

        let err = plan
            .apply_rebuild_uniforms(
                &mut uniforms,
                rebuild_uniform_input(WindowPaddingColor::Extend),
            )
            .expect_err("resize without full rebuild should reject");

        assert_eq!(
            err,
            FrameRebuildUniformValidationError::ResizeWithoutFullRebuild {
                resize_to: grid(7, 3),
            }
        );
        assert_eq!(uniforms.grid_size, before.grid_size);
        assert_eq!(uniforms.padding_extend, before.padding_extend);
    }

    fn padding_extend_input(
        padding_color: WindowPaddingColor,
        row_never_extend: &[bool],
    ) -> FramePaddingExtendInput<'_> {
        FramePaddingExtendInput {
            padding_color,
            row_never_extend,
        }
    }

    #[test]
    fn refine_padding_extend_rows_top_row_can_clear_up_edge() {
        let plan = FrameRebuildPlan::build(input(
            grid(4, 3),
            grid(4, 3),
            RenderDirty::Partial,
            &[true, false, false],
        ))
        .expect("plan");
        let mut uniforms = rebuild_uniforms();
        uniforms.padding_extend = 15;

        let applied = plan
            .refine_padding_extend_rows(
                &mut uniforms,
                padding_extend_input(WindowPaddingColor::Extend, &[true]),
            )
            .expect("refine padding extend");

        assert_eq!(
            applied,
            FramePaddingExtendApplication {
                refined_rows: vec![0],
                padding_extend_mutated: true,
            }
        );
        assert_eq!(uniforms.padding_extend, 11);
    }

    #[test]
    fn refine_padding_extend_rows_bottom_row_can_clear_down_edge() {
        let plan = FrameRebuildPlan::build(input(
            grid(4, 3),
            grid(4, 3),
            RenderDirty::Partial,
            &[false, false, true],
        ))
        .expect("plan");
        let mut uniforms = rebuild_uniforms();
        uniforms.padding_extend = 15;

        let applied = plan
            .refine_padding_extend_rows(
                &mut uniforms,
                padding_extend_input(WindowPaddingColor::Extend, &[false, false, true]),
            )
            .expect("refine padding extend");

        assert_eq!(
            applied,
            FramePaddingExtendApplication {
                refined_rows: vec![2],
                padding_extend_mutated: true,
            }
        );
        assert_eq!(uniforms.padding_extend, 7);
    }

    #[test]
    fn refine_padding_extend_rows_middle_row_and_clean_plan_are_noops() {
        let middle = FrameRebuildPlan::build(input(
            grid(4, 3),
            grid(4, 3),
            RenderDirty::Partial,
            &[false, true, false],
        ))
        .expect("middle plan");
        let mut uniforms = rebuild_uniforms();
        uniforms.padding_extend = 15;

        let applied = middle
            .refine_padding_extend_rows(
                &mut uniforms,
                padding_extend_input(WindowPaddingColor::Extend, &[]),
            )
            .expect("middle row should not require row inputs");

        assert_eq!(
            applied,
            FramePaddingExtendApplication {
                refined_rows: vec![],
                padding_extend_mutated: false,
            }
        );
        assert_eq!(uniforms.padding_extend, 15);

        let clean = FrameRebuildPlan::build(input(
            grid(4, 3),
            grid(4, 3),
            RenderDirty::Clean,
            &[false, false, false],
        ))
        .expect("clean plan");
        let applied = clean
            .refine_padding_extend_rows(
                &mut uniforms,
                padding_extend_input(WindowPaddingColor::Extend, &[]),
            )
            .expect("clean plan should not require row inputs");

        assert_eq!(
            applied,
            FramePaddingExtendApplication {
                refined_rows: vec![],
                padding_extend_mutated: false,
            }
        );
        assert_eq!(uniforms.padding_extend, 15);
    }

    #[test]
    fn refine_padding_extend_rows_one_row_grid_uses_top_branch_only() {
        let plan = plan_for_grid(grid(4, 1));
        let mut uniforms = rebuild_uniforms();
        uniforms.padding_extend = 15;

        let applied = plan
            .refine_padding_extend_rows(
                &mut uniforms,
                padding_extend_input(WindowPaddingColor::Extend, &[true]),
            )
            .expect("refine padding extend");

        assert_eq!(
            applied,
            FramePaddingExtendApplication {
                refined_rows: vec![0],
                padding_extend_mutated: true,
            }
        );
        assert_eq!(uniforms.padding_extend, 11);
    }

    #[test]
    fn refine_padding_extend_rows_zero_row_plan_is_noop() {
        let plan = plan_for_grid(grid(4, 0));
        let mut uniforms = rebuild_uniforms();
        uniforms.padding_extend = 15;

        let applied = plan
            .refine_padding_extend_rows(
                &mut uniforms,
                padding_extend_input(WindowPaddingColor::Extend, &[]),
            )
            .expect("zero row plan should not require row inputs");

        assert_eq!(
            applied,
            FramePaddingExtendApplication {
                refined_rows: vec![],
                padding_extend_mutated: false,
            }
        );
        assert_eq!(uniforms.padding_extend, 15);
    }

    #[test]
    fn refine_padding_extend_rows_background_and_extend_always_skip_row_inputs() {
        let plan = plan_for_grid(grid(4, 3));
        let mut uniforms = rebuild_uniforms();
        uniforms.padding_extend = 11;

        let background = plan
            .refine_padding_extend_rows(
                &mut uniforms,
                padding_extend_input(WindowPaddingColor::Background, &[]),
            )
            .expect("background should skip refinement");

        assert_eq!(
            background,
            FramePaddingExtendApplication {
                refined_rows: vec![],
                padding_extend_mutated: false,
            }
        );
        assert_eq!(uniforms.padding_extend, 11);

        let extend_always = plan
            .refine_padding_extend_rows(
                &mut uniforms,
                padding_extend_input(WindowPaddingColor::ExtendAlways, &[]),
            )
            .expect("extend always should skip refinement");

        assert_eq!(
            extend_always,
            FramePaddingExtendApplication {
                refined_rows: vec![],
                padding_extend_mutated: false,
            }
        );
        assert_eq!(uniforms.padding_extend, 11);
    }

    #[test]
    fn refine_padding_extend_rows_refined_boundary_can_leave_padding_unchanged() {
        let plan = FrameRebuildPlan::build(input(
            grid(4, 3),
            grid(4, 3),
            RenderDirty::Partial,
            &[true, false, false],
        ))
        .expect("plan");
        let mut uniforms = rebuild_uniforms();
        uniforms.padding_extend = 15;

        let applied = plan
            .refine_padding_extend_rows(
                &mut uniforms,
                padding_extend_input(WindowPaddingColor::Extend, &[false]),
            )
            .expect("refine padding extend");

        assert_eq!(
            applied,
            FramePaddingExtendApplication {
                refined_rows: vec![0],
                padding_extend_mutated: false,
            }
        );
        assert_eq!(uniforms.padding_extend, 15);
    }

    #[test]
    fn refine_padding_extend_rows_rejects_missing_boundary_input_without_mutation() {
        let plan = plan_for_grid(grid(4, 3));
        let mut uniforms = rebuild_uniforms();
        let before = uniforms;

        let err = plan
            .refine_padding_extend_rows(
                &mut uniforms,
                padding_extend_input(WindowPaddingColor::Extend, &[]),
            )
            .expect_err("missing boundary input should reject");

        assert_eq!(
            err,
            FramePaddingExtendValidationError::MissingRowNeverExtend { row: 0, rows: 0 }
        );
        assert_eq!(uniforms.padding_extend, before.padding_extend);
    }

    #[test]
    fn refine_padding_extend_rows_rejects_duplicate_rebuild_row_without_mutation() {
        let mut plan = plan_for_grid(grid(4, 3));
        plan.rows_to_rebuild.push(0);
        let mut uniforms = rebuild_uniforms();
        let before = uniforms;

        let err = plan
            .refine_padding_extend_rows(
                &mut uniforms,
                padding_extend_input(WindowPaddingColor::Extend, &[false, false, false]),
            )
            .expect_err("duplicate row should reject");

        assert_eq!(
            err,
            FramePaddingExtendValidationError::DuplicateRebuildRow { row: 0 }
        );
        assert_eq!(uniforms.padding_extend, before.padding_extend);
    }

    #[test]
    fn refine_padding_extend_rows_rejects_out_of_bounds_row_without_mutation() {
        let mut plan = plan_for_grid(grid(4, 3));
        plan.rows_to_rebuild = vec![3];
        let mut uniforms = rebuild_uniforms();
        let before = uniforms;

        let err = plan
            .refine_padding_extend_rows(
                &mut uniforms,
                padding_extend_input(WindowPaddingColor::Extend, &[false, false, false, true]),
            )
            .expect_err("out of bounds row should reject");

        assert_eq!(
            err,
            FramePaddingExtendValidationError::RebuildRowOutOfBounds { row: 3, rows: 3 }
        );
        assert_eq!(uniforms.padding_extend, before.padding_extend);
    }

    #[test]
    fn refine_padding_extend_rows_rejects_nonempty_zero_row_plan_without_mutation() {
        let mut plan = plan_for_grid(grid(4, 0));
        plan.rows_to_rebuild = vec![0];
        let mut uniforms = rebuild_uniforms();
        let before = uniforms;

        let err = plan
            .refine_padding_extend_rows(
                &mut uniforms,
                padding_extend_input(WindowPaddingColor::Extend, &[true]),
            )
            .expect_err("nonempty zero row plan should reject");

        assert_eq!(
            err,
            FramePaddingExtendValidationError::RebuildRowOutOfBounds { row: 0, rows: 0 }
        );
        assert_eq!(uniforms.padding_extend, before.padding_extend);
    }

    // A full prepared-rebuild fixture: a live 4x3 terminal with one written row
    // and a cursor, plus the caller-supplied inputs for every stage.
    fn prepared_rebuild_terminal() -> Terminal {
        let mut terminal = terminal(4, 3);
        terminal.clear_dirty_for_tests();
        terminal.set_cursor_position_for_tests(0, 1);
        write_terminal(&mut terminal, b"A");
        terminal
    }

    #[test]
    fn rebuild_frame_runs_the_full_prepared_sequence() {
        let terminal = prepared_rebuild_terminal();
        let snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 3), RenderDirty::Full, None);

        let mut contents = contents_with_rows(grid(4, 3));
        let mut shared = menlo_grid();
        let mut row_dirty = snapshot.row_dirty.clone();
        let mut uniforms = rebuild_uniforms();
        uniforms.padding_extend = 15;
        let highlights = Vec::new();
        let links = Vec::new();
        let selection_config = SelectionConfig::default();

        let applied = snapshot
            .rebuild_frame(
                FramePreparedRebuildTargets {
                    contents: &mut contents,
                    grid: &mut shared,
                    row_dirty: &mut row_dirty,
                    uniforms: &mut uniforms,
                },
                FramePreparedRebuildInput {
                    row_format: snapshot_format_input(&highlights, &links, &selection_config),
                    text_overlay: snapshot_text_overlay_input(
                        Some(snapshot_cursor_overlay_input()),
                    ),
                    cursor_uniform: snapshot_cursor_uniform_input(Some(
                        snapshot_block_cursor_uniform_input(),
                    )),
                    rebuild_uniform: rebuild_uniform_input(WindowPaddingColor::Extend),
                    padding_extend: padding_extend_input(
                        WindowPaddingColor::Extend,
                        &[false, false, false],
                    ),
                },
            )
            .expect("rebuild frame");

        // Every stage ran: rows rebuilt (full rebuild → all rows), overlay cursor
        // drawn (no preedit), rebuild uniforms reset padding-extend, padding rows
        // refined (first+last of the rebuild set), block cursor applied.
        assert_eq!(applied.rows.rebuilt_rows, vec![0, 1, 2]);
        assert!(applied.rows.reset_contents);
        assert!(applied.text_overlays.cursor_drawn.is_some());
        assert!(applied.rebuild_uniforms.padding_extend_mutated);
        assert_eq!(applied.padding_extend.refined_rows, vec![0, 2]);
        assert!(applied.cursor_uniforms.block_cursor_applied);
        // row_dirty cleared for the rebuilt rows.
        assert_eq!(row_dirty, vec![false, false, false]);
    }

    #[test]
    fn rebuild_frame_matches_hand_sequenced_drivers() {
        let terminal = prepared_rebuild_terminal();
        let highlights = Vec::new();
        let links = Vec::new();
        let selection_config = SelectionConfig::default();

        // Composed path.
        let snapshot_a =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 3), RenderDirty::Full, None);
        let mut contents_a = contents_with_rows(grid(4, 3));
        let mut shared_a = menlo_grid();
        let mut row_dirty_a = snapshot_a.row_dirty.clone();
        let mut uniforms_a = rebuild_uniforms();
        uniforms_a.padding_extend = 15;
        let applied = snapshot_a
            .rebuild_frame(
                FramePreparedRebuildTargets {
                    contents: &mut contents_a,
                    grid: &mut shared_a,
                    row_dirty: &mut row_dirty_a,
                    uniforms: &mut uniforms_a,
                },
                FramePreparedRebuildInput {
                    row_format: snapshot_format_input(&highlights, &links, &selection_config),
                    text_overlay: snapshot_text_overlay_input(
                        Some(snapshot_cursor_overlay_input()),
                    ),
                    cursor_uniform: snapshot_cursor_uniform_input(Some(
                        snapshot_block_cursor_uniform_input(),
                    )),
                    rebuild_uniform: rebuild_uniform_input(WindowPaddingColor::Extend),
                    padding_extend: padding_extend_input(
                        WindowPaddingColor::Extend,
                        &[false, false, false],
                    ),
                },
            )
            .expect("rebuild frame");

        // Hand-sequenced path: identical fixture, the five drivers called by hand
        // in the same order.
        let snapshot_b =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 3), RenderDirty::Full, None);
        let plan = snapshot_b.build_plan().expect("plan");
        let mut contents_b = contents_with_rows(grid(4, 3));
        let mut shared_b = menlo_grid();
        let mut row_dirty_b = snapshot_b.row_dirty.clone();
        let mut uniforms_b = rebuild_uniforms();
        uniforms_b.padding_extend = 15;

        let rows = plan
            .format_rows(
                &mut contents_b,
                &mut shared_b,
                &mut row_dirty_b,
                snapshot_b.row_format_input(snapshot_format_input(
                    &highlights,
                    &links,
                    &selection_config,
                )),
            )
            .expect("format rows");
        let text_overlays = plan
            .draw_text_overlays(
                &mut contents_b,
                &mut shared_b,
                snapshot_b.text_overlay_input(snapshot_text_overlay_input(Some(
                    snapshot_cursor_overlay_input(),
                ))),
            )
            .expect("draw overlays");
        let rebuild_uniforms_app = plan
            .apply_rebuild_uniforms(
                &mut uniforms_b,
                rebuild_uniform_input(WindowPaddingColor::Extend),
            )
            .expect("rebuild uniforms");
        let padding_extend = plan
            .refine_padding_extend_rows(
                &mut uniforms_b,
                padding_extend_input(WindowPaddingColor::Extend, &[false, false, false]),
            )
            .expect("refine padding");
        let cursor_uniforms = plan
            .apply_cursor_uniforms(
                &mut uniforms_b,
                snapshot_b.cursor_uniform_input(snapshot_cursor_uniform_input(Some(
                    snapshot_block_cursor_uniform_input(),
                ))),
            )
            .expect("cursor uniforms");

        // Same per-stage applications and same observable target mutations.
        assert_eq!(applied.rows, rows);
        assert_eq!(applied.text_overlays, text_overlays);
        assert_eq!(applied.rebuild_uniforms, rebuild_uniforms_app);
        assert_eq!(applied.padding_extend, padding_extend);
        assert_eq!(applied.cursor_uniforms, cursor_uniforms);
        assert_eq!(contents_a.bg_cells(), contents_b.bg_cells());
        assert_eq!(contents_a.fg_rows(), contents_b.fg_rows());
        assert_eq!(uniforms_a.padding_extend, uniforms_b.padding_extend);
        assert_eq!(row_dirty_a, row_dirty_b);
    }

    #[test]
    fn rebuild_frame_fails_fast_on_plan_error_without_mutating_targets() {
        let terminal = prepared_rebuild_terminal();
        let mut snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 3), RenderDirty::Full, None);
        // Truncate row_dirty so build_plan rejects it (DirtyRowsTooShort) before any
        // driver runs.
        snapshot.row_dirty.clear();

        let mut contents = contents_with_rows(grid(4, 3));
        let mut shared = menlo_grid();
        let mut row_dirty = vec![true, true, true];
        let mut uniforms = rebuild_uniforms();
        uniforms.padding_extend = 15;
        let before_bg = contents.bg_cells().to_vec();
        let before_padding = uniforms.padding_extend;
        let highlights = Vec::new();
        let links = Vec::new();
        let selection_config = SelectionConfig::default();

        let err = snapshot
            .rebuild_frame(
                FramePreparedRebuildTargets {
                    contents: &mut contents,
                    grid: &mut shared,
                    row_dirty: &mut row_dirty,
                    uniforms: &mut uniforms,
                },
                FramePreparedRebuildInput {
                    row_format: snapshot_format_input(&highlights, &links, &selection_config),
                    text_overlay: snapshot_text_overlay_input(
                        Some(snapshot_cursor_overlay_input()),
                    ),
                    cursor_uniform: snapshot_cursor_uniform_input(Some(
                        snapshot_block_cursor_uniform_input(),
                    )),
                    rebuild_uniform: rebuild_uniform_input(WindowPaddingColor::Extend),
                    padding_extend: padding_extend_input(
                        WindowPaddingColor::Extend,
                        &[false, false, false],
                    ),
                },
            )
            .expect_err("plan should fail");

        assert!(matches!(err, FramePreparedRebuildError::Plan(_)));
        // No stage ran: targets untouched.
        assert_eq!(contents.bg_cells(), before_bg.as_slice());
        assert_eq!(uniforms.padding_extend, before_padding);
        assert_eq!(row_dirty, vec![true, true, true]);
    }

    #[test]
    fn rebuild_frame_fails_fast_on_format_rows_error_and_skips_later_stages() {
        let terminal = prepared_rebuild_terminal();
        let snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 3), RenderDirty::Full, None);

        let mut contents = contents_with_rows(grid(4, 3));
        let mut shared = menlo_grid();
        // A too-short target row_dirty makes the first driver (format_rows) reject
        // before mutation; the plan still builds (it validates the snapshot's own
        // row_dirty, which is intact).
        let mut row_dirty = Vec::new();
        let mut uniforms = rebuild_uniforms();
        uniforms.padding_extend = 15;
        let before_padding = uniforms.padding_extend;
        let highlights = Vec::new();
        let links = Vec::new();
        let selection_config = SelectionConfig::default();

        let err = snapshot
            .rebuild_frame(
                FramePreparedRebuildTargets {
                    contents: &mut contents,
                    grid: &mut shared,
                    row_dirty: &mut row_dirty,
                    uniforms: &mut uniforms,
                },
                FramePreparedRebuildInput {
                    row_format: snapshot_format_input(&highlights, &links, &selection_config),
                    text_overlay: snapshot_text_overlay_input(
                        Some(snapshot_cursor_overlay_input()),
                    ),
                    cursor_uniform: snapshot_cursor_uniform_input(Some(
                        snapshot_block_cursor_uniform_input(),
                    )),
                    rebuild_uniform: rebuild_uniform_input(WindowPaddingColor::Extend),
                    padding_extend: padding_extend_input(
                        WindowPaddingColor::Extend,
                        &[false, false, false],
                    ),
                },
            )
            .expect_err("format_rows should fail");

        assert!(matches!(err, FramePreparedRebuildError::FormatRows(_)));
        // The later uniform stages did not run.
        assert_eq!(uniforms.padding_extend, before_padding);
    }

    #[test]
    fn rebuild_frame_fails_fast_on_padding_extend_after_earlier_stages_ran() {
        let terminal = prepared_rebuild_terminal();
        let snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 3), RenderDirty::Full, None);

        let mut contents = contents_with_rows(grid(4, 3));
        let mut shared = menlo_grid();
        let mut row_dirty = snapshot.row_dirty.clone();
        let mut uniforms = rebuild_uniforms();
        uniforms.padding_extend = 15;
        let highlights = Vec::new();
        let links = Vec::new();
        let selection_config = SelectionConfig::default();

        let err = snapshot
            .rebuild_frame(
                FramePreparedRebuildTargets {
                    contents: &mut contents,
                    grid: &mut shared,
                    row_dirty: &mut row_dirty,
                    uniforms: &mut uniforms,
                },
                FramePreparedRebuildInput {
                    row_format: snapshot_format_input(&highlights, &links, &selection_config),
                    text_overlay: snapshot_text_overlay_input(
                        Some(snapshot_cursor_overlay_input()),
                    ),
                    cursor_uniform: snapshot_cursor_uniform_input(Some(
                        snapshot_block_cursor_uniform_input(),
                    )),
                    rebuild_uniform: rebuild_uniform_input(WindowPaddingColor::Extend),
                    // Too short: the full rebuild touches row 2, so refine validates
                    // a `row_never_extend` index the 1-element slice does not cover.
                    padding_extend: padding_extend_input(WindowPaddingColor::Extend, &[false]),
                },
            )
            .expect_err("padding extend should fail");

        assert!(matches!(err, FramePreparedRebuildError::PaddingExtend(_)));
        // An earlier stage genuinely ran before refine failed: format_rows cleared
        // the dirty rows — a real mid-sequence failure, not a pre-mutation reject.
        assert_eq!(row_dirty, vec![false, false, false]);
    }

    #[test]
    fn rebuild_and_present_frame_rebuilds_then_presents() {
        let Some(device) = metal_device() else {
            return;
        };
        let terminal = prepared_rebuild_terminal();
        let snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 3), RenderDirty::Full, None);
        let grayscale = Atlas::new(8, Format::Grayscale);
        let color = Atlas::new(8, Format::Bgra);
        let mut compositor = metal_compositor(device, 8, 6, &grayscale, &color);
        let mut contents = contents_with_rows(grid(4, 3));
        let mut shared = menlo_grid();
        let mut row_dirty = snapshot.row_dirty.clone();
        // Uniforms grid must match the 4x3 effective grid for presentation validation.
        let mut uniforms = metal_uniforms([8, 6], [4, 3]);
        let highlights = Vec::new();
        let links = Vec::new();
        let selection_config = SelectionConfig::default();

        let applied = snapshot
            .rebuild_and_present_frame(
                FramePreparedRebuildTargets {
                    contents: &mut contents,
                    grid: &mut shared,
                    row_dirty: &mut row_dirty,
                    uniforms: &mut uniforms,
                },
                FramePreparedRebuildInput {
                    row_format: snapshot_format_input(&highlights, &links, &selection_config),
                    text_overlay: snapshot_text_overlay_input(
                        Some(snapshot_cursor_overlay_input()),
                    ),
                    cursor_uniform: snapshot_cursor_uniform_input(Some(
                        snapshot_block_cursor_uniform_input(),
                    )),
                    rebuild_uniform: rebuild_uniform_input(WindowPaddingColor::Extend),
                    padding_extend: padding_extend_input(
                        WindowPaddingColor::Extend,
                        &[false, false, false],
                    ),
                },
                FramePreparedPresentationInput {
                    compositor: &mut compositor,
                    width: 8,
                    height: 6,
                    contents_scale: 1.0,
                },
            )
            .expect("rebuild and present");

        // The rebuild ran (full rebuild → all rows) and the frame presented at the
        // requested drawable size.
        assert_eq!(applied.rebuild.rows.rebuilt_rows, vec![0, 1, 2]);
        assert!(applied.rebuild.cursor_uniforms.block_cursor_applied);
        assert_eq!(applied.present.presentation.width, 8);
        assert_eq!(applied.present.presentation.height, 6);
    }

    #[test]
    fn rebuild_and_present_frame_fails_fast_before_presentation() {
        let Some(device) = metal_device() else {
            return;
        };
        let terminal = prepared_rebuild_terminal();
        let mut snapshot =
            FrameTerminalSnapshot::collect(&terminal, grid(4, 3), RenderDirty::Full, None);
        // build_plan rejects the truncated row_dirty before any stage — including
        // presentation — runs.
        snapshot.row_dirty.clear();

        let grayscale = Atlas::new(8, Format::Grayscale);
        let color = Atlas::new(8, Format::Bgra);
        let mut compositor = metal_compositor(device, 8, 6, &grayscale, &color);
        let mut contents = contents_with_rows(grid(4, 3));
        let mut shared = menlo_grid();
        let mut row_dirty = vec![true, true, true];
        let mut uniforms = metal_uniforms([8, 6], [4, 3]);
        let highlights = Vec::new();
        let links = Vec::new();
        let selection_config = SelectionConfig::default();

        let err = snapshot
            .rebuild_and_present_frame(
                FramePreparedRebuildTargets {
                    contents: &mut contents,
                    grid: &mut shared,
                    row_dirty: &mut row_dirty,
                    uniforms: &mut uniforms,
                },
                FramePreparedRebuildInput {
                    row_format: snapshot_format_input(&highlights, &links, &selection_config),
                    text_overlay: snapshot_text_overlay_input(
                        Some(snapshot_cursor_overlay_input()),
                    ),
                    cursor_uniform: snapshot_cursor_uniform_input(Some(
                        snapshot_block_cursor_uniform_input(),
                    )),
                    rebuild_uniform: rebuild_uniform_input(WindowPaddingColor::Extend),
                    padding_extend: padding_extend_input(
                        WindowPaddingColor::Extend,
                        &[false, false, false],
                    ),
                },
                FramePreparedPresentationInput {
                    compositor: &mut compositor,
                    width: 8,
                    height: 6,
                    contents_scale: 1.0,
                },
            )
            .expect_err("should fail before presentation");

        // A rebuild-side error is reported as `Rebuild(..)`, never `Present(..)` —
        // proving presentation was not reached.
        assert!(matches!(
            err,
            FramePreparedFrameError::Rebuild(FramePreparedRebuildError::Plan(_))
        ));
    }
}
