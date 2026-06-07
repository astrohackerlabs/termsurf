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

use crate::renderer::size::{GridSize, Unit};
use crate::renderer::state::{Preedit, PreeditRange};
use crate::terminal::point::Coordinate;

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
}

fn plan_preedit_range(
    input: FrameRebuildInput<'_>,
    row_count: Unit,
    rows_to_rebuild: &[Unit],
) -> Option<FramePreeditRange> {
    let preedit = input.preedit?;
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
    use crate::renderer::state::Codepoint;

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
}
