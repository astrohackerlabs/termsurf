use std::fmt::Write as _;
use std::ops::RangeInclusive;
use std::ptr::NonNull;

use super::page::{
    page_layout, Capacity, CapacityAdjustment, Cell, CloneFromError, ContentTag, Page,
    PageAllocError, Row, SemanticContent, SemanticPrompt, Wide, STD_CAPACITY,
};
use super::point::{self, Coordinate};
use super::size::{
    CellCountInt, GraphemeBytesInt, HyperlinkCountInt, StringBytesInt, StyleCountInt, MAX_PAGE_SIZE,
};
use super::{color, highlight, selection, selection_codepoints, style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Viewport {
    Active,
    Top,
    Pin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Scrollbar {
    total: usize,
    offset: usize,
    len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scroll {
    Active,
    Top,
    Row(usize),
    DeltaRow(isize),
    Pin(Pin),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    RightDown,
    LeftUp,
}

#[derive(Debug)]
pub(super) struct PageList {
    cols: CellCountInt,
    rows: CellCountInt,
    pages: Vec<Box<Node>>,
    page_serial: u64,
    page_serial_min: u64,
    page_size: usize,
    explicit_max_size: usize,
    min_max_size: usize,
    total_rows: CellCountInt,
    tracked_pins: Vec<NonNull<Pin>>,
    tracked_pin_storage: Vec<Box<Pin>>,
    viewport: Viewport,
    viewport_pin: Box<Pin>,
    viewport_pin_row_offset: Option<usize>,
}

#[derive(Debug)]
pub(super) struct Node {
    page: Page,
    serial: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Pin {
    node: NonNull<Node>,
    y: CellCountInt,
    x: CellCountInt,
    garbage: bool,
}

impl Pin {
    pub(super) fn new(node: NonNull<Node>, y: CellCountInt, x: CellCountInt) -> Self {
        Self {
            node,
            y,
            x,
            garbage: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PageListCell<'a> {
    node: &'a Node,
    node_ptr: NonNull<Node>,
    row: &'a Row,
    cell: &'a Cell,
    row_idx: CellCountInt,
    col_idx: CellCountInt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PageChunk {
    node: NonNull<Node>,
    start: CellCountInt,
    end: CellCountInt,
}

impl PageChunk {
    fn full_page(&self, list: &PageList) -> bool {
        let Some(node) = list.node_for_ptr(self.node) else {
            return false;
        };

        self.start == 0 && self.end == node.page.size_rows()
    }

    fn overlaps(&self, other: &Self) -> bool {
        if self.node != other.node {
            return false;
        }
        if self.end <= other.start {
            return false;
        }
        if self.start >= other.end {
            return false;
        }
        true
    }
}

#[derive(Debug)]
struct PageIterator<'a> {
    list: &'a PageList,
    row: Option<Pin>,
    limit: Option<Pin>,
    direction: Direction,
}

#[derive(Debug)]
struct RowIterator<'a> {
    page_it: PageIterator<'a>,
    chunk: Option<PageChunk>,
    offset: CellCountInt,
}

#[derive(Debug)]
struct CellIterator<'a> {
    row_it: RowIterator<'a>,
    cell: Option<Pin>,
}

#[derive(Debug)]
struct PromptIterator<'a> {
    list: &'a PageList,
    current: Option<Pin>,
    limit: Option<Pin>,
    direction: Direction,
}

#[derive(Debug)]
struct LineIterator<'a> {
    list: &'a PageList,
    current: Option<Pin>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectionStringOptions {
    selection: Option<selection::Selection>,
    trim: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PlainStringOptions {
    selection: Option<selection::Selection>,
    trim: bool,
    unwrap: bool,
}

#[derive(Debug, Clone, Copy)]
struct PlainStringWithMapOptions<'a> {
    selection: Option<selection::Selection>,
    trim: bool,
    unwrap: bool,
    codepoint_map: Option<&'a [CodepointMapEntry]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PageStringWithMap {
    text: String,
    point_map: Vec<point::Coordinate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PageStringWithPinMap {
    text: String,
    pin_map: Vec<Pin>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CodepointReplacement {
    Codepoint(char),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodepointMapEntry {
    range: RangeInclusive<u32>,
    replacement: CodepointReplacement,
}

impl CodepointMapEntry {
    fn new(start: u32, end: u32, replacement: CodepointReplacement) -> Option<Self> {
        if !valid_scalar_range(start, end) {
            return None;
        }

        Some(Self {
            range: start..=end,
            replacement,
        })
    }

    fn matches(&self, codepoint: u32) -> bool {
        self.range.contains(&codepoint)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageOutputFormat {
    Plain,
    Vt,
    Html,
}

#[derive(Debug, Clone, Copy)]
struct PageStringOptions<'a> {
    selection: Option<selection::Selection>,
    trim: bool,
    unwrap: bool,
    emit: PageOutputFormat,
    palette: Option<&'a color::Palette>,
    codepoint_map: Option<&'a [CodepointMapEntry]>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PlainTrailingState {
    rows: usize,
    cells: usize,
}

#[derive(Debug, Clone, Copy)]
struct PlainPageFormat<'a> {
    node: &'a Node,
    screen_y_base: u32,
    start_x: CellCountInt,
    start_y: CellCountInt,
    end_x: CellCountInt,
    end_y: CellCountInt,
    rectangle: bool,
    trim: bool,
    unwrap: bool,
    trailing_state: Option<PlainTrailingState>,
    codepoint_map: Option<&'a [CodepointMapEntry]>,
}

struct StyledPageFormat<'a> {
    node: &'a Node,
    screen_y_base: u32,
    start_x: CellCountInt,
    start_y: CellCountInt,
    end_x: CellCountInt,
    end_y: CellCountInt,
    rectangle: bool,
    trim: bool,
    unwrap: bool,
    emit: PageOutputFormat,
    palette: Option<&'a color::Palette>,
    trailing_state: Option<PlainTrailingState>,
    codepoint_map: Option<&'a [CodepointMapEntry]>,
    point_map: Option<&'a mut Vec<point::Coordinate>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PromptClickMove {
    left: usize,
    right: usize,
}

impl PromptClickMove {
    const ZERO: Self = Self { left: 0, right: 0 };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptClickMode {
    None,
    ClickEvents,
    Line,
    Multiple,
    ConservativeVertical,
    SmartVertical,
}

#[derive(Debug, Default)]
struct TrackedPinsRemap {
    entries: Vec<(NonNull<Pin>, NonNull<Pin>)>,
}

impl TrackedPinsRemap {
    fn insert(&mut self, old: NonNull<Pin>, new: NonNull<Pin>) {
        self.entries.push((old, new));
    }

    fn get(&self, old: NonNull<Pin>) -> Option<NonNull<Pin>> {
        self.entries
            .iter()
            .find_map(|(candidate, new)| (*candidate == old).then_some(*new))
    }
}

#[derive(Debug)]
struct CloneOptions<'a> {
    top: point::Point,
    bottom: Option<point::Point>,
    tracked_pins: Option<&'a mut TrackedPinsRemap>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DragGeometry {
    columns: u32,
    cell_width: u32,
    padding_left: u32,
    screen_height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectLineOptions<'a> {
    pin: Pin,
    whitespace: Option<&'a [u32]>,
    semantic_prompt_boundary: bool,
}

impl<'a> SelectLineOptions<'a> {
    fn new(pin: Pin) -> Self {
        Self {
            pin,
            whitespace: Some(selection_codepoints::DEFAULT_LINE_WHITESPACE),
            semantic_prompt_boundary: true,
        }
    }
}

const SELECT_ALL_WHITESPACE: [u32; 3] = [0, ' ' as u32, '\t' as u32];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloneRegionError {
    Empty,
    PageAlloc,
    CloneFrom(CloneFromError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IncreaseCapacity {
    Styles,
    GraphemeBytes,
    HyperlinkBytes,
    StringBytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IncreaseCapacityError {
    PageAlloc,
    OutOfSpace,
    CloneFrom(CloneFromError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplitError {
    OutOfMemory,
    OutOfSpace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EraseRowError {
    InvalidPoint,
    CloneFrom(CloneFromError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErasePageError {
    InvalidPage,
    MiddlePage,
    OnlyPage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EraseRowsError {
    InvalidPoint,
    MiddlePage,
    Grow(GrowError),
    ErasePage(ErasePageError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EraseRowsMode {
    History,
    Active,
}

impl From<PageAllocError> for CloneRegionError {
    fn from(_: PageAllocError) -> Self {
        Self::PageAlloc
    }
}

impl From<CloneFromError> for CloneRegionError {
    fn from(err: CloneFromError) -> Self {
        Self::CloneFrom(err)
    }
}

impl From<GrowError> for CloneRegionError {
    fn from(err: GrowError) -> Self {
        match err {
            GrowError::PageAlloc => Self::PageAlloc,
        }
    }
}

impl From<PageAllocError> for IncreaseCapacityError {
    fn from(_: PageAllocError) -> Self {
        Self::PageAlloc
    }
}

impl From<PageAllocError> for SplitError {
    fn from(_: PageAllocError) -> Self {
        Self::OutOfMemory
    }
}

impl From<CloneFromError> for EraseRowError {
    fn from(err: CloneFromError) -> Self {
        Self::CloneFrom(err)
    }
}

impl From<GrowError> for EraseRowsError {
    fn from(err: GrowError) -> Self {
        Self::Grow(err)
    }
}

impl From<ErasePageError> for EraseRowsError {
    fn from(err: ErasePageError) -> Self {
        Self::ErasePage(err)
    }
}

impl From<CloneFromError> for IncreaseCapacityError {
    fn from(err: CloneFromError) -> Self {
        Self::CloneFrom(err)
    }
}

impl Pin {
    fn is_dirty(self, list: &PageList) -> bool {
        list.pin_is_dirty(self)
    }

    fn mark_dirty(self, list: &mut PageList) {
        if let Some(index) = list.node_index(self.node) {
            list.pages[index]
                .page
                .get_row_mut(self.y as usize)
                .set_dirty(true);
        }
    }
}

impl PageListCell<'_> {
    fn is_dirty(self) -> bool {
        self.node.page.is_dirty() || self.row.dirty()
    }

    fn style(self) -> super::style::Style {
        let style_id = self.cell.style_id();
        if style_id == super::style::DEFAULT_ID {
            super::style::Style::default()
        } else {
            self.node.page.get_style(style_id)
        }
    }

    fn screen_point(self, list: &PageList) -> Option<point::Point> {
        let mut y = self.row_idx as u32;
        for node in &list.pages {
            let ptr = NonNull::from(node.as_ref());
            if ptr == self.node_ptr {
                return Some(point::Point::screen(Coordinate::new(self.col_idx, y)));
            }
            y += node.page.size_rows() as u32;
        }

        None
    }
}

impl Iterator for PageIterator<'_> {
    type Item = PageChunk;

    fn next(&mut self) -> Option<Self::Item> {
        match self.direction {
            Direction::RightDown => self.next_down(),
            Direction::LeftUp => self.next_up(),
        }
    }
}

impl PageIterator<'_> {
    fn next_down(&mut self) -> Option<PageChunk> {
        let row = self.row?;
        let row_index = self.list.node_index(row.node)?;

        match self.limit {
            None => {
                let node = &self.list.pages[row_index];
                self.row = self.list.pages.get(row_index + 1).map(|next| Pin {
                    node: NonNull::from(next.as_ref()),
                    y: 0,
                    x: 0,
                    garbage: false,
                });

                Some(PageChunk {
                    node: row.node,
                    start: row.y,
                    end: node.page.size_rows(),
                })
            }
            Some(limit) if limit.node != row.node => {
                let node = &self.list.pages[row_index];
                self.row = self.list.pages.get(row_index + 1).map(|next| Pin {
                    node: NonNull::from(next.as_ref()),
                    y: 0,
                    x: 0,
                    garbage: false,
                });

                Some(PageChunk {
                    node: row.node,
                    start: row.y,
                    end: node.page.size_rows(),
                })
            }
            Some(limit) => {
                self.row = None;
                if row.y > limit.y {
                    return None;
                }

                Some(PageChunk {
                    node: row.node,
                    start: row.y,
                    end: limit.y + 1,
                })
            }
        }
    }

    fn next_up(&mut self) -> Option<PageChunk> {
        let row = self.row?;
        let row_index = self.list.node_index(row.node)?;

        match self.limit {
            None => {
                self.row = row_index.checked_sub(1).map(|prev_index| {
                    let prev = &self.list.pages[prev_index];
                    Pin {
                        node: NonNull::from(prev.as_ref()),
                        y: prev.page.size_rows() - 1,
                        x: 0,
                        garbage: false,
                    }
                });

                Some(PageChunk {
                    node: row.node,
                    start: 0,
                    end: row.y + 1,
                })
            }
            Some(limit) if limit.node != row.node => {
                self.row = row_index.checked_sub(1).map(|prev_index| {
                    let prev = &self.list.pages[prev_index];
                    Pin {
                        node: NonNull::from(prev.as_ref()),
                        y: prev.page.size_rows() - 1,
                        x: 0,
                        garbage: false,
                    }
                });

                Some(PageChunk {
                    node: row.node,
                    start: 0,
                    end: row.y + 1,
                })
            }
            Some(limit) => {
                self.row = None;
                if row.y < limit.y {
                    return None;
                }

                Some(PageChunk {
                    node: row.node,
                    start: limit.y,
                    end: row.y + 1,
                })
            }
        }
    }
}

impl Iterator for RowIterator<'_> {
    type Item = Pin;

    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self.chunk?;
        let row = Pin {
            node: chunk.node,
            y: self.offset,
            x: 0,
            garbage: false,
        };

        match self.page_it.direction {
            Direction::RightDown => {
                self.offset += 1;
                if self.offset >= chunk.end {
                    self.chunk = self.page_it.next();
                    if let Some(next_chunk) = self.chunk {
                        self.offset = next_chunk.start;
                    }
                }
            }
            Direction::LeftUp => {
                if self.offset == 0 {
                    self.chunk = self.page_it.next();
                    if let Some(next_chunk) = self.chunk {
                        self.offset = next_chunk.end - 1;
                    }
                } else if self.offset == chunk.start {
                    self.chunk = None;
                } else {
                    self.offset -= 1;
                }
            }
        }

        Some(row)
    }
}

impl Iterator for CellIterator<'_> {
    type Item = Pin;

    fn next(&mut self) -> Option<Self::Item> {
        let cell = self.cell?;
        let cols = self
            .row_it
            .page_it
            .list
            .node_for_pin(&cell)?
            .page
            .size_cols();

        match self.row_it.page_it.direction {
            Direction::RightDown => {
                if cell.x + 1 < cols {
                    self.cell = Some(Pin {
                        x: cell.x + 1,
                        ..cell
                    });
                } else {
                    self.cell = self.row_it.next();
                }
            }
            Direction::LeftUp => {
                if cell.x > 0 {
                    self.cell = Some(Pin {
                        x: cell.x - 1,
                        ..cell
                    });
                } else if let Some(mut next_cell) = self.row_it.next() {
                    let cols = self
                        .row_it
                        .page_it
                        .list
                        .node_for_pin(&next_cell)?
                        .page
                        .size_cols();
                    next_cell.x = cols - 1;
                    self.cell = Some(next_cell);
                } else {
                    self.cell = None;
                }
            }
        }

        Some(cell)
    }
}

impl Iterator for PromptIterator<'_> {
    type Item = Pin;

    fn next(&mut self) -> Option<Self::Item> {
        match self.direction {
            Direction::RightDown => self.next_down(),
            Direction::LeftUp => self.next_up(),
        }
    }
}

impl Iterator for LineIterator<'_> {
    type Item = selection::Selection;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current?;
        let result = self.list.select_line(SelectLineOptions {
            pin: current,
            whitespace: None,
            semantic_prompt_boundary: false,
        })?;

        self.current = self.list.pin_down(result.end(), 1);
        Some(result)
    }
}

impl PlainPageFormat<'_> {
    fn format(
        &self,
        output: &mut String,
        mut point_map: Option<&mut Vec<point::Coordinate>>,
    ) -> PlainTrailingState {
        let page = &self.node.page;
        let mut blank_rows = 0usize;
        let mut blank_cells = 0usize;

        if let Some(state) = self.trailing_state {
            if self.start_y == 0 && self.start_x == 0 {
                blank_rows = state.rows;
                blank_cells = state.cells;
            }
        }

        if self.start_x >= page.size_cols() || self.start_y >= page.size_rows() {
            return PlainTrailingState {
                rows: blank_rows,
                cells: blank_cells,
            };
        }

        let mut end_x = self.end_x.min(page.size_cols() - 1);
        let mut end_y = self.end_y.min(page.size_rows() - 1);
        if self.start_y > end_y {
            return PlainTrailingState {
                rows: blank_rows,
                cells: blank_cells,
            };
        }

        if self.unwrap && !self.rectangle {
            let final_row = page.get_row(end_y as usize);
            let cells = page.get_cells(final_row);
            if cells[end_x as usize].wide() == Wide::SpacerHead && end_y < page.size_rows() - 1 {
                end_y += 1;
                end_x = 0;
            }
        }

        if self.start_y == end_y && self.start_x > end_x {
            return PlainTrailingState {
                rows: blank_rows,
                cells: blank_cells,
            };
        }

        for y_usize in self.start_y as usize..=end_y as usize {
            let y: CellCountInt = y_usize
                .try_into()
                .expect("page row index must fit CellCountInt");
            let row = page.get_row(y_usize);
            let cells = page.get_cells(row);

            let row_end_x = if self.rectangle || y == end_y {
                end_x.saturating_add(1).min(page.size_cols())
            } else {
                page.size_cols()
            };

            let row_start_x = if self.start_x > 0 && (self.rectangle || y == self.start_y) {
                match cells[self.start_x as usize].wide() {
                    Wide::SpacerTail => self.start_x - 1,
                    Wide::SpacerHead => continue,
                    Wide::Narrow | Wide::Wide => self.start_x,
                }
            } else {
                0
            };

            if row_start_x >= row_end_x {
                blank_rows += 1;
                continue;
            }

            let subset = &cells[row_start_x as usize..row_end_x as usize];
            if !Cell::has_text_any(subset) {
                blank_rows += 1;
                continue;
            }

            if blank_rows > 0 {
                push_pending_plain_newlines(
                    output,
                    point_map.as_mut().map(|map| &mut **map),
                    blank_rows,
                    self.screen_y_base + y_usize as u32,
                );
                blank_rows = 0;
            }

            if !self.unwrap || !row.wrap() {
                blank_rows += 1;
            }

            if !self.unwrap || !row.wrap_continuation() {
                blank_cells = 0;
            }

            for (cell, x_usize) in subset.iter().zip(row_start_x as usize..) {
                match cell.wide() {
                    Wide::Narrow | Wide::Wide => {}
                    Wide::SpacerHead | Wide::SpacerTail => continue,
                }

                if !cell.has_text() {
                    blank_cells += 1;
                    continue;
                }

                if cell.codepoint() == ' ' as u32 && self.trim {
                    blank_cells += 1;
                    continue;
                }

                if blank_cells > 0 {
                    push_blank_cells_plain(
                        output,
                        point_map.as_mut().map(|map| &mut **map),
                        blank_cells,
                        x_usize,
                        y_usize,
                        page.size_cols() as usize,
                        self.screen_y_base,
                    );
                    blank_cells = 0;
                }

                push_cell_plain(
                    page,
                    x_usize,
                    y_usize,
                    cell,
                    self.codepoint_map,
                    output,
                    point_map.as_mut().map(|map| &mut **map),
                    self.screen_y_base,
                );
            }
        }

        PlainTrailingState {
            rows: blank_rows,
            cells: blank_cells,
        }
    }
}

impl StyledPageFormat<'_> {
    fn format(&mut self, output: &mut String) -> PlainTrailingState {
        let page = &self.node.page;
        let mut blank_rows = 0usize;
        let mut blank_cells = 0usize;
        let mut current_style = style::Style::default();

        if self.emit == PageOutputFormat::Html {
            output.push_str("<div style=\"font-family: monospace; white-space: pre;\">");
        }

        if let Some(state) = self.trailing_state {
            if self.start_y == 0 && self.start_x == 0 {
                blank_rows = state.rows;
                blank_cells = state.cells;
            }
        }

        if self.start_x >= page.size_cols() || self.start_y >= page.size_rows() {
            self.close_format(output, current_style);
            return PlainTrailingState {
                rows: blank_rows,
                cells: blank_cells,
            };
        }

        let mut end_x = self.end_x.min(page.size_cols() - 1);
        let mut end_y = self.end_y.min(page.size_rows() - 1);
        if self.start_y > end_y {
            self.close_format(output, current_style);
            return PlainTrailingState {
                rows: blank_rows,
                cells: blank_cells,
            };
        }

        if self.unwrap && !self.rectangle {
            let final_row = page.get_row(end_y as usize);
            let cells = page.get_cells(final_row);
            if cells[end_x as usize].wide() == Wide::SpacerHead && end_y < page.size_rows() - 1 {
                end_y += 1;
                end_x = 0;
            }
        }

        if self.start_y == end_y && self.start_x > end_x {
            self.close_format(output, current_style);
            return PlainTrailingState {
                rows: blank_rows,
                cells: blank_cells,
            };
        }

        for y_usize in self.start_y as usize..=end_y as usize {
            let y: CellCountInt = y_usize
                .try_into()
                .expect("page row index must fit CellCountInt");
            let row = page.get_row(y_usize);
            let cells = page.get_cells(row);

            let row_end_x = if self.rectangle || y == end_y {
                end_x.saturating_add(1).min(page.size_cols())
            } else {
                page.size_cols()
            };

            let row_start_x = if self.start_x > 0 && (self.rectangle || y == self.start_y) {
                match cells[self.start_x as usize].wide() {
                    Wide::SpacerTail => self.start_x - 1,
                    Wide::SpacerHead => continue,
                    Wide::Narrow | Wide::Wide => self.start_x,
                }
            } else {
                0
            };

            if row_start_x >= row_end_x {
                blank_rows += 1;
                continue;
            }

            let subset = &cells[row_start_x as usize..row_end_x as usize];
            if !Cell::has_text_any(subset) {
                blank_rows += 1;
                continue;
            }

            if blank_rows > 0 {
                if !current_style.is_default() {
                    self.push_style_close(output);
                    current_style = style::Style::default();
                }
                self.push_pending_newlines(output, blank_rows);
                blank_rows = 0;
            }

            if !self.unwrap || !row.wrap() {
                blank_rows += 1;
            }

            if !self.unwrap || !row.wrap_continuation() {
                blank_cells = 0;
            }

            for (cell, x_usize) in subset.iter().zip(row_start_x as usize..) {
                match cell.wide() {
                    Wide::Narrow | Wide::Wide => {}
                    Wide::SpacerHead | Wide::SpacerTail => continue,
                }

                if cell.is_empty() && !cell.has_styling() {
                    blank_cells += 1;
                    continue;
                }

                if cell.content_tag() == ContentTag::Codepoint
                    && cell.codepoint() == ' ' as u32
                    && self.trim
                {
                    blank_cells += 1;
                    continue;
                }

                if blank_cells > 0 {
                    push_blank_cells_plain(
                        output,
                        self.point_map.as_deref_mut(),
                        blank_cells,
                        x_usize,
                        y_usize,
                        page.size_cols() as usize,
                        self.screen_y_base,
                    );
                    blank_cells = 0;
                }

                let source = self.source_coord(x_usize, y_usize);
                let cell_style = self.cell_style(page, cell);
                if cell_style != current_style {
                    if !current_style.is_default() {
                        match self.emit {
                            PageOutputFormat::Vt => {
                                if cell_style.is_default() {
                                    self.push_style_close(output);
                                }
                            }
                            PageOutputFormat::Html => self.push_style_close(output),
                            PageOutputFormat::Plain => unreachable!(),
                        }
                    }

                    current_style = cell_style;
                    if !current_style.is_default() {
                        self.push_style_open(output, current_style, source);
                    }
                }

                self.push_cell(page, x_usize, y_usize, cell, output, source);
            }
        }

        self.close_format(output, current_style);
        PlainTrailingState {
            rows: blank_rows,
            cells: blank_cells,
        }
    }

    fn close_format(&mut self, output: &mut String, current_style: style::Style) {
        if !current_style.is_default() {
            self.push_style_close(output);
        }
        if self.emit == PageOutputFormat::Html {
            output.push_str("</div>");
        }
    }

    fn push_newline_text(&self, output: &mut String) {
        match self.emit {
            PageOutputFormat::Vt => output.push_str("\r\n"),
            PageOutputFormat::Html => output.push('\n'),
            PageOutputFormat::Plain => unreachable!(),
        }
    }

    fn push_pending_newlines(&mut self, output: &mut String, count: usize) {
        if self.emit != PageOutputFormat::Vt {
            for _ in 0..count {
                self.push_newline_text(output);
            }
            return;
        }

        let source = self
            .point_map
            .as_ref()
            .and_then(|map| map.last().copied())
            .unwrap_or_else(|| point::Coordinate::new(0, self.screen_y_base));
        for row_offset in 0..count {
            let coord = if row_offset == 0 {
                source
            } else {
                point::Coordinate::new(0, source.y + row_offset as u32)
            };
            self.push_newline_text(output);
            self.push_map_entries(coord, 2);
        }
    }

    fn push_style_open(
        &mut self,
        output: &mut String,
        value: style::Style,
        source: point::Coordinate,
    ) {
        let start_len = output.len();
        match self.emit {
            PageOutputFormat::Vt => {
                let formatter = if let Some(palette) = self.palette {
                    value.formatter_vt().with_palette(palette).to_string()
                } else {
                    value.formatter_vt().to_string()
                };
                output.push_str(&formatter);
            }
            PageOutputFormat::Html => {
                let formatter = if let Some(palette) = self.palette {
                    value.formatter_html().with_palette(palette).to_string()
                } else {
                    value.formatter_html().to_string()
                };
                output.push_str("<div style=\"display: inline;");
                output.push_str(&formatter);
                output.push_str("\">");
            }
            PageOutputFormat::Plain => unreachable!(),
        }
        if self.emit == PageOutputFormat::Vt {
            self.push_map_entries(source, output.len() - start_len);
        }
    }

    fn push_style_close(&mut self, output: &mut String) {
        let start_len = output.len();
        match self.emit {
            PageOutputFormat::Vt => output.push_str("\x1b[0m"),
            PageOutputFormat::Html => output.push_str("</div>"),
            PageOutputFormat::Plain => unreachable!(),
        }
        if self.emit == PageOutputFormat::Vt {
            self.push_previous_map_entries(output.len() - start_len);
        }
    }

    fn cell_style(&self, page: &Page, cell: &Cell) -> style::Style {
        match cell.content_tag() {
            ContentTag::Codepoint | ContentTag::CodepointGrapheme => {
                if !cell.has_styling() {
                    style::Style::default()
                } else {
                    page.get_style(cell.style_id())
                }
            }
            ContentTag::BgColorPalette => style::Style {
                bg_color: style::Color::Palette(cell.color_palette()),
                ..style::Style::default()
            },
            ContentTag::BgColorRgb => style::Style {
                bg_color: style::Color::Rgb(cell.color_rgb()),
                ..style::Style::default()
            },
        }
    }

    fn push_cell(
        &mut self,
        page: &Page,
        x: usize,
        y: usize,
        cell: &Cell,
        output: &mut String,
        source: point::Coordinate,
    ) {
        let start_len = output.len();
        match cell.content_tag() {
            ContentTag::Codepoint | ContentTag::CodepointGrapheme => {
                if !cell.has_text() {
                    output.push(' ');
                    return;
                }

                self.push_codepoint_with_replacement(cell.codepoint(), output);
                if cell.has_grapheme() {
                    if let Some(graphemes) = page.lookup_grapheme_at(x, y) {
                        for cp in graphemes {
                            self.push_codepoint_with_replacement(cp, output);
                        }
                    }
                }
            }
            ContentTag::BgColorPalette | ContentTag::BgColorRgb => output.push(' '),
        }
        if self.emit == PageOutputFormat::Vt {
            self.push_map_entries(source, output.len() - start_len);
        }
    }

    fn push_codepoint(&self, codepoint: u32, output: &mut String) {
        match self.emit {
            PageOutputFormat::Vt => {
                if let Some(ch) = char::from_u32(codepoint) {
                    output.push(ch);
                }
            }
            PageOutputFormat::Html => match codepoint {
                0x3c => output.push_str("&lt;"),
                0x3e => output.push_str("&gt;"),
                0x26 => output.push_str("&amp;"),
                0x22 => output.push_str("&quot;"),
                0x27 => output.push_str("&#39;"),
                0x00..=0x7f => {
                    if let Some(ch) = char::from_u32(codepoint) {
                        output.push(ch);
                    }
                }
                _ => {
                    let _ = write!(output, "&#{codepoint};");
                }
            },
            PageOutputFormat::Plain => unreachable!(),
        }
    }

    fn push_codepoint_with_replacement(&self, codepoint: u32, output: &mut String) {
        match replacement_for(self.codepoint_map, codepoint) {
            Some(CodepointReplacement::Codepoint(ch)) => {
                self.push_codepoint(*ch as u32, output);
            }
            Some(CodepointReplacement::String(value)) => {
                for ch in value.chars() {
                    self.push_codepoint(ch as u32, output);
                }
            }
            None => self.push_codepoint(codepoint, output),
        }
    }

    fn source_coord(&self, x: usize, y: usize) -> point::Coordinate {
        point::Coordinate::new(
            x.try_into().expect("page cell x must fit CellCountInt"),
            self.screen_y_base + y as u32,
        )
    }

    fn push_map_entries(&mut self, source: point::Coordinate, count: usize) {
        if let Some(point_map) = self.point_map.as_deref_mut() {
            point_map.extend(std::iter::repeat_n(source, count));
        }
    }

    fn push_previous_map_entries(&mut self, count: usize) {
        let Some(point_map) = self.point_map.as_deref_mut() else {
            return;
        };
        let Some(source) = point_map.last().copied() else {
            return;
        };
        point_map.extend(std::iter::repeat_n(source, count));
    }
}

fn push_cell_plain(
    page: &Page,
    x: usize,
    y: usize,
    cell: &Cell,
    codepoint_map: Option<&[CodepointMapEntry]>,
    output: &mut String,
    point_map: Option<&mut Vec<point::Coordinate>>,
    screen_y_base: u32,
) {
    let source = point::Coordinate::new(
        x.try_into().expect("page cell x must fit CellCountInt"),
        screen_y_base + y as u32,
    );
    let mut point_map = point_map;
    push_codepoint_plain_with_replacement(
        cell.codepoint(),
        codepoint_map,
        output,
        source,
        point_map.as_mut().map(|map| &mut **map),
    );

    if cell.has_grapheme() {
        if let Some(graphemes) = page.lookup_grapheme_at(x, y) {
            for cp in graphemes {
                push_codepoint_plain_with_replacement(
                    cp,
                    codepoint_map,
                    output,
                    source,
                    point_map.as_mut().map(|map| &mut **map),
                );
            }
        }
    }
}

fn push_codepoint_plain(
    codepoint: u32,
    output: &mut String,
    source: point::Coordinate,
    point_map: Option<&mut Vec<point::Coordinate>>,
) {
    if let Some(ch) = char::from_u32(codepoint) {
        let len = ch.len_utf8();
        output.push(ch);
        if let Some(point_map) = point_map {
            point_map.extend(std::iter::repeat_n(source, len));
        }
    }
}

fn push_codepoint_plain_with_replacement(
    codepoint: u32,
    codepoint_map: Option<&[CodepointMapEntry]>,
    output: &mut String,
    source: point::Coordinate,
    point_map: Option<&mut Vec<point::Coordinate>>,
) {
    match replacement_for(codepoint_map, codepoint) {
        Some(CodepointReplacement::Codepoint(ch)) => {
            let len = ch.len_utf8();
            output.push(*ch);
            if let Some(point_map) = point_map {
                point_map.extend(std::iter::repeat_n(source, len));
            }
        }
        Some(CodepointReplacement::String(value)) => {
            output.push_str(value);
            if let Some(point_map) = point_map {
                point_map.extend(std::iter::repeat_n(source, value.len()));
            }
        }
        None => push_codepoint_plain(codepoint, output, source, point_map),
    }
}

fn push_blank_cells_plain(
    output: &mut String,
    point_map: Option<&mut Vec<point::Coordinate>>,
    count: usize,
    before_x: usize,
    before_y: usize,
    cols: usize,
    screen_y_base: u32,
) {
    output.extend(std::iter::repeat_n(' ', count));
    let Some(point_map) = point_map else {
        return;
    };

    let mut x = before_x;
    let mut y = screen_y_base + before_y as u32;
    for _ in 0..count {
        if x == 0 {
            if y == 0 {
                x = 0;
            } else {
                y -= 1;
                x = cols.saturating_sub(1);
            }
        } else {
            x -= 1;
        }
        point_map.push(point::Coordinate::new(
            x.try_into().expect("page cell x must fit CellCountInt"),
            y,
        ));
    }
}

fn push_pending_plain_newlines(
    output: &mut String,
    point_map: Option<&mut Vec<point::Coordinate>>,
    count: usize,
    current_screen_y: u32,
) {
    output.extend(std::iter::repeat_n('\n', count));
    let Some(point_map) = point_map else {
        return;
    };

    let first = point_map
        .last()
        .copied()
        .unwrap_or_else(|| point::Coordinate::new(0, 0));
    point_map.push(first);

    for i in 1..count {
        let y = current_screen_y
            .saturating_sub(count as u32)
            .saturating_add(i as u32);
        point_map.push(point::Coordinate::new(0, y));
    }
}

fn replacement_for(
    codepoint_map: Option<&[CodepointMapEntry]>,
    codepoint: u32,
) -> Option<&CodepointReplacement> {
    codepoint_map?
        .iter()
        .rev()
        .find(|entry| entry.matches(codepoint))
        .map(|entry| &entry.replacement)
}

fn valid_scalar_range(start: u32, end: u32) -> bool {
    if start > end {
        return false;
    }
    if char::from_u32(start).is_none() || char::from_u32(end).is_none() {
        return false;
    }
    !(start <= 0xdfff && end >= 0xd800)
}

impl PromptIterator<'_> {
    fn next_down(&mut self) -> Option<Pin> {
        let mut current = self.current?;

        loop {
            let at_limit = self.limit == Some(current);
            match self.list.pin_semantic_prompt(current)? {
                SemanticPrompt::None => {
                    if at_limit {
                        break;
                    }
                }
                SemanticPrompt::Prompt | SemanticPrompt::PromptContinuation => {
                    if at_limit {
                        self.current = None;
                        return Some(Pin { x: 0, ..current });
                    }

                    let mut end_pin = current;
                    while let Some(next_pin) = self.list.pin_down(end_pin, 1) {
                        match self.list.pin_semantic_prompt(next_pin)? {
                            SemanticPrompt::PromptContinuation => {
                                if self.limit == Some(next_pin) {
                                    break;
                                }
                            }
                            SemanticPrompt::Prompt | SemanticPrompt::None => {
                                self.current = Some(next_pin);
                                return Some(Pin { x: 0, ..current });
                            }
                        }
                        end_pin = next_pin;
                    }

                    self.current = None;
                    return Some(Pin { x: 0, ..current });
                }
            }

            let Some(next_pin) = self.list.pin_down(current, 1) else {
                break;
            };
            current = next_pin;
        }

        self.current = None;
        None
    }

    fn next_up(&mut self) -> Option<Pin> {
        let mut current = self.current?;

        loop {
            let at_limit = self.limit == Some(current);
            match self.list.pin_semantic_prompt(current)? {
                SemanticPrompt::None => {
                    if at_limit {
                        break;
                    }
                }
                SemanticPrompt::Prompt => {
                    self.current = if at_limit {
                        None
                    } else {
                        self.list.pin_up(current, 1)
                    };
                    return Some(Pin { x: 0, ..current });
                }
                SemanticPrompt::PromptContinuation => {
                    if at_limit {
                        self.current = None;
                        return Some(Pin { x: 0, ..current });
                    }

                    let mut end_pin = current;
                    while let Some(prior) = self.list.pin_up(end_pin, 1) {
                        if self.limit == Some(prior) {
                            break;
                        }

                        match self.list.pin_semantic_prompt(prior)? {
                            SemanticPrompt::None => {
                                self.current = Some(prior);
                                return Some(Pin { x: 0, ..end_pin });
                            }
                            SemanticPrompt::PromptContinuation => {}
                            SemanticPrompt::Prompt => {
                                self.current = self.list.pin_up(prior, 1);
                                return Some(Pin { x: 0, ..prior });
                            }
                        }
                        end_pin = prior;
                    }

                    self.current = None;
                    return Some(Pin { x: 0, ..current });
                }
            }

            let Some(prior) = self.list.pin_up(current, 1) else {
                break;
            };
            current = prior;
        }

        self.current = None;
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PageListAllocError {
    PageAlloc,
}

impl From<PageAllocError> for PageListAllocError {
    fn from(_: PageAllocError) -> Self {
        Self::PageAlloc
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GrowError {
    PageAlloc,
}

impl From<PageAllocError> for GrowError {
    fn from(_: PageAllocError) -> Self {
        Self::PageAlloc
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntegrityError {
    PageSerialInvalid,
    TotalRowsMismatch,
    TrackedPinInvalid,
    ViewportPinInvalid,
    ViewportPinGarbage,
    ViewportPinOffsetMismatch,
    ViewportPinInsufficientRows,
}

fn standard_page_size() -> usize {
    page_layout(STD_CAPACITY).total_size()
}

fn initial_capacity(cols: CellCountInt) -> Capacity {
    if let Ok(capacity) = STD_CAPACITY.adjust(CapacityAdjustment::cols(cols)) {
        return capacity;
    }

    STD_CAPACITY.with_cols(cols)
}

fn min_max_size(cols: CellCountInt, rows: CellCountInt) -> usize {
    let capacity = initial_capacity(cols);
    let capacity_rows = capacity.rows() as usize;
    let rows = rows as usize;
    let pages_exact = if capacity_rows >= rows {
        1
    } else {
        rows.div_ceil(capacity_rows)
    };
    let pages = pages_exact + 1;
    debug_assert!(pages >= 2);

    standard_page_size() * pages
}

fn double_capacity_u16(value: u16) -> Result<u16, IncreaseCapacityError> {
    if value == u16::MAX {
        return Err(IncreaseCapacityError::OutOfSpace);
    }

    Ok(value.checked_mul(2).unwrap_or(u16::MAX))
}

fn double_capacity_u32(value: u32) -> Result<u32, IncreaseCapacityError> {
    if value == u32::MAX {
        return Err(IncreaseCapacityError::OutOfSpace);
    }

    Ok(value.checked_mul(2).unwrap_or(u32::MAX))
}

fn increase_capacity_value(
    capacity: Capacity,
    adjustment: Option<IncreaseCapacity>,
) -> Result<Capacity, IncreaseCapacityError> {
    let Some(adjustment) = adjustment else {
        return Ok(capacity);
    };

    let capacity = match adjustment {
        IncreaseCapacity::Styles => Capacity::with_metadata(
            capacity.cols(),
            capacity.rows(),
            double_capacity_u16(capacity.styles())? as StyleCountInt,
            capacity.hyperlink_bytes(),
            capacity.grapheme_bytes(),
            capacity.string_bytes(),
        ),
        IncreaseCapacity::GraphemeBytes => Capacity::with_metadata(
            capacity.cols(),
            capacity.rows(),
            capacity.styles(),
            capacity.hyperlink_bytes(),
            double_capacity_u32(capacity.grapheme_bytes())? as GraphemeBytesInt,
            capacity.string_bytes(),
        ),
        IncreaseCapacity::HyperlinkBytes => Capacity::with_metadata(
            capacity.cols(),
            capacity.rows(),
            capacity.styles(),
            double_capacity_u16(capacity.hyperlink_bytes())? as HyperlinkCountInt,
            capacity.grapheme_bytes(),
            capacity.string_bytes(),
        ),
        IncreaseCapacity::StringBytes => Capacity::with_metadata(
            capacity.cols(),
            capacity.rows(),
            capacity.styles(),
            capacity.hyperlink_bytes(),
            capacity.grapheme_bytes(),
            double_capacity_u32(capacity.string_bytes())? as StringBytesInt,
        ),
    };

    if page_layout(capacity).total_size() > MAX_PAGE_SIZE as usize {
        return Err(IncreaseCapacityError::OutOfSpace);
    }

    Ok(capacity)
}

impl PageList {
    fn init(
        cols: CellCountInt,
        rows: CellCountInt,
        max_size: Option<usize>,
    ) -> Result<Self, PageListAllocError> {
        assert!(rows > 0);

        let mut page_serial = 0;
        let (pages, page_size) = init_pages(&mut page_serial, cols, rows)?;
        let first_node = NonNull::from(pages[0].as_ref());
        let mut viewport_pin = Box::new(Pin {
            node: first_node,
            y: 0,
            x: 0,
            garbage: false,
        });
        let tracked_pins = vec![NonNull::from(viewport_pin.as_mut())];

        let result = Self {
            cols,
            rows,
            pages,
            page_serial,
            page_serial_min: 0,
            page_size,
            explicit_max_size: max_size.unwrap_or(usize::MAX),
            min_max_size: min_max_size(cols, rows),
            total_rows: rows,
            tracked_pins,
            tracked_pin_storage: Vec::new(),
            viewport: Viewport::Active,
            viewport_pin,
            viewport_pin_row_offset: None,
        };
        result
            .verify_integrity()
            .expect("newly initialized PageList should be valid");
        Ok(result)
    }

    fn max_size(&self) -> usize {
        self.explicit_max_size.max(self.min_max_size)
    }

    fn reset(&mut self) {
        self.page_serial_min = self.page_serial;

        let capacity = initial_capacity(self.cols);
        let capacity_rows = capacity.rows() as usize;
        assert!(capacity_rows > 0);
        let page_count = (self.rows as usize).div_ceil(capacity_rows);
        assert!(page_count > 0);
        assert!(
            self.pages.len() >= page_count,
            "PageList must contain enough pages to cover active area"
        );

        self.pages.truncate(page_count);
        self.page_size = 0;

        let mut remaining_rows = self.rows as usize;
        for node in &mut self.pages {
            node.page.reinit_with_capacity(capacity);
            let active_rows = remaining_rows.min(capacity_rows);
            node.page.set_size_rows(
                active_rows
                    .try_into()
                    .expect("active page row count must fit CellCountInt"),
            );
            remaining_rows -= active_rows;
            node.serial = self.page_serial;
            self.page_serial += 1;
            self.page_size += node.page.backing_len();
        }
        debug_assert_eq!(remaining_rows, 0);

        self.total_rows = self.rows;

        let first_node = self.first_node_ptr();
        for tracked in &mut self.tracked_pins {
            let pin = unsafe {
                // Safety: tracked pins are owned by this PageList. Reset keeps
                // the Pin allocations stable and only mutates their values.
                tracked.as_mut()
            };
            pin.node = first_node;
            pin.x = 0;
            pin.y = 0;
            pin.garbage = true;
        }
        self.viewport_pin.garbage = false;

        self.viewport = Viewport::Active;
        self.viewport_pin_row_offset = None;
        self.verify_integrity()
            .expect("reset result must preserve PageList integrity");
    }

    fn verify_integrity(&self) -> Result<(), IntegrityError> {
        let mut actual_total_rows = 0usize;
        for node in &self.pages {
            actual_total_rows += node.page.size_rows() as usize;
            if node.serial < self.page_serial_min {
                return Err(IntegrityError::PageSerialInvalid);
            }
        }

        if actual_total_rows != self.total_rows as usize {
            return Err(IntegrityError::TotalRowsMismatch);
        }

        for pin in &self.tracked_pins {
            let pin = unsafe {
                // Safety: tracked pins are created from stable Box<Pin>
                // allocations owned by this PageList. Later mutation code must
                // remove pins before freeing them.
                pin.as_ref()
            };
            if !self.pin_is_valid(pin) {
                return Err(IntegrityError::TrackedPinInvalid);
            }
        }

        if self.viewport_pin.garbage {
            return Err(IntegrityError::ViewportPinGarbage);
        }

        if !self.pin_is_valid(&self.viewport_pin) {
            return Err(IntegrityError::ViewportPinInvalid);
        }

        if self.viewport == Viewport::Pin {
            let Some(actual_offset) = self.viewport_pin_absolute_offset() else {
                return Err(IntegrityError::ViewportPinOffsetMismatch);
            };

            if self
                .viewport_pin_row_offset
                .is_some_and(|cached_offset| cached_offset != actual_offset)
            {
                return Err(IntegrityError::ViewportPinOffsetMismatch);
            }

            if self.total_rows as usize - actual_offset < self.rows as usize {
                return Err(IntegrityError::ViewportPinInsufficientRows);
            }
        }

        Ok(())
    }

    fn pin_is_valid(&self, pin: &Pin) -> bool {
        let Some(node) = self.node_for_pin(pin) else {
            return false;
        };

        pin.x < node.page.size_cols() && pin.y < node.page.size_rows()
    }

    fn node_for_pin(&self, pin: &Pin) -> Option<&Node> {
        self.node_for_ptr(pin.node)
    }

    fn node_for_ptr(&self, node_ptr: NonNull<Node>) -> Option<&Node> {
        self.pages
            .iter()
            .map(Box::as_ref)
            .find(|node| NonNull::from(*node) == node_ptr)
    }

    fn node_index(&self, node_ptr: NonNull<Node>) -> Option<usize> {
        self.pages
            .iter()
            .position(|node| NonNull::from(node.as_ref()) == node_ptr)
    }

    fn viewport_pin_absolute_offset(&self) -> Option<usize> {
        let mut offset = 0usize;
        for node in &self.pages {
            if NonNull::from(node.as_ref()) == self.viewport_pin.node {
                if self.viewport_pin.y >= node.page.size_rows() {
                    return None;
                }
                return Some(offset + self.viewport_pin.y as usize);
            }
            offset += node.page.size_rows() as usize;
        }

        None
    }

    fn pins_reference_node(&self, node: NonNull<Node>) -> bool {
        self.tracked_pins.iter().any(|tracked| unsafe {
            // Safety: tracked pins are created from stable Box<Pin>
            // allocations owned by this PageList.
            tracked.as_ref().node == node
        }) || self.viewport_pin.node == node
    }

    fn first_node_ptr(&self) -> NonNull<Node> {
        NonNull::from(
            self.pages
                .first()
                .expect("PageList must contain at least one page")
                .as_ref(),
        )
    }

    fn last_node_ptr(&self) -> NonNull<Node> {
        NonNull::from(
            self.pages
                .last()
                .expect("PageList must contain at least one page")
                .as_ref(),
        )
    }

    fn get_top_left(&self, tag: point::Tag) -> Pin {
        match tag {
            point::Tag::Screen | point::Tag::History => Pin {
                node: self.first_node_ptr(),
                y: 0,
                x: 0,
                garbage: false,
            },
            point::Tag::Viewport => match self.viewport {
                Viewport::Active => self.get_top_left(point::Tag::Active),
                Viewport::Top => self.get_top_left(point::Tag::Screen),
                Viewport::Pin => *self.viewport_pin,
            },
            point::Tag::Active => {
                let mut remaining = self.rows as usize;
                for node in self.pages.iter().rev() {
                    let node_rows = node.page.size_rows() as usize;
                    if remaining <= node_rows {
                        return Pin {
                            node: NonNull::from(node.as_ref()),
                            y: (node_rows - remaining)
                                .try_into()
                                .expect("active top-left row must fit CellCountInt"),
                            x: 0,
                            garbage: false,
                        };
                    }

                    remaining -= node_rows;
                }

                unreachable!("PageList must contain enough rows for active area");
            }
        }
    }

    fn get_bottom_right(&self, tag: point::Tag) -> Option<Pin> {
        match tag {
            point::Tag::Screen | point::Tag::Active => {
                let node = self.pages.last()?;
                Some(Pin {
                    node: NonNull::from(node.as_ref()),
                    y: node.page.size_rows() - 1,
                    x: node.page.size_cols() - 1,
                    garbage: false,
                })
            }
            point::Tag::Viewport => {
                let mut bottom_right = self.get_top_left(point::Tag::Viewport);
                bottom_right = self.pin_down(bottom_right, self.rows as usize - 1)?;
                let node = self.node_for_pin(&bottom_right)?;
                bottom_right.x = node.page.size_cols() - 1;
                Some(bottom_right)
            }
            point::Tag::History => {
                let mut bottom_right = self.get_top_left(point::Tag::Active);
                bottom_right = self.pin_up(bottom_right, 1)?;
                let node = self.node_for_pin(&bottom_right)?;
                bottom_right.x = node.page.size_cols() - 1;
                Some(bottom_right)
            }
        }
    }

    fn pin(&self, point: point::Point) -> Option<Pin> {
        let coord = point.coord();
        if coord.x >= self.cols {
            return None;
        }

        let mut pin = self.pin_down(self.get_top_left(point.tag()), coord.y as usize)?;
        pin.x = coord.x;
        Some(pin)
    }

    fn point_from_pin(&self, tag: point::Tag, pin: Pin) -> Option<point::Point> {
        let top_left = self.get_top_left(tag);
        let top_left_index = self.node_index(top_left.node)?;
        let pin_index = self.node_index(pin.node)?;

        let mut coord = Coordinate::new(pin.x, 0);
        if pin_index == top_left_index {
            if top_left.y > pin.y {
                return None;
            }
            coord.y = (pin.y - top_left.y) as u32;
        } else {
            if pin_index < top_left_index {
                return None;
            }

            coord.y += (self.pages[top_left_index].page.size_rows() - top_left.y) as u32;
            for node in &self.pages[top_left_index + 1..pin_index] {
                coord.y += node.page.size_rows() as u32;
            }
            coord.y += pin.y as u32;
        }

        Some(match tag {
            point::Tag::Active => point::Point::active(coord),
            point::Tag::Viewport => point::Point::viewport(coord),
            point::Tag::Screen => point::Point::screen(coord),
            point::Tag::History => point::Point::history(coord),
        })
    }

    fn selection_pin_screen_point(&self, pin: Pin) -> Option<point::Point> {
        if pin.garbage || !self.pin_is_valid(&pin) {
            return None;
        }

        self.point_from_pin(point::Tag::Screen, pin)
    }

    fn selection_screen_points(
        &self,
        selection: selection::Selection,
    ) -> Option<(point::Point, point::Point)> {
        Some((
            self.selection_pin_screen_point(selection.start())?,
            self.selection_pin_screen_point(selection.end())?,
        ))
    }

    fn selection_order(&self, selection: selection::Selection) -> Option<selection::Order> {
        let (start, end) = self.selection_screen_points(selection)?;
        let start = start.coord();
        let end = end.coord();

        if selection.rectangle() {
            if start.y > end.y && start.x >= end.x {
                return Some(selection::Order::Reverse);
            }
            if start.y >= end.y && start.x > end.x {
                return Some(selection::Order::Reverse);
            }
            if start.y > end.y && start.x < end.x {
                return Some(selection::Order::MirroredReverse);
            }
            if start.y < end.y && start.x > end.x {
                return Some(selection::Order::MirroredForward);
            }

            return Some(selection::Order::Forward);
        }

        if start.y < end.y {
            return Some(selection::Order::Forward);
        }
        if start.y > end.y {
            return Some(selection::Order::Reverse);
        }
        if start.x <= end.x {
            return Some(selection::Order::Forward);
        }

        Some(selection::Order::Reverse)
    }

    fn selection_top_left(&self, selection: selection::Selection) -> Option<Pin> {
        Some(match self.selection_order(selection)? {
            selection::Order::Forward => selection.start(),
            selection::Order::Reverse => selection.end(),
            selection::Order::MirroredForward => {
                let mut pin = selection.start();
                pin.x = selection.end().x;
                pin
            }
            selection::Order::MirroredReverse => {
                let mut pin = selection.end();
                pin.x = selection.start().x;
                pin
            }
        })
    }

    fn selection_bottom_right(&self, selection: selection::Selection) -> Option<Pin> {
        Some(match self.selection_order(selection)? {
            selection::Order::Forward => selection.end(),
            selection::Order::Reverse => selection.start(),
            selection::Order::MirroredForward => {
                let mut pin = selection.end();
                pin.x = selection.start().x;
                pin
            }
            selection::Order::MirroredReverse => {
                let mut pin = selection.start();
                pin.x = selection.end().x;
                pin
            }
        })
    }

    fn selection_ordered(
        &self,
        selection: selection::Selection,
        desired: selection::Order,
    ) -> Option<selection::Selection> {
        if self.selection_order(selection)? == desired {
            return Some(selection::Selection::new(
                selection.start(),
                selection.end(),
                selection.rectangle(),
            ));
        }

        let top_left = self.selection_top_left(selection)?;
        let bottom_right = self.selection_bottom_right(selection)?;
        Some(match desired {
            selection::Order::Forward
            | selection::Order::MirroredForward
            | selection::Order::MirroredReverse => {
                selection::Selection::new(top_left, bottom_right, selection.rectangle())
            }
            selection::Order::Reverse => {
                selection::Selection::new(bottom_right, top_left, selection.rectangle())
            }
        })
    }

    fn selection_contains(&self, selection: selection::Selection, pin: Pin) -> Option<bool> {
        let top_left = self.selection_top_left(selection)?;
        let bottom_right = self.selection_bottom_right(selection)?;
        let top_left = self.selection_pin_screen_point(top_left)?.coord();
        let bottom_right = self.selection_pin_screen_point(bottom_right)?.coord();
        let point = self.selection_pin_screen_point(pin)?.coord();

        if selection.rectangle() {
            return Some(
                point.y >= top_left.y
                    && point.y <= bottom_right.y
                    && point.x >= top_left.x
                    && point.x <= bottom_right.x,
            );
        }

        if top_left.y == bottom_right.y {
            return Some(
                point.y == top_left.y && point.x >= top_left.x && point.x <= bottom_right.x,
            );
        }

        if point.y == top_left.y {
            return Some(point.x >= top_left.x);
        }

        if point.y == bottom_right.y {
            return Some(point.x <= bottom_right.x);
        }

        Some(point.y > top_left.y && point.y < bottom_right.y)
    }

    fn selection_contained_row(
        &self,
        selection: selection::Selection,
        pin: Pin,
    ) -> Option<selection::Selection> {
        let top_left_pin = self.selection_top_left(selection)?;
        let bottom_right_pin = self.selection_bottom_right(selection)?;
        let top_left = self.selection_pin_screen_point(top_left_pin)?.coord();
        let bottom_right = self.selection_pin_screen_point(bottom_right_pin)?.coord();
        let point = self.selection_pin_screen_point(pin)?.coord();

        self.selection_contained_row_cached(
            selection,
            top_left_pin,
            bottom_right_pin,
            pin,
            top_left,
            bottom_right,
            point,
        )
    }

    fn selection_contained_row_cached(
        &self,
        selection: selection::Selection,
        top_left_pin: Pin,
        bottom_right_pin: Pin,
        pin: Pin,
        top_left: Coordinate,
        bottom_right: Coordinate,
        point: Coordinate,
    ) -> Option<selection::Selection> {
        if point.y < top_left.y || point.y > bottom_right.y {
            return None;
        }

        if selection.rectangle() {
            let mut start = pin;
            start.x = top_left.x;
            let mut end = pin;
            end.x = bottom_right.x;
            return Some(selection::Selection::new(start, end, true));
        }

        if point.y == top_left.y {
            if point.y == bottom_right.y {
                return Some(selection::Selection::new(
                    top_left_pin,
                    bottom_right_pin,
                    false,
                ));
            }

            let mut end = pin;
            end.x = self.cols - 1;
            return Some(selection::Selection::new(top_left_pin, end, false));
        }

        if point.y == bottom_right.y {
            let mut start = pin;
            start.x = 0;
            return Some(selection::Selection::new(start, bottom_right_pin, false));
        }

        let mut start = pin;
        start.x = 0;
        let mut end = pin;
        end.x = self.cols - 1;
        Some(selection::Selection::new(start, end, false))
    }

    fn selection_set_end(selection: &mut selection::Selection, end: Pin) {
        *selection.end_mut() = end;
    }

    fn pin_row_has_text(&self, pin: Pin) -> Option<bool> {
        let node = self.node_for_pin(&pin)?;
        let row = node.page.get_row(pin.y as usize);
        Some(Cell::has_text_any(node.page.get_cells(row)))
    }

    fn pin_last_column(&self, pin: Pin) -> Option<CellCountInt> {
        let node = self.node_for_pin(&pin)?;
        Some(node.page.size_cols() - 1)
    }

    fn selection_adjust(
        &self,
        selection: &mut selection::Selection,
        adjustment: selection::Adjustment,
    ) -> Option<()> {
        let end = selection.end();
        self.selection_pin_screen_point(end)?;

        match adjustment {
            selection::Adjustment::Up => {
                if let Some(new_end) = self.pin_up(end, 1) {
                    Self::selection_set_end(selection, new_end);
                    Some(())
                } else {
                    self.selection_adjust(selection, selection::Adjustment::BeginningOfLine)
                }
            }
            selection::Adjustment::Down => {
                let mut current = end;
                while let Some(next) = self.pin_down(current, 1) {
                    if self.pin_row_has_text(next)? {
                        Self::selection_set_end(selection, next);
                        return Some(());
                    }
                    current = next;
                }

                self.selection_adjust(selection, selection::Adjustment::EndOfLine)
            }
            selection::Adjustment::Left => {
                let mut it = self.cell_iterator_from_pin(Direction::LeftUp, end, None);
                it.next()?;
                for next in it {
                    if self.pin_cell(next)?.has_text() {
                        Self::selection_set_end(selection, next);
                        break;
                    }
                }
                Some(())
            }
            selection::Adjustment::Right => {
                let mut it = self.cell_iterator_from_pin(Direction::RightDown, end, None);
                it.next()?;
                for next in it {
                    if self.pin_cell(next)?.has_text() {
                        Self::selection_set_end(selection, next);
                        break;
                    }
                }
                Some(())
            }
            selection::Adjustment::PageUp => {
                if let Some(new_end) = self.pin_up(end, self.rows as usize) {
                    Self::selection_set_end(selection, new_end);
                    Some(())
                } else {
                    self.selection_adjust(selection, selection::Adjustment::Home)
                }
            }
            selection::Adjustment::PageDown => {
                if let Some(new_end) = self.pin_down(end, self.rows as usize) {
                    Self::selection_set_end(selection, new_end);
                    Some(())
                } else {
                    self.selection_adjust(selection, selection::Adjustment::End)
                }
            }
            selection::Adjustment::Home => {
                let new_end = self.pin(point::Point::screen(Coordinate::new(0, 0)))?;
                Self::selection_set_end(selection, new_end);
                Some(())
            }
            selection::Adjustment::End => {
                let mut it = self.row_iterator(
                    Direction::LeftUp,
                    point::Point::screen(Coordinate::new(0, 0)),
                    None,
                );
                for mut next in &mut it {
                    if self.pin_row_has_text(next)? {
                        next.x = self.pin_last_column(next)?;
                        Self::selection_set_end(selection, next);
                        break;
                    }
                }
                Some(())
            }
            selection::Adjustment::BeginningOfLine => {
                let mut new_end = end;
                new_end.x = 0;
                Self::selection_set_end(selection, new_end);
                Some(())
            }
            selection::Adjustment::EndOfLine => {
                let mut new_end = end;
                new_end.x = self.pin_last_column(end)?;
                Self::selection_set_end(selection, new_end);
                Some(())
            }
        }
    }

    fn track_selection(&mut self, selection: selection::Selection) -> Option<selection::Selection> {
        if selection.is_tracked() {
            return None;
        }

        let start = selection.start();
        let end = selection.end();
        if start.garbage || end.garbage {
            return None;
        }

        let start = self.track_pin(start)?;
        let Some(end) = self.track_pin(end) else {
            self.untrack_pin(start);
            return None;
        };

        Some(selection::Selection::tracked(
            start,
            end,
            selection.rectangle(),
        ))
    }

    fn untrack_selection(&mut self, selection: selection::Selection) {
        let Some((start, end)) = selection.tracked_pins() else {
            return;
        };

        self.untrack_pin(start);
        self.untrack_pin(end);
    }

    fn page_iterator(
        &self,
        direction: Direction,
        top_left: point::Point,
        bottom_left: Option<point::Point>,
    ) -> PageIterator<'_> {
        let top_pin = self.pin(top_left);
        let bottom_pin = bottom_left
            .map(|point| self.pin(point))
            .unwrap_or_else(|| self.get_bottom_right(top_left.tag()));

        match (direction, top_pin, bottom_pin) {
            (Direction::RightDown, Some(top_pin), Some(bottom_pin)) => PageIterator {
                list: self,
                row: Some(top_pin),
                limit: Some(bottom_pin),
                direction,
            },
            (Direction::LeftUp, Some(top_pin), Some(bottom_pin)) => PageIterator {
                list: self,
                row: Some(bottom_pin),
                limit: Some(top_pin),
                direction,
            },
            _ => PageIterator {
                list: self,
                row: None,
                limit: None,
                direction,
            },
        }
    }

    fn row_iterator_from_pin(
        &self,
        direction: Direction,
        pin: Pin,
        limit: Option<Pin>,
    ) -> RowIterator<'_> {
        let mut page_it = PageIterator {
            list: self,
            row: Some(pin),
            limit,
            direction,
        };
        let chunk = page_it.next();
        let offset = match (direction, chunk) {
            (_, None) => 0,
            (Direction::RightDown, Some(chunk)) => chunk.start,
            (Direction::LeftUp, Some(chunk)) => chunk.end - 1,
        };

        RowIterator {
            page_it,
            chunk,
            offset,
        }
    }

    fn empty_row_iterator(&self, direction: Direction) -> RowIterator<'_> {
        RowIterator {
            page_it: PageIterator {
                list: self,
                row: None,
                limit: None,
                direction,
            },
            chunk: None,
            offset: 0,
        }
    }

    fn row_iterator(
        &self,
        direction: Direction,
        top_left: point::Point,
        bottom_left: Option<point::Point>,
    ) -> RowIterator<'_> {
        let top_pin = self.pin(top_left);
        let bottom_pin = bottom_left
            .map(|point| self.pin(point))
            .unwrap_or_else(|| self.get_bottom_right(top_left.tag()));

        match (direction, top_pin, bottom_pin) {
            (Direction::RightDown, Some(top_pin), Some(bottom_pin)) => {
                self.row_iterator_from_pin(direction, top_pin, Some(bottom_pin))
            }
            (Direction::LeftUp, Some(top_pin), Some(bottom_pin)) => {
                self.row_iterator_from_pin(direction, bottom_pin, Some(top_pin))
            }
            _ => self.empty_row_iterator(direction),
        }
    }

    fn cell_iterator_from_pin(
        &self,
        direction: Direction,
        pin: Pin,
        limit: Option<Pin>,
    ) -> CellIterator<'_> {
        let mut row_it = self.row_iterator_from_pin(direction, pin, limit);
        let Some(mut cell) = row_it.next() else {
            return CellIterator { row_it, cell: None };
        };
        cell.x = pin.x;
        CellIterator {
            row_it,
            cell: Some(cell),
        }
    }

    fn empty_cell_iterator(&self, direction: Direction) -> CellIterator<'_> {
        CellIterator {
            row_it: self.empty_row_iterator(direction),
            cell: None,
        }
    }

    fn cell_iterator(
        &self,
        direction: Direction,
        top_left: point::Point,
        bottom_left: Option<point::Point>,
    ) -> CellIterator<'_> {
        let top_pin = self.pin(top_left);
        let bottom_pin = bottom_left
            .map(|point| self.pin(point))
            .unwrap_or_else(|| self.get_bottom_right(top_left.tag()));

        match (direction, top_pin, bottom_pin) {
            (Direction::RightDown, Some(top_pin), Some(bottom_pin)) => {
                self.cell_iterator_from_pin(direction, top_pin, Some(bottom_pin))
            }
            (Direction::LeftUp, Some(top_pin), Some(bottom_pin)) => {
                self.cell_iterator_from_pin(direction, bottom_pin, Some(top_pin))
            }
            _ => self.empty_cell_iterator(direction),
        }
    }

    fn prompt_iterator_from_pin(
        &self,
        direction: Direction,
        pin: Pin,
        limit: Option<Pin>,
    ) -> PromptIterator<'_> {
        PromptIterator {
            list: self,
            current: Some(pin),
            limit,
            direction,
        }
    }

    fn empty_prompt_iterator(&self, direction: Direction) -> PromptIterator<'_> {
        PromptIterator {
            list: self,
            current: None,
            limit: None,
            direction,
        }
    }

    fn prompt_iterator(
        &self,
        direction: Direction,
        top_left: point::Point,
        bottom_left: Option<point::Point>,
    ) -> PromptIterator<'_> {
        let top_pin = self.pin(top_left);
        let bottom_pin = bottom_left
            .map(|point| self.pin(point))
            .unwrap_or_else(|| self.get_bottom_right(top_left.tag()));

        match (direction, top_pin, bottom_pin) {
            (Direction::RightDown, Some(top_pin), Some(bottom_pin)) => {
                self.prompt_iterator_from_pin(direction, top_pin, Some(bottom_pin))
            }
            (Direction::LeftUp, Some(top_pin), Some(bottom_pin)) => {
                self.prompt_iterator_from_pin(direction, bottom_pin, Some(top_pin))
            }
            _ => self.empty_prompt_iterator(direction),
        }
    }

    fn line_iterator(&self, start: Pin) -> LineIterator<'_> {
        LineIterator {
            list: self,
            current: (!start.garbage && self.pin_is_valid(&start)).then_some(start),
        }
    }

    fn selection_string(&self, options: SelectionStringOptions) -> String {
        self.plain_string(PlainStringOptions {
            selection: options.selection,
            trim: options.trim,
            unwrap: true,
        })
    }

    fn dump_string(&self, top_left: Pin, bottom_right: Option<Pin>, unwrap: bool) -> String {
        if top_left.garbage || !self.pin_is_valid(&top_left) {
            return String::new();
        }

        let mut top_left = top_left;
        top_left.x = 0;

        let Some(mut bottom_right) =
            bottom_right.or_else(|| self.get_bottom_right(point::Tag::Screen))
        else {
            return String::new();
        };

        if bottom_right.garbage || !self.pin_is_valid(&bottom_right) {
            return String::new();
        }

        let Some(bottom_node) = self.node_for_ptr(bottom_right.node) else {
            return String::new();
        };
        bottom_right.x = bottom_node.page.size_cols().saturating_sub(1);

        self.plain_string(PlainStringOptions {
            selection: Some(selection::Selection::new(top_left, bottom_right, false)),
            trim: false,
            unwrap,
        })
    }

    fn plain_string(&self, options: PlainStringOptions) -> String {
        self.page_string(PageStringOptions {
            selection: options.selection,
            trim: options.trim,
            unwrap: options.unwrap,
            emit: PageOutputFormat::Plain,
            palette: None,
            codepoint_map: None,
        })
    }

    fn plain_string_with_point_map(
        &self,
        options: PlainStringWithMapOptions<'_>,
    ) -> PageStringWithMap {
        let mut point_map = Vec::new();
        let text = self.page_string_with_point_map(
            PageStringOptions {
                selection: options.selection,
                trim: options.trim,
                unwrap: options.unwrap,
                emit: PageOutputFormat::Plain,
                palette: None,
                codepoint_map: options.codepoint_map,
            },
            &mut point_map,
        );
        PageStringWithMap { text, point_map }
    }

    fn plain_string_with_pin_map(
        &self,
        options: PlainStringWithMapOptions<'_>,
    ) -> PageStringWithPinMap {
        let mapped = self.plain_string_with_point_map(options);
        let mut pin_map = Vec::with_capacity(mapped.point_map.len());
        for coord in mapped.point_map {
            let Some(pin) = self.pin(point::Point::screen(coord)) else {
                return PageStringWithPinMap {
                    text: String::new(),
                    pin_map: Vec::new(),
                };
            };
            pin_map.push(pin);
        }

        PageStringWithPinMap {
            text: mapped.text,
            pin_map,
        }
    }

    fn page_string(&self, options: PageStringOptions<'_>) -> String {
        self.page_string_inner(options, None)
    }

    fn page_string_with_point_map(
        &self,
        options: PageStringOptions<'_>,
        point_map: &mut Vec<point::Coordinate>,
    ) -> String {
        self.page_string_inner(options, Some(point_map))
    }

    fn page_string_inner(
        &self,
        options: PageStringOptions<'_>,
        point_map: Option<&mut Vec<point::Coordinate>>,
    ) -> String {
        let (top_left, bottom_right, rectangle) = match options.selection {
            Some(selection) => {
                let Some(top_left) = self.selection_top_left(selection) else {
                    return String::new();
                };
                let Some(bottom_right) = self.selection_bottom_right(selection) else {
                    return String::new();
                };
                (top_left, bottom_right, selection.rectangle())
            }
            None => {
                let top_left = self.get_top_left(point::Tag::Screen);
                let Some(bottom_right) = self.get_bottom_right(point::Tag::Screen) else {
                    return String::new();
                };
                (top_left, bottom_right, false)
            }
        };

        self.page_string_between(top_left, bottom_right, rectangle, options, point_map)
    }

    fn page_string_between(
        &self,
        top_left: Pin,
        bottom_right: Pin,
        rectangle: bool,
        options: PageStringOptions<'_>,
        mut point_map: Option<&mut Vec<point::Coordinate>>,
    ) -> String {
        if top_left.garbage
            || bottom_right.garbage
            || !self.pin_is_valid(&top_left)
            || !self.pin_is_valid(&bottom_right)
        {
            return String::new();
        }

        let mut output = String::new();
        let mut trailing_state = None;
        let iter = PageIterator {
            list: self,
            row: Some(top_left),
            limit: Some(bottom_right),
            direction: Direction::RightDown,
        };

        for chunk in iter {
            let Some(node) = self.node_for_ptr(chunk.node) else {
                return String::new();
            };
            let Some(screen_y_base) = self
                .point_from_pin(point::Tag::Screen, Pin::new(chunk.node, 0, 0))
                .map(|point| point.coord().y)
            else {
                return String::new();
            };

            let formatter = PlainPageFormat {
                node,
                screen_y_base,
                start_x: if chunk.node == top_left.node {
                    top_left.x
                } else {
                    0
                },
                start_y: chunk.start,
                end_x: if chunk.node == bottom_right.node {
                    bottom_right.x
                } else {
                    node.page.size_cols() - 1
                },
                end_y: chunk.end - 1,
                rectangle,
                trim: options.trim,
                unwrap: options.unwrap,
                trailing_state,
                codepoint_map: options.codepoint_map,
            };
            trailing_state = Some(match options.emit {
                PageOutputFormat::Plain => {
                    formatter.format(&mut output, point_map.as_mut().map(|map| &mut **map))
                }
                PageOutputFormat::Vt | PageOutputFormat::Html => {
                    let mut formatter = StyledPageFormat {
                        node,
                        screen_y_base,
                        start_x: if chunk.node == top_left.node {
                            top_left.x
                        } else {
                            0
                        },
                        start_y: chunk.start,
                        end_x: if chunk.node == bottom_right.node {
                            bottom_right.x
                        } else {
                            node.page.size_cols() - 1
                        },
                        end_y: chunk.end - 1,
                        rectangle,
                        trim: options.trim,
                        unwrap: options.unwrap,
                        emit: options.emit,
                        palette: options.palette,
                        trailing_state,
                        codepoint_map: options.codepoint_map,
                        point_map: if options.emit == PageOutputFormat::Vt {
                            point_map.as_mut().map(|map| &mut **map)
                        } else {
                            None
                        },
                    };
                    formatter.format(&mut output)
                }
            });
        }

        output
    }

    fn prompt_click_move(
        &self,
        cursor_pin: Pin,
        cursor_state_semantic: SemanticContent,
        click_pin: Pin,
        mode: PromptClickMode,
    ) -> PromptClickMove {
        if cursor_pin.garbage
            || click_pin.garbage
            || !self.pin_is_valid(&cursor_pin)
            || !self.pin_is_valid(&click_pin)
        {
            return PromptClickMove::ZERO;
        }

        let Some(cursor_cell_semantic) = self
            .pin_cell(cursor_pin)
            .map(|cell| cell.semantic_content())
        else {
            return PromptClickMove::ZERO;
        };

        if cursor_state_semantic != SemanticContent::Input
            && cursor_cell_semantic != SemanticContent::Input
        {
            return PromptClickMove::ZERO;
        }

        match mode {
            PromptClickMode::None | PromptClickMode::ClickEvents => PromptClickMove::ZERO,
            PromptClickMode::Line
            | PromptClickMode::Multiple
            | PromptClickMode::ConservativeVertical
            | PromptClickMode::SmartVertical => {
                self.prompt_click_line(cursor_pin, cursor_cell_semantic, click_pin)
            }
        }
    }

    fn prompt_click_line(
        &self,
        cursor_pin: Pin,
        cursor_cell_semantic: SemanticContent,
        click_pin: Pin,
    ) -> PromptClickMove {
        if cursor_pin == click_pin {
            return PromptClickMove::ZERO;
        }

        if self.pin_before(cursor_pin, click_pin).unwrap_or(false) {
            return self.prompt_click_line_right(cursor_pin, cursor_cell_semantic, click_pin);
        }

        self.prompt_click_line_left(cursor_pin, click_pin)
    }

    fn prompt_click_line_right(
        &self,
        cursor_pin: Pin,
        cursor_cell_semantic: SemanticContent,
        click_pin: Pin,
    ) -> PromptClickMove {
        let mut count = 0usize;
        let rows = self.row_iterator_from_pin(Direction::RightDown, cursor_pin, Some(click_pin));

        for row_pin in rows {
            let Some(node) = self.node_for_pin(&row_pin) else {
                break;
            };
            let row = node.page.get_row(row_pin.y as usize);
            let cells = node.page.get_cells(row);
            let is_cursor_row = row_pin.node == cursor_pin.node && row_pin.y == cursor_pin.y;

            if !is_cursor_row && row.semantic_prompt() != SemanticPrompt::PromptContinuation {
                break;
            }

            let start_x = if is_cursor_row {
                cursor_pin.x as usize + 1
            } else {
                cells
                    .iter()
                    .position(|cell| cell.semantic_content() == SemanticContent::Input)
                    .unwrap_or(cells.len())
            };

            for (x, cell) in cells.iter().enumerate().skip(start_x) {
                if cell.semantic_content() != SemanticContent::Input {
                    continue;
                }

                count += 1;
                if row_pin.node == click_pin.node
                    && row_pin.y == click_pin.y
                    && x == click_pin.x as usize
                {
                    return PromptClickMove {
                        left: 0,
                        right: count,
                    };
                }
            }

            if !row.wrap() {
                if cursor_cell_semantic == SemanticContent::Input {
                    count += 1;
                }
                break;
            }
        }

        PromptClickMove {
            left: 0,
            right: count,
        }
    }

    fn prompt_click_line_left(&self, cursor_pin: Pin, click_pin: Pin) -> PromptClickMove {
        let mut count = 0usize;
        let rows = self.row_iterator_from_pin(Direction::LeftUp, cursor_pin, Some(click_pin));

        for row_pin in rows {
            let Some(node) = self.node_for_pin(&row_pin) else {
                break;
            };
            let row = node.page.get_row(row_pin.y as usize);
            let cells = node.page.get_cells(row);
            let end_len = if row_pin.node == cursor_pin.node && row_pin.y == cursor_pin.y {
                cursor_pin.x as usize
            } else {
                cells.len()
            };

            for x in (0..end_len).rev() {
                if cells[x].semantic_content() != SemanticContent::Input {
                    continue;
                }

                count += 1;
                if row_pin.node == click_pin.node
                    && row_pin.y == click_pin.y
                    && x == click_pin.x as usize
                {
                    return PromptClickMove {
                        left: count,
                        right: 0,
                    };
                }
            }

            if !row.wrap_continuation() {
                break;
            }
        }

        PromptClickMove {
            left: count,
            right: 0,
        }
    }

    fn semantic_prompt_zone_end(&self, at: Pin) -> Option<Pin> {
        let mut it = self.prompt_iterator_from_pin(Direction::RightDown, at, None);
        let first = it.next()?;
        debug_assert_eq!(first.node, at.node);
        debug_assert_eq!(first.y, at.y);

        if let Some(next) = it.next() {
            let mut prev = self.pin_up(next, 1)?;
            let node = self.node_for_pin(&prev)?;
            prev.x = node.page.size_cols() - 1;
            Some(prev)
        } else {
            self.get_bottom_right(point::Tag::Screen)
        }
    }

    fn highlight_semantic_prompt(&self, at: Pin) -> Option<highlight::Untracked> {
        let end = self.semantic_prompt_zone_end(at)?;
        let mut result = highlight::Untracked {
            start: Pin { x: 0, ..at },
            end: at,
        };

        let it = self.cell_iterator_from_pin(Direction::RightDown, at, Some(end));
        for pin in it {
            let cell = self.pin_cell(pin)?;
            match cell.semantic_content() {
                SemanticContent::Prompt | SemanticContent::Input => result.end = pin,
                SemanticContent::Output => break,
            }
        }

        Some(result)
    }

    fn highlight_semantic_input(&self, at: Pin) -> Option<highlight::Untracked> {
        let end = self.semantic_prompt_zone_end(at)?;
        let mut it = self.cell_iterator_from_pin(Direction::RightDown, at, Some(end));

        let mut result = loop {
            let pin = it.next()?;
            let cell = self.pin_cell(pin)?;
            match cell.semantic_content() {
                SemanticContent::Prompt => {}
                SemanticContent::Input => {
                    break highlight::Untracked {
                        start: pin,
                        end: pin,
                    }
                }
                SemanticContent::Output => return None,
            }
        };

        for pin in it {
            let cell = self.pin_cell(pin)?;
            match cell.semantic_content() {
                SemanticContent::Prompt => {}
                SemanticContent::Input => result.end = pin,
                SemanticContent::Output => break,
            }
        }

        Some(result)
    }

    fn highlight_semantic_output(&self, at: Pin) -> Option<highlight::Untracked> {
        let end = self.semantic_prompt_zone_end(at)?;
        let mut it = self.cell_iterator_from_pin(Direction::RightDown, at, Some(end));

        let mut result = loop {
            let pin = it.next()?;
            let cell = self.pin_cell(pin)?;
            match cell.semantic_content() {
                SemanticContent::Prompt | SemanticContent::Input => {}
                SemanticContent::Output => {
                    if !cell.has_text() {
                        continue;
                    }
                    break highlight::Untracked {
                        start: pin,
                        end: pin,
                    };
                }
            }
        };

        for pin in it {
            let cell = self.pin_cell(pin)?;
            match cell.semantic_content() {
                SemanticContent::Prompt | SemanticContent::Input => break,
                SemanticContent::Output => {
                    if cell.has_text() {
                        result.end = pin;
                    }
                }
            }
        }

        Some(result)
    }

    fn highlight_semantic_content(
        &self,
        at: Pin,
        content: SemanticContent,
    ) -> Option<highlight::Untracked> {
        match content {
            SemanticContent::Prompt => self.highlight_semantic_prompt(at),
            SemanticContent::Input => self.highlight_semantic_input(at),
            SemanticContent::Output => self.highlight_semantic_output(at),
        }
    }

    fn highlight_pin_order_key(&self, pin: Pin) -> Option<(usize, CellCountInt, CellCountInt)> {
        if pin.garbage {
            return None;
        }

        let index = self.node_index(pin.node)?;
        let node = &self.pages[index];
        if pin.y >= node.page.size_rows() {
            return None;
        }
        if pin.x >= self.cols || pin.x >= node.page.size_cols() {
            return None;
        }

        Some((index, pin.y, pin.x))
    }

    fn flatten_highlight(&self, start: Pin, end: Pin) -> Option<highlight::Flattened> {
        let start_key = self.highlight_pin_order_key(start)?;
        let end_key = self.highlight_pin_order_key(end)?;
        if end_key < start_key {
            return None;
        }

        let page_it = PageIterator {
            list: self,
            row: Some(start),
            limit: Some(end),
            direction: Direction::RightDown,
        };
        let mut chunks = Vec::new();
        for chunk in page_it {
            let node = self.node_for_ptr(chunk.node)?;
            chunks.push(highlight::Chunk {
                node: chunk.node,
                serial: node.serial,
                start: chunk.start,
                end: chunk.end,
            });
        }

        if chunks.is_empty() {
            return None;
        }

        Some(highlight::Flattened {
            chunks,
            top_x: start.x,
            bot_x: end.x,
        })
    }

    fn track_highlight(&mut self, highlight: highlight::Untracked) -> Option<highlight::Tracked> {
        let start_key = self.highlight_pin_order_key(highlight.start)?;
        let end_key = self.highlight_pin_order_key(highlight.end)?;
        if end_key < start_key {
            return None;
        }

        let start = self.track_pin(highlight.start)?;
        let Some(end) = self.track_pin(highlight.end) else {
            self.untrack_pin(start);
            return None;
        };

        Some(highlight::Tracked { start, end })
    }

    fn untrack_highlight(&mut self, highlight: highlight::Tracked) {
        self.untrack_pin(highlight.start);
        self.untrack_pin(highlight.end);
    }

    fn clone_region(&self, mut opts: CloneOptions<'_>) -> Result<Self, CloneRegionError> {
        let chunks = self
            .page_iterator(Direction::RightDown, opts.top, opts.bottom)
            .collect::<Vec<_>>();
        if chunks.is_empty() {
            return Err(CloneRegionError::Empty);
        }

        let mut pages = Vec::with_capacity(chunks.len());
        let mut chunk_nodes = Vec::with_capacity(chunks.len());
        let mut page_serial = 0_u64;
        let mut page_size = 0_usize;
        let mut total_rows = 0_usize;

        for chunk in &chunks {
            let source_node = self
                .node_for_ptr(chunk.node)
                .ok_or(CloneRegionError::Empty)?;
            let start = chunk.start as usize;
            let end = chunk.end as usize;
            let capacity = source_node.page.exact_row_capacity(start, end);
            let mut page = Page::init(capacity)?;
            page.set_size_rows(
                (end - start)
                    .try_into()
                    .expect("cloned chunk row count must fit CellCountInt"),
            );
            page.clone_rows_from(&source_node.page, start, end)?;
            page.set_dirty(source_node.page.is_dirty());

            page_size += page.backing_len();
            total_rows += end - start;

            let node = Box::new(Node {
                page,
                serial: page_serial,
            });
            page_serial += 1;
            let node_ptr = NonNull::from(node.as_ref());
            chunk_nodes.push((*chunk, node_ptr));
            pages.push(node);
        }

        let mut viewport_pin = Box::new(Pin {
            node: NonNull::from(pages[0].as_ref()),
            y: 0,
            x: 0,
            garbage: false,
        });
        let mut tracked_pins = vec![NonNull::from(viewport_pin.as_mut())];
        let mut tracked_pin_storage = Vec::new();

        if let Some(remap) = &mut opts.tracked_pins {
            for (chunk, clone_node) in &chunk_nodes {
                for tracked in &self.tracked_pins {
                    let pin = unsafe {
                        // Safety: tracked pins are owned by self and are only
                        // read while self is immutably borrowed.
                        tracked.as_ref()
                    };
                    if pin.node != chunk.node || pin.y < chunk.start || pin.y >= chunk.end {
                        continue;
                    }

                    let mut clone_pin = Box::new(Pin {
                        node: *clone_node,
                        y: pin.y - chunk.start,
                        x: pin.x,
                        garbage: pin.garbage,
                    });
                    let clone_pin_ptr = NonNull::from(clone_pin.as_mut());
                    tracked_pin_storage.push(clone_pin);
                    tracked_pins.push(clone_pin_ptr);
                    remap.insert(*tracked, clone_pin_ptr);
                }
            }
        }

        let mut result = Self {
            cols: self.cols,
            rows: self.rows,
            pages,
            page_serial,
            page_serial_min: 0,
            page_size,
            explicit_max_size: self.explicit_max_size,
            min_max_size: self.min_max_size,
            total_rows: total_rows
                .try_into()
                .expect("cloned total row count must fit CellCountInt"),
            tracked_pins,
            tracked_pin_storage,
            viewport: Viewport::Active,
            viewport_pin,
            viewport_pin_row_offset: None,
        };

        while result.total_rows < result.rows {
            result.grow()?;
        }

        result
            .verify_integrity()
            .expect("clone result must preserve PageList integrity");
        Ok(result)
    }

    fn clear_dirty(&mut self) {
        for node in &mut self.pages {
            node.page.set_dirty(false);
            for y in 0..node.page.size_rows() as usize {
                node.page.get_row_mut(y).set_dirty(false);
            }
        }
    }

    fn total_pages(&self) -> usize {
        self.pages.len()
    }

    fn get_cell(&self, point: point::Point) -> Option<PageListCell<'_>> {
        let pin = self.pin(point)?;
        let node = self.node_for_pin(&pin)?;
        let row = node.page.get_row(pin.y as usize);
        let cell = &node.page.get_cells(row)[pin.x as usize];
        Some(PageListCell {
            node,
            node_ptr: pin.node,
            row,
            cell,
            row_idx: pin.y,
            col_idx: pin.x,
        })
    }

    fn is_dirty(&self, point: point::Point) -> bool {
        self.get_cell(point)
            .map(PageListCell::is_dirty)
            .unwrap_or(false)
    }

    fn mark_dirty(&mut self, point: point::Point) {
        if let Some(pin) = self.pin(point) {
            pin.mark_dirty(self);
        }
    }

    fn pin_is_dirty(&self, pin: Pin) -> bool {
        let Some(node) = self.node_for_pin(&pin) else {
            return false;
        };
        node.page.is_dirty() || node.page.get_row(pin.y as usize).dirty()
    }

    fn pin_semantic_prompt(&self, pin: Pin) -> Option<SemanticPrompt> {
        self.node_for_pin(&pin)
            .map(|node| node.page.get_row(pin.y as usize).semantic_prompt())
    }

    fn pin_cell(&self, pin: Pin) -> Option<&Cell> {
        let node = self.node_for_pin(&pin)?;
        let row = node.page.get_row(pin.y as usize);
        node.page.get_cells(row).get(pin.x as usize)
    }

    fn track_pin(&mut self, pin: Pin) -> Option<NonNull<Pin>> {
        if !self.pin_is_valid(&pin) {
            return None;
        }

        let mut tracked = Box::new(pin);
        let ptr = NonNull::from(tracked.as_mut());
        self.tracked_pin_storage.push(tracked);
        self.tracked_pins.push(ptr);
        Some(ptr)
    }

    fn untrack_pin(&mut self, pin: NonNull<Pin>) {
        assert_ne!(pin, NonNull::from(&*self.viewport_pin));

        let Some(tracked_index) = self.tracked_pins.iter().position(|tracked| *tracked == pin)
        else {
            return;
        };
        self.tracked_pins.swap_remove(tracked_index);

        if let Some(storage_index) = self
            .tracked_pin_storage
            .iter()
            .position(|tracked| NonNull::from(tracked.as_ref()) == pin)
        {
            self.tracked_pin_storage.swap_remove(storage_index);
        }
    }

    fn count_tracked_pins(&self) -> usize {
        self.tracked_pins.len()
    }

    fn tracked_pins(&self) -> &[NonNull<Pin>] {
        &self.tracked_pins
    }

    fn pin_is_active(&self, pin: Pin) -> bool {
        let active = self.get_top_left(point::Tag::Active);
        let Some(active_index) = self.node_index(active.node) else {
            return false;
        };
        let Some(pin_index) = self.node_index(pin.node) else {
            return false;
        };

        if pin_index == active_index {
            pin.y >= active.y
        } else {
            pin_index > active_index
        }
    }

    fn pin_is_top(&self, pin: Pin) -> bool {
        pin.y == 0 && pin.node == self.first_node_ptr()
    }

    fn viewport_row_offset(&mut self) -> usize {
        match self.viewport {
            Viewport::Top => 0,
            Viewport::Active => self.total_rows as usize - self.rows as usize,
            Viewport::Pin => {
                if let Some(offset) = self.viewport_pin_row_offset {
                    self.verify_integrity()
                        .expect("cached viewport pin offset must be valid");
                    return offset;
                }

                let offset = self
                    .viewport_pin_absolute_offset()
                    .expect("viewport pin must point into PageList");
                self.viewport_pin_row_offset = Some(offset);
                self.verify_integrity()
                    .expect("computed viewport pin offset must be valid");
                offset
            }
        }
    }

    fn fixup_viewport(&mut self, removed: usize) {
        match self.viewport {
            Viewport::Active => {}
            Viewport::Pin => {
                if self.pin_is_active(*self.viewport_pin) {
                    self.viewport = Viewport::Active;
                } else if let Some(offset) = &mut self.viewport_pin_row_offset {
                    if *offset < removed {
                        self.viewport = Viewport::Top;
                    } else {
                        *offset -= removed;
                    }
                }
            }
            Viewport::Top => {
                let first = Pin {
                    node: self.first_node_ptr(),
                    y: 0,
                    x: 0,
                    garbage: false,
                };
                if self.pin_is_active(first) {
                    self.viewport = Viewport::Active;
                }
            }
        }
    }

    fn scrollbar(&mut self) -> Scrollbar {
        if self.explicit_max_size == 0 {
            return Scrollbar {
                total: self.rows as usize,
                offset: 0,
                len: self.rows as usize,
            };
        }

        Scrollbar {
            total: self.total_rows as usize,
            offset: self.viewport_row_offset(),
            len: self.rows as usize,
        }
    }

    fn scroll(&mut self, behavior: Scroll) {
        if self.explicit_max_size == 0 {
            self.viewport = Viewport::Active;
            self.verify_integrity()
                .expect("no-scrollback scroll result must be valid");
            return;
        }

        match behavior {
            Scroll::Active => self.viewport = Viewport::Active,
            Scroll::Top => self.viewport = Viewport::Top,
            Scroll::Pin(pin) => self.scroll_to_pin(pin),
            Scroll::Row(row) => self.scroll_to_row(row),
            Scroll::DeltaRow(delta) => self.scroll_delta_row(delta),
        }

        self.verify_integrity()
            .expect("scroll result must preserve PageList integrity");
    }

    fn scroll_to_pin(&mut self, mut pin: Pin) {
        pin.x = 0;
        if self.pin_is_active(pin) {
            self.viewport = Viewport::Active;
        } else if self.pin_is_top(pin) {
            self.viewport = Viewport::Top;
        } else {
            self.set_viewport_pin(pin);
            self.viewport = Viewport::Pin;
            self.viewport_pin_row_offset = None;
        }
    }

    fn scroll_to_row(&mut self, row: usize) {
        if row == 0 {
            self.viewport = Viewport::Top;
            return;
        }

        let active_offset = self.total_rows as usize - self.rows as usize;
        if row >= active_offset {
            self.viewport = Viewport::Active;
            return;
        }

        if self.viewport == Viewport::Pin {
            if let Some(cached_offset) = self.viewport_pin_row_offset {
                let delta = row as isize - cached_offset as isize;
                self.scroll_delta_row(delta);
                return;
            }
        }

        self.viewport_pin_row_offset = Some(row);
        self.viewport = Viewport::Pin;

        let midpoint = self.total_rows as usize / 2;
        if row < midpoint {
            let mut remaining = row;
            for node in &self.pages {
                let node_rows = node.page.size_rows() as usize;
                if remaining < node_rows {
                    self.set_viewport_pin(Pin {
                        node: NonNull::from(node.as_ref()),
                        y: remaining
                            .try_into()
                            .expect("row offset must fit CellCountInt"),
                        x: 0,
                        garbage: false,
                    });
                    return;
                }
                remaining -= node_rows;
            }
        } else {
            let mut remaining = self.total_rows as usize - row;
            for node in self.pages.iter().rev() {
                let node_rows = node.page.size_rows() as usize;
                if remaining <= node_rows {
                    self.set_viewport_pin(Pin {
                        node: NonNull::from(node.as_ref()),
                        y: (node_rows - remaining)
                            .try_into()
                            .expect("row offset must fit CellCountInt"),
                        x: 0,
                        garbage: false,
                    });
                    return;
                }
                remaining -= node_rows;
            }
        }

        self.viewport = Viewport::Active;
    }

    fn scroll_delta_row(&mut self, delta: isize) {
        match self.viewport {
            Viewport::Top if delta <= 0 => return,
            Viewport::Active if delta >= 0 => return,
            Viewport::Pin => {
                if delta == 0 {
                    return;
                }

                if delta < 0 {
                    let rows = (-delta) as usize;
                    if let Some(mut pin) = self.pin_up(*self.viewport_pin, rows) {
                        pin.x = 0;
                        self.set_viewport_pin(pin);
                        if let Some(offset) = &mut self.viewport_pin_row_offset {
                            *offset -= rows;
                        }
                    } else {
                        self.viewport = Viewport::Top;
                    }
                } else {
                    let rows = delta as usize;
                    if let Some(mut pin) = self.pin_down(*self.viewport_pin, rows) {
                        pin.x = 0;
                        if self.pin_is_active(pin) {
                            self.viewport = Viewport::Active;
                        } else {
                            self.set_viewport_pin(pin);
                            if let Some(offset) = &mut self.viewport_pin_row_offset {
                                *offset += rows;
                            }
                        }
                    } else {
                        self.viewport = Viewport::Active;
                    }
                }
                return;
            }
            _ => {}
        }

        let top = self.get_top_left(point::Tag::Viewport);
        let pin = if delta < 0 {
            match self.pin_up(top, (-delta) as usize) {
                Some(pin) => pin,
                None => self.get_top_left(point::Tag::Screen),
            }
        } else {
            match self.pin_down(top, delta as usize) {
                Some(pin) => pin,
                None => {
                    self.viewport = Viewport::Active;
                    return;
                }
            }
        };

        if self.pin_is_active(pin) {
            self.viewport = Viewport::Active;
        } else if self.pin_is_top(pin) {
            self.viewport = Viewport::Top;
        } else {
            self.set_viewport_pin(Pin { x: 0, ..pin });
            self.viewport = Viewport::Pin;
            self.viewport_pin_row_offset = None;
        }
    }

    fn set_viewport_pin(&mut self, pin: Pin) {
        *self.viewport_pin = pin;
    }

    fn create_page(&mut self, capacity: Capacity) -> Result<Box<Node>, PageAllocError> {
        let mut page = Page::init(capacity)?;
        page.set_size_rows(0);
        self.page_size += page.backing_len();

        let node = Box::new(Node {
            page,
            serial: self.page_serial,
        });
        self.page_serial += 1;
        Ok(node)
    }

    fn increase_capacity(
        &mut self,
        target: NonNull<Node>,
        adjustment: Option<IncreaseCapacity>,
    ) -> Result<NonNull<Node>, IncreaseCapacityError> {
        let Some(index) = self.node_index(target) else {
            return Err(IncreaseCapacityError::OutOfSpace);
        };

        let old_capacity = self.pages[index].page.capacity();
        let new_capacity = increase_capacity_value(old_capacity, adjustment)?;
        let old_rows = self.pages[index].page.size_rows();
        let old_cols = self.pages[index].page.size_cols();
        let old_dirty = self.pages[index].page.is_dirty();
        let old_backing_len = self.pages[index].page.backing_len();
        let page_size_before = self.page_size;
        let page_serial_before = self.page_serial;

        let mut replacement = self.create_page(new_capacity)?;
        replacement.page.set_size_rows(old_rows);
        replacement.page.set_size_cols(old_cols);
        if let Err(err) =
            replacement
                .page
                .clone_rows_from(&self.pages[index].page, 0, old_rows as usize)
        {
            self.page_size = page_size_before;
            self.page_serial = page_serial_before;
            return Err(IncreaseCapacityError::CloneFrom(err));
        }
        replacement.page.set_dirty(old_dirty);

        let replacement_ptr = NonNull::from(replacement.as_ref());
        self.pages.insert(index, replacement);
        let old = self.pages.remove(index + 1);
        self.page_size -= old_backing_len;
        drop(old);

        for tracked in &mut self.tracked_pins {
            let pin = unsafe {
                // Safety: tracked pins are owned by this PageList and remain
                // allocated while we update their target node.
                tracked.as_mut()
            };
            if pin.node == target {
                pin.node = replacement_ptr;
            }
        }
        if self.viewport_pin.node == target {
            self.viewport_pin.node = replacement_ptr;
        }

        self.verify_integrity()
            .expect("increase_capacity result must preserve PageList integrity");
        Ok(replacement_ptr)
    }

    fn compact(&mut self, target: NonNull<Node>) -> Result<Option<NonNull<Node>>, PageAllocError> {
        let Some(index) = self.node_index(target) else {
            return Ok(None);
        };

        let old_backing_len = self.pages[index].page.backing_len();
        if old_backing_len <= standard_page_size() {
            return Ok(None);
        }

        let old_rows = self.pages[index].page.size_rows();
        let old_cols = self.pages[index].page.size_cols();
        let old_dirty = self.pages[index].page.is_dirty();
        let required_capacity = self.pages[index]
            .page
            .exact_row_capacity(0, old_rows as usize);
        let new_size = page_layout(required_capacity).total_size();
        if new_size >= old_backing_len {
            return Ok(None);
        }

        let page_size_before = self.page_size;
        let page_serial_before = self.page_serial;
        let mut replacement = self.create_page(required_capacity)?;
        replacement.page.set_size_rows(old_rows);
        replacement.page.set_size_cols(old_cols);
        if replacement
            .page
            .clone_rows_from(&self.pages[index].page, 0, old_rows as usize)
            .is_err()
        {
            self.page_size = page_size_before;
            self.page_serial = page_serial_before;
            return Ok(None);
        }
        replacement.page.set_dirty(old_dirty);

        let replacement_ptr = NonNull::from(replacement.as_ref());
        self.pages.insert(index, replacement);
        let old = self.pages.remove(index + 1);
        self.page_size -= old_backing_len;
        drop(old);

        for tracked in &mut self.tracked_pins {
            let pin = unsafe {
                // Safety: tracked pins are owned by this PageList and remain
                // allocated while we update their target node.
                tracked.as_mut()
            };
            if pin.node == target {
                pin.node = replacement_ptr;
            }
        }
        if self.viewport_pin.node == target {
            self.viewport_pin.node = replacement_ptr;
        }

        self.verify_integrity()
            .expect("compact result must preserve PageList integrity");
        Ok(Some(replacement_ptr))
    }

    fn split(&mut self, pin: Pin) -> Result<(), SplitError> {
        if !self.pin_is_valid(&pin) {
            return Err(SplitError::OutOfSpace);
        }

        let original_node = pin.node;
        let Some(index) = self.node_index(original_node) else {
            return Err(SplitError::OutOfSpace);
        };

        let old_rows = self.pages[index].page.size_rows();
        if old_rows <= 1 {
            return Err(SplitError::OutOfSpace);
        }
        if pin.y == 0 {
            return Ok(());
        }

        let old_cols = self.pages[index].page.size_cols();
        let old_capacity = self.pages[index].page.capacity();
        let page_size_before = self.page_size;
        let page_serial_before = self.page_serial;
        let new_rows = old_rows - pin.y;
        let mut target = self.create_page(old_capacity)?;
        target.page.set_size_rows(new_rows);
        target.page.set_size_cols(old_cols);

        if target
            .page
            .clone_rows_from(&self.pages[index].page, pin.y as usize, old_rows as usize)
            .is_err()
        {
            self.page_size = page_size_before;
            self.page_serial = page_serial_before;
            return Err(SplitError::OutOfSpace);
        }

        let target_ptr = NonNull::from(target.as_ref());
        for tracked in &mut self.tracked_pins {
            let tracked_pin = unsafe {
                // Safety: tracked pins are owned by this PageList and remain
                // allocated while we update their target node.
                tracked.as_mut()
            };
            if tracked_pin.node == original_node && tracked_pin.y >= pin.y {
                tracked_pin.node = target_ptr;
                tracked_pin.y -= pin.y;
            }
        }
        if self.viewport_pin.node == original_node && self.viewport_pin.y >= pin.y {
            self.viewport_pin.node = target_ptr;
            self.viewport_pin.y -= pin.y;
        }

        for row in pin.y as usize..old_rows as usize {
            self.pages[index]
                .page
                .clear_cells(row, 0, old_cols as usize);
        }
        self.pages[index].page.set_size_rows(pin.y);
        self.pages.insert(index + 1, target);

        self.verify_integrity()
            .expect("split result must preserve PageList integrity");
        Ok(())
    }

    fn erase_row(&mut self, point: point::Point) -> Result<(), EraseRowError> {
        let pin = self.pin(point).ok_or(EraseRowError::InvalidPoint)?;
        let Some(mut index) = self.node_index(pin.node) else {
            return Err(EraseRowError::InvalidPoint);
        };

        let page_rows = self.pages[index].page.size_rows() as usize;
        self.pages[index]
            .page
            .rotate_rows_left(pin.y as usize, page_rows);

        let current = NonNull::from(self.pages[index].as_ref());
        for tracked in &mut self.tracked_pins {
            let tracked_pin = unsafe {
                // Safety: tracked pins are owned by this PageList and remain
                // allocated while we update their target node.
                tracked.as_mut()
            };
            if tracked_pin.node == current && tracked_pin.y > pin.y {
                tracked_pin.y -= 1;
            }
        }

        self.fixup_viewport(1);
        self.pages[index].page.set_dirty(true);

        while index + 1 < self.pages.len() {
            let (left, right) = self.pages.split_at_mut(index + 1);
            let previous = &mut left[index];
            let next = &mut right[0];
            let previous_last = previous.page.size_rows() as usize - 1;
            previous.page.clone_row_from(&next.page, previous_last, 0)?;

            index += 1;
            let current = NonNull::from(self.pages[index].as_ref());
            let previous = NonNull::from(self.pages[index - 1].as_ref());
            let previous_last = self.pages[index - 1].page.size_rows() - 1;
            let page_rows = self.pages[index].page.size_rows() as usize;
            self.pages[index].page.rotate_rows_left(0, page_rows);
            self.pages[index].page.set_dirty(true);

            for tracked in &mut self.tracked_pins {
                let tracked_pin = unsafe {
                    // Safety: tracked pins are owned by this PageList and
                    // remain allocated while we update their target node.
                    tracked.as_mut()
                };
                if tracked_pin.node != current {
                    continue;
                }
                if tracked_pin.y == 0 {
                    tracked_pin.node = previous;
                    tracked_pin.y = previous_last;
                } else {
                    tracked_pin.y -= 1;
                }
            }
        }

        let last_row = self.pages[index].page.size_rows() as usize - 1;
        let cols = self.pages[index].page.size_cols() as usize;
        self.pages[index].page.clear_cells(last_row, 0, cols);

        self.verify_integrity()
            .expect("erase_row result must preserve PageList integrity");
        Ok(())
    }

    fn erase_row_bounded(
        &mut self,
        point: point::Point,
        limit: usize,
    ) -> Result<(), EraseRowError> {
        let pin = self.pin(point).ok_or(EraseRowError::InvalidPoint)?;
        let Some(mut index) = self.node_index(pin.node) else {
            return Err(EraseRowError::InvalidPoint);
        };

        let mut current = NonNull::from(self.pages[index].as_ref());
        let page_rows = self.pages[index].page.size_rows() as usize;
        let start = pin.y as usize;

        if page_rows - start > limit {
            let cols = self.pages[index].page.size_cols() as usize;
            self.pages[index].page.clear_cells(start, 0, cols);
            self.pages[index]
                .page
                .rotate_rows_left(start, start + limit + 1);
            self.pages[index].page.set_dirty(true);

            if self.viewport == Viewport::Pin {
                let viewport_pin = *self.viewport_pin;
                if let Some(offset) = self.viewport_pin_row_offset.as_mut() {
                    if viewport_pin.node == current
                        && viewport_pin.y >= pin.y
                        && viewport_pin.y <= pin.y + limit as CellCountInt
                        && viewport_pin.y != 0
                    {
                        *offset -= 1;
                    }
                }
            }

            for tracked in &mut self.tracked_pins {
                let tracked_pin = unsafe {
                    // Safety: tracked pins are owned by this PageList and
                    // remain allocated while we update their target node.
                    tracked.as_mut()
                };
                if tracked_pin.node == current
                    && tracked_pin.y >= pin.y
                    && tracked_pin.y <= pin.y + limit as CellCountInt
                {
                    if tracked_pin.y == 0 {
                        tracked_pin.x = 0;
                    } else {
                        tracked_pin.y -= 1;
                    }
                }
            }

            self.verify_integrity()
                .expect("erase_row_bounded result must preserve PageList integrity");
            return Ok(());
        }

        self.pages[index].page.rotate_rows_left(start, page_rows);
        self.pages[index].page.set_dirty(true);
        let mut shifted = page_rows - start;

        if self.viewport == Viewport::Pin {
            let viewport_pin = *self.viewport_pin;
            if let Some(offset) = self.viewport_pin_row_offset.as_mut() {
                if viewport_pin.node == current && viewport_pin.y >= pin.y && viewport_pin.y != 0 {
                    *offset -= 1;
                }
            }
        }

        for tracked in &mut self.tracked_pins {
            let tracked_pin = unsafe {
                // Safety: tracked pins are owned by this PageList and remain
                // allocated while we update their target node.
                tracked.as_mut()
            };
            if tracked_pin.node == current && tracked_pin.y >= pin.y {
                if tracked_pin.y == 0 {
                    tracked_pin.x = 0;
                } else {
                    tracked_pin.y -= 1;
                }
            }
        }

        while index + 1 < self.pages.len() {
            let (left, right) = self.pages.split_at_mut(index + 1);
            let previous = &mut left[index];
            let next = &mut right[0];
            let previous_last = previous.page.size_rows() as usize - 1;
            previous.page.clone_row_from(&next.page, previous_last, 0)?;

            index += 1;
            current = NonNull::from(self.pages[index].as_ref());
            let previous = NonNull::from(self.pages[index - 1].as_ref());
            let previous_last = self.pages[index - 1].page.size_rows() - 1;
            let page_rows = self.pages[index].page.size_rows() as usize;
            let shifted_limit = limit - shifted;

            if page_rows > shifted_limit {
                let cols = self.pages[index].page.size_cols() as usize;
                self.pages[index].page.clear_cells(0, 0, cols);
                self.pages[index]
                    .page
                    .rotate_rows_left(0, shifted_limit + 1);
                self.pages[index].page.set_dirty(true);

                if self.viewport == Viewport::Pin {
                    let viewport_pin = *self.viewport_pin;
                    if let Some(offset) = self.viewport_pin_row_offset.as_mut() {
                        if viewport_pin.node == current
                            && viewport_pin.y <= shifted_limit as CellCountInt
                        {
                            *offset -= 1;
                        }
                    }
                }

                for tracked in &mut self.tracked_pins {
                    let tracked_pin = unsafe {
                        // Safety: tracked pins are owned by this PageList and
                        // remain allocated while we update their target node.
                        tracked.as_mut()
                    };
                    if tracked_pin.node != current || tracked_pin.y > shifted_limit as CellCountInt
                    {
                        continue;
                    }
                    if tracked_pin.y == 0 {
                        tracked_pin.node = previous;
                        tracked_pin.y = previous_last;
                    } else {
                        tracked_pin.y -= 1;
                    }
                }

                self.verify_integrity()
                    .expect("erase_row_bounded result must preserve PageList integrity");
                return Ok(());
            }

            self.pages[index].page.rotate_rows_left(0, page_rows);
            self.pages[index].page.set_dirty(true);
            shifted += page_rows;

            if self.viewport == Viewport::Pin {
                let viewport_pin = *self.viewport_pin;
                if let Some(offset) = self.viewport_pin_row_offset.as_mut() {
                    if viewport_pin.node == current {
                        *offset -= 1;
                    }
                }
            }

            for tracked in &mut self.tracked_pins {
                let tracked_pin = unsafe {
                    // Safety: tracked pins are owned by this PageList and
                    // remain allocated while we update their target node.
                    tracked.as_mut()
                };
                if tracked_pin.node != current {
                    continue;
                }
                if tracked_pin.y == 0 {
                    tracked_pin.node = previous;
                    tracked_pin.y = previous_last;
                } else {
                    tracked_pin.y -= 1;
                }
            }
        }

        let last_row = self.pages[index].page.size_rows() as usize - 1;
        let cols = self.pages[index].page.size_cols() as usize;
        self.pages[index].page.clear_cells(last_row, 0, cols);

        self.verify_integrity()
            .expect("erase_row_bounded result must preserve PageList integrity");
        Ok(())
    }

    fn erase_page(&mut self, node: NonNull<Node>) -> Result<(), ErasePageError> {
        let Some(index) = self.node_index(node) else {
            return Err(ErasePageError::InvalidPage);
        };
        if self.pages.len() == 1 {
            return Err(ErasePageError::OnlyPage);
        }
        if index != 0 && index + 1 != self.pages.len() {
            return Err(ErasePageError::MiddlePage);
        }

        let replacement = if index == 0 {
            NonNull::from(self.pages[1].as_ref())
        } else {
            NonNull::from(self.pages[index - 1].as_ref())
        };
        if index == 0 {
            self.page_serial_min = self.pages[1].serial;
        }

        for tracked in &mut self.tracked_pins {
            let tracked_pin = unsafe {
                // Safety: tracked pins are owned by this PageList and remain
                // allocated while we update their target node before dropping
                // the removed page allocation.
                tracked.as_mut()
            };
            if tracked_pin.node == node {
                tracked_pin.node = replacement;
                tracked_pin.y = 0;
                tracked_pin.x = 0;
            }
        }

        if self.viewport == Viewport::Pin {
            self.viewport_pin_row_offset = None;
        }

        let removed = self.pages.remove(index);
        self.page_size -= removed.page.backing_len();
        debug_assert!(
            !self.pins_reference_node(node),
            "erase_page must not leave pins pointing at a removed page"
        );
        Ok(())
    }

    fn erase_history(&mut self, bottom_left: Option<point::Point>) -> Result<(), EraseRowsError> {
        self.erase_rows(
            EraseRowsMode::History,
            point::Point::history(Coordinate::new(0, 0)),
            bottom_left,
        )
    }

    fn erase_active(&mut self, y: CellCountInt) -> Result<(), EraseRowsError> {
        assert!(y < self.rows);
        self.erase_rows(
            EraseRowsMode::Active,
            point::Point::active(Coordinate::new(0, 0)),
            Some(point::Point::active(Coordinate::new(0, y as u32))),
        )
    }

    fn validate_erase_chunks(
        &self,
        mode: EraseRowsMode,
        chunks: &[PageChunk],
    ) -> Result<(), EraseRowsError> {
        if chunks.is_empty() {
            return Err(EraseRowsError::InvalidPoint);
        }

        match mode {
            EraseRowsMode::History => {
                let mut expected_front = 0usize;
                let mut saw_partial = false;
                for chunk in chunks {
                    if chunk.full_page(self) {
                        if saw_partial {
                            return Err(EraseRowsError::MiddlePage);
                        }
                        let Some(index) = self.node_index(chunk.node) else {
                            return Err(EraseRowsError::InvalidPoint);
                        };
                        if index != expected_front {
                            return Err(EraseRowsError::MiddlePage);
                        }
                        expected_front += 1;
                    } else {
                        saw_partial = true;
                    }
                }
            }
            EraseRowsMode::Active => {
                let full_indexes = chunks
                    .iter()
                    .filter(|chunk| chunk.full_page(self))
                    .map(|chunk| {
                        self.node_index(chunk.node)
                            .ok_or(EraseRowsError::InvalidPoint)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if !full_indexes.is_empty() {
                    let suffix_start = self.pages.len() - full_indexes.len();
                    let mut sorted = full_indexes;
                    sorted.sort_unstable();
                    if sorted != (suffix_start..self.pages.len()).collect::<Vec<_>>() {
                        return Err(EraseRowsError::MiddlePage);
                    }
                }
            }
        }

        Ok(())
    }

    fn erase_rows(
        &mut self,
        mode: EraseRowsMode,
        top_left: point::Point,
        bottom_left: Option<point::Point>,
    ) -> Result<(), EraseRowsError> {
        let chunks = self
            .page_iterator(Direction::RightDown, top_left, bottom_left)
            .collect::<Vec<_>>();
        self.validate_erase_chunks(mode, &chunks)?;

        let mut erased = 0usize;
        let ordered_chunks = match mode {
            EraseRowsMode::History => chunks,
            EraseRowsMode::Active => chunks.into_iter().rev().collect(),
        };

        for chunk in ordered_chunks {
            let Some(index) = self.node_index(chunk.node) else {
                return Err(EraseRowsError::InvalidPoint);
            };
            if chunk.full_page(self) {
                let rows = self.pages[index].page.size_rows() as usize;
                if self.pages.len() == 1 {
                    self.pages[index].page.reinit();
                    self.pages[index].page.set_size_rows(0);
                    let current = NonNull::from(self.pages[index].as_ref());
                    for tracked in &mut self.tracked_pins {
                        let pin = unsafe {
                            // Safety: tracked pins are owned by this
                            // PageList. The only page remains allocated; we
                            // only move pins to its new top-left position.
                            tracked.as_mut()
                        };
                        if pin.node == current {
                            pin.y = 0;
                            pin.x = 0;
                        }
                    }
                    erased += rows;
                    break;
                }

                erased += rows;
                self.erase_page(chunk.node)?;
                continue;
            }

            erased += self.erase_partial_chunk(chunk)?;
        }

        let erased_rows =
            CellCountInt::try_from(erased).expect("erased row count must fit CellCountInt");
        self.total_rows -= erased_rows;
        if mode == EraseRowsMode::Active {
            self.grow_rows(erased)?;
        }
        self.fixup_viewport(erased);
        self.verify_integrity()
            .expect("erase_rows result must preserve PageList integrity");
        Ok(())
    }

    fn erase_partial_chunk(&mut self, chunk: PageChunk) -> Result<usize, EraseRowsError> {
        let Some(index) = self.node_index(chunk.node) else {
            return Err(EraseRowsError::InvalidPoint);
        };
        let old_rows = self.pages[index].page.size_rows();
        if chunk.start >= chunk.end || chunk.end > old_rows {
            return Err(EraseRowsError::InvalidPoint);
        }

        let start = chunk.start as usize;
        let end = chunk.end as usize;
        let erased = end - start;
        let old_rows_usize = old_rows as usize;
        let new_rows = old_rows_usize - erased;
        let next_node = self
            .pages
            .get(index + 1)
            .map(|node| NonNull::from(node.as_ref()));
        for _ in 0..erased {
            self.pages[index]
                .page
                .rotate_rows_left(start, old_rows_usize);
        }

        let cols = self.pages[index].page.size_cols() as usize;
        for row in new_rows..old_rows_usize {
            self.pages[index].page.clear_cells(row, 0, cols);
        }
        self.pages[index].page.set_size_rows(
            new_rows
                .try_into()
                .expect("page row count must fit CellCountInt"),
        );
        self.pages[index].page.set_dirty(true);

        let current = NonNull::from(self.pages[index].as_ref());
        let new_rows_cell =
            CellCountInt::try_from(new_rows).expect("page row count must fit CellCountInt");
        let erased_cell =
            CellCountInt::try_from(erased).expect("erased row count must fit CellCountInt");
        for tracked in &mut self.tracked_pins {
            let pin = unsafe {
                // Safety: tracked pins are owned by this PageList and remain
                // allocated while we update their row after the page shrink.
                tracked.as_mut()
            };
            if pin.node != current {
                continue;
            }
            if pin.y >= chunk.end {
                pin.y -= erased_cell;
            } else if pin.y >= chunk.start {
                if chunk.end == old_rows {
                    if let Some(next_node) = next_node {
                        pin.node = next_node;
                        pin.y = 0;
                    } else {
                        pin.y = chunk.start;
                    }
                } else {
                    pin.y = chunk.start.min(new_rows_cell.saturating_sub(1));
                }
                pin.x = 0;
            }
        }

        Ok(erased)
    }

    fn scroll_clear(&mut self) -> Result<(), GrowError> {
        let mut rows_to_scroll = 0usize;
        for active_y in (0..self.rows).rev() {
            let Some(pin) = self.pin(point::Point::active(Coordinate::new(0, active_y as u32)))
            else {
                continue;
            };
            let Some(node) = self.node_for_pin(&pin) else {
                continue;
            };
            let row = node.page.get_row(pin.y as usize);
            let cells = node.page.get_cells(row);
            if cells[..self.cols as usize]
                .iter()
                .any(|cell| !cell.is_empty())
            {
                rows_to_scroll = active_y as usize + 1;
                break;
            }
        }

        self.grow_rows(rows_to_scroll)?;
        self.verify_integrity()
            .expect("scroll_clear result must preserve PageList integrity");
        Ok(())
    }

    fn grow(&mut self) -> Result<Option<NonNull<Node>>, GrowError> {
        let last = self
            .pages
            .last_mut()
            .expect("PageList must contain at least one page");
        if last.page.capacity().rows() > last.page.size_rows() {
            last.page.set_size_rows(last.page.size_rows() + 1);
            self.total_rows += 1;
            self.verify_integrity()
                .expect("fast grow result must preserve PageList integrity");
            return Ok(None);
        }

        if self.pages.len() > 1 && self.page_size + standard_page_size() > self.max_size() {
            if let Some(reused) = self.prune_for_growth()? {
                return Ok(Some(reused));
            }
        }

        let capacity = initial_capacity(self.cols);
        let mut node = self.create_page(capacity)?;
        node.page.set_size_rows(1);
        let node_ptr = NonNull::from(node.as_ref());
        self.pages.push(node);
        self.total_rows += 1;
        self.verify_integrity()
            .expect("append grow result must preserve PageList integrity");
        Ok(Some(node_ptr))
    }

    fn prune_for_growth(&mut self) -> Result<Option<NonNull<Node>>, GrowError> {
        let mut first = self.pages.remove(0);
        let first_rows = first.page.size_rows() as usize;
        let first_serial = first.serial;
        let first_ptr = NonNull::from(first.as_ref());
        self.total_rows -= first.page.size_rows();

        if self.total_rows as usize + 1 < self.rows as usize {
            self.total_rows += first.page.size_rows();
            self.pages.insert(0, first);
            return Ok(None);
        }

        if self.viewport == Viewport::Pin {
            if let Some(offset) = &mut self.viewport_pin_row_offset {
                if *offset < first_rows {
                    self.viewport = Viewport::Top;
                } else {
                    *offset -= first_rows;
                }
            }
        }

        let new_first = self.first_node_ptr();
        for tracked in &mut self.tracked_pins {
            let pin = unsafe {
                // Safety: tracked pins are owned by this PageList. We are only
                // mutating pins that remain tracked.
                tracked.as_mut()
            };
            if pin.node != first_ptr {
                continue;
            }

            pin.node = new_first;
            pin.x = 0;
            pin.y = 0;
            pin.garbage = true;
        }
        self.viewport_pin.garbage = false;

        if first.page.backing_len() > standard_page_size() {
            self.page_size -= first.page.backing_len();
            drop(first);
            return Ok(None);
        }

        first.page.reinit_with_capacity(initial_capacity(self.cols));
        first.page.set_size_rows(1);
        self.page_serial_min = first_serial + 1;
        first.serial = self.page_serial;
        self.page_serial += 1;
        let reused = NonNull::from(first.as_ref());
        self.pages.push(first);
        self.total_rows += 1;
        self.verify_integrity()
            .expect("prune grow result must preserve PageList integrity");
        Ok(Some(reused))
    }

    fn grow_rows(&mut self, rows: usize) -> Result<(), GrowError> {
        for _ in 0..rows {
            self.grow()?;
        }

        Ok(())
    }

    fn pin_down(&self, pin: Pin, rows: usize) -> Option<Pin> {
        let index = self.node_index(pin.node)?;
        let node_rows = self.pages[index].page.size_rows() as usize;
        let remaining_in_row = node_rows - (pin.y as usize + 1);
        if rows <= remaining_in_row {
            let mut result = pin;
            result.y = (pin.y as usize + rows)
                .try_into()
                .expect("pin row must fit CellCountInt");
            return Some(result);
        }

        let mut rows_left = rows - remaining_in_row;
        for node in &self.pages[index + 1..] {
            let page_rows = node.page.size_rows() as usize;
            if rows_left <= page_rows {
                return Some(Pin {
                    node: NonNull::from(node.as_ref()),
                    y: (rows_left - 1)
                        .try_into()
                        .expect("pin row must fit CellCountInt"),
                    x: pin.x,
                    garbage: pin.garbage,
                });
            }
            rows_left -= page_rows;
        }

        None
    }

    fn pin_up(&self, pin: Pin, rows: usize) -> Option<Pin> {
        let index = self.node_index(pin.node)?;
        if rows <= pin.y as usize {
            let mut result = pin;
            result.y = (pin.y as usize - rows)
                .try_into()
                .expect("pin row must fit CellCountInt");
            return Some(result);
        }

        let mut rows_left = rows - pin.y as usize;
        for node in self.pages[..index].iter().rev() {
            let page_rows = node.page.size_rows() as usize;
            if rows_left <= page_rows {
                return Some(Pin {
                    node: NonNull::from(node.as_ref()),
                    y: (page_rows - rows_left)
                        .try_into()
                        .expect("pin row must fit CellCountInt"),
                    x: pin.x,
                    garbage: pin.garbage,
                });
            }
            rows_left -= page_rows;
        }

        None
    }

    fn pin_absolute_row(&self, pin: Pin) -> Option<usize> {
        if pin.garbage || !self.pin_is_valid(&pin) {
            return None;
        }

        let mut row = 0usize;
        for node in &self.pages {
            if NonNull::from(node.as_ref()) == pin.node {
                return Some(row + pin.y as usize);
            }
            row += node.page.size_rows() as usize;
        }

        None
    }

    fn pin_at_absolute_row(&self, row: usize, x: CellCountInt, garbage: bool) -> Option<Pin> {
        let mut remaining = row;
        for node in &self.pages {
            let rows = node.page.size_rows() as usize;
            if remaining < rows {
                if x >= node.page.size_cols() {
                    return None;
                }
                return Some(Pin {
                    node: NonNull::from(node.as_ref()),
                    y: remaining
                        .try_into()
                        .expect("absolute row offset must fit CellCountInt"),
                    x,
                    garbage,
                });
            }
            remaining -= rows;
        }

        None
    }

    fn pin_before(&self, pin: Pin, other: Pin) -> Option<bool> {
        if pin.garbage || other.garbage {
            return None;
        }
        if !self.pin_is_valid(&pin) || !self.pin_is_valid(&other) {
            return None;
        }

        if pin.node == other.node {
            if pin.y < other.y {
                return Some(true);
            }
            if pin.y > other.y {
                return Some(false);
            }
            return Some(pin.x < other.x);
        }

        Some(self.node_index(pin.node)? < self.node_index(other.node)?)
    }

    fn pin_left_clamp(&self, pin: Pin, cells: CellCountInt) -> Option<Pin> {
        if pin.garbage || !self.pin_is_valid(&pin) {
            return None;
        }

        let mut result = pin;
        result.x = result.x.saturating_sub(cells);
        Some(result)
    }

    fn pin_right_clamp(&self, pin: Pin, cells: CellCountInt) -> Option<Pin> {
        if pin.garbage || !self.pin_is_valid(&pin) {
            return None;
        }

        let node = self.node_for_pin(&pin)?;
        let max_x = node.page.size_cols() - 1;
        let mut result = pin;
        result.x = result.x.saturating_add(cells).min(max_x);
        Some(result)
    }

    fn pin_left_wrap(&self, pin: Pin, cells: usize) -> Option<Pin> {
        let node = self.node_for_pin(&pin)?;
        let cols = node.page.size_cols() as usize;
        let row = self.pin_absolute_row(pin)?;
        let linear = row.checked_mul(cols)?.checked_add(pin.x as usize)?;
        let target = linear.checked_sub(cells)?;
        let target_row = target / cols;
        let target_x = (target % cols)
            .try_into()
            .expect("wrapped pin x must fit CellCountInt");

        self.pin_at_absolute_row(target_row, target_x, pin.garbage)
    }

    fn pin_right_wrap(&self, pin: Pin, cells: usize) -> Option<Pin> {
        let node = self.node_for_pin(&pin)?;
        let cols = node.page.size_cols() as usize;
        let total = self.total_rows().checked_mul(cols)?.checked_sub(1)?;
        let row = self.pin_absolute_row(pin)?;
        let linear = row.checked_mul(cols)?.checked_add(pin.x as usize)?;
        let target = linear.checked_add(cells)?;
        if target > total {
            return None;
        }
        let target_row = target / cols;
        let target_x = (target % cols)
            .try_into()
            .expect("wrapped pin x must fit CellCountInt");

        self.pin_at_absolute_row(target_row, target_x, pin.garbage)
    }

    fn select_all(&self) -> Option<selection::Selection> {
        let start = {
            let mut result = None;
            for pin in self.cell_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(0, 0)),
                None,
            ) {
                let cell = self.pin_cell(pin)?;
                if !cell.has_text() || SELECT_ALL_WHITESPACE.contains(&cell.codepoint()) {
                    continue;
                }

                result = Some(pin);
                break;
            }
            result?
        };

        let end = {
            let mut result = None;
            for pin in self.cell_iterator(
                Direction::LeftUp,
                point::Point::screen(Coordinate::new(0, 0)),
                None,
            ) {
                let cell = self.pin_cell(pin)?;
                if !cell.has_text() || SELECT_ALL_WHITESPACE.contains(&cell.codepoint()) {
                    continue;
                }

                result = Some(pin);
                break;
            }
            result?
        };

        Some(selection::Selection::new(start, end, false))
    }

    fn select_output(&self, pin: Pin) -> Option<selection::Selection> {
        if pin.garbage || !self.pin_is_valid(&pin) {
            return None;
        }
        if self.pin_cell(pin)?.semantic_content() != SemanticContent::Output {
            return None;
        }

        let prompt_pin = {
            let mut prompts = self.prompt_iterator_from_pin(Direction::LeftUp, pin, None);
            prompts.next()
        };

        if let Some(prompt_pin) = prompt_pin {
            let highlight = self.highlight_semantic_content(prompt_pin, SemanticContent::Output)?;
            return Some(selection::Selection::new(
                highlight.start,
                highlight.end,
                false,
            ));
        }

        let mut prompts = self.prompt_iterator_from_pin(Direction::RightDown, pin, None);
        let next_prompt = prompts.next()?;
        let mut end = self.pin_up(next_prompt, 1)?;
        end.x = self.node_for_pin(&end)?.page.size_cols() - 1;
        let start = self.get_top_left(point::Tag::Screen);

        let mut trimmed_end = None;
        for candidate in self.cell_iterator_from_pin(Direction::LeftUp, end, Some(start)) {
            let cell = self.pin_cell(candidate)?;
            if cell.has_text() {
                trimmed_end = Some(candidate);
                break;
            }
        }

        Some(selection::Selection::new(start, trimmed_end?, false))
    }

    fn select_line(&self, options: SelectLineOptions<'_>) -> Option<selection::Selection> {
        let pin = options.pin;
        if pin.garbage || !self.pin_is_valid(&pin) {
            return None;
        }

        let semantic_state = options
            .semantic_prompt_boundary
            .then(|| self.pin_cell(pin).map(|cell| cell.semantic_content()))
            .flatten();

        let start_pin = {
            let mut rows = self.row_iterator_from_pin(Direction::LeftUp, pin, None);
            let mut previous = rows.next()?;
            let mut start = previous;
            let mut found_start = false;

            if let Some(semantic) = semantic_state {
                let node = self.node_for_pin(&previous)?;
                let row = node.page.get_row(previous.y as usize);
                let cells = node.page.get_cells(row);
                for offset in 0..=pin.x {
                    let x = pin.x - offset;
                    if cells[x as usize].semantic_content() != semantic {
                        let mut result = previous;
                        result.x = x + 1;
                        start = result;
                        found_start = true;
                        break;
                    }
                }
            }

            if !found_start {
                let mut stopped_at_boundary = false;
                'rows: for mut row_pin in rows {
                    let node = self.node_for_pin(&row_pin)?;
                    let row = node.page.get_row(row_pin.y as usize);
                    if !row.wrap() {
                        start = previous;
                        start.x = 0;
                        stopped_at_boundary = true;
                        break;
                    }

                    if let Some(semantic) = semantic_state {
                        let cells = node.page.get_cells(row);
                        for x in (0..cells.len()).rev() {
                            if cells[x].semantic_content() != semantic {
                                start = previous;
                                stopped_at_boundary = true;
                                break 'rows;
                            }

                            row_pin.x = x.try_into().expect("row cell index must fit CellCountInt");
                            previous = row_pin;
                            start = row_pin;
                        }

                        continue;
                    }

                    previous = row_pin;
                    start = row_pin;
                }

                if !stopped_at_boundary {
                    start.x = 0;
                }
            }

            start
        };

        let end_pin = {
            let rows = self.row_iterator_from_pin(Direction::RightDown, pin, None);
            let mut end = None;
            for mut row_pin in rows {
                let node = self.node_for_pin(&row_pin)?;
                let row = node.page.get_row(row_pin.y as usize);
                let cells = node.page.get_cells(row);

                if let Some(semantic) = semantic_state {
                    let same_row = row_pin.node == pin.node && row_pin.y == pin.y;
                    let start_offset = if same_row { pin.x as usize } else { 0 };

                    if start_offset == 0 && cells[0].semantic_content() != semantic {
                        let mut previous_row = self.pin_up(row_pin, 1)?;
                        previous_row.x = self.node_for_pin(&previous_row)?.page.size_cols() - 1;
                        end = Some(previous_row);
                        break;
                    }

                    for (x, cell) in cells.iter().enumerate().skip(start_offset) {
                        if cell.semantic_content() != semantic {
                            row_pin.x = (x - 1)
                                .try_into()
                                .expect("row cell index must fit CellCountInt");
                            end = Some(row_pin);
                            break;
                        }
                    }

                    if end.is_some() {
                        break;
                    }
                }

                if !row.wrap() {
                    row_pin.x = node.page.size_cols() - 1;
                    end = Some(row_pin);
                    break;
                }
            }

            end?
        };

        self.select_line_trimmed(start_pin, end_pin, options.whitespace)
    }

    fn select_line_trimmed(
        &self,
        start_pin: Pin,
        end_pin: Pin,
        whitespace: Option<&[u32]>,
    ) -> Option<selection::Selection> {
        let start = if let Some(whitespace) = whitespace {
            let mut result = None;
            for pin in self.cell_iterator_from_pin(Direction::RightDown, start_pin, Some(end_pin)) {
                if self.pin_before(end_pin, pin).unwrap_or(true) {
                    break;
                }

                let cell = self.pin_cell(pin)?;
                if !cell.has_text() || whitespace.contains(&cell.codepoint()) {
                    continue;
                }

                result = Some(pin);
                break;
            }
            result?
        } else {
            start_pin
        };

        let end = if let Some(whitespace) = whitespace {
            let mut result = None;
            for pin in self.cell_iterator_from_pin(Direction::LeftUp, end_pin, Some(start_pin)) {
                if self.pin_before(pin, start_pin).unwrap_or(true) {
                    break;
                }

                let cell = self.pin_cell(pin)?;
                if !cell.has_text() || whitespace.contains(&cell.codepoint()) {
                    continue;
                }

                result = Some(pin);
                break;
            }
            result?
        } else {
            end_pin
        };

        Some(selection::Selection::new(start, end, false))
    }

    fn select_word(&self, pin: Pin, boundary_codepoints: &[u32]) -> Option<selection::Selection> {
        if pin.garbage || !self.pin_is_valid(&pin) {
            return None;
        }

        let start_cell = self.pin_cell(pin)?;
        if !start_cell.has_text() {
            return None;
        }

        let expect_boundary = boundary_codepoints.contains(&start_cell.codepoint());

        let end = {
            let mut it = self.cell_iterator_from_pin(Direction::RightDown, pin, None);
            let mut prev = it.next()?;
            let mut end = prev;
            for next in it {
                let node = self.node_for_pin(&next)?;
                let row = node.page.get_row(next.y as usize);
                let cell = self.pin_cell(next)?;

                if !cell.has_text() {
                    end = prev;
                    break;
                }

                let this_boundary = boundary_codepoints.contains(&cell.codepoint());
                if this_boundary != expect_boundary {
                    end = prev;
                    break;
                }

                if next.x == node.page.size_cols() - 1 && !row.wrap() {
                    end = next;
                    break;
                }

                prev = next;
                end = next;
            }
            end
        };

        let start = {
            let mut it = self.cell_iterator_from_pin(Direction::LeftUp, pin, None);
            let mut prev = it.next()?;
            let mut start = prev;
            for next in it {
                let node = self.node_for_pin(&next)?;
                let row = node.page.get_row(next.y as usize);

                if next.x == node.page.size_cols() - 1 && !row.wrap() {
                    start = prev;
                    break;
                }

                let cell = self.pin_cell(next)?;
                if !cell.has_text() {
                    start = prev;
                    break;
                }

                let this_boundary = boundary_codepoints.contains(&cell.codepoint());
                if this_boundary != expect_boundary {
                    start = prev;
                    break;
                }

                prev = next;
                start = next;
            }
            start
        };

        Some(selection::Selection::new(start, end, false))
    }

    fn select_word_between(
        &self,
        start: Pin,
        end: Pin,
        boundary_codepoints: &[u32],
    ) -> Option<selection::Selection> {
        if start.garbage || end.garbage || !self.pin_is_valid(&start) || !self.pin_is_valid(&end) {
            return None;
        }

        let direction = if self.pin_before(start, end).unwrap_or(false) {
            Direction::RightDown
        } else {
            Direction::LeftUp
        };

        for pin in self.cell_iterator_from_pin(direction, start, Some(end)) {
            match direction {
                Direction::RightDown => {
                    if self.pin_before(end, pin).unwrap_or(true) {
                        return None;
                    }
                }
                Direction::LeftUp => {
                    if self.pin_before(pin, end).unwrap_or(true) {
                        return None;
                    }
                }
            }

            if let Some(selection) = self.select_word(pin, boundary_codepoints) {
                return Some(selection);
            }
        }

        None
    }

    fn drag_selection(
        &self,
        click_pin: Pin,
        drag_pin: Pin,
        click_x: u32,
        drag_x: u32,
        rectangle_selection: bool,
        geometry: DragGeometry,
    ) -> Option<selection::Selection> {
        if click_pin.garbage
            || drag_pin.garbage
            || !self.pin_is_valid(&click_pin)
            || !self.pin_is_valid(&drag_pin)
        {
            return None;
        }

        if geometry.columns == 0 || geometry.cell_width == 0 {
            return None;
        }
        let max_x = geometry
            .columns
            .checked_mul(geometry.cell_width)?
            .checked_sub(1)?;

        let threshold_point = ((geometry.cell_width as f64) * 0.6).round() as u32;
        let drag_x_frac =
            drag_x.saturating_sub(geometry.padding_left).min(max_x) % geometry.cell_width;
        let click_x_frac =
            click_x.saturating_sub(geometry.padding_left).min(max_x) % geometry.cell_width;
        let same_pin = drag_pin == click_pin;

        let end_before_start = if same_pin {
            drag_x_frac < click_x_frac
        } else if rectangle_selection {
            match drag_pin.x.cmp(&click_pin.x) {
                std::cmp::Ordering::Equal => drag_x_frac < click_x_frac,
                std::cmp::Ordering::Less => true,
                std::cmp::Ordering::Greater => false,
            }
        } else {
            self.pin_before(drag_pin, click_pin)?
        };

        let include_click_cell = if end_before_start {
            click_x_frac >= threshold_point
        } else {
            click_x_frac < threshold_point
        };
        let include_drag_cell = if end_before_start {
            drag_x_frac < threshold_point
        } else {
            drag_x_frac >= threshold_point
        };

        let start_pin = if include_click_cell {
            click_pin
        } else if end_before_start {
            if rectangle_selection {
                self.pin_left_clamp(click_pin, 1)?
            } else {
                self.pin_left_wrap(click_pin, 1).unwrap_or(click_pin)
            }
        } else if rectangle_selection {
            self.pin_right_clamp(click_pin, 1)?
        } else {
            self.pin_right_wrap(click_pin, 1).unwrap_or(click_pin)
        };

        let end_pin = if include_drag_cell {
            drag_pin
        } else if end_before_start {
            if rectangle_selection {
                self.pin_right_clamp(drag_pin, 1)?
            } else {
                self.pin_right_wrap(drag_pin, 1).unwrap_or(drag_pin)
            }
        } else if rectangle_selection {
            self.pin_left_clamp(drag_pin, 1)?
        } else {
            self.pin_left_wrap(drag_pin, 1).unwrap_or(drag_pin)
        };

        if (!include_click_cell && same_pin)
            || (!include_click_cell && rectangle_selection && click_pin.x == drag_pin.x)
            || (!include_click_cell && end_pin == click_pin)
            || (!include_click_cell && rectangle_selection && end_pin.x == click_pin.x)
            || (!include_drag_cell && start_pin == drag_pin)
            || (!include_drag_cell && rectangle_selection && start_pin.x == drag_pin.x)
        {
            return None;
        }

        Some(selection::Selection::new(
            start_pin,
            end_pin,
            rectangle_selection,
        ))
    }

    fn total_rows(&self) -> usize {
        self.pages
            .iter()
            .map(|node| node.page.size_rows() as usize)
            .sum()
    }

    fn active_top_left(&self) -> &Pin {
        &self.viewport_pin
    }
}

fn init_pages(
    serial: &mut u64,
    cols: CellCountInt,
    rows: CellCountInt,
) -> Result<(Vec<Box<Node>>, usize), PageListAllocError> {
    let capacity = initial_capacity(cols);
    let mut remaining_rows = rows as usize;
    let mut pages = Vec::new();
    let mut page_size = 0;

    while remaining_rows > 0 {
        let mut page = Page::init(capacity)?;
        let active_rows = remaining_rows.min(capacity.rows() as usize);
        page.set_size_rows(
            active_rows
                .try_into()
                .expect("active page row count must fit CellCountInt"),
        );
        remaining_rows -= active_rows;
        page_size += page.backing_len();

        pages.push(Box::new(Node {
            page,
            serial: *serial,
        }));
        *serial += 1;
    }

    debug_assert!(!pages.is_empty());
    Ok((pages, page_size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::page::{page_layout, Cell, HyperlinkSnapshot, HyperlinkSnapshotId, Wide};
    use crate::terminal::{color, hyperlink, selection, selection_codepoints, style};

    fn simulate_history(list: &mut PageList, total_rows: CellCountInt) {
        list.pages[0].page.set_size_rows(total_rows);
        list.total_rows = total_rows;
    }

    fn viewport_top_left_screen_coord(list: &PageList) -> Coordinate {
        let pin = list.get_top_left(point::Tag::Viewport);
        list.point_from_pin(point::Tag::Screen, pin)
            .expect("viewport top-left must map to screen")
            .coord()
    }

    fn active_top_left_screen_coord(list: &PageList) -> Coordinate {
        let pin = list.get_top_left(point::Tag::Active);
        list.point_from_pin(point::Tag::Screen, pin)
            .expect("active top-left must map to screen")
            .coord()
    }

    fn chunk_tuple(list: &PageList, chunk: PageChunk) -> (usize, CellCountInt, CellCountInt) {
        (
            list.node_index(chunk.node).expect("chunk node must exist"),
            chunk.start,
            chunk.end,
        )
    }

    fn chunk_tuples(
        list: &PageList,
        iterator: PageIterator<'_>,
    ) -> Vec<(usize, CellCountInt, CellCountInt)> {
        iterator.map(|chunk| chunk_tuple(list, chunk)).collect()
    }

    fn row_tuple(list: &PageList, pin: Pin) -> (usize, CellCountInt, CellCountInt) {
        (
            list.node_index(pin.node).expect("row node must exist"),
            pin.y,
            pin.x,
        )
    }

    fn screen_pin(list: &PageList, x: CellCountInt, y: u32) -> Pin {
        list.pin(point::Point::screen(Coordinate::new(x, y)))
            .expect("screen point must map to a pin")
    }

    fn screen_coord(list: &PageList, pin: Pin) -> Coordinate {
        list.point_from_pin(point::Tag::Screen, pin)
            .expect("pin must map to a screen point")
            .coord()
    }

    fn screen_selection(
        list: &PageList,
        start: (CellCountInt, u32),
        end: (CellCountInt, u32),
        rectangle: bool,
    ) -> selection::Selection {
        selection::Selection::new(
            screen_pin(list, start.0, start.1),
            screen_pin(list, end.0, end.1),
            rectangle,
        )
    }

    fn set_screen_cell(list: &mut PageList, x: CellCountInt, y: u32, codepoint: char) {
        let pin = screen_pin(list, x, y);
        let index = list.node_index(pin.node).expect("screen node must exist");
        *list.pages[index]
            .page
            .get_row_and_cell_mut(pin.x as usize, pin.y as usize)
            .cell = Cell::init(codepoint as u32);
    }

    fn set_screen_text_lines(list: &mut PageList, lines: &[&str]) {
        for (y, line) in lines.iter().enumerate() {
            for (x, ch) in line.chars().enumerate() {
                set_screen_cell(
                    list,
                    x.try_into().expect("test x must fit CellCountInt"),
                    y.try_into().expect("test y must fit u32"),
                    ch,
                );
            }
        }
    }

    fn set_screen_text_lines_at(list: &mut PageList, start_y: u32, lines: &[&str]) {
        for (offset_y, line) in lines.iter().enumerate() {
            let y = start_y + offset_y as u32;
            for (x, ch) in line.chars().enumerate() {
                set_screen_cell(
                    list,
                    x.try_into().expect("test x must fit CellCountInt"),
                    y,
                    ch,
                );
            }
        }
    }

    fn set_screen_row_wrap(list: &mut PageList, y: u32, wrap: bool) {
        let pin = screen_pin(list, 0, y);
        let index = list.node_index(pin.node).expect("screen node must exist");
        list.pages[index]
            .page
            .get_row_mut(pin.y as usize)
            .set_wrap(wrap);
    }

    fn set_screen_row_wrap_continuation(list: &mut PageList, y: u32, wrap: bool) {
        let pin = screen_pin(list, 0, y);
        let index = list.node_index(pin.node).expect("screen node must exist");
        list.pages[index]
            .page
            .get_row_mut(pin.y as usize)
            .set_wrap_continuation(wrap);
    }

    fn set_screen_cell_raw(list: &mut PageList, x: CellCountInt, y: u32, cell: Cell) {
        let pin = screen_pin(list, x, y);
        let index = list.node_index(pin.node).expect("screen node must exist");
        *list.pages[index]
            .page
            .get_row_and_cell_mut(pin.x as usize, pin.y as usize)
            .cell = cell;
    }

    fn append_screen_grapheme(list: &mut PageList, x: CellCountInt, y: u32, cp: u32) {
        let pin = screen_pin(list, x, y);
        let index = list.node_index(pin.node).expect("screen node must exist");
        list.pages[index]
            .page
            .append_grapheme_at(pin.x as usize, pin.y as usize, cp)
            .expect("test grapheme must fit page storage");
    }

    fn assert_selection_screen_points(
        list: &PageList,
        selection: selection::Selection,
        start: (CellCountInt, u32),
        end: (CellCountInt, u32),
    ) {
        assert_eq!(
            screen_coord(list, selection.start()),
            Coordinate::new(start.0, start.1)
        );
        assert_eq!(
            screen_coord(list, selection.end()),
            Coordinate::new(end.0, end.1)
        );
    }

    fn row_tuples(
        list: &PageList,
        iterator: RowIterator<'_>,
    ) -> Vec<(usize, CellCountInt, CellCountInt)> {
        iterator.map(|pin| row_tuple(list, pin)).collect()
    }

    fn cell_tuples(
        list: &PageList,
        iterator: CellIterator<'_>,
    ) -> Vec<(usize, CellCountInt, CellCountInt)> {
        iterator.map(|pin| row_tuple(list, pin)).collect()
    }

    fn flattened_chunk_tuples(
        list: &PageList,
        flattened: &highlight::Flattened,
    ) -> Vec<(usize, u64, CellCountInt, CellCountInt)> {
        flattened
            .chunks
            .iter()
            .map(|chunk| {
                (
                    list.node_index(chunk.node).expect("chunk node must exist"),
                    chunk.serial,
                    chunk.start,
                    chunk.end,
                )
            })
            .collect()
    }

    fn tracked_pin_value(pin: NonNull<Pin>) -> Pin {
        unsafe {
            // Safety: tests call this only while the tracked pin is still
            // owned by the PageList that allocated it.
            *pin.as_ref()
        }
    }

    fn clone_options(top: point::Point) -> CloneOptions<'static> {
        CloneOptions {
            top,
            bottom: None,
            tracked_pins: None,
        }
    }

    fn drag_geometry() -> DragGeometry {
        DragGeometry {
            columns: 10,
            cell_width: 10,
            padding_left: 5,
            screen_height: 110,
        }
    }

    fn drag_x_pos(x: f64, geometry: DragGeometry) -> u32 {
        ((x * geometry.cell_width as f64).floor() as u32) + geometry.padding_left
    }

    fn assert_drag_selection(
        click: (f64, u32),
        drag: (f64, u32),
        expected_start: (CellCountInt, u32),
        expected_end: (CellCountInt, u32),
        rectangle: bool,
    ) {
        let list = PageList::init(10, 5, None).unwrap();
        let geometry = drag_geometry();
        let click_pin = screen_pin(&list, click.0.floor() as CellCountInt, click.1);
        let drag_pin = screen_pin(&list, drag.0.floor() as CellCountInt, drag.1);
        let expected = screen_selection(&list, expected_start, expected_end, rectangle);

        assert_eq!(
            list.drag_selection(
                click_pin,
                drag_pin,
                drag_x_pos(click.0, geometry),
                drag_x_pos(drag.0, geometry),
                rectangle,
                geometry,
            ),
            Some(expected)
        );
    }

    fn assert_drag_selection_is_none(click: (f64, u32), drag: (f64, u32), rectangle: bool) {
        let list = PageList::init(10, 5, None).unwrap();
        let geometry = drag_geometry();
        let click_pin = screen_pin(&list, click.0.floor() as CellCountInt, click.1);
        let drag_pin = screen_pin(&list, drag.0.floor() as CellCountInt, drag.1);

        assert_eq!(
            list.drag_selection(
                click_pin,
                drag_pin,
                drag_x_pos(click.0, geometry),
                drag_x_pos(drag.0, geometry),
                rectangle,
                geometry,
            ),
            None
        );
    }

    fn assert_word_selection(
        list: &PageList,
        pin: (CellCountInt, u32),
        start: (CellCountInt, u32),
        end: (CellCountInt, u32),
    ) {
        let selection = list
            .select_word(
                screen_pin(list, pin.0, pin.1),
                selection_codepoints::DEFAULT_WORD_BOUNDARIES,
            )
            .expect("word selection must exist");
        assert!(!selection.rectangle());
        assert_selection_screen_points(list, selection, start, end);
    }

    fn assert_word_between_selection(
        list: &PageList,
        scan_start: (CellCountInt, u32),
        scan_end: (CellCountInt, u32),
        start: (CellCountInt, u32),
        end: (CellCountInt, u32),
    ) {
        let selection = list
            .select_word_between(
                screen_pin(list, scan_start.0, scan_start.1),
                screen_pin(list, scan_end.0, scan_end.1),
                selection_codepoints::DEFAULT_WORD_BOUNDARIES,
            )
            .expect("word-between selection must exist");
        assert!(!selection.rectangle());
        assert_selection_screen_points(list, selection, start, end);
    }

    fn assert_line_selection(
        list: &PageList,
        pin: (CellCountInt, u32),
        start: (CellCountInt, u32),
        end: (CellCountInt, u32),
    ) {
        let selection = list
            .select_line(SelectLineOptions::new(screen_pin(list, pin.0, pin.1)))
            .expect("line selection must exist");
        assert!(!selection.is_tracked());
        assert!(!selection.rectangle());
        assert_selection_screen_points(list, selection, start, end);
    }

    fn assert_line_selection_with_options(
        list: &PageList,
        options: SelectLineOptions<'_>,
        start: (CellCountInt, u32),
        end: (CellCountInt, u32),
    ) {
        let selection = list
            .select_line(options)
            .expect("line selection must exist");
        assert!(!selection.is_tracked());
        assert!(!selection.rectangle());
        assert_selection_screen_points(list, selection, start, end);
    }

    fn assert_select_all(list: &PageList, start: (CellCountInt, u32), end: (CellCountInt, u32)) {
        let selection = list.select_all().expect("select all must exist");
        assert!(!selection.is_tracked());
        assert!(!selection.rectangle());
        assert_selection_screen_points(list, selection, start, end);
    }

    fn assert_select_output(
        list: &PageList,
        pin: (CellCountInt, u32),
        start: (CellCountInt, u32),
        end: (CellCountInt, u32),
    ) {
        let selection = list
            .select_output(screen_pin(list, pin.0, pin.1))
            .expect("output selection must exist");
        assert!(!selection.is_tracked());
        assert!(!selection.rectangle());
        assert_selection_screen_points(list, selection, start, end);
    }

    fn assert_line_iterator(
        list: &PageList,
        start: Pin,
        expected: &[(CellCountInt, u32, CellCountInt, u32)],
    ) {
        let mut iter = list.line_iterator(start);
        for &(start_x, start_y, end_x, end_y) in expected {
            let selection = iter.next().expect("line iterator selection must exist");
            assert!(!selection.is_tracked());
            assert!(!selection.rectangle());
            assert_selection_screen_points(list, selection, (start_x, start_y), (end_x, end_y));
        }
        assert!(iter.next().is_none());
    }

    fn assert_selection_string(
        list: &PageList,
        start: (CellCountInt, u32),
        end: (CellCountInt, u32),
        rectangle: bool,
        trim: bool,
        expected: &str,
    ) {
        let actual = list.selection_string(SelectionStringOptions {
            selection: Some(screen_selection(list, start, end, rectangle)),
            trim,
        });
        assert_eq!(actual, expected);
    }

    fn assert_dump_string(
        list: &PageList,
        start: (CellCountInt, u32),
        end: Option<(CellCountInt, u32)>,
        unwrap: bool,
        expected: &str,
    ) {
        let actual = list.dump_string(
            screen_pin(list, start.0, start.1),
            end.map(|(x, y)| screen_pin(list, x, y)),
            unwrap,
        );
        assert_eq!(actual, expected);
    }

    fn assert_page_string(
        list: &PageList,
        start: (CellCountInt, u32),
        end: (CellCountInt, u32),
        emit: PageOutputFormat,
        palette: Option<&color::Palette>,
        expected: &str,
    ) {
        let actual = list.page_string(PageStringOptions {
            selection: Some(screen_selection(list, start, end, false)),
            trim: true,
            unwrap: true,
            emit,
            palette,
            codepoint_map: None,
        });
        assert_eq!(actual, expected);
    }

    fn assert_page_string_with_map(
        list: &PageList,
        start: (CellCountInt, u32),
        end: (CellCountInt, u32),
        emit: PageOutputFormat,
        trim: bool,
        codepoint_map: &[CodepointMapEntry],
        expected: &str,
    ) {
        let actual = list.page_string(PageStringOptions {
            selection: Some(screen_selection(list, start, end, false)),
            trim,
            unwrap: true,
            emit,
            palette: None,
            codepoint_map: Some(codepoint_map),
        });
        assert_eq!(actual, expected);
    }

    fn assert_plain_point_map(
        list: &PageList,
        start: (CellCountInt, u32),
        end: (CellCountInt, u32),
        trim: bool,
        unwrap: bool,
        codepoint_map: Option<&[CodepointMapEntry]>,
        expected_text: &str,
        expected_points: &[point::Coordinate],
    ) {
        let actual = list.plain_string_with_point_map(PlainStringWithMapOptions {
            selection: Some(screen_selection(list, start, end, false)),
            trim,
            unwrap,
            codepoint_map,
        });
        assert_eq!(actual.text, expected_text);
        assert_eq!(actual.point_map, expected_points);
        assert_eq!(actual.text.len(), actual.point_map.len());
    }

    fn assert_plain_point_map_for_selection(
        list: &PageList,
        selection: Option<selection::Selection>,
        trim: bool,
        unwrap: bool,
        expected_text: &str,
        expected_points: &[point::Coordinate],
    ) {
        let actual = list.plain_string_with_point_map(PlainStringWithMapOptions {
            selection,
            trim,
            unwrap,
            codepoint_map: None,
        });
        assert_eq!(actual.text, expected_text);
        assert_eq!(actual.point_map, expected_points);
        assert_eq!(actual.text.len(), actual.point_map.len());
    }

    fn assert_vt_point_map(
        list: &PageList,
        start: (CellCountInt, u32),
        end: (CellCountInt, u32),
        trim: bool,
        unwrap: bool,
        codepoint_map: Option<&[CodepointMapEntry]>,
        expected_text: &str,
        expected_points: &[point::Coordinate],
    ) {
        let mut point_map = Vec::new();
        let actual = list.page_string_with_point_map(
            PageStringOptions {
                selection: Some(screen_selection(list, start, end, false)),
                trim,
                unwrap,
                emit: PageOutputFormat::Vt,
                palette: None,
                codepoint_map,
            },
            &mut point_map,
        );
        assert_eq!(actual, expected_text);
        assert_eq!(point_map, expected_points);
        assert_eq!(actual.len(), point_map.len());
    }

    fn assert_vt_point_map_for_selection(
        list: &PageList,
        selection: Option<selection::Selection>,
        trim: bool,
        unwrap: bool,
        expected_text: &str,
        expected_points: &[point::Coordinate],
    ) {
        let mut point_map = Vec::new();
        let actual = list.page_string_with_point_map(
            PageStringOptions {
                selection,
                trim,
                unwrap,
                emit: PageOutputFormat::Vt,
                palette: None,
                codepoint_map: None,
            },
            &mut point_map,
        );
        assert_eq!(actual, expected_text);
        assert_eq!(point_map, expected_points);
        assert_eq!(actual.len(), point_map.len());
    }

    fn assert_plain_pin_map(
        list: &PageList,
        start: (CellCountInt, u32),
        end: (CellCountInt, u32),
        trim: bool,
        unwrap: bool,
        codepoint_map: Option<&[CodepointMapEntry]>,
        expected_text: &str,
        expected_pins: &[Pin],
    ) {
        let actual = list.plain_string_with_pin_map(PlainStringWithMapOptions {
            selection: Some(screen_selection(list, start, end, false)),
            trim,
            unwrap,
            codepoint_map,
        });
        assert_eq!(actual.text, expected_text);
        assert_eq!(actual.pin_map, expected_pins);
        assert_eq!(actual.text.len(), actual.pin_map.len());
    }

    fn assert_plain_pin_map_for_selection(
        list: &PageList,
        selection: Option<selection::Selection>,
        trim: bool,
        unwrap: bool,
        expected_text: &str,
        expected_pins: &[Pin],
    ) {
        let actual = list.plain_string_with_pin_map(PlainStringWithMapOptions {
            selection,
            trim,
            unwrap,
            codepoint_map: None,
        });
        assert_eq!(actual.text, expected_text);
        assert_eq!(actual.pin_map, expected_pins);
        assert_eq!(actual.text.len(), actual.pin_map.len());
    }

    fn coords(points: &[(CellCountInt, u32)]) -> Vec<point::Coordinate> {
        points
            .iter()
            .map(|&(x, y)| point::Coordinate::new(x, y))
            .collect()
    }

    fn repeat_coords(
        target: &mut Vec<point::Coordinate>,
        point: (CellCountInt, u32),
        count: usize,
    ) {
        target.extend(std::iter::repeat_n(
            point::Coordinate::new(point.0, point.1),
            count,
        ));
    }

    fn pins(list: &PageList, points: &[(CellCountInt, u32)]) -> Vec<Pin> {
        points
            .iter()
            .map(|&(x, y)| screen_pin(list, x, y))
            .collect()
    }

    fn codepoint_map_entry(
        start: char,
        end: char,
        replacement: CodepointReplacement,
    ) -> CodepointMapEntry {
        CodepointMapEntry::new(start as u32, end as u32, replacement)
            .expect("test codepoint range must be valid")
    }

    fn set_screen_styled_cell(
        list: &mut PageList,
        x: CellCountInt,
        y: u32,
        ch: char,
        cell_style: style::Style,
    ) {
        let pin = screen_pin(list, x, y);
        let index = list.node_index(pin.node).expect("screen node must exist");
        let page = &mut list.pages[index].page;
        let style_id = page.add_style(cell_style).unwrap();
        {
            let rac = page.get_row_and_cell_mut(pin.x as usize, pin.y as usize);
            rac.row.set_styled(true);
            *rac.cell = Cell::init(ch as u32);
            rac.cell.set_style_id(style_id);
        }
        page.use_style(style_id);
    }

    fn page_cell(page: &Page, x: usize, y: usize) -> Cell {
        page.get_cells(page.get_row(y))[x]
    }

    fn multi_page_list(rows: CellCountInt) -> (PageList, CellCountInt) {
        let mut capacity = STD_CAPACITY.adjust(CapacityAdjustment::cols(50)).unwrap();
        while capacity.rows() >= rows {
            capacity = STD_CAPACITY
                .adjust(CapacityAdjustment::cols(capacity.cols() + 50))
                .unwrap();
        }

        let list = PageList::init(capacity.cols(), rows, None).unwrap();
        assert!(list.pages.len() > 1);
        (list, capacity.rows())
    }

    fn fill_visible_cells(page: &mut Page, cols: CellCountInt, rows: CellCountInt) {
        for y in 0..rows as usize {
            for x in 0..cols as usize {
                *page.get_row_and_cell_mut(x, y).cell = Cell::init((x + y * cols as usize) as u32);
            }
        }
    }

    fn assert_visible_cells(page: &Page, cols: CellCountInt, rows: CellCountInt) {
        for y in 0..rows as usize {
            for x in 0..cols as usize {
                assert_eq!(
                    page_cell(page, x, y).codepoint(),
                    (x + y * cols as usize) as u32
                );
            }
        }
    }

    fn set_row_marker(page: &mut Page, y: usize, value: u32) {
        *page.get_row_and_cell_mut(0, y).cell = Cell::init(value);
    }

    fn row_marker(page: &Page, y: usize) -> u32 {
        page_cell(page, 0, y).codepoint()
    }

    fn set_active_row_marker(list: &mut PageList, y: CellCountInt, value: u32) {
        let pin = list
            .pin(point::Point::active(Coordinate::new(0, y as u32)))
            .expect("active row must exist");
        let index = list.node_index(pin.node).expect("active node must exist");
        set_row_marker(&mut list.pages[index].page, pin.y as usize, value);
    }

    fn set_active_cell(list: &mut PageList, y: CellCountInt, cell: Cell) {
        let pin = list
            .pin(point::Point::active(Coordinate::new(0, y as u32)))
            .expect("active row must exist");
        let index = list.node_index(pin.node).expect("active node must exist");
        *list.pages[index]
            .page
            .get_row_and_cell_mut(0, pin.y as usize)
            .cell = cell;
    }

    fn active_row_marker(list: &PageList, y: CellCountInt) -> u32 {
        let pin = list
            .pin(point::Point::active(Coordinate::new(0, y as u32)))
            .expect("active row must exist");
        let node = list.node_for_pin(&pin).expect("active node must exist");
        row_marker(&node.page, pin.y as usize)
    }

    fn set_screen_semantic_prompt(list: &mut PageList, y: u32, prompt: SemanticPrompt) {
        let pin = list
            .pin(point::Point::screen(Coordinate::new(0, y)))
            .expect("screen row must exist");
        let index = list.node_index(pin.node).expect("screen node must exist");
        list.pages[index]
            .page
            .get_row_mut(pin.y as usize)
            .set_semantic_prompt(prompt);
    }

    fn set_screen_cell_semantic(
        list: &mut PageList,
        x: CellCountInt,
        y: u32,
        codepoint: char,
        semantic: SemanticContent,
    ) {
        let pin = list
            .pin(point::Point::screen(Coordinate::new(x, y)))
            .expect("screen cell must exist");
        let index = list.node_index(pin.node).expect("screen node must exist");
        let mut cell = Cell::init(codepoint as u32);
        cell.set_semantic_content(semantic);
        *list.pages[index]
            .page
            .get_row_and_cell_mut(pin.x as usize, pin.y as usize)
            .cell = cell;
    }

    fn set_screen_blank_semantic(
        list: &mut PageList,
        x: CellCountInt,
        y: u32,
        semantic: SemanticContent,
    ) {
        let pin = list
            .pin(point::Point::screen(Coordinate::new(x, y)))
            .expect("screen cell must exist");
        let index = list.node_index(pin.node).expect("screen node must exist");
        let mut cell = Cell::init(0);
        cell.set_semantic_content(semantic);
        *list.pages[index]
            .page
            .get_row_and_cell_mut(pin.x as usize, pin.y as usize)
            .cell = cell;
    }

    fn set_screen_semantic_text(
        list: &mut PageList,
        x: CellCountInt,
        y: u32,
        text: &str,
        semantic: SemanticContent,
    ) {
        for (offset, ch) in text.chars().enumerate() {
            set_screen_cell_semantic(
                list,
                x + CellCountInt::try_from(offset).expect("test x offset must fit CellCountInt"),
                y,
                ch,
                semantic,
            );
        }
    }

    fn prompt_screen_points(list: &PageList, iterator: PromptIterator<'_>) -> Vec<Coordinate> {
        iterator
            .map(|pin| {
                assert_eq!(pin.x, 0);
                list.point_from_pin(point::Tag::Screen, pin)
                    .expect("prompt pin must map to screen point")
                    .coord()
            })
            .collect()
    }

    fn highlight_screen_points(
        list: &PageList,
        highlight: highlight::Untracked,
    ) -> [Coordinate; 2] {
        [
            list.point_from_pin(point::Tag::Screen, highlight.start)
                .expect("highlight start must map to screen point")
                .coord(),
            list.point_from_pin(point::Tag::Screen, highlight.end)
                .expect("highlight end must map to screen point")
                .coord(),
        ]
    }

    fn bounded_viewport_list(page_multiplier: usize) -> (PageList, usize) {
        let mut list = PageList::init(80, 24, None).unwrap();
        let page_rows = list
            .pages
            .last()
            .expect("list must have an initial page")
            .page
            .capacity()
            .rows() as usize;
        list.grow_rows(page_rows * page_multiplier).unwrap();
        (list, page_rows)
    }

    fn assert_integrity_after_caller_row_accounting(
        list: &mut PageList,
        deleted_rows: CellCountInt,
    ) {
        list.total_rows -= deleted_rows;
        list.verify_integrity().unwrap();
        list.total_rows += deleted_rows;
    }

    fn replace_first_page_capacity(list: &mut PageList, capacity: Capacity) {
        let old_len = list.pages[0].page.backing_len();
        let mut page = Page::init(capacity).unwrap();
        page.set_size_rows(list.rows);
        list.pages[0].page = page;
        list.page_size = list.page_size - old_len + list.pages[0].page.backing_len();
        list.verify_integrity().unwrap();
    }

    fn make_first_page_oversized(list: &mut PageList) -> NonNull<Node> {
        let mut node = list.first_node_ptr();
        while list.node_for_ptr(node).unwrap().page.backing_len() <= standard_page_size() {
            node = list
                .increase_capacity(node, Some(IncreaseCapacity::GraphemeBytes))
                .unwrap();
        }
        node
    }

    #[test]
    fn viewport_variants_compare_as_expected() {
        assert_eq!(Viewport::Active, Viewport::Active);
        assert_eq!(Viewport::Top, Viewport::Top);
        assert_eq!(Viewport::Pin, Viewport::Pin);
        assert_ne!(Viewport::Active, Viewport::Top);
        assert_ne!(Viewport::Active, Viewport::Pin);
        assert_ne!(Viewport::Top, Viewport::Pin);
    }

    #[test]
    fn initial_capacity_normal_width_preserves_standard_size() {
        let standard_size = standard_page_size();
        let capacity = initial_capacity(80);

        assert_eq!(capacity.cols(), 80);
        assert!(capacity.rows() >= 1);
        assert_eq!(page_layout(capacity).total_size(), standard_size);
    }

    #[test]
    fn initial_capacity_max_standard_width_preserves_standard_size() {
        let standard_size = standard_page_size();
        let max_cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row");
        let capacity = initial_capacity(max_cols);

        assert_eq!(capacity.cols(), max_cols);
        assert_eq!(capacity.rows(), 1);
        assert_eq!(page_layout(capacity).total_size(), standard_size);
    }

    #[test]
    fn initial_capacity_too_wide_uses_non_standard_page() {
        let standard_size = standard_page_size();
        let max_cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row");
        assert!(max_cols < CellCountInt::MAX);
        let requested_cols = max_cols + 1;
        let capacity = initial_capacity(requested_cols);

        assert_eq!(capacity.cols(), requested_cols);
        assert_eq!(capacity.rows(), STD_CAPACITY.rows());
        assert!(page_layout(capacity).total_size() > standard_size);
    }

    #[test]
    fn initial_capacity_max_columns_lays_out() {
        let capacity = initial_capacity(CellCountInt::MAX);
        let layout = page_layout(capacity);

        assert_eq!(capacity.cols(), CellCountInt::MAX);
        assert!(capacity.rows() >= 1);
        assert!(layout.total_size() >= standard_page_size());
    }

    #[test]
    fn min_max_size_normal_dimensions_are_two_standard_pages() {
        assert_eq!(min_max_size(80, 24), standard_page_size() * 2);
    }

    #[test]
    fn min_max_size_adds_extra_page_for_multi_page_active_area() {
        let cols = 80;
        let capacity = initial_capacity(cols);
        let rows = capacity.rows() + 1;
        let expected_pages = (rows as usize).div_ceil(capacity.rows() as usize) + 1;

        assert!(expected_pages > 2);
        assert_eq!(
            min_max_size(cols, rows),
            standard_page_size() * expected_pages
        );
    }

    #[test]
    fn page_list_max_size_uses_min_when_explicit_is_smaller() {
        let list = PageList::init(80, 24, Some(1)).unwrap();

        assert_eq!(list.max_size(), list.min_max_size);
    }

    #[test]
    fn page_list_max_size_uses_explicit_when_larger() {
        let explicit = min_max_size(80, 24) + 1024;
        let list = PageList::init(80, 24, Some(explicit)).unwrap();

        assert_eq!(list.max_size(), explicit);
    }

    #[test]
    fn page_list_create_page_starts_with_zero_rows() {
        let mut list = PageList::init(80, 24, None).unwrap();
        let serial = list.page_serial;
        let page_size = list.page_size;

        let node = list.create_page(initial_capacity(80)).unwrap();

        assert_eq!(node.page.size_rows(), 0);
        assert_eq!(node.serial, serial);
        assert_eq!(list.page_serial, serial + 1);
        assert_eq!(list.page_size, page_size + node.page.backing_len());
    }

    #[test]
    fn page_list_increase_capacity_styles_preserves_cells() {
        let mut list = PageList::init(2, 2, Some(0)).unwrap();
        fill_visible_cells(&mut list.pages[0].page, 2, 2);
        let old_capacity = list.pages[0].page.capacity();

        let new_node = list
            .increase_capacity(list.first_node_ptr(), Some(IncreaseCapacity::Styles))
            .unwrap();
        let new_page = &list.node_for_ptr(new_node).unwrap().page;

        assert_eq!(new_page.capacity().styles(), old_capacity.styles() * 2);
        assert_visible_cells(new_page, 2, 2);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_increase_capacity_grapheme_bytes_preserves_cells() {
        let mut list = PageList::init(2, 2, Some(0)).unwrap();
        fill_visible_cells(&mut list.pages[0].page, 2, 2);
        let old_capacity = list.pages[0].page.capacity();

        let new_node = list
            .increase_capacity(list.first_node_ptr(), Some(IncreaseCapacity::GraphemeBytes))
            .unwrap();
        let new_page = &list.node_for_ptr(new_node).unwrap().page;

        assert_eq!(
            new_page.capacity().grapheme_bytes(),
            old_capacity.grapheme_bytes() * 2
        );
        assert_visible_cells(new_page, 2, 2);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_increase_capacity_hyperlink_bytes_preserves_cells() {
        let mut list = PageList::init(2, 2, Some(0)).unwrap();
        fill_visible_cells(&mut list.pages[0].page, 2, 2);
        let old_capacity = list.pages[0].page.capacity();

        let new_node = list
            .increase_capacity(
                list.first_node_ptr(),
                Some(IncreaseCapacity::HyperlinkBytes),
            )
            .unwrap();
        let new_page = &list.node_for_ptr(new_node).unwrap().page;

        assert_eq!(
            new_page.capacity().hyperlink_bytes(),
            old_capacity.hyperlink_bytes() * 2
        );
        assert_visible_cells(new_page, 2, 2);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_increase_capacity_string_bytes_preserves_cells() {
        let mut list = PageList::init(2, 2, Some(0)).unwrap();
        fill_visible_cells(&mut list.pages[0].page, 2, 2);
        let old_capacity = list.pages[0].page.capacity();

        let new_node = list
            .increase_capacity(list.first_node_ptr(), Some(IncreaseCapacity::StringBytes))
            .unwrap();
        let new_page = &list.node_for_ptr(new_node).unwrap().page;

        assert_eq!(
            new_page.capacity().string_bytes(),
            old_capacity.string_bytes() * 2
        );
        assert_visible_cells(new_page, 2, 2);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_increase_capacity_none_reclones_same_capacity() {
        let mut list = PageList::init(2, 2, Some(0)).unwrap();
        fill_visible_cells(&mut list.pages[0].page, 2, 2);
        let old_node = list.first_node_ptr();
        let old_capacity = list.pages[0].page.capacity();

        let new_node = list.increase_capacity(old_node, None).unwrap();
        let new_page = &list.node_for_ptr(new_node).unwrap().page;

        assert_ne!(new_node, old_node);
        assert_eq!(new_page.capacity(), old_capacity);
        assert_visible_cells(new_page, 2, 2);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_increase_capacity_preserves_managed_memory() {
        let mut list = PageList::init(3, 2, Some(0)).unwrap();
        let page = &mut list.pages[0].page;
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let style_id = page.add_style(bold).unwrap();
        let link_id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"cap"),
                uri: b"https://example.com/cap",
            })
            .unwrap();

        {
            let rac = page.get_row_and_cell_mut(0, 0);
            rac.row.set_styled(true);
            let mut cell = Cell::init('s' as u32);
            cell.set_style_id(style_id);
            *rac.cell = cell;
        }
        page.use_style(style_id);
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('g' as u32);
        page.append_grapheme_at(1, 0, 0x0301).unwrap();
        *page.get_row_and_cell_mut(2, 0).cell = Cell::init('h' as u32);
        page.set_hyperlink(2, 0, link_id).unwrap();

        let new_node = list
            .increase_capacity(list.first_node_ptr(), Some(IncreaseCapacity::Styles))
            .unwrap();
        let new_page = &list.node_for_ptr(new_node).unwrap().page;
        let cloned_style_id = page_cell(new_page, 0, 0).style_id();
        let cloned_link_id = new_page.lookup_hyperlink_at(2, 0).unwrap();

        assert_eq!(new_page.style_count(), 1);
        assert_eq!(new_page.get_style(cloned_style_id), bold);
        assert_eq!(new_page.lookup_grapheme_at(1, 0).unwrap(), vec![0x0301]);
        assert_eq!(
            new_page.get_hyperlink(cloned_link_id),
            HyperlinkSnapshot {
                id: HyperlinkSnapshotId::Explicit(b"cap".to_vec()),
                uri: b"https://example.com/cap".to_vec(),
            }
        );
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_increase_capacity_remaps_tracked_pins() {
        let mut list = PageList::init(2, 2, Some(0)).unwrap();
        let tracked = list
            .track_pin(
                list.pin(point::Point::active(Coordinate::new(1, 1)))
                    .unwrap(),
            )
            .unwrap();
        let old_node = list.first_node_ptr();

        let new_node = list
            .increase_capacity(old_node, Some(IncreaseCapacity::Styles))
            .unwrap();
        let pin = unsafe {
            // Safety: tracked remains owned by list and remains tracked.
            tracked.as_ref()
        };

        assert_eq!(pin.node, new_node);
        assert_eq!(pin.x, 1);
        assert_eq!(pin.y, 1);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_increase_capacity_remaps_viewport_pin() {
        let mut list = PageList::init(2, 2, Some(0)).unwrap();
        let old_node = list.viewport_pin.node;

        let new_node = list
            .increase_capacity(old_node, Some(IncreaseCapacity::Styles))
            .unwrap();

        assert_eq!(list.viewport_pin.node, new_node);
        assert_eq!(list.tracked_pins[0], NonNull::from(&*list.viewport_pin));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_increase_capacity_out_of_space_preserves_list() {
        let mut list = PageList::init(2, 2, Some(0)).unwrap();
        let cap = Capacity::with_metadata(
            2,
            2,
            StyleCountInt::MAX,
            list.pages[0].page.capacity().hyperlink_bytes(),
            list.pages[0].page.capacity().grapheme_bytes(),
            list.pages[0].page.capacity().string_bytes(),
        );
        replace_first_page_capacity(&mut list, cap);
        let page_size = list.page_size;
        let page_serial = list.page_serial;
        let node = list.first_node_ptr();
        let node_serial = list.pages[0].serial;

        assert_eq!(
            list.increase_capacity(node, Some(IncreaseCapacity::Styles)),
            Err(IncreaseCapacityError::OutOfSpace)
        );

        assert_eq!(list.first_node_ptr(), node);
        assert_eq!(list.pages[0].serial, node_serial);
        assert_eq!(list.page_size, page_size);
        assert_eq!(list.page_serial, page_serial);
        assert_eq!(list.pages[0].page.capacity(), cap);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_increase_capacity_saturates_final_overflow_then_reports_out_of_space() {
        let mut list = PageList::init(2, 2, Some(0)).unwrap();

        loop {
            let node = list.first_node_ptr();
            let before = list.pages[0].page.capacity().styles();
            let result = list.increase_capacity(node, Some(IncreaseCapacity::Styles));
            if before == StyleCountInt::MAX {
                assert_eq!(result, Err(IncreaseCapacityError::OutOfSpace));
                break;
            }

            let new_node = result.unwrap();
            let after = list
                .node_for_ptr(new_node)
                .unwrap()
                .page
                .capacity()
                .styles();
            let expected = before.checked_mul(2).unwrap_or(StyleCountInt::MAX);
            assert_eq!(after, expected);
        }

        assert_eq!(list.pages[0].page.capacity().styles(), StyleCountInt::MAX);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_increase_capacity_multi_page_preserves_order() {
        let (mut list, _) = multi_page_list(100);
        let first = list.first_node_ptr();
        let second = NonNull::from(list.pages[1].as_ref());
        let second_capacity = list.pages[1].page.capacity();
        let first_styles = list.pages[0].page.capacity().styles();

        let new_first = list
            .increase_capacity(first, Some(IncreaseCapacity::Styles))
            .unwrap();

        assert_eq!(list.first_node_ptr(), new_first);
        assert_eq!(NonNull::from(list.pages[1].as_ref()), second);
        assert_eq!(list.pages[0].page.capacity().styles(), first_styles * 2);
        assert_eq!(list.pages[1].page.capacity(), second_capacity);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_increase_capacity_preserves_dirty_flags() {
        let mut list = PageList::init(2, 4, Some(0)).unwrap();
        list.pages[0].page.set_dirty(true);
        list.pages[0].page.get_row_mut(0).set_dirty(true);
        list.pages[0].page.get_row_mut(2).set_dirty(true);

        let new_node = list
            .increase_capacity(list.first_node_ptr(), Some(IncreaseCapacity::Styles))
            .unwrap();
        let page = &list.node_for_ptr(new_node).unwrap().page;

        assert!(page.is_dirty());
        assert!(page.get_row(0).dirty());
        assert!(!page.get_row(1).dirty());
        assert!(page.get_row(2).dirty());
        assert!(!page.get_row(3).dirty());
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_compact_standard_page_returns_none() {
        let mut list = PageList::init(80, 24, Some(0)).unwrap();
        let node = list.first_node_ptr();
        let page_size = list.page_size;
        let page_serial = list.page_serial;
        let node_serial = list.pages[0].serial;

        let result = list.compact(node).unwrap();

        assert_eq!(result, None);
        assert_eq!(list.first_node_ptr(), node);
        assert_eq!(list.pages[0].serial, node_serial);
        assert_eq!(list.page_size, page_size);
        assert_eq!(list.page_serial, page_serial);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_compact_oversized_page() {
        let mut list = PageList::init(80, 24, Some(0)).unwrap();
        let node = make_first_page_oversized(&mut list);
        fill_visible_cells(&mut list.pages[0].page, 80, 24);
        list.pages[0].page.set_dirty(true);
        list.pages[0].page.get_row_mut(0).set_dirty(true);
        list.pages[0].page.get_row_mut(2).set_dirty(true);
        let tracked = list
            .track_pin(Pin {
                node,
                x: 5,
                y: 10,
                garbage: false,
            })
            .unwrap();
        let oversized_len = list.pages[0].page.backing_len();
        let page_size = list.page_size;

        let compacted = list.compact(node).unwrap().unwrap();
        let page = &list.node_for_ptr(compacted).unwrap().page;
        let tracked_pin = unsafe {
            // Safety: tracked remains owned by list and remains tracked.
            tracked.as_ref()
        };

        assert!(page.backing_len() < oversized_len);
        assert_eq!(
            list.page_size,
            page_size - oversized_len + page.backing_len()
        );
        assert_eq!(list.first_node_ptr(), compacted);
        assert_eq!(page.size_rows(), 24);
        assert_eq!(page.size_cols(), 80);
        assert_visible_cells(page, 80, 24);
        assert!(page.is_dirty());
        assert!(page.get_row(0).dirty());
        assert!(!page.get_row(1).dirty());
        assert!(page.get_row(2).dirty());
        assert!(!page.get_row(3).dirty());
        assert_eq!(tracked_pin.node, compacted);
        assert_eq!(tracked_pin.x, 5);
        assert_eq!(tracked_pin.y, 10);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_compact_preserves_managed_memory_exactly() {
        let mut list = PageList::init(3, 2, Some(0)).unwrap();
        let node = make_first_page_oversized(&mut list);
        let page = &mut list.pages[0].page;
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let style_id = page.add_style(bold).unwrap();
        let link_id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"compact"),
                uri: b"https://example.com/compact",
            })
            .unwrap();

        {
            let rac = page.get_row_and_cell_mut(0, 0);
            rac.row.set_styled(true);
            let mut cell = Cell::init('s' as u32);
            cell.set_style_id(style_id);
            *rac.cell = cell;
        }
        page.use_style(style_id);
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('g' as u32);
        page.append_grapheme_at(1, 0, 0x0301).unwrap();
        *page.get_row_and_cell_mut(2, 0).cell = Cell::init('h' as u32);
        page.set_hyperlink(2, 0, link_id).unwrap();
        let exact_capacity = page.exact_row_capacity(0, page.size_rows() as usize);

        let compacted = list.compact(node).unwrap().unwrap();
        let page = &list.node_for_ptr(compacted).unwrap().page;
        let cloned_style_id = page_cell(page, 0, 0).style_id();
        let cloned_link_id = page.lookup_hyperlink_at(2, 0).unwrap();

        assert_eq!(page.capacity(), exact_capacity);
        assert_eq!(page.style_count(), 1);
        assert_eq!(page.get_style(cloned_style_id), bold);
        assert_eq!(page.lookup_grapheme_at(1, 0).unwrap(), vec![0x0301]);
        assert_eq!(
            page.get_hyperlink(cloned_link_id),
            HyperlinkSnapshot {
                id: HyperlinkSnapshotId::Explicit(b"compact".to_vec()),
                uri: b"https://example.com/compact".to_vec(),
            }
        );
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_compact_remaps_viewport_pin() {
        let mut list = PageList::init(80, 24, Some(0)).unwrap();
        let node = make_first_page_oversized(&mut list);
        assert_eq!(list.viewport_pin.node, node);

        let compacted = list.compact(node).unwrap().unwrap();

        assert_eq!(list.viewport_pin.node, compacted);
        assert_eq!(list.tracked_pins[0], NonNull::from(&*list.viewport_pin));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_compact_insufficient_savings_is_safe() {
        let mut list = PageList::init(80, 24, Some(0)).unwrap();
        let node = list
            .increase_capacity(list.first_node_ptr(), Some(IncreaseCapacity::GraphemeBytes))
            .unwrap();
        let old_len = list.node_for_ptr(node).unwrap().page.backing_len();

        let result = list.compact(node).unwrap();

        if let Some(compacted) = result {
            assert!(list.node_for_ptr(compacted).unwrap().page.backing_len() < old_len);
        } else {
            assert_eq!(list.first_node_ptr(), node);
        }
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_compact_multi_page_preserves_order() {
        let (mut list, _) = multi_page_list(100);
        let first = make_first_page_oversized(&mut list);
        let second_rows = list.pages[1].page.size_rows();
        fill_visible_cells(&mut list.pages[1].page, list.cols, second_rows);
        let second = NonNull::from(list.pages[1].as_ref());
        let second_capacity = list.pages[1].page.capacity();
        let second_backing = list.pages[1].page.backing_len();
        let second_serial = list.pages[1].serial;

        let compacted = list.compact(first).unwrap().unwrap();

        assert_eq!(list.first_node_ptr(), compacted);
        assert_eq!(NonNull::from(list.pages[1].as_ref()), second);
        assert_eq!(list.pages[1].page.capacity(), second_capacity);
        assert_eq!(list.pages[1].page.backing_len(), second_backing);
        assert_eq!(list.pages[1].serial, second_serial);
        assert_visible_cells(&list.pages[1].page, list.cols, second_rows);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_split_at_middle_row() {
        let mut list = PageList::init(10, 10, Some(0)).unwrap();
        fill_visible_cells(&mut list.pages[0].page, 10, 10);
        let node = list.first_node_ptr();

        list.split(Pin {
            node,
            y: 5,
            x: 0,
            garbage: false,
        })
        .unwrap();

        assert_eq!(list.pages.len(), 2);
        assert_eq!(list.pages[0].page.size_rows(), 5);
        assert_eq!(list.pages[1].page.size_rows(), 5);
        assert_visible_cells(&list.pages[0].page, 10, 5);
        for y in 0..5 {
            for x in 0..10 {
                assert_eq!(
                    page_cell(&list.pages[1].page, x, y).codepoint(),
                    (x + (y + 5) * 10) as u32
                );
            }
        }
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_split_at_row_zero_is_noop() {
        let mut list = PageList::init(10, 10, Some(0)).unwrap();
        fill_visible_cells(&mut list.pages[0].page, 10, 10);
        let node = list.first_node_ptr();
        let page_size = list.page_size;
        let page_serial = list.page_serial;
        let backing = list.pages[0].page.backing_len();

        list.split(Pin {
            node,
            y: 0,
            x: 0,
            garbage: false,
        })
        .unwrap();

        assert_eq!(list.pages.len(), 1);
        assert_eq!(list.first_node_ptr(), node);
        assert_eq!(list.page_size, page_size);
        assert_eq!(list.page_serial, page_serial);
        assert_eq!(list.pages[0].page.backing_len(), backing);
        assert_eq!(list.pages[0].page.size_rows(), 10);
        assert_visible_cells(&list.pages[0].page, 10, 10);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_split_at_last_row() {
        let mut list = PageList::init(10, 10, Some(0)).unwrap();
        fill_visible_cells(&mut list.pages[0].page, 10, 10);
        let node = list.first_node_ptr();

        list.split(Pin {
            node,
            y: 9,
            x: 0,
            garbage: false,
        })
        .unwrap();

        assert_eq!(list.pages.len(), 2);
        assert_eq!(list.pages[0].page.size_rows(), 9);
        assert_eq!(list.pages[1].page.size_rows(), 1);
        assert_eq!(page_cell(&list.pages[1].page, 0, 0).codepoint(), 90);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_split_single_row_returns_out_of_space() {
        let mut list = PageList::init(10, 1, Some(0)).unwrap();
        let node = list.first_node_ptr();
        let result = list.split(Pin {
            node,
            y: 0,
            x: 0,
            garbage: false,
        });

        assert_eq!(result, Err(SplitError::OutOfSpace));
        assert_eq!(list.pages.len(), 1);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_split_moves_tracked_pins() {
        let mut list = PageList::init(10, 10, Some(0)).unwrap();
        let node = list.first_node_ptr();
        let pin_before = list
            .track_pin(Pin {
                node,
                y: 1,
                x: 0,
                garbage: false,
            })
            .unwrap();
        let pin_at = list
            .track_pin(Pin {
                node,
                y: 5,
                x: 2,
                garbage: false,
            })
            .unwrap();
        let pin_after = list
            .track_pin(Pin {
                node,
                y: 7,
                x: 3,
                garbage: false,
            })
            .unwrap();

        list.split(Pin {
            node,
            y: 5,
            x: 0,
            garbage: false,
        })
        .unwrap();

        let first = list.first_node_ptr();
        let second = NonNull::from(list.pages[1].as_ref());
        let before = unsafe { pin_before.as_ref() };
        let at = unsafe { pin_at.as_ref() };
        let after = unsafe { pin_after.as_ref() };
        assert_eq!(before.node, first);
        assert_eq!(before.y, 1);
        assert_eq!(before.x, 0);
        assert_eq!(at.node, second);
        assert_eq!(at.y, 0);
        assert_eq!(at.x, 2);
        assert_eq!(after.node, second);
        assert_eq!(after.y, 2);
        assert_eq!(after.x, 3);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_split_remaps_viewport_pin() {
        let mut list = PageList::init(10, 10, Some(0)).unwrap();
        let node = list.first_node_ptr();
        list.viewport_pin.node = node;
        list.viewport_pin.y = 7;
        list.viewport_pin.x = 6;

        list.split(Pin {
            node,
            y: 5,
            x: 0,
            garbage: false,
        })
        .unwrap();

        assert_eq!(
            list.viewport_pin.node,
            NonNull::from(list.pages[1].as_ref())
        );
        assert_eq!(list.viewport_pin.y, 2);
        assert_eq!(list.viewport_pin.x, 6);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_split_middle_page_preserves_order() {
        let mut list = PageList::init(10, 12, Some(0)).unwrap();
        let first = list.first_node_ptr();
        list.split(Pin {
            node: first,
            y: 4,
            x: 0,
            garbage: false,
        })
        .unwrap();
        let middle = NonNull::from(list.pages[1].as_ref());

        list.split(Pin {
            node: middle,
            y: 4,
            x: 0,
            garbage: false,
        })
        .unwrap();

        assert_eq!(list.pages.len(), 3);
        assert_eq!(list.first_node_ptr(), first);
        assert_eq!(NonNull::from(list.pages[1].as_ref()), middle);
        assert_eq!(list.pages[0].page.size_rows(), 4);
        assert_eq!(list.pages[1].page.size_rows(), 4);
        assert_eq!(list.pages[2].page.size_rows(), 4);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_split_last_page_makes_new_page_last() {
        let mut list = PageList::init(10, 10, Some(0)).unwrap();
        let first = list.first_node_ptr();
        list.split(Pin {
            node: first,
            y: 5,
            x: 0,
            garbage: false,
        })
        .unwrap();
        let last_before = NonNull::from(list.pages[1].as_ref());

        list.split(Pin {
            node: last_before,
            y: 2,
            x: 0,
            garbage: false,
        })
        .unwrap();

        assert_eq!(list.pages.len(), 3);
        assert_eq!(NonNull::from(list.pages[1].as_ref()), last_before);
        assert_eq!(list.pages[1].page.size_rows(), 2);
        assert_eq!(list.pages[2].page.size_rows(), 3);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_split_first_page_keeps_original_first() {
        let mut list = PageList::init(10, 10, Some(0)).unwrap();
        let first = list.first_node_ptr();
        list.split(Pin {
            node: first,
            y: 5,
            x: 0,
            garbage: false,
        })
        .unwrap();
        let second = NonNull::from(list.pages[1].as_ref());

        list.split(Pin {
            node: first,
            y: 2,
            x: 0,
            garbage: false,
        })
        .unwrap();

        assert_eq!(list.first_node_ptr(), first);
        assert_ne!(NonNull::from(list.pages[1].as_ref()), second);
        assert_eq!(NonNull::from(list.pages[2].as_ref()), second);
        assert_eq!(list.pages[0].page.size_rows(), 2);
        assert_eq!(list.pages[1].page.size_rows(), 3);
        assert_eq!(list.pages[2].page.size_rows(), 5);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_split_preserves_wrap_and_dirty_flags() {
        let mut list = PageList::init(10, 10, Some(0)).unwrap();
        let node = list.first_node_ptr();
        list.pages[0].page.set_dirty(true);
        list.pages[0].page.get_row_mut(2).set_dirty(true);
        list.pages[0].page.get_row_mut(5).set_dirty(true);
        list.pages[0].page.get_row_mut(5).set_wrap(true);
        list.pages[0]
            .page
            .get_row_mut(6)
            .set_wrap_continuation(true);
        list.pages[0].page.get_row_mut(7).set_wrap(true);
        list.pages[0]
            .page
            .get_row_mut(7)
            .set_wrap_continuation(true);

        list.split(Pin {
            node,
            y: 5,
            x: 0,
            garbage: false,
        })
        .unwrap();

        assert!(list.pages[0].page.is_dirty());
        assert!(!list.pages[1].page.is_dirty());
        assert!(list.pages[0].page.get_row(2).dirty());
        assert!(list.pages[1].page.get_row(0).dirty());
        assert!(list.pages[1].page.get_row(0).wrap());
        assert!(!list.pages[1].page.get_row(0).wrap_continuation());
        assert!(!list.pages[1].page.get_row(1).wrap());
        assert!(list.pages[1].page.get_row(1).wrap_continuation());
        assert!(list.pages[1].page.get_row(2).wrap());
        assert!(list.pages[1].page.get_row(2).wrap_continuation());
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_split_preserves_styled_cells() {
        let mut list = PageList::init(10, 10, Some(0)).unwrap();
        let node = list.first_node_ptr();
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let style_id = list.pages[0].page.add_style(bold).unwrap();
        for y in 5..8 {
            let rac = list.pages[0].page.get_row_and_cell_mut(0, y);
            rac.row.set_styled(true);
            let mut cell = Cell::init('S' as u32);
            cell.set_style_id(style_id);
            *rac.cell = cell;
            list.pages[0].page.use_style(style_id);
        }
        list.pages[0].page.release_style(style_id);

        list.split(Pin {
            node,
            y: 5,
            x: 0,
            garbage: false,
        })
        .unwrap();

        assert_eq!(list.pages[0].page.style_count(), 0);
        assert_eq!(list.pages[1].page.style_count(), 1);
        for y in 0..3 {
            let cell = page_cell(&list.pages[1].page, 0, y);
            assert_eq!(cell.codepoint(), 'S' as u32);
            assert_eq!(list.pages[1].page.get_style(cell.style_id()), bold);
            assert!(list.pages[1].page.get_row(y).styled());
        }
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_split_preserves_graphemes() {
        let mut list = PageList::init(10, 10, Some(0)).unwrap();
        let node = list.first_node_ptr();
        *list.pages[0].page.get_row_and_cell_mut(0, 6).cell = Cell::init(0x1f468);
        list.pages[0].page.append_grapheme_at(0, 6, 0x200d).unwrap();
        list.pages[0]
            .page
            .append_grapheme_at(0, 6, 0x1f469)
            .unwrap();

        list.split(Pin {
            node,
            y: 5,
            x: 0,
            garbage: false,
        })
        .unwrap();

        assert_eq!(list.pages[0].page.grapheme_count(), 0);
        assert_eq!(list.pages[1].page.grapheme_count(), 1);
        assert_eq!(
            list.pages[1].page.lookup_grapheme_at(0, 1).unwrap(),
            vec![0x200d, 0x1f469]
        );
        assert!(list.pages[1].page.get_row(1).grapheme());
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_split_preserves_hyperlinks() {
        let mut list = PageList::init(10, 10, Some(0)).unwrap();
        let node = list.first_node_ptr();
        let link_id = list.pages[0]
            .page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(0),
                uri: b"https://example.com",
            })
            .unwrap();
        *list.pages[0].page.get_row_and_cell_mut(0, 7).cell = Cell::init('L' as u32);
        list.pages[0].page.set_hyperlink(0, 7, link_id).unwrap();

        list.split(Pin {
            node,
            y: 5,
            x: 0,
            garbage: false,
        })
        .unwrap();

        assert_eq!(list.pages[0].page.hyperlink_count(), 0);
        assert_eq!(list.pages[1].page.hyperlink_count(), 1);
        let cloned_link = list.pages[1].page.lookup_hyperlink_at(0, 2).unwrap();
        assert_eq!(
            list.pages[1].page.get_hyperlink(cloned_link),
            HyperlinkSnapshot {
                id: HyperlinkSnapshotId::Implicit(0),
                uri: b"https://example.com".to_vec(),
            }
        );
        assert!(list.pages[1].page.get_row(2).hyperlink());
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_split_preserves_accounting() {
        let mut list = PageList::init(10, 10, Some(0)).unwrap();
        let node = list.first_node_ptr();
        let page_size = list.page_size;
        let page_serial = list.page_serial;
        let backing = list.pages[0].page.backing_len();
        let capacity = list.pages[0].page.capacity();
        let total_rows = list.total_rows();

        list.split(Pin {
            node,
            y: 5,
            x: 0,
            garbage: false,
        })
        .unwrap();

        assert_eq!(list.page_size, page_size + backing);
        assert_eq!(list.page_serial, page_serial + 1);
        assert_eq!(list.total_rows(), total_rows);
        assert_eq!(list.pages[0].page.backing_len(), backing);
        assert_eq!(list.pages[1].page.capacity(), capacity);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_init() {
        let list = PageList::init(80, 24, None).unwrap();

        assert_eq!(list.cols, 80);
        assert_eq!(list.rows, 24);
        assert_eq!(list.viewport, Viewport::Active);
        assert!(!list.pages.is_empty());
        assert_eq!(list.total_rows(), 24);
        assert_eq!(list.total_rows, 24);
        assert_eq!(list.page_serial, list.pages.len() as u64);
        assert_eq!(list.page_serial_min, 0);
        assert_eq!(list.explicit_max_size, usize::MAX);
        assert_eq!(list.min_max_size, min_max_size(80, 24));
        assert_eq!(list.page_size, list.pages[0].page.backing_len());

        let top_left = list.active_top_left();
        assert_eq!(top_left.node, NonNull::from(list.pages[0].as_ref()));
        assert_eq!(top_left.x, 0);
        assert_eq!(top_left.y, 0);
        assert!(!top_left.garbage);
        assert_eq!(list.tracked_pins.len(), 1);
        assert_eq!(list.tracked_pins[0], NonNull::from(&*list.viewport_pin));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_init_respects_max_size_metadata() {
        let list = PageList::init(80, 24, Some(1024)).unwrap();

        assert_eq!(list.explicit_max_size, 1024);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_init_rows_across_two_pages() {
        let rows = 100;
        let mut capacity = STD_CAPACITY.adjust(CapacityAdjustment::cols(50)).unwrap();
        while capacity.rows() >= rows {
            capacity = STD_CAPACITY
                .adjust(CapacityAdjustment::cols(capacity.cols() + 50))
                .unwrap();
        }

        let list = PageList::init(capacity.cols(), rows, None).unwrap();

        assert!(list.pages.len() > 1);
        assert_eq!(list.viewport, Viewport::Active);
        assert_eq!(list.total_rows(), rows as usize);
        assert_eq!(list.total_rows, rows);
        assert_eq!(list.pages[0].page.size_rows(), capacity.rows());
        assert_eq!(
            list.pages.last().unwrap().page.size_rows() as usize,
            rows as usize % capacity.rows() as usize
        );
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_init_more_than_max_cols() {
        let requested_cols = STD_CAPACITY.max_cols().unwrap() + 1;
        let list = PageList::init(requested_cols, 80, None).unwrap();

        assert_eq!(list.viewport, Viewport::Active);
        assert_eq!(list.total_rows(), 80);
        assert_eq!(list.total_rows, 80);
        assert_eq!(list.pages.len(), 1);
        assert_eq!(list.pages[0].page.size_cols(), requested_cols);
        assert!(list.pages[0].page.backing_len() > standard_page_size());
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_grow_with_capacity_adds_row_without_new_page() {
        let mut list = PageList::init(80, 24, None).unwrap();
        let last_index = list.pages.len() - 1;
        let last_rows = list.pages[last_index].page.size_rows();
        let total_rows = list.total_rows;
        let page_size = list.page_size;
        let page_serial = list.page_serial;

        assert!(last_rows < list.pages[last_index].page.capacity().rows());
        assert_eq!(list.grow(), Ok(None));

        assert_eq!(list.pages[last_index].page.size_rows(), last_rows + 1);
        assert_eq!(list.total_rows, total_rows + 1);
        assert_eq!(list.page_size, page_size);
        assert_eq!(list.page_serial, page_serial);
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 1));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_grow_rows_builds_history_without_manual_size_mutation() {
        let mut list = PageList::init(80, 24, None).unwrap();

        list.grow_rows(10).unwrap();

        assert_eq!(list.total_rows, 34);
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 10));
        list.scroll(Scroll::Top);
        assert_eq!(viewport_top_left_screen_coord(&list), Coordinate::new(0, 0));
        assert_eq!(
            list.scrollbar(),
            Scrollbar {
                total: 34,
                offset: 0,
                len: 24,
            }
        );
    }

    #[test]
    fn page_list_grow_appends_page_when_last_page_is_full() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row");
        let mut list = PageList::init(cols, 1, None).unwrap();
        let old_last = list.last_node_ptr();
        let page_size = list.page_size;
        let page_serial = list.page_serial;

        assert_eq!(list.pages.len(), 1);
        assert_eq!(list.pages[0].page.size_rows(), 1);
        assert_eq!(list.pages[0].page.capacity().rows(), 1);

        let new = list.grow().unwrap().unwrap();

        assert_eq!(list.pages.len(), 2);
        assert_ne!(new, old_last);
        assert_eq!(new, list.last_node_ptr());
        assert_eq!(list.pages[1].page.size_rows(), 1);
        assert_eq!(list.total_rows, 2);
        assert!(list.page_size > page_size);
        assert_eq!(list.page_size, page_size + list.pages[1].page.backing_len());
        assert_eq!(list.page_serial, page_serial + 1);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_grow_allows_single_page_max_exceedance() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row")
            + 1;
        let rows = STD_CAPACITY.rows();
        let mut list = PageList::init(cols, rows, Some(0)).unwrap();

        assert_eq!(list.pages.len(), 1);
        assert!(list.pages[0].page.backing_len() > standard_page_size());
        assert_eq!(list.pages[0].page.size_rows(), rows);
        assert_eq!(list.pages[0].page.capacity().rows(), rows);
        assert!(list.page_size + standard_page_size() > list.max_size());
        assert!(list.grow().unwrap().is_some());

        assert_eq!(list.pages.len(), 2);
        assert_eq!(list.total_rows, rows + 1);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_grow_prunes_and_reuses_standard_page() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row");
        let mut list = PageList::init(cols, 1, Some(standard_page_size())).unwrap();
        let page1 = list.first_node_ptr();
        let page1_backing = list.pages[0].page.backing_ptr();
        let page1_serial = list.pages[0].serial;

        let tracked = list
            .track_pin(Pin {
                node: page1,
                y: 0,
                x: 0,
                garbage: false,
            })
            .unwrap();

        let page2 = list.grow().unwrap().unwrap();
        let old_page_size = list.page_size;
        let old_page_serial = list.page_serial;

        assert_eq!(list.pages.len(), 2);
        assert!(list.page_size + standard_page_size() > list.max_size());
        let reused = list.grow().unwrap().unwrap();

        assert_eq!(list.pages.len(), 2);
        assert_eq!(list.first_node_ptr(), page2);
        assert_eq!(list.last_node_ptr(), page1);
        assert_eq!(reused, page1);
        assert_eq!(list.pages[1].page.backing_ptr(), page1_backing);
        assert_eq!(list.page_size, old_page_size);
        assert_eq!(list.page_serial_min, page1_serial + 1);
        assert_eq!(list.pages[1].serial, old_page_serial);
        assert_eq!(list.page_serial, old_page_serial + 1);
        assert_eq!(list.total_rows, 2);

        let tracked_pin = unsafe {
            // Safety: tracked remains owned by list.tracked_pin_storage.
            tracked.as_ref()
        };
        assert_eq!(tracked_pin.node, list.first_node_ptr());
        assert_eq!(tracked_pin.x, 0);
        assert_eq!(tracked_pin.y, 0);
        assert!(tracked_pin.garbage);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_grow_prune_cached_viewport_inside_pruned_page_moves_top() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row");
        let mut list = PageList::init(cols, 1, Some(standard_page_size())).unwrap();
        list.grow().unwrap();
        let page1 = list.first_node_ptr();

        list.viewport = Viewport::Pin;
        list.set_viewport_pin(Pin {
            node: page1,
            y: 0,
            x: 0,
            garbage: false,
        });
        assert_eq!(list.scrollbar().offset, 0);

        list.grow().unwrap();

        assert_eq!(list.viewport, Viewport::Top);
        assert_eq!(list.scrollbar().offset, 0);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_grow_prune_cached_viewport_after_pruned_page_decrements_offset() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row");
        let mut list = PageList::init(cols, 1, Some(standard_page_size())).unwrap();
        let page1 = list.first_node_ptr();
        let page2 = list.grow().unwrap().unwrap();
        assert_eq!(list.first_node_ptr(), page1);

        list.viewport = Viewport::Pin;
        list.set_viewport_pin(Pin {
            node: page2,
            y: 0,
            x: 0,
            garbage: false,
        });
        assert_eq!(list.scrollbar().offset, 1);

        list.grow().unwrap();

        assert_eq!(list.viewport, Viewport::Pin);
        assert_eq!(list.viewport_pin.node, page2);
        assert_eq!(list.scrollbar().offset, 0);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_grow_prune_backs_out_to_preserve_active_area() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row")
            + 1;
        let capacity_rows = initial_capacity(cols).rows();
        let rows = capacity_rows + 2;
        let mut list = PageList::init(cols, rows, Some(standard_page_size())).unwrap();
        let page1 = list.first_node_ptr();

        assert_eq!(list.pages.len(), 2);
        while {
            let last = list.pages.last().unwrap();
            last.page.size_rows() < last.page.capacity().rows()
        } {
            assert_eq!(list.grow(), Ok(None));
        }

        let old_page_size = list.page_size;
        let old_total_rows = list.total_rows;

        assert_eq!(list.pages.len(), 2);
        assert!(list.page_size + standard_page_size() > list.max_size());
        assert!(
            list.total_rows as usize - list.pages[0].page.size_rows() as usize + 1
                < list.rows as usize
        );
        let appended = list.grow().unwrap().unwrap();

        assert_eq!(list.pages.len(), 3);
        assert_eq!(list.first_node_ptr(), page1);
        assert_eq!(list.last_node_ptr(), appended);
        assert_eq!(list.total_rows, old_total_rows + 1);
        assert_eq!(
            list.page_size,
            old_page_size + list.pages.last().unwrap().page.backing_len()
        );
        assert!(list.total_rows >= list.rows);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_grow_prune_drops_non_standard_page_and_allocates_fresh() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row")
            + 1;
        let rows = STD_CAPACITY.rows();
        let mut list = PageList::init(cols, rows, Some(0)).unwrap();
        let page1 = list.first_node_ptr();
        let page1_len = list.pages[0].page.backing_len();
        let tracked = list
            .track_pin(Pin {
                node: page1,
                y: 0,
                x: 0,
                garbage: false,
            })
            .unwrap();
        let page2 = list.grow().unwrap().unwrap();
        let page2_len = list.pages[1].page.backing_len();

        while {
            let last = list.pages.last().unwrap();
            last.page.size_rows() < last.page.capacity().rows()
        } {
            assert_eq!(list.grow(), Ok(None));
        }

        let old_page_size = list.page_size;
        let old_page_serial = list.page_serial;

        assert!(page1_len > standard_page_size());
        assert!(list.page_size + standard_page_size() > list.max_size());
        let fresh = list.grow().unwrap().unwrap();

        assert_eq!(list.first_node_ptr(), page2);
        assert_eq!(list.last_node_ptr(), fresh);
        assert_eq!(
            list.page_size,
            old_page_size - page1_len + list.pages.last().unwrap().page.backing_len()
        );
        assert_eq!(list.pages[0].page.backing_len(), page2_len);
        assert_eq!(list.pages.last().unwrap().serial, old_page_serial);
        assert_eq!(list.page_serial, old_page_serial + 1);

        let tracked_pin = unsafe {
            // Safety: tracked remains owned by list.tracked_pin_storage.
            tracked.as_ref()
        };
        assert_eq!(tracked_pin.node, list.first_node_ptr());
        assert!(tracked_pin.garbage);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_reset_basic() {
        let mut list = PageList::init(80, 24, None).unwrap();

        list.reset();

        assert_eq!(list.viewport, Viewport::Active);
        assert_eq!(list.total_rows, list.rows);
        assert_eq!(
            list.get_top_left(point::Tag::Active),
            Pin {
                node: list.first_node_ptr(),
                y: 0,
                x: 0,
                garbage: false,
            }
        );
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_reset_clears_history() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.grow_rows(30).unwrap();
        assert!(list.total_rows > list.rows);

        list.reset();

        assert_eq!(list.viewport, Viewport::Active);
        assert_eq!(list.total_rows, list.rows);
        assert_eq!(
            list.get_top_left(point::Tag::Active),
            Pin {
                node: list.first_node_ptr(),
                y: 0,
                x: 0,
                garbage: false,
            }
        );
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_reset_across_two_active_pages() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row")
            + 1;
        let capacity = initial_capacity(cols);
        let rows = capacity.rows() + 2;
        let mut list = PageList::init(cols, rows, None).unwrap();
        assert_eq!(list.pages.len(), 2);

        list.reset();

        assert_eq!(list.pages.len(), 2);
        assert_eq!(list.total_rows, rows);
        assert_eq!(list.pages[0].page.size_rows(), capacity.rows());
        assert_eq!(list.pages[1].page.size_rows(), 2);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_reset_moves_tracked_pins_and_marks_them_garbage() {
        let mut list = PageList::init(80, 24, None).unwrap();
        let tracked = list
            .track_pin(
                list.pin(point::Point::active(Coordinate::new(42, 12)))
                    .unwrap(),
            )
            .unwrap();

        list.reset();

        let tracked_pin = unsafe {
            // Safety: tracked remains owned by list.tracked_pin_storage.
            tracked.as_ref()
        };
        assert_eq!(tracked_pin.node, list.first_node_ptr());
        assert_eq!(tracked_pin.x, 0);
        assert_eq!(tracked_pin.y, 0);
        assert!(tracked_pin.garbage);
        assert_eq!(list.viewport_pin.node, list.first_node_ptr());
        assert_eq!(list.viewport_pin.x, 0);
        assert_eq!(list.viewport_pin.y, 0);
        assert!(!list.viewport_pin.garbage);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_reset_invalidates_old_page_serials() {
        let mut list = PageList::init(80, 24, None).unwrap();
        let old_serial = list.pages[0].serial;
        assert!(old_serial >= list.page_serial_min);
        assert!(old_serial < list.page_serial);

        list.reset();

        assert!(old_serial < list.page_serial_min);
        for node in &list.pages {
            assert!(node.serial >= list.page_serial_min);
            assert!(node.serial < list.page_serial);
        }
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_reset_drops_extra_non_standard_pages() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row")
            + 1;
        let rows = initial_capacity(cols).rows();
        let mut list = PageList::init(cols, rows, None).unwrap();
        list.grow().unwrap();
        assert_eq!(list.pages.len(), 2);
        assert!(list.pages[0].page.backing_len() > standard_page_size());

        list.reset();

        assert_eq!(list.pages.len(), 1);
        assert_eq!(list.total_rows, rows);
        assert_eq!(list.page_size, list.pages[0].page.backing_len());
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_reset_clears_cached_viewport_offset() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.grow_rows(30).unwrap();
        list.scroll(Scroll::Row(1));
        assert_eq!(list.viewport, Viewport::Pin);
        assert_eq!(list.viewport_pin_row_offset, Some(1));

        list.reset();

        assert_eq!(list.viewport, Viewport::Active);
        assert_eq!(list.viewport_pin_row_offset, None);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_page_iterator_full_active_region_one_page() {
        let list = PageList::init(80, 24, None).unwrap();
        let chunks = chunk_tuples(
            &list,
            list.page_iterator(
                Direction::RightDown,
                point::Point::active(Coordinate::new(0, 0)),
                None,
            ),
        );

        assert_eq!(chunks, vec![(0, 0, 24)]);
        let chunk = list
            .page_iterator(
                Direction::RightDown,
                point::Point::active(Coordinate::new(0, 0)),
                None,
            )
            .next()
            .unwrap();
        assert!(chunk.full_page(&list));
    }

    #[test]
    fn page_list_page_iterator_trimmed_bottom_one_page() {
        let list = PageList::init(80, 20, None).unwrap();
        let chunks = chunk_tuples(
            &list,
            list.page_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(0, 0)),
                Some(point::Point::screen(Coordinate::new(0, 9))),
            ),
        );

        assert_eq!(chunks, vec![(0, 0, 10)]);
    }

    #[test]
    fn page_list_page_iterator_trimmed_top_one_page() {
        let list = PageList::init(80, 20, None).unwrap();
        let chunks = chunk_tuples(
            &list,
            list.page_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(0, 10)),
                None,
            ),
        );

        assert_eq!(chunks, vec![(0, 10, 20)]);
    }

    #[test]
    fn page_list_page_iterator_trimmed_both_sides_one_page() {
        let list = PageList::init(80, 20, None).unwrap();
        let chunks = chunk_tuples(
            &list,
            list.page_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(0, 5)),
                Some(point::Point::screen(Coordinate::new(0, 12))),
            ),
        );

        assert_eq!(chunks, vec![(0, 5, 13)]);
    }

    #[test]
    fn page_list_page_iterator_cross_page_right_down() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row");
        let mut list = PageList::init(cols, 1, None).unwrap();
        list.grow_rows(3).unwrap();

        let chunks = chunk_tuples(
            &list,
            list.page_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(0, 0)),
                None,
            ),
        );

        assert_eq!(chunks, vec![(0, 0, 1), (1, 0, 1), (2, 0, 1), (3, 0, 1)]);
    }

    #[test]
    fn page_list_page_iterator_cross_page_left_up() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row");
        let mut list = PageList::init(cols, 1, None).unwrap();
        list.grow_rows(3).unwrap();

        let chunks = chunk_tuples(
            &list,
            list.page_iterator(
                Direction::LeftUp,
                point::Point::screen(Coordinate::new(0, 0)),
                None,
            ),
        );

        assert_eq!(chunks, vec![(3, 0, 1), (2, 0, 1), (1, 0, 1), (0, 0, 1)]);
    }

    #[test]
    fn page_list_page_iterator_active_cross_page_partial_right_down() {
        let capacity_rows = initial_capacity(80).rows();
        assert!(capacity_rows > 2);
        let mut list = PageList::init(80, capacity_rows, None).unwrap();
        list.grow_rows(2).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));

        let chunks = chunk_tuples(
            &list,
            list.page_iterator(
                Direction::RightDown,
                point::Point::active(Coordinate::new(0, 0)),
                None,
            ),
        );

        assert_eq!(chunks, vec![(0, 2, capacity_rows), (1, 0, 2)]);
    }

    #[test]
    fn page_list_page_iterator_active_cross_page_partial_left_up() {
        let capacity_rows = initial_capacity(80).rows();
        assert!(capacity_rows > 2);
        let mut list = PageList::init(80, capacity_rows, None).unwrap();
        list.grow_rows(2).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));

        let chunks = chunk_tuples(
            &list,
            list.page_iterator(
                Direction::LeftUp,
                point::Point::active(Coordinate::new(0, 0)),
                None,
            ),
        );

        assert_eq!(chunks, vec![(1, 0, 2), (0, 2, capacity_rows)]);
    }

    #[test]
    fn page_list_page_iterator_history_right_down_stops_before_active() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row");
        let mut list = PageList::init(cols, 1, None).unwrap();
        list.grow_rows(4).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 4));

        let chunks = chunk_tuples(
            &list,
            list.page_iterator(
                Direction::RightDown,
                point::Point::history(Coordinate::new(0, 0)),
                None,
            ),
        );

        assert_eq!(chunks, vec![(0, 0, 1), (1, 0, 1), (2, 0, 1), (3, 0, 1)]);
    }

    #[test]
    fn page_list_page_iterator_history_left_up_stops_before_active() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row");
        let mut list = PageList::init(cols, 1, None).unwrap();
        list.grow_rows(4).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 4));

        let chunks = chunk_tuples(
            &list,
            list.page_iterator(
                Direction::LeftUp,
                point::Point::history(Coordinate::new(0, 0)),
                None,
            ),
        );

        assert_eq!(chunks, vec![(3, 0, 1), (2, 0, 1), (1, 0, 1), (0, 0, 1)]);
    }

    #[test]
    fn page_list_page_iterator_invalid_endpoint_is_empty() {
        let list = PageList::init(80, 20, None).unwrap();

        assert_eq!(
            list.page_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(80, 0)),
                None,
            )
            .count(),
            0
        );
        assert_eq!(
            list.page_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(0, 0)),
                Some(point::Point::screen(Coordinate::new(80, 0))),
            )
            .count(),
            0
        );
    }

    #[test]
    fn page_list_row_iterator_active_single_page_right_down() {
        let list = PageList::init(80, 4, None).unwrap();
        let rows = row_tuples(
            &list,
            list.row_iterator(
                Direction::RightDown,
                point::Point::active(Coordinate::new(7, 0)),
                None,
            ),
        );

        assert_eq!(rows, vec![(0, 0, 0), (0, 1, 0), (0, 2, 0), (0, 3, 0)]);
    }

    #[test]
    fn page_list_row_iterator_active_single_page_left_up() {
        let list = PageList::init(80, 4, None).unwrap();
        let rows = row_tuples(
            &list,
            list.row_iterator(
                Direction::LeftUp,
                point::Point::active(Coordinate::new(9, 0)),
                None,
            ),
        );

        assert_eq!(rows, vec![(0, 3, 0), (0, 2, 0), (0, 1, 0), (0, 0, 0)]);
    }

    #[test]
    fn page_list_row_iterator_cross_page_right_down() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row");
        let mut list = PageList::init(cols, 1, None).unwrap();
        list.grow_rows(3).unwrap();

        let rows = row_tuples(
            &list,
            list.row_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(0, 0)),
                None,
            ),
        );

        assert_eq!(rows, vec![(0, 0, 0), (1, 0, 0), (2, 0, 0), (3, 0, 0)]);
    }

    #[test]
    fn page_list_row_iterator_cross_page_left_up() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row");
        let mut list = PageList::init(cols, 1, None).unwrap();
        list.grow_rows(3).unwrap();

        let rows = row_tuples(
            &list,
            list.row_iterator(
                Direction::LeftUp,
                point::Point::screen(Coordinate::new(0, 0)),
                None,
            ),
        );

        assert_eq!(rows, vec![(3, 0, 0), (2, 0, 0), (1, 0, 0), (0, 0, 0)]);
    }

    #[test]
    fn page_list_row_iterator_active_cross_page_partial_right_down() {
        let capacity_rows = initial_capacity(80).rows();
        assert!(capacity_rows > 2);
        let mut list = PageList::init(80, capacity_rows, None).unwrap();
        list.grow_rows(2).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));

        let rows = row_tuples(
            &list,
            list.row_iterator(
                Direction::RightDown,
                point::Point::active(Coordinate::new(0, 0)),
                None,
            ),
        );
        let mut expected = (2..capacity_rows).map(|y| (0, y, 0)).collect::<Vec<_>>();
        expected.extend([(1, 0, 0), (1, 1, 0)]);

        assert_eq!(rows, expected);
    }

    #[test]
    fn page_list_row_iterator_active_cross_page_partial_left_up() {
        let capacity_rows = initial_capacity(80).rows();
        assert!(capacity_rows > 2);
        let mut list = PageList::init(80, capacity_rows, None).unwrap();
        list.grow_rows(2).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));

        let rows = row_tuples(
            &list,
            list.row_iterator(
                Direction::LeftUp,
                point::Point::active(Coordinate::new(0, 0)),
                None,
            ),
        );
        let mut expected = vec![(1, 1, 0), (1, 0, 0)];
        expected.extend((2..capacity_rows).rev().map(|y| (0, y, 0)));

        assert_eq!(rows, expected);
    }

    #[test]
    fn page_list_row_iterator_history_right_down_stops_before_active() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row");
        let mut list = PageList::init(cols, 1, None).unwrap();
        list.grow_rows(4).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 4));

        let rows = row_tuples(
            &list,
            list.row_iterator(
                Direction::RightDown,
                point::Point::history(Coordinate::new(0, 0)),
                None,
            ),
        );

        assert_eq!(rows, vec![(0, 0, 0), (1, 0, 0), (2, 0, 0), (3, 0, 0)]);
    }

    #[test]
    fn page_list_row_iterator_history_left_up_stops_before_active() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row");
        let mut list = PageList::init(cols, 1, None).unwrap();
        list.grow_rows(4).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 4));

        let rows = row_tuples(
            &list,
            list.row_iterator(
                Direction::LeftUp,
                point::Point::history(Coordinate::new(0, 0)),
                None,
            ),
        );

        assert_eq!(rows, vec![(3, 0, 0), (2, 0, 0), (1, 0, 0), (0, 0, 0)]);
    }

    #[test]
    fn page_list_row_iterator_explicit_limit_right_down() {
        let list = PageList::init(80, 20, None).unwrap();
        let rows = row_tuples(
            &list,
            list.row_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(6, 5)),
                Some(point::Point::screen(Coordinate::new(2, 12))),
            ),
        );

        assert_eq!(
            rows,
            vec![
                (0, 5, 0),
                (0, 6, 0),
                (0, 7, 0),
                (0, 8, 0),
                (0, 9, 0),
                (0, 10, 0),
                (0, 11, 0),
                (0, 12, 0)
            ]
        );
    }

    #[test]
    fn page_list_row_iterator_explicit_limit_left_up_nonzero_start() {
        let list = PageList::init(80, 20, None).unwrap();
        let rows = row_tuples(
            &list,
            list.row_iterator(
                Direction::LeftUp,
                point::Point::screen(Coordinate::new(6, 5)),
                Some(point::Point::screen(Coordinate::new(2, 12))),
            ),
        );

        assert_eq!(
            rows,
            vec![
                (0, 12, 0),
                (0, 11, 0),
                (0, 10, 0),
                (0, 9, 0),
                (0, 8, 0),
                (0, 7, 0),
                (0, 6, 0),
                (0, 5, 0)
            ]
        );
    }

    #[test]
    fn page_list_row_iterator_invalid_endpoint_is_empty() {
        let list = PageList::init(80, 20, None).unwrap();

        assert_eq!(
            list.row_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(80, 0)),
                None,
            )
            .count(),
            0
        );
        assert_eq!(
            list.row_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(0, 0)),
                Some(point::Point::screen(Coordinate::new(80, 0))),
            )
            .count(),
            0
        );
    }

    #[test]
    fn page_list_row_iterator_pins_convert_back_to_points() {
        let list = PageList::init(80, 4, None).unwrap();
        let points = list
            .row_iterator(
                Direction::RightDown,
                point::Point::active(Coordinate::new(3, 1)),
                Some(point::Point::active(Coordinate::new(9, 3))),
            )
            .map(|pin| {
                list.point_from_pin(point::Tag::Active, pin)
                    .expect("row pin must map to active point")
                    .coord()
            })
            .collect::<Vec<_>>();

        assert_eq!(
            points,
            vec![
                Coordinate::new(0, 1),
                Coordinate::new(0, 2),
                Coordinate::new(0, 3)
            ]
        );
    }

    #[test]
    fn page_list_cell_iterator_single_row_right_down_ignores_limit_x() {
        let list = PageList::init(4, 1, None).unwrap();
        let cells = cell_tuples(
            &list,
            list.cell_iterator(
                Direction::RightDown,
                point::Point::active(Coordinate::new(1, 0)),
                Some(point::Point::active(Coordinate::new(0, 0))),
            ),
        );

        assert_eq!(cells, vec![(0, 0, 1), (0, 0, 2), (0, 0, 3)]);
    }

    #[test]
    fn page_list_cell_iterator_single_row_left_up_ignores_limit_x() {
        let list = PageList::init(4, 1, None).unwrap();
        let cells = cell_tuples(
            &list,
            list.cell_iterator(
                Direction::LeftUp,
                point::Point::active(Coordinate::new(3, 0)),
                Some(point::Point::active(Coordinate::new(2, 0))),
            ),
        );

        assert_eq!(cells, vec![(0, 0, 2), (0, 0, 1), (0, 0, 0)]);
    }

    #[test]
    fn page_list_cell_iterator_multi_row_right_down_resets_next_rows() {
        let list = PageList::init(4, 3, None).unwrap();
        let cells = cell_tuples(
            &list,
            list.cell_iterator(
                Direction::RightDown,
                point::Point::active(Coordinate::new(2, 0)),
                Some(point::Point::active(Coordinate::new(1, 2))),
            ),
        );

        assert_eq!(
            cells,
            vec![
                (0, 0, 2),
                (0, 0, 3),
                (0, 1, 0),
                (0, 1, 1),
                (0, 1, 2),
                (0, 1, 3),
                (0, 2, 0),
                (0, 2, 1),
                (0, 2, 2),
                (0, 2, 3)
            ]
        );
    }

    #[test]
    fn page_list_cell_iterator_multi_row_left_up_resets_prior_rows() {
        let list = PageList::init(4, 3, None).unwrap();
        let cells = cell_tuples(
            &list,
            list.cell_iterator(
                Direction::LeftUp,
                point::Point::active(Coordinate::new(2, 0)),
                Some(point::Point::active(Coordinate::new(1, 2))),
            ),
        );

        assert_eq!(
            cells,
            vec![
                (0, 2, 1),
                (0, 2, 0),
                (0, 1, 3),
                (0, 1, 2),
                (0, 1, 1),
                (0, 1, 0),
                (0, 0, 3),
                (0, 0, 2),
                (0, 0, 1),
                (0, 0, 0)
            ]
        );
    }

    #[test]
    fn page_list_cell_iterator_cross_page_right_down() {
        let mut list = PageList::init(4, 4, None).unwrap();
        list.split(Pin {
            node: list.first_node_ptr(),
            y: 2,
            x: 0,
            garbage: false,
        })
        .unwrap();

        let cells = cell_tuples(
            &list,
            list.cell_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(2, 1)),
                Some(point::Point::screen(Coordinate::new(0, 2))),
            ),
        );

        assert_eq!(
            cells,
            vec![
                (0, 1, 2),
                (0, 1, 3),
                (1, 0, 0),
                (1, 0, 1),
                (1, 0, 2),
                (1, 0, 3)
            ]
        );
    }

    #[test]
    fn page_list_cell_iterator_cross_page_left_up() {
        let mut list = PageList::init(4, 4, None).unwrap();
        list.split(Pin {
            node: list.first_node_ptr(),
            y: 2,
            x: 0,
            garbage: false,
        })
        .unwrap();

        let cells = cell_tuples(
            &list,
            list.cell_iterator(
                Direction::LeftUp,
                point::Point::screen(Coordinate::new(2, 1)),
                Some(point::Point::screen(Coordinate::new(0, 2))),
            ),
        );

        assert_eq!(
            cells,
            vec![(1, 0, 0), (0, 1, 3), (0, 1, 2), (0, 1, 1), (0, 1, 0)]
        );
    }

    #[test]
    fn page_list_cell_iterator_active_partial_cross_page_right_down() {
        let mut list = PageList::init(4, 4, None).unwrap();
        list.grow_rows(2).unwrap();
        list.split(Pin {
            node: list.first_node_ptr(),
            y: 3,
            x: 0,
            garbage: false,
        })
        .unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));

        let cells = cell_tuples(
            &list,
            list.cell_iterator(
                Direction::RightDown,
                point::Point::active(Coordinate::new(1, 0)),
                None,
            ),
        );

        assert_eq!(
            cells,
            vec![
                (0, 2, 1),
                (0, 2, 2),
                (0, 2, 3),
                (1, 0, 0),
                (1, 0, 1),
                (1, 0, 2),
                (1, 0, 3),
                (1, 1, 0),
                (1, 1, 1),
                (1, 1, 2),
                (1, 1, 3),
                (1, 2, 0),
                (1, 2, 1),
                (1, 2, 2),
                (1, 2, 3)
            ]
        );
    }

    #[test]
    fn page_list_cell_iterator_active_partial_cross_page_left_up() {
        let mut list = PageList::init(4, 4, None).unwrap();
        list.grow_rows(2).unwrap();
        list.split(Pin {
            node: list.first_node_ptr(),
            y: 3,
            x: 0,
            garbage: false,
        })
        .unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));

        let cells = cell_tuples(
            &list,
            list.cell_iterator(
                Direction::LeftUp,
                point::Point::active(Coordinate::new(1, 0)),
                None,
            ),
        );

        assert_eq!(
            cells,
            vec![
                (1, 2, 3),
                (1, 2, 2),
                (1, 2, 1),
                (1, 2, 0),
                (1, 1, 3),
                (1, 1, 2),
                (1, 1, 1),
                (1, 1, 0),
                (1, 0, 3),
                (1, 0, 2),
                (1, 0, 1),
                (1, 0, 0),
                (0, 2, 3),
                (0, 2, 2),
                (0, 2, 1),
                (0, 2, 0)
            ]
        );
    }

    #[test]
    fn page_list_cell_iterator_history_right_down_stops_before_active() {
        let mut list = PageList::init(3, 2, None).unwrap();
        list.grow_rows(2).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));

        let cells = cell_tuples(
            &list,
            list.cell_iterator(
                Direction::RightDown,
                point::Point::history(Coordinate::new(1, 0)),
                None,
            ),
        );

        assert_eq!(
            cells,
            vec![(0, 0, 1), (0, 0, 2), (0, 1, 0), (0, 1, 1), (0, 1, 2)]
        );
    }

    #[test]
    fn page_list_cell_iterator_history_left_up_stops_before_active() {
        let mut list = PageList::init(3, 2, None).unwrap();
        list.grow_rows(2).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));

        let cells = cell_tuples(
            &list,
            list.cell_iterator(
                Direction::LeftUp,
                point::Point::history(Coordinate::new(1, 0)),
                None,
            ),
        );

        assert_eq!(
            cells,
            vec![
                (0, 1, 2),
                (0, 1, 1),
                (0, 1, 0),
                (0, 0, 2),
                (0, 0, 1),
                (0, 0, 0)
            ]
        );
    }

    #[test]
    fn page_list_cell_iterator_invalid_endpoint_is_empty() {
        let list = PageList::init(4, 2, None).unwrap();

        assert_eq!(
            list.cell_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(4, 0)),
                None,
            )
            .count(),
            0
        );
        assert_eq!(
            list.cell_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(0, 0)),
                Some(point::Point::screen(Coordinate::new(4, 0))),
            )
            .count(),
            0
        );
    }

    #[test]
    fn page_list_cell_iterator_pins_convert_back_to_points() {
        let list = PageList::init(4, 2, None).unwrap();
        let points = list
            .cell_iterator(
                Direction::RightDown,
                point::Point::active(Coordinate::new(2, 0)),
                Some(point::Point::active(Coordinate::new(1, 1))),
            )
            .map(|pin| {
                list.point_from_pin(point::Tag::Active, pin)
                    .expect("cell pin must map to active point")
                    .coord()
            })
            .collect::<Vec<_>>();

        assert_eq!(
            points,
            vec![
                Coordinate::new(2, 0),
                Coordinate::new(3, 0),
                Coordinate::new(0, 1),
                Coordinate::new(1, 1),
                Coordinate::new(2, 1),
                Coordinate::new(3, 1)
            ]
        );
    }

    #[test]
    fn page_list_prompt_iterator_left_up() {
        let mut list = PageList::init(2, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 3, SemanticPrompt::Prompt);
        set_screen_semantic_prompt(&mut list, 6, SemanticPrompt::Prompt);
        set_screen_semantic_prompt(&mut list, 7, SemanticPrompt::PromptContinuation);
        set_screen_semantic_prompt(&mut list, 8, SemanticPrompt::PromptContinuation);
        set_screen_semantic_prompt(&mut list, 12, SemanticPrompt::PromptContinuation);

        let prompts = prompt_screen_points(
            &list,
            list.prompt_iterator(
                Direction::LeftUp,
                point::Point::screen(Coordinate::new(1, 0)),
                None,
            ),
        );

        assert_eq!(
            prompts,
            vec![
                Coordinate::new(0, 12),
                Coordinate::new(0, 6),
                Coordinate::new(0, 3)
            ]
        );
    }

    #[test]
    fn page_list_prompt_iterator_right_down() {
        let mut list = PageList::init(2, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 3, SemanticPrompt::Prompt);
        set_screen_semantic_prompt(&mut list, 6, SemanticPrompt::Prompt);
        set_screen_semantic_prompt(&mut list, 7, SemanticPrompt::PromptContinuation);
        set_screen_semantic_prompt(&mut list, 8, SemanticPrompt::PromptContinuation);
        set_screen_semantic_prompt(&mut list, 12, SemanticPrompt::PromptContinuation);

        let prompts = prompt_screen_points(
            &list,
            list.prompt_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(1, 0)),
                None,
            ),
        );

        assert_eq!(
            prompts,
            vec![
                Coordinate::new(0, 3),
                Coordinate::new(0, 6),
                Coordinate::new(0, 12)
            ]
        );
    }

    #[test]
    fn page_list_prompt_iterator_right_down_continuation_at_start() {
        let mut list = PageList::init(2, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 0, SemanticPrompt::PromptContinuation);
        set_screen_semantic_prompt(&mut list, 1, SemanticPrompt::PromptContinuation);
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);

        let prompts = prompt_screen_points(
            &list,
            list.prompt_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(1, 0)),
                None,
            ),
        );

        assert_eq!(prompts, vec![Coordinate::new(0, 0), Coordinate::new(0, 5)]);
    }

    #[test]
    fn page_list_prompt_iterator_right_down_starts_inside_continuation() {
        let mut list = PageList::init(2, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 2, SemanticPrompt::Prompt);
        set_screen_semantic_prompt(&mut list, 3, SemanticPrompt::PromptContinuation);
        set_screen_semantic_prompt(&mut list, 4, SemanticPrompt::PromptContinuation);

        let prompts = prompt_screen_points(
            &list,
            list.prompt_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(1, 3)),
                None,
            ),
        );

        assert_eq!(prompts, vec![Coordinate::new(0, 3)]);
    }

    #[test]
    fn page_list_prompt_iterator_right_down_limit_prompt_inclusive() {
        let mut list = PageList::init(2, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let prompts = prompt_screen_points(
            &list,
            list.prompt_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(1, 0)),
                Some(point::Point::screen(Coordinate::new(1, 5))),
            ),
        );

        assert_eq!(prompts, vec![Coordinate::new(0, 5)]);
    }

    #[test]
    fn page_list_prompt_iterator_left_up_limit_prompt_inclusive() {
        let mut list = PageList::init(2, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let prompts = prompt_screen_points(
            &list,
            list.prompt_iterator(
                Direction::LeftUp,
                point::Point::screen(Coordinate::new(1, 10)),
                Some(point::Point::screen(Coordinate::new(1, 15))),
            ),
        );

        assert_eq!(prompts, vec![Coordinate::new(0, 10)]);
    }

    #[test]
    fn page_list_prompt_iterator_cross_page_continuation_right_down() {
        let mut list = PageList::init(2, 6, None).unwrap();
        list.split(Pin {
            node: list.first_node_ptr(),
            y: 3,
            x: 0,
            garbage: false,
        })
        .unwrap();
        set_screen_semantic_prompt(&mut list, 2, SemanticPrompt::Prompt);
        set_screen_semantic_prompt(&mut list, 3, SemanticPrompt::PromptContinuation);
        set_screen_semantic_prompt(&mut list, 4, SemanticPrompt::PromptContinuation);
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);

        let prompts = prompt_screen_points(
            &list,
            list.prompt_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(1, 0)),
                None,
            ),
        );

        assert_eq!(prompts, vec![Coordinate::new(0, 2), Coordinate::new(0, 5)]);
    }

    #[test]
    fn page_list_prompt_iterator_cross_page_continuation_left_up() {
        let mut list = PageList::init(2, 6, None).unwrap();
        list.split(Pin {
            node: list.first_node_ptr(),
            y: 3,
            x: 0,
            garbage: false,
        })
        .unwrap();
        set_screen_semantic_prompt(&mut list, 2, SemanticPrompt::Prompt);
        set_screen_semantic_prompt(&mut list, 3, SemanticPrompt::PromptContinuation);
        set_screen_semantic_prompt(&mut list, 4, SemanticPrompt::PromptContinuation);
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);

        let prompts = prompt_screen_points(
            &list,
            list.prompt_iterator(
                Direction::LeftUp,
                point::Point::screen(Coordinate::new(1, 0)),
                None,
            ),
        );

        assert_eq!(prompts, vec![Coordinate::new(0, 5), Coordinate::new(0, 2)]);
    }

    #[test]
    fn page_list_prompt_iterator_right_down_limit_continuation() {
        let mut list = PageList::init(2, 10, None).unwrap();
        set_screen_semantic_prompt(&mut list, 2, SemanticPrompt::Prompt);
        set_screen_semantic_prompt(&mut list, 3, SemanticPrompt::PromptContinuation);
        set_screen_semantic_prompt(&mut list, 4, SemanticPrompt::PromptContinuation);

        let prompts = prompt_screen_points(
            &list,
            list.prompt_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(1, 0)),
                Some(point::Point::screen(Coordinate::new(1, 3))),
            ),
        );

        assert_eq!(prompts, vec![Coordinate::new(0, 2)]);
    }

    #[test]
    fn page_list_prompt_iterator_left_up_limit_continuation() {
        let mut list = PageList::init(2, 10, None).unwrap();
        set_screen_semantic_prompt(&mut list, 3, SemanticPrompt::PromptContinuation);

        let prompts = prompt_screen_points(
            &list,
            list.prompt_iterator(
                Direction::LeftUp,
                point::Point::screen(Coordinate::new(1, 3)),
                Some(point::Point::screen(Coordinate::new(1, 3))),
            ),
        );

        assert_eq!(prompts, vec![Coordinate::new(0, 3)]);
    }

    #[test]
    fn page_list_prompt_iterator_history_stops_before_active() {
        let mut list = PageList::init(2, 2, None).unwrap();
        list.grow_rows(2).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));
        set_screen_semantic_prompt(&mut list, 0, SemanticPrompt::Prompt);
        set_screen_semantic_prompt(&mut list, 1, SemanticPrompt::Prompt);
        set_screen_semantic_prompt(&mut list, 2, SemanticPrompt::Prompt);

        let right_down = prompt_screen_points(
            &list,
            list.prompt_iterator(
                Direction::RightDown,
                point::Point::history(Coordinate::new(1, 0)),
                None,
            ),
        );
        let left_up = prompt_screen_points(
            &list,
            list.prompt_iterator(
                Direction::LeftUp,
                point::Point::history(Coordinate::new(1, 0)),
                None,
            ),
        );

        assert_eq!(
            right_down,
            vec![Coordinate::new(0, 0), Coordinate::new(0, 1)]
        );
        assert_eq!(left_up, vec![Coordinate::new(0, 1), Coordinate::new(0, 0)]);
    }

    #[test]
    fn page_list_prompt_iterator_invalid_endpoint_is_empty() {
        let list = PageList::init(2, 10, None).unwrap();

        assert_eq!(
            list.prompt_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(2, 0)),
                None,
            )
            .count(),
            0
        );
        assert_eq!(
            list.prompt_iterator(
                Direction::RightDown,
                point::Point::screen(Coordinate::new(0, 0)),
                Some(point::Point::screen(Coordinate::new(2, 0))),
            )
            .count(),
            0
        );
    }

    #[test]
    fn page_list_highlight_semantic_prompt_includes_input() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..5 {
            set_screen_cell_semantic(&mut list, x, 5, 'A', SemanticContent::Prompt);
        }
        for x in 5..8 {
            set_screen_cell_semantic(&mut list, x, 5, 'B', SemanticContent::Input);
        }
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(2, 5)))
            .unwrap();
        let highlight = list.highlight_semantic_prompt(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(0, 5), Coordinate::new(7, 5)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_prompt_stops_before_output() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..3 {
            set_screen_cell_semantic(&mut list, x, 5, '$', SemanticContent::Prompt);
        }
        for x in 3..5 {
            set_screen_cell_semantic(&mut list, x, 5, 'i', SemanticContent::Input);
        }
        set_screen_cell_semantic(&mut list, 5, 5, 'o', SemanticContent::Output);
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 5)))
            .unwrap();
        let highlight = list.highlight_semantic_prompt(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(0, 5), Coordinate::new(4, 5)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_prompt_multiline() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..3 {
            set_screen_cell_semantic(&mut list, x, 5, '$', SemanticContent::Prompt);
        }
        for x in 3..10 {
            set_screen_cell_semantic(&mut list, x, 5, 'i', SemanticContent::Input);
        }
        for x in 0..5 {
            set_screen_cell_semantic(&mut list, x, 6, 'i', SemanticContent::Input);
        }
        set_screen_cell_semantic(&mut list, 5, 6, 'o', SemanticContent::Output);
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 5)))
            .unwrap();
        let highlight = list.highlight_semantic_prompt(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(0, 5), Coordinate::new(4, 6)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_prompt_only() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..3 {
            set_screen_cell_semantic(&mut list, x, 5, '$', SemanticContent::Prompt);
        }
        set_screen_cell_semantic(&mut list, 3, 5, 'o', SemanticContent::Output);
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 5)))
            .unwrap();
        let highlight = list.highlight_semantic_prompt(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(0, 5), Coordinate::new(2, 5)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_prompt_scans_from_at_x() {
        let mut list = PageList::init(4, 2, None).unwrap();
        set_screen_semantic_prompt(&mut list, 0, SemanticPrompt::Prompt);
        set_screen_cell_semantic(&mut list, 0, 0, 'o', SemanticContent::Output);
        set_screen_cell_semantic(&mut list, 1, 0, 'p', SemanticContent::Prompt);
        set_screen_cell_semantic(&mut list, 2, 0, 'i', SemanticContent::Input);
        set_screen_cell_semantic(&mut list, 3, 0, 'i', SemanticContent::Input);
        set_screen_semantic_prompt(&mut list, 1, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(1, 0)))
            .unwrap();
        let highlight = list.highlight_semantic_prompt(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(0, 0), Coordinate::new(3, 0)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_prompt_cross_page_zone() {
        let mut list = PageList::init(4, 4, None).unwrap();
        list.split(Pin {
            node: list.first_node_ptr(),
            y: 2,
            x: 0,
            garbage: false,
        })
        .unwrap();
        set_screen_semantic_prompt(&mut list, 1, SemanticPrompt::Prompt);
        for x in 0..4 {
            set_screen_cell_semantic(&mut list, x, 1, 'p', SemanticContent::Prompt);
            set_screen_cell_semantic(&mut list, x, 2, 'i', SemanticContent::Input);
        }
        set_screen_semantic_prompt(&mut list, 3, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 1)))
            .unwrap();
        let highlight = list.highlight_semantic_prompt(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(0, 1), Coordinate::new(3, 2)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_prompt_to_screen_bottom() {
        let mut list = PageList::init(3, 2, None).unwrap();
        set_screen_semantic_prompt(&mut list, 0, SemanticPrompt::Prompt);
        for y in 0..2 {
            for x in 0..3 {
                set_screen_cell_semantic(&mut list, x, y, 'i', SemanticContent::Input);
            }
        }

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 0)))
            .unwrap();
        let highlight = list.highlight_semantic_prompt(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(0, 0), Coordinate::new(2, 1)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_prompt_is_untracked() {
        let mut list = PageList::init(4, 2, None).unwrap();
        set_screen_semantic_prompt(&mut list, 0, SemanticPrompt::Prompt);
        set_screen_cell_semantic(&mut list, 0, 0, '$', SemanticContent::Prompt);
        set_screen_semantic_prompt(&mut list, 1, SemanticPrompt::Prompt);
        let tracked_count = list.tracked_pins.len();

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 0)))
            .unwrap();
        let highlight = list.highlight_semantic_prompt(at).unwrap();

        assert_eq!(list.tracked_pins.len(), tracked_count);
        assert_ne!(NonNull::from(&highlight.start), list.tracked_pins[0]);
        assert_ne!(NonNull::from(&highlight.end), list.tracked_pins[0]);
    }

    #[test]
    fn page_list_highlight_semantic_input_basic() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..3 {
            set_screen_cell_semantic(&mut list, x, 5, '$', SemanticContent::Prompt);
        }
        for x in 3..8 {
            set_screen_cell_semantic(&mut list, x, 5, 'i', SemanticContent::Input);
        }
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 5)))
            .unwrap();
        let highlight = list.highlight_semantic_input(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(3, 5), Coordinate::new(7, 5)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_input_stops_before_output() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..2 {
            set_screen_cell_semantic(&mut list, x, 5, '$', SemanticContent::Prompt);
        }
        for x in 2..5 {
            set_screen_cell_semantic(&mut list, x, 5, 'i', SemanticContent::Input);
        }
        for x in 5..10 {
            set_screen_cell_semantic(&mut list, x, 5, 'o', SemanticContent::Output);
        }
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 5)))
            .unwrap();
        let highlight = list.highlight_semantic_input(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(2, 5), Coordinate::new(4, 5)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_input_multiline_with_nested_prompt() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..2 {
            set_screen_cell_semantic(&mut list, x, 5, '$', SemanticContent::Prompt);
        }
        for x in 2..10 {
            set_screen_cell_semantic(&mut list, x, 5, 'i', SemanticContent::Input);
        }
        for x in 0..2 {
            set_screen_cell_semantic(&mut list, x, 6, '>', SemanticContent::Prompt);
        }
        for x in 2..6 {
            set_screen_cell_semantic(&mut list, x, 6, 'i', SemanticContent::Input);
        }
        set_screen_cell_semantic(&mut list, 6, 6, 'o', SemanticContent::Output);
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 5)))
            .unwrap();
        let highlight = list.highlight_semantic_input(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(2, 5), Coordinate::new(5, 6)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_input_no_input_before_output_returns_none() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..3 {
            set_screen_cell_semantic(&mut list, x, 5, '$', SemanticContent::Prompt);
        }
        set_screen_cell_semantic(&mut list, 3, 5, 'o', SemanticContent::Output);
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 5)))
            .unwrap();

        assert!(list.highlight_semantic_input(at).is_none());
    }

    #[test]
    fn page_list_highlight_semantic_input_to_screen_bottom() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 15, SemanticPrompt::Prompt);
        for x in 0..2 {
            set_screen_cell_semantic(&mut list, x, 15, '$', SemanticContent::Prompt);
        }
        for x in 2..7 {
            set_screen_cell_semantic(&mut list, x, 15, 'i', SemanticContent::Input);
        }

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 15)))
            .unwrap();
        let highlight = list.highlight_semantic_input(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(2, 15), Coordinate::new(6, 15)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_input_prompt_only_returns_none() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..10 {
            set_screen_cell_semantic(&mut list, x, 5, '$', SemanticContent::Prompt);
        }
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 5)))
            .unwrap();

        assert!(list.highlight_semantic_input(at).is_none());
    }

    #[test]
    fn page_list_highlight_semantic_input_scans_from_at_x() {
        let mut list = PageList::init(4, 2, None).unwrap();
        set_screen_semantic_prompt(&mut list, 0, SemanticPrompt::Prompt);
        set_screen_cell_semantic(&mut list, 0, 0, 'o', SemanticContent::Output);
        set_screen_cell_semantic(&mut list, 1, 0, 'i', SemanticContent::Input);
        set_screen_cell_semantic(&mut list, 2, 0, 'i', SemanticContent::Input);
        set_screen_cell_semantic(&mut list, 3, 0, 'i', SemanticContent::Input);
        set_screen_semantic_prompt(&mut list, 1, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(1, 0)))
            .unwrap();
        let highlight = list.highlight_semantic_input(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(1, 0), Coordinate::new(3, 0)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_input_cross_page_zone() {
        let mut list = PageList::init(4, 4, None).unwrap();
        list.split(Pin {
            node: list.first_node_ptr(),
            y: 2,
            x: 0,
            garbage: false,
        })
        .unwrap();
        set_screen_semantic_prompt(&mut list, 1, SemanticPrompt::Prompt);
        for x in 0..2 {
            set_screen_cell_semantic(&mut list, x, 1, '$', SemanticContent::Prompt);
        }
        for x in 2..4 {
            set_screen_cell_semantic(&mut list, x, 1, 'i', SemanticContent::Input);
        }
        for x in 0..3 {
            set_screen_cell_semantic(&mut list, x, 2, 'i', SemanticContent::Input);
        }
        set_screen_cell_semantic(&mut list, 3, 2, 'o', SemanticContent::Output);
        set_screen_semantic_prompt(&mut list, 3, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 1)))
            .unwrap();
        let highlight = list.highlight_semantic_input(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(2, 1), Coordinate::new(2, 2)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_input_is_untracked() {
        let mut list = PageList::init(4, 2, None).unwrap();
        set_screen_semantic_prompt(&mut list, 0, SemanticPrompt::Prompt);
        set_screen_cell_semantic(&mut list, 0, 0, '$', SemanticContent::Prompt);
        set_screen_cell_semantic(&mut list, 1, 0, 'i', SemanticContent::Input);
        set_screen_semantic_prompt(&mut list, 1, SemanticPrompt::Prompt);
        let tracked_count = list.tracked_pins.len();

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 0)))
            .unwrap();
        let highlight = list.highlight_semantic_input(at).unwrap();

        assert_eq!(list.tracked_pins.len(), tracked_count);
        assert_ne!(NonNull::from(&highlight.start), list.tracked_pins[0]);
        assert_ne!(NonNull::from(&highlight.end), list.tracked_pins[0]);
    }

    #[test]
    fn page_list_highlight_semantic_output_basic() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..2 {
            set_screen_cell_semantic(&mut list, x, 5, '$', SemanticContent::Prompt);
        }
        for x in 2..5 {
            set_screen_cell_semantic(&mut list, x, 5, 'i', SemanticContent::Input);
        }
        for x in 5..8 {
            set_screen_cell_semantic(&mut list, x, 5, 'o', SemanticContent::Output);
        }
        for x in 8..10 {
            set_screen_cell_semantic(&mut list, x, 5, '$', SemanticContent::Prompt);
        }
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 5)))
            .unwrap();
        let highlight = list.highlight_semantic_output(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(5, 5), Coordinate::new(7, 5)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_output_multiline() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..2 {
            set_screen_cell_semantic(&mut list, x, 5, '$', SemanticContent::Prompt);
        }
        for x in 2..4 {
            set_screen_cell_semantic(&mut list, x, 5, 'i', SemanticContent::Input);
        }
        for x in 4..10 {
            set_screen_cell_semantic(&mut list, x, 5, 'o', SemanticContent::Output);
        }
        for x in 0..10 {
            set_screen_cell_semantic(&mut list, x, 6, 'o', SemanticContent::Output);
        }
        for x in 0..5 {
            set_screen_cell_semantic(&mut list, x, 7, 'o', SemanticContent::Output);
        }
        for x in 5..10 {
            set_screen_cell_semantic(&mut list, x, 7, 'i', SemanticContent::Input);
        }
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 5)))
            .unwrap();
        let highlight = list.highlight_semantic_output(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(4, 5), Coordinate::new(4, 7)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_output_stops_at_next_prompt() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..2 {
            set_screen_cell_semantic(&mut list, x, 5, '$', SemanticContent::Prompt);
        }
        for x in 2..4 {
            set_screen_cell_semantic(&mut list, x, 5, 'i', SemanticContent::Input);
        }
        for x in 4..10 {
            set_screen_cell_semantic(&mut list, x, 5, 'o', SemanticContent::Output);
        }
        for x in 0..3 {
            set_screen_cell_semantic(&mut list, x, 6, 'o', SemanticContent::Output);
        }
        for x in 3..6 {
            set_screen_cell_semantic(&mut list, x, 6, '$', SemanticContent::Prompt);
        }
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 5)))
            .unwrap();
        let highlight = list.highlight_semantic_output(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(4, 5), Coordinate::new(2, 6)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_output_to_screen_bottom() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 15, SemanticPrompt::Prompt);
        for x in 0..2 {
            set_screen_cell_semantic(&mut list, x, 15, '$', SemanticContent::Prompt);
        }
        for x in 2..4 {
            set_screen_cell_semantic(&mut list, x, 15, 'i', SemanticContent::Input);
        }
        for x in 4..10 {
            set_screen_cell_semantic(&mut list, x, 15, 'o', SemanticContent::Output);
        }
        for x in 0..8 {
            set_screen_cell_semantic(&mut list, x, 16, 'o', SemanticContent::Output);
        }
        for x in 8..10 {
            set_screen_cell_semantic(&mut list, x, 16, '$', SemanticContent::Prompt);
        }

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 15)))
            .unwrap();
        let highlight = list.highlight_semantic_output(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(4, 15), Coordinate::new(7, 16)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_output_no_output_returns_none() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..3 {
            set_screen_cell_semantic(&mut list, x, 5, '$', SemanticContent::Prompt);
        }
        for x in 3..10 {
            set_screen_cell_semantic(&mut list, x, 5, 'i', SemanticContent::Input);
        }
        for y in 6..10 {
            for x in 0..10 {
                set_screen_cell_semantic(&mut list, x, y, 'i', SemanticContent::Input);
            }
        }
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 5)))
            .unwrap();

        assert!(list.highlight_semantic_output(at).is_none());
    }

    #[test]
    fn page_list_highlight_semantic_output_skips_empty_cells_before_start() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..3 {
            set_screen_cell_semantic(&mut list, x, 5, '$', SemanticContent::Prompt);
        }
        for x in 0..4 {
            set_screen_cell_semantic(&mut list, x, 6, 'i', SemanticContent::Input);
        }
        for y in 7..9 {
            for x in 0..5 {
                set_screen_cell_semantic(&mut list, x, y, 'o', SemanticContent::Output);
            }
        }
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 5)))
            .unwrap();
        let highlight = list.highlight_semantic_output(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(0, 7), Coordinate::new(4, 8)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_output_empty_cells_inside_range() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..2 {
            set_screen_cell_semantic(&mut list, x, 5, '$', SemanticContent::Prompt);
        }
        for x in 2..4 {
            set_screen_cell_semantic(&mut list, x, 5, 'i', SemanticContent::Input);
        }
        for x in 4..6 {
            set_screen_cell_semantic(&mut list, x, 5, 'o', SemanticContent::Output);
        }
        for x in 8..10 {
            set_screen_cell_semantic(&mut list, x, 5, 'o', SemanticContent::Output);
        }
        set_screen_cell_semantic(&mut list, 0, 6, 'i', SemanticContent::Input);
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 5)))
            .unwrap();
        let highlight = list.highlight_semantic_output(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(4, 5), Coordinate::new(9, 5)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_output_scans_from_at_x() {
        let mut list = PageList::init(5, 2, None).unwrap();
        set_screen_semantic_prompt(&mut list, 0, SemanticPrompt::Prompt);
        set_screen_cell_semantic(&mut list, 0, 0, '$', SemanticContent::Prompt);
        set_screen_cell_semantic(&mut list, 1, 0, 'i', SemanticContent::Input);
        set_screen_cell_semantic(&mut list, 2, 0, 'o', SemanticContent::Output);
        set_screen_cell_semantic(&mut list, 3, 0, 'o', SemanticContent::Output);
        set_screen_cell_semantic(&mut list, 4, 0, 'o', SemanticContent::Output);
        set_screen_semantic_prompt(&mut list, 1, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(2, 0)))
            .unwrap();
        let highlight = list.highlight_semantic_output(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(2, 0), Coordinate::new(4, 0)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_output_cross_page_zone() {
        let mut list = PageList::init(4, 4, None).unwrap();
        list.split(Pin {
            node: list.first_node_ptr(),
            y: 2,
            x: 0,
            garbage: false,
        })
        .unwrap();
        set_screen_semantic_prompt(&mut list, 1, SemanticPrompt::Prompt);
        for x in 0..2 {
            set_screen_cell_semantic(&mut list, x, 1, '$', SemanticContent::Prompt);
        }
        for x in 2..4 {
            set_screen_cell_semantic(&mut list, x, 1, 'o', SemanticContent::Output);
        }
        for x in 0..3 {
            set_screen_cell_semantic(&mut list, x, 2, 'o', SemanticContent::Output);
        }
        set_screen_cell_semantic(&mut list, 3, 2, 'i', SemanticContent::Input);
        set_screen_semantic_prompt(&mut list, 3, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 1)))
            .unwrap();
        let highlight = list.highlight_semantic_output(at).unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(2, 1), Coordinate::new(2, 2)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_output_is_untracked() {
        let mut list = PageList::init(4, 2, None).unwrap();
        set_screen_semantic_prompt(&mut list, 0, SemanticPrompt::Prompt);
        set_screen_cell_semantic(&mut list, 0, 0, '$', SemanticContent::Prompt);
        set_screen_cell_semantic(&mut list, 1, 0, 'o', SemanticContent::Output);
        set_screen_semantic_prompt(&mut list, 1, SemanticPrompt::Prompt);
        let tracked_count = list.tracked_pins.len();

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 0)))
            .unwrap();
        let highlight = list.highlight_semantic_output(at).unwrap();

        assert_eq!(list.tracked_pins.len(), tracked_count);
        assert_ne!(NonNull::from(&highlight.start), list.tracked_pins[0]);
        assert_ne!(NonNull::from(&highlight.end), list.tracked_pins[0]);
    }

    #[test]
    fn page_list_highlight_semantic_content_dispatches_prompt_input_output() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..2 {
            set_screen_cell_semantic(&mut list, x, 5, '$', SemanticContent::Prompt);
        }
        for x in 2..5 {
            set_screen_cell_semantic(&mut list, x, 5, 'i', SemanticContent::Input);
        }
        for x in 5..8 {
            set_screen_cell_semantic(&mut list, x, 5, 'o', SemanticContent::Output);
        }
        set_screen_cell_semantic(&mut list, 8, 5, '$', SemanticContent::Prompt);
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 5)))
            .unwrap();

        assert_eq!(
            highlight_screen_points(
                &list,
                list.highlight_semantic_content(at, SemanticContent::Prompt)
                    .unwrap()
            ),
            [Coordinate::new(0, 5), Coordinate::new(4, 5)]
        );
        assert_eq!(
            highlight_screen_points(
                &list,
                list.highlight_semantic_content(at, SemanticContent::Input)
                    .unwrap()
            ),
            [Coordinate::new(2, 5), Coordinate::new(4, 5)]
        );
        assert_eq!(
            highlight_screen_points(
                &list,
                list.highlight_semantic_content(at, SemanticContent::Output)
                    .unwrap()
            ),
            [Coordinate::new(5, 5), Coordinate::new(7, 5)]
        );
    }

    #[test]
    fn page_list_highlight_semantic_content_preserves_none_results() {
        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..3 {
            set_screen_cell_semantic(&mut list, x, 5, '$', SemanticContent::Prompt);
        }
        set_screen_cell_semantic(&mut list, 3, 5, 'o', SemanticContent::Output);
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 5)))
            .unwrap();

        assert!(list
            .highlight_semantic_content(at, SemanticContent::Input)
            .is_none());

        let mut list = PageList::init(10, 20, None).unwrap();
        set_screen_semantic_prompt(&mut list, 5, SemanticPrompt::Prompt);
        for x in 0..10 {
            set_screen_cell_semantic(&mut list, x, 5, 'i', SemanticContent::Input);
        }
        set_screen_semantic_prompt(&mut list, 10, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(0, 5)))
            .unwrap();

        assert!(list
            .highlight_semantic_content(at, SemanticContent::Output)
            .is_none());
    }

    #[test]
    fn page_list_highlight_semantic_content_scans_from_at_x() {
        let mut list = PageList::init(5, 2, None).unwrap();
        set_screen_semantic_prompt(&mut list, 0, SemanticPrompt::Prompt);
        set_screen_cell_semantic(&mut list, 0, 0, 'o', SemanticContent::Output);
        set_screen_cell_semantic(&mut list, 1, 0, '$', SemanticContent::Prompt);
        set_screen_cell_semantic(&mut list, 2, 0, 'i', SemanticContent::Input);
        set_screen_cell_semantic(&mut list, 3, 0, 'i', SemanticContent::Input);
        set_screen_cell_semantic(&mut list, 4, 0, 'o', SemanticContent::Output);
        set_screen_semantic_prompt(&mut list, 1, SemanticPrompt::Prompt);

        let at = list
            .pin(point::Point::screen(Coordinate::new(1, 0)))
            .unwrap();
        let highlight = list
            .highlight_semantic_content(at, SemanticContent::Input)
            .unwrap();

        assert_eq!(
            highlight_screen_points(&list, highlight),
            [Coordinate::new(2, 0), Coordinate::new(3, 0)]
        );
    }

    #[test]
    fn flattened_highlight_empty_has_no_chunks_and_zero_bounds() {
        let highlight = highlight::Flattened::empty();

        assert!(highlight.chunks.is_empty());
        assert_eq!(highlight.top_x, 0);
        assert_eq!(highlight.bot_x, 0);
    }

    #[test]
    fn flattened_highlight_start_end_and_untracked_pins() {
        let mut list = PageList::init(4, 4, None).unwrap();
        list.split(Pin {
            node: list.first_node_ptr(),
            y: 2,
            x: 0,
            garbage: false,
        })
        .unwrap();

        let first = list.first_node_ptr();
        let last = list.last_node_ptr();
        let first_serial = list.node_for_ptr(first).unwrap().serial;
        let last_serial = list.node_for_ptr(last).unwrap().serial;
        let flattened = highlight::Flattened {
            chunks: vec![
                highlight::Chunk {
                    node: first,
                    serial: first_serial,
                    start: 1,
                    end: 2,
                },
                highlight::Chunk {
                    node: last,
                    serial: last_serial,
                    start: 0,
                    end: 1,
                },
            ],
            top_x: 2,
            bot_x: 3,
        };

        let start = flattened.start_pin();
        let end = flattened.end_pin();
        let untracked = flattened.untracked();

        assert_eq!(start, untracked.start);
        assert_eq!(end, untracked.end);
        assert_eq!(
            list.point_from_pin(point::Tag::Screen, start)
                .unwrap()
                .coord(),
            Coordinate::new(2, 1)
        );
        assert_eq!(
            list.point_from_pin(point::Tag::Screen, end)
                .unwrap()
                .coord(),
            Coordinate::new(3, 2)
        );
        assert!(!start.garbage);
        assert!(!end.garbage);
    }

    #[test]
    fn flattened_highlight_clone_preserves_chunks_and_bounds() {
        let list = PageList::init(4, 2, None).unwrap();
        let node = list.first_node_ptr();
        let serial = list.node_for_ptr(node).unwrap().serial;
        let flattened = highlight::Flattened {
            chunks: vec![highlight::Chunk {
                node,
                serial,
                start: 0,
                end: 1,
            }],
            top_x: 1,
            bot_x: 2,
        };

        assert_eq!(flattened.clone(), flattened);
    }

    #[test]
    #[should_panic(expected = "flattened highlight must contain at least one chunk")]
    fn flattened_highlight_empty_start_pin_panics() {
        let _ = highlight::Flattened::empty().start_pin();
    }

    #[test]
    #[should_panic(expected = "flattened highlight must contain at least one chunk")]
    fn flattened_highlight_empty_end_pin_panics() {
        let _ = highlight::Flattened::empty().end_pin();
    }

    #[test]
    #[should_panic(expected = "flattened highlight must contain at least one chunk")]
    fn flattened_highlight_empty_untracked_panics() {
        let _ = highlight::Flattened::empty().untracked();
    }

    #[test]
    fn page_list_flatten_highlight_single_page() {
        let list = PageList::init(10, 20, None).unwrap();
        let start = list
            .pin(point::Point::screen(Coordinate::new(2, 5)))
            .unwrap();
        let end = list
            .pin(point::Point::screen(Coordinate::new(7, 5)))
            .unwrap();
        let node = list.first_node_ptr();
        let serial = list.node_for_ptr(node).unwrap().serial;

        let flattened = list.flatten_highlight(start, end).unwrap();

        assert_eq!(
            flattened_chunk_tuples(&list, &flattened),
            vec![(0, serial, 5, 6)]
        );
        assert_eq!(flattened.top_x, 2);
        assert_eq!(flattened.bot_x, 7);
        assert_eq!(flattened.start_pin(), start);
        assert_eq!(flattened.end_pin(), end);
        assert_eq!(flattened.untracked(), highlight::Untracked { start, end });
    }

    #[test]
    fn page_list_flatten_highlight_cross_page() {
        let mut list = PageList::init(4, 4, None).unwrap();
        let split = list
            .pin(point::Point::screen(Coordinate::new(0, 2)))
            .unwrap();
        list.split(split).unwrap();
        let start = list
            .pin(point::Point::screen(Coordinate::new(1, 1)))
            .unwrap();
        let end = list
            .pin(point::Point::screen(Coordinate::new(3, 2)))
            .unwrap();
        let first_serial = list.node_for_ptr(list.first_node_ptr()).unwrap().serial;
        let last_serial = list.node_for_ptr(list.last_node_ptr()).unwrap().serial;

        let flattened = list.flatten_highlight(start, end).unwrap();

        assert_eq!(
            flattened_chunk_tuples(&list, &flattened),
            vec![(0, first_serial, 1, 2), (1, last_serial, 0, 1)]
        );
        assert_eq!(flattened.top_x, 1);
        assert_eq!(flattened.bot_x, 3);
        assert_eq!(
            list.point_from_pin(point::Tag::Screen, flattened.start_pin())
                .unwrap()
                .coord(),
            Coordinate::new(1, 1)
        );
        assert_eq!(
            list.point_from_pin(point::Tag::Screen, flattened.end_pin())
                .unwrap()
                .coord(),
            Coordinate::new(3, 2)
        );
        assert_eq!(flattened.untracked(), highlight::Untracked { start, end });
    }

    #[test]
    fn page_list_flatten_highlight_same_page_reversed_returns_none() {
        let list = PageList::init(10, 20, None).unwrap();
        let start = list
            .pin(point::Point::screen(Coordinate::new(2, 5)))
            .unwrap();
        let same_row_end = list
            .pin(point::Point::screen(Coordinate::new(1, 5)))
            .unwrap();
        let prior_row_end = list
            .pin(point::Point::screen(Coordinate::new(9, 4)))
            .unwrap();

        assert!(list.flatten_highlight(start, same_row_end).is_none());
        assert!(list.flatten_highlight(start, prior_row_end).is_none());
    }

    #[test]
    fn page_list_flatten_highlight_cross_page_reversed_returns_none() {
        let mut list = PageList::init(4, 4, None).unwrap();
        let split = list
            .pin(point::Point::screen(Coordinate::new(0, 2)))
            .unwrap();
        list.split(split).unwrap();
        let start = list
            .pin(point::Point::screen(Coordinate::new(0, 2)))
            .unwrap();
        let end = list
            .pin(point::Point::screen(Coordinate::new(3, 1)))
            .unwrap();

        assert!(list.flatten_highlight(start, end).is_none());
    }

    #[test]
    fn page_list_flatten_highlight_garbage_pin_returns_none() {
        let list = PageList::init(10, 20, None).unwrap();
        let mut start = list
            .pin(point::Point::screen(Coordinate::new(2, 5)))
            .unwrap();
        let end = list
            .pin(point::Point::screen(Coordinate::new(7, 5)))
            .unwrap();

        start.garbage = true;

        assert!(list.flatten_highlight(start, end).is_none());
        assert!(list.flatten_highlight(end, start).is_none());
    }

    #[test]
    fn page_list_flatten_highlight_missing_node_returns_none() {
        let list = PageList::init(10, 20, None).unwrap();
        let other = PageList::init(10, 20, None).unwrap();
        let start = Pin::new(other.first_node_ptr(), 0, 0);
        let end = list
            .pin(point::Point::screen(Coordinate::new(7, 5)))
            .unwrap();

        assert!(list.flatten_highlight(start, end).is_none());
        assert!(list.flatten_highlight(end, start).is_none());
    }

    #[test]
    fn page_list_flatten_highlight_out_of_bounds_row_returns_none() {
        let list = PageList::init(10, 20, None).unwrap();
        let node = list.first_node_ptr();
        let rows = list.node_for_ptr(node).unwrap().page.size_rows();
        let start = Pin::new(node, rows, 0);
        let end = list
            .pin(point::Point::screen(Coordinate::new(7, 5)))
            .unwrap();

        assert!(list.flatten_highlight(start, end).is_none());
        assert!(list.flatten_highlight(end, start).is_none());
    }

    #[test]
    fn page_list_flatten_highlight_out_of_bounds_column_returns_none() {
        let list = PageList::init(10, 20, None).unwrap();
        let node = list.first_node_ptr();
        let start = Pin::new(node, 0, list.cols);
        let end = list
            .pin(point::Point::screen(Coordinate::new(7, 5)))
            .unwrap();

        assert!(list.flatten_highlight(start, end).is_none());
        assert!(list.flatten_highlight(end, start).is_none());
    }

    #[test]
    fn tracked_highlight_init_assume_wraps_pointer_identity() {
        let mut list = PageList::init(10, 20, None).unwrap();
        let start = list
            .pin(point::Point::screen(Coordinate::new(2, 5)))
            .unwrap();
        let end = list
            .pin(point::Point::screen(Coordinate::new(7, 5)))
            .unwrap();
        let start_ptr = list.track_pin(start).unwrap();
        let end_ptr = list.track_pin(end).unwrap();
        let count = list.count_tracked_pins();

        let tracked = highlight::Tracked::init_assume(start_ptr, end_ptr);

        assert_eq!(tracked.start, start_ptr);
        assert_eq!(tracked.end, end_ptr);
        assert_eq!(list.count_tracked_pins(), count);

        list.untrack_pin(start_ptr);
        list.untrack_pin(end_ptr);
    }

    #[test]
    fn selection_tracked_wraps_page_list_pins_without_ownership_change() {
        let mut list = PageList::init(10, 20, None).unwrap();
        let start = list
            .pin(point::Point::screen(Coordinate::new(2, 5)))
            .unwrap();
        let end = list
            .pin(point::Point::screen(Coordinate::new(7, 5)))
            .unwrap();
        let start_ptr = list.track_pin(start).unwrap();
        let end_ptr = list.track_pin(end).unwrap();
        let count = list.count_tracked_pins();

        let selection = selection::Selection::tracked(start_ptr, end_ptr, false);

        assert!(selection.is_tracked());
        assert_eq!(selection.start(), start);
        assert_eq!(selection.end(), end);
        assert_eq!(list.count_tracked_pins(), count);

        list.untrack_pin(start_ptr);
        list.untrack_pin(end_ptr);
    }

    #[test]
    fn selection_order_regular_uses_screen_coordinates() {
        let list = PageList::init(10, 20, None).unwrap();

        assert_eq!(
            list.selection_order(screen_selection(&list, (1, 2), (8, 5), false)),
            Some(selection::Order::Forward)
        );
        assert_eq!(
            list.selection_order(screen_selection(&list, (8, 5), (1, 2), false)),
            Some(selection::Order::Reverse)
        );
        assert_eq!(
            list.selection_order(screen_selection(&list, (1, 5), (8, 5), false)),
            Some(selection::Order::Forward)
        );
        assert_eq!(
            list.selection_order(screen_selection(&list, (4, 5), (4, 5), false)),
            Some(selection::Order::Forward)
        );
        assert_eq!(
            list.selection_order(screen_selection(&list, (8, 5), (1, 5), false)),
            Some(selection::Order::Reverse)
        );
    }

    #[test]
    fn selection_order_rectangle_matches_upstream_edge_cases() {
        let list = PageList::init(10, 20, None).unwrap();

        assert_eq!(
            list.selection_order(screen_selection(&list, (1, 2), (8, 5), true)),
            Some(selection::Order::Forward)
        );
        assert_eq!(
            list.selection_order(screen_selection(&list, (8, 5), (1, 2), true)),
            Some(selection::Order::Reverse)
        );
        assert_eq!(
            list.selection_order(screen_selection(&list, (8, 2), (1, 5), true)),
            Some(selection::Order::MirroredForward)
        );
        assert_eq!(
            list.selection_order(screen_selection(&list, (1, 5), (8, 2), true)),
            Some(selection::Order::MirroredReverse)
        );
        assert_eq!(
            list.selection_order(screen_selection(&list, (1, 5), (8, 5), true)),
            Some(selection::Order::Forward)
        );
        assert_eq!(
            list.selection_order(screen_selection(&list, (8, 5), (1, 5), true)),
            Some(selection::Order::Reverse)
        );
        assert_eq!(
            list.selection_order(screen_selection(&list, (4, 2), (4, 5), true)),
            Some(selection::Order::Forward)
        );
        assert_eq!(
            list.selection_order(screen_selection(&list, (4, 5), (4, 2), true)),
            Some(selection::Order::Reverse)
        );
        assert_eq!(
            list.selection_order(screen_selection(&list, (4, 5), (4, 5), true)),
            Some(selection::Order::Forward)
        );
    }

    #[test]
    fn selection_top_left_normalizes_rectangle_orders() {
        let list = PageList::init(10, 20, None).unwrap();

        let cases = [
            (screen_selection(&list, (1, 2), (8, 5), true), (1, 2)),
            (screen_selection(&list, (8, 5), (1, 2), true), (1, 2)),
            (screen_selection(&list, (8, 2), (1, 5), true), (1, 2)),
            (screen_selection(&list, (1, 5), (8, 2), true), (1, 2)),
        ];

        for (selection, expected) in cases {
            assert_eq!(
                screen_coord(&list, list.selection_top_left(selection).unwrap()),
                Coordinate::new(expected.0, expected.1)
            );
        }
    }

    #[test]
    fn selection_bottom_right_normalizes_rectangle_orders() {
        let list = PageList::init(10, 20, None).unwrap();

        let cases = [
            (screen_selection(&list, (1, 2), (8, 5), true), (8, 5)),
            (screen_selection(&list, (8, 5), (1, 2), true), (8, 5)),
            (screen_selection(&list, (8, 2), (1, 5), true), (8, 5)),
            (screen_selection(&list, (1, 5), (8, 2), true), (8, 5)),
        ];

        for (selection, expected) in cases {
            assert_eq!(
                screen_coord(&list, list.selection_bottom_right(selection).unwrap()),
                Coordinate::new(expected.0, expected.1)
            );
        }
    }

    #[test]
    fn selection_ordered_returns_untracked_forward_and_reverse() {
        let list = PageList::init(10, 20, None).unwrap();
        let selection = screen_selection(&list, (8, 5), (1, 2), true);

        let forward = list
            .selection_ordered(selection, selection::Order::Forward)
            .unwrap();
        assert!(!forward.is_tracked());
        assert_eq!(screen_coord(&list, forward.start()), Coordinate::new(1, 2));
        assert_eq!(screen_coord(&list, forward.end()), Coordinate::new(8, 5));
        assert!(forward.rectangle());

        let reverse = list
            .selection_ordered(forward, selection::Order::Reverse)
            .unwrap();
        assert!(!reverse.is_tracked());
        assert_eq!(screen_coord(&list, reverse.start()), Coordinate::new(8, 5));
        assert_eq!(screen_coord(&list, reverse.end()), Coordinate::new(1, 2));
        assert!(reverse.rectangle());
    }

    #[test]
    fn selection_ordered_preserves_matching_mirrored_order() {
        let list = PageList::init(10, 20, None).unwrap();
        let selection = screen_selection(&list, (8, 2), (1, 5), true);

        let ordered = list
            .selection_ordered(selection, selection::Order::MirroredForward)
            .unwrap();

        assert!(!ordered.is_tracked());
        assert_eq!(ordered.start(), selection.start());
        assert_eq!(ordered.end(), selection.end());
        assert!(ordered.rectangle());
    }

    #[test]
    fn selection_ordered_treats_nonmatching_mirrored_desired_order_as_forward() {
        let list = PageList::init(10, 20, None).unwrap();
        let selection = screen_selection(&list, (8, 2), (1, 5), true);

        let ordered = list
            .selection_ordered(selection, selection::Order::MirroredReverse)
            .unwrap();

        assert!(!ordered.is_tracked());
        assert_eq!(screen_coord(&list, ordered.start()), Coordinate::new(1, 2));
        assert_eq!(screen_coord(&list, ordered.end()), Coordinate::new(8, 5));
        assert!(ordered.rectangle());
    }

    #[test]
    fn selection_order_uses_screen_rows_across_pages() {
        let (list, page_rows) = multi_page_list(80);
        let selection = screen_selection(
            &list,
            (1, page_rows as u32 + 1),
            (2, page_rows as u32 - 1),
            false,
        );

        assert_ne!(
            row_tuple(&list, selection.start()).0,
            row_tuple(&list, selection.end()).0
        );
        assert_eq!(
            list.selection_order(selection),
            Some(selection::Order::Reverse)
        );
    }

    #[test]
    fn selection_helpers_return_none_for_unmappable_endpoints() {
        let list = PageList::init(10, 20, None).unwrap();
        let other = PageList::init(10, 20, None).unwrap();
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let valid = screen_pin(&list, 2, 5);

        for selection in [
            selection::Selection::new(invalid, valid, false),
            selection::Selection::new(valid, invalid, false),
        ] {
            assert!(list.selection_order(selection).is_none());
            assert!(list.selection_top_left(selection).is_none());
            assert!(list.selection_bottom_right(selection).is_none());
            assert!(list
                .selection_ordered(selection, selection::Order::Forward)
                .is_none());
        }
    }

    #[test]
    fn selection_contains_regular_matches_upstream_forward_and_reverse() {
        let list = PageList::init(10, 10, None).unwrap();
        for selection in [
            screen_selection(&list, (5, 1), (3, 2), false),
            screen_selection(&list, (3, 2), (5, 1), false),
        ] {
            assert_eq!(
                list.selection_contains(selection, screen_pin(&list, 6, 1)),
                Some(true)
            );
            assert_eq!(
                list.selection_contains(selection, screen_pin(&list, 1, 2)),
                Some(true)
            );
            assert_eq!(
                list.selection_contains(selection, screen_pin(&list, 1, 1)),
                Some(false)
            );
            assert_eq!(
                list.selection_contains(selection, screen_pin(&list, 5, 2)),
                Some(false)
            );
        }
    }

    #[test]
    fn selection_contains_regular_single_line_matches_upstream() {
        let list = PageList::init(10, 10, None).unwrap();
        let selection = screen_selection(&list, (5, 1), (8, 1), false);

        assert_eq!(
            list.selection_contains(selection, screen_pin(&list, 6, 1)),
            Some(true)
        );
        assert_eq!(
            list.selection_contains(selection, screen_pin(&list, 2, 1)),
            Some(false)
        );
        assert_eq!(
            list.selection_contains(selection, screen_pin(&list, 9, 1)),
            Some(false)
        );
    }

    #[test]
    fn selection_contains_rectangle_matches_upstream_forward_and_reverse() {
        let list = PageList::init(15, 15, None).unwrap();
        for selection in [
            screen_selection(&list, (3, 3), (7, 9), true),
            screen_selection(&list, (7, 9), (3, 3), true),
        ] {
            for point in [(5, 6), (3, 6), (7, 6), (5, 3), (5, 9)] {
                assert_eq!(
                    list.selection_contains(selection, screen_pin(&list, point.0, point.1)),
                    Some(true),
                    "expected {:?} to be contained",
                    point
                );
            }

            for point in [(5, 2), (5, 10), (2, 6), (8, 6), (8, 3), (2, 9)] {
                assert_eq!(
                    list.selection_contains(selection, screen_pin(&list, point.0, point.1)),
                    Some(false),
                    "expected {:?} to be excluded",
                    point
                );
            }
        }
    }

    #[test]
    fn selection_contains_rectangle_single_line_matches_upstream() {
        let list = PageList::init(15, 15, None).unwrap();
        let selection = screen_selection(&list, (5, 1), (10, 1), true);

        assert_eq!(
            list.selection_contains(selection, screen_pin(&list, 6, 1)),
            Some(true)
        );
        assert_eq!(
            list.selection_contains(selection, screen_pin(&list, 2, 1)),
            Some(false)
        );
        assert_eq!(
            list.selection_contains(selection, screen_pin(&list, 12, 1)),
            Some(false)
        );
    }

    #[test]
    fn selection_contains_mirrored_rectangles_after_normalization() {
        let list = PageList::init(15, 15, None).unwrap();
        for selection in [
            screen_selection(&list, (7, 3), (3, 9), true),
            screen_selection(&list, (3, 9), (7, 3), true),
        ] {
            for point in [(5, 6), (3, 6), (7, 6), (5, 3), (5, 9)] {
                assert_eq!(
                    list.selection_contains(selection, screen_pin(&list, point.0, point.1)),
                    Some(true),
                    "expected {:?} to be contained",
                    point
                );
            }

            for point in [(5, 2), (5, 10), (2, 6), (8, 6)] {
                assert_eq!(
                    list.selection_contains(selection, screen_pin(&list, point.0, point.1)),
                    Some(false),
                    "expected {:?} to be excluded",
                    point
                );
            }
        }
    }

    #[test]
    fn selection_contains_uses_screen_rows_across_pages() {
        let (list, page_rows) = multi_page_list(80);
        let selection = screen_selection(
            &list,
            (5, page_rows as u32 - 1),
            (3, page_rows as u32 + 1),
            false,
        );

        assert_ne!(
            row_tuple(&list, selection.start()).0,
            row_tuple(&list, selection.end()).0
        );
        assert_eq!(
            list.selection_contains(selection, screen_pin(&list, 6, page_rows as u32 - 1)),
            Some(true)
        );
        assert_eq!(
            list.selection_contains(selection, screen_pin(&list, 4, page_rows as u32)),
            Some(true)
        );
        assert_eq!(
            list.selection_contains(selection, screen_pin(&list, 4, page_rows as u32 + 1)),
            Some(false)
        );
    }

    #[test]
    fn selection_contains_returns_none_for_invalid_selection_or_candidate_pins() {
        let list = PageList::init(10, 20, None).unwrap();
        let other = PageList::init(10, 20, None).unwrap();
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let valid = screen_pin(&list, 2, 5);
        let selection = selection::Selection::new(valid, screen_pin(&list, 4, 5), false);
        let mut garbage = valid;
        garbage.garbage = true;

        assert!(list
            .selection_contains(selection::Selection::new(invalid, valid, false), valid)
            .is_none());
        assert!(list
            .selection_contains(selection::Selection::new(valid, invalid, false), valid)
            .is_none());
        assert!(list.selection_contains(selection, invalid).is_none());
        assert!(list.selection_contains(selection, garbage).is_none());
    }

    #[test]
    fn selection_contained_row_regular_matches_upstream_rows() {
        let list = PageList::init(10, 5, None).unwrap();
        let selection = screen_selection(&list, (5, 1), (3, 3), false);

        assert!(list
            .selection_contained_row(selection, screen_pin(&list, 1, 4))
            .is_none());

        let top = list
            .selection_contained_row(selection, screen_pin(&list, 1, 1))
            .unwrap();
        assert_eq!(top.start(), selection.start());
        assert_eq!(top.end(), screen_pin(&list, list.cols - 1, 1));
        assert!(!top.rectangle());

        let bottom = list
            .selection_contained_row(selection, screen_pin(&list, 2, 3))
            .unwrap();
        assert_eq!(bottom.start(), screen_pin(&list, 0, 3));
        assert_eq!(bottom.end(), selection.end());
        assert!(!bottom.rectangle());

        let middle = list
            .selection_contained_row(selection, screen_pin(&list, 2, 2))
            .unwrap();
        assert_eq!(middle.start(), screen_pin(&list, 0, 2));
        assert_eq!(middle.end(), screen_pin(&list, list.cols - 1, 2));
        assert!(!middle.rectangle());
    }

    #[test]
    fn selection_contained_row_regular_reverse_normalizes_before_extraction() {
        let list = PageList::init(10, 5, None).unwrap();
        let selection = screen_selection(&list, (3, 3), (5, 1), false);

        let top = list
            .selection_contained_row(selection, screen_pin(&list, 1, 1))
            .unwrap();
        assert_eq!(top.start(), screen_pin(&list, 5, 1));
        assert_eq!(top.end(), screen_pin(&list, list.cols - 1, 1));
        assert!(!top.rectangle());

        let bottom = list
            .selection_contained_row(selection, screen_pin(&list, 2, 3))
            .unwrap();
        assert_eq!(bottom.start(), screen_pin(&list, 0, 3));
        assert_eq!(bottom.end(), screen_pin(&list, 3, 3));
        assert!(!bottom.rectangle());
    }

    #[test]
    fn selection_contained_row_rectangle_matches_upstream_rows() {
        let list = PageList::init(10, 5, None).unwrap();
        let selection = screen_selection(&list, (3, 1), (6, 3), true);

        assert!(list
            .selection_contained_row(selection, screen_pin(&list, 1, 4))
            .is_none());

        for y in [1, 2, 3] {
            let row = list
                .selection_contained_row(selection, screen_pin(&list, 1, y))
                .unwrap();
            assert_eq!(row.start(), screen_pin(&list, 3, y));
            assert_eq!(row.end(), screen_pin(&list, 6, y));
            assert!(row.rectangle());
        }
    }

    #[test]
    fn selection_contained_row_rectangle_reverse_and_mirrored_normalize() {
        let list = PageList::init(10, 5, None).unwrap();
        for selection in [
            screen_selection(&list, (6, 3), (3, 1), true),
            screen_selection(&list, (6, 1), (3, 3), true),
            screen_selection(&list, (3, 3), (6, 1), true),
        ] {
            let row = list
                .selection_contained_row(selection, screen_pin(&list, 1, 2))
                .unwrap();
            assert_eq!(row.start(), screen_pin(&list, 3, 2));
            assert_eq!(row.end(), screen_pin(&list, 6, 2));
            assert!(row.rectangle());
        }
    }

    #[test]
    fn selection_contained_row_regular_single_line_returns_normalized_original() {
        let list = PageList::init(10, 5, None).unwrap();
        for selection in [
            screen_selection(&list, (2, 1), (6, 1), false),
            screen_selection(&list, (6, 1), (2, 1), false),
        ] {
            assert!(list
                .selection_contained_row(selection, screen_pin(&list, 1, 0))
                .is_none());
            assert!(list
                .selection_contained_row(selection, screen_pin(&list, 1, 2))
                .is_none());

            let row = list
                .selection_contained_row(selection, screen_pin(&list, 1, 1))
                .unwrap();
            assert_eq!(row.start(), screen_pin(&list, 2, 1));
            assert_eq!(row.end(), screen_pin(&list, 6, 1));
            assert!(!row.rectangle());
        }
    }

    #[test]
    fn selection_contained_row_uses_screen_rows_across_pages() {
        let (list, page_rows) = multi_page_list(80);
        let selection = screen_selection(
            &list,
            (5, page_rows as u32 - 1),
            (3, page_rows as u32 + 1),
            false,
        );

        assert_ne!(
            row_tuple(&list, selection.start()).0,
            row_tuple(&list, selection.end()).0
        );

        let middle = list
            .selection_contained_row(selection, screen_pin(&list, 2, page_rows as u32))
            .unwrap();
        assert_eq!(middle.start(), screen_pin(&list, 0, page_rows as u32));
        assert_eq!(
            middle.end(),
            screen_pin(&list, list.cols - 1, page_rows as u32)
        );
        assert!(!middle.rectangle());
    }

    #[test]
    fn selection_contained_row_returns_none_for_invalid_inputs() {
        let list = PageList::init(10, 20, None).unwrap();
        let other = PageList::init(10, 20, None).unwrap();
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let valid = screen_pin(&list, 2, 5);
        let selection = selection::Selection::new(valid, screen_pin(&list, 4, 5), false);
        let mut garbage = valid;
        garbage.garbage = true;

        assert!(list
            .selection_contained_row(selection::Selection::new(invalid, valid, false), valid)
            .is_none());
        assert!(list
            .selection_contained_row(selection::Selection::new(valid, invalid, false), valid)
            .is_none());
        assert!(list.selection_contained_row(selection, invalid).is_none());
        assert!(list.selection_contained_row(selection, garbage).is_none());
    }

    #[test]
    fn selection_adjust_right_matches_upstream_cases() {
        let mut list = PageList::init(10, 10, None).unwrap();
        set_screen_text_lines(&mut list, &["A1234", "B5678", "C1234", "D5678"]);

        let mut selection = screen_selection(&list, (5, 1), (3, 3), false);
        list.selection_adjust(&mut selection, selection::Adjustment::Right)
            .unwrap();
        assert_selection_screen_points(&list, selection, (5, 1), (4, 3));

        let mut selection = screen_selection(&list, (4, 1), (4, 2), false);
        list.selection_adjust(&mut selection, selection::Adjustment::Right)
            .unwrap();
        assert_selection_screen_points(&list, selection, (4, 1), (0, 3));

        let mut selection = screen_selection(&list, (5, 1), (4, 3), false);
        list.selection_adjust(&mut selection, selection::Adjustment::Right)
            .unwrap();
        assert_selection_screen_points(&list, selection, (5, 1), (4, 3));
    }

    #[test]
    fn selection_adjust_left_matches_upstream_cases_and_skips_blanks() {
        let mut list = PageList::init(10, 10, None).unwrap();
        set_screen_text_lines(&mut list, &["A1234", "B5678", "C12", "D56"]);

        let mut selection = screen_selection(&list, (5, 1), (3, 3), false);
        list.selection_adjust(&mut selection, selection::Adjustment::Left)
            .unwrap();
        assert_selection_screen_points(&list, selection, (5, 1), (2, 3));

        let mut selection = screen_selection(&list, (5, 1), (0, 3), false);
        list.selection_adjust(&mut selection, selection::Adjustment::Left)
            .unwrap();
        assert_selection_screen_points(&list, selection, (5, 1), (2, 2));
    }

    #[test]
    fn selection_adjust_vertical_and_line_boundary_matches_upstream() {
        let mut list = PageList::init(10, 10, None).unwrap();
        set_screen_text_lines(&mut list, &["A", "B", "C", "D", "E"]);

        let mut selection = screen_selection(&list, (5, 1), (3, 3), false);
        list.selection_adjust(&mut selection, selection::Adjustment::Up)
            .unwrap();
        assert_selection_screen_points(&list, selection, (5, 1), (3, 2));

        let mut selection = screen_selection(&list, (5, 1), (3, 0), false);
        list.selection_adjust(&mut selection, selection::Adjustment::Up)
            .unwrap();
        assert_selection_screen_points(&list, selection, (5, 1), (0, 0));

        let mut selection = screen_selection(&list, (5, 1), (3, 3), false);
        list.selection_adjust(&mut selection, selection::Adjustment::Down)
            .unwrap();
        assert_selection_screen_points(&list, selection, (5, 1), (3, 4));

        let mut selection = screen_selection(&list, (4, 1), (3, 4), false);
        list.selection_adjust(&mut selection, selection::Adjustment::Down)
            .unwrap();
        assert_selection_screen_points(&list, selection, (4, 1), (9, 4));
    }

    #[test]
    fn selection_adjust_down_preserves_x_and_handles_not_full_screen() {
        let mut list = PageList::init(10, 10, None).unwrap();
        set_screen_text_lines(&mut list, &["A", "B", "C"]);

        let mut selection = screen_selection(&list, (4, 1), (3, 1), false);
        list.selection_adjust(&mut selection, selection::Adjustment::Down)
            .unwrap();
        assert_selection_screen_points(&list, selection, (4, 1), (3, 2));

        let mut selection = screen_selection(&list, (4, 1), (3, 2), false);
        list.selection_adjust(&mut selection, selection::Adjustment::Down)
            .unwrap();
        assert_selection_screen_points(&list, selection, (4, 1), (9, 2));
    }

    #[test]
    fn selection_adjust_page_up_and_page_down_move_or_fallback() {
        let mut list = PageList::init(10, 5, None).unwrap();
        simulate_history(&mut list, 12);
        set_screen_text_lines(&mut list, &["A", "B", "C", "D", "E", "F", "G", "H"]);

        let mut selection = screen_selection(&list, (4, 1), (3, 6), false);
        list.selection_adjust(&mut selection, selection::Adjustment::PageUp)
            .unwrap();
        assert_selection_screen_points(&list, selection, (4, 1), (3, 1));

        let mut selection = screen_selection(&list, (4, 1), (3, 2), false);
        list.selection_adjust(&mut selection, selection::Adjustment::PageUp)
            .unwrap();
        assert_selection_screen_points(&list, selection, (4, 1), (0, 0));

        let mut selection = screen_selection(&list, (4, 1), (3, 1), false);
        list.selection_adjust(&mut selection, selection::Adjustment::PageDown)
            .unwrap();
        assert_selection_screen_points(&list, selection, (4, 1), (3, 6));

        let mut selection = screen_selection(&list, (4, 1), (1, 8), false);
        list.selection_adjust(&mut selection, selection::Adjustment::PageDown)
            .unwrap();
        assert_selection_screen_points(&list, selection, (4, 1), (9, 7));
    }

    #[test]
    fn selection_adjust_home_end_and_line_edges_match_upstream() {
        let mut list = PageList::init(10, 10, None).unwrap();
        set_screen_text_lines(&mut list, &["A12 B34", "C12 D34", "E"]);

        let mut selection = screen_selection(&list, (4, 1), (1, 2), false);
        list.selection_adjust(&mut selection, selection::Adjustment::Home)
            .unwrap();
        assert_selection_screen_points(&list, selection, (4, 1), (0, 0));

        let mut selection = screen_selection(&list, (4, 0), (1, 1), false);
        list.selection_adjust(&mut selection, selection::Adjustment::End)
            .unwrap();
        assert_selection_screen_points(&list, selection, (4, 0), (9, 2));

        let mut selection = screen_selection(&list, (5, 1), (5, 1), false);
        list.selection_adjust(&mut selection, selection::Adjustment::BeginningOfLine)
            .unwrap();
        assert_selection_screen_points(&list, selection, (5, 1), (0, 1));

        let mut selection = screen_selection(&list, (1, 0), (1, 0), false);
        list.selection_adjust(&mut selection, selection::Adjustment::EndOfLine)
            .unwrap();
        assert_selection_screen_points(&list, selection, (1, 0), (9, 0));
    }

    #[test]
    fn selection_adjust_mutates_tracked_end_and_preserves_start() {
        let mut list = PageList::init(10, 10, None).unwrap();
        set_screen_text_lines(&mut list, &["A1234", "B5678", "C1234", "D5678"]);
        let start = screen_pin(&list, 5, 1);
        let end = screen_pin(&list, 3, 3);
        let start_ptr = list.track_pin(start).unwrap();
        let end_ptr = list.track_pin(end).unwrap();
        let mut selection = selection::Selection::tracked(start_ptr, end_ptr, false);

        list.selection_adjust(&mut selection, selection::Adjustment::Right)
            .unwrap();

        assert_eq!(selection.start(), start);
        assert_eq!(selection.end(), screen_pin(&list, 4, 3));
        assert_eq!(tracked_pin_value(start_ptr), start);
        assert_eq!(tracked_pin_value(end_ptr), screen_pin(&list, 4, 3));

        list.untrack_pin(start_ptr);
        list.untrack_pin(end_ptr);
    }

    #[test]
    fn selection_adjust_invalid_or_noop_edges() {
        let mut list = PageList::init(10, 10, None).unwrap();
        set_screen_text_lines(&mut list, &["A"]);
        let other = PageList::init(10, 10, None).unwrap();
        let start = screen_pin(&list, 0, 0);
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let mut selection = selection::Selection::new(start, invalid, false);

        assert!(list
            .selection_adjust(&mut selection, selection::Adjustment::Right)
            .is_none());
        assert_eq!(selection.start(), start);
        assert_eq!(selection.end(), invalid);

        let mut garbage = start;
        garbage.garbage = true;
        let mut selection = selection::Selection::new(start, garbage, false);
        assert!(list
            .selection_adjust(&mut selection, selection::Adjustment::Left)
            .is_none());
        assert_eq!(selection.start(), start);

        let mut selection = screen_selection(&list, (0, 0), (0, 0), false);
        list.selection_adjust(&mut selection, selection::Adjustment::Right)
            .unwrap();
        assert_selection_screen_points(&list, selection, (0, 0), (0, 0));
    }

    #[test]
    fn page_list_track_selection_tracks_and_untracks_owned_pins() {
        let mut list = PageList::init(10, 20, None).unwrap();
        let selection = screen_selection(&list, (2, 5), (7, 5), true);
        let count = list.count_tracked_pins();

        let tracked = list.track_selection(selection).unwrap();

        assert!(tracked.is_tracked());
        assert_eq!(tracked.start(), selection.start());
        assert_eq!(tracked.end(), selection.end());
        assert_eq!(tracked.rectangle(), selection.rectangle());
        assert_eq!(list.count_tracked_pins(), count + 2);
        let (start, end) = tracked.tracked_pins().unwrap();
        assert!(list.tracked_pins().contains(&start));
        assert!(list.tracked_pins().contains(&end));
        assert_eq!(tracked_pin_value(start), selection.start());
        assert_eq!(tracked_pin_value(end), selection.end());

        list.untrack_selection(tracked);

        assert_eq!(list.count_tracked_pins(), count);
        assert!(!list.tracked_pins().contains(&start));
        assert!(!list.tracked_pins().contains(&end));
    }

    #[test]
    fn page_list_untrack_selection_untracked_is_noop() {
        let mut list = PageList::init(10, 20, None).unwrap();
        let selection = screen_selection(&list, (2, 5), (7, 5), false);
        let count = list.count_tracked_pins();

        list.untrack_selection(selection);

        assert_eq!(list.count_tracked_pins(), count);
    }

    #[test]
    fn page_list_track_selection_tracked_input_returns_none_without_leak() {
        let mut list = PageList::init(10, 20, None).unwrap();
        let start = screen_pin(&list, 2, 5);
        let end = screen_pin(&list, 7, 5);
        let start_ptr = list.track_pin(start).unwrap();
        let end_ptr = list.track_pin(end).unwrap();
        let selection = selection::Selection::tracked(start_ptr, end_ptr, false);
        let count = list.count_tracked_pins();

        assert!(list.track_selection(selection).is_none());
        assert_eq!(list.count_tracked_pins(), count);

        list.untrack_pin(start_ptr);
        list.untrack_pin(end_ptr);
    }

    #[test]
    fn page_list_track_selection_invalid_start_returns_none_without_leak() {
        let mut list = PageList::init(10, 20, None).unwrap();
        let node = list.first_node_ptr();
        let start = Pin::new(node, 0, list.cols);
        let end = screen_pin(&list, 7, 5);
        let count = list.count_tracked_pins();

        assert!(list
            .track_selection(selection::Selection::new(start, end, false))
            .is_none());
        assert_eq!(list.count_tracked_pins(), count);
    }

    #[test]
    fn page_list_track_selection_invalid_end_rolls_back_start() {
        let mut list = PageList::init(10, 20, None).unwrap();
        let start = screen_pin(&list, 2, 5);
        let node = list.first_node_ptr();
        let end = Pin::new(node, list.node_for_ptr(node).unwrap().page.size_rows(), 0);
        let count = list.count_tracked_pins();

        assert!(list
            .track_selection(selection::Selection::new(start, end, false))
            .is_none());
        assert_eq!(list.count_tracked_pins(), count);
    }

    #[test]
    fn page_list_track_selection_missing_start_returns_none_without_leak() {
        let mut list = PageList::init(10, 20, None).unwrap();
        let other = PageList::init(10, 20, None).unwrap();
        let start = Pin::new(other.first_node_ptr(), 0, 0);
        let end = screen_pin(&list, 7, 5);
        let count = list.count_tracked_pins();

        assert!(list
            .track_selection(selection::Selection::new(start, end, false))
            .is_none());
        assert_eq!(list.count_tracked_pins(), count);
    }

    #[test]
    fn page_list_track_selection_missing_end_rolls_back_start() {
        let mut list = PageList::init(10, 20, None).unwrap();
        let other = PageList::init(10, 20, None).unwrap();
        let start = screen_pin(&list, 2, 5);
        let end = Pin::new(other.first_node_ptr(), 0, 0);
        let count = list.count_tracked_pins();

        assert!(list
            .track_selection(selection::Selection::new(start, end, false))
            .is_none());
        assert_eq!(list.count_tracked_pins(), count);
    }

    #[test]
    fn page_list_track_selection_garbage_pins_return_none_without_leak() {
        let mut list = PageList::init(10, 20, None).unwrap();
        let start = screen_pin(&list, 2, 5);
        let end = screen_pin(&list, 7, 5);
        let count = list.count_tracked_pins();
        let mut garbage_start = start;
        garbage_start.garbage = true;
        let mut garbage_end = end;
        garbage_end.garbage = true;

        assert!(list
            .track_selection(selection::Selection::new(garbage_start, end, false))
            .is_none());
        assert_eq!(list.count_tracked_pins(), count);
        assert!(list
            .track_selection(selection::Selection::new(start, garbage_end, false))
            .is_none());
        assert_eq!(list.count_tracked_pins(), count);
    }

    #[test]
    fn page_list_track_selection_preserves_reversed_stored_endpoints() {
        let mut list = PageList::init(10, 20, None).unwrap();
        let selection = screen_selection(&list, (7, 5), (2, 5), false);
        let count = list.count_tracked_pins();

        let tracked = list.track_selection(selection).unwrap();

        assert_eq!(tracked.start(), selection.start());
        assert_eq!(tracked.end(), selection.end());
        assert_eq!(
            list.selection_order(tracked),
            Some(selection::Order::Reverse)
        );
        assert_eq!(list.count_tracked_pins(), count + 2);

        list.untrack_selection(tracked);
    }

    #[test]
    fn page_list_track_selection_duplicate_endpoints_are_distinct_tracked_pins() {
        let mut list = PageList::init(10, 20, None).unwrap();
        let pin = screen_pin(&list, 2, 5);
        let selection = selection::Selection::new(pin, pin, false);
        let count = list.count_tracked_pins();

        let tracked = list.track_selection(selection).unwrap();
        let (start, end) = tracked.tracked_pins().unwrap();

        assert_ne!(start, end);
        assert_eq!(tracked.start(), pin);
        assert_eq!(tracked.end(), pin);
        assert_eq!(list.count_tracked_pins(), count + 2);

        list.untrack_selection(tracked);
    }

    #[test]
    fn page_list_track_selection_tracks_page_list_pin_mutation() {
        let mut list = PageList::init(10, 10, Some(0)).unwrap();
        let node = list.first_node_ptr();
        let selection = selection::Selection::new(
            Pin {
                node,
                y: 1,
                x: 2,
                garbage: false,
            },
            Pin {
                node,
                y: 7,
                x: 3,
                garbage: false,
            },
            false,
        );
        let tracked = list.track_selection(selection).unwrap();

        list.split(Pin {
            node,
            y: 5,
            x: 0,
            garbage: false,
        })
        .unwrap();

        let second = NonNull::from(list.pages[1].as_ref());
        assert_eq!(tracked.start().node, list.first_node_ptr());
        assert_eq!(tracked.start().y, 1);
        assert_eq!(tracked.start().x, 2);
        assert_eq!(tracked.end().node, second);
        assert_eq!(tracked.end().y, 2);
        assert_eq!(tracked.end().x, 3);

        list.untrack_selection(tracked);
    }

    #[test]
    fn page_list_track_highlight_tracks_and_untracks_owned_pins() {
        let mut list = PageList::init(10, 20, None).unwrap();
        let start = list
            .pin(point::Point::screen(Coordinate::new(2, 5)))
            .unwrap();
        let end = list
            .pin(point::Point::screen(Coordinate::new(7, 5)))
            .unwrap();
        let count = list.count_tracked_pins();

        let tracked = list
            .track_highlight(highlight::Untracked { start, end })
            .unwrap();

        assert_eq!(list.count_tracked_pins(), count + 2);
        assert!(list.tracked_pins().contains(&tracked.start));
        assert!(list.tracked_pins().contains(&tracked.end));
        assert_eq!(tracked_pin_value(tracked.start), start);
        assert_eq!(tracked_pin_value(tracked.end), end);

        list.untrack_highlight(tracked);

        assert_eq!(list.count_tracked_pins(), count);
        assert!(!list.tracked_pins().contains(&tracked.start));
        assert!(!list.tracked_pins().contains(&tracked.end));
    }

    #[test]
    fn page_list_track_highlight_invalid_start_returns_none_without_leak() {
        let mut list = PageList::init(10, 20, None).unwrap();
        let node = list.first_node_ptr();
        let start = Pin::new(node, 0, list.cols);
        let end = list
            .pin(point::Point::screen(Coordinate::new(7, 5)))
            .unwrap();
        let count = list.count_tracked_pins();

        assert!(list
            .track_highlight(highlight::Untracked { start, end })
            .is_none());
        assert_eq!(list.count_tracked_pins(), count);
    }

    #[test]
    fn page_list_track_highlight_invalid_end_returns_none_without_leak() {
        let mut list = PageList::init(10, 20, None).unwrap();
        let start = list
            .pin(point::Point::screen(Coordinate::new(2, 5)))
            .unwrap();
        let node = list.first_node_ptr();
        let end = Pin::new(node, list.node_for_ptr(node).unwrap().page.size_rows(), 0);
        let count = list.count_tracked_pins();

        assert!(list
            .track_highlight(highlight::Untracked { start, end })
            .is_none());
        assert_eq!(list.count_tracked_pins(), count);
    }

    #[test]
    fn page_list_track_highlight_garbage_pins_return_none_without_leak() {
        let mut list = PageList::init(10, 20, None).unwrap();
        let start = list
            .pin(point::Point::screen(Coordinate::new(2, 5)))
            .unwrap();
        let end = list
            .pin(point::Point::screen(Coordinate::new(7, 5)))
            .unwrap();
        let count = list.count_tracked_pins();
        let mut garbage_start = start;
        garbage_start.garbage = true;
        let mut garbage_end = end;
        garbage_end.garbage = true;

        assert!(list
            .track_highlight(highlight::Untracked {
                start: garbage_start,
                end,
            })
            .is_none());
        assert_eq!(list.count_tracked_pins(), count);
        assert!(list
            .track_highlight(highlight::Untracked {
                start,
                end: garbage_end,
            })
            .is_none());
        assert_eq!(list.count_tracked_pins(), count);
    }

    #[test]
    fn page_list_track_highlight_same_page_reversed_returns_none_without_leak() {
        let mut list = PageList::init(10, 20, None).unwrap();
        let start = list
            .pin(point::Point::screen(Coordinate::new(2, 5)))
            .unwrap();
        let end = list
            .pin(point::Point::screen(Coordinate::new(1, 5)))
            .unwrap();
        let count = list.count_tracked_pins();

        assert!(list
            .track_highlight(highlight::Untracked { start, end })
            .is_none());
        assert_eq!(list.count_tracked_pins(), count);
    }

    #[test]
    fn page_list_track_highlight_cross_page_reversed_returns_none_without_leak() {
        let mut list = PageList::init(4, 4, None).unwrap();
        let split = list
            .pin(point::Point::screen(Coordinate::new(0, 2)))
            .unwrap();
        list.split(split).unwrap();
        let start = list
            .pin(point::Point::screen(Coordinate::new(0, 2)))
            .unwrap();
        let end = list
            .pin(point::Point::screen(Coordinate::new(3, 1)))
            .unwrap();
        let count = list.count_tracked_pins();

        assert!(list
            .track_highlight(highlight::Untracked { start, end })
            .is_none());
        assert_eq!(list.count_tracked_pins(), count);
    }

    #[test]
    fn page_chunk_full_page_and_overlaps() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit at least one row");
        let mut list = PageList::init(cols, 1, None).unwrap();
        list.grow_rows(1).unwrap();
        let first = list.first_node_ptr();
        let second = list.last_node_ptr();

        let full = PageChunk {
            node: first,
            start: 0,
            end: 1,
        };
        let partial = PageChunk {
            node: first,
            start: 0,
            end: 0,
        };
        let same_overlap = PageChunk {
            node: first,
            start: 0,
            end: 1,
        };
        let same_disjoint = PageChunk {
            node: first,
            start: 1,
            end: 1,
        };
        let other_node = PageChunk {
            node: second,
            start: 0,
            end: 1,
        };

        assert!(full.full_page(&list));
        assert!(!partial.full_page(&list));
        assert!(full.overlaps(&same_overlap));
        assert!(!full.overlaps(&same_disjoint));
        assert!(!full.overlaps(&other_node));
    }

    #[test]
    fn page_list_clone_region_basic() {
        let list = PageList::init(80, 24, None).unwrap();

        let clone = list
            .clone_region(clone_options(point::Point::screen(Coordinate::new(0, 0))))
            .unwrap();

        assert_eq!(clone.total_rows, list.total_rows);
        assert_eq!(clone.viewport, Viewport::Active);
        assert_eq!(clone.page_serial_min, 0);
        assert_eq!(clone.pages.len(), 1);
        assert_eq!(clone.pages[0].serial, 0);
        assert_eq!(clone.page_serial, 1);
        clone.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_clone_region_partial_trimmed_right() {
        let mut list = PageList::init(80, 20, None).unwrap();
        list.grow_rows(30).unwrap();

        let clone = list
            .clone_region(CloneOptions {
                top: point::Point::screen(Coordinate::new(0, 0)),
                bottom: Some(point::Point::screen(Coordinate::new(0, 39))),
                tracked_pins: None,
            })
            .unwrap();

        assert_eq!(clone.total_rows, 40);
        clone.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_clone_region_partial_trimmed_left() {
        let mut list = PageList::init(80, 20, None).unwrap();
        list.grow_rows(30).unwrap();

        let clone = list
            .clone_region(clone_options(point::Point::screen(Coordinate::new(0, 10))))
            .unwrap();

        assert_eq!(clone.total_rows, 40);
        clone.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_clone_region_partial_trimmed_both() {
        let mut list = PageList::init(80, 20, None).unwrap();
        list.grow_rows(30).unwrap();

        let clone = list
            .clone_region(CloneOptions {
                top: point::Point::screen(Coordinate::new(0, 10)),
                bottom: Some(point::Point::screen(Coordinate::new(0, 35))),
                tracked_pins: None,
            })
            .unwrap();

        assert_eq!(clone.total_rows, 26);
        clone.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_clone_region_less_than_active_pads_blank_rows() {
        let list = PageList::init(80, 24, None).unwrap();

        let clone = list
            .clone_region(clone_options(point::Point::active(Coordinate::new(0, 5))))
            .unwrap();

        assert_eq!(clone.total_rows, clone.rows);
        let last = clone.pages.last().unwrap();
        let last_row = last.page.get_row(last.page.size_rows() as usize - 1);
        assert!(last
            .page
            .get_cells(last_row)
            .iter()
            .all(|cell| cell.is_zero()));
        clone.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_clone_region_copies_row_data() {
        let mut list = PageList::init(80, 20, None).unwrap();
        for y in 10..=12 {
            *list.pages[0].page.get_row_and_cell_mut(0, y).cell =
                Cell::init(('a' as usize + y) as u32);
        }

        let clone = list
            .clone_region(CloneOptions {
                top: point::Point::screen(Coordinate::new(0, 10)),
                bottom: Some(point::Point::screen(Coordinate::new(0, 12))),
                tracked_pins: None,
            })
            .unwrap();

        assert_eq!(
            page_cell(&clone.pages[0].page, 0, 0).codepoint(),
            'k' as u32
        );
        assert_eq!(
            page_cell(&clone.pages[0].page, 0, 1).codepoint(),
            'l' as u32
        );
        assert_eq!(
            page_cell(&clone.pages[0].page, 0, 2).codepoint(),
            'm' as u32
        );
        assert!(page_cell(&clone.pages[0].page, 1, 0).is_zero());
        clone.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_clone_region_reclaims_trimmed_managed_memory() {
        let mut list = PageList::init(80, 20, None).unwrap();
        let page = &mut list.pages[0].page;
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let style_id = page.add_style(bold).unwrap();
        let link_id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(7),
                uri: b"https://example.com",
            })
            .unwrap();

        {
            let rac = page.get_row_and_cell_mut(0, 0);
            rac.row.set_styled(true);
            rac.row.set_grapheme(true);
            rac.row.set_hyperlink(true);
            let mut cell = Cell::init('x' as u32);
            cell.set_style_id(style_id);
            cell.set_hyperlink(true);
            *rac.cell = cell;
        }
        page.use_style(style_id);
        page.append_grapheme_at(0, 0, 0x0301).unwrap();
        page.set_hyperlink(0, 0, link_id).unwrap();
        *page.get_row_and_cell_mut(0, 1).cell = Cell::init('y' as u32);

        let clone = list
            .clone_region(CloneOptions {
                top: point::Point::screen(Coordinate::new(0, 1)),
                bottom: Some(point::Point::screen(Coordinate::new(0, 1))),
                tracked_pins: None,
            })
            .unwrap();

        assert_eq!(clone.pages[0].page.style_count(), 0);
        assert_eq!(clone.pages[0].page.grapheme_count(), 0);
        assert_eq!(clone.pages[0].page.hyperlink_count(), 0);
        assert_eq!(
            page_cell(&clone.pages[0].page, 0, 0).codepoint(),
            'y' as u32
        );
        clone.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_clone_region_copies_managed_memory_inside_range() {
        let mut list = PageList::init(80, 20, None).unwrap();
        let page = &mut list.pages[0].page;
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let style_id = page.add_style(bold).unwrap();
        let link_id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"clone"),
                uri: b"https://example.com/clone",
            })
            .unwrap();

        {
            let rac = page.get_row_and_cell_mut(0, 1);
            rac.row.set_styled(true);
            let mut cell = Cell::init('s' as u32);
            cell.set_style_id(style_id);
            *rac.cell = cell;
        }
        page.use_style(style_id);
        *page.get_row_and_cell_mut(1, 1).cell = Cell::init('g' as u32);
        page.append_grapheme_at(1, 1, 0x0301).unwrap();
        *page.get_row_and_cell_mut(2, 1).cell = Cell::init('h' as u32);
        page.set_hyperlink(2, 1, link_id).unwrap();

        let clone = list
            .clone_region(CloneOptions {
                top: point::Point::screen(Coordinate::new(0, 1)),
                bottom: Some(point::Point::screen(Coordinate::new(0, 1))),
                tracked_pins: None,
            })
            .unwrap();
        let cloned_page = &clone.pages[0].page;
        let cloned_style_id = page_cell(cloned_page, 0, 0).style_id();
        let cloned_link_id = cloned_page.lookup_hyperlink_at(2, 0).unwrap();

        assert_eq!(cloned_page.style_count(), 1);
        assert_eq!(cloned_page.get_style(cloned_style_id), bold);
        assert_eq!(cloned_page.grapheme_count(), 1);
        assert_eq!(cloned_page.lookup_grapheme_at(1, 0).unwrap(), vec![0x0301]);
        assert_eq!(cloned_page.hyperlink_count(), 1);
        assert_eq!(
            cloned_page.get_hyperlink(cloned_link_id),
            HyperlinkSnapshot {
                id: HyperlinkSnapshotId::Explicit(b"clone".to_vec()),
                uri: b"https://example.com/clone".to_vec(),
            }
        );
        clone.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_clone_region_remaps_tracked_pin_inside_range() {
        let mut list = PageList::init(80, 24, None).unwrap();
        let tracked = list
            .track_pin(
                list.pin(point::Point::active(Coordinate::new(0, 6)))
                    .unwrap(),
            )
            .unwrap();
        let mut remap = TrackedPinsRemap::default();

        let clone = list
            .clone_region(CloneOptions {
                top: point::Point::active(Coordinate::new(0, 5)),
                bottom: None,
                tracked_pins: Some(&mut remap),
            })
            .unwrap();

        let cloned_pin = unsafe {
            // Safety: remapped pins are owned by clone.tracked_pin_storage.
            remap.get(tracked).unwrap().as_ref()
        };
        assert_eq!(
            clone
                .point_from_pin(point::Tag::Active, *cloned_pin)
                .unwrap(),
            point::Point::active(Coordinate::new(0, 1))
        );
        clone.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_clone_region_does_not_remap_tracked_pin_outside_range() {
        let mut list = PageList::init(80, 24, None).unwrap();
        let tracked = list
            .track_pin(
                list.pin(point::Point::active(Coordinate::new(0, 3)))
                    .unwrap(),
            )
            .unwrap();
        let mut remap = TrackedPinsRemap::default();

        let clone = list
            .clone_region(CloneOptions {
                top: point::Point::active(Coordinate::new(0, 5)),
                bottom: None,
                tracked_pins: Some(&mut remap),
            })
            .unwrap();

        assert_eq!(remap.get(tracked), None);
        clone.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_clone_region_invalid_request_returns_empty_error() {
        let list = PageList::init(80, 24, None).unwrap();

        assert_eq!(
            list.clone_region(clone_options(point::Point::screen(Coordinate::new(80, 0))))
                .unwrap_err(),
            CloneRegionError::Empty
        );
    }

    #[test]
    fn page_list_get_cell_active_screen_and_history() {
        let capacity_rows = initial_capacity(80).rows();
        let mut list = PageList::init(80, capacity_rows, None).unwrap();
        list.grow_rows(2).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));
        set_row_marker(&mut list.pages[0].page, 0, 10);
        set_active_row_marker(&mut list, 0, 20);
        set_active_row_marker(&mut list, 1, 30);

        let history = list
            .get_cell(point::Point::history(Coordinate::new(0, 0)))
            .unwrap();
        assert_eq!(history.cell.codepoint(), 10);
        assert_eq!(history.row_idx, 0);
        assert_eq!(history.col_idx, 0);

        let active = list
            .get_cell(point::Point::active(Coordinate::new(0, 1)))
            .unwrap();
        assert_eq!(active.cell.codepoint(), 30);
        assert_eq!(
            active.screen_point(&list).unwrap().coord(),
            Coordinate::new(0, 3)
        );

        let screen = list
            .get_cell(point::Point::screen(Coordinate::new(0, 2)))
            .unwrap();
        assert_eq!(screen.cell.codepoint(), 20);
        assert_eq!(
            screen.screen_point(&list).unwrap().coord(),
            Coordinate::new(0, 2)
        );
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_get_cell_crosses_page_boundaries() {
        let cols = STD_CAPACITY
            .max_cols()
            .expect("standard capacity should fit one row");
        let mut list = PageList::init(cols, 1, None).unwrap();
        list.grow_rows(2).unwrap();
        set_row_marker(&mut list.pages[0].page, 0, 11);
        set_row_marker(&mut list.pages[1].page, 0, 22);
        set_row_marker(&mut list.pages[2].page, 0, 33);

        for (screen_y, expected) in [(0, 11), (1, 22), (2, 33)] {
            let cell = list
                .get_cell(point::Point::screen(Coordinate::new(0, screen_y)))
                .unwrap();
            assert_eq!(cell.cell.codepoint(), expected);
            assert_eq!(cell.screen_point(&list).unwrap().coord().y, screen_y);
        }
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_get_cell_returns_none_for_invalid_points() {
        let list = PageList::init(80, 24, None).unwrap();

        assert!(list
            .get_cell(point::Point::active(Coordinate::new(80, 0)))
            .is_none());
        assert!(list
            .get_cell(point::Point::history(Coordinate::new(0, 24)))
            .is_none());
        assert!(list
            .get_cell(point::Point::screen(Coordinate::new(0, 24)))
            .is_none());
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_cell_dirty_style_and_screen_point() {
        let mut list = PageList::init(80, 24, None).unwrap();
        let styled = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let style_id = list.pages[0].page.add_style(styled).unwrap();
        {
            let rac = list.pages[0].page.get_row_and_cell_mut(4, 3);
            let mut cell = Cell::init('S' as u32);
            cell.set_style_id(style_id);
            *rac.cell = cell;
            rac.row.set_dirty(true);
        }
        list.pages[0].page.use_style(style_id);
        list.pages[0].page.release_style(style_id);

        let cell = list
            .get_cell(point::Point::active(Coordinate::new(4, 3)))
            .unwrap();
        assert!(cell.is_dirty());
        assert_eq!(cell.style(), styled);
        assert_eq!(
            cell.screen_point(&list).unwrap().coord(),
            Coordinate::new(4, 3)
        );

        let default = list
            .get_cell(point::Point::active(Coordinate::new(0, 0)))
            .unwrap();
        assert_eq!(default.style(), style::Style::default());
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_total_pages_reports_current_count() {
        let mut list = PageList::init(80, 24, None).unwrap();
        assert_eq!(list.total_pages(), list.pages.len());
        let initial = list.total_pages();

        let page_rows = list.pages[0].page.capacity().rows() as usize;
        list.grow_rows(page_rows * 2).unwrap();

        assert!(list.total_pages() > initial);
        assert_eq!(list.total_pages(), list.pages.len());
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_dirty_helpers_mark_and_query_row_dirty() {
        let mut list = PageList::init(80, 24, None).unwrap();

        assert!(!list.is_dirty(point::Point::active(Coordinate::new(0, 4))));
        list.mark_dirty(point::Point::active(Coordinate::new(0, 4)));

        assert!(list.is_dirty(point::Point::active(Coordinate::new(0, 4))));
        assert!(list.is_dirty(point::Point::active(Coordinate::new(79, 4))));
        assert!(!list.is_dirty(point::Point::active(Coordinate::new(0, 3))));
        assert!(!list.is_dirty(point::Point::active(Coordinate::new(0, 5))));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_dirty_helpers_page_dirty_marks_all_page_points_dirty() {
        let mut list = PageList::init(80, 24, None).unwrap();

        list.pages[0].page.set_dirty(true);

        assert!(list.is_dirty(point::Point::active(Coordinate::new(0, 0))));
        assert!(list.is_dirty(point::Point::active(Coordinate::new(0, 23))));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_clear_dirty_clears_all_pages_and_rows() {
        let (mut list, second_page_y) = multi_page_list(100);
        let first_page_point = point::Point::screen(Coordinate::new(0, 2));
        let second_page_point = point::Point::screen(Coordinate::new(0, second_page_y as u32));

        list.pages[0].page.set_dirty(true);
        list.pages[1].page.set_dirty(true);
        list.mark_dirty(first_page_point);
        list.mark_dirty(second_page_point);

        assert!(list.is_dirty(first_page_point));
        assert!(list.is_dirty(second_page_point));

        list.clear_dirty();

        assert!(!list.is_dirty(first_page_point));
        assert!(!list.is_dirty(second_page_point));
        for node in &list.pages {
            assert!(!node.page.is_dirty());
            for y in 0..node.page.size_rows() as usize {
                assert!(!node.page.get_row(y).dirty());
            }
        }
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_clone_region_preserves_full_dirty_rows() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.mark_dirty(point::Point::active(Coordinate::new(0, 0)));
        list.mark_dirty(point::Point::active(Coordinate::new(0, 12)));
        list.mark_dirty(point::Point::active(Coordinate::new(0, 23)));

        let clone = list
            .clone_region(clone_options(point::Point::screen(Coordinate::new(0, 0))))
            .unwrap();

        assert!(clone.is_dirty(point::Point::active(Coordinate::new(0, 0))));
        assert!(!clone.is_dirty(point::Point::active(Coordinate::new(0, 1))));
        assert!(clone.is_dirty(point::Point::active(Coordinate::new(0, 12))));
        assert!(!clone.is_dirty(point::Point::active(Coordinate::new(0, 14))));
        assert!(clone.is_dirty(point::Point::active(Coordinate::new(0, 23))));
        clone.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_initial_scrollbar_matches_viewport_rows() {
        let mut list = PageList::init(80, 24, None).unwrap();

        assert_eq!(
            list.scrollbar(),
            Scrollbar {
                total: 24,
                offset: 0,
                len: 24,
            }
        );
    }

    #[test]
    fn page_list_scrollbar_max_size_zero_hides_simulated_scrollback() {
        let mut list = PageList::init(80, 24, Some(0)).unwrap();
        list.pages[0].page.set_size_rows(30);
        list.total_rows = 30;

        assert_eq!(
            list.scrollbar(),
            Scrollbar {
                total: 24,
                offset: 0,
                len: 24,
            }
        );
    }

    #[test]
    fn page_list_scrollbar_active_viewport_reports_bottom_offset() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.pages[0].page.set_size_rows(30);
        list.total_rows = 30;

        assert_eq!(
            list.scrollbar(),
            Scrollbar {
                total: 30,
                offset: 6,
                len: 24,
            }
        );
    }

    #[test]
    fn page_list_scrollbar_top_viewport_reports_zero_offset() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.pages[0].page.set_size_rows(30);
        list.total_rows = 30;
        list.viewport = Viewport::Top;

        assert_eq!(
            list.scrollbar(),
            Scrollbar {
                total: 30,
                offset: 0,
                len: 24,
            }
        );
    }

    #[test]
    fn page_list_scrollbar_pin_viewport_offsets_within_single_page() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.pages[0].page.set_size_rows(30);
        list.total_rows = 30;
        list.viewport = Viewport::Pin;
        list.viewport_pin.y = 4;

        assert_eq!(
            list.scrollbar(),
            Scrollbar {
                total: 30,
                offset: 4,
                len: 24,
            }
        );
        assert_eq!(list.viewport_pin_row_offset, Some(4));
    }

    #[test]
    fn page_list_scrollbar_pin_viewport_offsets_across_pages() {
        let cols = 50;
        let capacity = initial_capacity(cols);
        let total_rows = capacity.rows() * 2;
        let mut list = PageList::init(cols, total_rows, None).unwrap();
        assert_eq!(list.pages.len(), 2);
        list.rows = 24;
        list.viewport = Viewport::Pin;
        list.viewport_pin.node = NonNull::from(list.pages[1].as_ref());
        list.viewport_pin.y = 5;
        let expected_offset = capacity.rows() as usize + 5;

        assert_eq!(
            list.scrollbar(),
            Scrollbar {
                total: total_rows as usize,
                offset: expected_offset,
                len: 24,
            }
        );
        assert_eq!(list.viewport_pin_row_offset, Some(expected_offset));
    }

    #[test]
    fn page_list_scrollbar_pin_viewport_reuses_cached_offset() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.pages[0].page.set_size_rows(30);
        list.total_rows = 30;
        list.viewport = Viewport::Pin;
        list.viewport_pin.y = 4;

        assert_eq!(list.scrollbar().offset, 4);
        assert_eq!(list.viewport_pin_row_offset, Some(4));
        assert_eq!(list.scrollbar().offset, 4);
        assert_eq!(list.viewport_pin_row_offset, Some(4));
    }

    #[test]
    fn page_list_fixup_viewport_active_is_noop() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.viewport = Viewport::Active;
        list.viewport_pin_row_offset = Some(7);

        list.fixup_viewport(3);

        assert_eq!(list.viewport, Viewport::Active);
        assert_eq!(list.viewport_pin_row_offset, Some(7));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_fixup_viewport_pin_becomes_active() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.pages[0].page.set_size_rows(30);
        list.total_rows = 30;
        list.viewport = Viewport::Pin;
        list.viewport_pin.y = 6;

        list.fixup_viewport(1);

        assert_eq!(list.viewport, Viewport::Active);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_fixup_viewport_pin_active_takes_precedence_over_cache() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.pages[0].page.set_size_rows(30);
        list.total_rows = 30;
        list.viewport = Viewport::Pin;
        list.viewport_pin.y = 6;
        list.viewport_pin_row_offset = Some(0);

        list.fixup_viewport(3);

        assert_eq!(list.viewport, Viewport::Active);
        assert_eq!(list.viewport_pin_row_offset, Some(0));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_fixup_viewport_pin_cached_offset_decrements() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.pages[0].page.set_size_rows(30);
        list.total_rows = 30;
        list.viewport = Viewport::Pin;
        list.viewport_pin.y = 2;
        list.viewport_pin_row_offset = Some(5);

        list.fixup_viewport(3);

        assert_eq!(list.viewport, Viewport::Pin);
        assert_eq!(list.viewport_pin_row_offset, Some(2));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_fixup_viewport_pin_cached_offset_equal_removed_stays_pinned() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.pages[0].page.set_size_rows(30);
        list.total_rows = 30;
        list.viewport = Viewport::Pin;
        list.viewport_pin.y = 0;
        list.viewport_pin_row_offset = Some(3);

        list.fixup_viewport(3);

        assert_eq!(list.viewport, Viewport::Pin);
        assert_eq!(list.viewport_pin_row_offset, Some(0));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_fixup_viewport_pin_cached_offset_below_removed_moves_top() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.pages[0].page.set_size_rows(30);
        list.total_rows = 30;
        list.viewport = Viewport::Pin;
        list.viewport_pin.y = 0;
        list.viewport_pin_row_offset = Some(2);

        list.fixup_viewport(3);

        assert_eq!(list.viewport, Viewport::Top);
        assert_eq!(list.viewport_pin_row_offset, Some(2));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_fixup_viewport_pin_without_cache_stays_pinned() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.pages[0].page.set_size_rows(30);
        list.total_rows = 30;
        list.viewport = Viewport::Pin;
        list.viewport_pin.y = 4;
        list.viewport_pin_row_offset = None;

        list.fixup_viewport(1);

        assert_eq!(list.viewport, Viewport::Pin);
        assert_eq!(list.viewport_pin_row_offset, None);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_fixup_viewport_top_becomes_active_when_first_page_is_active() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.viewport = Viewport::Top;

        list.fixup_viewport(1);

        assert_eq!(list.viewport, Viewport::Active);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_fixup_viewport_top_remains_top_when_first_page_is_not_active() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.pages[0].page.set_size_rows(30);
        list.total_rows = 30;
        list.viewport = Viewport::Top;

        list.fixup_viewport(1);

        assert_eq!(list.viewport, Viewport::Top);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_row_single_page_shifts_rows() {
        let mut list = PageList::init(10, 5, Some(0)).unwrap();
        for y in 0..5 {
            set_row_marker(&mut list.pages[0].page, y, y as u32);
        }
        let page_size = list.page_size;
        let page_serial = list.page_serial;
        let total_rows = list.total_rows();

        list.erase_row(point::Point::active(Coordinate::new(0, 2)))
            .unwrap();

        assert_eq!(list.pages.len(), 1);
        assert_eq!(list.pages[0].page.size_rows(), 5);
        assert_eq!(list.total_rows(), total_rows);
        assert_eq!(list.page_size, page_size);
        assert_eq!(list.page_serial, page_serial);
        assert!(list.pages[0].page.is_dirty());
        assert_eq!(row_marker(&list.pages[0].page, 0), 0);
        assert_eq!(row_marker(&list.pages[0].page, 1), 1);
        assert_eq!(row_marker(&list.pages[0].page, 2), 3);
        assert_eq!(row_marker(&list.pages[0].page, 3), 4);
        assert_eq!(row_marker(&list.pages[0].page, 4), 0);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_row_shifts_across_pages() {
        let (mut list, first_rows) = multi_page_list(100);
        let second_rows = list.pages[1].page.size_rows();
        for y in 0..first_rows as usize {
            set_row_marker(&mut list.pages[0].page, y, 1000 + y as u32);
        }
        for y in 0..second_rows as usize {
            set_row_marker(&mut list.pages[1].page, y, 2000 + y as u32);
        }
        let first = list.first_node_ptr();
        let second = NonNull::from(list.pages[1].as_ref());
        let page_size = list.page_size;
        let page_serial = list.page_serial;
        let total_rows = list.total_rows();

        list.erase_row(point::Point::screen(Coordinate::new(
            0,
            first_rows as u32 - 2,
        )))
        .unwrap();

        assert_eq!(list.first_node_ptr(), first);
        assert_eq!(NonNull::from(list.pages[1].as_ref()), second);
        assert_eq!(list.page_size, page_size);
        assert_eq!(list.page_serial, page_serial);
        assert_eq!(list.total_rows(), total_rows);
        assert_eq!(
            row_marker(&list.pages[0].page, first_rows as usize - 2),
            1000 + first_rows as u32 - 1
        );
        assert_eq!(
            row_marker(&list.pages[0].page, first_rows as usize - 1),
            2000
        );
        assert_eq!(row_marker(&list.pages[1].page, 0), 2001);
        assert_eq!(row_marker(&list.pages[1].page, second_rows as usize - 1), 0);
        assert!(list.pages[0].page.is_dirty());
        assert!(list.pages[1].page.is_dirty());
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_row_updates_tracked_pins() {
        let (mut list, first_rows) = multi_page_list(100);
        let first = list.first_node_ptr();
        let second = NonNull::from(list.pages[1].as_ref());
        let pin_before = list
            .track_pin(Pin {
                node: first,
                y: first_rows - 3,
                x: 1,
                garbage: false,
            })
            .unwrap();
        let pin_erased = list
            .track_pin(Pin {
                node: first,
                y: first_rows - 2,
                x: 5,
                garbage: false,
            })
            .unwrap();
        let pin_after = list
            .track_pin(Pin {
                node: first,
                y: first_rows - 1,
                x: 2,
                garbage: false,
            })
            .unwrap();
        let pin_next_top = list
            .track_pin(Pin {
                node: second,
                y: 0,
                x: 3,
                garbage: false,
            })
            .unwrap();
        let pin_next_after = list
            .track_pin(Pin {
                node: second,
                y: 2,
                x: 4,
                garbage: false,
            })
            .unwrap();

        list.erase_row(point::Point::screen(Coordinate::new(
            0,
            first_rows as u32 - 2,
        )))
        .unwrap();

        let before = unsafe { pin_before.as_ref() };
        let erased = unsafe { pin_erased.as_ref() };
        let after = unsafe { pin_after.as_ref() };
        let next_top = unsafe { pin_next_top.as_ref() };
        let next_after = unsafe { pin_next_after.as_ref() };
        assert_eq!(before.node, first);
        assert_eq!(before.y, first_rows - 3);
        assert_eq!(before.x, 1);
        assert_eq!(erased.node, first);
        assert_eq!(erased.y, first_rows - 2);
        assert_eq!(erased.x, 5);
        assert_eq!(after.node, first);
        assert_eq!(after.y, first_rows - 2);
        assert_eq!(after.x, 2);
        assert_eq!(next_top.node, first);
        assert_eq!(next_top.y, first_rows - 1);
        assert_eq!(next_top.x, 3);
        assert_eq!(next_after.node, second);
        assert_eq!(next_after.y, 1);
        assert_eq!(next_after.x, 4);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_row_preserves_row_dirty_flags() {
        let (mut list, first_rows) = multi_page_list(100);
        for node in &mut list.pages {
            node.page.set_dirty(false);
            for y in 0..node.page.size_rows() as usize {
                node.page.get_row_mut(y).set_dirty(false);
            }
        }
        list.pages[0]
            .page
            .get_row_mut(first_rows as usize - 1)
            .set_dirty(true);
        list.pages[1].page.get_row_mut(0).set_dirty(true);
        list.pages[1].page.get_row_mut(1).set_dirty(true);

        list.erase_row(point::Point::screen(Coordinate::new(
            0,
            first_rows as u32 - 2,
        )))
        .unwrap();

        assert!(list.pages[0].page.get_row(first_rows as usize - 2).dirty());
        assert!(list.pages[0].page.get_row(first_rows as usize - 1).dirty());
        assert!(list.pages[1].page.get_row(0).dirty());
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_row_updates_pinned_viewport_cache() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.grow_rows(80).unwrap();
        let pin = list
            .pin(point::Point::screen(Coordinate::new(0, 10)))
            .unwrap();
        list.scroll(Scroll::Pin(pin));
        assert_eq!(list.viewport, Viewport::Pin);
        assert_eq!(list.scrollbar().offset, 10);
        assert_eq!(list.viewport_pin_row_offset, Some(10));

        list.erase_row(point::Point::history(Coordinate::new(0, 0)))
            .unwrap();

        assert_eq!(list.scrollbar().offset, 9);
        assert_eq!(list.viewport_pin_row_offset, Some(9));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_row_releases_erased_managed_memory() {
        let mut list = PageList::init(10, 5, Some(0)).unwrap();
        let erased_style = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let moved_style = style::Style {
            flags: style::Flags {
                italic: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let erased_style_id = list.pages[0].page.add_style(erased_style).unwrap();
        let moved_style_id = list.pages[0].page.add_style(moved_style).unwrap();
        let erased_link_id = list.pages[0]
            .page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"erased"),
                uri: b"https://example.com/erased",
            })
            .unwrap();
        let moved_link_id = list.pages[0]
            .page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"moved"),
                uri: b"https://example.com/moved",
            })
            .unwrap();
        {
            let page = &mut list.pages[0].page;
            let rac = page.get_row_and_cell_mut(0, 1);
            rac.row.set_styled(true);
            let mut cell = Cell::init('E' as u32);
            cell.set_style_id(erased_style_id);
            *rac.cell = cell;
            page.use_style(erased_style_id);
            page.set_hyperlink(0, 1, erased_link_id).unwrap();
            page.append_grapheme_at(0, 1, 0x0301).unwrap();

            let rac = page.get_row_and_cell_mut(0, 2);
            rac.row.set_styled(true);
            let mut cell = Cell::init('M' as u32);
            cell.set_style_id(moved_style_id);
            *rac.cell = cell;
            page.use_style(moved_style_id);
            page.set_hyperlink(0, 2, moved_link_id).unwrap();
            page.append_grapheme_at(0, 2, 0x0302).unwrap();
        }
        list.pages[0].page.release_style(erased_style_id);
        list.pages[0].page.release_style(moved_style_id);

        list.erase_row(point::Point::active(Coordinate::new(0, 1)))
            .unwrap();

        let page = &list.pages[0].page;
        assert_eq!(page.style_count(), 1);
        assert_eq!(page.hyperlink_count(), 1);
        assert_eq!(page.grapheme_count(), 1);
        let moved = page_cell(page, 0, 1);
        assert_eq!(moved.codepoint(), 'M' as u32);
        assert_eq!(page.get_style(moved.style_id()), moved_style);
        assert_eq!(page.lookup_grapheme_at(0, 1).unwrap(), vec![0x0302]);
        let moved_link = page.lookup_hyperlink_at(0, 1).unwrap();
        assert_eq!(
            page.get_hyperlink(moved_link),
            HyperlinkSnapshot {
                id: HyperlinkSnapshotId::Explicit(b"moved".to_vec()),
                uri: b"https://example.com/moved".to_vec(),
            }
        );
        let final_row = page.get_row(4);
        assert!(!final_row.managed_memory());
        assert_eq!(row_marker(page, 4), 0);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_row_preserves_boundary_managed_memory() {
        let (mut list, first_rows) = multi_page_list(100);
        let boundary_style = style::Style {
            flags: style::Flags {
                italic: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let style_id = list.pages[1].page.add_style(boundary_style).unwrap();
        let link_id = list.pages[1]
            .page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"boundary"),
                uri: b"https://example.com/boundary",
            })
            .unwrap();
        {
            let page = &mut list.pages[1].page;
            let rac = page.get_row_and_cell_mut(0, 0);
            rac.row.set_styled(true);
            let mut cell = Cell::init('B' as u32);
            cell.set_style_id(style_id);
            *rac.cell = cell;
            page.use_style(style_id);
            page.set_hyperlink(0, 0, link_id).unwrap();
            page.append_grapheme_at(0, 0, 0x0303).unwrap();
        }
        list.pages[1].page.release_style(style_id);

        list.erase_row(point::Point::screen(Coordinate::new(
            0,
            first_rows as u32 - 1,
        )))
        .unwrap();

        let page = &list.pages[0].page;
        let moved_y = first_rows as usize - 1;
        let moved = page_cell(page, 0, moved_y);
        assert_eq!(moved.codepoint(), 'B' as u32);
        assert_eq!(page.get_style(moved.style_id()), boundary_style);
        assert_eq!(page.lookup_grapheme_at(0, moved_y).unwrap(), vec![0x0303]);
        let moved_link = page.lookup_hyperlink_at(0, moved_y).unwrap();
        assert_eq!(
            page.get_hyperlink(moved_link),
            HyperlinkSnapshot {
                id: HyperlinkSnapshotId::Explicit(b"boundary".to_vec()),
                uri: b"https://example.com/boundary".to_vec(),
            }
        );
        assert_eq!(list.pages[1].page.hyperlink_count(), 0);
        assert_eq!(list.pages[1].page.style_count(), 0);
        assert_eq!(list.pages[1].page.grapheme_count(), 0);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_row_bounded_single_page_prefix() {
        let mut list = PageList::init(10, 6, Some(0)).unwrap();
        for y in 0..6 {
            set_row_marker(&mut list.pages[0].page, y, y as u32);
        }
        let total_rows = list.total_rows();
        let page_size = list.page_size;
        let page_serial = list.page_serial;

        list.erase_row_bounded(point::Point::active(Coordinate::new(0, 2)), 2)
            .unwrap();

        assert_eq!(list.total_rows(), total_rows);
        assert_eq!(list.page_size, page_size);
        assert_eq!(list.page_serial, page_serial);
        assert_eq!(row_marker(&list.pages[0].page, 0), 0);
        assert_eq!(row_marker(&list.pages[0].page, 1), 1);
        assert_eq!(row_marker(&list.pages[0].page, 2), 3);
        assert_eq!(row_marker(&list.pages[0].page, 3), 4);
        assert_eq!(row_marker(&list.pages[0].page, 4), 0);
        assert_eq!(row_marker(&list.pages[0].page, 5), 5);
        assert!(list.pages[0].page.is_dirty());
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_row_bounded_pin_at_top() {
        let mut list = PageList::init(10, 6, Some(0)).unwrap();
        let first = list.first_node_ptr();
        for y in 0..6 {
            set_row_marker(&mut list.pages[0].page, y, y as u32);
        }
        let pin_top = list
            .track_pin(Pin {
                node: first,
                y: 0,
                x: 5,
                garbage: false,
            })
            .unwrap();
        let pin_inside = list
            .track_pin(Pin {
                node: first,
                y: 2,
                x: 3,
                garbage: false,
            })
            .unwrap();
        let pin_outside = list
            .track_pin(Pin {
                node: first,
                y: 4,
                x: 2,
                garbage: false,
            })
            .unwrap();

        list.erase_row_bounded(point::Point::active(Coordinate::new(0, 0)), 3)
            .unwrap();

        assert_eq!(row_marker(&list.pages[0].page, 0), 1);
        assert_eq!(row_marker(&list.pages[0].page, 1), 2);
        assert_eq!(row_marker(&list.pages[0].page, 2), 3);
        assert_eq!(row_marker(&list.pages[0].page, 3), 0);
        assert_eq!(row_marker(&list.pages[0].page, 4), 4);
        let top = unsafe { pin_top.as_ref() };
        let inside = unsafe { pin_inside.as_ref() };
        let outside = unsafe { pin_outside.as_ref() };
        assert_eq!(top.node, first);
        assert_eq!(top.y, 0);
        assert_eq!(top.x, 0);
        assert_eq!(inside.node, first);
        assert_eq!(inside.y, 1);
        assert_eq!(inside.x, 3);
        assert_eq!(outside.node, first);
        assert_eq!(outside.y, 4);
        assert_eq!(outside.x, 2);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_row_bounded_exact_page_boundary() {
        let (mut list, first_rows) = multi_page_list(100);
        let second_rows = list.pages[1].page.size_rows();
        for y in 0..first_rows as usize {
            set_row_marker(&mut list.pages[0].page, y, 1000 + y as u32);
        }
        for y in 0..second_rows as usize {
            set_row_marker(&mut list.pages[1].page, y, 2000 + y as u32);
        }
        let first = list.first_node_ptr();
        let second = NonNull::from(list.pages[1].as_ref());
        let pin_next_top = list
            .track_pin(Pin {
                node: second,
                y: 0,
                x: 7,
                garbage: false,
            })
            .unwrap();
        list.pages[1].page.get_row_mut(0).set_dirty(true);
        let total_rows = list.total_rows();
        let page_size = list.page_size;
        let page_serial = list.page_serial;
        let boundary_style = style::Style {
            flags: style::Flags {
                italic: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let style_id = list.pages[1].page.add_style(boundary_style).unwrap();
        let link_id = list.pages[1]
            .page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"bounded-cross"),
                uri: b"https://example.com/bounded-cross",
            })
            .unwrap();
        {
            let page = &mut list.pages[1].page;
            let rac = page.get_row_and_cell_mut(0, 0);
            rac.row.set_styled(true);
            let mut cell = Cell::init('C' as u32);
            cell.set_style_id(style_id);
            *rac.cell = cell;
            page.use_style(style_id);
            page.set_hyperlink(0, 0, link_id).unwrap();
            page.append_grapheme_at(0, 0, 0x0304).unwrap();
        }
        list.pages[1].page.release_style(style_id);

        list.erase_row_bounded(
            point::Point::screen(Coordinate::new(0, first_rows as u32 - 2)),
            2,
        )
        .unwrap();

        assert_eq!(list.total_rows(), total_rows);
        assert_eq!(list.page_size, page_size);
        assert_eq!(list.page_serial, page_serial);
        assert_eq!(
            row_marker(&list.pages[0].page, first_rows as usize - 2),
            1000 + first_rows as u32 - 1
        );
        let moved_y = first_rows as usize - 1;
        let moved = page_cell(&list.pages[0].page, 0, moved_y);
        assert_eq!(moved.codepoint(), 'C' as u32);
        assert_eq!(
            list.pages[0].page.get_style(moved.style_id()),
            boundary_style
        );
        assert_eq!(
            list.pages[0].page.lookup_grapheme_at(0, moved_y).unwrap(),
            vec![0x0304]
        );
        let moved_link = list.pages[0].page.lookup_hyperlink_at(0, moved_y).unwrap();
        assert_eq!(
            list.pages[0].page.get_hyperlink(moved_link),
            HyperlinkSnapshot {
                id: HyperlinkSnapshotId::Explicit(b"bounded-cross".to_vec()),
                uri: b"https://example.com/bounded-cross".to_vec(),
            }
        );
        assert_eq!(row_marker(&list.pages[1].page, 0), 0);
        assert_eq!(row_marker(&list.pages[1].page, 1), 2001);
        assert_eq!(list.pages[1].page.style_count(), 0);
        assert_eq!(list.pages[1].page.hyperlink_count(), 0);
        assert_eq!(list.pages[1].page.grapheme_count(), 0);
        let next_top = unsafe { pin_next_top.as_ref() };
        assert_eq!(next_top.node, first);
        assert_eq!(next_top.y, first_rows - 1);
        assert_eq!(next_top.x, 7);
        assert!(list.pages[0].page.get_row(first_rows as usize - 1).dirty());
        assert!(list.pages[0].page.is_dirty());
        assert!(list.pages[1].page.is_dirty());
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_row_bounded_full_span_single_page() {
        let mut list = PageList::init(10, 6, Some(0)).unwrap();
        let first = list.first_node_ptr();
        for y in 0..6 {
            set_row_marker(&mut list.pages[0].page, y, y as u32);
        }
        let pin_inside = list
            .track_pin(Pin {
                node: first,
                y: 4,
                x: 2,
                garbage: false,
            })
            .unwrap();
        let pin_outside = list
            .track_pin(Pin {
                node: first,
                y: 2,
                x: 3,
                garbage: false,
            })
            .unwrap();

        list.erase_row_bounded(point::Point::active(Coordinate::new(0, 3)), 3)
            .unwrap();

        assert_eq!(row_marker(&list.pages[0].page, 0), 0);
        assert_eq!(row_marker(&list.pages[0].page, 1), 1);
        assert_eq!(row_marker(&list.pages[0].page, 2), 2);
        assert_eq!(row_marker(&list.pages[0].page, 3), 4);
        assert_eq!(row_marker(&list.pages[0].page, 4), 5);
        assert_eq!(row_marker(&list.pages[0].page, 5), 0);
        let inside = unsafe { pin_inside.as_ref() };
        let outside = unsafe { pin_outside.as_ref() };
        assert_eq!(inside.node, first);
        assert_eq!(inside.y, 3);
        assert_eq!(inside.x, 2);
        assert_eq!(outside.node, first);
        assert_eq!(outside.y, 2);
        assert_eq!(outside.x, 3);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_row_bounded_two_page_partial_span() {
        let (mut list, first_rows) = multi_page_list(100);
        let second_rows = list.pages[1].page.size_rows();
        for y in 0..first_rows as usize {
            set_row_marker(&mut list.pages[0].page, y, 1000 + y as u32);
        }
        for y in 0..second_rows as usize {
            set_row_marker(&mut list.pages[1].page, y, 2000 + y as u32);
        }

        list.erase_row_bounded(
            point::Point::screen(Coordinate::new(0, first_rows as u32 - 2)),
            4,
        )
        .unwrap();

        assert_eq!(
            row_marker(&list.pages[0].page, first_rows as usize - 2),
            1000 + first_rows as u32 - 1
        );
        assert_eq!(
            row_marker(&list.pages[0].page, first_rows as usize - 1),
            2000
        );
        assert_eq!(row_marker(&list.pages[1].page, 0), 2001);
        assert_eq!(row_marker(&list.pages[1].page, 1), 2002);
        assert_eq!(row_marker(&list.pages[1].page, 2), 0);
        assert_eq!(row_marker(&list.pages[1].page, 3), 2003);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_row_bounded_updates_pinned_viewport_cache() {
        let (mut list, _) = bounded_viewport_list(3);
        let pin = list
            .pin(point::Point::screen(Coordinate::new(0, 4)))
            .unwrap();
        list.scroll(Scroll::Pin(pin));
        assert_eq!(list.scrollbar().offset, 4);
        assert_eq!(list.viewport_pin_row_offset, Some(4));

        list.erase_row_bounded(point::Point::history(Coordinate::new(0, 0)), 10)
            .unwrap();

        assert_eq!(list.scrollbar().offset, 3);
        assert_eq!(list.viewport_pin_row_offset, Some(3));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_row_bounded_multi_page_viewport_cache() {
        let (mut list, page_rows) = bounded_viewport_list(3);
        let pin_y = page_rows + 1;
        let pin = list
            .pin(point::Point::screen(Coordinate::new(0, pin_y as u32)))
            .unwrap();
        list.scroll(Scroll::Pin(pin));
        assert_eq!(list.scrollbar().offset, pin_y);

        list.erase_row_bounded(point::Point::history(Coordinate::new(0, 0)), page_rows + 10)
            .unwrap();

        assert_eq!(list.scrollbar().offset, pin_y - 1);
        assert_eq!(list.viewport_pin_row_offset, Some(pin_y - 1));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_row_bounded_full_page_shift_viewport_cache() {
        let (mut list, page_rows) = bounded_viewport_list(4);
        let pin_y = 5;
        let pin = list
            .pin(point::Point::screen(Coordinate::new(0, pin_y as u32)))
            .unwrap();
        list.scroll(Scroll::Pin(pin));
        assert_eq!(list.scrollbar().offset, pin_y);

        list.erase_row_bounded(
            point::Point::history(Coordinate::new(0, 0)),
            page_rows * 2 + 10,
        )
        .unwrap();

        assert_eq!(list.scrollbar().offset, pin_y - 1);
        assert_eq!(list.viewport_pin_row_offset, Some(pin_y - 1));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_row_bounded_exhausts_pages_viewport_cache() {
        let (mut list, page_rows) = bounded_viewport_list(3);
        let total_rows = list.total_rows();
        assert!(total_rows > list.rows as usize);
        let pin_y = page_rows * 2 + 10;
        let pin = list
            .pin(point::Point::screen(Coordinate::new(0, pin_y as u32)))
            .unwrap();
        list.scroll(Scroll::Pin(pin));
        assert_eq!(list.scrollbar().offset, pin_y);

        list.erase_row_bounded(point::Point::history(Coordinate::new(0, 0)), total_rows * 2)
            .unwrap();

        assert_eq!(list.scrollbar().offset, pin_y - 1);
        assert_eq!(list.viewport_pin_row_offset, Some(pin_y - 1));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_row_bounded_managed_memory() {
        let mut list = PageList::init(10, 6, Some(0)).unwrap();
        let erased_style = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let moved_style = style::Style {
            flags: style::Flags {
                italic: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let erased_style_id = list.pages[0].page.add_style(erased_style).unwrap();
        let moved_style_id = list.pages[0].page.add_style(moved_style).unwrap();
        let erased_link_id = list.pages[0]
            .page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"bounded-erased"),
                uri: b"https://example.com/bounded-erased",
            })
            .unwrap();
        let moved_link_id = list.pages[0]
            .page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"bounded-moved"),
                uri: b"https://example.com/bounded-moved",
            })
            .unwrap();
        {
            let page = &mut list.pages[0].page;
            for (y, ch, style_id, accent) in [
                (1, 'E', erased_style_id, 0x0301),
                (2, 'M', moved_style_id, 0x0302),
            ] {
                let rac = page.get_row_and_cell_mut(0, y);
                rac.row.set_styled(true);
                let mut cell = Cell::init(ch as u32);
                cell.set_style_id(style_id);
                *rac.cell = cell;
                page.use_style(style_id);
                page.append_grapheme_at(0, y, accent).unwrap();
            }
            page.set_hyperlink(0, 1, erased_link_id).unwrap();
            page.set_hyperlink(0, 2, moved_link_id).unwrap();
        }
        list.pages[0].page.release_style(erased_style_id);
        list.pages[0].page.release_style(moved_style_id);

        list.erase_row_bounded(point::Point::active(Coordinate::new(0, 1)), 1)
            .unwrap();

        let page = &list.pages[0].page;
        assert_eq!(page.style_count(), 1);
        assert_eq!(page.hyperlink_count(), 1);
        assert_eq!(page.grapheme_count(), 1);
        let moved = page_cell(page, 0, 1);
        assert_eq!(moved.codepoint(), 'M' as u32);
        assert_eq!(page.get_style(moved.style_id()), moved_style);
        assert_eq!(page.lookup_grapheme_at(0, 1).unwrap(), vec![0x0302]);
        let moved_link = page.lookup_hyperlink_at(0, 1).unwrap();
        assert_eq!(
            page.get_hyperlink(moved_link),
            HyperlinkSnapshot {
                id: HyperlinkSnapshotId::Explicit(b"bounded-moved".to_vec()),
                uri: b"https://example.com/bounded-moved".to_vec(),
            }
        );
        assert!(!page.get_row(2).managed_memory());
        assert_eq!(row_marker(page, 2), 0);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_page_deletes_first_page() {
        let (mut list, _) = bounded_viewport_list(3);
        assert!(list.pages.len() > 1);
        let removed = list.first_node_ptr();
        let replacement = NonNull::from(list.pages[1].as_ref());
        let replacement_serial = list.pages[1].serial;
        let stable_second_pin = list
            .track_pin(Pin {
                node: replacement,
                y: 1,
                x: 2,
                garbage: false,
            })
            .unwrap();
        let deleted_pin = list
            .track_pin(Pin {
                node: removed,
                y: 2,
                x: 3,
                garbage: false,
            })
            .unwrap();
        let removed_rows = list.pages[0].page.size_rows();
        let removed_len = list.pages[0].page.backing_len();
        let page_size = list.page_size;
        let total_rows = list.total_rows;
        let page_serial = list.page_serial;
        let rows = list.rows;
        let cols = list.cols;
        let explicit_max_size = list.explicit_max_size;
        let min_max_size = list.min_max_size;

        list.erase_page(removed).unwrap();

        assert_eq!(list.first_node_ptr(), replacement);
        assert_eq!(list.page_serial_min, replacement_serial);
        assert_eq!(list.page_size, page_size - removed_len);
        assert_eq!(list.total_rows, total_rows);
        assert_eq!(list.page_serial, page_serial);
        assert_eq!(list.rows, rows);
        assert_eq!(list.cols, cols);
        assert_eq!(list.explicit_max_size, explicit_max_size);
        assert_eq!(list.min_max_size, min_max_size);
        assert!(!list.pins_reference_node(removed));
        let deleted = unsafe { deleted_pin.as_ref() };
        assert_eq!(deleted.node, replacement);
        assert_eq!(deleted.y, 0);
        assert_eq!(deleted.x, 0);
        let stable_second = unsafe { stable_second_pin.as_ref() };
        assert_eq!(stable_second.node, replacement);
        assert_eq!(stable_second.y, 1);
        assert_eq!(stable_second.x, 2);
        assert_integrity_after_caller_row_accounting(&mut list, removed_rows);
    }

    #[test]
    fn page_list_erase_page_deletes_last_page() {
        let (mut list, _) = bounded_viewport_list(4);
        assert!(list.pages.len() > 2);
        let last_index = list.pages.len() - 1;
        let removed = NonNull::from(list.pages[last_index].as_ref());
        let replacement = NonNull::from(list.pages[last_index - 1].as_ref());
        let page_serial_min = list.page_serial_min;
        let stable_previous_pin = list
            .track_pin(Pin {
                node: replacement,
                y: 1,
                x: 2,
                garbage: false,
            })
            .unwrap();
        let deleted_pin = list
            .track_pin(Pin {
                node: removed,
                y: 2,
                x: 3,
                garbage: false,
            })
            .unwrap();
        let removed_rows = list.pages[last_index].page.size_rows();
        let removed_len = list.pages[last_index].page.backing_len();
        let page_size = list.page_size;
        let total_rows = list.total_rows;
        let page_serial = list.page_serial;
        let rows = list.rows;
        let cols = list.cols;
        let explicit_max_size = list.explicit_max_size;
        let min_max_size = list.min_max_size;

        list.erase_page(removed).unwrap();

        assert_eq!(list.last_node_ptr(), replacement);
        assert_eq!(list.page_serial_min, page_serial_min);
        assert_eq!(list.page_size, page_size - removed_len);
        assert_eq!(list.total_rows, total_rows);
        assert_eq!(list.page_serial, page_serial);
        assert_eq!(list.rows, rows);
        assert_eq!(list.cols, cols);
        assert_eq!(list.explicit_max_size, explicit_max_size);
        assert_eq!(list.min_max_size, min_max_size);
        assert!(!list.pins_reference_node(removed));
        let deleted = unsafe { deleted_pin.as_ref() };
        assert_eq!(deleted.node, replacement);
        assert_eq!(deleted.y, 0);
        assert_eq!(deleted.x, 0);
        let stable_previous = unsafe { stable_previous_pin.as_ref() };
        assert_eq!(stable_previous.node, replacement);
        assert_eq!(stable_previous.y, 1);
        assert_eq!(stable_previous.x, 2);
        assert_integrity_after_caller_row_accounting(&mut list, removed_rows);
    }

    #[test]
    fn page_list_erase_page_rejects_middle_page() {
        let (mut list, _) = bounded_viewport_list(4);
        assert!(list.pages.len() > 2);
        let middle = NonNull::from(list.pages[1].as_ref());
        let ptrs: Vec<_> = list
            .pages
            .iter()
            .map(|node| NonNull::from(node.as_ref()))
            .collect();
        let serials: Vec<_> = list.pages.iter().map(|node| node.serial).collect();
        let page_size = list.page_size;
        let total_rows = list.total_rows;
        let page_serial_min = list.page_serial_min;

        assert_eq!(list.erase_page(middle), Err(ErasePageError::MiddlePage));

        assert_eq!(list.page_size, page_size);
        assert_eq!(list.total_rows, total_rows);
        assert_eq!(list.page_serial_min, page_serial_min);
        assert_eq!(
            list.pages
                .iter()
                .map(|node| NonNull::from(node.as_ref()))
                .collect::<Vec<_>>(),
            ptrs
        );
        assert_eq!(
            list.pages
                .iter()
                .map(|node| node.serial)
                .collect::<Vec<_>>(),
            serials
        );
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_page_rejects_only_page() {
        let mut list = PageList::init(10, 5, Some(0)).unwrap();
        assert_eq!(list.pages.len(), 1);
        let only = list.first_node_ptr();
        let page_size = list.page_size;
        let total_rows = list.total_rows;
        let page_serial_min = list.page_serial_min;

        assert_eq!(list.erase_page(only), Err(ErasePageError::OnlyPage));

        assert_eq!(list.pages.len(), 1);
        assert_eq!(list.first_node_ptr(), only);
        assert_eq!(list.page_size, page_size);
        assert_eq!(list.total_rows, total_rows);
        assert_eq!(list.page_serial_min, page_serial_min);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_page_moves_viewport_pin_and_clears_cache() {
        let (mut list, _) = bounded_viewport_list(3);
        let removed = list.first_node_ptr();
        let replacement = NonNull::from(list.pages[1].as_ref());
        let removed_rows = list.pages[0].page.size_rows();
        list.scroll(Scroll::Pin(Pin {
            node: removed,
            y: 2,
            x: 5,
            garbage: false,
        }));
        assert_eq!(list.viewport, Viewport::Pin);
        assert!(list.viewport_pin_row_offset.is_none());
        let _ = list.scrollbar();
        assert!(list.viewport_pin_row_offset.is_some());

        list.erase_page(removed).unwrap();

        assert_eq!(list.viewport, Viewport::Pin);
        assert_eq!(list.viewport_pin.node, replacement);
        assert_eq!(list.viewport_pin.y, 0);
        assert_eq!(list.viewport_pin.x, 0);
        assert_eq!(list.viewport_pin_row_offset, None);
        assert!(!list.pins_reference_node(removed));
        assert_integrity_after_caller_row_accounting(&mut list, removed_rows);
    }

    #[test]
    fn page_list_erase_history_removes_scrollback() {
        let (mut list, page_rows) = bounded_viewport_list(5);
        assert!(list.total_rows > list.rows);
        assert!(list.pages.len() > 1);
        let start_page_size = list.page_size;
        let tracked = list
            .track_pin(
                list.pin(point::Point::history(Coordinate::new(0, 0)))
                    .unwrap(),
            )
            .unwrap();

        list.erase_history(None).unwrap();

        assert_eq!(list.total_rows, list.rows);
        assert_eq!(list.pages.len(), (list.rows as usize).div_ceil(page_rows));
        assert!(list.page_size < start_page_size);
        let pin = unsafe { tracked.as_ref() };
        assert_eq!(pin.node, list.first_node_ptr());
        assert_eq!(pin.y, 0);
        assert_eq!(pin.x, 0);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_history_bounded_keeps_remaining_history() {
        let capacity_rows = initial_capacity(80).rows();
        let mut list = PageList::init(80, capacity_rows, None).unwrap();
        list.grow_rows(4).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 4));
        list.scroll(Scroll::Top);

        list.erase_history(Some(point::Point::history(Coordinate::new(0, 1))))
            .unwrap();

        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));
        assert_eq!(list.viewport, Viewport::Top);
        assert_eq!(list.total_rows, capacity_rows + 2);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_active_regrows_and_shifts_pins() {
        let mut list = PageList::init(10, 5, None).unwrap();
        for y in 0..5 {
            set_row_marker(&mut list.pages[0].page, y, y as u32 + 10);
        }
        let shifted_pin = list
            .track_pin(Pin {
                node: list.first_node_ptr(),
                y: 4,
                x: 2,
                garbage: false,
            })
            .unwrap();
        let erased_pin = list
            .track_pin(Pin {
                node: list.first_node_ptr(),
                y: 1,
                x: 3,
                garbage: false,
            })
            .unwrap();

        list.erase_active(1).unwrap();

        assert_eq!(list.total_rows, list.rows);
        assert_eq!(row_marker(&list.pages[0].page, 0), 12);
        assert_eq!(row_marker(&list.pages[0].page, 1), 13);
        assert_eq!(row_marker(&list.pages[0].page, 2), 14);
        assert_eq!(row_marker(&list.pages[0].page, 3), 0);
        assert_eq!(row_marker(&list.pages[0].page, 4), 0);
        let shifted = unsafe { shifted_pin.as_ref() };
        assert_eq!(shifted.y, 2);
        assert_eq!(shifted.x, 2);
        let erased = unsafe { erased_pin.as_ref() };
        assert_eq!(erased.y, 0);
        assert_eq!(erased.x, 0);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_active_handles_mid_page_start() {
        let capacity_rows = initial_capacity(80).rows();
        assert!(capacity_rows > 4);
        let mut list = PageList::init(80, capacity_rows, None).unwrap();
        list.grow_rows(2).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));
        set_row_marker(&mut list.pages[0].page, 2, 22);
        set_row_marker(&mut list.pages[0].page, 3, 33);
        set_row_marker(&mut list.pages[0].page, 4, 44);
        let shifted_pin = list
            .track_pin(Pin {
                node: list.first_node_ptr(),
                y: 4,
                x: 5,
                garbage: false,
            })
            .unwrap();

        list.erase_active(1).unwrap();

        assert_eq!(list.total_rows, capacity_rows + 2);
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));
        assert_eq!(row_marker(&list.pages[0].page, 2), 44);
        assert_eq!(row_marker(&list.pages[0].page, 3), 0);
        let shifted = unsafe { shifted_pin.as_ref() };
        assert_eq!(shifted.y, 2);
        assert_eq!(shifted.x, 5);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_active_mid_page_to_page_end_moves_erased_pin_to_next_page() {
        let capacity_rows = initial_capacity(80).rows();
        assert!(capacity_rows > 4);
        let mut list = PageList::init(80, capacity_rows, None).unwrap();
        list.grow_rows(2).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));
        let erased_pin = list
            .track_pin(Pin {
                node: list.first_node_ptr(),
                y: 3,
                x: 5,
                garbage: false,
            })
            .unwrap();

        list.erase_active(capacity_rows - 3).unwrap();

        let pin = unsafe { erased_pin.as_ref() };
        assert_eq!(pin.node, NonNull::from(list.pages[1].as_ref()));
        assert_eq!(pin.y, 0);
        assert_eq!(pin.x, 0);
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_active_partial_releases_managed_memory() {
        let mut list = PageList::init(10, 5, None).unwrap();
        let erased_style = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let moved_style = style::Style {
            flags: style::Flags {
                italic: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let erased_style_id = list.pages[0].page.add_style(erased_style).unwrap();
        let moved_style_id = list.pages[0].page.add_style(moved_style).unwrap();
        let erased_link_id = list.pages[0]
            .page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"erase-rows-erased"),
                uri: b"https://example.com/erase-rows-erased",
            })
            .unwrap();
        let moved_link_id = list.pages[0]
            .page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"erase-rows-moved"),
                uri: b"https://example.com/erase-rows-moved",
            })
            .unwrap();
        {
            let page = &mut list.pages[0].page;
            for (y, ch, style_id, accent) in [
                (1, 'E', erased_style_id, 0x0301),
                (2, 'M', moved_style_id, 0x0302),
            ] {
                let rac = page.get_row_and_cell_mut(0, y);
                rac.row.set_styled(true);
                let mut cell = Cell::init(ch as u32);
                cell.set_style_id(style_id);
                *rac.cell = cell;
                page.use_style(style_id);
                page.append_grapheme_at(0, y, accent).unwrap();
            }
            page.set_hyperlink(0, 1, erased_link_id).unwrap();
            page.set_hyperlink(0, 2, moved_link_id).unwrap();
        }
        list.pages[0].page.release_style(erased_style_id);
        list.pages[0].page.release_style(moved_style_id);

        list.erase_active(1).unwrap();

        let page = &list.pages[0].page;
        assert_eq!(page.style_count(), 1);
        assert_eq!(page.hyperlink_count(), 1);
        assert_eq!(page.grapheme_count(), 1);
        let moved = page_cell(page, 0, 0);
        assert_eq!(moved.codepoint(), 'M' as u32);
        assert_eq!(page.get_style(moved.style_id()), moved_style);
        assert_eq!(page.lookup_grapheme_at(0, 0).unwrap(), vec![0x0302]);
        let moved_link = page.lookup_hyperlink_at(0, 0).unwrap();
        assert_eq!(
            page.get_hyperlink(moved_link),
            HyperlinkSnapshot {
                id: HyperlinkSnapshotId::Explicit(b"erase-rows-moved".to_vec()),
                uri: b"https://example.com/erase-rows-moved".to_vec(),
            }
        );
        assert!(!page.get_row(3).managed_memory());
        assert!(!page.get_row(4).managed_memory());
        assert_eq!(row_marker(page, 3), 0);
        assert_eq!(row_marker(page, 4), 0);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_active_rejects_middle_full_page() {
        let capacity_rows = initial_capacity(80).rows();
        let mut list = PageList::init(80, capacity_rows * 2, None).unwrap();
        list.grow_rows(2).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));
        let ptrs = list
            .pages
            .iter()
            .map(|node| NonNull::from(node.as_ref()))
            .collect::<Vec<_>>();
        let total_rows = list.total_rows;
        let page_size = list.page_size;
        let page_serial_min = list.page_serial_min;
        let y = capacity_rows * 2 - 3;

        assert_eq!(list.erase_active(y), Err(EraseRowsError::MiddlePage));

        assert_eq!(list.total_rows, total_rows);
        assert_eq!(list.page_size, page_size);
        assert_eq!(list.page_serial_min, page_serial_min);
        assert_eq!(
            list.pages
                .iter()
                .map(|node| NonNull::from(node.as_ref()))
                .collect::<Vec<_>>(),
            ptrs
        );
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_active_one_row() {
        let mut list = PageList::init(10, 1, None).unwrap();
        set_row_marker(&mut list.pages[0].page, 0, b'A' as u32);

        list.erase_active(0).unwrap();

        assert_eq!(list.total_rows, list.rows);
        assert_eq!(row_marker(&list.pages[0].page, 0), 0);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_history_all_moves_top_viewport_to_active() {
        let (mut list, _) = bounded_viewport_list(3);
        list.scroll(Scroll::Top);
        assert_eq!(list.viewport, Viewport::Top);

        list.erase_history(None).unwrap();

        assert_eq!(list.viewport, Viewport::Active);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_erase_history_updates_pinned_viewport_cache() {
        let (mut list, _) = bounded_viewport_list(3);
        list.scroll(Scroll::Row(2));
        assert_eq!(list.viewport, Viewport::Pin);
        let _ = list.scrollbar();
        assert_eq!(list.viewport_pin_row_offset, Some(2));

        list.erase_history(Some(point::Point::history(Coordinate::new(0, 0))))
            .unwrap();

        assert_eq!(list.viewport, Viewport::Pin);
        assert_eq!(list.viewport_pin_row_offset, Some(1));
        assert_eq!(list.viewport_pin.y, 1);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_scroll_clear_empty_active_scrolls_zero_rows() {
        let mut list = PageList::init(10, 5, None).unwrap();
        let total_rows = list.total_rows;
        let page_count = list.pages.len();

        list.scroll_clear().unwrap();

        assert_eq!(list.total_rows, total_rows);
        assert_eq!(list.pages.len(), page_count);
        for y in 0..list.rows {
            assert_eq!(active_row_marker(&list, y), 0);
        }
        assert_eq!(list.viewport, Viewport::Active);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_scroll_clear_ignores_non_empty_history() {
        let capacity_rows = initial_capacity(80).rows();
        let mut list = PageList::init(80, capacity_rows, None).unwrap();
        list.grow_rows(1).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 1));
        set_row_marker(&mut list.pages[0].page, 0, 99);
        let total_rows = list.total_rows;

        list.scroll_clear().unwrap();

        assert_eq!(list.total_rows, total_rows);
        assert_eq!(row_marker(&list.pages[0].page, 0), 99);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_scroll_clear_top_active_row_scrolls_one_row() {
        let mut list = PageList::init(10, 5, None).unwrap();
        set_active_row_marker(&mut list, 0, 11);

        list.scroll_clear().unwrap();

        assert_eq!(list.total_rows, 6);
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 1));
        for y in 0..list.rows {
            assert_eq!(active_row_marker(&list, y), 0);
        }
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_scroll_clear_middle_active_row_uses_active_y() {
        let mut list = PageList::init(10, 5, None).unwrap();
        set_active_row_marker(&mut list, 2, 22);

        list.scroll_clear().unwrap();

        assert_eq!(list.total_rows, 8);
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 3));
        for y in 0..list.rows {
            assert_eq!(active_row_marker(&list, y), 0);
        }
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_scroll_clear_bottom_active_row_scrolls_all_rows() {
        let mut list = PageList::init(10, 5, None).unwrap();
        set_active_row_marker(&mut list, 4, 44);

        list.scroll_clear().unwrap();

        assert_eq!(list.total_rows, 10);
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 5));
        for y in 0..list.rows {
            assert_eq!(active_row_marker(&list, y), 0);
        }
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_scroll_clear_active_spans_partial_pages() {
        let capacity_rows = initial_capacity(80).rows();
        let mut list = PageList::init(80, capacity_rows, None).unwrap();
        list.grow_rows(2).unwrap();
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));
        set_active_row_marker(&mut list, 1, 77);
        let total_rows = list.total_rows;

        list.scroll_clear().unwrap();

        assert_eq!(list.total_rows, total_rows + 2);
        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 4));
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_scroll_clear_cell_empty_semantics() {
        {
            let mut list = PageList::init(10, 5, None).unwrap();
            let styled = style::Style {
                flags: style::Flags {
                    bold: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            };
            let style_id = list.pages[0].page.add_style(styled).unwrap();
            let mut cell = Cell::init('s' as u32);
            cell.set_style_id(style_id);
            set_active_cell(&mut list, 0, cell);
            list.pages[0].page.use_style(style_id);
            list.pages[0].page.release_style(style_id);

            list.scroll_clear().unwrap();

            assert_eq!(list.total_rows, 6);
            list.verify_integrity().unwrap();
        }

        {
            let mut list = PageList::init(10, 5, None).unwrap();
            set_active_cell(&mut list, 0, Cell::init('g' as u32));
            list.pages[0].page.append_grapheme_at(0, 0, 0x0301).unwrap();

            list.scroll_clear().unwrap();

            assert_eq!(list.total_rows, 6);
            list.verify_integrity().unwrap();
        }

        {
            let mut list = PageList::init(10, 5, None).unwrap();
            let link_id = list.pages[0]
                .page
                .insert_hyperlink(hyperlink::Hyperlink {
                    id: hyperlink::HyperlinkId::Explicit(b"scroll-clear"),
                    uri: b"https://example.com/scroll-clear",
                })
                .unwrap();
            set_active_cell(&mut list, 0, Cell::init('h' as u32));
            list.pages[0].page.set_hyperlink(0, 0, link_id).unwrap();

            list.scroll_clear().unwrap();

            assert_eq!(list.total_rows, 6);
            list.verify_integrity().unwrap();
        }

        for cell in {
            let mut spacer = Cell::default();
            spacer.set_wide(Wide::SpacerTail);
            [spacer, Cell::bg_palette(1)]
        } {
            let mut list = PageList::init(10, 5, None).unwrap();
            set_active_cell(&mut list, 0, cell);

            list.scroll_clear().unwrap();

            assert_eq!(list.total_rows, 6);
            list.verify_integrity().unwrap();
        }
    }

    #[test]
    fn page_list_scroll_clear_preserves_viewport_modes() {
        let mut active = PageList::init(10, 5, None).unwrap();
        set_active_row_marker(&mut active, 0, 1);
        active.scroll_clear().unwrap();
        assert_eq!(active.viewport, Viewport::Active);

        let (mut top, _) = bounded_viewport_list(2);
        top.scroll(Scroll::Top);
        set_active_row_marker(&mut top, 0, 1);
        top.scroll_clear().unwrap();
        assert_eq!(top.viewport, Viewport::Top);

        let (mut pinned, _) = bounded_viewport_list(2);
        pinned.scroll(Scroll::Row(2));
        let _ = pinned.scrollbar();
        assert_eq!(pinned.viewport, Viewport::Pin);
        assert_eq!(pinned.viewport_pin_row_offset, Some(2));
        set_active_row_marker(&mut pinned, 0, 1);
        pinned.scroll_clear().unwrap();
        assert_eq!(pinned.viewport, Viewport::Pin);
        assert_eq!(pinned.viewport_pin_row_offset, Some(2));
        pinned.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_integrity_rejects_viewport_pin_offset_mismatch() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.pages[0].page.set_size_rows(30);
        list.total_rows = 30;
        list.viewport = Viewport::Pin;
        list.viewport_pin.y = 4;
        list.viewport_pin_row_offset = Some(5);

        assert_eq!(
            list.verify_integrity(),
            Err(IntegrityError::ViewportPinOffsetMismatch)
        );
    }

    #[test]
    fn page_list_integrity_rejects_viewport_pin_without_enough_rows() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.pages[0].page.set_size_rows(30);
        list.total_rows = 30;
        list.viewport = Viewport::Pin;
        list.viewport_pin.y = 10;

        assert_eq!(
            list.verify_integrity(),
            Err(IntegrityError::ViewportPinInsufficientRows)
        );
    }

    #[test]
    fn page_list_pin_is_active_matches_active_top_left() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.pages[0].page.set_size_rows(30);
        list.total_rows = 30;

        let node = NonNull::from(list.pages[0].as_ref());
        assert!(!list.pin_is_active(Pin {
            node,
            y: 5,
            x: 0,
            garbage: false,
        }));
        assert!(list.pin_is_active(Pin {
            node,
            y: 6,
            x: 0,
            garbage: false,
        }));
        assert!(list.pin_is_active(Pin {
            node,
            y: 29,
            x: 0,
            garbage: false,
        }));
    }

    #[test]
    fn page_list_pin_is_top_requires_first_node_row_zero() {
        let cols = 50;
        let capacity = initial_capacity(cols);
        let total_rows = capacity.rows() * 2;
        let list = PageList::init(cols, total_rows, None).unwrap();
        assert_eq!(list.pages.len(), 2);

        let first = NonNull::from(list.pages[0].as_ref());
        let second = NonNull::from(list.pages[1].as_ref());
        assert!(list.pin_is_top(Pin {
            node: first,
            y: 0,
            x: 0,
            garbage: false,
        }));
        assert!(!list.pin_is_top(Pin {
            node: first,
            y: 1,
            x: 0,
            garbage: false,
        }));
        assert!(!list.pin_is_top(Pin {
            node: second,
            y: 0,
            x: 0,
            garbage: false,
        }));
    }

    #[test]
    fn page_list_scroll_max_size_zero_stays_active() {
        let mut list = PageList::init(80, 24, Some(0)).unwrap();
        simulate_history(&mut list, 30);
        let before = viewport_top_left_screen_coord(&list);
        let pin = Pin {
            node: NonNull::from(list.pages[0].as_ref()),
            y: 4,
            x: 2,
            garbage: false,
        };

        for behavior in [
            Scroll::Top,
            Scroll::Pin(pin),
            Scroll::Row(4),
            Scroll::DeltaRow(-3),
        ] {
            list.scroll(behavior);
            assert_eq!(list.viewport, Viewport::Active);
            assert_eq!(viewport_top_left_screen_coord(&list), before);
            assert_eq!(
                list.scrollbar(),
                Scrollbar {
                    total: 24,
                    offset: 0,
                    len: 24,
                }
            );
        }
    }

    #[test]
    fn page_list_scroll_top_moves_viewport_to_top() {
        let mut list = PageList::init(80, 24, None).unwrap();
        simulate_history(&mut list, 34);

        list.scroll(Scroll::Top);

        assert_eq!(list.viewport, Viewport::Top);
        assert_eq!(viewport_top_left_screen_coord(&list), Coordinate::new(0, 0));
        assert_eq!(
            list.scrollbar(),
            Scrollbar {
                total: 34,
                offset: 0,
                len: 24,
            }
        );
    }

    #[test]
    fn page_list_scroll_active_returns_to_active_viewport() {
        let mut list = PageList::init(80, 24, None).unwrap();
        simulate_history(&mut list, 34);
        list.scroll(Scroll::Top);

        list.scroll(Scroll::Active);

        assert_eq!(list.viewport, Viewport::Active);
        assert_eq!(
            viewport_top_left_screen_coord(&list),
            Coordinate::new(0, 10)
        );
        assert_eq!(
            list.scrollbar(),
            Scrollbar {
                total: 34,
                offset: 10,
                len: 24,
            }
        );
    }

    #[test]
    fn page_list_scroll_delta_row_back_from_active() {
        let mut list = PageList::init(80, 24, None).unwrap();
        simulate_history(&mut list, 34);

        list.scroll(Scroll::DeltaRow(-1));

        assert_eq!(list.viewport, Viewport::Pin);
        assert_eq!(viewport_top_left_screen_coord(&list), Coordinate::new(0, 9));
        assert_eq!(
            list.scrollbar(),
            Scrollbar {
                total: 34,
                offset: 9,
                len: 24,
            }
        );
    }

    #[test]
    fn page_list_scroll_delta_row_back_overflow_clamps_to_top() {
        let mut list = PageList::init(80, 24, None).unwrap();
        simulate_history(&mut list, 34);

        list.scroll(Scroll::DeltaRow(-100));

        assert_eq!(list.viewport, Viewport::Top);
        assert_eq!(viewport_top_left_screen_coord(&list), Coordinate::new(0, 0));
        assert_eq!(
            list.scrollbar(),
            Scrollbar {
                total: 34,
                offset: 0,
                len: 24,
            }
        );
    }

    #[test]
    fn page_list_scroll_delta_row_back_without_history_preserves_active() {
        let mut list = PageList::init(80, 24, None).unwrap();

        list.scroll(Scroll::DeltaRow(-1));

        assert_eq!(list.viewport, Viewport::Active);
        assert_eq!(viewport_top_left_screen_coord(&list), Coordinate::new(0, 0));
        assert_eq!(
            list.scrollbar(),
            Scrollbar {
                total: 24,
                offset: 0,
                len: 24,
            }
        );
    }

    #[test]
    fn page_list_scroll_delta_row_forward_from_top_creates_pin() {
        let mut list = PageList::init(80, 24, None).unwrap();
        simulate_history(&mut list, 34);
        list.scroll(Scroll::Top);

        list.scroll(Scroll::DeltaRow(2));

        assert_eq!(list.viewport, Viewport::Pin);
        assert_eq!(viewport_top_left_screen_coord(&list), Coordinate::new(0, 2));
        assert_eq!(
            list.scrollbar(),
            Scrollbar {
                total: 34,
                offset: 2,
                len: 24,
            }
        );
    }

    #[test]
    fn page_list_scroll_delta_row_forward_into_active_clamps_to_active() {
        let mut list = PageList::init(80, 24, None).unwrap();
        simulate_history(&mut list, 34);
        list.scroll(Scroll::Top);

        list.scroll(Scroll::DeltaRow(10));

        assert_eq!(list.viewport, Viewport::Active);
        assert_eq!(
            viewport_top_left_screen_coord(&list),
            Coordinate::new(0, 10)
        );
        assert_eq!(
            list.scrollbar(),
            Scrollbar {
                total: 34,
                offset: 10,
                len: 24,
            }
        );
    }

    #[test]
    fn page_list_scroll_to_pin_in_scrollback_ignores_x() {
        let mut list = PageList::init(80, 24, None).unwrap();
        simulate_history(&mut list, 34);
        let pin = Pin {
            node: NonNull::from(list.pages[0].as_ref()),
            y: 4,
            x: 2,
            garbage: false,
        };

        list.scroll(Scroll::Pin(pin));

        assert_eq!(list.viewport, Viewport::Pin);
        assert_eq!(viewport_top_left_screen_coord(&list), Coordinate::new(0, 4));
        assert_eq!(
            list.scrollbar(),
            Scrollbar {
                total: 34,
                offset: 4,
                len: 24,
            }
        );
    }

    #[test]
    fn page_list_scroll_to_pin_in_active_clamps_to_active() {
        let mut list = PageList::init(80, 24, None).unwrap();
        simulate_history(&mut list, 34);
        let pin = Pin {
            node: NonNull::from(list.pages[0].as_ref()),
            y: 30,
            x: 2,
            garbage: false,
        };

        list.scroll(Scroll::Pin(pin));

        assert_eq!(list.viewport, Viewport::Active);
        assert_eq!(
            viewport_top_left_screen_coord(&list),
            Coordinate::new(0, 10)
        );
    }

    #[test]
    fn page_list_scroll_to_pin_at_top_clamps_to_top() {
        let mut list = PageList::init(80, 24, None).unwrap();
        simulate_history(&mut list, 34);
        let pin = Pin {
            node: NonNull::from(list.pages[0].as_ref()),
            y: 0,
            x: 2,
            garbage: false,
        };

        list.scroll(Scroll::Pin(pin));

        assert_eq!(list.viewport, Viewport::Top);
        assert_eq!(viewport_top_left_screen_coord(&list), Coordinate::new(0, 0));
    }

    #[test]
    fn page_list_scroll_to_row_zero_clamps_to_top() {
        let mut list = PageList::init(80, 24, None).unwrap();
        simulate_history(&mut list, 34);

        list.scroll(Scroll::Row(0));

        assert_eq!(list.viewport, Viewport::Top);
        assert_eq!(viewport_top_left_screen_coord(&list), Coordinate::new(0, 0));
        assert_eq!(list.scrollbar().offset, 0);
    }

    #[test]
    fn page_list_scroll_to_row_in_scrollback_sets_cache() {
        let mut list = PageList::init(80, 24, None).unwrap();
        simulate_history(&mut list, 44);

        list.scroll(Scroll::Row(5));

        assert_eq!(list.viewport, Viewport::Pin);
        assert_eq!(list.viewport_pin_row_offset, Some(5));
        assert_eq!(viewport_top_left_screen_coord(&list), Coordinate::new(0, 5));
        assert_eq!(
            list.scrollbar(),
            Scrollbar {
                total: 44,
                offset: 5,
                len: 24,
            }
        );
    }

    #[test]
    fn page_list_scroll_to_row_in_middle() {
        let mut list = PageList::init(80, 24, None).unwrap();
        simulate_history(&mut list, 74);

        list.scroll(Scroll::Row(37));

        assert_eq!(list.viewport, Viewport::Pin);
        assert_eq!(list.viewport_pin_row_offset, Some(37));
        assert_eq!(
            viewport_top_left_screen_coord(&list),
            Coordinate::new(0, 37)
        );
        assert_eq!(list.scrollbar().offset, 37);
    }

    #[test]
    fn page_list_scroll_to_row_at_active_boundary_clamps_to_active() {
        let mut list = PageList::init(80, 24, None).unwrap();
        simulate_history(&mut list, 44);

        list.scroll(Scroll::Row(20));

        assert_eq!(list.viewport, Viewport::Active);
        assert_eq!(
            viewport_top_left_screen_coord(&list),
            Coordinate::new(0, 20)
        );
        assert_eq!(list.scrollbar().offset, 20);
    }

    #[test]
    fn page_list_scroll_to_row_beyond_active_clamps_to_active() {
        let mut list = PageList::init(80, 24, None).unwrap();
        simulate_history(&mut list, 34);

        list.scroll(Scroll::Row(1000));

        assert_eq!(list.viewport, Viewport::Active);
        assert_eq!(
            viewport_top_left_screen_coord(&list),
            Coordinate::new(0, 10)
        );
        assert_eq!(list.scrollbar().offset, 10);
    }

    #[test]
    fn page_list_scroll_to_row_without_scrollback_preserves_active() {
        let mut list = PageList::init(80, 24, None).unwrap();

        list.scroll(Scroll::Row(5));

        assert_eq!(list.viewport, Viewport::Active);
        assert_eq!(viewport_top_left_screen_coord(&list), Coordinate::new(0, 0));
        assert_eq!(list.scrollbar().offset, 0);
    }

    #[test]
    fn page_list_scroll_to_row_then_delta_row() {
        let mut list = PageList::init(80, 24, None).unwrap();
        simulate_history(&mut list, 54);

        list.scroll(Scroll::Row(10));
        assert_eq!(
            viewport_top_left_screen_coord(&list),
            Coordinate::new(0, 10)
        );
        assert_eq!(list.scrollbar().offset, 10);

        list.scroll(Scroll::DeltaRow(5));
        assert_eq!(
            viewport_top_left_screen_coord(&list),
            Coordinate::new(0, 15)
        );
        assert_eq!(list.scrollbar().offset, 15);

        list.scroll(Scroll::DeltaRow(-3));
        assert_eq!(
            viewport_top_left_screen_coord(&list),
            Coordinate::new(0, 12)
        );
        assert_eq!(list.scrollbar().offset, 12);
    }

    #[test]
    fn page_list_scroll_to_row_uses_cache_fast_path_down() {
        let mut list = PageList::init(80, 24, None).unwrap();
        simulate_history(&mut list, 74);

        list.scroll(Scroll::Row(10));
        assert_eq!(list.viewport_pin_row_offset, Some(10));
        list.scroll(Scroll::Row(20));

        assert_eq!(list.viewport, Viewport::Pin);
        assert_eq!(list.viewport_pin_row_offset, Some(20));
        assert_eq!(
            viewport_top_left_screen_coord(&list),
            Coordinate::new(0, 20)
        );
        assert_eq!(list.scrollbar().offset, 20);
    }

    #[test]
    fn page_list_scroll_to_row_uses_cache_fast_path_up() {
        let mut list = PageList::init(80, 24, None).unwrap();
        simulate_history(&mut list, 74);

        list.scroll(Scroll::Row(30));
        assert_eq!(list.viewport_pin_row_offset, Some(30));
        list.scroll(Scroll::Row(20));

        assert_eq!(list.viewport, Viewport::Pin);
        assert_eq!(list.viewport_pin_row_offset, Some(20));
        assert_eq!(
            viewport_top_left_screen_coord(&list),
            Coordinate::new(0, 20)
        );
        assert_eq!(list.scrollbar().offset, 20);
    }

    #[test]
    fn page_list_integrity_rejects_total_rows_mismatch() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.total_rows += 1;

        assert_eq!(
            list.verify_integrity(),
            Err(IntegrityError::TotalRowsMismatch)
        );
    }

    #[test]
    fn page_list_integrity_rejects_invalid_serial() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.page_serial_min = list.pages[0].serial + 1;

        assert_eq!(
            list.verify_integrity(),
            Err(IntegrityError::PageSerialInvalid)
        );
    }

    #[test]
    fn page_list_integrity_rejects_garbage_viewport_pin() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.viewport_pin.garbage = true;

        assert_eq!(
            list.verify_integrity(),
            Err(IntegrityError::ViewportPinGarbage)
        );
    }

    #[test]
    fn page_list_integrity_rejects_viewport_pin_x_out_of_bounds() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.viewport_pin.x = list.pages[0].page.size_cols();

        assert_eq!(
            list.verify_integrity(),
            Err(IntegrityError::TrackedPinInvalid)
        );
    }

    #[test]
    fn page_list_integrity_rejects_viewport_pin_y_out_of_bounds() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.viewport_pin.y = list.pages[0].page.size_rows();

        assert_eq!(
            list.verify_integrity(),
            Err(IntegrityError::TrackedPinInvalid)
        );
    }

    #[test]
    fn page_list_point_from_pin_active_no_history() {
        let list = PageList::init(80, 24, None).unwrap();

        assert_eq!(
            list.point_from_pin(
                point::Tag::Active,
                Pin {
                    node: NonNull::from(list.pages[0].as_ref()),
                    y: 0,
                    x: 0,
                    garbage: false,
                },
            ),
            Some(point::Point::active(Coordinate::new(0, 0)))
        );
        assert_eq!(
            list.point_from_pin(
                point::Tag::Active,
                Pin {
                    node: NonNull::from(list.pages[0].as_ref()),
                    y: 2,
                    x: 4,
                    garbage: false,
                },
            ),
            Some(point::Point::active(Coordinate::new(4, 2)))
        );
    }

    #[test]
    fn page_list_pin_active_point() {
        let list = PageList::init(80, 24, None).unwrap();
        let pin = list
            .pin(point::Point::active(Coordinate::new(4, 2)))
            .unwrap();

        assert_eq!(pin.node, NonNull::from(list.pages[0].as_ref()));
        assert_eq!(pin.x, 4);
        assert_eq!(pin.y, 2);
    }

    #[test]
    fn page_list_pin_rejects_out_of_bounds_x() {
        let list = PageList::init(80, 24, None).unwrap();

        assert_eq!(list.pin(point::Point::active(Coordinate::new(80, 0))), None);
    }

    #[test]
    fn page_list_pin_rejects_out_of_bounds_y() {
        let list = PageList::init(80, 24, None).unwrap();

        assert_eq!(list.pin(point::Point::active(Coordinate::new(0, 24))), None);
    }

    #[test]
    fn page_list_viewport_point_conversion_preserves_tag() {
        let list = PageList::init(80, 24, None).unwrap();
        let pin = list
            .pin(point::Point::viewport(Coordinate::new(3, 5)))
            .unwrap();

        assert_eq!(pin.node, NonNull::from(list.pages[0].as_ref()));
        assert_eq!(pin.x, 3);
        assert_eq!(pin.y, 5);
        assert_eq!(
            list.point_from_pin(point::Tag::Viewport, pin),
            Some(point::Point::viewport(Coordinate::new(3, 5)))
        );
    }

    #[test]
    fn page_list_history_point_conversion_preserves_upstream_no_history_semantics() {
        let list = PageList::init(80, 24, None).unwrap();
        let pin = list
            .pin(point::Point::history(Coordinate::new(2, 4)))
            .unwrap();

        assert_eq!(pin.node, NonNull::from(list.pages[0].as_ref()));
        assert_eq!(pin.x, 2);
        assert_eq!(pin.y, 4);
        assert_eq!(
            list.point_from_pin(point::Tag::History, pin),
            Some(point::Point::history(Coordinate::new(2, 4)))
        );
        assert_eq!(list.get_bottom_right(point::Tag::History), None);
    }

    #[test]
    fn page_list_get_top_left_active_multi_page_initialized_list() {
        let rows = 100;
        let mut capacity = STD_CAPACITY.adjust(CapacityAdjustment::cols(50)).unwrap();
        while capacity.rows() >= rows {
            capacity = STD_CAPACITY
                .adjust(CapacityAdjustment::cols(capacity.cols() + 50))
                .unwrap();
        }
        let list = PageList::init(capacity.cols(), rows, None).unwrap();
        let top_left = list.get_top_left(point::Tag::Active);

        assert_eq!(top_left.node, NonNull::from(list.pages[0].as_ref()));
        assert_eq!(top_left.y, 0);
        assert_eq!(top_left.x, 0);
    }

    #[test]
    fn page_list_point_from_pin_screen_accumulates_rows_across_pages() {
        let rows = 100;
        let mut capacity = STD_CAPACITY.adjust(CapacityAdjustment::cols(50)).unwrap();
        while capacity.rows() >= rows {
            capacity = STD_CAPACITY
                .adjust(CapacityAdjustment::cols(capacity.cols() + 50))
                .unwrap();
        }
        let list = PageList::init(capacity.cols(), rows, None).unwrap();
        assert!(list.pages.len() > 1);

        let pin = Pin {
            node: NonNull::from(list.pages[1].as_ref()),
            y: 5,
            x: 2,
            garbage: false,
        };
        assert_eq!(
            list.point_from_pin(point::Tag::Screen, pin),
            Some(point::Point::screen(Coordinate::new(
                2,
                capacity.rows() as u32 + 5
            )))
        );
    }

    #[test]
    fn page_list_get_bottom_right_active_returns_last_active_cell() {
        let list = PageList::init(80, 24, None).unwrap();
        let bottom_right = list.get_bottom_right(point::Tag::Active).unwrap();

        assert_eq!(bottom_right.node, NonNull::from(list.pages[0].as_ref()));
        assert_eq!(bottom_right.x, 79);
        assert_eq!(bottom_right.y, 23);
    }

    #[test]
    fn page_list_point_from_pin_rejects_pin_before_active_top_left() {
        let mut list = PageList::init(80, 24, None).unwrap();
        list.pages[0].page.set_size_rows(30);
        list.total_rows = 30;
        list.rows = 24;
        let active_top_left = list.get_top_left(point::Tag::Active);
        assert_eq!(active_top_left.y, 6);

        let before_active = Pin {
            node: NonNull::from(list.pages[0].as_ref()),
            y: 5,
            x: 0,
            garbage: false,
        };
        assert_eq!(list.point_from_pin(point::Tag::Active, before_active), None);
    }

    #[test]
    fn pin_navigation_before_within_row_and_equal() {
        let list = PageList::init(10, 5, None).unwrap();
        let left = screen_pin(&list, 2, 3);
        let right = screen_pin(&list, 7, 3);

        assert_eq!(list.pin_before(left, right), Some(true));
        assert_eq!(list.pin_before(right, left), Some(false));
        assert_eq!(list.pin_before(left, left), Some(false));
    }

    #[test]
    fn pin_navigation_before_across_rows_and_pages() {
        let mut list = PageList::init(4, 4, Some(0)).unwrap();
        let first = list.first_node_ptr();
        list.split(Pin::new(first, 2, 0)).unwrap();
        let top = screen_pin(&list, 3, 1);
        let bottom = screen_pin(&list, 0, 2);

        assert_ne!(top.node, bottom.node);
        assert_eq!(list.pin_before(screen_pin(&list, 0, 0), top), Some(true));
        assert_eq!(list.pin_before(top, bottom), Some(true));
        assert_eq!(list.pin_before(bottom, top), Some(false));
    }

    #[test]
    fn pin_navigation_before_rejects_invalid_or_garbage() {
        let list = PageList::init(10, 5, None).unwrap();
        let other = PageList::init(10, 5, None).unwrap();
        let valid = screen_pin(&list, 2, 3);
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let mut garbage = valid;
        garbage.garbage = true;

        assert_eq!(list.pin_before(invalid, valid), None);
        assert_eq!(list.pin_before(valid, invalid), None);
        assert_eq!(list.pin_before(garbage, valid), None);
        assert_eq!(list.pin_before(valid, garbage), None);
    }

    #[test]
    fn pin_navigation_clamp_moves_and_saturates() {
        let list = PageList::init(10, 5, None).unwrap();
        let pin = screen_pin(&list, 5, 3);

        assert_eq!(
            screen_coord(&list, list.pin_left_clamp(pin, 1).unwrap()),
            Coordinate::new(4, 3)
        );
        assert_eq!(
            screen_coord(&list, list.pin_right_clamp(pin, 1).unwrap()),
            Coordinate::new(6, 3)
        );
        assert_eq!(
            screen_coord(&list, list.pin_left_clamp(pin, 9).unwrap()),
            Coordinate::new(0, 3)
        );
        assert_eq!(
            screen_coord(&list, list.pin_right_clamp(pin, 9).unwrap()),
            Coordinate::new(9, 3)
        );
    }

    #[test]
    fn pin_navigation_clamp_rejects_invalid_or_garbage() {
        let list = PageList::init(10, 5, None).unwrap();
        let other = PageList::init(10, 5, None).unwrap();
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let mut garbage = screen_pin(&list, 2, 3);
        garbage.garbage = true;

        assert!(list.pin_left_clamp(invalid, 1).is_none());
        assert!(list.pin_right_clamp(invalid, 1).is_none());
        assert!(list.pin_left_clamp(garbage, 1).is_none());
        assert!(list.pin_right_clamp(garbage, 1).is_none());
    }

    #[test]
    fn pin_navigation_wrap_moves_within_row() {
        let list = PageList::init(10, 5, None).unwrap();
        let pin = screen_pin(&list, 5, 3);

        assert_eq!(
            screen_coord(&list, list.pin_left_wrap(pin, 1).unwrap()),
            Coordinate::new(4, 3)
        );
        assert_eq!(
            screen_coord(&list, list.pin_right_wrap(pin, 1).unwrap()),
            Coordinate::new(6, 3)
        );
    }

    #[test]
    fn pin_navigation_wrap_crosses_rows() {
        let list = PageList::init(10, 5, None).unwrap();

        assert_eq!(
            screen_coord(
                &list,
                list.pin_left_wrap(screen_pin(&list, 0, 3), 1).unwrap()
            ),
            Coordinate::new(9, 2)
        );
        assert_eq!(
            screen_coord(
                &list,
                list.pin_right_wrap(screen_pin(&list, 9, 2), 1).unwrap()
            ),
            Coordinate::new(0, 3)
        );
    }

    #[test]
    fn pin_navigation_wrap_returns_none_at_edges() {
        let list = PageList::init(10, 5, None).unwrap();

        assert!(list.pin_left_wrap(screen_pin(&list, 0, 0), 1).is_none());
        assert!(list.pin_right_wrap(screen_pin(&list, 9, 4), 1).is_none());
    }

    #[test]
    fn pin_navigation_wrap_crosses_page_boundaries() {
        let mut list = PageList::init(4, 4, Some(0)).unwrap();
        let first = list.first_node_ptr();
        list.split(Pin::new(first, 2, 0)).unwrap();
        let before_boundary = screen_pin(&list, 3, 1);
        let after_boundary = screen_pin(&list, 0, 2);

        assert_ne!(before_boundary.node, after_boundary.node);
        assert_eq!(
            list.pin_right_wrap(before_boundary, 1),
            Some(after_boundary)
        );
        assert_eq!(list.pin_left_wrap(after_boundary, 1), Some(before_boundary));
    }

    #[test]
    fn pin_navigation_wrap_rejects_invalid_or_garbage() {
        let list = PageList::init(10, 5, None).unwrap();
        let other = PageList::init(10, 5, None).unwrap();
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let mut garbage = screen_pin(&list, 2, 3);
        garbage.garbage = true;

        assert!(list.pin_left_wrap(invalid, 1).is_none());
        assert!(list.pin_right_wrap(invalid, 1).is_none());
        assert!(list.pin_left_wrap(garbage, 1).is_none());
        assert!(list.pin_right_wrap(garbage, 1).is_none());
    }

    #[test]
    fn drag_selection_regular_matches_upstream_ltr_cases() {
        assert_drag_selection((3.0, 3), (3.9, 3), (3, 3), (3, 3), false);
        assert_drag_selection((3.0, 3), (5.9, 3), (3, 3), (5, 3), false);
        assert_drag_selection((3.0, 3), (5.0, 3), (3, 3), (4, 3), false);
        assert_drag_selection((3.9, 3), (5.9, 3), (4, 3), (5, 3), false);
        assert_drag_selection((3.9, 3), (5.0, 3), (4, 3), (4, 3), false);
        assert_drag_selection_is_none((3.0, 3), (3.1, 3), false);
        assert_drag_selection_is_none((3.8, 3), (3.9, 3), false);
        assert_drag_selection_is_none((3.9, 3), (4.0, 3), false);
    }

    #[test]
    fn drag_selection_regular_matches_upstream_rtl_cases() {
        assert_drag_selection((3.9, 3), (3.0, 3), (3, 3), (3, 3), false);
        assert_drag_selection((5.9, 3), (3.0, 3), (5, 3), (3, 3), false);
        assert_drag_selection((5.9, 3), (3.9, 3), (5, 3), (4, 3), false);
        assert_drag_selection((5.0, 3), (3.0, 3), (4, 3), (3, 3), false);
        assert_drag_selection((5.0, 3), (3.9, 3), (4, 3), (4, 3), false);
        assert_drag_selection_is_none((3.1, 3), (3.0, 3), false);
        assert_drag_selection_is_none((3.9, 3), (3.8, 3), false);
        assert_drag_selection_is_none((4.0, 3), (3.9, 3), false);
    }

    #[test]
    fn drag_selection_regular_wraps_rows_like_upstream() {
        assert_drag_selection((9.9, 2), (0.0, 4), (0, 3), (9, 3), false);
        assert_drag_selection((0.0, 4), (9.9, 2), (9, 3), (0, 3), false);
    }

    #[test]
    fn drag_selection_regular_wrap_failure_falls_back_to_original_pin() {
        assert_drag_selection_is_none((0.5, 0), (0.0, 0), false);
        assert_drag_selection_is_none((9.9, 4), (9.9, 4), false);
    }

    #[test]
    fn drag_selection_rectangle_matches_upstream_ltr_cases() {
        assert_drag_selection((3.0, 2), (3.9, 4), (3, 2), (3, 4), true);
        assert_drag_selection((3.0, 2), (5.9, 4), (3, 2), (5, 4), true);
        assert_drag_selection((3.0, 2), (5.0, 4), (3, 2), (4, 4), true);
        assert_drag_selection((3.9, 2), (5.9, 4), (4, 2), (5, 4), true);
        assert_drag_selection((3.9, 2), (5.0, 4), (4, 2), (4, 4), true);
        assert_drag_selection_is_none((3.0, 2), (3.1, 4), true);
        assert_drag_selection_is_none((3.8, 2), (3.9, 4), true);
        assert_drag_selection_is_none((3.9, 2), (4.0, 4), true);
    }

    #[test]
    fn drag_selection_rectangle_matches_upstream_rtl_cases() {
        assert_drag_selection((3.9, 2), (3.0, 4), (3, 2), (3, 4), true);
        assert_drag_selection((5.9, 2), (3.0, 4), (5, 2), (3, 4), true);
        assert_drag_selection((5.9, 2), (3.9, 4), (5, 2), (4, 4), true);
        assert_drag_selection((5.0, 2), (3.0, 4), (4, 2), (3, 4), true);
        assert_drag_selection((5.0, 2), (3.9, 4), (4, 2), (4, 4), true);
        assert_drag_selection_is_none((3.1, 2), (3.0, 4), true);
        assert_drag_selection_is_none((3.9, 2), (3.8, 4), true);
        assert_drag_selection_is_none((4.0, 2), (3.9, 4), true);
    }

    #[test]
    fn drag_selection_rectangle_does_not_wrap_like_upstream() {
        assert_drag_selection((9.9, 2), (0.0, 4), (9, 2), (0, 4), true);
        assert_drag_selection((0.0, 4), (9.9, 2), (0, 4), (9, 2), true);
    }

    #[test]
    fn drag_selection_handles_padding_and_invalid_geometry() {
        let list = PageList::init(10, 5, None).unwrap();
        let mut geometry = drag_geometry();
        geometry.padding_left = 50;
        assert_eq!(
            list.drag_selection(
                screen_pin(&list, 3, 3),
                screen_pin(&list, 3, 3),
                10,
                39,
                false,
                geometry,
            ),
            None
        );

        for geometry in [
            DragGeometry {
                columns: 0,
                ..drag_geometry()
            },
            DragGeometry {
                cell_width: 0,
                ..drag_geometry()
            },
            DragGeometry {
                columns: u32::MAX,
                cell_width: 2,
                ..drag_geometry()
            },
        ] {
            assert!(list
                .drag_selection(
                    screen_pin(&list, 3, 3),
                    screen_pin(&list, 4, 3),
                    35,
                    45,
                    false,
                    geometry,
                )
                .is_none());
        }
    }

    #[test]
    fn drag_selection_rejects_invalid_or_garbage_pins() {
        let list = PageList::init(10, 5, None).unwrap();
        let other = PageList::init(10, 5, None).unwrap();
        let valid = screen_pin(&list, 3, 3);
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let mut garbage = valid;
        garbage.garbage = true;
        let geometry = drag_geometry();

        assert!(list
            .drag_selection(invalid, valid, 35, 45, false, geometry)
            .is_none());
        assert!(list
            .drag_selection(valid, invalid, 35, 45, false, geometry)
            .is_none());
        assert!(list
            .drag_selection(garbage, valid, 35, 45, false, geometry)
            .is_none());
        assert!(list
            .drag_selection(valid, garbage, 35, 45, false, geometry)
            .is_none());
    }

    #[test]
    fn select_all_matches_upstream_cases() {
        let mut list = PageList::init(10, 10, None).unwrap();
        set_screen_text_lines(&mut list, &["ABC  DEF", " 123", "456"]);
        assert_select_all(&list, (0, 0), (2, 2));

        let mut list = PageList::init(10, 10, None).unwrap();
        set_screen_text_lines(
            &mut list,
            &[
                "ABC  DEF",
                " 123",
                "456",
                "FOO",
                " BAR",
                " BAZ",
                " QWERTY",
                " 12345678",
            ],
        );
        assert_select_all(&list, (0, 0), (8, 7));
    }

    #[test]
    fn select_all_returns_none_for_empty_or_whitespace_only() {
        let empty = PageList::init(10, 3, None).unwrap();
        assert!(empty.select_all().is_none());

        let mut whitespace = PageList::init(10, 3, None).unwrap();
        set_screen_text_lines(&mut whitespace, &["   ", "\t\t", " \t "]);
        assert!(whitespace.select_all().is_none());
    }

    #[test]
    fn select_all_trims_edges_but_preserves_internal_span() {
        let mut list = PageList::init(10, 3, None).unwrap();
        set_screen_text_lines(&mut list, &["  \tA B\t  "]);

        assert_select_all(&list, (3, 0), (5, 0));
    }

    #[test]
    fn select_all_uses_screen_domain_across_scrollback() {
        let mut list = PageList::init(3, 3, None).unwrap();
        list.grow_rows(2).unwrap();
        set_screen_text_lines(&mut list, &["1  ", "2B ", "3  ", "4D ", "5E "]);

        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));
        assert_select_all(&list, (0, 0), (1, 4));
    }

    fn select_output_upstream_fixture() -> PageList {
        let mut list = PageList::init(10, 15, None).unwrap();

        set_screen_semantic_text(&mut list, 0, 0, "output1", SemanticContent::Output);
        set_screen_semantic_text(&mut list, 0, 1, "output1", SemanticContent::Output);

        set_screen_semantic_prompt(&mut list, 2, SemanticPrompt::Prompt);
        set_screen_semantic_text(&mut list, 0, 2, "prompt2", SemanticContent::Prompt);
        set_screen_semantic_text(&mut list, 0, 3, "input2", SemanticContent::Input);

        set_screen_semantic_text(&mut list, 0, 4, "output2out", SemanticContent::Output);
        set_screen_semantic_text(&mut list, 0, 5, "put2outpu", SemanticContent::Output);
        set_screen_semantic_text(&mut list, 0, 6, "t2output2", SemanticContent::Output);
        set_screen_semantic_text(&mut list, 0, 7, "output2", SemanticContent::Output);

        set_screen_semantic_prompt(&mut list, 8, SemanticPrompt::Prompt);
        set_screen_semantic_text(&mut list, 0, 8, "$ ", SemanticContent::Prompt);
        set_screen_semantic_text(&mut list, 2, 8, "input3", SemanticContent::Input);

        set_screen_semantic_text(&mut list, 0, 9, "output3", SemanticContent::Output);
        set_screen_semantic_text(&mut list, 0, 10, "output3", SemanticContent::Output);
        set_screen_semantic_text(&mut list, 0, 11, "output3", SemanticContent::Output);

        list
    }

    #[test]
    fn select_output_matches_upstream_blocks() {
        let list = select_output_upstream_fixture();

        assert_select_output(&list, (1, 1), (0, 0), (6, 1));
        assert_select_output(&list, (3, 7), (0, 4), (6, 7));
        assert_select_output(&list, (2, 10), (0, 9), (6, 11));
    }

    #[test]
    fn select_output_rejects_prompt_input_invalid_and_garbage() {
        let list = select_output_upstream_fixture();
        assert!(list.select_output(screen_pin(&list, 1, 8)).is_none());
        assert!(list.select_output(screen_pin(&list, 5, 8)).is_none());

        let other = PageList::init(10, 15, None).unwrap();
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let mut garbage = screen_pin(&list, 1, 1);
        garbage.garbage = true;

        assert!(list.select_output(invalid).is_none());
        assert!(list.select_output(garbage).is_none());
    }

    #[test]
    fn select_output_without_prompt_boundary_returns_none() {
        let mut list = PageList::init(10, 3, None).unwrap();
        set_screen_semantic_text(&mut list, 0, 0, "output", SemanticContent::Output);

        assert!(list.select_output(screen_pin(&list, 1, 0)).is_none());
    }

    #[test]
    fn select_output_uses_screen_domain_across_scrollback() {
        let mut list = PageList::init(4, 3, None).unwrap();
        list.grow_rows(2).unwrap();
        set_screen_semantic_prompt(&mut list, 0, SemanticPrompt::Prompt);
        set_screen_semantic_text(&mut list, 0, 0, "$ ", SemanticContent::Prompt);
        set_screen_semantic_text(&mut list, 0, 1, "old", SemanticContent::Output);
        set_screen_semantic_prompt(&mut list, 3, SemanticPrompt::Prompt);
        set_screen_semantic_text(&mut list, 0, 3, "$ ", SemanticContent::Prompt);

        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));
        assert_select_output(&list, (1, 1), (0, 1), (2, 1));
    }

    #[test]
    fn line_iterator_matches_upstream_basic_case() {
        let mut list = PageList::init(5, 5, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH"]);

        assert_line_iterator(
            &list,
            screen_pin(&list, 0, 0),
            &[
                (0, 0, 4, 0),
                (0, 1, 4, 1),
                (0, 2, 4, 2),
                (0, 3, 4, 3),
                (0, 4, 4, 4),
            ],
        );
    }

    #[test]
    fn line_iterator_matches_upstream_soft_wrap_case() {
        let mut list = PageList::init(5, 5, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH", "3ABCD"]);
        set_screen_row_wrap(&mut list, 0, true);

        assert_line_iterator(
            &list,
            screen_pin(&list, 0, 0),
            &[(0, 0, 4, 1), (0, 2, 4, 2), (0, 3, 4, 3), (0, 4, 4, 4)],
        );
    }

    #[test]
    fn line_iterator_non_wrapped_second_row_starts_there() {
        let mut list = PageList::init(5, 5, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH"]);

        assert_line_iterator(
            &list,
            screen_pin(&list, 0, 1),
            &[(0, 1, 4, 1), (0, 2, 4, 2), (0, 3, 4, 3), (0, 4, 4, 4)],
        );
    }

    #[test]
    fn line_iterator_continuation_row_returns_full_soft_wrap() {
        let mut list = PageList::init(5, 5, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH", "3ABCD"]);
        set_screen_row_wrap(&mut list, 0, true);

        assert_line_iterator(
            &list,
            screen_pin(&list, 0, 1),
            &[(0, 0, 4, 1), (0, 2, 4, 2), (0, 3, 4, 3), (0, 4, 4, 4)],
        );
    }

    #[test]
    fn line_iterator_rejects_invalid_or_garbage_start() {
        let list = PageList::init(5, 5, None).unwrap();
        let other = PageList::init(5, 5, None).unwrap();
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let mut garbage = screen_pin(&list, 0, 0);
        garbage.garbage = true;

        assert!(list.line_iterator(invalid).next().is_none());
        assert!(list.line_iterator(garbage).next().is_none());
    }

    #[test]
    fn line_iterator_uses_supplied_screen_start_in_scrollback() {
        let mut list = PageList::init(3, 3, None).unwrap();
        list.grow_rows(2).unwrap();
        set_screen_text_lines(&mut list, &["1AB", "2CD", "3EF", "4GH", "5IJ"]);

        assert_eq!(active_top_left_screen_coord(&list), Coordinate::new(0, 2));
        assert_line_iterator(
            &list,
            screen_pin(&list, 0, 0),
            &[
                (0, 0, 2, 0),
                (0, 1, 2, 1),
                (0, 2, 2, 2),
                (0, 3, 2, 3),
                (0, 4, 2, 4),
            ],
        );
    }

    #[test]
    fn selection_string_basic() {
        let mut list = PageList::init(5, 3, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH", "3IJKL"]);

        assert_selection_string(&list, (0, 1), (2, 2), false, true, "2EFGH\n3IJ");
    }

    #[test]
    fn selection_string_start_outside_written_area() {
        let mut list = PageList::init(5, 10, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH", "3IJKL"]);

        assert_selection_string(&list, (0, 5), (2, 6), false, true, "");
    }

    #[test]
    fn selection_string_end_outside_written_area() {
        let mut list = PageList::init(5, 10, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH", "3IJKL"]);

        assert_selection_string(&list, (0, 2), (2, 6), false, true, "3IJKL");
    }

    #[test]
    fn selection_string_trim_space() {
        let mut list = PageList::init(5, 3, None).unwrap();
        set_screen_text_lines(&mut list, &["1AB  ", "2EFGH", "3IJKL"]);

        assert_selection_string(&list, (0, 0), (2, 1), false, true, "1AB\n2EF");
        assert_selection_string(&list, (0, 0), (2, 1), false, false, "1AB  \n2EF");
    }

    #[test]
    fn selection_string_trim_empty_line() {
        let mut list = PageList::init(5, 5, None).unwrap();
        set_screen_text_lines(&mut list, &["1AB  ", "", "2EFGH", "3IJKL"]);

        assert_selection_string(&list, (0, 0), (2, 2), false, true, "1AB\n\n2EF");
        assert_selection_string(&list, (0, 0), (2, 2), false, false, "1AB  \n\n2EF");
    }

    #[test]
    fn selection_string_soft_wrap() {
        let mut list = PageList::init(5, 3, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH", "3IJKL"]);
        set_screen_row_wrap(&mut list, 0, true);
        set_screen_row_wrap(&mut list, 1, true);
        set_screen_row_wrap_continuation(&mut list, 1, true);
        set_screen_row_wrap_continuation(&mut list, 2, true);

        assert_selection_string(&list, (0, 1), (2, 2), false, true, "2EFGH3IJ");
    }

    #[test]
    fn selection_string_wide_char() {
        let mut list = PageList::init(5, 3, None).unwrap();
        set_screen_cell(&mut list, 0, 0, '1');
        set_screen_cell(&mut list, 1, 0, 'A');
        let mut wide = Cell::init('⚡' as u32);
        wide.set_wide(Wide::Wide);
        set_screen_cell_raw(&mut list, 2, 0, wide);
        let mut tail = Cell::init(0);
        tail.set_wide(Wide::SpacerTail);
        set_screen_cell_raw(&mut list, 3, 0, tail);

        assert_selection_string(&list, (0, 0), (3, 0), false, true, "1A⚡");
        assert_selection_string(&list, (0, 0), (2, 0), false, true, "1A⚡");
        assert_selection_string(&list, (3, 0), (3, 0), false, true, "⚡");
    }

    #[test]
    fn selection_string_wide_char_with_header() {
        let mut list = PageList::init(5, 3, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABC"]);
        let mut head = Cell::init(0);
        head.set_wide(Wide::SpacerHead);
        set_screen_cell_raw(&mut list, 4, 0, head);
        let mut wide = Cell::init('⚡' as u32);
        wide.set_wide(Wide::Wide);
        set_screen_cell_raw(&mut list, 0, 1, wide);
        let mut tail = Cell::init(0);
        tail.set_wide(Wide::SpacerTail);
        set_screen_cell_raw(&mut list, 1, 1, tail);
        set_screen_row_wrap(&mut list, 0, true);
        set_screen_row_wrap_continuation(&mut list, 1, true);

        assert_selection_string(&list, (0, 0), (4, 0), false, true, "1ABC⚡");
    }

    #[test]
    fn selection_string_empty_with_soft_wrap() {
        let mut list = PageList::init(5, 2, None).unwrap();
        let mut wide = Cell::init('👨' as u32);
        wide.set_wide(Wide::Wide);
        set_screen_cell_raw(&mut list, 0, 0, wide);
        let mut tail = Cell::init(0);
        tail.set_wide(Wide::SpacerTail);
        set_screen_cell_raw(&mut list, 1, 0, tail);
        set_screen_cell(&mut list, 2, 0, ' ');
        set_screen_cell(&mut list, 3, 0, ' ');
        set_screen_cell(&mut list, 4, 0, ' ');
        set_screen_row_wrap(&mut list, 0, true);
        set_screen_row_wrap_continuation(&mut list, 1, true);

        assert_selection_string(&list, (1, 0), (2, 0), false, true, "👨");
    }

    #[test]
    fn selection_string_with_zero_width_joiner() {
        let mut list = PageList::init(10, 1, None).unwrap();
        let mut wide = Cell::init('👨' as u32);
        wide.set_wide(Wide::Wide);
        set_screen_cell_raw(&mut list, 0, 0, wide);
        let mut tail = Cell::init(0);
        tail.set_wide(Wide::SpacerTail);
        set_screen_cell_raw(&mut list, 1, 0, tail);
        append_screen_grapheme(&mut list, 0, 0, 0x200d);

        assert_selection_string(&list, (0, 0), (1, 0), false, true, "👨‍");
    }

    #[test]
    fn selection_string_rectangle_basic() {
        let mut list = PageList::init(30, 5, None).unwrap();
        set_screen_text_lines(
            &mut list,
            &[
                "Lorem ipsum dolor",
                "sit amet, consectetur",
                "adipiscing elit, sed do",
                "eiusmod tempor incididunt",
                "ut labore et dolore",
            ],
        );

        assert_selection_string(&list, (2, 1), (6, 3), true, true, "t ame\nipisc\nusmod");
    }

    #[test]
    fn selection_string_rectangle_with_eol() {
        let mut list = PageList::init(30, 5, None).unwrap();
        set_screen_text_lines(
            &mut list,
            &[
                "Lorem ipsum dolor",
                "sit amet, consectetur",
                "adipiscing elit, sed do",
                "eiusmod tempor incididunt",
                "ut labore et dolore",
            ],
        );

        assert_selection_string(
            &list,
            (12, 0),
            (26, 4),
            true,
            true,
            "dolor\nnsectetur\nlit, sed do\nor incididunt\n dolore",
        );
    }

    #[test]
    fn selection_string_rectangle_complex_with_breaks() {
        let mut list = PageList::init(30, 8, None).unwrap();
        set_screen_text_lines(
            &mut list,
            &[
                "Lorem ipsum dolor",
                "sit amet, consectetur",
                "adipiscing elit, sed do",
                "eiusmod tempor incididunt",
                "ut labore et dolore",
                "",
                "magna aliqua. Ut enim",
                "ad minim veniam, quis",
            ],
        );

        assert_selection_string(
            &list,
            (11, 2),
            (26, 7),
            true,
            true,
            "elit, sed do\npor incididunt\nt dolore\n\na. Ut enim\nniam, quis",
        );
    }

    #[test]
    fn selection_string_multi_page() {
        let (mut list, page_rows) = multi_page_list(100);
        let start_y = page_rows as u32 - 1;
        set_screen_text_lines_at(&mut list, start_y, &["123456789", "!@#$%^&*(", "123456789"]);

        assert_selection_string(
            &list,
            (0, start_y),
            (2, start_y + 2),
            false,
            true,
            "123456789\n!@#$%^&*(\n123",
        );
    }

    #[test]
    fn selection_string_carries_trailing_state_across_page_chunks() {
        let (mut list, page_rows) = multi_page_list(100);
        let first_y = page_rows as u32 - 1;
        set_screen_text_lines_at(&mut list, first_y, &["A  ", "B"]);
        set_screen_row_wrap(&mut list, first_y, true);
        set_screen_row_wrap_continuation(&mut list, first_y + 1, true);

        let mut expected = String::from("A");
        expected.extend(std::iter::repeat_n(' ', list.cols as usize - 1));
        expected.push('B');
        assert_selection_string(
            &list,
            (0, first_y),
            (0, first_y + 1),
            false,
            true,
            &expected,
        );
    }

    #[test]
    fn selection_string_none_formats_full_screen_domain() {
        let mut list = PageList::init(5, 3, None).unwrap();
        set_screen_text_lines(&mut list, &["1AB  ", "2EFGH"]);

        assert_eq!(
            list.selection_string(SelectionStringOptions {
                selection: None,
                trim: true,
            }),
            "1AB\n2EFGH"
        );
    }

    #[test]
    fn selection_string_invalid_or_garbage_endpoints_are_empty() {
        let mut list = PageList::init(5, 3, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD"]);
        let other = PageList::init(5, 3, None).unwrap();
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let valid = screen_pin(&list, 0, 0);
        let mut garbage = valid;
        garbage.garbage = true;

        for selection in [
            selection::Selection::new(invalid, valid, false),
            selection::Selection::new(valid, invalid, false),
            selection::Selection::new(garbage, valid, false),
            selection::Selection::new(valid, garbage, false),
        ] {
            assert_eq!(
                list.selection_string(SelectionStringOptions {
                    selection: Some(selection),
                    trim: true,
                }),
                ""
            );
        }
    }

    #[test]
    fn selection_string_uses_current_tracked_pins() {
        let mut list = PageList::init(5, 3, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH"]);
        let tracked = list
            .track_selection(screen_selection(&list, (0, 0), (1, 0), false))
            .unwrap();
        let (start, end) = tracked.tracked_pins().unwrap();

        unsafe {
            *start.as_ptr() = screen_pin(&list, 0, 1);
            *end.as_ptr() = screen_pin(&list, 2, 1);
        }

        assert_eq!(
            list.selection_string(SelectionStringOptions {
                selection: Some(tracked),
                trim: true,
            }),
            "2EF"
        );

        list.untrack_selection(tracked);
    }

    #[test]
    fn selection_string_uses_screen_domain_across_scrollback() {
        let mut list = PageList::init(3, 3, None).unwrap();
        list.grow_rows(2).unwrap();
        set_screen_text_lines(&mut list, &["1AB", "2CD", "3EF", "4GH", "5IJ"]);

        assert_selection_string(&list, (0, 0), (1, 1), false, true, "1AB\n2C");
    }

    #[test]
    fn selection_string_start_on_spacer_head_skips_row() {
        let mut list = PageList::init(5, 2, None).unwrap();
        set_screen_cell(&mut list, 0, 0, 'A');
        let mut head = Cell::init(0);
        head.set_wide(Wide::SpacerHead);
        set_screen_cell_raw(&mut list, 1, 0, head);
        set_screen_text_lines_at(&mut list, 1, &["BC"]);

        assert_selection_string(&list, (1, 0), (1, 1), false, true, "BC");
    }

    #[test]
    fn line_iterator_selection_strings_match_upstream_basic() {
        let mut list = PageList::init(5, 5, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH"]);
        let mut iter = list.line_iterator(screen_pin(&list, 0, 0));

        assert_eq!(
            list.selection_string(SelectionStringOptions {
                selection: iter.next(),
                trim: false,
            }),
            "1ABCD"
        );
        assert_eq!(
            list.selection_string(SelectionStringOptions {
                selection: iter.next(),
                trim: false,
            }),
            "2EFGH"
        );
    }

    #[test]
    fn line_iterator_selection_strings_match_upstream_soft_wrap() {
        let mut list = PageList::init(5, 5, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH", "3ABCD"]);
        set_screen_row_wrap(&mut list, 0, true);
        set_screen_row_wrap_continuation(&mut list, 1, true);
        let mut iter = list.line_iterator(screen_pin(&list, 0, 0));

        assert_eq!(
            list.selection_string(SelectionStringOptions {
                selection: iter.next(),
                trim: false,
            }),
            "1ABCD2EFGH"
        );
        assert_eq!(
            list.selection_string(SelectionStringOptions {
                selection: iter.next(),
                trim: false,
            }),
            "3ABCD"
        );
    }

    #[test]
    fn dump_string_basic_single_and_multi_row() {
        let mut list = PageList::init(5, 4, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH", "3IJKL"]);

        assert_dump_string(&list, (0, 0), Some((4, 0)), true, "1ABCD");
        assert_dump_string(&list, (0, 0), Some((4, 2)), true, "1ABCD\n2EFGH\n3IJKL");
    }

    #[test]
    fn dump_string_ignores_endpoint_x_values() {
        let mut list = PageList::init(5, 3, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH"]);

        assert_dump_string(&list, (3, 0), Some((1, 1)), true, "1ABCD\n2EFGH");
    }

    #[test]
    fn dump_string_defaults_to_screen_bottom_right() {
        let mut list = PageList::init(5, 4, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH"]);

        assert_dump_string(&list, (0, 0), None, true, "1ABCD\n2EFGH");
    }

    #[test]
    fn dump_string_unwraps_soft_wraps() {
        let mut list = PageList::init(5, 3, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH", "3IJKL"]);
        set_screen_row_wrap(&mut list, 0, true);
        set_screen_row_wrap(&mut list, 1, true);
        set_screen_row_wrap_continuation(&mut list, 1, true);
        set_screen_row_wrap_continuation(&mut list, 2, true);

        assert_dump_string(&list, (0, 0), Some((4, 2)), true, "1ABCD2EFGH3IJKL");
    }

    #[test]
    fn dump_string_can_preserve_visual_rows() {
        let mut list = PageList::init(5, 3, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH", "3IJKL"]);
        set_screen_row_wrap(&mut list, 0, true);
        set_screen_row_wrap(&mut list, 1, true);
        set_screen_row_wrap_continuation(&mut list, 1, true);
        set_screen_row_wrap_continuation(&mut list, 2, true);

        assert_dump_string(&list, (0, 0), Some((4, 2)), false, "1ABCD\n2EFGH\n3IJKL");
    }

    #[test]
    fn dump_string_does_not_trim_explicit_spaces() {
        let mut list = PageList::init(5, 3, None).unwrap();
        set_screen_text_lines(&mut list, &["A  ", "B"]);

        assert_dump_string(&list, (0, 0), Some((4, 1)), true, "A  \nB");
    }

    #[test]
    fn dump_string_handles_wide_characters_and_spacer_heads() {
        let mut list = PageList::init(5, 3, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABC"]);
        let mut head = Cell::init(0);
        head.set_wide(Wide::SpacerHead);
        set_screen_cell_raw(&mut list, 4, 0, head);
        let mut wide = Cell::init('⚡' as u32);
        wide.set_wide(Wide::Wide);
        set_screen_cell_raw(&mut list, 0, 1, wide);
        let mut tail = Cell::init(0);
        tail.set_wide(Wide::SpacerTail);
        set_screen_cell_raw(&mut list, 1, 1, tail);
        set_screen_row_wrap(&mut list, 0, true);
        set_screen_row_wrap_continuation(&mut list, 1, true);

        assert_dump_string(&list, (0, 0), Some((4, 0)), true, "1ABC⚡");
        assert_dump_string(&list, (0, 0), Some((4, 0)), false, "1ABC");
        assert_dump_string(&list, (0, 0), Some((4, 1)), false, "1ABC\n⚡");
    }

    #[test]
    fn dump_string_invalid_or_garbage_pins_are_empty() {
        let mut list = PageList::init(5, 3, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD"]);
        let other = PageList::init(5, 3, None).unwrap();
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let valid = screen_pin(&list, 0, 0);
        let mut garbage = valid;
        garbage.garbage = true;

        assert_eq!(list.dump_string(invalid, Some(valid), true), "");
        assert_eq!(list.dump_string(valid, Some(invalid), true), "");
        assert_eq!(list.dump_string(garbage, Some(valid), true), "");
        assert_eq!(list.dump_string(valid, Some(garbage), true), "");
    }

    #[test]
    fn dump_string_uses_current_tracked_pin_locations() {
        let mut list = PageList::init(5, 3, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH"]);
        let tracked = list
            .track_selection(screen_selection(&list, (0, 0), (1, 0), false))
            .unwrap();
        let (start, end) = tracked.tracked_pins().unwrap();

        unsafe {
            *start.as_ptr() = screen_pin(&list, 3, 1);
            *end.as_ptr() = screen_pin(&list, 1, 1);
        }

        assert_eq!(
            list.dump_string(tracked_pin_value(start), Some(tracked_pin_value(end)), true),
            "2EFGH"
        );

        list.untrack_selection(tracked);
    }

    #[test]
    fn dump_string_uses_screen_domain_across_scrollback() {
        let mut list = PageList::init(3, 3, None).unwrap();
        list.grow_rows(2).unwrap();
        set_screen_text_lines(&mut list, &["1AB", "2CD", "3EF", "4GH", "5IJ"]);

        assert_dump_string(&list, (0, 0), Some((2, 1)), true, "1AB\n2CD");
    }

    #[test]
    fn dump_string_carries_unwrapped_state_across_page_chunks() {
        let (mut list, page_rows) = multi_page_list(100);
        let first_y = page_rows as u32 - 1;
        set_screen_text_lines_at(&mut list, first_y, &["A  ", "B"]);
        set_screen_row_wrap(&mut list, first_y, true);
        set_screen_row_wrap_continuation(&mut list, first_y + 1, true);

        let mut expected = String::from("A");
        expected.extend(std::iter::repeat_n(' ', list.cols as usize - 1));
        expected.push('B');
        assert_dump_string(&list, (0, first_y), Some((0, first_y + 1)), true, &expected);
    }

    #[test]
    fn dump_string_carries_visual_blank_rows_across_page_chunks() {
        let (mut list, page_rows) = multi_page_list(100);
        let first_y = page_rows as u32 - 1;
        set_screen_text_lines_at(&mut list, first_y, &["A ", "", "B"]);
        set_screen_row_wrap(&mut list, first_y, true);
        set_screen_row_wrap_continuation(&mut list, first_y + 1, true);

        assert_dump_string(
            &list,
            (0, first_y),
            Some((0, first_y + 2)),
            false,
            "A \n\nB",
        );
    }

    #[test]
    fn dump_string_emits_leading_but_not_trailing_blank_rows() {
        let mut list = PageList::init(5, 5, None).unwrap();
        set_screen_text_lines_at(&mut list, 2, &["AB"]);

        assert_dump_string(&list, (0, 0), None, true, "\n\nAB");
    }

    #[test]
    fn codepoint_map_single_replacement() {
        let mut list = PageList::init(12, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello world"]);
        let map = [codepoint_map_entry(
            'o',
            'o',
            CodepointReplacement::Codepoint('x'),
        )];

        assert_page_string_with_map(
            &list,
            (0, 0),
            (10, 0),
            PageOutputFormat::Plain,
            true,
            &map,
            "hellx wxrld",
        );
    }

    #[test]
    fn codepoint_map_conflicting_replacement_prefers_last() {
        let mut list = PageList::init(8, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello"]);
        let map = [
            codepoint_map_entry('o', 'o', CodepointReplacement::Codepoint('x')),
            codepoint_map_entry('o', 'o', CodepointReplacement::Codepoint('y')),
        ];

        assert_page_string_with_map(
            &list,
            (0, 0),
            (4, 0),
            PageOutputFormat::Plain,
            true,
            &map,
            "helly",
        );
    }

    #[test]
    fn codepoint_map_replacement_with_string() {
        let mut list = PageList::init(8, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello"]);
        let map = [codepoint_map_entry(
            'o',
            'o',
            CodepointReplacement::String("XYZ".to_string()),
        )];

        assert_page_string_with_map(
            &list,
            (0, 0),
            (4, 0),
            PageOutputFormat::Plain,
            true,
            &map,
            "hellXYZ",
        );
    }

    #[test]
    fn codepoint_map_range_replacement() {
        let mut list = PageList::init(8, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["abcdefg"]);
        let map = [codepoint_map_entry(
            'b',
            'e',
            CodepointReplacement::Codepoint('X'),
        )];

        assert_page_string_with_map(
            &list,
            (0, 0),
            (6, 0),
            PageOutputFormat::Plain,
            true,
            &map,
            "aXXXXfg",
        );
    }

    #[test]
    fn codepoint_map_multiple_ranges() {
        let mut list = PageList::init(12, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello world"]);
        let map = [
            codepoint_map_entry('a', 'm', CodepointReplacement::Codepoint('A')),
            codepoint_map_entry('n', 'z', CodepointReplacement::Codepoint('Z')),
        ];

        assert_page_string_with_map(
            &list,
            (0, 0),
            (10, 0),
            PageOutputFormat::Plain,
            true,
            &map,
            "AAAAZ ZZZAA",
        );
    }

    #[test]
    fn codepoint_map_unicode_string_replacement() {
        let mut list = PageList::init(16, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello ⚡ world"]);
        let map = [codepoint_map_entry(
            '⚡',
            '⚡',
            CodepointReplacement::String("🔥".to_string()),
        )];

        assert_page_string_with_map(
            &list,
            (0, 0),
            (13, 0),
            PageOutputFormat::Plain,
            true,
            &map,
            "hello 🔥 world",
        );
    }

    #[test]
    fn codepoint_map_empty_map_preserves_output() {
        let mut list = PageList::init(12, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello world"]);
        let map = [];

        assert_page_string_with_map(
            &list,
            (0, 0),
            (10, 0),
            PageOutputFormat::Plain,
            true,
            &map,
            "hello world",
        );
    }

    #[test]
    fn codepoint_map_replaces_grapheme_codepoints() {
        let mut list = PageList::init(4, 2, None).unwrap();
        set_screen_cell(&mut list, 0, 0, 'e');
        append_screen_grapheme(&mut list, 0, 0, 0x0301);
        let map = [CodepointMapEntry::new(
            0x0301,
            0x0301,
            CodepointReplacement::String("*".to_string()),
        )
        .expect("combining acute accent must be valid")];

        assert_page_string_with_map(
            &list,
            (0, 0),
            (0, 0),
            PageOutputFormat::Plain,
            true,
            &map,
            "e*",
        );
    }

    #[test]
    fn codepoint_map_replacements_are_not_recursive() {
        let mut list = PageList::init(4, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["a"]);
        let map = [
            codepoint_map_entry('a', 'a', CodepointReplacement::String("b".to_string())),
            codepoint_map_entry('b', 'b', CodepointReplacement::String("c".to_string())),
        ];

        assert_page_string_with_map(
            &list,
            (0, 0),
            (0, 0),
            PageOutputFormat::Plain,
            true,
            &map,
            "b",
        );
    }

    #[test]
    fn codepoint_map_vt_output() {
        let mut list = PageList::init(10, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["red text"]);
        let map = [codepoint_map_entry(
            'e',
            'e',
            CodepointReplacement::Codepoint('X'),
        )];

        assert_page_string_with_map(
            &list,
            (0, 0),
            (7, 0),
            PageOutputFormat::Vt,
            true,
            &map,
            "rXd tXxt",
        );
    }

    #[test]
    fn codepoint_map_styled_output_keeps_replacement_inside_style() {
        let mut list = PageList::init(4, 2, None).unwrap();
        let styled = style::Style {
            fg_color: style::Color::Palette(1),
            ..style::Style::default()
        };
        set_screen_styled_cell(&mut list, 0, 0, 'e', styled);
        let map = [codepoint_map_entry(
            'e',
            'e',
            CodepointReplacement::Codepoint('X'),
        )];

        assert_page_string_with_map(
            &list,
            (0, 0),
            (0, 0),
            PageOutputFormat::Vt,
            true,
            &map,
            "\x1b[0m\x1b[38;5;1mX\x1b[0m",
        );
        assert_page_string_with_map(
            &list,
            (0, 0),
            (0, 0),
            PageOutputFormat::Html,
            true,
            &map,
            "<div style=\"font-family: monospace; white-space: pre;\"><div style=\"display: inline;color: var(--vt-palette-1);\">X</div></div>",
        );
    }

    #[test]
    fn codepoint_map_html_output_replaces_before_escaping() {
        let mut list = PageList::init(4, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["xy"]);
        let map = [
            codepoint_map_entry('x', 'x', CodepointReplacement::Codepoint('<')),
            codepoint_map_entry('y', 'y', CodepointReplacement::String("é".to_string())),
        ];

        assert_page_string_with_map(
            &list,
            (0, 0),
            (1, 0),
            PageOutputFormat::Html,
            true,
            &map,
            "<div style=\"font-family: monospace; white-space: pre;\">&lt;&#233;</div>",
        );
    }

    #[test]
    fn codepoint_map_does_not_replace_generated_alignment_blanks() {
        let mut list = PageList::init(4, 2, None).unwrap();
        set_screen_cell(&mut list, 0, 0, 'A');
        set_screen_cell(&mut list, 2, 0, 'B');
        let map = [codepoint_map_entry(
            ' ',
            ' ',
            CodepointReplacement::Codepoint('_'),
        )];

        assert_page_string_with_map(
            &list,
            (0, 0),
            (2, 0),
            PageOutputFormat::Plain,
            false,
            &map,
            "A B",
        );

        set_screen_cell(&mut list, 1, 0, ' ');
        assert_page_string_with_map(
            &list,
            (0, 0),
            (2, 0),
            PageOutputFormat::Plain,
            false,
            &map,
            "A_B",
        );
    }

    #[test]
    fn codepoint_map_rejects_invalid_ranges() {
        assert!(CodepointMapEntry::new(
            'z' as u32,
            'a' as u32,
            CodepointReplacement::Codepoint('x')
        )
        .is_none());
        assert!(
            CodepointMapEntry::new(0xd800, 0xd800, CodepointReplacement::Codepoint('x')).is_none()
        );
        assert!(
            CodepointMapEntry::new(0xd7ff, 0xe000, CodepointReplacement::Codepoint('x')).is_none()
        );
        assert!(
            CodepointMapEntry::new(0x11_0000, 0x11_0000, CodepointReplacement::Codepoint('x'))
                .is_none()
        );
    }

    #[test]
    fn codepoint_map_no_map_formatters_are_unchanged() {
        let mut list = PageList::init(6, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["A B"]);

        assert_selection_string(&list, (0, 0), (2, 0), false, false, "A B");
        assert_dump_string(&list, (0, 0), Some((2, 0)), true, "A B");
        assert_page_string(&list, (0, 0), (2, 0), PageOutputFormat::Plain, None, "A B");
    }

    #[test]
    fn point_map_plain_single_line_ascii() {
        let mut list = PageList::init(8, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello"]);

        assert_plain_point_map(
            &list,
            (0, 0),
            (4, 0),
            true,
            true,
            None,
            "hello",
            &coords(&[(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)]),
        );
    }

    #[test]
    fn point_map_plain_unicode_and_grapheme_bytes() {
        let mut list = PageList::init(4, 2, None).unwrap();
        set_screen_cell(&mut list, 0, 0, 'é');
        set_screen_cell(&mut list, 1, 0, 'e');
        append_screen_grapheme(&mut list, 1, 0, 0x0301);

        assert_plain_point_map(
            &list,
            (0, 0),
            (1, 0),
            true,
            true,
            None,
            "ée\u{301}",
            &coords(&[(0, 0), (0, 0), (1, 0), (1, 0), (1, 0)]),
        );
    }

    #[test]
    fn point_map_plain_wide_character_and_spacer_tail_start() {
        let mut list = PageList::init(6, 2, None).unwrap();
        set_screen_cell(&mut list, 0, 0, '1');
        set_screen_cell(&mut list, 1, 0, 'A');
        let mut wide = Cell::init('⚡' as u32);
        wide.set_wide(Wide::Wide);
        set_screen_cell_raw(&mut list, 2, 0, wide);
        let mut tail = Cell::init(0);
        tail.set_wide(Wide::SpacerTail);
        set_screen_cell_raw(&mut list, 3, 0, tail);

        let expected = coords(&[(2, 0), (2, 0), (2, 0)]);
        assert_plain_point_map(&list, (2, 0), (3, 0), true, true, None, "⚡", &expected);
        assert_plain_point_map(&list, (3, 0), (3, 0), true, true, None, "⚡", &expected);
    }

    #[test]
    fn point_map_plain_multiline_and_rectangle_newlines() {
        let mut list = PageList::init(8, 3, None).unwrap();
        set_screen_text_lines(&mut list, &["hello", "world"]);

        assert_plain_point_map(
            &list,
            (0, 0),
            (4, 1),
            true,
            true,
            None,
            "hello\nworld",
            &coords(&[
                (0, 0),
                (1, 0),
                (2, 0),
                (3, 0),
                (4, 0),
                (4, 0),
                (0, 1),
                (1, 1),
                (2, 1),
                (3, 1),
                (4, 1),
            ]),
        );

        let actual = list.plain_string_with_point_map(PlainStringWithMapOptions {
            selection: Some(screen_selection(&list, (1, 0), (3, 1), true)),
            trim: true,
            unwrap: true,
            codepoint_map: None,
        });
        assert_eq!(actual.text, "ell\norl");
        assert_eq!(
            actual.point_map,
            coords(&[(1, 0), (2, 0), (3, 0), (3, 0), (1, 1), (2, 1), (3, 1)])
        );
    }

    #[test]
    fn point_map_plain_generated_blanks_use_upstream_reverse_order() {
        let mut list = PageList::init(5, 2, None).unwrap();
        set_screen_cell(&mut list, 0, 0, 'A');
        set_screen_cell(&mut list, 3, 0, 'C');

        assert_plain_point_map(
            &list,
            (0, 0),
            (3, 0),
            false,
            true,
            None,
            "A  C",
            &coords(&[(0, 0), (2, 0), (1, 0), (3, 0)]),
        );
    }

    #[test]
    fn point_map_plain_explicit_spaces_keep_source_order() {
        let mut list = PageList::init(5, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["A  C"]);

        assert_plain_point_map(
            &list,
            (0, 0),
            (3, 0),
            false,
            true,
            None,
            "A  C",
            &coords(&[(0, 0), (1, 0), (2, 0), (3, 0)]),
        );
    }

    #[test]
    fn point_map_plain_trimmed_spaces_emit_no_points() {
        let mut list = PageList::init(8, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello   "]);

        assert_plain_point_map(
            &list,
            (0, 0),
            (7, 0),
            true,
            true,
            None,
            "hello",
            &coords(&[(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)]),
        );
    }

    #[test]
    fn point_map_plain_leading_blank_rows() {
        let mut list = PageList::init(4, 4, None).unwrap();
        set_screen_cell(&mut list, 0, 2, 'X');

        assert_plain_point_map(
            &list,
            (0, 0),
            (0, 2),
            true,
            true,
            None,
            "\n\nX",
            &coords(&[(0, 0), (0, 1), (0, 2)]),
        );
    }

    #[test]
    fn point_map_plain_trailing_spaces_with_trim_false() {
        let mut list = PageList::init(8, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hi  "]);

        assert_plain_point_map(
            &list,
            (0, 0),
            (3, 0),
            false,
            true,
            None,
            "hi  ",
            &coords(&[(0, 0), (1, 0), (2, 0), (3, 0)]),
        );
    }

    #[test]
    fn point_map_plain_wrap_continuation_blank_cells() {
        let mut list = PageList::init(5, 2, None).unwrap();
        set_screen_cell(&mut list, 0, 0, 'A');
        set_screen_cell(&mut list, 2, 1, 'B');
        set_screen_row_wrap(&mut list, 0, true);
        set_screen_row_wrap_continuation(&mut list, 1, true);

        assert_plain_point_map(
            &list,
            (0, 0),
            (2, 1),
            false,
            true,
            None,
            "A      B",
            &coords(&[
                (0, 0),
                (1, 1),
                (0, 1),
                (4, 0),
                (3, 0),
                (2, 0),
                (1, 0),
                (2, 1),
            ]),
        );
    }

    #[test]
    fn point_map_plain_string_replacement_maps_original_cell() {
        let mut list = PageList::init(4, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["ao"]);
        let map = [codepoint_map_entry(
            'o',
            'o',
            CodepointReplacement::String("XYZ".to_string()),
        )];

        assert_plain_point_map(
            &list,
            (0, 0),
            (1, 0),
            true,
            true,
            Some(&map),
            "aXYZ",
            &coords(&[(0, 0), (1, 0), (1, 0), (1, 0)]),
        );
    }

    #[test]
    fn point_map_plain_single_codepoint_replacement_maps_original_cell() {
        let mut list = PageList::init(4, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["ao"]);
        let map = [codepoint_map_entry(
            'o',
            'o',
            CodepointReplacement::Codepoint('é'),
        )];

        assert_plain_point_map(
            &list,
            (0, 0),
            (1, 0),
            true,
            true,
            Some(&map),
            "aé",
            &coords(&[(0, 0), (1, 0), (1, 0)]),
        );
    }

    #[test]
    fn point_map_plain_codepoint_map_does_not_touch_generated_blanks() {
        let mut list = PageList::init(5, 2, None).unwrap();
        set_screen_cell(&mut list, 0, 0, 'A');
        set_screen_cell(&mut list, 2, 0, 'B');
        let map = [codepoint_map_entry(
            ' ',
            ' ',
            CodepointReplacement::Codepoint('_'),
        )];

        assert_plain_point_map(
            &list,
            (0, 0),
            (2, 0),
            false,
            true,
            Some(&map),
            "A B",
            &coords(&[(0, 0), (1, 0), (2, 0)]),
        );
    }

    #[test]
    fn point_map_plain_multi_page_pending_blank_rows_are_screen_domain() {
        let (mut list, page_rows) = multi_page_list(80);
        let first_y = page_rows as u32 - 1;
        set_screen_cell(&mut list, 0, first_y, 'A');
        set_screen_cell(&mut list, 0, first_y + 2, 'B');

        assert_plain_point_map(
            &list,
            (0, first_y),
            (0, first_y + 2),
            true,
            true,
            None,
            "A\n\nB",
            &coords(&[
                (0, first_y),
                (0, first_y),
                (0, first_y + 1),
                (0, first_y + 2),
            ]),
        );
    }

    #[test]
    fn point_map_plain_invalid_or_garbage_endpoints_are_empty() {
        let mut list = PageList::init(5, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello"]);
        let other = PageList::init(5, 2, None).unwrap();
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let valid = screen_pin(&list, 0, 0);
        let mut garbage = valid;
        garbage.garbage = true;

        for selection in [
            selection::Selection::new(invalid, valid, false),
            selection::Selection::new(valid, invalid, false),
            selection::Selection::new(garbage, valid, false),
            selection::Selection::new(valid, garbage, false),
        ] {
            assert_plain_point_map_for_selection(&list, Some(selection), true, true, "", &[]);
        }
    }

    #[test]
    fn point_map_plain_no_map_formatters_are_unchanged() {
        let mut list = PageList::init(6, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["A B"]);

        assert_selection_string(&list, (0, 0), (2, 0), false, false, "A B");
        assert_dump_string(&list, (0, 0), Some((2, 0)), true, "A B");
        assert_page_string(&list, (0, 0), (2, 0), PageOutputFormat::Plain, None, "A B");
    }

    #[test]
    fn pin_map_plain_single_line_ascii() {
        let mut list = PageList::init(8, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello"]);

        assert_plain_pin_map(
            &list,
            (0, 0),
            (4, 0),
            true,
            true,
            None,
            "hello",
            &pins(&list, &[(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)]),
        );
    }

    #[test]
    fn pin_map_plain_unicode_bytes_share_source_pin() {
        let mut list = PageList::init(4, 2, None).unwrap();
        set_screen_cell(&mut list, 0, 0, 'é');

        assert_plain_pin_map(
            &list,
            (0, 0),
            (0, 0),
            true,
            true,
            None,
            "é",
            &pins(&list, &[(0, 0), (0, 0)]),
        );
    }

    #[test]
    fn pin_map_plain_wide_character_spacer_tail_start() {
        let mut list = PageList::init(6, 2, None).unwrap();
        let mut wide = Cell::init('⚡' as u32);
        wide.set_wide(Wide::Wide);
        set_screen_cell_raw(&mut list, 2, 0, wide);
        let mut tail = Cell::init(0);
        tail.set_wide(Wide::SpacerTail);
        set_screen_cell_raw(&mut list, 3, 0, tail);

        assert_plain_pin_map(
            &list,
            (3, 0),
            (3, 0),
            true,
            true,
            None,
            "⚡",
            &pins(&list, &[(2, 0), (2, 0), (2, 0)]),
        );
    }

    #[test]
    fn pin_map_plain_generated_blanks_use_reverse_order() {
        let mut list = PageList::init(5, 2, None).unwrap();
        set_screen_cell(&mut list, 0, 0, 'A');
        set_screen_cell(&mut list, 3, 0, 'C');

        assert_plain_pin_map(
            &list,
            (0, 0),
            (3, 0),
            false,
            true,
            None,
            "A  C",
            &pins(&list, &[(0, 0), (2, 0), (1, 0), (3, 0)]),
        );
    }

    #[test]
    fn pin_map_plain_explicit_spaces_and_trimmed_spaces() {
        let mut list = PageList::init(8, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hi  "]);

        assert_plain_pin_map(
            &list,
            (0, 0),
            (3, 0),
            false,
            true,
            None,
            "hi  ",
            &pins(&list, &[(0, 0), (1, 0), (2, 0), (3, 0)]),
        );
        assert_plain_pin_map(
            &list,
            (0, 0),
            (3, 0),
            true,
            true,
            None,
            "hi",
            &pins(&list, &[(0, 0), (1, 0)]),
        );
    }

    #[test]
    fn pin_map_plain_multiline_and_leading_blank_rows() {
        let mut list = PageList::init(5, 4, None).unwrap();
        set_screen_cell(&mut list, 0, 1, 'A');
        set_screen_cell(&mut list, 0, 3, 'B');

        assert_plain_pin_map(
            &list,
            (0, 0),
            (0, 3),
            true,
            true,
            None,
            "\nA\n\nB",
            &pins(&list, &[(0, 0), (0, 1), (0, 1), (0, 2), (0, 3)]),
        );
    }

    #[test]
    fn pin_map_plain_wrap_continuation_trailing_state_cells() {
        let mut list = PageList::init(5, 2, None).unwrap();
        set_screen_cell(&mut list, 0, 0, 'A');
        set_screen_cell(&mut list, 2, 1, 'B');
        set_screen_row_wrap(&mut list, 0, true);
        set_screen_row_wrap_continuation(&mut list, 1, true);

        assert_plain_pin_map(
            &list,
            (0, 0),
            (2, 1),
            false,
            true,
            None,
            "A      B",
            &pins(
                &list,
                &[
                    (0, 0),
                    (1, 1),
                    (0, 1),
                    (4, 0),
                    (3, 0),
                    (2, 0),
                    (1, 0),
                    (2, 1),
                ],
            ),
        );
    }

    #[test]
    fn pin_map_plain_codepoint_replacements_map_original_pin() {
        let mut list = PageList::init(4, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["ao"]);
        let string_map = [codepoint_map_entry(
            'o',
            'o',
            CodepointReplacement::String("XYZ".to_string()),
        )];
        assert_plain_pin_map(
            &list,
            (0, 0),
            (1, 0),
            true,
            true,
            Some(&string_map),
            "aXYZ",
            &pins(&list, &[(0, 0), (1, 0), (1, 0), (1, 0)]),
        );

        let char_map = [codepoint_map_entry(
            'o',
            'o',
            CodepointReplacement::Codepoint('é'),
        )];
        assert_plain_pin_map(
            &list,
            (0, 0),
            (1, 0),
            true,
            true,
            Some(&char_map),
            "aé",
            &pins(&list, &[(0, 0), (1, 0), (1, 0)]),
        );
    }

    #[test]
    fn pin_map_plain_multi_page_preserves_source_nodes() {
        let (mut list, page_rows) = multi_page_list(80);
        let first_y = page_rows as u32 - 1;
        set_screen_cell(&mut list, 0, first_y, 'A');
        set_screen_cell(&mut list, 0, first_y + 1, 'B');

        let actual = list.plain_string_with_pin_map(PlainStringWithMapOptions {
            selection: Some(screen_selection(
                &list,
                (0, first_y),
                (0, first_y + 1),
                false,
            )),
            trim: true,
            unwrap: true,
            codepoint_map: None,
        });
        let expected = pins(&list, &[(0, first_y), (0, first_y), (0, first_y + 1)]);
        assert_eq!(actual.text, "A\nB");
        assert_eq!(actual.pin_map, expected);
        assert_ne!(actual.pin_map[0].node, actual.pin_map[2].node);
    }

    #[test]
    fn pin_map_plain_multi_page_wrap_continuation_trailing_cells() {
        let (mut list, page_rows) = multi_page_list(80);
        let first_y = page_rows as u32 - 1;
        let second_y = first_y + 1;
        set_screen_cell(&mut list, 0, first_y, 'A');
        set_screen_cell(&mut list, 2, second_y, 'B');
        set_screen_row_wrap(&mut list, first_y, true);
        set_screen_row_wrap_continuation(&mut list, second_y, true);

        let actual = list.plain_string_with_pin_map(PlainStringWithMapOptions {
            selection: Some(screen_selection(&list, (0, first_y), (2, second_y), false)),
            trim: false,
            unwrap: true,
            codepoint_map: None,
        });

        let mut expected_points = vec![(0, first_y), (1, second_y), (0, second_y)];
        for x in (1..list.cols).rev() {
            expected_points.push((x, first_y));
        }
        expected_points.push((2, second_y));
        let expected_text = format!("A{}B", " ".repeat(expected_points.len() - 2));

        assert_eq!(actual.text, expected_text);
        assert_eq!(actual.pin_map, pins(&list, &expected_points));
        assert_ne!(actual.pin_map[0].node, actual.pin_map[1].node);
        assert_ne!(
            actual.pin_map[0].node,
            actual.pin_map[expected_points.len() - 1].node
        );
    }

    #[test]
    fn pin_map_plain_invalid_or_garbage_endpoints_are_empty() {
        let mut list = PageList::init(5, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello"]);
        let other = PageList::init(5, 2, None).unwrap();
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let valid = screen_pin(&list, 0, 0);
        let mut garbage = valid;
        garbage.garbage = true;

        for selection in [
            selection::Selection::new(invalid, valid, false),
            selection::Selection::new(valid, invalid, false),
            selection::Selection::new(garbage, valid, false),
            selection::Selection::new(valid, garbage, false),
        ] {
            assert_plain_pin_map_for_selection(&list, Some(selection), true, true, "", &[]);
        }
    }

    #[test]
    fn pin_map_plain_no_map_and_point_map_formatters_are_unchanged() {
        let mut list = PageList::init(6, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["A B"]);

        assert_selection_string(&list, (0, 0), (2, 0), false, false, "A B");
        assert_dump_string(&list, (0, 0), Some((2, 0)), true, "A B");
        assert_page_string(&list, (0, 0), (2, 0), PageOutputFormat::Plain, None, "A B");
        assert_plain_point_map(
            &list,
            (0, 0),
            (2, 0),
            false,
            true,
            None,
            "A B",
            &coords(&[(0, 0), (1, 0), (2, 0)]),
        );
    }

    #[test]
    fn vt_point_map_unstyled_single_line() {
        let mut list = PageList::init(8, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello"]);

        assert_vt_point_map(
            &list,
            (0, 0),
            (4, 0),
            true,
            true,
            None,
            "hello",
            &coords(&[(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)]),
        );
    }

    #[test]
    fn vt_point_map_style_open_and_final_close() {
        let mut list = PageList::init(8, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello"]);
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        for x in 0..5 {
            let ch = char::from_u32(page_cell(&list.pages[0].page, x, 0).codepoint()).unwrap();
            set_screen_styled_cell(&mut list, x.try_into().unwrap(), 0, ch, bold);
        }

        let expected = "\x1b[0m\x1b[1mhello\x1b[0m";
        let mut expected_points = Vec::new();
        repeat_coords(&mut expected_points, (0, 0), "\x1b[0m\x1b[1m".len());
        expected_points.extend(coords(&[(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)]));
        repeat_coords(&mut expected_points, (4, 0), "\x1b[0m".len());

        assert_vt_point_map(
            &list,
            (0, 0),
            (4, 0),
            true,
            true,
            None,
            expected,
            &expected_points,
        );
    }

    #[test]
    fn vt_point_map_multiple_style_transitions_and_background_space() {
        let mut list = PageList::init(5, 2, None).unwrap();
        let styled = style::Style {
            fg_color: style::Color::Palette(1),
            bg_color: style::Color::Palette(4),
            ..style::Style::default()
        };
        set_screen_styled_cell(&mut list, 0, 0, 'R', styled);
        set_screen_cell_raw(&mut list, 1, 0, Cell::bg_palette(4));
        set_screen_cell(&mut list, 2, 0, 'X');

        let expected = "\x1b[0m\x1b[38;5;1m\x1b[48;5;4mR\x1b[0m\x1b[48;5;4m \x1b[0mX";
        let mut expected_points = Vec::new();
        repeat_coords(
            &mut expected_points,
            (0, 0),
            "\x1b[0m\x1b[38;5;1m\x1b[48;5;4m".len(),
        );
        repeat_coords(&mut expected_points, (0, 0), 1);
        repeat_coords(&mut expected_points, (1, 0), "\x1b[0m\x1b[48;5;4m".len());
        repeat_coords(&mut expected_points, (1, 0), 1);
        repeat_coords(&mut expected_points, (1, 0), "\x1b[0m".len());
        repeat_coords(&mut expected_points, (2, 0), 1);

        assert_vt_point_map(
            &list,
            (0, 0),
            (2, 0),
            true,
            true,
            None,
            expected,
            &expected_points,
        );
    }

    #[test]
    fn vt_point_map_grapheme_and_newline() {
        let mut list = PageList::init(5, 2, None).unwrap();
        set_screen_cell(&mut list, 0, 0, 'e');
        append_screen_grapheme(&mut list, 0, 0, 0x0301);
        set_screen_cell(&mut list, 0, 1, 'x');

        assert_vt_point_map(
            &list,
            (0, 0),
            (0, 1),
            true,
            true,
            None,
            "e\u{301}\r\nx",
            &coords(&[(0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 1)]),
        );
    }

    #[test]
    fn vt_point_map_wide_character_spacer_tail_start() {
        let mut list = PageList::init(6, 2, None).unwrap();
        let mut wide = Cell::init('⚡' as u32);
        wide.set_wide(Wide::Wide);
        set_screen_cell_raw(&mut list, 2, 0, wide);
        let mut tail = Cell::init(0);
        tail.set_wide(Wide::SpacerTail);
        set_screen_cell_raw(&mut list, 3, 0, tail);

        assert_vt_point_map(
            &list,
            (3, 0),
            (3, 0),
            true,
            true,
            None,
            "⚡",
            &coords(&[(2, 0), (2, 0), (2, 0)]),
        );
    }

    #[test]
    fn vt_point_map_generated_blanks_use_reverse_order() {
        let mut list = PageList::init(5, 2, None).unwrap();
        set_screen_cell(&mut list, 0, 0, 'A');
        set_screen_cell(&mut list, 3, 0, 'C');

        assert_vt_point_map(
            &list,
            (0, 0),
            (3, 0),
            false,
            true,
            None,
            "A  C",
            &coords(&[(0, 0), (2, 0), (1, 0), (3, 0)]),
        );
    }

    #[test]
    fn vt_point_map_codepoint_replacements_map_original_cell() {
        let mut list = PageList::init(4, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["ao"]);

        let string_map = [codepoint_map_entry(
            'o',
            'o',
            CodepointReplacement::String("XYZ".to_string()),
        )];
        assert_vt_point_map(
            &list,
            (0, 0),
            (1, 0),
            true,
            true,
            Some(&string_map),
            "aXYZ",
            &coords(&[(0, 0), (1, 0), (1, 0), (1, 0)]),
        );

        let char_map = [codepoint_map_entry(
            'o',
            'o',
            CodepointReplacement::Codepoint('é'),
        )];
        assert_vt_point_map(
            &list,
            (0, 0),
            (1, 0),
            true,
            true,
            Some(&char_map),
            "aé",
            &coords(&[(0, 0), (1, 0), (1, 0)]),
        );
    }

    #[test]
    fn vt_point_map_pending_blank_rows_and_style_reset() {
        let (mut list, page_rows) = multi_page_list(100);
        let first_y = page_rows as u32 - 1;
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        set_screen_styled_cell(&mut list, 0, first_y, 'A', bold);
        set_screen_cell(&mut list, 0, first_y + 2, 'B');

        let expected = "\x1b[0m\x1b[1mA\x1b[0m\r\n\r\nB";
        let mut expected_points = Vec::new();
        repeat_coords(&mut expected_points, (0, first_y), "\x1b[0m\x1b[1m".len());
        repeat_coords(&mut expected_points, (0, first_y), 1);
        repeat_coords(&mut expected_points, (0, first_y), "\x1b[0m".len());
        repeat_coords(&mut expected_points, (0, first_y), "\r\n".len());
        repeat_coords(&mut expected_points, (0, first_y + 1), "\r\n".len());
        repeat_coords(&mut expected_points, (0, first_y + 2), 1);

        assert_vt_point_map(
            &list,
            (0, first_y),
            (0, first_y + 2),
            true,
            true,
            None,
            expected,
            &expected_points,
        );
    }

    #[test]
    fn vt_point_map_multi_page_screen_domain_coordinates() {
        let (mut list, page_rows) = multi_page_list(80);
        let first_y = page_rows as u32 - 1;
        set_screen_cell(&mut list, 0, first_y, 'A');
        set_screen_cell(&mut list, 0, first_y + 1, 'B');

        assert_vt_point_map(
            &list,
            (0, first_y),
            (0, first_y + 1),
            true,
            true,
            None,
            "A\r\nB",
            &coords(&[(0, first_y), (0, first_y), (0, first_y), (0, first_y + 1)]),
        );
    }

    #[test]
    fn vt_point_map_invalid_or_garbage_endpoints_are_empty() {
        let mut list = PageList::init(5, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello"]);
        let other = PageList::init(5, 2, None).unwrap();
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let valid = screen_pin(&list, 0, 0);
        let mut garbage = valid;
        garbage.garbage = true;

        for selection in [
            selection::Selection::new(invalid, valid, false),
            selection::Selection::new(valid, invalid, false),
            selection::Selection::new(garbage, valid, false),
            selection::Selection::new(valid, garbage, false),
        ] {
            assert_vt_point_map_for_selection(&list, Some(selection), true, true, "", &[]);
        }
    }

    #[test]
    fn vt_point_map_html_helper_path_is_deferred() {
        let mut list = PageList::init(8, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hi"]);

        let mut point_map = Vec::new();
        let actual = list.page_string_with_point_map(
            PageStringOptions {
                selection: Some(screen_selection(&list, (0, 0), (1, 0), false)),
                trim: true,
                unwrap: true,
                emit: PageOutputFormat::Html,
                palette: None,
                codepoint_map: None,
            },
            &mut point_map,
        );

        assert_eq!(
            actual,
            "<div style=\"font-family: monospace; white-space: pre;\">hi</div>"
        );
        assert!(point_map.is_empty());
    }

    #[test]
    fn vt_point_map_no_map_and_plain_maps_are_unchanged() {
        let mut list = PageList::init(6, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["A B"]);

        assert_page_string(&list, (0, 0), (2, 0), PageOutputFormat::Vt, None, "A B");
        assert_plain_point_map(
            &list,
            (0, 0),
            (2, 0),
            false,
            true,
            None,
            "A B",
            &coords(&[(0, 0), (1, 0), (2, 0)]),
        );
        assert_plain_pin_map(
            &list,
            (0, 0),
            (2, 0),
            false,
            true,
            None,
            "A B",
            &pins(&list, &[(0, 0), (1, 0), (2, 0)]),
        );
    }

    #[test]
    fn page_string_vt_unstyled_single_line() {
        let mut list = PageList::init(10, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello"]);

        assert_page_string(&list, (0, 0), (4, 0), PageOutputFormat::Vt, None, "hello");
    }

    #[test]
    fn page_string_vt_bold_style() {
        let mut list = PageList::init(10, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello"]);
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        for x in 0..5 {
            let ch = char::from_u32(page_cell(&list.pages[0].page, x, 0).codepoint()).unwrap();
            set_screen_styled_cell(&mut list, x.try_into().unwrap(), 0, ch, bold);
        }

        assert_page_string(
            &list,
            (0, 0),
            (4, 0),
            PageOutputFormat::Vt,
            None,
            "\x1b[0m\x1b[1mhello\x1b[0m",
        );
    }

    #[test]
    fn page_string_vt_multiple_style_transitions() {
        let mut list = PageList::init(12, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello world"]);
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let italic = style::Style {
            flags: style::Flags {
                bold: true,
                italic: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        for x in 0..6 {
            let ch =
                char::from_u32(page_cell(&list.pages[0].page, x as usize, 0).codepoint()).unwrap();
            set_screen_styled_cell(&mut list, x, 0, ch, bold);
        }
        for x in 6..11 {
            let ch =
                char::from_u32(page_cell(&list.pages[0].page, x as usize, 0).codepoint()).unwrap();
            set_screen_styled_cell(&mut list, x, 0, ch, italic);
        }

        assert_page_string(
            &list,
            (0, 0),
            (10, 0),
            PageOutputFormat::Vt,
            None,
            "\x1b[0m\x1b[1mhello \x1b[0m\x1b[1m\x1b[3mworld\x1b[0m",
        );
    }

    #[test]
    fn page_string_vt_palette_modes_and_background_only_cells() {
        let mut list = PageList::init(5, 2, None).unwrap();
        let styled = style::Style {
            fg_color: style::Color::Palette(1),
            bg_color: style::Color::Palette(4),
            ..style::Style::default()
        };
        set_screen_styled_cell(&mut list, 0, 0, 'R', styled);
        set_screen_cell_raw(&mut list, 1, 0, Cell::bg_palette(4));
        set_screen_cell(&mut list, 2, 0, 'X');

        assert_page_string(
            &list,
            (0, 0),
            (2, 0),
            PageOutputFormat::Vt,
            None,
            "\x1b[0m\x1b[38;5;1m\x1b[48;5;4mR\x1b[0m\x1b[48;5;4m \x1b[0mX",
        );
        assert_page_string(
            &list,
            (0, 0),
            (2, 0),
            PageOutputFormat::Vt,
            Some(&color::DEFAULT_PALETTE),
            "\x1b[0m\x1b[38;2;204;102;102m\x1b[48;2;129;162;190mR\x1b[0m\x1b[48;2;129;162;190m \x1b[0mX",
        );
    }

    #[test]
    fn page_string_vt_background_only_row_matches_upstream_skip() {
        let mut list = PageList::init(5, 2, None).unwrap();
        set_screen_cell_raw(&mut list, 0, 0, Cell::bg_palette(4));
        set_screen_cell_raw(&mut list, 1, 0, Cell::bg_rgb(color::Rgb::new(1, 2, 3)));

        assert_page_string(&list, (0, 0), (1, 0), PageOutputFormat::Vt, None, "");
    }

    #[test]
    fn page_string_vt_grapheme_and_newline() {
        let mut list = PageList::init(5, 2, None).unwrap();
        set_screen_cell(&mut list, 0, 0, 'e');
        append_screen_grapheme(&mut list, 0, 0, 0x0301);
        set_screen_cell(&mut list, 0, 1, 'x');

        assert_page_string(
            &list,
            (0, 0),
            (0, 1),
            PageOutputFormat::Vt,
            None,
            "e\u{301}\r\nx",
        );
    }

    #[test]
    fn page_string_vt_closes_style_before_pending_newlines() {
        let (mut list, page_rows) = multi_page_list(100);
        let first_y = page_rows as u32 - 1;
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        set_screen_styled_cell(&mut list, 0, first_y, 'A', bold);
        set_screen_cell(&mut list, 0, first_y + 2, 'B');

        assert_page_string(
            &list,
            (0, first_y),
            (0, first_y + 2),
            PageOutputFormat::Vt,
            None,
            "\x1b[0m\x1b[1mA\x1b[0m\r\n\r\nB",
        );
    }

    #[test]
    fn page_string_invalid_or_garbage_endpoints_are_empty() {
        let mut list = PageList::init(5, 3, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD"]);
        let other = PageList::init(5, 3, None).unwrap();
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let valid = screen_pin(&list, 0, 0);
        let mut garbage = valid;
        garbage.garbage = true;

        for selection in [
            selection::Selection::new(invalid, valid, false),
            selection::Selection::new(valid, invalid, false),
            selection::Selection::new(garbage, valid, false),
            selection::Selection::new(valid, garbage, false),
        ] {
            assert_eq!(
                list.page_string(PageStringOptions {
                    selection: Some(selection),
                    trim: true,
                    unwrap: true,
                    emit: PageOutputFormat::Vt,
                    palette: None,
                    codepoint_map: None,
                }),
                ""
            );
            assert_eq!(
                list.page_string(PageStringOptions {
                    selection: Some(selection),
                    trim: true,
                    unwrap: true,
                    emit: PageOutputFormat::Html,
                    palette: None,
                    codepoint_map: None,
                }),
                ""
            );
        }
    }

    #[test]
    fn page_string_html_plain_text_wrapper() {
        let mut list = PageList::init(20, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["hello, world"]);

        assert_page_string(
            &list,
            (0, 0),
            (11, 0),
            PageOutputFormat::Html,
            None,
            "<div style=\"font-family: monospace; white-space: pre;\">hello, world</div>",
        );
    }

    #[test]
    fn page_string_html_styles_and_palette_modes() {
        let mut list = PageList::init(5, 2, None).unwrap();
        let styled = style::Style {
            fg_color: style::Color::Palette(1),
            bg_color: style::Color::Palette(4),
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        set_screen_styled_cell(&mut list, 0, 0, 'R', styled);
        set_screen_cell_raw(&mut list, 1, 0, Cell::bg_palette(4));
        set_screen_cell(&mut list, 2, 0, 'X');

        assert_page_string(
            &list,
            (0, 0),
            (2, 0),
            PageOutputFormat::Html,
            None,
            "<div style=\"font-family: monospace; white-space: pre;\"><div style=\"display: inline;color: var(--vt-palette-1);background-color: var(--vt-palette-4);font-weight: bold;\">R</div><div style=\"display: inline;background-color: var(--vt-palette-4);\"> </div>X</div>",
        );
        assert_page_string(
            &list,
            (0, 0),
            (2, 0),
            PageOutputFormat::Html,
            Some(&color::DEFAULT_PALETTE),
            "<div style=\"font-family: monospace; white-space: pre;\"><div style=\"display: inline;color: rgb(204, 102, 102);background-color: rgb(129, 162, 190);font-weight: bold;\">R</div><div style=\"display: inline;background-color: rgb(129, 162, 190);\"> </div>X</div>",
        );
    }

    #[test]
    fn page_string_html_escapes_and_numeric_entities() {
        let mut list = PageList::init(12, 2, None).unwrap();
        set_screen_text_lines(&mut list, &["<>&\"'é"]);

        assert_page_string(
            &list,
            (0, 0),
            (5, 0),
            PageOutputFormat::Html,
            None,
            "<div style=\"font-family: monospace; white-space: pre;\">&lt;&gt;&amp;&quot;&#39;&#233;</div>",
        );
    }

    #[test]
    fn page_string_html_grapheme_and_hyperlink_guard() {
        let mut list = PageList::init(8, 2, None).unwrap();
        set_screen_cell(&mut list, 0, 0, 'e');
        append_screen_grapheme(&mut list, 0, 0, 0x0301);
        set_screen_cell(&mut list, 1, 0, 'L');
        let link_id = list.pages[0]
            .page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"guard"),
                uri: b"https://example.com",
            })
            .unwrap();
        list.pages[0].page.set_hyperlink(1, 0, link_id).unwrap();

        assert_page_string(
            &list,
            (0, 0),
            (1, 0),
            PageOutputFormat::Html,
            None,
            "<div style=\"font-family: monospace; white-space: pre;\">e&#769;L</div>",
        );
    }

    fn assert_prompt_click(
        list: &PageList,
        cursor: (CellCountInt, u32),
        cursor_state: SemanticContent,
        click: (CellCountInt, u32),
        mode: PromptClickMode,
        expected: PromptClickMove,
    ) {
        assert_eq!(
            list.prompt_click_move(
                screen_pin(list, cursor.0, cursor.1),
                cursor_state,
                screen_pin(list, click.0, click.1),
                mode,
            ),
            expected
        );
    }

    fn set_prompt_and_input(list: &mut PageList, prompt: &str, input: &str) {
        set_screen_semantic_text(list, 0, 0, prompt, SemanticContent::Prompt);
        set_screen_semantic_text(
            list,
            prompt
                .len()
                .try_into()
                .expect("prompt len must fit CellCountInt"),
            0,
            input,
            SemanticContent::Input,
        );
    }

    #[test]
    fn prompt_click_move_line_right_basic() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_prompt_and_input(&mut list, "> ", "hello");

        assert_prompt_click(
            &list,
            (2, 0),
            SemanticContent::Input,
            (4, 0),
            PromptClickMode::Line,
            PromptClickMove { left: 0, right: 2 },
        );
    }

    #[test]
    fn prompt_click_move_line_right_cursor_not_on_input() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_prompt_and_input(&mut list, "> ", "hello");

        assert_prompt_click(
            &list,
            (0, 0),
            SemanticContent::Output,
            (4, 0),
            PromptClickMode::Line,
            PromptClickMove::ZERO,
        );
    }

    #[test]
    fn prompt_click_move_line_right_click_on_same_position() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_prompt_and_input(&mut list, "> ", "hello");

        assert_prompt_click(
            &list,
            (4, 0),
            SemanticContent::Input,
            (4, 0),
            PromptClickMode::Line,
            PromptClickMove::ZERO,
        );
    }

    #[test]
    fn prompt_click_move_line_right_skips_non_input_cells() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_screen_semantic_text(&mut list, 0, 0, "> ", SemanticContent::Prompt);
        set_screen_semantic_text(&mut list, 2, 0, "h", SemanticContent::Input);
        set_screen_semantic_text(&mut list, 3, 0, "X", SemanticContent::Output);
        set_screen_semantic_text(&mut list, 4, 0, "llo", SemanticContent::Input);

        assert_prompt_click(
            &list,
            (2, 0),
            SemanticContent::Input,
            (5, 0),
            PromptClickMode::Line,
            PromptClickMove { left: 0, right: 2 },
        );
    }

    #[test]
    fn prompt_click_move_line_right_soft_wrapped_line() {
        let mut list = PageList::init(10, 5, None).unwrap();
        set_screen_semantic_text(&mut list, 0, 0, "> ", SemanticContent::Prompt);
        set_screen_semantic_text(&mut list, 2, 0, "abcdefgh", SemanticContent::Input);
        set_screen_semantic_text(&mut list, 0, 1, "ij", SemanticContent::Input);
        set_screen_row_wrap(&mut list, 0, true);
        set_screen_row_wrap_continuation(&mut list, 1, true);
        set_screen_semantic_prompt(&mut list, 1, SemanticPrompt::PromptContinuation);

        assert_prompt_click(
            &list,
            (2, 0),
            SemanticContent::Input,
            (1, 1),
            PromptClickMode::Line,
            PromptClickMove { left: 0, right: 9 },
        );
    }

    #[test]
    fn prompt_click_move_disabled_modes_return_zero() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_prompt_and_input(&mut list, "> ", "hello");

        for mode in [PromptClickMode::None, PromptClickMode::ClickEvents] {
            assert_prompt_click(
                &list,
                (2, 0),
                SemanticContent::Input,
                (4, 0),
                mode,
                PromptClickMove::ZERO,
            );
        }
    }

    #[test]
    fn prompt_click_move_line_right_stops_at_hard_wrap() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_prompt_and_input(&mut list, "> ", "hello");
        set_screen_semantic_text(&mut list, 0, 1, "world", SemanticContent::Input);

        assert_prompt_click(
            &list,
            (2, 0),
            SemanticContent::Input,
            (0, 1),
            PromptClickMode::Line,
            PromptClickMove { left: 0, right: 5 },
        );
    }

    #[test]
    fn prompt_click_move_line_right_stops_at_non_continuation_row() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_screen_semantic_text(&mut list, 0, 0, "> ", SemanticContent::Prompt);
        set_screen_semantic_text(&mut list, 2, 0, "hello", SemanticContent::Input);
        set_screen_semantic_text(&mut list, 0, 1, "world", SemanticContent::Input);
        set_screen_semantic_prompt(&mut list, 1, SemanticPrompt::PromptContinuation);
        set_screen_semantic_text(&mut list, 0, 2, "> ", SemanticContent::Prompt);
        set_screen_semantic_text(&mut list, 2, 2, "again", SemanticContent::Input);

        assert_prompt_click(
            &list,
            (0, 1),
            SemanticContent::Input,
            (2, 2),
            PromptClickMode::Line,
            PromptClickMove { left: 0, right: 5 },
        );
    }

    #[test]
    fn prompt_click_move_line_left_basic() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_prompt_and_input(&mut list, "> ", "hello");

        assert_prompt_click(
            &list,
            (6, 0),
            SemanticContent::Input,
            (2, 0),
            PromptClickMode::Line,
            PromptClickMove { left: 4, right: 0 },
        );
    }

    #[test]
    fn prompt_click_move_line_left_skips_non_input_cells() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_screen_semantic_text(&mut list, 0, 0, "> ", SemanticContent::Prompt);
        set_screen_semantic_text(&mut list, 2, 0, "h", SemanticContent::Input);
        set_screen_semantic_text(&mut list, 3, 0, "X", SemanticContent::Output);
        set_screen_semantic_text(&mut list, 4, 0, "llo", SemanticContent::Input);

        assert_prompt_click(
            &list,
            (6, 0),
            SemanticContent::Input,
            (2, 0),
            PromptClickMode::Line,
            PromptClickMove { left: 3, right: 0 },
        );
    }

    #[test]
    fn prompt_click_move_line_left_soft_wrapped_line() {
        let mut list = PageList::init(10, 5, None).unwrap();
        set_screen_semantic_text(&mut list, 0, 0, "> ", SemanticContent::Prompt);
        set_screen_semantic_text(&mut list, 2, 0, "abcdefgh", SemanticContent::Input);
        set_screen_semantic_text(&mut list, 0, 1, "ij", SemanticContent::Input);
        set_screen_row_wrap(&mut list, 0, true);
        set_screen_row_wrap_continuation(&mut list, 1, true);

        assert_prompt_click(
            &list,
            (1, 1),
            SemanticContent::Input,
            (2, 0),
            PromptClickMode::Line,
            PromptClickMove { left: 9, right: 0 },
        );
    }

    #[test]
    fn prompt_click_move_line_left_stops_at_hard_wrap() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_prompt_and_input(&mut list, "> ", "hello");
        set_screen_semantic_text(&mut list, 0, 1, "world", SemanticContent::Input);

        assert_prompt_click(
            &list,
            (4, 1),
            SemanticContent::Input,
            (2, 0),
            PromptClickMode::Line,
            PromptClickMove { left: 4, right: 0 },
        );
    }

    #[test]
    fn prompt_click_move_click_right_of_input_same_line() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_prompt_and_input(&mut list, "> ", "hello");

        assert_prompt_click(
            &list,
            (2, 0),
            SemanticContent::Input,
            (15, 0),
            PromptClickMode::Line,
            PromptClickMove { left: 0, right: 5 },
        );
    }

    #[test]
    fn prompt_click_move_click_right_of_input_cursor_at_end() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_prompt_and_input(&mut list, "> ", "hello");

        assert_prompt_click(
            &list,
            (7, 0),
            SemanticContent::Input,
            (15, 0),
            PromptClickMode::Line,
            PromptClickMove::ZERO,
        );
    }

    #[test]
    fn prompt_click_move_click_right_of_input_on_lower_line() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_prompt_and_input(&mut list, "> ", "hello");

        assert_prompt_click(
            &list,
            (2, 0),
            SemanticContent::Input,
            (5, 1),
            PromptClickMode::Line,
            PromptClickMove { left: 0, right: 5 },
        );
    }

    #[test]
    fn prompt_click_move_click_right_of_input_cursor_at_end_lower_line() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_prompt_and_input(&mut list, "> ", "hello");

        assert_prompt_click(
            &list,
            (7, 0),
            SemanticContent::Input,
            (5, 1),
            PromptClickMode::Line,
            PromptClickMove::ZERO,
        );
    }

    #[test]
    fn prompt_click_move_click_right_of_input_cursor_on_last_char() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_prompt_and_input(&mut list, "> ", "hello");

        assert_prompt_click(
            &list,
            (6, 0),
            SemanticContent::Input,
            (15, 0),
            PromptClickMode::Line,
            PromptClickMove { left: 0, right: 1 },
        );
    }

    #[test]
    fn prompt_click_move_split_cursor_state_and_page_cell_semantics() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_prompt_and_input(&mut list, "> ", "hello");

        assert_prompt_click(
            &list,
            (7, 0),
            SemanticContent::Input,
            (2, 0),
            PromptClickMode::Line,
            PromptClickMove { left: 5, right: 0 },
        );
        assert_prompt_click(
            &list,
            (7, 0),
            SemanticContent::Input,
            (15, 0),
            PromptClickMode::Line,
            PromptClickMove::ZERO,
        );
        assert_prompt_click(
            &list,
            (2, 0),
            SemanticContent::Output,
            (4, 0),
            PromptClickMode::Line,
            PromptClickMove { left: 0, right: 2 },
        );
    }

    #[test]
    fn prompt_click_move_line_mode_aliases_match_upstream() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_prompt_and_input(&mut list, "> ", "hello");

        for mode in [
            PromptClickMode::Multiple,
            PromptClickMode::ConservativeVertical,
            PromptClickMode::SmartVertical,
        ] {
            assert_prompt_click(
                &list,
                (2, 0),
                SemanticContent::Input,
                (4, 0),
                mode,
                PromptClickMove { left: 0, right: 2 },
            );
        }
    }

    #[test]
    fn prompt_click_move_invalid_or_garbage_pins_return_zero() {
        let mut list = PageList::init(20, 5, None).unwrap();
        set_prompt_and_input(&mut list, "> ", "hello");
        let other = PageList::init(20, 5, None).unwrap();
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let valid = screen_pin(&list, 2, 0);
        let mut garbage = valid;
        garbage.garbage = true;

        for (cursor, click) in [
            (invalid, valid),
            (valid, invalid),
            (garbage, valid),
            (valid, garbage),
        ] {
            assert_eq!(
                list.prompt_click_move(
                    cursor,
                    SemanticContent::Input,
                    click,
                    PromptClickMode::Line
                ),
                PromptClickMove::ZERO
            );
        }
    }

    #[test]
    fn prompt_click_move_cross_page_wrapped_input() {
        let (mut list, page_rows) = multi_page_list(100);
        let first_y = page_rows as u32 - 1;
        set_screen_semantic_text(&mut list, 0, first_y, "abcdef", SemanticContent::Input);
        set_screen_semantic_text(&mut list, 0, first_y + 1, "gh", SemanticContent::Input);
        set_screen_row_wrap(&mut list, first_y, true);
        set_screen_row_wrap_continuation(&mut list, first_y + 1, true);
        set_screen_semantic_prompt(&mut list, first_y + 1, SemanticPrompt::PromptContinuation);

        assert_prompt_click(
            &list,
            (0, first_y),
            SemanticContent::Input,
            (1, first_y + 1),
            PromptClickMode::Line,
            PromptClickMove { left: 0, right: 7 },
        );
    }

    #[test]
    fn select_line_matches_upstream_basic_cases() {
        let mut list = PageList::init(10, 10, None).unwrap();
        set_screen_text_lines(&mut list, &["ABC  DEF", " 123", "456"]);

        assert_line_selection(&list, (0, 0), (0, 0), (7, 0));
        assert_line_selection(&list, (7, 0), (0, 0), (7, 0));
        assert_line_selection(&list, (3, 0), (0, 0), (7, 0));
        assert_line_selection(&list, (9, 0), (0, 0), (7, 0));
        assert!(list
            .select_line(SelectLineOptions::new(screen_pin(&list, 0, 5)))
            .is_none());
    }

    #[test]
    fn select_line_crosses_soft_wrap_like_upstream() {
        let mut list = PageList::init(5, 10, None).unwrap();
        set_screen_text_lines(&mut list, &[" 12 3", "4012 ", "     ", " 123"]);
        set_screen_row_wrap(&mut list, 0, true);
        set_screen_row_wrap(&mut list, 1, true);

        assert_line_selection(&list, (1, 0), (1, 0), (3, 1));
        assert_line_selection(&list, (1, 1), (1, 0), (3, 1));
        assert_line_selection(&list, (3, 0), (1, 0), (3, 1));
    }

    #[test]
    fn select_line_crosses_full_soft_wrap_like_upstream() {
        let mut list = PageList::init(5, 5, None).unwrap();
        set_screen_text_lines(&mut list, &["1ABCD", "2EFGH", "3IJKL"]);
        set_screen_row_wrap(&mut list, 0, true);

        assert_line_selection(&list, (2, 1), (0, 0), (4, 1));
    }

    #[test]
    fn select_line_stops_at_hard_row_boundaries() {
        let mut list = PageList::init(5, 5, None).unwrap();
        set_screen_text_lines(&mut list, &["12345", "678"]);

        assert_line_selection(&list, (2, 0), (0, 0), (4, 0));
        assert_line_selection(&list, (1, 1), (0, 1), (2, 1));
    }

    #[test]
    fn select_line_disabled_whitespace_selects_full_span() {
        let mut list = PageList::init(5, 10, None).unwrap();
        set_screen_text_lines(&mut list, &[" 12 3", "4012 ", "     ", " 123"]);
        set_screen_row_wrap(&mut list, 0, true);
        set_screen_row_wrap(&mut list, 1, true);

        assert_line_selection_with_options(
            &list,
            SelectLineOptions {
                pin: screen_pin(&list, 1, 0),
                whitespace: None,
                semantic_prompt_boundary: true,
            },
            (0, 0),
            (4, 2),
        );
        assert_line_selection_with_options(
            &list,
            SelectLineOptions {
                pin: screen_pin(&list, 1, 3),
                whitespace: None,
                semantic_prompt_boundary: true,
            },
            (0, 3),
            (4, 3),
        );
    }

    #[test]
    fn select_line_with_scrollback_uses_active_coordinates() {
        let mut list = PageList::init(2, 3, None).unwrap();
        list.grow_rows(2).unwrap();
        set_screen_text_lines(&mut list, &["1A", "2B", "3C", "4D", "5E"]);

        let first = list
            .select_line(SelectLineOptions::new(
                list.pin(point::Point::active(Coordinate::new(0, 0)))
                    .unwrap(),
            ))
            .unwrap();
        assert_eq!(
            list.point_from_pin(point::Tag::Active, first.start()),
            Some(point::Point::active(Coordinate::new(0, 0)))
        );
        assert_eq!(
            list.point_from_pin(point::Tag::Active, first.end()),
            Some(point::Point::active(Coordinate::new(1, 0)))
        );
        assert_eq!(screen_coord(&list, first.start()), Coordinate::new(0, 2));
        assert_eq!(screen_coord(&list, first.end()), Coordinate::new(1, 2));

        let last = list
            .select_line(SelectLineOptions::new(
                list.pin(point::Point::active(Coordinate::new(0, 2)))
                    .unwrap(),
            ))
            .unwrap();
        assert_eq!(
            list.point_from_pin(point::Tag::Active, last.start()),
            Some(point::Point::active(Coordinate::new(0, 2)))
        );
        assert_eq!(
            list.point_from_pin(point::Tag::Active, last.end()),
            Some(point::Point::active(Coordinate::new(1, 2)))
        );
        assert_eq!(screen_coord(&list, last.start()), Coordinate::new(0, 4));
        assert_eq!(screen_coord(&list, last.end()), Coordinate::new(1, 4));
    }

    #[test]
    fn select_line_semantic_boundaries_split_mid_row() {
        let mut list = PageList::init(10, 5, None).unwrap();
        for (x, ch) in "$>".chars().enumerate() {
            set_screen_cell_semantic(
                &mut list,
                x.try_into().unwrap(),
                0,
                ch,
                SemanticContent::Prompt,
            );
        }
        for (offset, ch) in "command".chars().enumerate() {
            set_screen_cell_semantic(
                &mut list,
                (offset + 2).try_into().unwrap(),
                0,
                ch,
                SemanticContent::Input,
            );
        }

        assert_line_selection(&list, (0, 0), (0, 0), (1, 0));
        assert_line_selection(&list, (5, 0), (2, 0), (8, 0));
    }

    #[test]
    fn select_line_semantic_boundaries_split_rows() {
        let mut list = PageList::init(10, 5, None).unwrap();
        for (x, ch) in "ls -la".chars().enumerate() {
            set_screen_cell_semantic(
                &mut list,
                x.try_into().unwrap(),
                0,
                ch,
                SemanticContent::Input,
            );
        }
        for (x, ch) in "file.txt".chars().enumerate() {
            set_screen_cell_semantic(
                &mut list,
                x.try_into().unwrap(),
                1,
                ch,
                SemanticContent::Output,
            );
        }

        assert_line_selection(&list, (2, 0), (0, 0), (5, 0));
        assert_line_selection(&list, (2, 1), (0, 1), (7, 1));
    }

    #[test]
    fn select_line_semantic_boundaries_split_output_prompt_input() {
        let mut list = PageList::init(10, 5, None).unwrap();
        for (x, ch) in "out".chars().enumerate() {
            set_screen_cell_semantic(
                &mut list,
                x.try_into().unwrap(),
                0,
                ch,
                SemanticContent::Output,
            );
        }
        for (offset, ch) in "$>".chars().enumerate() {
            set_screen_cell_semantic(
                &mut list,
                (offset + 3).try_into().unwrap(),
                0,
                ch,
                SemanticContent::Prompt,
            );
        }
        for (offset, ch) in "cmd".chars().enumerate() {
            set_screen_cell_semantic(
                &mut list,
                (offset + 5).try_into().unwrap(),
                0,
                ch,
                SemanticContent::Input,
            );
        }

        assert_line_selection(&list, (1, 0), (0, 0), (2, 0));
        assert_line_selection(&list, (3, 0), (3, 0), (4, 0));
        assert_line_selection(&list, (6, 0), (5, 0), (7, 0));
    }

    #[test]
    fn select_line_semantic_boundaries_cross_soft_wrap() {
        let mut list = PageList::init(5, 5, None).unwrap();
        set_screen_cell_semantic(&mut list, 0, 0, '$', SemanticContent::Prompt);
        set_screen_cell_semantic(&mut list, 1, 0, ' ', SemanticContent::Prompt);
        for (offset, ch) in "cmd".chars().enumerate() {
            set_screen_cell_semantic(
                &mut list,
                (offset + 2).try_into().unwrap(),
                0,
                ch,
                SemanticContent::Input,
            );
        }
        set_screen_row_wrap(&mut list, 0, true);
        set_screen_cell_semantic(&mut list, 0, 1, '1', SemanticContent::Input);
        set_screen_cell_semantic(&mut list, 1, 1, '2', SemanticContent::Input);
        for (offset, ch) in "out".chars().enumerate() {
            set_screen_cell_semantic(
                &mut list,
                (offset + 2).try_into().unwrap(),
                1,
                ch,
                SemanticContent::Output,
            );
        }

        assert_line_selection(&list, (3, 0), (2, 0), (1, 1));
        assert_line_selection(&list, (0, 1), (2, 0), (1, 1));
        assert_line_selection(&list, (3, 1), (2, 1), (4, 1));
    }

    #[test]
    fn select_line_disabled_semantic_boundary_selects_whole_line() {
        let mut list = PageList::init(10, 5, None).unwrap();
        set_screen_cell_semantic(&mut list, 0, 0, '$', SemanticContent::Prompt);
        set_screen_cell_semantic(&mut list, 1, 0, ' ', SemanticContent::Prompt);
        for (offset, ch) in "command".chars().enumerate() {
            set_screen_cell_semantic(
                &mut list,
                (offset + 2).try_into().unwrap(),
                0,
                ch,
                SemanticContent::Input,
            );
        }

        assert_line_selection_with_options(
            &list,
            SelectLineOptions {
                pin: screen_pin(&list, 0, 0),
                whitespace: Some(selection_codepoints::DEFAULT_LINE_WHITESPACE),
                semantic_prompt_boundary: false,
            },
            (0, 0),
            (8, 0),
        );
    }

    #[test]
    fn select_line_disabled_whitespace_still_honors_semantic_boundaries() {
        let mut list = PageList::init(10, 5, None).unwrap();
        set_screen_cell_semantic(&mut list, 0, 0, '$', SemanticContent::Prompt);
        set_screen_cell_semantic(&mut list, 1, 0, ' ', SemanticContent::Prompt);
        for (offset, ch) in "command".chars().enumerate() {
            set_screen_cell_semantic(
                &mut list,
                (offset + 2).try_into().unwrap(),
                0,
                ch,
                SemanticContent::Input,
            );
        }

        assert_line_selection_with_options(
            &list,
            SelectLineOptions {
                pin: screen_pin(&list, 0, 0),
                whitespace: None,
                semantic_prompt_boundary: true,
            },
            (0, 0),
            (1, 0),
        );
        assert_line_selection_with_options(
            &list,
            SelectLineOptions {
                pin: screen_pin(&list, 0, 0),
                whitespace: None,
                semantic_prompt_boundary: false,
            },
            (0, 0),
            (9, 0),
        );
    }

    #[test]
    fn select_line_semantic_boundary_first_cell_of_row() {
        let mut list = PageList::init(5, 5, None).unwrap();
        for (x, ch) in "12345".chars().enumerate() {
            set_screen_cell_semantic(
                &mut list,
                x.try_into().unwrap(),
                0,
                ch,
                SemanticContent::Input,
            );
        }
        set_screen_row_wrap(&mut list, 0, true);
        for (x, ch) in "ABCDE".chars().enumerate() {
            set_screen_cell_semantic(
                &mut list,
                x.try_into().unwrap(),
                1,
                ch,
                SemanticContent::Output,
            );
        }

        assert_line_selection(&list, (2, 0), (0, 0), (4, 0));
        assert_line_selection(&list, (2, 1), (0, 1), (4, 1));
    }

    #[test]
    fn select_line_semantic_all_same_content_crosses_soft_wrap() {
        let mut list = PageList::init(5, 5, None).unwrap();
        for (y, text) in ["promp", "t tex", "t"].iter().enumerate() {
            for (x, ch) in text.chars().enumerate() {
                set_screen_cell_semantic(
                    &mut list,
                    x.try_into().unwrap(),
                    y.try_into().unwrap(),
                    ch,
                    SemanticContent::Prompt,
                );
            }
        }
        set_screen_row_wrap(&mut list, 0, true);
        set_screen_row_wrap(&mut list, 1, true);

        assert_line_selection(&list, (2, 1), (0, 0), (0, 2));
    }

    #[test]
    fn select_line_rejects_invalid_garbage_and_all_whitespace() {
        let mut list = PageList::init(5, 5, None).unwrap();
        let other = PageList::init(5, 5, None).unwrap();
        set_screen_text_lines(&mut list, &["     ", "     "]);
        set_screen_row_wrap(&mut list, 0, true);
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let mut garbage = screen_pin(&list, 0, 0);
        garbage.garbage = true;

        assert!(list.select_line(SelectLineOptions::new(invalid)).is_none());
        assert!(list.select_line(SelectLineOptions::new(garbage)).is_none());
        assert!(list
            .select_line(SelectLineOptions::new(screen_pin(&list, 0, 0)))
            .is_none());
    }

    #[test]
    fn select_word_matches_upstream_basic_cases() {
        let mut list = PageList::init(10, 10, None).unwrap();
        set_screen_text_lines(&mut list, &["ABC  DEF", " 123", "456"]);

        assert_word_selection(&list, (0, 0), (0, 0), (2, 0));
        assert_word_selection(&list, (2, 0), (0, 0), (2, 0));
        assert_word_selection(&list, (1, 0), (0, 0), (2, 0));
        assert_word_selection(&list, (3, 0), (3, 0), (4, 0));
        assert_word_selection(&list, (0, 1), (0, 1), (0, 1));
        assert_word_selection(&list, (1, 2), (0, 2), (2, 2));
        assert!(list
            .select_word(
                screen_pin(&list, 9, 0),
                selection_codepoints::DEFAULT_WORD_BOUNDARIES,
            )
            .is_none());
        assert!(list
            .select_word(
                screen_pin(&list, 0, 5),
                selection_codepoints::DEFAULT_WORD_BOUNDARIES,
            )
            .is_none());
    }

    #[test]
    fn select_word_crosses_soft_wrap_like_upstream() {
        let mut list = PageList::init(5, 10, None).unwrap();
        set_screen_text_lines(&mut list, &[" 1234", "012", " 123"]);
        set_screen_row_wrap(&mut list, 0, true);

        assert_word_selection(&list, (1, 0), (1, 0), (2, 1));
        assert_word_selection(&list, (1, 1), (1, 0), (2, 1));
        assert_word_selection(&list, (3, 0), (1, 0), (2, 1));
    }

    #[test]
    fn select_word_whitespace_crosses_soft_wrap_like_upstream() {
        let mut list = PageList::init(5, 10, None).unwrap();
        set_screen_text_lines(&mut list, &["1    ", "   1", " 123"]);
        set_screen_row_wrap(&mut list, 0, true);

        assert_word_selection(&list, (1, 0), (1, 0), (2, 1));
        assert_word_selection(&list, (1, 1), (1, 0), (2, 1));
        assert_word_selection(&list, (3, 0), (1, 0), (2, 1));
    }

    #[test]
    fn select_word_stops_at_hard_row_boundaries() {
        let mut list = PageList::init(5, 10, None).unwrap();
        set_screen_text_lines(&mut list, &["12345", "678"]);

        assert_word_selection(&list, (1, 0), (0, 0), (4, 0));
        assert_word_selection(&list, (1, 1), (0, 1), (2, 1));
    }

    #[test]
    fn select_word_matches_upstream_character_boundaries() {
        let cases = [
            " 'abc' ",
            " \"abc\" ",
            " │abc│ ",
            " `abc` ",
            " |abc| ",
            " :abc: ",
            " ;abc; ",
            " ,abc, ",
            " (abc( ",
            " )abc) ",
            " [abc[ ",
            " ]abc] ",
            " {abc{ ",
            " }abc} ",
            " <abc< ",
            " >abc> ",
            " $abc$ ",
        ];

        for case in cases {
            let mut list = PageList::init(20, 10, None).unwrap();
            set_screen_text_lines(&mut list, &[case, "123"]);

            assert_word_selection(&list, (2, 0), (2, 0), (4, 0));
            assert_word_selection(&list, (4, 0), (2, 0), (4, 0));
            assert_word_selection(&list, (3, 0), (2, 0), (4, 0));
            assert_word_selection(&list, (1, 0), (0, 0), (1, 0));
        }
    }

    #[test]
    fn select_word_rejects_invalid_or_garbage_pins() {
        let mut list = PageList::init(10, 5, None).unwrap();
        let other = PageList::init(10, 5, None).unwrap();
        set_screen_text_lines(&mut list, &["hello"]);
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let mut garbage = screen_pin(&list, 0, 0);
        garbage.garbage = true;

        assert!(list
            .select_word(invalid, selection_codepoints::DEFAULT_WORD_BOUNDARIES)
            .is_none());
        assert!(list
            .select_word(garbage, selection_codepoints::DEFAULT_WORD_BOUNDARIES)
            .is_none());
    }

    #[test]
    fn select_word_between_finds_nearest_word_in_direction() {
        let mut list = PageList::init(10, 3, None).unwrap();
        set_screen_cell(&mut list, 3, 0, 'a');
        set_screen_cell(&mut list, 4, 0, 'b');
        set_screen_cell(&mut list, 5, 0, 'c');
        set_screen_cell(&mut list, 7, 0, 'd');
        set_screen_cell(&mut list, 8, 0, 'e');
        set_screen_cell(&mut list, 9, 0, 'f');

        assert_word_between_selection(&list, (0, 0), (9, 0), (3, 0), (5, 0));
        assert_word_between_selection(&list, (9, 0), (0, 0), (7, 0), (9, 0));
        assert_word_between_selection(&list, (4, 0), (4, 0), (3, 0), (5, 0));
        assert!(list
            .select_word_between(
                screen_pin(&list, 0, 1),
                screen_pin(&list, 9, 1),
                selection_codepoints::DEFAULT_WORD_BOUNDARIES,
            )
            .is_none());
    }

    #[test]
    fn select_word_between_rejects_invalid_or_garbage_pins() {
        let mut list = PageList::init(10, 5, None).unwrap();
        let other = PageList::init(10, 5, None).unwrap();
        set_screen_text_lines(&mut list, &["hello"]);
        let valid = screen_pin(&list, 0, 0);
        let invalid = Pin::new(other.first_node_ptr(), 0, 0);
        let mut garbage = valid;
        garbage.garbage = true;

        assert!(list
            .select_word_between(
                invalid,
                valid,
                selection_codepoints::DEFAULT_WORD_BOUNDARIES
            )
            .is_none());
        assert!(list
            .select_word_between(
                valid,
                invalid,
                selection_codepoints::DEFAULT_WORD_BOUNDARIES
            )
            .is_none());
        assert!(list
            .select_word_between(
                garbage,
                valid,
                selection_codepoints::DEFAULT_WORD_BOUNDARIES
            )
            .is_none());
        assert!(list
            .select_word_between(
                valid,
                garbage,
                selection_codepoints::DEFAULT_WORD_BOUNDARIES
            )
            .is_none());
    }

    #[test]
    fn page_list_initially_tracks_viewport_pin() {
        let list = PageList::init(80, 24, None).unwrap();

        assert_eq!(list.count_tracked_pins(), 1);
        assert_eq!(list.tracked_pins(), &[NonNull::from(&*list.viewport_pin)]);
    }

    #[test]
    fn page_list_track_pin_adds_stable_valid_pin() {
        let mut list = PageList::init(80, 24, None).unwrap();
        let pin = list
            .pin(point::Point::active(Coordinate::new(4, 2)))
            .unwrap();
        let tracked = list.track_pin(pin).unwrap();

        assert_eq!(list.count_tracked_pins(), 2);
        assert_eq!(list.tracked_pin_storage.len(), 1);
        assert_eq!(list.tracked_pins()[1], tracked);
        let tracked_pin = unsafe {
            // Safety: tracked was just returned by track_pin and remains owned
            // by list.tracked_pin_storage.
            tracked.as_ref()
        };
        assert_eq!(*tracked_pin, pin);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_track_pin_keeps_duplicate_pins_distinct() {
        let mut list = PageList::init(80, 24, None).unwrap();
        let pin = list
            .pin(point::Point::active(Coordinate::new(4, 2)))
            .unwrap();
        let first = list.track_pin(pin).unwrap();
        let second = list.track_pin(pin).unwrap();

        assert_ne!(first, second);
        assert_eq!(list.count_tracked_pins(), 3);
        assert_eq!(list.tracked_pin_storage.len(), 2);
        assert_eq!(
            list.tracked_pins(),
            &[NonNull::from(&*list.viewport_pin), first, second]
        );
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_untrack_pin_removes_arbitrary_pin() {
        let mut list = PageList::init(80, 24, None).unwrap();
        let pin = list
            .pin(point::Point::active(Coordinate::new(4, 2)))
            .unwrap();
        let tracked = list.track_pin(pin).unwrap();

        list.untrack_pin(tracked);

        assert_eq!(list.count_tracked_pins(), 1);
        assert_eq!(list.tracked_pin_storage.len(), 0);
        assert_eq!(list.tracked_pins(), &[NonNull::from(&*list.viewport_pin)]);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_untrack_pin_is_idempotent_after_first_removal() {
        let mut list = PageList::init(80, 24, None).unwrap();
        let pin = list
            .pin(point::Point::active(Coordinate::new(4, 2)))
            .unwrap();
        let tracked = list.track_pin(pin).unwrap();

        list.untrack_pin(tracked);
        list.untrack_pin(tracked);

        assert_eq!(list.count_tracked_pins(), 1);
        assert_eq!(list.tracked_pin_storage.len(), 0);
        list.verify_integrity().unwrap();
    }

    #[test]
    #[should_panic(expected = "assertion `left != right` failed")]
    fn page_list_untrack_viewport_pin_panics() {
        let mut list = PageList::init(80, 24, None).unwrap();

        list.untrack_pin(NonNull::from(&*list.viewport_pin));
    }

    #[test]
    fn page_list_track_pin_rejects_invalid_pin() {
        let mut list = PageList::init(80, 24, None).unwrap();
        let invalid = Pin {
            node: NonNull::from(list.pages[0].as_ref()),
            y: 0,
            x: list.pages[0].page.size_cols(),
            garbage: false,
        };

        assert_eq!(list.track_pin(invalid), None);
        assert_eq!(list.count_tracked_pins(), 1);
        assert_eq!(list.tracked_pin_storage.len(), 0);
        list.verify_integrity().unwrap();
    }

    #[test]
    fn page_list_untrack_removes_pin_from_integrity_consideration() {
        let mut list = PageList::init(80, 24, None).unwrap();
        let pin = list
            .pin(point::Point::active(Coordinate::new(4, 2)))
            .unwrap();
        let tracked = list.track_pin(pin).unwrap();
        unsafe {
            // Safety: tracked remains owned by list.tracked_pin_storage until
            // untrack_pin removes it below.
            tracked.as_ptr().write(Pin {
                x: list.pages[0].page.size_cols(),
                ..pin
            });
        }
        assert_eq!(
            list.verify_integrity(),
            Err(IntegrityError::TrackedPinInvalid)
        );

        list.untrack_pin(tracked);

        assert_eq!(list.count_tracked_pins(), 1);
        list.verify_integrity().unwrap();
    }
}
