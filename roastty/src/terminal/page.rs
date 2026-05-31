use std::io;
use std::mem::{align_of, size_of};
use std::ptr::NonNull;

use super::bitmap_allocator::{BitmapAllocator, Layout as BitmapAllocatorLayout};
use super::color::Rgb;
use super::offset_hash_map;
use super::size::{
    get_offset, CellCountInt, GraphemeBytesInt, HyperlinkCountInt, Offset, OffsetSlice,
    StringBytesInt,
};
use super::style;

const PAGE_SIZE_MIN: usize = 16_384;
const CODEPOINT_STORAGE_SIZE: usize = 4;
const GRAPHEME_CHUNK_LEN: usize = 4;
const GRAPHEME_CHUNK: usize = GRAPHEME_CHUNK_LEN * CODEPOINT_STORAGE_SIZE;
type GraphemeAlloc = BitmapAllocator<GRAPHEME_CHUNK>;
const GRAPHEME_COUNT_DEFAULT: usize = GraphemeAlloc::BITMAP_BIT_SIZE;
pub(super) const GRAPHEME_BYTES_DEFAULT: GraphemeBytesInt =
    (GRAPHEME_COUNT_DEFAULT * GRAPHEME_CHUNK) as GraphemeBytesInt;

const STRING_CHUNK_LEN: usize = 32;
const STRING_CHUNK: usize = STRING_CHUNK_LEN * size_of::<u8>();
type StringAlloc = BitmapAllocator<STRING_CHUNK>;
const STRING_COUNT_DEFAULT: usize = StringAlloc::BITMAP_BIT_SIZE;
pub(super) const STRING_BYTES_DEFAULT: StringBytesInt =
    (STRING_COUNT_DEFAULT * STRING_CHUNK) as StringBytesInt;

const HYPERLINK_COUNT_DEFAULT: usize = 4;
const HYPERLINK_CELL_MULTIPLIER: usize = 16;
pub(super) const HYPERLINK_BYTES_DEFAULT: HyperlinkCountInt =
    (HYPERLINK_COUNT_DEFAULT * HYPERLINK_SET_ITEM_SIZE) as HyperlinkCountInt;

const STYLE_VALUE_SIZE: usize = 28;
const STYLE_VALUE_ALIGN: usize = 4;
const STYLE_SET_ITEM_SIZE: usize = 36;
const STYLE_SET_ITEM_ALIGN: usize = 4;
const HYPERLINK_PAGE_ENTRY_SIZE: usize = 40;
const HYPERLINK_PAGE_ENTRY_ALIGN: usize = 8;
const HYPERLINK_SET_ITEM_SIZE: usize = 48;
const HYPERLINK_SET_ITEM_ALIGN: usize = 8;
const REF_COUNTED_SET_ID_SIZE: usize = size_of::<u16>();
const REF_COUNTED_SET_ID_ALIGN: usize = align_of::<u16>();
const HASH_MAP_HEADER_SIZE: usize = 16;
const HASH_MAP_HEADER_ALIGN: usize = 4;
const HASH_MAP_METADATA_SIZE: usize = 1;
const HASH_MAP_METADATA_ALIGN: usize = 1;
const OFFSET_CELL_SIZE: usize = size_of::<Offset<Cell>>();
const OFFSET_CELL_ALIGN: usize = align_of::<Offset<Cell>>();
const OFFSET_U21_SLICE_SIZE: usize = 16;
const OFFSET_U21_SLICE_ALIGN: usize = 8;
const HYPERLINK_ID_SIZE: usize = size_of::<HyperlinkId>();
const HYPERLINK_ID_ALIGN: usize = align_of::<HyperlinkId>();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Size {
    cols: CellCountInt,
    rows: CellCountInt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Capacity {
    cols: CellCountInt,
    rows: CellCountInt,
    styles: CellCountInt,
    hyperlink_bytes: HyperlinkCountInt,
    grapheme_bytes: GraphemeBytesInt,
    string_bytes: StringBytesInt,
}

impl Capacity {
    pub(super) const fn new(cols: CellCountInt, rows: CellCountInt) -> Self {
        Self {
            cols,
            rows,
            styles: 16,
            hyperlink_bytes: HYPERLINK_BYTES_DEFAULT,
            grapheme_bytes: GRAPHEME_BYTES_DEFAULT,
            string_bytes: STRING_BYTES_DEFAULT,
        }
    }

    pub(super) const fn with_metadata(
        cols: CellCountInt,
        rows: CellCountInt,
        styles: CellCountInt,
        hyperlink_bytes: HyperlinkCountInt,
        grapheme_bytes: GraphemeBytesInt,
        string_bytes: StringBytesInt,
    ) -> Self {
        Self {
            cols,
            rows,
            styles,
            hyperlink_bytes,
            grapheme_bytes,
            string_bytes,
        }
    }

    pub(super) fn max_cols(self) -> Option<CellCountInt> {
        let available_bits = self.available_bits_for_grid();
        let row_bits = row_bits();
        if available_bits <= row_bits {
            return None;
        }

        let remaining_bits = available_bits - row_bits;
        let max_cols = remaining_bits / cell_bits();
        Some(max_cols.min(CellCountInt::MAX as usize) as CellCountInt)
    }

    pub(super) fn adjust(
        self,
        adjustment: CapacityAdjustment,
    ) -> Result<Self, CapacityAdjustError> {
        let mut adjusted = self;
        if let Some(cols) = adjustment.cols {
            let available_bits = self.available_bits_for_grid();
            let bits_per_row = row_bits() + cell_bits() * cols as usize;
            let new_rows = available_bits / bits_per_row;
            if new_rows == 0 {
                return Err(CapacityAdjustError::OutOfMemory);
            }

            adjusted.cols = cols;
            adjusted.rows =
                CellCountInt::try_from(new_rows).expect("adjusted row count must fit u16");
        }

        Ok(adjusted)
    }

    fn available_bits_for_grid(self) -> usize {
        assert_eq!(size_of::<Row>() % align_of::<Cell>(), 0);

        let layout = page_layout(self);
        let hyperlink_map_start = align_backward(
            layout.total_size - layout.hyperlink_map_layout.total_size,
            HyperlinkMapLayout::BASE_ALIGN,
        );
        let hyperlink_set_start = align_backward(
            hyperlink_map_start - layout.hyperlink_set_layout.total_size,
            HyperlinkSetLayout::BASE_ALIGN,
        );
        let string_alloc_start = align_backward(
            hyperlink_set_start - layout.string_alloc_layout.total_size,
            StringAlloc::BASE_ALIGN,
        );
        let grapheme_map_start = align_backward(
            string_alloc_start - layout.grapheme_map_layout.total_size,
            GraphemeMapLayout::BASE_ALIGN,
        );
        let grapheme_alloc_start = align_backward(
            grapheme_map_start - layout.grapheme_alloc_layout.total_size,
            GraphemeAlloc::BASE_ALIGN,
        );
        let styles_start = align_backward(
            grapheme_alloc_start - layout.styles_layout.total_size,
            StyleSetLayout::BASE_ALIGN,
        );

        styles_start * u8::BITS as usize
    }
}

pub(super) const STD_CAPACITY: Capacity = Capacity {
    cols: 215,
    rows: 215,
    styles: 128,
    hyperlink_bytes: HYPERLINK_BYTES_DEFAULT,
    grapheme_bytes: 512,
    string_bytes: STRING_BYTES_DEFAULT,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct CapacityAdjustment {
    cols: Option<CellCountInt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CapacityAdjustError {
    OutOfMemory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PageLayout {
    total_size: usize,
    rows_start: usize,
    rows_size: usize,
    cells_start: usize,
    cells_size: usize,
    styles_start: usize,
    styles_layout: StyleSetLayout,
    grapheme_alloc_start: usize,
    grapheme_alloc_layout: BitmapAllocatorLayout,
    grapheme_map_start: usize,
    grapheme_map_layout: GraphemeMapLayout,
    string_alloc_start: usize,
    string_alloc_layout: BitmapAllocatorLayout,
    hyperlink_map_start: usize,
    hyperlink_map_layout: HyperlinkMapLayout,
    hyperlink_set_start: usize,
    hyperlink_set_layout: HyperlinkSetLayout,
    capacity: Capacity,
}

pub(super) fn page_layout(capacity: Capacity) -> PageLayout {
    let rows_count = capacity.rows as usize;
    let rows_start = 0;
    let rows_end = rows_start + rows_count * size_of::<Row>();

    let cells_count = capacity.cols as usize * capacity.rows as usize;
    let cells_start = align_forward(rows_end, align_of::<Cell>());
    let cells_end = cells_start + cells_count * size_of::<Cell>();

    let styles_layout = StyleSetLayout::init(capacity.styles as usize);
    let styles_start = align_forward(cells_end, StyleSetLayout::BASE_ALIGN);
    let styles_end = styles_start + styles_layout.total_size;

    let grapheme_alloc_layout = GraphemeAlloc::layout(capacity.grapheme_bytes as usize);
    let grapheme_alloc_start = align_forward(styles_end, GraphemeAlloc::BASE_ALIGN);
    let grapheme_alloc_end = grapheme_alloc_start + grapheme_alloc_layout.total_size;

    let grapheme_count = if capacity.grapheme_bytes == 0 {
        0
    } else {
        div_ceil(capacity.grapheme_bytes as usize, GRAPHEME_CHUNK).next_power_of_two()
    };
    let grapheme_map_layout = GraphemeMapLayout::layout(grapheme_count as u32);
    let grapheme_map_start = align_forward(grapheme_alloc_end, GraphemeMapLayout::BASE_ALIGN);
    let grapheme_map_end = grapheme_map_start + grapheme_map_layout.total_size;

    let string_alloc_layout = StringAlloc::layout(capacity.string_bytes as usize);
    let string_alloc_start = align_forward(grapheme_map_end, StringAlloc::BASE_ALIGN);
    let string_alloc_end = string_alloc_start + string_alloc_layout.total_size;

    let hyperlink_count = capacity.hyperlink_bytes as usize / HYPERLINK_SET_ITEM_SIZE;
    let hyperlink_set_layout = HyperlinkSetLayout::init(hyperlink_count);
    let hyperlink_set_start = align_forward(string_alloc_end, HyperlinkSetLayout::BASE_ALIGN);
    let hyperlink_set_end = hyperlink_set_start + hyperlink_set_layout.total_size;

    let hyperlink_map_count = if hyperlink_count == 0 {
        0
    } else {
        hyperlink_count
            .checked_mul(HYPERLINK_CELL_MULTIPLIER)
            .and_then(|count| u32::try_from(count).ok())
            .unwrap_or(u32::MAX)
            .next_power_of_two()
    };
    let hyperlink_map_layout = HyperlinkMapLayout::layout(hyperlink_map_count);
    let hyperlink_map_start = align_forward(hyperlink_set_end, HyperlinkMapLayout::BASE_ALIGN);
    let hyperlink_map_end = hyperlink_map_start + hyperlink_map_layout.total_size;

    let total_size = align_forward(hyperlink_map_end, PAGE_SIZE_MIN);

    PageLayout {
        total_size,
        rows_start,
        rows_size: rows_end - rows_start,
        cells_start,
        cells_size: cells_end - cells_start,
        styles_start,
        styles_layout,
        grapheme_alloc_start,
        grapheme_alloc_layout,
        grapheme_map_start,
        grapheme_map_layout,
        string_alloc_start,
        string_alloc_layout,
        hyperlink_map_start,
        hyperlink_map_layout,
        hyperlink_set_start,
        hyperlink_set_layout,
        capacity,
    }
}

#[derive(Debug)]
pub(super) struct Page {
    memory: PageMemory,
    rows: Offset<Row>,
    cells: Offset<Cell>,
    dirty: bool,
    size: Size,
    capacity: Capacity,
    layout: PageLayout,
    styles: style::Set,
    grapheme_alloc: GraphemeAlloc,
    grapheme_map: Option<offset_hash_map::OffsetHashMap<Offset<Cell>, OffsetSlice<u32>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GraphemeError {
    GraphemeMapOutOfMemory,
    GraphemeAllocOutOfMemory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CloneFromError {
    SourceRowManagedMemory,
    DestinationRowManagedMemory,
    SourceCellManagedMemory,
    DestinationCellManagedMemory,
}

impl Page {
    pub(super) fn init(capacity: Capacity) -> Result<Self, PageAllocError> {
        let layout = page_layout(capacity);
        assert_eq!(layout.total_size % PAGE_SIZE_MIN, 0);

        let mut memory = PageMemory::new(layout.total_size)?;
        let buf = super::size::OffsetBuf::init(memory.as_mut_ptr());
        let rows = buf.member::<Row>(layout.rows_start);
        let cells = buf.member::<Cell>(layout.cells_start);
        let styles = style::Set::init(
            unsafe {
                // Safety: styles_start is inside this page's live backing memory.
                memory.as_mut_ptr().add(layout.styles_start)
            },
            layout.styles_layout.into(),
        );
        let grapheme_alloc = unsafe {
            // Safety: PageMemory is live for the lifetime of Page, and the
            // layout range is inside the page backing allocation.
            GraphemeAlloc::init(
                super::size::OffsetBuf::init_offset(
                    memory.as_mut_ptr(),
                    layout.grapheme_alloc_start,
                ),
                layout.grapheme_alloc_layout,
            )
        };
        let grapheme_map = if layout.grapheme_map_layout.capacity == 0 {
            None
        } else {
            Some(unsafe {
                // Safety: PageMemory is live for the lifetime of Page, and the
                // layout range is inside the page backing allocation.
                offset_hash_map::OffsetHashMap::init(
                    super::size::OffsetBuf::init_offset(
                        memory.as_mut_ptr(),
                        layout.grapheme_map_start,
                    ),
                    layout.grapheme_map_layout.into(),
                )
            })
        };

        let cells_len = capacity.cols as usize * capacity.rows as usize;
        let cells_ptr = cells.ptr(memory.as_ptr());
        for y in 0..capacity.rows as usize {
            let start = y * capacity.cols as usize;
            debug_assert!(start < cells_len || cells_len == 0);

            let mut row = Row::default();
            row.set_cells(Offset::new(
                (layout.cells_start + start * size_of::<Cell>())
                    .try_into()
                    .expect("cell offset must fit OffsetInt"),
            ));

            unsafe {
                // Safety: `rows` points into the live page backing memory and
                // `y < capacity.rows`, so this writes one initialized row.
                *rows.ptr_mut(memory.as_mut_ptr()).add(y) = row;
            }
            debug_assert_eq!(row.cells().ptr(memory.as_ptr()), unsafe {
                // Safety: `start` is within the allocated cells region.
                cells_ptr.add(start)
            });
        }

        Ok(Self {
            memory,
            rows,
            cells,
            dirty: false,
            size: Size {
                cols: capacity.cols,
                rows: capacity.rows,
            },
            capacity,
            layout,
            styles,
            grapheme_alloc,
            grapheme_map,
        })
    }

    pub(super) fn backing_len(&self) -> usize {
        self.memory.len()
    }

    pub(super) fn backing_ptr(&self) -> *const u8 {
        self.memory.as_ptr()
    }

    pub(super) fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub(super) fn capacity(&self) -> Capacity {
        self.capacity
    }

    pub(super) fn clone_page(&self) -> Result<Self, PageAllocError> {
        let mut memory = PageMemory::new(self.memory.len())?;
        memory
            .as_mut_slice()
            .copy_from_slice(self.memory.as_slice());

        Ok(Self {
            memory,
            rows: self.rows,
            cells: self.cells,
            dirty: self.dirty,
            size: self.size,
            capacity: self.capacity,
            layout: self.layout,
            styles: self.styles,
            grapheme_alloc: self.grapheme_alloc,
            grapheme_map: self.grapheme_map,
        })
    }

    fn clone_plain_rows_from(
        &mut self,
        other: &Page,
        y_start: usize,
        y_end: usize,
    ) -> Result<(), CloneFromError> {
        assert!(y_start <= y_end);
        assert!(y_end <= other.size.rows as usize);
        assert!(y_end - y_start <= self.size.rows as usize);

        for (dst_y, src_y) in (y_start..y_end).enumerate() {
            self.clone_plain_row_from(other, dst_y, src_y)?;
        }

        Ok(())
    }

    fn clone_plain_row_from(
        &mut self,
        other: &Page,
        dst_y: usize,
        src_y: usize,
    ) -> Result<(), CloneFromError> {
        let src_row = *other.get_row(src_y);
        let dst_row = *self.get_row(dst_y);
        if src_row.managed_memory() {
            return Err(CloneFromError::SourceRowManagedMemory);
        }
        if dst_row.managed_memory() {
            return Err(CloneFromError::DestinationRowManagedMemory);
        }

        let copy_len = (self.size.cols as usize).min(other.size.cols as usize);
        {
            let src_cells = other.get_cells(other.get_row(src_y));
            if src_cells[..copy_len]
                .iter()
                .any(|cell| cell.managed_memory())
            {
                return Err(CloneFromError::SourceCellManagedMemory);
            }
        }
        {
            let dst_cells = self.get_cells(self.get_row(dst_y));
            if dst_cells[..copy_len]
                .iter()
                .any(|cell| cell.managed_memory())
            {
                return Err(CloneFromError::DestinationCellManagedMemory);
            }
        }

        let mut row_copy = src_row;
        row_copy.set_cells(dst_row.cells());

        if copy_len < self.size.cols as usize {
            row_copy.set_wrap(dst_row.wrap());
            row_copy.set_wrap_continuation(dst_row.wrap_continuation());
            row_copy.set_grapheme(dst_row.grapheme());
            row_copy.set_hyperlink(dst_row.hyperlink());
            row_copy.set_styled(dst_row.styled());
            row_copy.set_dirty(src_row.dirty() || dst_row.dirty());
        }

        *self.get_row_mut(dst_y) = row_copy;

        let should_clear_spacer_head = self.size.cols > other.size.cols && copy_len > 0;
        let src_cells = other.get_cells(other.get_row(src_y));
        let cells = self.get_cells_mut(dst_y);
        cells[..copy_len].copy_from_slice(&src_cells[..copy_len]);

        if should_clear_spacer_head {
            let last = &mut cells[copy_len - 1];
            if last.wide() == Wide::SpacerHead {
                last.set_wide(Wide::Narrow);
            }
        }

        Ok(())
    }

    pub(super) fn get_row(&self, y: usize) -> &Row {
        assert!(y < self.size.rows as usize);
        unsafe {
            // Safety: bounds were checked and rows points into live backing
            // memory initialized during Page::init.
            &*self.rows.ptr(self.memory.as_ptr()).add(y)
        }
    }

    pub(super) fn get_row_mut(&mut self, y: usize) -> &mut Row {
        assert!(y < self.size.rows as usize);
        unsafe {
            // Safety: bounds were checked, `&mut self` guarantees no competing
            // mutable page access, and rows points into live backing memory.
            &mut *self.rows.ptr_mut(self.memory.as_mut_ptr()).add(y)
        }
    }

    pub(super) fn get_cells(&self, row: &Row) -> &[Cell] {
        self.assert_row_provenance(row);
        self.assert_row_cells_range(row);
        unsafe {
            // Safety: row provenance was checked and row.cells was initialized
            // to a cells-region offset whose row-sized range is valid.
            std::slice::from_raw_parts(
                row.cells().ptr(self.memory.as_ptr()),
                self.size.cols as usize,
            )
        }
    }

    pub(super) fn get_cells_mut(&mut self, row_index: usize) -> &mut [Cell] {
        assert!(row_index < self.size.rows as usize);
        let cells = self.get_row(row_index).cells();
        self.assert_cells_range(cells);
        unsafe {
            // Safety: row_index was checked, cells offset was initialized for
            // this page and range-checked, and `&mut self` guarantees
            // exclusive mutable access.
            std::slice::from_raw_parts_mut(
                cells.ptr_mut(self.memory.as_mut_ptr()),
                self.size.cols as usize,
            )
        }
    }

    pub(super) fn get_row_and_cell_mut(&mut self, x: usize, y: usize) -> RowAndCellMut<'_> {
        assert!(y < self.size.rows as usize);
        assert!(x < self.size.cols as usize);

        unsafe {
            // Safety: bounds were checked. Row and cell arrays occupy disjoint
            // regions in one live page allocation. The row's cells offset is
            // range-checked before creating the cell reference.
            let row = &mut *self.rows.ptr_mut(self.memory.as_mut_ptr()).add(y);
            Self::assert_cells_range_for_layout(self.layout, self.size, row.cells());
            let cell = &mut *row.cells().ptr_mut(self.memory.as_mut_ptr()).add(x);
            RowAndCellMut { row, cell }
        }
    }

    pub(super) fn append_grapheme_at(
        &mut self,
        x: usize,
        y: usize,
        cp: u32,
    ) -> Result<(), GraphemeError> {
        assert!(cp <= 0x10ffff);
        let cell = self.cell_copy_at(x, y);
        assert!(cell.codepoint() != 0);

        if !cell.has_grapheme() {
            return self.append_first_grapheme_at(x, y, cp);
        }

        let cell_offset = self.cell_offset_at(x, y);
        let slice = self
            .grapheme_map_ref()
            .and_then(|map| map.get(cell_offset))
            .expect("grapheme cell must have map data");

        if slice.len() % GRAPHEME_CHUNK_LEN != 0 {
            unsafe {
                // Safety: `slice` came from this page's grapheme map and has
                // spare capacity inside its allocated chunk.
                *slice
                    .offset()
                    .ptr_mut(self.memory.as_mut_ptr())
                    .add(slice.len()) = cp;
            }
            let updated = OffsetSlice::new(slice.offset(), slice.len() + 1);
            {
                let mut map = self.grapheme_map_mut().expect("grapheme map must exist");
                *map.get_mut(cell_offset)
                    .expect("grapheme cell must have map data") = updated;
            }
            return Ok(());
        }

        let old_values = unsafe {
            // Safety: `slice` came from this page's grapheme map.
            slice.slice(self.memory.as_ptr()).to_vec()
        };
        let new_slice = self.alloc_grapheme_slice(slice.len() + 1)?;
        unsafe {
            // Safety: `new_slice` was just allocated from this page and is
            // uniquely owned until inserted into the map below.
            let cps = new_slice.slice_mut(self.memory.as_mut_ptr());
            cps[..old_values.len()].copy_from_slice(&old_values);
            cps[old_values.len()] = cp;
        }

        {
            let mut map = self.grapheme_map_mut().expect("grapheme map must exist");
            *map.get_mut(cell_offset)
                .expect("grapheme cell must have map data") = new_slice;
        }

        self.free_grapheme_slice(slice);
        Ok(())
    }

    pub(super) fn lookup_grapheme_at(&self, x: usize, y: usize) -> Option<Vec<u32>> {
        let cell_offset = self.cell_offset_at(x, y);
        let slice = self.grapheme_map_ref()?.get(cell_offset)?;
        Some(unsafe {
            // Safety: `slice` came from this page's grapheme map.
            slice.slice(self.memory.as_ptr()).to_vec()
        })
    }

    pub(super) fn clear_grapheme_at(&mut self, x: usize, y: usize) {
        assert!(self.cell_copy_at(x, y).has_grapheme());
        let cell_offset = self.cell_offset_at(x, y);
        let slice = {
            let mut map = self
                .grapheme_map_mut()
                .expect("grapheme cell must have map storage");
            let (_, slice) = map
                .fetch_remove(cell_offset)
                .expect("grapheme cell must have map data");
            slice
        };

        self.free_grapheme_slice(slice);
        self.cell_mut_at(x, y)
            .set_content_tag(ContentTag::Codepoint);
    }

    pub(super) fn update_row_grapheme_flag(&mut self, row_index: usize) {
        let has_grapheme = self
            .get_cells(self.get_row(row_index))
            .iter()
            .any(|cell| cell.has_grapheme());
        if !has_grapheme {
            self.get_row_mut(row_index).set_grapheme(false);
        }
    }

    pub(super) fn grapheme_count(&self) -> usize {
        self.grapheme_map_ref()
            .map(|map| map.count() as usize)
            .unwrap_or(0)
    }

    pub(super) fn grapheme_capacity(&self) -> usize {
        self.grapheme_map_ref()
            .map(|map| map.capacity() as usize)
            .unwrap_or(0)
    }

    pub(super) fn add_style(
        &mut self,
        style: style::Style,
    ) -> Result<style::Id, super::ref_counted_set::AddError> {
        let base = unsafe {
            // Safety: styles_start is inside this page's live backing memory.
            self.memory.as_mut_ptr().add(self.layout.styles_start)
        };
        self.styles.add(base, style)
    }

    pub(super) fn get_style(&self, id: style::Id) -> style::Style {
        self.styles.get(self.style_base(), id)
    }

    pub(super) fn use_style(&self, id: style::Id) {
        self.styles.use_one(self.style_base(), id);
    }

    pub(super) fn release_style(&mut self, id: style::Id) {
        let base = unsafe {
            // Safety: styles_start is inside this page's live backing memory.
            self.memory.as_mut_ptr().add(self.layout.styles_start)
        };
        self.styles.release(base, id);
    }

    pub(super) fn style_ref_count(&self, id: style::Id) -> style::Id {
        self.styles.ref_count(self.style_base(), id)
    }

    pub(super) fn style_count(&self) -> usize {
        self.styles.count()
    }

    #[cfg(test)]
    fn grapheme_used_bytes(&self) -> usize {
        unsafe {
            // Safety: this page initialized grapheme_alloc with its backing
            // memory and the allocation is still live.
            self.grapheme_alloc.used_bytes(self.memory.as_ptr())
        }
    }

    fn append_first_grapheme_at(
        &mut self,
        x: usize,
        y: usize,
        cp: u32,
    ) -> Result<(), GraphemeError> {
        let Some(_) = self.grapheme_map else {
            return Err(GraphemeError::GraphemeMapOutOfMemory);
        };

        let cell_offset = self.cell_offset_at(x, y);
        let slice = self.alloc_grapheme_slice(1)?;
        unsafe {
            // Safety: `slice` was just allocated from this page and is uniquely
            // owned until inserted into the map below.
            slice.offset().ptr_mut(self.memory.as_mut_ptr()).write(cp);
        }

        let inserted = {
            let mut map = self.grapheme_map_mut().expect("grapheme map must exist");
            map.put_no_clobber(cell_offset, slice)
        };

        if inserted.is_err() {
            self.free_grapheme_slice(slice);
            return Err(GraphemeError::GraphemeMapOutOfMemory);
        }

        self.cell_mut_at(x, y)
            .set_content_tag(ContentTag::CodepointGrapheme);
        self.get_row_mut(y).set_grapheme(true);
        Ok(())
    }

    fn alloc_grapheme_slice(&mut self, len: usize) -> Result<OffsetSlice<u32>, GraphemeError> {
        let slice = unsafe {
            // Safety: this page initialized grapheme_alloc with its backing
            // memory and the allocation is uniquely borrowed through &mut self.
            self.grapheme_alloc
                .alloc::<u32, _>(self.memory.as_mut_ptr(), len)
        }
        .map_err(|_| GraphemeError::GraphemeAllocOutOfMemory)?;
        let offset = get_offset(self.memory.as_ptr(), slice.as_ptr());
        Ok(OffsetSlice::new(offset, slice.len()))
    }

    fn free_grapheme_slice(&mut self, slice: OffsetSlice<u32>) {
        let slice = unsafe {
            // Safety: `slice` came from this page's grapheme allocator and is
            // no longer referenced by the grapheme map at call sites.
            slice.slice_mut(self.memory.as_mut_ptr())
        };
        unsafe {
            // Safety: the slice was allocated by this allocator from this page
            // backing memory and is being freed exactly once.
            self.grapheme_alloc.free(self.memory.as_mut_ptr(), slice);
        }
    }

    fn grapheme_map_ref(
        &self,
    ) -> Option<offset_hash_map::MapRef<'_, Offset<Cell>, OffsetSlice<u32>>> {
        self.grapheme_map
            .as_ref()
            .map(|map| map.map_ref(self.memory.as_slice()))
    }

    fn grapheme_map_mut(
        &mut self,
    ) -> Option<offset_hash_map::Map<'_, Offset<Cell>, OffsetSlice<u32>>> {
        let map = self.grapheme_map?;
        Some(map.map(self.memory.as_mut_slice()))
    }

    fn style_base(&self) -> *const u8 {
        unsafe {
            // Safety: styles_start is produced by Page layout and is inside
            // this page's live backing memory.
            self.memory.as_ptr().add(self.layout.styles_start)
        }
    }

    fn cell_offset_at(&self, x: usize, y: usize) -> Offset<Cell> {
        assert!(y < self.size.rows as usize);
        assert!(x < self.size.cols as usize);
        let index = y * self.size.cols as usize + x;
        Offset::new(
            (self.layout.cells_start + index * size_of::<Cell>())
                .try_into()
                .expect("cell offset must fit OffsetInt"),
        )
    }

    fn cell_copy_at(&self, x: usize, y: usize) -> Cell {
        unsafe {
            // Safety: bounds are checked by cell_offset_at.
            *self.cell_offset_at(x, y).ptr(self.memory.as_ptr())
        }
    }

    fn cell_mut_at(&mut self, x: usize, y: usize) -> &mut Cell {
        unsafe {
            // Safety: bounds are checked by cell_offset_at, and &mut self
            // guarantees exclusive access.
            &mut *self.cell_offset_at(x, y).ptr_mut(self.memory.as_mut_ptr())
        }
    }

    fn assert_row_provenance(&self, row: &Row) {
        let row_addr = row as *const Row as usize;
        let rows_start = self.rows.ptr(self.memory.as_ptr()) as usize;
        let rows_len = self.size.rows as usize * size_of::<Row>();
        let rows_end = rows_start + rows_len;

        assert!(row_addr >= rows_start);
        assert!(row_addr < rows_end);
        assert_eq!((row_addr - rows_start) % size_of::<Row>(), 0);
    }

    fn assert_row_cells_range(&self, row: &Row) {
        self.assert_cells_range(row.cells());
    }

    fn assert_cells_range(&self, cells: Offset<Cell>) {
        Self::assert_cells_range_for_layout(self.layout, self.size, cells);
    }

    fn assert_cells_range_for_layout(layout: PageLayout, size: Size, cells: Offset<Cell>) {
        let offset = cells.offset() as usize;
        let row_bytes = size.cols as usize * size_of::<Cell>();
        let cells_start = layout.cells_start;
        let cells_end = layout.cells_start + layout.cells_size;

        assert!(
            offset >= cells_start,
            "row cell offset is before cells region"
        );
        assert!(
            offset
                .checked_add(row_bytes)
                .is_some_and(|end| end <= cells_end),
            "row cell range is outside cells region"
        );
        assert_eq!(
            (offset - cells_start) % size_of::<Cell>(),
            0,
            "row cell offset is not cell aligned"
        );
    }
}

pub(super) struct RowAndCellMut<'a> {
    pub(super) row: &'a mut Row,
    pub(super) cell: &'a mut Cell,
}

#[derive(Debug)]
struct PageMemory {
    ptr: NonNull<u8>,
    len: usize,
}

impl PageMemory {
    fn new(len: usize) -> Result<Self, PageAllocError> {
        assert!(len > 0);
        assert_eq!(len % PAGE_SIZE_MIN, 0);

        let ptr = unsafe {
            // Safety: mmap is called with a null address hint, a non-zero
            // length, read/write protection, anonymous private mapping, and no
            // file descriptor. Return value is checked against MAP_FAILED.
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON,
                -1,
                0,
            )
        };

        if ptr == libc::MAP_FAILED {
            return Err(PageAllocError::MmapFailed(io::Error::last_os_error()));
        }

        let ptr = NonNull::new(ptr.cast::<u8>()).ok_or_else(|| {
            PageAllocError::MmapFailed(io::Error::new(io::ErrorKind::Other, "mmap returned null"))
        })?;
        Ok(Self { ptr, len })
    }

    fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    fn len(&self) -> usize {
        self.len
    }

    fn as_slice(&self) -> &[u8] {
        unsafe {
            // Safety: PageMemory owns a live mmap allocation for `len` bytes.
            std::slice::from_raw_parts(self.as_ptr(), self.len)
        }
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe {
            // Safety: PageMemory owns a live mmap allocation for `len` bytes
            // and &mut self guarantees unique access to it.
            std::slice::from_raw_parts_mut(self.as_mut_ptr(), self.len)
        }
    }
}

impl Drop for PageMemory {
    fn drop(&mut self) {
        let result = unsafe {
            // Safety: PageMemory only stores successful mmap mappings and Drop
            // runs once for this owner, passing the original pointer/length.
            libc::munmap(self.ptr.as_ptr().cast(), self.len)
        };
        debug_assert_eq!(result, 0);
    }
}

#[derive(Debug)]
pub(super) enum PageAllocError {
    MmapFailed(io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StyleSetLayout {
    cap: usize,
    table_cap: usize,
    table_mask: u16,
    table_start: usize,
    items_start: usize,
    total_size: usize,
}

impl StyleSetLayout {
    const BASE_ALIGN: usize = style::Set::BASE_ALIGN;

    fn init(capacity: usize) -> Self {
        style::Set::layout(capacity).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HyperlinkSetLayout {
    cap: usize,
    table_cap: usize,
    table_mask: u16,
    table_start: usize,
    items_start: usize,
    total_size: usize,
}

impl HyperlinkSetLayout {
    const BASE_ALIGN: usize = 8;

    fn init(capacity: usize) -> Self {
        ref_counted_set_layout(capacity, HYPERLINK_SET_ITEM_SIZE, HYPERLINK_SET_ITEM_ALIGN).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RefCountedSetLayout {
    cap: usize,
    table_cap: usize,
    table_mask: u16,
    table_start: usize,
    items_start: usize,
    total_size: usize,
}

impl From<RefCountedSetLayout> for StyleSetLayout {
    fn from(value: RefCountedSetLayout) -> Self {
        Self {
            cap: value.cap,
            table_cap: value.table_cap,
            table_mask: value.table_mask,
            table_start: value.table_start,
            items_start: value.items_start,
            total_size: value.total_size,
        }
    }
}

impl From<super::ref_counted_set::Layout> for StyleSetLayout {
    fn from(value: super::ref_counted_set::Layout) -> Self {
        Self {
            cap: value.cap,
            table_cap: value.table_cap,
            table_mask: value.table_mask,
            table_start: value.table_start,
            items_start: value.items_start,
            total_size: value.total_size,
        }
    }
}

impl From<StyleSetLayout> for super::ref_counted_set::Layout {
    fn from(value: StyleSetLayout) -> Self {
        Self {
            cap: value.cap,
            table_cap: value.table_cap,
            table_mask: value.table_mask,
            table_start: value.table_start,
            items_start: value.items_start,
            total_size: value.total_size,
        }
    }
}

impl From<RefCountedSetLayout> for HyperlinkSetLayout {
    fn from(value: RefCountedSetLayout) -> Self {
        Self {
            cap: value.cap,
            table_cap: value.table_cap,
            table_mask: value.table_mask,
            table_start: value.table_start,
            items_start: value.items_start,
            total_size: value.total_size,
        }
    }
}

fn ref_counted_set_layout(
    capacity: usize,
    item_size: usize,
    item_align: usize,
) -> RefCountedSetLayout {
    assert!(capacity <= u16::MAX as usize + 1);

    if capacity == 0 {
        return RefCountedSetLayout {
            cap: 0,
            table_cap: 0,
            table_mask: 0,
            table_start: 0,
            items_start: 0,
            total_size: 0,
        };
    }

    let table_cap = capacity.next_power_of_two();
    let items_cap = ((table_cap as f64) * 0.8125).floor() as usize;
    let table_mask = (table_cap - 1) as u16;
    let table_start = 0;
    let table_end = table_start + table_cap * REF_COUNTED_SET_ID_SIZE;
    let items_start = align_forward(table_end, item_align);
    let items_end = items_start + items_cap * item_size;
    let total_size = items_end;

    RefCountedSetLayout {
        cap: items_cap,
        table_cap,
        table_mask,
        table_start,
        items_start,
        total_size,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct GraphemeMapLayout {
    total_size: usize,
    keys_start: usize,
    vals_start: usize,
    capacity: u32,
}

impl GraphemeMapLayout {
    const BASE_ALIGN: usize =
        offset_hash_map::OffsetHashMap::<Offset<Cell>, OffsetSlice<u32>>::BASE_ALIGN;

    fn layout(capacity: u32) -> Self {
        offset_hash_map::layout_for_capacity::<Offset<Cell>, OffsetSlice<u32>>(capacity).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HyperlinkMapLayout {
    total_size: usize,
    keys_start: usize,
    vals_start: usize,
    capacity: u32,
}

impl HyperlinkMapLayout {
    const BASE_ALIGN: usize =
        offset_hash_map::OffsetHashMap::<Offset<Cell>, HyperlinkId>::BASE_ALIGN;

    fn layout(capacity: u32) -> Self {
        offset_hash_map::layout_for_capacity::<Offset<Cell>, HyperlinkId>(capacity).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HashMapLayout {
    total_size: usize,
    keys_start: usize,
    vals_start: usize,
    capacity: u32,
}

impl From<offset_hash_map::Layout> for GraphemeMapLayout {
    fn from(value: offset_hash_map::Layout) -> Self {
        Self {
            total_size: value.total_size,
            keys_start: value.keys_start,
            vals_start: value.vals_start,
            capacity: value.capacity,
        }
    }
}

impl From<GraphemeMapLayout> for offset_hash_map::Layout {
    fn from(value: GraphemeMapLayout) -> Self {
        Self {
            total_size: value.total_size,
            keys_start: value.keys_start,
            vals_start: value.vals_start,
            capacity: value.capacity,
        }
    }
}

impl From<offset_hash_map::Layout> for HyperlinkMapLayout {
    fn from(value: offset_hash_map::Layout) -> Self {
        Self {
            total_size: value.total_size,
            keys_start: value.keys_start,
            vals_start: value.vals_start,
            capacity: value.capacity,
        }
    }
}

impl From<HyperlinkMapLayout> for offset_hash_map::Layout {
    fn from(value: HyperlinkMapLayout) -> Self {
        Self {
            total_size: value.total_size,
            keys_start: value.keys_start,
            vals_start: value.vals_start,
            capacity: value.capacity,
        }
    }
}

type HyperlinkId = u16;

const fn row_bits() -> usize {
    size_of::<Row>() * u8::BITS as usize
}

const fn cell_bits() -> usize {
    size_of::<Cell>() * u8::BITS as usize
}

fn div_ceil(value: usize, divisor: usize) -> usize {
    value.div_ceil(divisor)
}

fn align_forward(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (value + align - 1) & !(align - 1)
}

fn align_backward(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    value & !(align - 1)
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Row(u64);

impl Row {
    const CELLS_SHIFT: u32 = 0;
    const CELLS_MASK: u64 = u32::MAX as u64;
    const WRAP_SHIFT: u32 = 32;
    const WRAP_CONTINUATION_SHIFT: u32 = 33;
    const GRAPHEME_SHIFT: u32 = 34;
    const STYLED_SHIFT: u32 = 35;
    const HYPERLINK_SHIFT: u32 = 36;
    const SEMANTIC_PROMPT_SHIFT: u32 = 37;
    const SEMANTIC_PROMPT_MASK: u64 = 0b11;
    const KITTY_VIRTUAL_PLACEHOLDER_SHIFT: u32 = 39;
    const DIRTY_SHIFT: u32 = 40;

    pub(super) const fn cval(self) -> u64 {
        self.0
    }

    pub(super) const fn cells(self) -> Offset<Cell> {
        Offset::new(((self.0 >> Self::CELLS_SHIFT) & Self::CELLS_MASK) as u32)
    }

    pub(super) fn set_cells(&mut self, offset: Offset<Cell>) {
        self.set_bits(Self::CELLS_SHIFT, Self::CELLS_MASK, offset.offset() as u64);
    }

    pub(super) const fn wrap(self) -> bool {
        self.bit(Self::WRAP_SHIFT)
    }

    pub(super) fn set_wrap(&mut self, value: bool) {
        self.set_bit(Self::WRAP_SHIFT, value);
    }

    pub(super) const fn wrap_continuation(self) -> bool {
        self.bit(Self::WRAP_CONTINUATION_SHIFT)
    }

    pub(super) fn set_wrap_continuation(&mut self, value: bool) {
        self.set_bit(Self::WRAP_CONTINUATION_SHIFT, value);
    }

    pub(super) const fn grapheme(self) -> bool {
        self.bit(Self::GRAPHEME_SHIFT)
    }

    pub(super) fn set_grapheme(&mut self, value: bool) {
        self.set_bit(Self::GRAPHEME_SHIFT, value);
    }

    pub(super) const fn styled(self) -> bool {
        self.bit(Self::STYLED_SHIFT)
    }

    pub(super) fn set_styled(&mut self, value: bool) {
        self.set_bit(Self::STYLED_SHIFT, value);
    }

    pub(super) const fn hyperlink(self) -> bool {
        self.bit(Self::HYPERLINK_SHIFT)
    }

    pub(super) fn set_hyperlink(&mut self, value: bool) {
        self.set_bit(Self::HYPERLINK_SHIFT, value);
    }

    pub(super) const fn semantic_prompt(self) -> SemanticPrompt {
        SemanticPrompt::from_bits(
            ((self.0 >> Self::SEMANTIC_PROMPT_SHIFT) & Self::SEMANTIC_PROMPT_MASK) as u8,
        )
    }

    pub(super) fn set_semantic_prompt(&mut self, value: SemanticPrompt) {
        self.set_bits(
            Self::SEMANTIC_PROMPT_SHIFT,
            Self::SEMANTIC_PROMPT_MASK,
            value as u64,
        );
    }

    pub(super) const fn kitty_virtual_placeholder(self) -> bool {
        self.bit(Self::KITTY_VIRTUAL_PLACEHOLDER_SHIFT)
    }

    pub(super) fn set_kitty_virtual_placeholder(&mut self, value: bool) {
        self.set_bit(Self::KITTY_VIRTUAL_PLACEHOLDER_SHIFT, value);
    }

    pub(super) const fn dirty(self) -> bool {
        self.bit(Self::DIRTY_SHIFT)
    }

    pub(super) fn set_dirty(&mut self, value: bool) {
        self.set_bit(Self::DIRTY_SHIFT, value);
    }

    pub(super) const fn managed_memory(self) -> bool {
        self.styled() || self.hyperlink() || self.grapheme()
    }

    const fn bit(self, shift: u32) -> bool {
        ((self.0 >> shift) & 1) == 1
    }

    fn set_bit(&mut self, shift: u32, value: bool) {
        if value {
            self.0 |= 1_u64 << shift;
        } else {
            self.0 &= !(1_u64 << shift);
        }
    }

    fn set_bits(&mut self, shift: u32, mask: u64, value: u64) {
        assert_eq!(value & !mask, 0);
        self.0 = (self.0 & !(mask << shift)) | (value << shift);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum SemanticPrompt {
    #[default]
    None = 0,
    Prompt = 1,
    PromptContinuation = 2,
}

impl SemanticPrompt {
    const fn from_bits(bits: u8) -> Self {
        match bits {
            0 => Self::None,
            1 => Self::Prompt,
            2 => Self::PromptContinuation,
            _ => panic!("invalid semantic prompt bits"),
        }
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Cell(u64);

impl Cell {
    const CONTENT_TAG_SHIFT: u32 = 0;
    const CONTENT_TAG_MASK: u64 = 0b11;
    const CONTENT_SHIFT: u32 = 2;
    const CONTENT_MASK: u64 = 0x00ff_ffff;
    const CODEPOINT_MASK: u64 = 0x001f_ffff;
    const STYLE_ID_SHIFT: u32 = 26;
    const STYLE_ID_MASK: u64 = u16::MAX as u64;
    const WIDE_SHIFT: u32 = 42;
    const WIDE_MASK: u64 = 0b11;
    const PROTECTED_SHIFT: u32 = 44;
    const HYPERLINK_SHIFT: u32 = 45;
    const SEMANTIC_CONTENT_SHIFT: u32 = 46;
    const SEMANTIC_CONTENT_MASK: u64 = 0b11;

    pub(super) fn init(codepoint: u32) -> Self {
        assert!(codepoint <= 0x10ffff);

        let mut cell = Self::default();
        cell.set_content_tag(ContentTag::Codepoint);
        cell.set_content(codepoint as u64);
        cell
    }

    pub(super) fn bg_palette(index: u8) -> Self {
        let mut cell = Self::default();
        cell.set_content_tag(ContentTag::BgColorPalette);
        cell.set_content(index as u64);
        cell
    }

    pub(super) fn bg_rgb(rgb: Rgb) -> Self {
        let mut cell = Self::default();
        cell.set_content_tag(ContentTag::BgColorRgb);
        cell.set_content(rgb_to_bits(rgb));
        cell
    }

    pub(super) const fn cval(self) -> u64 {
        self.0
    }

    pub(super) const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub(super) const fn content_tag(self) -> ContentTag {
        ContentTag::from_bits(((self.0 >> Self::CONTENT_TAG_SHIFT) & Self::CONTENT_TAG_MASK) as u8)
    }

    pub(super) fn set_content_tag(&mut self, value: ContentTag) {
        self.set_bits(
            Self::CONTENT_TAG_SHIFT,
            Self::CONTENT_TAG_MASK,
            value as u64,
        );
    }

    pub(super) const fn codepoint(self) -> u32 {
        match self.content_tag() {
            ContentTag::Codepoint | ContentTag::CodepointGrapheme => {
                ((self.0 >> Self::CONTENT_SHIFT) & Self::CODEPOINT_MASK) as u32
            }
            ContentTag::BgColorPalette | ContentTag::BgColorRgb => 0,
        }
    }

    pub(super) const fn color_palette(self) -> u8 {
        ((self.0 >> Self::CONTENT_SHIFT) & 0xff) as u8
    }

    pub(super) const fn color_rgb(self) -> Rgb {
        bits_to_rgb((self.0 >> Self::CONTENT_SHIFT) & Self::CONTENT_MASK)
    }

    pub(super) const fn style_id(self) -> style::Id {
        ((self.0 >> Self::STYLE_ID_SHIFT) & Self::STYLE_ID_MASK) as style::Id
    }

    pub(super) fn set_style_id(&mut self, value: style::Id) {
        self.set_bits(Self::STYLE_ID_SHIFT, Self::STYLE_ID_MASK, value as u64);
    }

    pub(super) const fn wide(self) -> Wide {
        Wide::from_bits(((self.0 >> Self::WIDE_SHIFT) & Self::WIDE_MASK) as u8)
    }

    pub(super) fn set_wide(&mut self, value: Wide) {
        self.set_bits(Self::WIDE_SHIFT, Self::WIDE_MASK, value as u64);
    }

    pub(super) const fn protected(self) -> bool {
        self.bit(Self::PROTECTED_SHIFT)
    }

    pub(super) fn set_protected(&mut self, value: bool) {
        self.set_bit(Self::PROTECTED_SHIFT, value);
    }

    pub(super) const fn hyperlink(self) -> bool {
        self.bit(Self::HYPERLINK_SHIFT)
    }

    pub(super) fn set_hyperlink(&mut self, value: bool) {
        self.set_bit(Self::HYPERLINK_SHIFT, value);
    }

    pub(super) const fn semantic_content(self) -> SemanticContent {
        SemanticContent::from_bits(
            ((self.0 >> Self::SEMANTIC_CONTENT_SHIFT) & Self::SEMANTIC_CONTENT_MASK) as u8,
        )
    }

    pub(super) fn set_semantic_content(&mut self, value: SemanticContent) {
        self.set_bits(
            Self::SEMANTIC_CONTENT_SHIFT,
            Self::SEMANTIC_CONTENT_MASK,
            value as u64,
        );
    }

    pub(super) const fn has_text(self) -> bool {
        match self.content_tag() {
            ContentTag::Codepoint | ContentTag::CodepointGrapheme => self.codepoint() != 0,
            ContentTag::BgColorPalette | ContentTag::BgColorRgb => false,
        }
    }

    pub(super) const fn grid_width(self) -> u8 {
        match self.wide() {
            Wide::Narrow | Wide::SpacerHead | Wide::SpacerTail => 1,
            Wide::Wide => 2,
        }
    }

    pub(super) const fn has_styling(self) -> bool {
        self.style_id() != style::DEFAULT_ID
    }

    pub(super) const fn is_empty(self) -> bool {
        match self.content_tag() {
            ContentTag::Codepoint | ContentTag::CodepointGrapheme => {
                !self.has_text() && matches!(self.wide(), Wide::Narrow)
            }
            ContentTag::BgColorPalette | ContentTag::BgColorRgb => false,
        }
    }

    pub(super) const fn has_grapheme(self) -> bool {
        matches!(self.content_tag(), ContentTag::CodepointGrapheme)
    }

    const fn managed_memory(self) -> bool {
        self.has_grapheme() || self.has_styling() || self.hyperlink()
    }

    pub(super) fn has_text_any(cells: &[Cell]) -> bool {
        cells.iter().any(|cell| cell.has_text())
    }

    fn set_content(&mut self, value: u64) {
        self.set_bits(Self::CONTENT_SHIFT, Self::CONTENT_MASK, value);
    }

    const fn bit(self, shift: u32) -> bool {
        ((self.0 >> shift) & 1) == 1
    }

    fn set_bit(&mut self, shift: u32, value: bool) {
        if value {
            self.0 |= 1_u64 << shift;
        } else {
            self.0 &= !(1_u64 << shift);
        }
    }

    fn set_bits(&mut self, shift: u32, mask: u64, value: u64) {
        assert_eq!(value & !mask, 0);
        self.0 = (self.0 & !(mask << shift)) | (value << shift);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum ContentTag {
    #[default]
    Codepoint = 0,
    CodepointGrapheme = 1,
    BgColorPalette = 2,
    BgColorRgb = 3,
}

impl ContentTag {
    const fn from_bits(bits: u8) -> Self {
        match bits {
            0 => Self::Codepoint,
            1 => Self::CodepointGrapheme,
            2 => Self::BgColorPalette,
            3 => Self::BgColorRgb,
            _ => panic!("invalid content tag bits"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Wide {
    #[default]
    Narrow = 0,
    Wide = 1,
    SpacerTail = 2,
    SpacerHead = 3,
}

impl Wide {
    const fn from_bits(bits: u8) -> Self {
        match bits {
            0 => Self::Narrow,
            1 => Self::Wide,
            2 => Self::SpacerTail,
            3 => Self::SpacerHead,
            _ => panic!("invalid wide bits"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum SemanticContent {
    #[default]
    Output = 0,
    Input = 1,
    Prompt = 2,
}

impl SemanticContent {
    const fn from_bits(bits: u8) -> Self {
        match bits {
            0 => Self::Output,
            1 => Self::Input,
            2 => Self::Prompt,
            _ => panic!("invalid semantic content bits"),
        }
    }
}

const fn rgb_to_bits(rgb: Rgb) -> u64 {
    rgb.r as u64 | ((rgb.g as u64) << 8) | ((rgb.b as u64) << 16)
}

const fn bits_to_rgb(bits: u64) -> Rgb {
    Rgb::new(bits as u8, (bits >> 8) as u8, (bits >> 16) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, size_of};

    #[test]
    fn upstream_layout_surrogate_sizes() {
        assert_eq!(PAGE_SIZE_MIN, 16_384);
        assert_eq!(CODEPOINT_STORAGE_SIZE, 4);
        assert_eq!(STYLE_VALUE_SIZE, 28);
        assert_eq!(STYLE_VALUE_ALIGN, 4);
        assert_eq!(STYLE_SET_ITEM_SIZE, 36);
        assert_eq!(STYLE_SET_ITEM_ALIGN, 4);
        assert_eq!(HYPERLINK_PAGE_ENTRY_SIZE, 40);
        assert_eq!(HYPERLINK_PAGE_ENTRY_ALIGN, 8);
        assert_eq!(HYPERLINK_SET_ITEM_SIZE, 48);
        assert_eq!(HYPERLINK_SET_ITEM_ALIGN, 8);
        assert_eq!(REF_COUNTED_SET_ID_SIZE, 2);
        assert_eq!(REF_COUNTED_SET_ID_ALIGN, 2);
        assert_eq!(HASH_MAP_HEADER_SIZE, 16);
        assert_eq!(HASH_MAP_HEADER_ALIGN, 4);
        assert_eq!(HASH_MAP_METADATA_SIZE, 1);
        assert_eq!(HASH_MAP_METADATA_ALIGN, 1);
        assert_eq!(OFFSET_CELL_SIZE, 4);
        assert_eq!(OFFSET_CELL_ALIGN, 4);
        assert_eq!(OFFSET_U21_SLICE_SIZE, 16);
        assert_eq!(OFFSET_U21_SLICE_ALIGN, 8);
        assert_eq!(HYPERLINK_ID_SIZE, 2);
        assert_eq!(HYPERLINK_ID_ALIGN, 2);
    }

    #[test]
    fn dependency_layout_parity() {
        assert_eq!(
            StyleSetLayout::init(0),
            StyleSetLayout {
                cap: 0,
                table_cap: 0,
                table_mask: 0,
                table_start: 0,
                items_start: 0,
                total_size: 0,
            }
        );
        assert_eq!(
            StyleSetLayout::init(16),
            StyleSetLayout {
                cap: 13,
                table_cap: 16,
                table_mask: 15,
                table_start: 0,
                items_start: 32,
                total_size: 500,
            }
        );

        assert_eq!(
            GraphemeMapLayout::layout(0),
            GraphemeMapLayout {
                total_size: 16,
                keys_start: 0,
                vals_start: 0,
                capacity: 0,
            }
        );
        assert_eq!(
            GraphemeMapLayout::layout(32),
            GraphemeMapLayout {
                total_size: 688,
                keys_start: 32,
                vals_start: 160,
                capacity: 32,
            }
        );

        assert_eq!(
            HyperlinkMapLayout::layout(0),
            HyperlinkMapLayout {
                total_size: 16,
                keys_start: 0,
                vals_start: 0,
                capacity: 0,
            }
        );
        assert_eq!(
            HyperlinkMapLayout::layout(64),
            HyperlinkMapLayout {
                total_size: 464,
                keys_start: 64,
                vals_start: 320,
                capacity: 64,
            }
        );

        assert_eq!(
            HyperlinkSetLayout::init(0),
            HyperlinkSetLayout {
                cap: 0,
                table_cap: 0,
                table_mask: 0,
                table_start: 0,
                items_start: 0,
                total_size: 0,
            }
        );
        assert_eq!(
            HyperlinkSetLayout::init(4),
            HyperlinkSetLayout {
                cap: 3,
                table_cap: 4,
                table_mask: 3,
                table_start: 0,
                items_start: 8,
                total_size: 152,
            }
        );
    }

    #[test]
    fn bitmap_layout_values_used_by_page_layout() {
        assert_eq!(GraphemeAlloc::layout(512).total_size, 8200);
        assert_eq!(
            StringAlloc::layout(STRING_BYTES_DEFAULT as usize).total_size,
            65_544
        );
    }

    #[test]
    fn page_layout_std_capacity() {
        let layout = page_layout(STD_CAPACITY);

        assert_eq!(layout.total_size, 458_752);
        assert_eq!(layout.rows_start, 0);
        assert_eq!(layout.rows_size, 1_720);
        assert_eq!(layout.cells_start, 1_720);
        assert_eq!(layout.cells_size, 369_800);
        assert_eq!(layout.styles_start, 371_520);
        assert_eq!(layout.styles_layout.total_size, 4_000);
        assert_eq!(layout.grapheme_alloc_start, 375_520);
        assert_eq!(layout.grapheme_alloc_layout.total_size, 8_200);
        assert_eq!(layout.grapheme_map_start, 383_720);
        assert_eq!(layout.grapheme_map_layout.total_size, 688);
        assert_eq!(layout.string_alloc_start, 384_408);
        assert_eq!(layout.string_alloc_layout.total_size, 65_544);
        assert_eq!(layout.hyperlink_set_start, 449_952);
        assert_eq!(layout.hyperlink_set_layout.total_size, 152);
        assert_eq!(layout.hyperlink_map_start, 450_104);
        assert_eq!(layout.hyperlink_map_layout.total_size, 464);
    }

    #[test]
    fn page_layout_ordering() {
        let layout = page_layout(STD_CAPACITY);

        assert!(layout.rows_start < layout.cells_start);
        assert!(layout.cells_start < layout.styles_start);
        assert!(layout.styles_start < layout.grapheme_alloc_start);
        assert!(layout.grapheme_alloc_start < layout.grapheme_map_start);
        assert!(layout.grapheme_map_start < layout.string_alloc_start);
        assert!(layout.string_alloc_start < layout.hyperlink_set_start);
        assert!(layout.hyperlink_set_start < layout.hyperlink_map_start);
        assert_eq!(layout.total_size % PAGE_SIZE_MIN, 0);
    }

    #[test]
    fn page_memory_is_zeroed_and_aligned() {
        let memory = PageMemory::new(PAGE_SIZE_MIN).unwrap();

        assert_eq!(memory.len(), PAGE_SIZE_MIN);
        assert_eq!(memory.as_ptr() as usize % PAGE_SIZE_MIN, 0);
        assert!(memory.as_slice().iter().all(|byte| *byte == 0));
    }

    #[test]
    fn page_init() {
        let page = Page::init(Capacity {
            cols: 120,
            rows: 80,
            styles: 32,
            ..Capacity::new(120, 80)
        })
        .unwrap();

        assert_eq!(page.backing_len(), page.layout.total_size);
        assert_eq!(page.backing_ptr() as usize % PAGE_SIZE_MIN, 0);
        assert!(!page.is_dirty());
        assert_eq!(page.capacity().cols, 120);
        assert_eq!(page.capacity().rows, 80);
        assert_eq!(page.size.cols, 120);
        assert_eq!(page.size.rows, 80);
    }

    #[test]
    fn page_rows_point_to_expected_cell_ranges() {
        let page = Page::init(Capacity {
            cols: 10,
            rows: 10,
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        for y in 0..page.capacity.rows as usize {
            let row = page.get_row(y);
            let expected =
                page.layout.cells_start + y * page.capacity.cols as usize * size_of::<Cell>();
            assert_eq!(row.cells().offset() as usize, expected);
            assert_eq!(page.get_cells(row).len(), page.size.cols as usize);
        }
    }

    #[test]
    fn page_read_and_write_cells() {
        let mut page = Page::init(Capacity {
            cols: 10,
            rows: 10,
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        for y in 0..page.capacity.rows as usize {
            let rac = page.get_row_and_cell_mut(1, y);
            *rac.cell = Cell::init(y as u32);
        }

        for y in 0..page.capacity.rows as usize {
            let row = page.get_row(y);
            let cells = page.get_cells(row);
            assert_eq!(cells[1].codepoint(), y as u32);
        }
    }

    #[test]
    fn page_get_cells_mut() {
        let mut page = Page::init(Capacity::new(3, 2)).unwrap();

        let cells = page.get_cells_mut(1);
        cells[2] = Cell::init('z' as u32);

        assert_eq!(page.get_row_and_cell_mut(2, 1).cell.codepoint(), 'z' as u32);
    }

    #[test]
    fn page_append_grapheme_small() {
        let mut page = Page::init(Capacity {
            cols: 10,
            rows: 10,
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init(0x09);

        page.append_grapheme_at(0, 0, 0x0a).unwrap();
        assert!(page.get_row(0).grapheme());
        assert!(page.cell_copy_at(0, 0).has_grapheme());
        assert_eq!(page.lookup_grapheme_at(0, 0).unwrap(), vec![0x0a]);
        assert_eq!(page.grapheme_count(), 1);

        page.append_grapheme_at(0, 0, 0x0b).unwrap();
        assert!(page.get_row(0).grapheme());
        assert!(page.cell_copy_at(0, 0).has_grapheme());
        assert_eq!(page.lookup_grapheme_at(0, 0).unwrap(), vec![0x0a, 0x0b]);

        page.clear_grapheme_at(0, 0);
        page.update_row_grapheme_flag(0);
        assert!(!page.get_row(0).grapheme());
        assert!(!page.cell_copy_at(0, 0).has_grapheme());
        assert_eq!(page.lookup_grapheme_at(0, 0), None);
        assert_eq!(page.grapheme_count(), 0);
    }

    #[test]
    fn page_append_grapheme_larger_than_chunk() {
        let mut page = Page::init(Capacity {
            cols: 10,
            rows: 10,
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init(0x09);

        let count = GRAPHEME_CHUNK_LEN * 10;
        for i in 0..count {
            page.append_grapheme_at(0, 0, 0x0a + i as u32).unwrap();
        }

        let cps = page.lookup_grapheme_at(0, 0).unwrap();
        assert_eq!(cps.len(), count);
        for (i, cp) in cps.iter().enumerate() {
            assert_eq!(*cp, 0x0a + i as u32);
        }
    }

    #[test]
    fn page_clear_grapheme_not_all_cells() {
        let mut page = Page::init(Capacity {
            cols: 10,
            rows: 10,
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init(0x09);
        page.append_grapheme_at(0, 0, 0x0a).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init(0x09);
        page.append_grapheme_at(1, 0, 0x0a).unwrap();

        page.clear_grapheme_at(0, 0);
        page.update_row_grapheme_flag(0);
        assert!(page.get_row(0).grapheme());
        assert!(!page.cell_copy_at(0, 0).has_grapheme());
        assert!(page.cell_copy_at(1, 0).has_grapheme());
    }

    #[test]
    fn page_grapheme_lookup_excludes_cell_codepoint() {
        let mut page = Page::init(Capacity::new(5, 5)).unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);

        page.append_grapheme_at(0, 0, 0x0301).unwrap();

        assert_eq!(page.cell_copy_at(0, 0).codepoint(), 'a' as u32);
        assert_eq!(page.lookup_grapheme_at(0, 0), Some(vec![0x0301]));
    }

    #[test]
    fn page_grapheme_count_and_capacity() {
        let mut page = Page::init(Capacity::new(5, 5)).unwrap();
        assert_eq!(page.grapheme_count(), 0);
        assert!(page.grapheme_capacity() > 0);

        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);
        page.append_grapheme_at(0, 0, 0x0301).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('b' as u32);
        page.append_grapheme_at(1, 0, 0x0300).unwrap();

        assert_eq!(page.grapheme_count(), 2);
        assert!(page.grapheme_capacity() >= 2);
    }

    #[test]
    fn page_zero_capacity_grapheme_map_fails_without_flags() {
        let mut page = Page::init(Capacity::with_metadata(
            5,
            5,
            8,
            HYPERLINK_BYTES_DEFAULT,
            0,
            STRING_BYTES_DEFAULT,
        ))
        .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);

        let before_used = page.grapheme_used_bytes();
        assert_eq!(
            page.append_grapheme_at(0, 0, 0x0301),
            Err(GraphemeError::GraphemeMapOutOfMemory)
        );

        assert_eq!(page.lookup_grapheme_at(0, 0), None);
        assert_eq!(page.grapheme_count(), 0);
        assert_eq!(page.grapheme_capacity(), 0);
        assert_eq!(page.grapheme_used_bytes(), before_used);
        assert!(!page.cell_copy_at(0, 0).has_grapheme());
        assert!(!page.get_row(0).grapheme());
    }

    #[test]
    fn page_grapheme_map_oom_rolls_back_allocation_and_flags() {
        let mut page = Page::init(Capacity::with_metadata(
            5,
            5,
            8,
            HYPERLINK_BYTES_DEFAULT,
            GraphemeAlloc::BITMAP_BIT_SIZE as GraphemeBytesInt,
            STRING_BYTES_DEFAULT,
        ))
        .unwrap();
        assert_eq!(page.grapheme_capacity(), 4);

        for x in 0..page.grapheme_capacity() {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init('a' as u32 + x as u32);
            page.append_grapheme_at(x, 0, 0x0301 + x as u32).unwrap();
        }
        let used_after_filling_map = page.grapheme_used_bytes();

        *page.get_row_and_cell_mut(4, 0).cell = Cell::init('z' as u32);
        assert_eq!(
            page.append_grapheme_at(4, 0, 0x0300),
            Err(GraphemeError::GraphemeMapOutOfMemory)
        );

        assert_eq!(page.grapheme_count(), 4);
        assert_eq!(page.grapheme_used_bytes(), used_after_filling_map);
        assert!(!page.cell_copy_at(4, 0).has_grapheme());
        assert_eq!(page.lookup_grapheme_at(4, 0), None);
    }

    #[test]
    fn page_grapheme_allocator_oom_preserves_existing_data() {
        let mut page = Page::init(Capacity::with_metadata(
            5,
            5,
            8,
            HYPERLINK_BYTES_DEFAULT,
            GraphemeAlloc::BITMAP_BIT_SIZE as GraphemeBytesInt,
            STRING_BYTES_DEFAULT,
        ))
        .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);

        let mut expected = Vec::new();
        for i in 0.. {
            let cp = 0x0300 + i;
            match page.append_grapheme_at(0, 0, cp) {
                Ok(()) => expected.push(cp),
                Err(GraphemeError::GraphemeAllocOutOfMemory) => break,
                Err(err) => panic!("unexpected grapheme error: {err:?}"),
            }
        }
        assert!(expected.len() > GRAPHEME_CHUNK_LEN);

        assert_eq!(page.lookup_grapheme_at(0, 0).unwrap(), expected);
        assert!(page.cell_copy_at(0, 0).has_grapheme());
        assert!(page.get_row(0).grapheme());
    }

    #[test]
    fn page_clear_after_growth_removes_active_allocation() {
        let mut page = Page::init(Capacity::new(5, 5)).unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);
        for i in 0..=GRAPHEME_CHUNK_LEN {
            page.append_grapheme_at(0, 0, 0x0300 + i as u32).unwrap();
        }
        assert!(page.grapheme_used_bytes() > 0);

        page.clear_grapheme_at(0, 0);
        page.update_row_grapheme_flag(0);

        assert_eq!(page.grapheme_count(), 0);
        assert_eq!(page.lookup_grapheme_at(0, 0), None);
        assert_eq!(page.grapheme_used_bytes(), 0);
        assert!(!page.cell_copy_at(0, 0).has_grapheme());
        assert!(!page.get_row(0).grapheme());
    }

    #[test]
    fn page_clone() {
        let mut page = Page::init(Capacity {
            cols: 10,
            rows: 10,
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        for y in 0..page.capacity.rows as usize {
            *page.get_row_and_cell_mut(1, y).cell = Cell::init(y as u32);
        }

        let page2 = page.clone_page().unwrap();
        assert_eq!(page2.capacity, page.capacity);
        assert_eq!(page2.backing_len(), page.backing_len());
        assert_ne!(page2.backing_ptr(), page.backing_ptr());

        for y in 0..page2.capacity.rows as usize {
            assert_eq!(page2.get_cells(page2.get_row(y))[1].codepoint(), y as u32);
        }

        for y in 0..page.capacity.rows as usize {
            *page.get_row_and_cell_mut(1, y).cell = Cell::init(0);
        }

        for y in 0..page2.capacity.rows as usize {
            assert_eq!(page2.get_cells(page2.get_row(y))[1].codepoint(), y as u32);
        }
        for y in 0..page.capacity.rows as usize {
            assert_eq!(page.get_cells(page.get_row(y))[1].codepoint(), 0);
        }
    }

    #[test]
    fn page_clone_graphemes() {
        let mut page = Page::init(Capacity {
            cols: 10,
            rows: 10,
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        *page.get_row_and_cell_mut(0, 0).cell = Cell::init(0x09);
        page.append_grapheme_at(0, 0, 0x0a).unwrap();
        page.append_grapheme_at(0, 0, 0x0b).unwrap();

        let page2 = page.clone_page().unwrap();
        assert!(page2.get_row(0).grapheme());
        assert!(page2.cell_copy_at(0, 0).has_grapheme());
        assert_eq!(page2.lookup_grapheme_at(0, 0).unwrap(), vec![0x0a, 0x0b]);
    }

    #[test]
    fn page_clone_styles() {
        let mut page = Page::init(Capacity {
            cols: 10,
            rows: 10,
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };

        let id = page.add_style(bold).unwrap();
        for x in 0..page.size.cols as usize {
            let rac = page.get_row_and_cell_mut(x, 0);
            rac.row.set_styled(true);
            let mut cell = Cell::init((x + 1) as u32);
            cell.set_style_id(id);
            *rac.cell = cell;
            page.use_style(id);
        }
        let expected_ref_count = page.size.cols + 1;

        let page2 = page.clone_page().unwrap();
        assert_eq!(page2.capacity, page.capacity);
        assert_ne!(page2.backing_ptr(), page.backing_ptr());
        assert_eq!(page2.style_count(), 1);

        let cloned_id = page2.cell_copy_at(0, 0).style_id();
        assert_eq!(cloned_id, id);
        for x in 0..page2.size.cols as usize {
            let row = page2.get_row(0);
            let cell = page2.get_cells(row)[x];
            assert!(row.styled());
            assert_eq!(cell.codepoint(), (x + 1) as u32);
            assert_eq!(cell.style_id(), cloned_id);
        }
        assert_eq!(page2.get_style(cloned_id), bold);
        assert_eq!(page2.style_ref_count(cloned_id), expected_ref_count);
    }

    #[test]
    fn page_clone_styles_survive_source_release_and_drop() {
        let (page2, id, expected_ref_count, bold) = {
            let mut page = Page::init(Capacity {
                cols: 5,
                rows: 5,
                styles: 8,
                ..Capacity::new(5, 5)
            })
            .unwrap();
            let bold = style::Style {
                flags: style::Flags {
                    bold: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            };
            let italic = style::Style {
                flags: style::Flags {
                    italic: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            };

            let id = page.add_style(bold).unwrap();
            for x in 0..page.size.cols as usize {
                let rac = page.get_row_and_cell_mut(x, 0);
                rac.row.set_styled(true);
                let mut cell = Cell::init((x + 1) as u32);
                cell.set_style_id(id);
                *rac.cell = cell;
                page.use_style(id);
            }
            let expected_ref_count = page.size.cols + 1;

            let page2 = page.clone_page().unwrap();
            for _ in 0..expected_ref_count {
                page.release_style(id);
            }
            assert_eq!(page.style_ref_count(id), 0);
            assert_eq!(page.style_count(), 0);
            let replacement = page.add_style(italic).unwrap();
            assert_eq!(page.get_style(replacement), italic);

            (page2, id, expected_ref_count, bold)
        };

        assert_eq!(page2.get_style(id), bold);
        assert_eq!(page2.style_ref_count(id), expected_ref_count);
        assert_eq!(page2.cell_copy_at(0, 0).style_id(), id);
        assert!(page2.get_row(0).styled());
    }

    #[test]
    fn page_zero_capacity_style_insert_fails() {
        let mut page = Page::init(Capacity::with_metadata(
            5,
            5,
            0,
            HYPERLINK_BYTES_DEFAULT,
            GRAPHEME_BYTES_DEFAULT,
            STRING_BYTES_DEFAULT,
        ))
        .unwrap();
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };

        assert_eq!(
            page.add_style(bold),
            Err(super::super::ref_counted_set::AddError::OutOfMemory)
        );
        assert_eq!(page.style_count(), 0);
    }

    #[test]
    fn page_clone_graphemes_survive_source_clear_and_drop() {
        let page2 = {
            let mut page = Page::init(Capacity::new(5, 5)).unwrap();
            *page.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);
            page.append_grapheme_at(0, 0, 0x0301).unwrap();

            let page2 = page.clone_page().unwrap();
            page.clear_grapheme_at(0, 0);
            page.update_row_grapheme_flag(0);
            assert_eq!(page.lookup_grapheme_at(0, 0), None);
            page2
        };

        assert!(page2.get_row(0).grapheme());
        assert!(page2.cell_copy_at(0, 0).has_grapheme());
        assert_eq!(page2.lookup_grapheme_at(0, 0), Some(vec![0x0301]));
    }

    #[test]
    fn page_clone_drop_does_not_affect_source() {
        let mut page = Page::init(Capacity::new(5, 5)).unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);
        page.append_grapheme_at(0, 0, 0x0301).unwrap();

        {
            let page2 = page.clone_page().unwrap();
            assert_eq!(page2.lookup_grapheme_at(0, 0), Some(vec![0x0301]));
        }

        assert_eq!(page.lookup_grapheme_at(0, 0), Some(vec![0x0301]));
        assert!(page.cell_copy_at(0, 0).has_grapheme());
    }

    #[test]
    fn page_clone_from_plain_rows() {
        let mut page = Page::init(Capacity {
            cols: 10,
            rows: 10,
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        for y in 0..page.capacity.rows as usize {
            *page.get_row_and_cell_mut(1, y).cell = Cell::init(y as u32);
        }

        let mut page2 = Page::init(Capacity {
            cols: 10,
            rows: 10,
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        page2
            .clone_plain_rows_from(&page, 0, page.size.rows as usize)
            .unwrap();

        for y in 0..page2.capacity.rows as usize {
            assert_eq!(page2.get_cells(page2.get_row(y))[1].codepoint(), y as u32);
        }

        for y in 0..page.capacity.rows as usize {
            *page.get_row_and_cell_mut(1, y).cell = Cell::init(0);
        }

        for y in 0..page2.capacity.rows as usize {
            assert_eq!(page2.get_cells(page2.get_row(y))[1].codepoint(), y as u32);
        }
        for y in 0..page.capacity.rows as usize {
            assert_eq!(page.get_cells(page.get_row(y))[1].codepoint(), 0);
        }
    }

    #[test]
    fn page_clone_from_plain_rows_shrink_columns() {
        let mut page = Page::init(Capacity {
            cols: 10,
            rows: 10,
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        for y in 0..page.capacity.rows as usize {
            *page.get_row_and_cell_mut(1, y).cell = Cell::init(y as u32);
        }

        let mut page2 = Page::init(Capacity {
            cols: 5,
            rows: 10,
            styles: 8,
            ..Capacity::new(5, 10)
        })
        .unwrap();
        page2
            .clone_plain_rows_from(&page, 0, page.size.rows as usize)
            .unwrap();
        assert_eq!(page2.size.cols, 5);

        for y in 0..page2.capacity.rows as usize {
            assert_eq!(page2.get_cells(page2.get_row(y))[1].codepoint(), y as u32);
        }
    }

    #[test]
    fn page_clone_from_plain_rows_partial() {
        let mut page = Page::init(Capacity {
            cols: 10,
            rows: 10,
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        for y in 0..page.capacity.rows as usize {
            *page.get_row_and_cell_mut(1, y).cell = Cell::init(y as u32);
        }

        let mut page2 = Page::init(Capacity {
            cols: 10,
            rows: 10,
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        page2.clone_plain_rows_from(&page, 0, 5).unwrap();

        for y in 0..5 {
            assert_eq!(page2.get_cells(page2.get_row(y))[1].codepoint(), y as u32);
        }
        for y in 5..page2.size.rows as usize {
            assert_eq!(page2.get_cells(page2.get_row(y))[1].codepoint(), 0);
        }
    }

    #[test]
    fn page_clone_from_plain_rows_preserves_trailing_destination_cells() {
        let mut page = Page::init(Capacity::new(3, 1)).unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('b' as u32);
        let mut spacer_head = Cell::init('c' as u32);
        spacer_head.set_wide(Wide::SpacerHead);
        *page.get_row_and_cell_mut(2, 0).cell = spacer_head;

        let mut page2 = Page::init(Capacity::new(5, 1)).unwrap();
        *page2.get_row_and_cell_mut(3, 0).cell = Cell::init('x' as u32);
        *page2.get_row_and_cell_mut(4, 0).cell = Cell::init('y' as u32);

        page2.clone_plain_rows_from(&page, 0, 1).unwrap();
        let cells = page2.get_cells(page2.get_row(0));
        assert_eq!(cells[0].codepoint(), 'a' as u32);
        assert_eq!(cells[1].codepoint(), 'b' as u32);
        assert_eq!(cells[2].codepoint(), 'c' as u32);
        assert_eq!(cells[2].wide(), Wide::Narrow);
        assert_eq!(cells[3].codepoint(), 'x' as u32);
        assert_eq!(cells[4].codepoint(), 'y' as u32);
    }

    #[test]
    fn page_clone_from_plain_rows_rejects_managed_rows_and_cells() {
        let mut source = Page::init(Capacity {
            cols: 2,
            rows: 1,
            styles: 8,
            ..Capacity::new(2, 1)
        })
        .unwrap();
        let mut destination = Page::init(Capacity {
            cols: 2,
            rows: 1,
            styles: 8,
            ..Capacity::new(2, 1)
        })
        .unwrap();

        source.get_row_mut(0).set_styled(true);
        assert_eq!(
            destination.clone_plain_rows_from(&source, 0, 1),
            Err(CloneFromError::SourceRowManagedMemory)
        );
        source.get_row_mut(0).set_styled(false);

        destination.get_row_mut(0).set_grapheme(true);
        assert_eq!(
            destination.clone_plain_rows_from(&source, 0, 1),
            Err(CloneFromError::DestinationRowManagedMemory)
        );
        destination.get_row_mut(0).set_grapheme(false);

        let mut source_cell = Cell::init('s' as u32);
        source_cell.set_style_id(1);
        *source.get_row_and_cell_mut(0, 0).cell = source_cell;
        assert_eq!(
            destination.clone_plain_rows_from(&source, 0, 1),
            Err(CloneFromError::SourceCellManagedMemory)
        );
        *source.get_row_and_cell_mut(0, 0).cell = Cell::init('s' as u32);

        let mut destination_cell = Cell::init('d' as u32);
        destination_cell.set_hyperlink(true);
        *destination.get_row_and_cell_mut(0, 0).cell = destination_cell;
        assert_eq!(
            destination.clone_plain_rows_from(&source, 0, 1),
            Err(CloneFromError::DestinationCellManagedMemory)
        );
    }

    #[test]
    fn page_create_drop_loop() {
        for _ in 0..32 {
            let _page = Page::init(Capacity::new(5, 5)).unwrap();
        }
    }

    #[test]
    #[should_panic(expected = "assertion failed: y < self.size.rows as usize")]
    fn page_get_row_rejects_out_of_bounds() {
        let page = Page::init(Capacity::new(2, 2)).unwrap();
        let _ = page.get_row(2);
    }

    #[test]
    #[should_panic(expected = "assertion failed: x < self.size.cols as usize")]
    fn page_get_row_and_cell_rejects_out_of_bounds_x() {
        let mut page = Page::init(Capacity::new(2, 2)).unwrap();
        let _ = page.get_row_and_cell_mut(2, 0);
    }

    #[test]
    #[should_panic(expected = "row cell offset is before cells region")]
    fn page_get_cells_rejects_corrupt_row_cells_offset() {
        let mut page = Page::init(Capacity::new(2, 2)).unwrap();
        page.get_row_mut(0).set_cells(Offset::new(0));

        let row = page.get_row(0);
        let _ = page.get_cells(row);
    }

    #[test]
    fn page_layout_can_take_a_maxed_capacity() {
        let cap = Capacity::with_metadata(
            CellCountInt::MAX,
            CellCountInt::MAX,
            CellCountInt::MAX,
            HyperlinkCountInt::MAX,
            GraphemeBytesInt::MAX,
            StringBytesInt::MAX,
        );

        let _ = page_layout(cap);
    }

    #[test]
    fn page_capacity_adjust_cols_down() {
        let original = STD_CAPACITY;
        let original_size = page_layout(original).total_size;
        let adjusted = original
            .adjust(CapacityAdjustment {
                cols: Some(original.cols / 2),
            })
            .unwrap();
        let adjusted_size = page_layout(adjusted).total_size;
        assert_eq!(original_size, adjusted_size);

        let mut bigger = adjusted;
        bigger.rows += 1;
        assert!(page_layout(bigger).total_size > original_size);
    }

    #[test]
    fn page_capacity_adjust_cols_down_to_one() {
        let original = STD_CAPACITY;
        let original_size = page_layout(original).total_size;
        let adjusted = original
            .adjust(CapacityAdjustment { cols: Some(1) })
            .unwrap();
        let adjusted_size = page_layout(adjusted).total_size;
        assert_eq!(original_size, adjusted_size);

        let mut bigger = adjusted;
        bigger.rows += 1;
        assert!(page_layout(bigger).total_size > original_size);
    }

    #[test]
    fn page_capacity_adjust_cols_up() {
        let original = STD_CAPACITY;
        let original_size = page_layout(original).total_size;
        let adjusted = original
            .adjust(CapacityAdjustment {
                cols: Some(original.cols * 2),
            })
            .unwrap();
        let adjusted_size = page_layout(adjusted).total_size;
        assert_eq!(original_size, adjusted_size);

        let mut bigger = adjusted;
        bigger.rows += 1;
        assert!(page_layout(bigger).total_size > original_size);
    }

    #[test]
    fn page_capacity_adjust_cols_sweep() {
        let mut cap = STD_CAPACITY;
        let original_cols = cap.cols;
        let original_size = page_layout(cap).total_size;

        for cols in 1..original_cols * 2 {
            cap = cap.adjust(CapacityAdjustment { cols: Some(cols) }).unwrap();
            assert_eq!(page_layout(cap).total_size, original_size);

            let mut bigger = cap;
            bigger.rows += 1;
            assert!(page_layout(bigger).total_size > original_size);
        }
    }

    #[test]
    fn page_capacity_adjust_cols_too_high() {
        let result = STD_CAPACITY.adjust(CapacityAdjustment {
            cols: Some(CellCountInt::MAX),
        });
        assert_eq!(result, Err(CapacityAdjustError::OutOfMemory));
    }

    #[test]
    fn capacity_max_cols_basic() {
        let cap = STD_CAPACITY;
        let max = cap.max_cols().unwrap();

        assert!(max >= cap.cols);
        let adjusted = cap.adjust(CapacityAdjustment { cols: Some(max) }).unwrap();
        assert!(adjusted.rows >= 1);
        assert_eq!(
            cap.adjust(CapacityAdjustment {
                cols: Some(max + 1),
            }),
            Err(CapacityAdjustError::OutOfMemory)
        );
    }

    #[test]
    fn capacity_max_cols_preserves_total_size() {
        let cap = STD_CAPACITY;
        let original_size = page_layout(cap).total_size;
        let max = cap.max_cols().unwrap();
        let adjusted = cap.adjust(CapacityAdjustment { cols: Some(max) }).unwrap();

        assert_eq!(page_layout(adjusted).total_size, original_size);
    }

    #[test]
    fn capacity_max_cols_with_one_row_exactly() {
        let cap = STD_CAPACITY;
        let max = cap.max_cols().unwrap();
        let adjusted = cap.adjust(CapacityAdjustment { cols: Some(max) }).unwrap();

        assert_eq!(adjusted.rows, 1);
    }

    #[test]
    fn row_layout() {
        assert_eq!(size_of::<Row>(), 8);
        assert_eq!(align_of::<Row>(), align_of::<u64>());
        assert_eq!(Row::default().cval(), 0);
    }

    #[test]
    fn row_raw_fields() {
        let mut row = Row::default();
        row.set_cells(Offset::new(0x1234_5678));
        assert_eq!(row.cells().offset(), 0x1234_5678);
        assert_eq!(row.cval(), 0x1234_5678);

        let bool_cases: [(fn(&mut Row, bool), u64); 7] = [
            (Row::set_wrap, 1_u64 << 32),
            (Row::set_wrap_continuation, 1_u64 << 33),
            (Row::set_grapheme, 1_u64 << 34),
            (Row::set_styled, 1_u64 << 35),
            (Row::set_hyperlink, 1_u64 << 36),
            (Row::set_kitty_virtual_placeholder, 1_u64 << 39),
            (Row::set_dirty, 1_u64 << 40),
        ];

        for (set, expected) in bool_cases {
            let mut row = Row::default();
            set(&mut row, true);
            assert_eq!(row.cval(), expected);
        }
    }

    #[test]
    fn row_semantic_prompt_raw_values() {
        let cases = [
            (SemanticPrompt::None, 0),
            (SemanticPrompt::Prompt, 1_u64 << 37),
            (SemanticPrompt::PromptContinuation, 2_u64 << 37),
        ];

        for (value, expected) in cases {
            let mut row = Row::default();
            row.set_semantic_prompt(value);
            assert_eq!(row.semantic_prompt(), value);
            assert_eq!(row.cval(), expected);
        }
    }

    #[test]
    fn row_managed_memory() {
        assert!(!Row::default().managed_memory());

        let mut row = Row::default();
        row.set_styled(true);
        assert!(row.managed_memory());

        let mut row = Row::default();
        row.set_hyperlink(true);
        assert!(row.managed_memory());

        let mut row = Row::default();
        row.set_grapheme(true);
        assert!(row.managed_memory());
    }

    #[test]
    fn cell_layout() {
        assert_eq!(size_of::<Cell>(), 8);
        assert_eq!(align_of::<Cell>(), align_of::<u64>());
    }

    #[test]
    fn cell_is_zero_by_default() {
        let cell = Cell::init(0);
        assert_eq!(cell.cval(), 0);
        assert!(cell.is_zero());
        assert_eq!(cell.semantic_content(), SemanticContent::Output);
    }

    #[test]
    fn cell_raw_content_fields() {
        assert_eq!(Cell::init('A' as u32).cval(), 0x41 << 2);
        assert_eq!(Cell::init('A' as u32).codepoint(), 'A' as u32);
        assert_eq!(Cell::bg_palette(7).cval(), 2 | (7 << 2));
        assert_eq!(Cell::bg_palette(7).color_palette(), 7);
        assert_eq!(
            Cell::bg_rgb(Rgb::new(1, 2, 3)).cval(),
            3 | ((1 | (2 << 8) | (3 << 16)) << 2)
        );
        assert_eq!(
            Cell::bg_rgb(Rgb::new(1, 2, 3)).color_rgb(),
            Rgb::new(1, 2, 3)
        );
    }

    #[test]
    fn cell_raw_style_and_flag_fields() {
        let mut cell = Cell::default();
        cell.set_style_id(1);
        assert_eq!(cell.style_id(), 1);
        assert_eq!(cell.cval(), 1_u64 << 26);

        let mut cell = Cell::default();
        cell.set_protected(true);
        assert!(cell.protected());
        assert_eq!(cell.cval(), 1_u64 << 44);

        let mut cell = Cell::default();
        cell.set_hyperlink(true);
        assert!(cell.hyperlink());
        assert_eq!(cell.cval(), 1_u64 << 45);
    }

    #[test]
    fn cell_raw_wide_values() {
        let cases = [
            (Wide::Narrow, 0),
            (Wide::Wide, 1_u64 << 42),
            (Wide::SpacerTail, 2_u64 << 42),
            (Wide::SpacerHead, 3_u64 << 42),
        ];

        for (value, expected) in cases {
            let mut cell = Cell::default();
            cell.set_wide(value);
            assert_eq!(cell.wide(), value);
            assert_eq!(cell.cval(), expected);
        }
    }

    #[test]
    fn cell_raw_content_tag_values() {
        let mut cell = Cell::default();
        cell.set_content_tag(ContentTag::Codepoint);
        assert_eq!(cell.content_tag(), ContentTag::Codepoint);
        assert_eq!(cell.cval(), 0);

        let mut cell = Cell::default();
        cell.set_content_tag(ContentTag::CodepointGrapheme);
        assert_eq!(cell.content_tag(), ContentTag::CodepointGrapheme);
        assert_eq!(cell.cval(), 1);

        assert_eq!(
            Cell::bg_palette(0).content_tag(),
            ContentTag::BgColorPalette
        );
        assert_eq!(Cell::bg_palette(0).cval(), 2);
        assert_eq!(
            Cell::bg_rgb(Rgb::new(0, 0, 0)).content_tag(),
            ContentTag::BgColorRgb
        );
        assert_eq!(Cell::bg_rgb(Rgb::new(0, 0, 0)).cval(), 3);
    }

    #[test]
    fn cell_raw_semantic_content_values() {
        let cases = [
            (SemanticContent::Output, 0),
            (SemanticContent::Input, 1_u64 << 46),
            (SemanticContent::Prompt, 2_u64 << 46),
        ];

        for (value, expected) in cases {
            let mut cell = Cell::default();
            cell.set_semantic_content(value);
            assert_eq!(cell.semantic_content(), value);
            assert_eq!(cell.cval(), expected);
        }
    }

    #[test]
    fn cell_helpers() {
        assert!(!Cell::init(0).has_text());
        assert!(Cell::init('x' as u32).has_text());
        assert_eq!(Cell::init('x' as u32).codepoint(), 'x' as u32);
        assert_eq!(Cell::bg_palette(1).codepoint(), 0);
        assert_eq!(Cell::bg_rgb(Rgb::new(1, 2, 3)).codepoint(), 0);

        let mut cell = Cell::init('x' as u32);
        assert_eq!(cell.grid_width(), 1);
        cell.set_wide(Wide::Wide);
        assert_eq!(cell.grid_width(), 2);
        cell.set_wide(Wide::SpacerTail);
        assert_eq!(cell.grid_width(), 1);
        cell.set_wide(Wide::SpacerHead);
        assert_eq!(cell.grid_width(), 1);

        assert!(!Cell::init(0).has_styling());
        let mut styled = Cell::init(0);
        styled.set_style_id(1);
        assert!(styled.has_styling());

        assert!(Cell::init(0).is_empty());
        assert!(!Cell::init('x' as u32).is_empty());
        let mut spacer = Cell::init(0);
        spacer.set_wide(Wide::SpacerTail);
        assert!(!spacer.is_empty());
        assert!(!Cell::bg_palette(1).is_empty());
    }

    #[test]
    fn cell_grapheme_and_has_text_any() {
        let mut grapheme = Cell::init('x' as u32);
        grapheme.set_content_tag(ContentTag::CodepointGrapheme);
        assert!(grapheme.has_grapheme());
        assert!(grapheme.has_text());

        assert!(!Cell::has_text_any(&[Cell::init(0), Cell::bg_palette(1)]));
        assert!(Cell::has_text_any(&[
            Cell::init(0),
            Cell::bg_palette(1),
            Cell::init('x' as u32),
        ]));
    }

    #[test]
    #[should_panic(expected = "assertion failed: codepoint <= 0x10ffff")]
    fn cell_rejects_invalid_codepoint() {
        let _ = Cell::init(0x11_0000);
    }
}
