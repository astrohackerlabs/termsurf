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
pub(super) enum PageListAllocError {
    PageAlloc,
}

impl From<PageAllocError> for PageListAllocError {
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
        };
        result
            .verify_integrity()
            .expect("newly initialized PageList should be valid");
        Ok(result)
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

        Ok(())
    }

    fn pin_is_valid(&self, pin: &Pin) -> bool {
        let Some(node) = self.node_for_pin(pin) else {
            return false;
        };

        pin.x < node.page.size_cols() && pin.y < node.page.size_rows()
    }

    fn node_for_pin(&self, pin: &Pin) -> Option<&Node> {
        self.pages
            .iter()
            .map(Box::as_ref)
            .find(|node| NonNull::from(*node) == pin.node)
    }

    fn node_index(&self, node_ptr: NonNull<Node>) -> Option<usize> {
        self.pages
            .iter()
            .position(|node| NonNull::from(node.as_ref()) == node_ptr)
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
