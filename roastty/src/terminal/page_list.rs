use std::ptr::NonNull;

use super::page::{page_layout, Capacity, CapacityAdjustment, Page, PageAllocError, STD_CAPACITY};
use super::point::{self, Coordinate};
use super::size::CellCountInt;

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
struct Node {
    page: Page,
    serial: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pin {
    node: NonNull<Node>,
    y: CellCountInt,
    x: CellCountInt,
    garbage: bool,
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
    use crate::terminal::page::page_layout;

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
