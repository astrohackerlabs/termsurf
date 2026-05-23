use crate::termwindow::render::TripleLayerQuadAllocator;
use crate::termwindow::{UIItem, UIItemType};
use mux::pane::Pane;
use mux::tab::{PositionedPane, PositionedSplit, SplitDirection};
use std::sync::Arc;

impl crate::TermWindow {
    pub fn paint_split(
        &mut self,
        _layers: &mut TripleLayerQuadAllocator,
        split: &PositionedSplit,
        _pane: &Arc<dyn Pane>,
        _active_pos: Option<&PositionedPane>,
    ) -> anyhow::Result<()> {
        let cell_width = self.render_metrics.cell_size.width as f32;
        let cell_height = self.render_metrics.cell_size.height as f32;

        let border = self.get_os_border();
        let first_row_offset = if self.show_tab_bar && !self.config.tab_bar_at_bottom {
            self.tab_bar_pixel_height()?
        } else {
            0.
        } + border.top.get() as f32;

        let (padding_left, padding_top) = self.padding_left_top();

        if split.direction == SplitDirection::Horizontal {
            self.ui_items.push(UIItem {
                x: border.left.get() as usize
                    + padding_left as usize
                    + (split.hit_left * cell_width as usize),
                width: cell_width as usize,
                y: padding_top as usize
                    + first_row_offset as usize
                    + split.hit_top * cell_height as usize,
                height: split.size * cell_height as usize,
                item_type: UIItemType::Split(split.clone()),
            });
        } else {
            self.ui_items.push(UIItem {
                x: border.left.get() as usize
                    + padding_left as usize
                    + (split.hit_left * cell_width as usize),
                width: split.size * cell_width as usize,
                y: padding_top as usize
                    + first_row_offset as usize
                    + split.hit_top * cell_height as usize,
                height: cell_height as usize,
                item_type: UIItemType::Split(split.clone()),
            });
        }

        Ok(())
    }
}
