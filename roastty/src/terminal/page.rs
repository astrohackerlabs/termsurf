use std::collections::HashMap;
use std::io;
use std::mem::{align_of, size_of};
use std::ptr::NonNull;

use super::bitmap_allocator::{BitmapAllocator, Layout as BitmapAllocatorLayout};
use super::color::Rgb;
use super::hyperlink;
use super::offset_hash_map;
use super::ref_counted_set;
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

    pub(super) const fn cols(self) -> CellCountInt {
        self.cols
    }

    pub(super) const fn rows(self) -> CellCountInt {
        self.rows
    }

    pub(super) const fn styles(self) -> CellCountInt {
        self.styles
    }

    pub(super) const fn hyperlink_bytes(self) -> HyperlinkCountInt {
        self.hyperlink_bytes
    }

    pub(super) const fn grapheme_bytes(self) -> GraphemeBytesInt {
        self.grapheme_bytes
    }

    pub(super) const fn string_bytes(self) -> StringBytesInt {
        self.string_bytes
    }

    pub(super) const fn with_cols(mut self, cols: CellCountInt) -> Self {
        self.cols = cols;
        self
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

impl CapacityAdjustment {
    pub(super) const fn cols(cols: CellCountInt) -> Self {
        Self { cols: Some(cols) }
    }
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

impl PageLayout {
    pub(super) const fn total_size(self) -> usize {
        self.total_size
    }
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
    string_alloc: StringAlloc,
    hyperlink_set: hyperlink::Set,
    hyperlink_map: Option<offset_hash_map::OffsetHashMap<Offset<Cell>, HyperlinkId>>,
}

#[derive(Debug)]
struct PageRegions {
    rows: Offset<Row>,
    cells: Offset<Cell>,
    styles: style::Set,
    grapheme_alloc: GraphemeAlloc,
    grapheme_map: Option<offset_hash_map::OffsetHashMap<Offset<Cell>, OffsetSlice<u32>>>,
    string_alloc: StringAlloc,
    hyperlink_set: hyperlink::Set,
    hyperlink_map: Option<offset_hash_map::OffsetHashMap<Offset<Cell>, HyperlinkId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GraphemeError {
    GraphemeMapOutOfMemory,
    GraphemeAllocOutOfMemory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InsertHyperlinkError {
    StringsOutOfMemory,
    SetOutOfMemory,
    SetNeedsRehash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HyperlinkError {
    HyperlinkMapOutOfMemory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PrintCellError {
    UnsupportedManagedCell,
    StyleOutOfMemory,
    HyperlinkOutOfMemory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CloneFromError {
    SourceRowUnsupportedManagedMemory,
    DestinationRowUnsupportedManagedMemory,
    SourceCellUnsupportedManagedMemory,
    DestinationCellUnsupportedManagedMemory,
    Grapheme(GraphemeError),
    Style(super::ref_counted_set::AddError),
    PageAlloc,
    StringAllocOutOfMemory,
    HyperlinkMapOutOfMemory,
    HyperlinkSetOutOfMemory,
    HyperlinkSetNeedsRehash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntegrityError {
    ZeroRowCount,
    ZeroColCount,
    UnmarkedGraphemeRow,
    MissingGraphemeData,
    InvalidGraphemeCount,
    UnmarkedGraphemeCell,
    MissingStyle,
    UnmarkedStyleRow,
    MismatchedStyleRef,
    InvalidStyleCount,
    MissingHyperlinkData,
    MismatchedHyperlinkRef,
    UnmarkedHyperlinkCell,
    UnmarkedHyperlinkRow,
    InvalidSpacerTailLocation,
    InvalidSpacerHeadLocation,
    UnwrappedSpacerHead,
}

impl From<GraphemeError> for CloneFromError {
    fn from(value: GraphemeError) -> Self {
        Self::Grapheme(value)
    }
}

impl From<super::ref_counted_set::AddError> for CloneFromError {
    fn from(value: super::ref_counted_set::AddError) -> Self {
        Self::Style(value)
    }
}

impl Page {
    pub(super) fn init(capacity: Capacity) -> Result<Self, PageAllocError> {
        let layout = page_layout(capacity);
        assert_eq!(layout.total_size % PAGE_SIZE_MIN, 0);

        let mut memory = PageMemory::new(layout.total_size)?;
        let regions = Self::init_regions(&mut memory, layout);

        Ok(Self {
            memory,
            rows: regions.rows,
            cells: regions.cells,
            dirty: false,
            size: Size {
                cols: capacity.cols,
                rows: capacity.rows,
            },
            capacity,
            layout,
            styles: regions.styles,
            grapheme_alloc: regions.grapheme_alloc,
            grapheme_map: regions.grapheme_map,
            string_alloc: regions.string_alloc,
            hyperlink_set: regions.hyperlink_set,
            hyperlink_map: regions.hyperlink_map,
        })
    }

    pub(super) fn reinit(&mut self) {
        let capacity = self.capacity;
        self.reinit_with_capacity(capacity);
    }

    pub(super) fn reinit_with_capacity(&mut self, capacity: Capacity) {
        let layout = page_layout(capacity);
        assert_eq!(layout.total_size, self.memory.len());

        self.memory.as_mut_slice().fill(0);
        let regions = Self::init_regions(&mut self.memory, layout);

        self.rows = regions.rows;
        self.cells = regions.cells;
        self.dirty = false;
        self.size = Size {
            cols: capacity.cols,
            rows: capacity.rows,
        };
        self.capacity = capacity;
        self.layout = layout;
        self.styles = regions.styles;
        self.grapheme_alloc = regions.grapheme_alloc;
        self.grapheme_map = regions.grapheme_map;
        self.string_alloc = regions.string_alloc;
        self.hyperlink_set = regions.hyperlink_set;
        self.hyperlink_map = regions.hyperlink_map;
    }

    fn verify_integrity(&self) -> Result<(), IntegrityError> {
        if self.size.rows == 0 {
            return Err(IntegrityError::ZeroRowCount);
        }
        if self.size.cols == 0 {
            return Err(IntegrityError::ZeroColCount);
        }

        let mut graphemes_seen = 0_usize;
        let grapheme_count = self.grapheme_count();
        let mut styles_seen: HashMap<style::Id, usize> = HashMap::new();
        let mut hyperlinks_seen: HashMap<hyperlink::Id, usize> = HashMap::new();

        for y in 0..self.size.rows as usize {
            let row = self.get_row(y);
            self.assert_row_cells_range(row);
            let graphemes_start = graphemes_seen;
            for (x, cell) in self.get_cells(row).iter().copied().enumerate() {
                let offset = self.row_cell_offset(row, x);

                if cell.has_grapheme() {
                    if self.lookup_grapheme_at_offset(offset).is_none() {
                        return Err(IntegrityError::MissingGraphemeData);
                    }
                    graphemes_seen += 1;
                } else if grapheme_count > 0 && self.lookup_grapheme_at_offset(offset).is_some() {
                    return Err(IntegrityError::UnmarkedGraphemeCell);
                }

                let style_id = cell.style_id();
                if style_id != style::DEFAULT_ID {
                    if !self.styles.contains_id(self.style_base(), style_id) {
                        return Err(IntegrityError::MissingStyle);
                    }
                    if !row.styled() {
                        return Err(IntegrityError::UnmarkedStyleRow);
                    }
                    *styles_seen.entry(style_id).or_insert(0) += 1;
                }

                if cell.hyperlink() {
                    let id = self
                        .lookup_hyperlink_at_offset(offset)
                        .ok_or(IntegrityError::MissingHyperlinkData)?;
                    if !row.hyperlink() {
                        return Err(IntegrityError::UnmarkedHyperlinkRow);
                    }
                    if !self
                        .hyperlink_set
                        .contains_id(self.hyperlink_set_base(), id)
                    {
                        return Err(IntegrityError::MissingHyperlinkData);
                    }
                    *hyperlinks_seen.entry(id).or_insert(0) += 1;
                } else if self.lookup_hyperlink_at_offset(offset).is_some() {
                    return Err(IntegrityError::UnmarkedHyperlinkCell);
                }

                match cell.wide() {
                    Wide::Narrow | Wide::Wide => {}
                    Wide::SpacerTail => {
                        if x == 0 {
                            return Err(IntegrityError::InvalidSpacerTailLocation);
                        }
                        let previous = self.cell_copy_at(x - 1, y);
                        if previous.wide() != Wide::Wide {
                            return Err(IntegrityError::InvalidSpacerTailLocation);
                        }
                    }
                    Wide::SpacerHead => {
                        if x != self.size.cols as usize - 1 {
                            return Err(IntegrityError::InvalidSpacerHeadLocation);
                        }
                        if !row.wrap() {
                            return Err(IntegrityError::UnwrappedSpacerHead);
                        }
                    }
                }
            }

            if graphemes_seen > graphemes_start && !row.grapheme() {
                return Err(IntegrityError::UnmarkedGraphemeRow);
            }
        }

        if graphemes_seen > self.grapheme_count() {
            return Err(IntegrityError::InvalidGraphemeCount);
        }

        for (id, seen) in styles_seen {
            if (self.style_ref_count(id) as usize) < seen {
                return Err(IntegrityError::MismatchedStyleRef);
            }
        }

        for (id, seen) in hyperlinks_seen {
            if (self.hyperlink_ref_count(id) as usize) < seen {
                return Err(IntegrityError::MismatchedHyperlinkRef);
            }
        }

        Ok(())
    }

    fn init_regions(memory: &mut PageMemory, layout: PageLayout) -> PageRegions {
        let capacity = layout.capacity;
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
        let string_alloc = unsafe {
            // Safety: PageMemory is live for the lifetime of Page, and the
            // layout range is inside the page backing allocation.
            StringAlloc::init(
                super::size::OffsetBuf::init_offset(memory.as_mut_ptr(), layout.string_alloc_start),
                layout.string_alloc_layout,
            )
        };
        let hyperlink_set = hyperlink::Set::init(
            unsafe {
                // Safety: hyperlink_set_start is inside this page's live
                // backing memory.
                memory.as_mut_ptr().add(layout.hyperlink_set_start)
            },
            layout.hyperlink_set_layout.into(),
        );
        let hyperlink_map = if layout.hyperlink_map_layout.capacity == 0 {
            None
        } else {
            Some(unsafe {
                // Safety: PageMemory is live for the lifetime of Page, and the
                // layout range is inside the page backing allocation.
                offset_hash_map::OffsetHashMap::init(
                    super::size::OffsetBuf::init_offset(
                        memory.as_mut_ptr(),
                        layout.hyperlink_map_start,
                    ),
                    layout.hyperlink_map_layout.into(),
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

        PageRegions {
            rows,
            cells,
            styles,
            grapheme_alloc,
            grapheme_map,
            string_alloc,
            hyperlink_set,
            hyperlink_map,
        }
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

    pub(super) fn set_dirty(&mut self, value: bool) {
        self.dirty = value;
    }

    pub(super) fn capacity(&self) -> Capacity {
        self.capacity
    }

    pub(super) fn size_cols(&self) -> CellCountInt {
        self.size.cols
    }

    pub(super) fn size_rows(&self) -> CellCountInt {
        self.size.rows
    }

    pub(super) fn set_size_rows(&mut self, rows: CellCountInt) {
        assert!(rows <= self.capacity.rows);
        self.size.rows = rows;
    }

    pub(super) fn set_size_cols(&mut self, cols: CellCountInt) {
        assert!(cols <= self.capacity.cols);
        self.size.cols = cols;
    }

    pub(super) fn exact_row_capacity(&self, y_start: usize, y_end: usize) -> Capacity {
        assert!(y_start < y_end);
        assert!(y_end <= self.size.rows as usize);

        let mut ids_seen = [false; CellCountInt::MAX as usize + 1];
        let mut style_count = 0_usize;
        let mut grapheme_bytes = 0_usize;

        for y in y_start..y_end {
            let row = self.get_row(y);
            for (x, cell) in self.get_cells(row).iter().enumerate() {
                let style_id = cell.style_id();
                if style_id != style::DEFAULT_ID && !ids_seen[style_id as usize] {
                    ids_seen[style_id as usize] = true;
                    style_count += 1;
                }

                if cell.has_grapheme() {
                    let offset = self.row_cell_offset(row, x);
                    let slice = self
                        .grapheme_map_ref()
                        .and_then(|map| map.get(offset))
                        .expect("grapheme cell must have map data");
                    grapheme_bytes += GraphemeAlloc::bytes_required::<u32>(slice.len());
                }
            }
        }
        let styles = style::Set::capacity_for_count(style_count);

        ids_seen.fill(false);
        let mut hyperlink_count = 0_usize;
        let mut hyperlink_cells = 0_usize;
        let mut string_bytes = 0_usize;

        for y in y_start..y_end {
            let row = self.get_row(y);
            for (x, cell) in self.get_cells(row).iter().enumerate() {
                if !cell.hyperlink() {
                    continue;
                }

                hyperlink_cells += 1;
                let offset = self.row_cell_offset(row, x);
                let id = self
                    .lookup_hyperlink_at_offset(offset)
                    .expect("hyperlink cell must have map data");
                if ids_seen[id as usize] {
                    continue;
                }
                ids_seen[id as usize] = true;
                hyperlink_count += 1;

                let entry = *self.hyperlink_set.get(self.hyperlink_set_base(), id);
                string_bytes += StringAlloc::bytes_required::<u8>(entry.uri().len());
                if entry.id().tag() == hyperlink::PageEntryIdTag::Explicit {
                    string_bytes +=
                        StringAlloc::bytes_required::<u8>(entry.id().explicit_value().len());
                }
            }
        }

        let hyperlink_set_cap = hyperlink::Set::capacity_for_count(hyperlink_count);
        let hyperlink_map_min = hyperlink_cells.div_ceil(HYPERLINK_CELL_MULTIPLIER);
        let hyperlink_cap = hyperlink_set_cap.max(hyperlink_map_min);

        Capacity {
            cols: self.size.cols,
            rows: (y_end - y_start)
                .try_into()
                .expect("row capacity must fit CellCountInt"),
            styles: styles
                .try_into()
                .expect("style capacity must fit CellCountInt"),
            hyperlink_bytes: (hyperlink_cap * HYPERLINK_SET_ITEM_SIZE)
                .try_into()
                .expect("hyperlink byte capacity must fit HyperlinkCountInt"),
            grapheme_bytes: grapheme_bytes
                .try_into()
                .expect("grapheme byte capacity must fit GraphemeBytesInt"),
            string_bytes: string_bytes
                .try_into()
                .expect("string byte capacity must fit StringBytesInt"),
        }
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
            string_alloc: self.string_alloc,
            hyperlink_set: self.hyperlink_set,
            hyperlink_map: self.hyperlink_map,
        })
    }

    pub(super) fn clone_rows_from(
        &mut self,
        other: &Page,
        y_start: usize,
        y_end: usize,
    ) -> Result<(), CloneFromError> {
        assert!(y_start <= y_end);
        assert!(y_end <= other.size.rows as usize);
        assert!(y_end - y_start <= self.size.rows as usize);

        for (dst_y, src_y) in (y_start..y_end).enumerate() {
            self.clone_row_from(other, dst_y, src_y)?;
        }

        Ok(())
    }

    pub(super) fn clone_row_from(
        &mut self,
        other: &Page,
        dst_y: usize,
        src_y: usize,
    ) -> Result<(), CloneFromError> {
        self.clone_partial_row_from(other, dst_y, src_y, 0, self.size.cols as usize)
    }

    pub(super) fn clone_partial_row_from(
        &mut self,
        other: &Page,
        dst_y: usize,
        src_y: usize,
        x_start: usize,
        x_end_req: usize,
    ) -> Result<(), CloneFromError> {
        assert!(dst_y < self.size.rows as usize);
        assert!(src_y < other.size.rows as usize);
        let cell_len = (self.size.cols as usize).min(other.size.cols as usize);
        let x_end = x_end_req.min(cell_len);
        assert!(x_start <= x_end);

        let snapshots = other.source_cell_snapshots(src_y, x_start, x_end);
        self.clone_partial_row_from_snapshots(
            Some(other),
            snapshots.as_slice(),
            dst_y,
            *other.get_row(src_y),
            x_start,
            x_end,
            other.size.cols as usize,
        )
    }

    pub(super) fn clone_partial_row_within_page(
        &mut self,
        dst_y: usize,
        src_y: usize,
        x_start: usize,
        x_end_req: usize,
    ) -> Result<(), CloneFromError> {
        assert!(dst_y < self.size.rows as usize);
        assert!(src_y < self.size.rows as usize);
        let cell_len = self.size.cols as usize;
        let x_end = x_end_req.min(cell_len);
        assert!(x_start <= x_end);

        let snapshots = self.source_cell_snapshots(src_y, x_start, x_end);
        self.hold_same_page_snapshot_refs(&snapshots);
        let result = self.clone_partial_row_from_snapshots(
            None,
            snapshots.as_slice(),
            dst_y,
            *self.get_row(src_y),
            x_start,
            x_end,
            self.size.cols as usize,
        );
        self.release_same_page_snapshot_refs(&snapshots);
        result
    }

    fn clone_partial_row_from_snapshots(
        &mut self,
        source: Option<&Page>,
        snapshots: &[SourceCellSnapshot],
        dst_y: usize,
        src_row: Row,
        x_start: usize,
        x_end: usize,
        source_cols: usize,
    ) -> Result<(), CloneFromError> {
        debug_assert_eq!(snapshots.len(), x_end - x_start);
        let dst_row = *self.get_row(dst_y);
        let dst_cell_offsets = (x_start..x_end)
            .map(|x| self.cell_offset_from_row_cells(dst_row.cells(), x))
            .collect::<Vec<_>>();

        self.clear_cells(dst_y, x_start, x_end);

        let mut row_copy = src_row;
        row_copy.set_cells(dst_row.cells());

        if snapshots.len() < self.size.cols as usize {
            row_copy.set_wrap(dst_row.wrap());
            row_copy.set_wrap_continuation(dst_row.wrap_continuation());
            row_copy.set_grapheme(dst_row.grapheme());
            row_copy.set_hyperlink(dst_row.hyperlink());
            row_copy.set_styled(dst_row.styled());
            row_copy.set_dirty(src_row.dirty() || dst_row.dirty());
        }

        *self.get_row_mut(dst_y) = row_copy;

        let should_clear_spacer_head = self.size.cols as usize > source_cols
            && source_cols > 0
            && x_start <= source_cols - 1
            && source_cols - 1 < x_end;
        {
            let cells = self.get_cells_mut(dst_y);
            for (x, snapshot) in (x_start..x_end).zip(snapshots) {
                cells[x] = snapshot.cell;
                let cell = &mut cells[x];
                cell.set_style_id(style::DEFAULT_ID);
                cell.set_hyperlink(false);
                if cell.has_grapheme() {
                    cell.set_content_tag(ContentTag::Codepoint);
                }
            }

            if should_clear_spacer_head {
                let last = &mut cells[source_cols - 1];
                if last.wide() == Wide::SpacerHead {
                    last.set_wide(Wide::Narrow);
                }
            }
        }

        for (offset, snapshot) in dst_cell_offsets.iter().copied().zip(snapshots) {
            if !snapshot.graphemes.is_empty() {
                unsafe {
                    // Safety: offset came from this row's valid cell range.
                    (*offset.ptr_mut(self.memory.as_mut_ptr()))
                        .set_content_tag(ContentTag::Codepoint);
                }
                for cp in snapshot.graphemes.iter().copied() {
                    if let Err(err) = self.append_grapheme_at_offset(offset, cp) {
                        self.update_row_grapheme_flag(dst_y);
                        self.update_row_hyperlink_flag(dst_y);
                        self.update_row_styled_flag(dst_y);
                        return Err(err.into());
                    }
                }
            }
        }

        for (offset, snapshot) in dst_cell_offsets.iter().copied().zip(snapshots) {
            if let Some(source_id) = snapshot.hyperlink {
                let dst_id = match source {
                    Some(source) => match self.prepare_cloned_hyperlink(source, source_id) {
                        Ok(id) => id,
                        Err(err) => {
                            self.update_row_grapheme_flag(dst_y);
                            self.update_row_hyperlink_flag(dst_y);
                            self.update_row_styled_flag(dst_y);
                            return Err(err);
                        }
                    },
                    None => {
                        self.use_hyperlink(source_id);
                        source_id
                    }
                };
                if let Err(HyperlinkError::HyperlinkMapOutOfMemory) =
                    self.set_hyperlink_at_offset(dst_y, offset, dst_id)
                {
                    self.release_hyperlink(dst_id);
                    self.update_row_grapheme_flag(dst_y);
                    self.update_row_hyperlink_flag(dst_y);
                    self.update_row_styled_flag(dst_y);
                    return Err(CloneFromError::HyperlinkMapOutOfMemory);
                }
            }
        }

        for (offset, snapshot) in dst_cell_offsets.iter().copied().zip(snapshots) {
            if let Some(source_id) = snapshot.style {
                let dst_id = match source {
                    Some(source) => {
                        match self.add_style_with_id(source.get_style(source_id), source_id) {
                            Ok(id) => id,
                            Err(err) => {
                                self.update_row_grapheme_flag(dst_y);
                                self.update_row_hyperlink_flag(dst_y);
                                self.update_row_styled_flag(dst_y);
                                return Err(err.into());
                            }
                        }
                    }
                    None => {
                        self.use_style(source_id);
                        source_id
                    }
                };
                unsafe {
                    // Safety: offset came from this row's valid cell range.
                    (*offset.ptr_mut(self.memory.as_mut_ptr())).set_style_id(dst_id);
                }
            }
        }

        self.update_row_grapheme_flag(dst_y);
        self.update_row_hyperlink_flag(dst_y);
        self.update_row_styled_flag(dst_y);
        Ok(())
    }

    fn source_cell_snapshots(
        &self,
        src_y: usize,
        x_start: usize,
        x_end: usize,
    ) -> Vec<SourceCellSnapshot> {
        assert!(src_y < self.size.rows as usize);
        assert!(x_end <= self.size.cols as usize);
        let src_row = self.get_row(src_y);
        let src_cells = self.get_cells(src_row);

        src_cells[x_start..x_end]
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                let x = x_start + index;
                let offset = self.cell_offset_from_row_cells(src_row.cells(), x);
                SourceCellSnapshot {
                    cell: *cell,
                    graphemes: cell
                        .has_grapheme()
                        .then(|| {
                            self.lookup_grapheme_at_offset(offset)
                                .expect("grapheme cell must have map data")
                        })
                        .unwrap_or_default(),
                    hyperlink: cell.hyperlink().then(|| {
                        self.lookup_hyperlink_at_offset(offset)
                            .expect("hyperlink cell must have map data")
                    }),
                    style: (cell.style_id() != style::DEFAULT_ID).then(|| cell.style_id()),
                }
            })
            .collect()
    }

    fn hold_same_page_snapshot_refs(&self, snapshots: &[SourceCellSnapshot]) {
        for snapshot in snapshots {
            if let Some(id) = snapshot.hyperlink {
                self.use_hyperlink(id);
            }
            if let Some(id) = snapshot.style {
                self.use_style(id);
            }
        }
    }

    fn release_same_page_snapshot_refs(&mut self, snapshots: &[SourceCellSnapshot]) {
        for snapshot in snapshots {
            if let Some(id) = snapshot.hyperlink {
                self.release_hyperlink(id);
            }
            if let Some(id) = snapshot.style {
                self.release_style(id);
            }
        }
    }

    fn clone_rows_within_page(
        &mut self,
        src_y_start: usize,
        src_y_end: usize,
        dst_y_start: usize,
    ) -> Result<(), CloneFromError> {
        assert!(src_y_start <= src_y_end);
        assert!(src_y_end <= self.size.rows as usize);
        assert!(src_y_end - src_y_start <= self.size.rows as usize - dst_y_start);

        let snapshot = self.clone_page().map_err(|_| CloneFromError::PageAlloc)?;
        for (dst_y, src_y) in (src_y_start..src_y_end).enumerate() {
            self.clone_row_from(&snapshot, dst_y_start + dst_y, src_y)?;
        }

        Ok(())
    }

    fn move_cells(
        &mut self,
        src_y: usize,
        src_left: usize,
        dst_y: usize,
        dst_left: usize,
        len: usize,
    ) {
        assert!(src_y < self.size.rows as usize);
        assert!(dst_y < self.size.rows as usize);
        let src_end = src_left.checked_add(len).expect("source range overflow");
        let dst_end = dst_left
            .checked_add(len)
            .expect("destination range overflow");
        assert!(src_end <= self.size.cols as usize);
        assert!(dst_end <= self.size.cols as usize);

        if src_y == dst_y && len > 0 {
            assert!(
                src_end <= dst_left || dst_end <= src_left,
                "move_cells does not support same-row overlapping ranges"
            );
        }

        self.clear_cells(dst_y, dst_left, dst_end);

        let src_cells = self.get_row(src_y).cells();
        let dst_cells = self.get_row(dst_y).cells();
        for index in 0..len {
            let src_offset = self.cell_offset_from_row_cells(src_cells, src_left + index);
            let dst_offset = self.cell_offset_from_row_cells(dst_cells, dst_left + index);
            let cell = self.cell_copy_at_offset(src_offset);

            if cell.has_grapheme() {
                self.move_grapheme_at_offset(src_offset, dst_offset);
            }
            if cell.hyperlink() {
                self.move_hyperlink_entry(src_offset, dst_offset);
            }

            *self.cell_mut_at_offset(dst_offset) = cell;
        }

        for x in src_left..src_end {
            let offset = self.cell_offset_from_row_cells(src_cells, x);
            *self.cell_mut_at_offset(offset) = Cell::default();
        }

        self.update_row_grapheme_flag(src_y);
        self.update_row_hyperlink_flag(src_y);
        self.update_row_styled_flag(src_y);
        if dst_y != src_y {
            self.update_row_grapheme_flag(dst_y);
            self.update_row_hyperlink_flag(dst_y);
            self.update_row_styled_flag(dst_y);
        }
    }

    pub(super) fn clear_cells(&mut self, row_index: usize, left: usize, end: usize) {
        assert!(row_index < self.size.rows as usize);
        assert!(left <= end);
        assert!(end <= self.size.cols as usize);

        let row_cells = self.get_row(row_index).cells();
        for x in left..end {
            let offset = self.cell_offset_from_row_cells(row_cells, x);
            let cell = self.cell_copy_at_offset(offset);
            if cell.has_grapheme() {
                self.clear_grapheme_at_offset(offset);
            }
            if cell.hyperlink() {
                self.clear_hyperlink_at_offset(offset);
            }
            let style_id = cell.style_id();
            if style_id != style::DEFAULT_ID {
                self.release_style(style_id);
            }
            *self.cell_mut_at_offset(offset) = Cell::default();
        }

        self.update_row_grapheme_flag(row_index);
        self.update_row_hyperlink_flag(row_index);
        self.update_row_styled_flag(row_index);
    }

    pub(super) fn clear_unprotected_cells(&mut self, row_index: usize, left: usize, end: usize) {
        assert!(row_index < self.size.rows as usize);
        assert!(left <= end);
        assert!(end <= self.size.cols as usize);

        let row_cells = self.get_row(row_index).cells();
        for x in left..end {
            let offset = self.cell_offset_from_row_cells(row_cells, x);
            let cell = self.cell_copy_at_offset(offset);
            if cell.protected() {
                continue;
            }
            if cell.has_grapheme() {
                self.clear_grapheme_at_offset(offset);
            }
            if cell.hyperlink() {
                self.clear_hyperlink_at_offset(offset);
            }
            let style_id = cell.style_id();
            if style_id != style::DEFAULT_ID {
                self.release_style(style_id);
            }
            *self.cell_mut_at_offset(offset) = Cell::default();
        }

        self.update_row_grapheme_flag(row_index);
        self.update_row_hyperlink_flag(row_index);
        self.update_row_styled_flag(row_index);
    }

    pub(super) fn reset_cleared_row_metadata(&mut self, row_index: usize) {
        assert!(row_index < self.size.rows as usize);
        let cells = self.get_row(row_index).cells();
        let mut row = Row::default();
        row.set_cells(cells);
        row.set_dirty(true);
        *self.get_row_mut(row_index) = row;
    }

    pub(super) fn rotate_rows_left(&mut self, start: usize, end: usize) {
        assert!(start < end);
        assert!(end <= self.size.rows as usize);
        unsafe {
            // Safety: bounds were checked above and the rows region belongs to
            // this live page backing allocation.
            std::slice::from_raw_parts_mut(
                self.rows.ptr_mut(self.memory.as_mut_ptr()).add(start),
                end - start,
            )
            .rotate_left(1);
        }
    }

    fn move_grapheme_at(&mut self, src_x: usize, src_y: usize, dst_x: usize, dst_y: usize) {
        let src_offset = self.cell_offset_at(src_x, src_y);
        let dst_offset = self.cell_offset_at(dst_x, dst_y);
        self.move_grapheme_at_offset(src_offset, dst_offset);
    }

    fn move_grapheme_at_offset(&mut self, src_offset: Offset<Cell>, dst_offset: Offset<Cell>) {
        assert!(self.cell_copy_at_offset(src_offset).has_grapheme());
        assert!(!self.cell_copy_at_offset(dst_offset).has_grapheme());

        let Some(mut map) = self.grapheme_map_mut() else {
            panic!("grapheme cell must have map storage");
        };
        let (_, slice) = map
            .fetch_remove(src_offset)
            .expect("grapheme cell must have map data");
        map.put_assume_capacity_no_clobber(dst_offset, slice);
    }

    fn move_hyperlink_entry(&mut self, src_offset: Offset<Cell>, dst_offset: Offset<Cell>) {
        let Some(mut map) = self.hyperlink_map_mut() else {
            panic!("hyperlink cell must have map storage");
        };
        let (_, id) = map
            .fetch_remove(src_offset)
            .expect("hyperlink cell must have map data");
        map.put_assume_capacity_no_clobber(dst_offset, id);
    }

    fn swap_cells(&mut self, src_y: usize, src_x: usize, dst_y: usize, dst_x: usize) {
        assert!(src_y < self.size.rows as usize);
        assert!(dst_y < self.size.rows as usize);
        assert!(src_x < self.size.cols as usize);
        assert!(dst_x < self.size.cols as usize);

        if src_y == dst_y && src_x == dst_x {
            return;
        }

        let src_offset = self.cell_offset_at(src_x, src_y);
        let dst_offset = self.cell_offset_at(dst_x, dst_y);
        let src_cell = self.cell_copy_at_offset(src_offset);
        let dst_cell = self.cell_copy_at_offset(dst_offset);

        self.swap_grapheme_entries(src_offset, src_cell, dst_offset, dst_cell);
        self.swap_hyperlink_entries(src_offset, src_cell, dst_offset, dst_cell);

        *self.cell_mut_at_offset(src_offset) = dst_cell;
        *self.cell_mut_at_offset(dst_offset) = src_cell;

        self.update_row_grapheme_flag(src_y);
        self.update_row_hyperlink_flag(src_y);
        self.update_row_styled_flag(src_y);
        if dst_y != src_y {
            self.update_row_grapheme_flag(dst_y);
            self.update_row_hyperlink_flag(dst_y);
            self.update_row_styled_flag(dst_y);
        }
    }

    pub(super) fn delete_chars_in_row(
        &mut self,
        row_index: usize,
        left: usize,
        right: usize,
        count: usize,
    ) {
        assert!(row_index < self.size.rows as usize);
        assert!(left <= right);
        assert!(right < self.size.cols as usize);
        assert!(count > 0);

        let width = right - left + 1;
        let count = count.min(width);
        let shift = width - count;
        for offset in 0..shift {
            self.swap_cells(row_index, left + count + offset, row_index, left + offset);
        }

        self.clear_cells(row_index, left + shift, right + 1);
    }

    pub(super) fn insert_chars_in_row(
        &mut self,
        row_index: usize,
        left: usize,
        right: usize,
        count: usize,
    ) {
        assert!(row_index < self.size.rows as usize);
        assert!(left <= right);
        assert!(right < self.size.cols as usize);
        assert!(count > 0);

        let width = right - left + 1;
        let count = count.min(width);
        let shift = width - count;
        for offset in (0..shift).rev() {
            self.swap_cells(row_index, left + offset, row_index, left + count + offset);
        }

        self.clear_cells(row_index, left, left + count);
    }

    fn swap_grapheme_entries(
        &mut self,
        src_offset: Offset<Cell>,
        src_cell: Cell,
        dst_offset: Offset<Cell>,
        dst_cell: Cell,
    ) {
        match (src_cell.has_grapheme(), dst_cell.has_grapheme()) {
            (false, false) => {}
            (true, false) => self.move_grapheme_at_offset(src_offset, dst_offset),
            (false, true) => self.move_grapheme_at_offset(dst_offset, src_offset),
            (true, true) => {
                let Some(mut map) = self.grapheme_map_mut() else {
                    panic!("grapheme cell must have map storage");
                };
                let src_slice = *map
                    .get_mut(src_offset)
                    .expect("source grapheme cell must have map data");
                let dst_slice = *map
                    .get_mut(dst_offset)
                    .expect("destination grapheme cell must have map data");
                *map.get_mut(src_offset)
                    .expect("source grapheme cell must have map data") = dst_slice;
                *map.get_mut(dst_offset)
                    .expect("destination grapheme cell must have map data") = src_slice;
            }
        }
    }

    fn swap_hyperlink_entries(
        &mut self,
        src_offset: Offset<Cell>,
        src_cell: Cell,
        dst_offset: Offset<Cell>,
        dst_cell: Cell,
    ) {
        match (src_cell.hyperlink(), dst_cell.hyperlink()) {
            (false, false) => {}
            (true, false) => self.move_hyperlink_entry(src_offset, dst_offset),
            (false, true) => self.move_hyperlink_entry(dst_offset, src_offset),
            (true, true) => {
                let Some(mut map) = self.hyperlink_map_mut() else {
                    panic!("hyperlink cell must have map storage");
                };
                let src_id = *map
                    .get_mut(src_offset)
                    .expect("source hyperlink cell must have map data");
                let dst_id = *map
                    .get_mut(dst_offset)
                    .expect("destination hyperlink cell must have map data");
                *map.get_mut(src_offset)
                    .expect("source hyperlink cell must have map data") = dst_id;
                *map.get_mut(dst_offset)
                    .expect("destination hyperlink cell must have map data") = src_id;
            }
        }
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
        let cell_offset = self.cell_offset_at(x, y);
        let cell = self.cell_copy_at_offset(cell_offset);
        assert!(cell.codepoint() != 0);

        self.append_grapheme_at_offset(cell_offset, cp)?;
        self.get_row_mut(y).set_grapheme(true);
        Ok(())
    }

    fn set_graphemes_at(&mut self, x: usize, y: usize, cps: &[u32]) -> Result<(), GraphemeError> {
        assert!(!cps.is_empty());

        let cell_offset = self.cell_offset_at(x, y);
        self.set_graphemes_at_offset(y, cell_offset, cps)
    }

    fn set_graphemes_at_offset(
        &mut self,
        row_index: usize,
        cell_offset: Offset<Cell>,
        cps: &[u32],
    ) -> Result<(), GraphemeError> {
        assert!(!cps.is_empty());
        for cp in cps {
            assert!(*cp <= 0x10ffff);
        }
        debug_assert!(row_index < self.size.rows as usize);
        let cell = self.cell_copy_at_offset(cell_offset);
        assert!(cell.codepoint() > 0);
        assert_eq!(cell.content_tag(), ContentTag::Codepoint);

        let slice = self.alloc_grapheme_slice(cps.len())?;
        unsafe {
            // Safety: `slice` was just allocated from this page and is
            // uniquely owned until inserted into the grapheme map below.
            slice
                .slice_mut(self.memory.as_mut_ptr())
                .copy_from_slice(cps);
        }

        let inserted = match self.grapheme_map_mut() {
            Some(mut map) => map.put_no_clobber(cell_offset, slice),
            None => Err(offset_hash_map::Error::OutOfMemory),
        };
        if inserted.is_err() {
            self.free_grapheme_slice(slice);
            return Err(GraphemeError::GraphemeMapOutOfMemory);
        }

        self.cell_mut_at_offset(cell_offset)
            .set_content_tag(ContentTag::CodepointGrapheme);
        self.get_row_mut(row_index).set_grapheme(true);
        Ok(())
    }

    fn append_grapheme_at_offset(
        &mut self,
        cell_offset: Offset<Cell>,
        cp: u32,
    ) -> Result<(), GraphemeError> {
        assert!(cp <= 0x10ffff);
        let cell = self.cell_copy_at_offset(cell_offset);
        assert!(cell.codepoint() != 0);

        if !cell.has_grapheme() {
            return self.append_first_grapheme_at_offset(cell_offset, cp);
        }

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
        self.lookup_grapheme_at_offset(cell_offset)
    }

    fn lookup_grapheme_at_offset(&self, cell_offset: Offset<Cell>) -> Option<Vec<u32>> {
        let slice = self.grapheme_map_ref()?.get(cell_offset)?;
        Some(unsafe {
            // Safety: `slice` came from this page's grapheme map.
            slice.slice(self.memory.as_ptr()).to_vec()
        })
    }

    pub(super) fn clear_grapheme_at(&mut self, x: usize, y: usize) {
        assert!(self.cell_copy_at(x, y).has_grapheme());
        let cell_offset = self.cell_offset_at(x, y);
        self.clear_grapheme_at_offset(cell_offset);
    }

    fn clear_grapheme_at_offset(&mut self, cell_offset: Offset<Cell>) {
        assert!(self.cell_copy_at_offset(cell_offset).has_grapheme());
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
        unsafe {
            // Safety: offset came from this page's valid cell range.
            (*cell_offset.ptr_mut(self.memory.as_mut_ptr())).set_content_tag(ContentTag::Codepoint);
        }
    }

    pub(super) fn update_row_grapheme_flag(&mut self, row_index: usize) {
        let has_grapheme = self
            .get_cells(self.get_row(row_index))
            .iter()
            .any(|cell| cell.has_grapheme());
        self.get_row_mut(row_index).set_grapheme(has_grapheme);
    }

    pub(super) fn update_row_styled_flag(&mut self, row_index: usize) {
        let has_styling = self
            .get_cells(self.get_row(row_index))
            .iter()
            .any(|cell| cell.has_styling());
        self.get_row_mut(row_index).set_styled(has_styling);
    }

    pub(super) fn write_print_cell(
        &mut self,
        x: usize,
        y: usize,
        codepoint: char,
        cell_style: style::Style,
        cell_hyperlink: Option<hyperlink::Hyperlink<'_>>,
        semantic_content: SemanticContent,
    ) -> Result<(), PrintCellError> {
        let cell_offset = self.cell_offset_at(x, y);
        let old_cell = self.cell_copy_at_offset(cell_offset);
        if cell_has_unsupported_print_replace_state(old_cell) {
            return Err(PrintCellError::UnsupportedManagedCell);
        }

        let new_style_id = if cell_style.is_default() {
            style::DEFAULT_ID
        } else {
            self.add_style(cell_style)
                .map_err(|_| PrintCellError::StyleOutOfMemory)?
        };

        let new_hyperlink_id = match cell_hyperlink {
            Some(link) => match self.insert_hyperlink(link) {
                Ok(id) => Some(id),
                Err(_) => {
                    if new_style_id != style::DEFAULT_ID {
                        self.release_style(new_style_id);
                    }
                    return Err(PrintCellError::HyperlinkOutOfMemory);
                }
            },
            None => None,
        };

        let old_hyperlink_id = match self.prepare_print_hyperlink(cell_offset, new_hyperlink_id) {
            Ok(id) => id,
            Err(err) => {
                if let Some(id) = new_hyperlink_id {
                    self.release_hyperlink(id);
                }
                if new_style_id != style::DEFAULT_ID {
                    self.release_style(new_style_id);
                }
                return Err(err);
            }
        };
        let old_style_id = old_cell.style_id();

        {
            let rac = self.get_row_and_cell_mut(x, y);
            *rac.cell = Cell::init(codepoint as u32);
            rac.cell.set_style_id(new_style_id);
            rac.cell.set_hyperlink(new_hyperlink_id.is_some());
            rac.cell.set_semantic_content(semantic_content);
            rac.row.set_dirty(true);
        }

        if old_style_id != style::DEFAULT_ID {
            self.release_style(old_style_id);
        }
        if let Some(id) = old_hyperlink_id {
            self.release_hyperlink(id);
        }

        self.update_row_styled_flag(y);
        self.update_row_hyperlink_flag(y);
        Ok(())
    }

    fn prepare_print_hyperlink(
        &mut self,
        cell_offset: Offset<Cell>,
        new_hyperlink_id: Option<hyperlink::Id>,
    ) -> Result<Option<hyperlink::Id>, PrintCellError> {
        match new_hyperlink_id {
            Some(id) => {
                let Some(mut map) = self.hyperlink_map_mut() else {
                    return Err(PrintCellError::HyperlinkOutOfMemory);
                };
                let result = map
                    .get_or_put(cell_offset)
                    .map_err(|_| PrintCellError::HyperlinkOutOfMemory)?;
                let old = result.found_existing.then_some(*result.value);
                *result.value = id;
                Ok(old)
            }
            None => {
                let Some(mut map) = self.hyperlink_map_mut() else {
                    return Ok(None);
                };
                Ok(map.fetch_remove(cell_offset).map(|(_, id)| id))
            }
        }
    }

    pub(super) fn insert_hyperlink(
        &mut self,
        link: hyperlink::Hyperlink<'_>,
    ) -> Result<hyperlink::Id, InsertHyperlinkError> {
        let uri = self.alloc_string_slice(link.uri)?;
        let page_id = match link.id {
            hyperlink::HyperlinkId::Implicit(id) => hyperlink::PageEntryId::implicit(id),
            hyperlink::HyperlinkId::Explicit(id) => {
                let id = match self.alloc_string_slice(id) {
                    Ok(id) => id,
                    Err(err) => {
                        self.free_string_slice(uri);
                        return Err(err);
                    }
                };
                hyperlink::PageEntryId::explicit(id)
            }
        };
        let entry = hyperlink::PageEntry::new(page_id, uri);

        let base = unsafe {
            // Safety: hyperlink_set_start is inside this page's live backing
            // memory.
            self.memory
                .as_mut_ptr()
                .add(self.layout.hyperlink_set_start)
        };
        let mut context =
            HyperlinkSetContext::new(self.memory.as_mut_ptr(), &mut self.string_alloc);
        match self.hyperlink_set.add(base, entry, &mut context) {
            Ok(id) => Ok(id),
            Err(ref_counted_set::AddError::OutOfMemory) => {
                self.free_hyperlink_entry(entry);
                Err(InsertHyperlinkError::SetOutOfMemory)
            }
            Err(ref_counted_set::AddError::NeedsRehash) => {
                self.free_hyperlink_entry(entry);
                Err(InsertHyperlinkError::SetNeedsRehash)
            }
        }
    }

    pub(super) fn lookup_hyperlink_at(&self, x: usize, y: usize) -> Option<hyperlink::Id> {
        self.lookup_hyperlink_at_offset(self.cell_offset_at(x, y))
    }

    fn lookup_hyperlink_at_offset(&self, cell_offset: Offset<Cell>) -> Option<hyperlink::Id> {
        self.hyperlink_map_ref()?.get(cell_offset)
    }

    pub(super) fn set_hyperlink(
        &mut self,
        x: usize,
        y: usize,
        id: hyperlink::Id,
    ) -> Result<(), HyperlinkError> {
        let cell_offset = self.cell_offset_at(x, y);
        self.set_hyperlink_at_offset(y, cell_offset, id)
    }

    fn set_hyperlink_at_offset(
        &mut self,
        row_index: usize,
        cell_offset: Offset<Cell>,
        id: hyperlink::Id,
    ) -> Result<(), HyperlinkError> {
        let Some(_) = self.hyperlink_map else {
            return Err(HyperlinkError::HyperlinkMapOutOfMemory);
        };

        let existing = {
            let mut map = self.hyperlink_map_mut().expect("hyperlink map must exist");
            let result = map
                .get_or_put(cell_offset)
                .map_err(|_| HyperlinkError::HyperlinkMapOutOfMemory)?;
            let existing = result.found_existing.then_some(*result.value);
            *result.value = id;
            existing
        };

        if let Some(old_id) = existing {
            self.release_hyperlink(old_id);
            if old_id == id {
                self.cell_mut_at_offset(cell_offset).set_hyperlink(true);
                assert!(self.get_row(row_index).hyperlink());
                return Ok(());
            }
        }

        self.cell_mut_at_offset(cell_offset).set_hyperlink(true);
        self.get_row_mut(row_index).set_hyperlink(true);
        Ok(())
    }

    pub(super) fn clear_hyperlink(&mut self, x: usize, y: usize) {
        let cell_offset = self.cell_offset_at(x, y);
        self.clear_hyperlink_at_offset(cell_offset);
    }

    fn clear_hyperlink_at_offset(&mut self, cell_offset: Offset<Cell>) {
        let Some(mut map) = self.hyperlink_map_mut() else {
            return;
        };
        let Some((_, id)) = map.fetch_remove(cell_offset) else {
            return;
        };
        drop(map);

        self.release_hyperlink(id);
        self.cell_mut_at_offset(cell_offset).set_hyperlink(false);
    }

    pub(super) fn update_row_hyperlink_flag(&mut self, row_index: usize) {
        let has_hyperlink = self
            .get_cells(self.get_row(row_index))
            .iter()
            .any(|cell| cell.hyperlink());
        self.get_row_mut(row_index).set_hyperlink(has_hyperlink);
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

    pub(super) fn hyperlink_count(&self) -> usize {
        self.hyperlink_map_ref()
            .map(|map| map.count() as usize)
            .unwrap_or(0)
    }

    pub(super) fn hyperlink_capacity(&self) -> usize {
        self.hyperlink_map_ref()
            .map(|map| map.capacity() as usize)
            .unwrap_or(0)
    }

    pub(super) fn get_hyperlink(&self, id: hyperlink::Id) -> HyperlinkSnapshot {
        let entry = *self.hyperlink_set.get(self.hyperlink_set_base(), id);
        self.hyperlink_snapshot(entry)
    }

    pub(super) fn use_hyperlink(&self, id: hyperlink::Id) {
        self.hyperlink_set.use_one(self.hyperlink_set_base(), id);
    }

    pub(super) fn release_hyperlink(&mut self, id: hyperlink::Id) {
        let base = unsafe {
            // Safety: hyperlink_set_start is inside this page's live backing
            // memory.
            self.memory
                .as_mut_ptr()
                .add(self.layout.hyperlink_set_start)
        };
        self.hyperlink_set.release(base, id);
    }

    fn prepare_cloned_hyperlink(
        &mut self,
        source: &Page,
        source_id: hyperlink::Id,
    ) -> Result<hyperlink::Id, CloneFromError> {
        if self.hyperlink_count() >= self.hyperlink_capacity() {
            return Err(CloneFromError::HyperlinkMapOutOfMemory);
        }

        let source_entry = *source
            .hyperlink_set
            .get(source.hyperlink_set_base(), source_id);
        if let Some(id) = self.lookup_hyperlink_entry_from(source, source_entry) {
            self.use_hyperlink(id);
            return Ok(id);
        }

        let entry = self.dupe_hyperlink_entry_from(source, source_entry)?;
        self.add_hyperlink_entry_with_id(entry, source_id)
    }

    fn lookup_hyperlink_entry_from(
        &self,
        source: &Page,
        entry: hyperlink::PageEntry,
    ) -> Option<hyperlink::Id> {
        let context =
            HyperlinkSetContext::with_source(self.memory.as_ptr(), source.memory.as_ptr());
        self.hyperlink_set
            .lookup(self.hyperlink_set_base(), entry, &context)
    }

    fn add_hyperlink_entry_with_id(
        &mut self,
        entry: hyperlink::PageEntry,
        preferred_id: hyperlink::Id,
    ) -> Result<hyperlink::Id, CloneFromError> {
        let base = unsafe {
            // Safety: hyperlink_set_start is inside this page's live backing
            // memory.
            self.memory
                .as_mut_ptr()
                .add(self.layout.hyperlink_set_start)
        };
        let mut context =
            HyperlinkSetContext::new(self.memory.as_mut_ptr(), &mut self.string_alloc);
        match self
            .hyperlink_set
            .add_with_id(base, entry, preferred_id, &mut context)
        {
            Ok(Some(id)) => Ok(id),
            Ok(None) => Ok(preferred_id),
            Err(ref_counted_set::AddError::OutOfMemory) => {
                self.free_hyperlink_entry(entry);
                Err(CloneFromError::HyperlinkSetOutOfMemory)
            }
            Err(ref_counted_set::AddError::NeedsRehash) => {
                self.free_hyperlink_entry(entry);
                Err(CloneFromError::HyperlinkSetNeedsRehash)
            }
        }
    }

    fn dupe_hyperlink_entry_from(
        &mut self,
        source: &Page,
        entry: hyperlink::PageEntry,
    ) -> Result<hyperlink::PageEntry, CloneFromError> {
        let uri = self
            .alloc_string_slice(source.hyperlink_bytes(entry.uri()))
            .map_err(|_| CloneFromError::StringAllocOutOfMemory)?;
        let id = match entry.id().tag() {
            hyperlink::PageEntryIdTag::Implicit => {
                hyperlink::PageEntryId::implicit(entry.id().implicit_value())
            }
            hyperlink::PageEntryIdTag::Explicit => {
                let explicit = match self
                    .alloc_string_slice(source.hyperlink_bytes(entry.id().explicit_value()))
                {
                    Ok(explicit) => explicit,
                    Err(_) => {
                        self.free_string_slice(uri);
                        return Err(CloneFromError::StringAllocOutOfMemory);
                    }
                };
                hyperlink::PageEntryId::explicit(explicit)
            }
        };

        Ok(hyperlink::PageEntry::new(id, uri))
    }

    pub(super) fn hyperlink_ref_count(&self, id: hyperlink::Id) -> hyperlink::Id {
        self.hyperlink_set.ref_count(self.hyperlink_set_base(), id)
    }

    pub(super) fn hyperlink_set_count(&self) -> usize {
        self.hyperlink_set.count()
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

    pub(super) fn add_style_with_id(
        &mut self,
        style: style::Style,
        id: style::Id,
    ) -> Result<style::Id, super::ref_counted_set::AddError> {
        let base = unsafe {
            // Safety: styles_start is inside this page's live backing memory.
            self.memory.as_mut_ptr().add(self.layout.styles_start)
        };
        Ok(self.styles.add_with_id(base, style, id)?.unwrap_or(id))
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

    #[cfg(test)]
    fn string_used_bytes(&self) -> usize {
        unsafe {
            // Safety: this page initialized string_alloc with its backing
            // memory and the allocation is still live.
            self.string_alloc.used_bytes(self.memory.as_ptr())
        }
    }

    fn append_first_grapheme_at_offset(
        &mut self,
        cell_offset: Offset<Cell>,
        cp: u32,
    ) -> Result<(), GraphemeError> {
        let Some(_) = self.grapheme_map else {
            return Err(GraphemeError::GraphemeMapOutOfMemory);
        };

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

        unsafe {
            // Safety: offset came from this page's valid cell range.
            (*cell_offset.ptr_mut(self.memory.as_mut_ptr()))
                .set_content_tag(ContentTag::CodepointGrapheme);
        }
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

    fn alloc_string_slice(
        &mut self,
        bytes: &[u8],
    ) -> Result<OffsetSlice<u8>, InsertHyperlinkError> {
        if bytes.is_empty() {
            return Ok(OffsetSlice::default());
        }

        let slice = unsafe {
            // Safety: this page initialized string_alloc with its backing
            // memory and the allocation is uniquely borrowed through &mut self.
            self.string_alloc
                .alloc::<u8, _>(self.memory.as_mut_ptr(), bytes.len())
        }
        .map_err(|_| InsertHyperlinkError::StringsOutOfMemory)?;
        slice.copy_from_slice(bytes);
        let offset = get_offset(self.memory.as_ptr(), slice.as_ptr());
        Ok(OffsetSlice::new(offset, slice.len()))
    }

    fn free_string_slice(&mut self, slice: OffsetSlice<u8>) {
        if slice.len() == 0 {
            return;
        }

        let slice = unsafe {
            // Safety: `slice` came from this page's string allocator and is no
            // longer referenced by any live hyperlink entry at call sites.
            slice.slice_mut(self.memory.as_mut_ptr())
        };
        unsafe {
            // Safety: the slice was allocated by this allocator from this page
            // backing memory and is being freed exactly once.
            self.string_alloc.free(self.memory.as_mut_ptr(), slice);
        }
    }

    fn free_hyperlink_entry(&mut self, entry: hyperlink::PageEntry) {
        match entry.id().tag() {
            hyperlink::PageEntryIdTag::Implicit => {}
            hyperlink::PageEntryIdTag::Explicit => {
                self.free_string_slice(entry.id().explicit_value());
            }
        }
        self.free_string_slice(entry.uri());
    }

    fn hyperlink_bytes(&self, slice: OffsetSlice<u8>) -> &[u8] {
        hyperlink_bytes_from(self.memory.as_ptr(), slice)
    }

    fn hyperlink_snapshot(&self, entry: hyperlink::PageEntry) -> HyperlinkSnapshot {
        let id = match entry.id().tag() {
            hyperlink::PageEntryIdTag::Implicit => {
                HyperlinkSnapshotId::Implicit(entry.id().implicit_value())
            }
            hyperlink::PageEntryIdTag::Explicit => HyperlinkSnapshotId::Explicit(
                self.hyperlink_bytes(entry.id().explicit_value()).to_vec(),
            ),
        };
        HyperlinkSnapshot {
            id,
            uri: self.hyperlink_bytes(entry.uri()).to_vec(),
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

    fn hyperlink_map_ref(&self) -> Option<offset_hash_map::MapRef<'_, Offset<Cell>, HyperlinkId>> {
        self.hyperlink_map
            .as_ref()
            .map(|map| map.map_ref(self.memory.as_slice()))
    }

    fn hyperlink_map_mut(&mut self) -> Option<offset_hash_map::Map<'_, Offset<Cell>, HyperlinkId>> {
        let map = self.hyperlink_map?;
        Some(map.map(self.memory.as_mut_slice()))
    }

    fn style_base(&self) -> *const u8 {
        unsafe {
            // Safety: styles_start is produced by Page layout and is inside
            // this page's live backing memory.
            self.memory.as_ptr().add(self.layout.styles_start)
        }
    }

    fn hyperlink_set_base(&self) -> *const u8 {
        unsafe {
            // Safety: hyperlink_set_start is produced by Page layout and is
            // inside this page's live backing memory.
            self.memory.as_ptr().add(self.layout.hyperlink_set_start)
        }
    }

    fn cell_offset_at(&self, x: usize, y: usize) -> Offset<Cell> {
        assert!(y < self.size.rows as usize);
        assert!(x < self.size.cols as usize);
        self.cell_offset_from_row_cells(self.get_row(y).cells(), x)
    }

    fn row_cell_offset(&self, row: &Row, x: usize) -> Offset<Cell> {
        assert!(x < self.size.cols as usize);
        self.assert_row_provenance(row);
        self.cell_offset_from_row_cells(row.cells(), x)
    }

    fn cell_offset_from_row_cells(&self, cells: Offset<Cell>, x: usize) -> Offset<Cell> {
        assert!(x < self.size.cols as usize);
        self.assert_cells_range(cells);
        Offset::new(
            (cells.offset() as usize + x * size_of::<Cell>())
                .try_into()
                .expect("cell offset must fit OffsetInt"),
        )
    }

    pub(super) fn cell_copy_at(&self, x: usize, y: usize) -> Cell {
        self.cell_copy_at_offset(self.cell_offset_at(x, y))
    }

    fn cell_copy_at_offset(&self, offset: Offset<Cell>) -> Cell {
        unsafe {
            // Safety: call sites derive offsets from checked page cell ranges.
            *offset.ptr(self.memory.as_ptr())
        }
    }

    fn cell_mut_at(&mut self, x: usize, y: usize) -> &mut Cell {
        unsafe {
            // Safety: bounds are checked by cell_offset_at, and &mut self
            // guarantees exclusive access.
            &mut *self.cell_offset_at(x, y).ptr_mut(self.memory.as_mut_ptr())
        }
    }

    fn cell_mut_at_offset(&mut self, offset: Offset<Cell>) -> &mut Cell {
        unsafe {
            // Safety: call sites derive offsets from checked page cell ranges.
            &mut *offset.ptr_mut(self.memory.as_mut_ptr())
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceCellSnapshot {
    cell: Cell,
    graphemes: Vec<u32>,
    hyperlink: Option<hyperlink::Id>,
    style: Option<style::Id>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HyperlinkSnapshot {
    pub(super) id: HyperlinkSnapshotId,
    pub(super) uri: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum HyperlinkSnapshotId {
    Explicit(Vec<u8>),
    Implicit(u32),
}

struct HyperlinkSetContext {
    page_memory: *mut u8,
    src_memory: *const u8,
    string_alloc: *mut StringAlloc,
}

impl HyperlinkSetContext {
    fn new(page_memory: *mut u8, string_alloc: &mut StringAlloc) -> Self {
        Self {
            page_memory,
            src_memory: std::ptr::null(),
            string_alloc,
        }
    }

    fn with_source(page_memory: *const u8, src_memory: *const u8) -> Self {
        Self {
            page_memory: page_memory.cast_mut(),
            src_memory,
            string_alloc: std::ptr::null_mut(),
        }
    }

    fn page_memory(&self) -> *const u8 {
        self.page_memory
    }

    fn candidate_memory(&self) -> *const u8 {
        if self.src_memory.is_null() {
            self.page_memory()
        } else {
            self.src_memory
        }
    }
}

impl ref_counted_set::Context<hyperlink::PageEntry> for HyperlinkSetContext {
    fn hash(&self, value: hyperlink::PageEntry) -> u64 {
        hash_hyperlink_entry(self.candidate_memory(), value)
    }

    fn eql(&self, candidate: hyperlink::PageEntry, resident: hyperlink::PageEntry) -> bool {
        hyperlink_entry_eq(
            self.candidate_memory(),
            candidate,
            self.page_memory(),
            resident,
        )
    }

    fn deleted(&mut self, value: hyperlink::PageEntry) {
        assert!(!self.string_alloc.is_null());
        free_hyperlink_entry_from(self.page_memory, self.string_alloc, value);
    }
}

fn hash_hyperlink_entry(base: *const u8, entry: hyperlink::PageEntry) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    fnv1a64_write(&mut hash, &[entry.id().tag() as u8]);
    match entry.id().tag() {
        hyperlink::PageEntryIdTag::Implicit => {
            fnv1a64_write(&mut hash, &entry.id().implicit_value().to_le_bytes());
        }
        hyperlink::PageEntryIdTag::Explicit => {
            fnv1a64_write(
                &mut hash,
                hyperlink_bytes_from(base, entry.id().explicit_value()),
            );
        }
    }
    fnv1a64_write(&mut hash, hyperlink_bytes_from(base, entry.uri()));
    hash
}

fn hyperlink_entry_eq(
    candidate_base: *const u8,
    candidate: hyperlink::PageEntry,
    resident_base: *const u8,
    resident: hyperlink::PageEntry,
) -> bool {
    if candidate.id().tag() != resident.id().tag() {
        return false;
    }

    match candidate.id().tag() {
        hyperlink::PageEntryIdTag::Implicit => {
            if candidate.id().implicit_value() != resident.id().implicit_value() {
                return false;
            }
        }
        hyperlink::PageEntryIdTag::Explicit => {
            if hyperlink_bytes_from(candidate_base, candidate.id().explicit_value())
                != hyperlink_bytes_from(resident_base, resident.id().explicit_value())
            {
                return false;
            }
        }
    }

    hyperlink_bytes_from(candidate_base, candidate.uri())
        == hyperlink_bytes_from(resident_base, resident.uri())
}

fn hyperlink_bytes_from<'a>(base: *const u8, slice: OffsetSlice<u8>) -> &'a [u8] {
    if slice.len() == 0 {
        return &[];
    }

    unsafe {
        // Safety: callers only pass slices allocated from the page memory base
        // for the active hyperlink entry.
        slice.slice(base)
    }
}

fn free_hyperlink_entry_from(
    base: *mut u8,
    string_alloc: *mut StringAlloc,
    entry: hyperlink::PageEntry,
) {
    match entry.id().tag() {
        hyperlink::PageEntryIdTag::Implicit => {}
        hyperlink::PageEntryIdTag::Explicit => {
            free_string_slice_from(base, string_alloc, entry.id().explicit_value());
        }
    }
    free_string_slice_from(base, string_alloc, entry.uri());
}

fn free_string_slice_from(base: *mut u8, string_alloc: *mut StringAlloc, slice: OffsetSlice<u8>) {
    if slice.len() == 0 {
        return;
    }

    let slice = unsafe {
        // Safety: callers only pass slices allocated from this string
        // allocator and base pair.
        slice.slice_mut(base)
    };
    unsafe {
        // Safety: string_alloc is the allocator that produced this slice, and
        // the slice is being released exactly once.
        (*string_alloc).free(base, slice);
    }
}

fn fnv1a64_write(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= *byte as u64;
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
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

impl From<HyperlinkSetLayout> for ref_counted_set::Layout {
    fn from(value: HyperlinkSetLayout) -> Self {
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

fn cell_has_unsupported_print_replace_state(cell: Cell) -> bool {
    !matches!(cell.content_tag(), ContentTag::Codepoint)
        || cell.has_grapheme()
        || !matches!(cell.wide(), Wide::Narrow)
        || cell.protected()
        || !matches!(cell.semantic_content(), SemanticContent::Output)
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

    const fn unsupported_clone_managed_memory(self) -> bool {
        self.hyperlink()
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
    use crate::terminal::sgr::Underline;
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
        assert_eq!(size_of::<hyperlink::PageEntry>(), HYPERLINK_PAGE_ENTRY_SIZE);
        assert_eq!(
            align_of::<hyperlink::PageEntry>(),
            HYPERLINK_PAGE_ENTRY_ALIGN
        );
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
    fn page_reinit_reuses_backing_and_resets_cells() {
        let capacity = Capacity {
            cols: 6,
            rows: 4,
            styles: 8,
            ..Capacity::new(6, 4)
        };
        let mut page = Page::init(capacity).unwrap();
        let backing_ptr = page.backing_ptr();
        let backing_len = page.backing_len();
        let backing_capacity = page.capacity();

        for y in 0..page.size.rows as usize {
            for x in 0..page.size.cols as usize {
                *page.get_row_and_cell_mut(x, y).cell = Cell::init((x + y + 1) as u32);
            }
        }
        {
            let row = page.get_row_mut(1);
            row.set_wrap(true);
            row.set_dirty(true);
        }
        page.dirty = true;
        page.size = Size { cols: 3, rows: 2 };

        page.reinit();

        assert_eq!(page.backing_ptr(), backing_ptr);
        assert_eq!(page.backing_len(), backing_len);
        assert_eq!(page.capacity(), backing_capacity);
        assert_eq!(page.size, Size { cols: 6, rows: 4 });
        assert!(!page.is_dirty());

        for y in 0..page.capacity.rows as usize {
            let row = page.get_row(y);
            let expected =
                page.layout.cells_start + y * page.capacity.cols as usize * size_of::<Cell>();
            assert_eq!(row.cells().offset() as usize, expected);
            assert_eq!(row.cval() & !Row::CELLS_MASK, 0);
            for cell in page.get_cells(row) {
                assert_eq!(*cell, Cell::default());
            }
        }
    }

    #[test]
    fn page_reinit_resets_managed_memory() {
        let mut page = Page::init(Capacity {
            cols: 6,
            rows: 2,
            styles: 8,
            ..Capacity::new(6, 2)
        })
        .unwrap();
        let style_id = page
            .add_style(style::Style {
                flags: style::Flags {
                    italic: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            })
            .unwrap();
        let link_id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"id"),
                uri: b"https://example.com/reinit",
            })
            .unwrap();

        let rac = page.get_row_and_cell_mut(0, 0);
        *rac.cell = Cell::init('s' as u32);
        rac.row.set_styled(true);
        rac.cell.set_style_id(style_id);
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('g' as u32);
        page.append_grapheme_at(1, 0, 0x0301).unwrap();
        *page.get_row_and_cell_mut(2, 0).cell = Cell::init('h' as u32);
        page.set_hyperlink(2, 0, link_id).unwrap();

        assert_eq!(page.style_count(), 1);
        assert_eq!(page.grapheme_count(), 1);
        assert_eq!(page.hyperlink_count(), 1);
        assert_eq!(page.hyperlink_set_count(), 1);
        assert!(page.grapheme_used_bytes() > 0);
        assert!(page.string_used_bytes() > 0);
        assert!(page.get_row(0).managed_memory());

        page.reinit();

        assert_eq!(page.style_count(), 0);
        assert_eq!(page.grapheme_count(), 0);
        assert_eq!(page.hyperlink_count(), 0);
        assert_eq!(page.hyperlink_set_count(), 0);
        assert_eq!(page.grapheme_used_bytes(), 0);
        assert_eq!(page.string_used_bytes(), 0);
        assert!(!page.get_row(0).managed_memory());
        assert_eq!(page.lookup_grapheme_at(1, 0), None);
        assert_eq!(page.lookup_hyperlink_at(2, 0), None);
        for cell in page.get_cells(page.get_row(0)) {
            assert_eq!(*cell, Cell::default());
        }
    }

    #[test]
    fn page_reinit_resets_dirty_and_row_metadata() {
        let mut page = Page::init(Capacity::new(4, 2)).unwrap();
        page.dirty = true;
        {
            let row = page.get_row_mut(0);
            row.set_wrap(true);
            row.set_wrap_continuation(true);
            row.set_grapheme(true);
            row.set_styled(true);
            row.set_hyperlink(true);
            row.set_semantic_prompt(SemanticPrompt::PromptContinuation);
            row.set_kitty_virtual_placeholder(true);
            row.set_dirty(true);
        }

        page.reinit();

        assert!(!page.is_dirty());
        let row = page.get_row(0);
        assert!(!row.wrap());
        assert!(!row.wrap_continuation());
        assert!(!row.grapheme());
        assert!(!row.styled());
        assert!(!row.hyperlink());
        assert_eq!(row.semantic_prompt(), SemanticPrompt::None);
        assert!(!row.kitty_virtual_placeholder());
        assert!(!row.dirty());
    }

    #[test]
    fn page_reinit_page_remains_usable() {
        let mut page = Page::init(Capacity {
            cols: 5,
            rows: 2,
            styles: 8,
            ..Capacity::new(5, 2)
        })
        .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('x' as u32);
        page.append_grapheme_at(0, 0, 0x0301).unwrap();
        page.reinit();

        let style_id = page
            .add_style(style::Style {
                flags: style::Flags {
                    underline: Underline::Single,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            })
            .unwrap();
        let link_id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(7),
                uri: b"https://example.com/after-reinit",
            })
            .unwrap();

        let rac = page.get_row_and_cell_mut(1, 0);
        *rac.cell = Cell::init('a' as u32);
        rac.row.set_styled(true);
        rac.cell.set_style_id(style_id);
        page.use_style(style_id);
        page.append_grapheme_at(1, 0, 0x0301).unwrap();
        page.set_hyperlink(1, 0, link_id).unwrap();

        assert_eq!(page.style_count(), 1);
        assert_eq!(page.style_ref_count(style_id), 2);
        assert_eq!(page.grapheme_count(), 1);
        assert_eq!(page.lookup_grapheme_at(1, 0), Some(vec![0x0301]));
        assert_eq!(page.hyperlink_count(), 1);
        assert_eq!(page.hyperlink_set_count(), 1);
        assert_eq!(page.hyperlink_ref_count(link_id), 1);
        assert_eq!(page.lookup_hyperlink_at(1, 0), Some(link_id));
        assert!(page.get_row(0).managed_memory());
    }

    #[test]
    fn page_verify_integrity_fresh_and_reinit_pages() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();

        assert_eq!(page.verify_integrity(), Ok(()));

        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('x' as u32);
        page.append_grapheme_at(0, 0, 0x0301).unwrap();
        page.reinit();

        assert_eq!(page.verify_integrity(), Ok(()));
    }

    #[test]
    fn page_verify_integrity_rejects_zero_size() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        page.size.rows = 0;
        assert_eq!(page.verify_integrity(), Err(IntegrityError::ZeroRowCount));

        page.size.rows = 2;
        page.size.cols = 0;
        assert_eq!(page.verify_integrity(), Err(IntegrityError::ZeroColCount));
    }

    #[test]
    fn page_verify_integrity_graphemes_good() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();

        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
            page.append_grapheme_at(x, 0, 0x0301).unwrap();
        }

        assert_eq!(page.verify_integrity(), Ok(()));
    }

    #[test]
    fn page_verify_integrity_grapheme_row_not_marked() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('g' as u32);
        page.append_grapheme_at(0, 0, 0x0301).unwrap();
        page.get_row_mut(0).set_grapheme(false);

        assert_eq!(
            page.verify_integrity(),
            Err(IntegrityError::UnmarkedGraphemeRow)
        );
    }

    #[test]
    fn page_verify_integrity_missing_grapheme_data() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('g' as u32);
        page.append_grapheme_at(0, 0, 0x0301).unwrap();
        let offset = page.cell_offset_at(0, 0);
        page.grapheme_map_mut()
            .unwrap()
            .fetch_remove(offset)
            .unwrap();

        assert_eq!(
            page.verify_integrity(),
            Err(IntegrityError::MissingGraphemeData)
        );
    }

    #[test]
    fn page_verify_integrity_unmarked_grapheme_cell() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('g' as u32);
        page.append_grapheme_at(0, 0, 0x0301).unwrap();
        page.get_row_and_cell_mut(0, 0)
            .cell
            .set_content_tag(ContentTag::Codepoint);

        assert_eq!(
            page.verify_integrity(),
            Err(IntegrityError::UnmarkedGraphemeCell)
        );
    }

    #[test]
    fn page_verify_integrity_styles_good() {
        let mut page = Page::init(Capacity {
            cols: 5,
            rows: 2,
            styles: 8,
            ..Capacity::new(5, 2)
        })
        .unwrap();
        let id = page
            .add_style(style::Style {
                flags: style::Flags {
                    bold: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            })
            .unwrap();

        for x in 0..page.size.cols as usize {
            let rac = page.get_row_and_cell_mut(x, 0);
            *rac.cell = Cell::init((x + 1) as u32);
            rac.row.set_styled(true);
            rac.cell.set_style_id(id);
            page.use_style(id);
        }
        page.release_style(id);

        assert_eq!(page.verify_integrity(), Ok(()));
    }

    #[test]
    fn page_verify_integrity_style_extra_ref_is_valid() {
        let mut page = Page::init(Capacity {
            cols: 5,
            rows: 2,
            styles: 8,
            ..Capacity::new(5, 2)
        })
        .unwrap();
        let id = page.add_style(style::Style::default()).unwrap();
        let rac = page.get_row_and_cell_mut(0, 0);
        *rac.cell = Cell::init('s' as u32);
        rac.row.set_styled(true);
        rac.cell.set_style_id(id);
        page.use_style(id);

        assert_eq!(page.style_ref_count(id), 2);
        assert_eq!(page.verify_integrity(), Ok(()));
    }

    #[test]
    fn page_verify_integrity_missing_style() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        let rac = page.get_row_and_cell_mut(0, 0);
        *rac.cell = Cell::init('s' as u32);
        rac.row.set_styled(true);
        rac.cell.set_style_id(42);

        assert_eq!(page.verify_integrity(), Err(IntegrityError::MissingStyle));
    }

    #[test]
    fn page_verify_integrity_unmarked_style_row() {
        let mut page = Page::init(Capacity {
            cols: 5,
            rows: 2,
            styles: 8,
            ..Capacity::new(5, 2)
        })
        .unwrap();
        let id = page.add_style(style::Style::default()).unwrap();
        let rac = page.get_row_and_cell_mut(0, 0);
        *rac.cell = Cell::init('s' as u32);
        rac.cell.set_style_id(id);

        assert_eq!(
            page.verify_integrity(),
            Err(IntegrityError::UnmarkedStyleRow)
        );
    }

    #[test]
    fn page_verify_integrity_style_ref_count_mismatch() {
        let mut page = Page::init(Capacity {
            cols: 5,
            rows: 2,
            styles: 8,
            ..Capacity::new(5, 2)
        })
        .unwrap();
        let id = page.add_style(style::Style::default()).unwrap();
        for x in 0..2 {
            let rac = page.get_row_and_cell_mut(x, 0);
            *rac.cell = Cell::init((x + 1) as u32);
            rac.row.set_styled(true);
            rac.cell.set_style_id(id);
        }

        assert_eq!(
            page.verify_integrity(),
            Err(IntegrityError::MismatchedStyleRef)
        );
    }

    #[test]
    fn page_verify_integrity_hyperlinks_good() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        let id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com",
            })
            .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('h' as u32);
        page.set_hyperlink(0, 0, id).unwrap();

        assert_eq!(page.verify_integrity(), Ok(()));
    }

    #[test]
    fn page_verify_integrity_hyperlink_extra_ref_is_valid() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        let id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com",
            })
            .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('h' as u32);
        page.set_hyperlink(0, 0, id).unwrap();
        page.use_hyperlink(id);

        assert_eq!(page.hyperlink_ref_count(id), 2);
        assert_eq!(page.verify_integrity(), Ok(()));
    }

    #[test]
    fn page_verify_integrity_missing_hyperlink_data() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        let id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com",
            })
            .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('h' as u32);
        page.set_hyperlink(0, 0, id).unwrap();
        let offset = page.cell_offset_at(0, 0);
        page.hyperlink_map_mut()
            .unwrap()
            .fetch_remove(offset)
            .unwrap();

        assert_eq!(
            page.verify_integrity(),
            Err(IntegrityError::MissingHyperlinkData)
        );
    }

    #[test]
    fn page_verify_integrity_unmarked_hyperlink_cell() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        let id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com",
            })
            .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('h' as u32);
        page.set_hyperlink(0, 0, id).unwrap();
        page.get_row_and_cell_mut(0, 0).cell.set_hyperlink(false);

        assert_eq!(
            page.verify_integrity(),
            Err(IntegrityError::UnmarkedHyperlinkCell)
        );
    }

    #[test]
    fn page_verify_integrity_unmarked_hyperlink_row() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        let id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com",
            })
            .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('h' as u32);
        page.set_hyperlink(0, 0, id).unwrap();
        page.get_row_mut(0).set_hyperlink(false);

        assert_eq!(
            page.verify_integrity(),
            Err(IntegrityError::UnmarkedHyperlinkRow)
        );
    }

    #[test]
    fn page_verify_integrity_hyperlink_ref_count_mismatch() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        let id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com",
            })
            .unwrap();
        for x in 0..2 {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
            page.set_hyperlink(x, 0, id).unwrap();
        }

        assert_eq!(
            page.verify_integrity(),
            Err(IntegrityError::MismatchedHyperlinkRef)
        );
    }

    #[test]
    fn page_verify_integrity_spacer_tail_at_column_zero() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        page.get_row_and_cell_mut(0, 0)
            .cell
            .set_wide(Wide::SpacerTail);

        assert_eq!(
            page.verify_integrity(),
            Err(IntegrityError::InvalidSpacerTailLocation)
        );
    }

    #[test]
    fn page_verify_integrity_spacer_tail_after_non_wide() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        page.get_row_and_cell_mut(1, 0)
            .cell
            .set_wide(Wide::SpacerTail);

        assert_eq!(
            page.verify_integrity(),
            Err(IntegrityError::InvalidSpacerTailLocation)
        );
    }

    #[test]
    fn page_verify_integrity_spacer_head_not_at_end() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        page.get_row_and_cell_mut(1, 0)
            .cell
            .set_wide(Wide::SpacerHead);

        assert_eq!(
            page.verify_integrity(),
            Err(IntegrityError::InvalidSpacerHeadLocation)
        );
    }

    #[test]
    fn page_verify_integrity_unwrapped_spacer_head() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        page.get_row_and_cell_mut(4, 0)
            .cell
            .set_wide(Wide::SpacerHead);

        assert_eq!(
            page.verify_integrity(),
            Err(IntegrityError::UnwrappedSpacerHead)
        );
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
    fn page_set_graphemes_multi_codepoint() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);

        page.set_graphemes_at(1, 0, &[0x0301, 0x0300, 0x0327])
            .unwrap();

        assert_eq!(
            page.lookup_grapheme_at(1, 0),
            Some(vec![0x0301, 0x0300, 0x0327])
        );
        assert!(page.cell_copy_at(1, 0).has_grapheme());
        assert!(page.get_row(0).grapheme());
        assert_eq!(page.grapheme_count(), 1);
        assert_eq!(page.verify_integrity(), Ok(()));
    }

    #[test]
    fn page_set_graphemes_single_codepoint() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);

        page.set_graphemes_at(1, 0, &[0x0301]).unwrap();

        assert_eq!(page.lookup_grapheme_at(1, 0), Some(vec![0x0301]));
        assert!(page.grapheme_used_bytes() > 0);
        assert_eq!(page.grapheme_count(), 1);
        assert_eq!(page.verify_integrity(), Ok(()));
    }

    #[test]
    #[should_panic]
    fn page_set_graphemes_rejects_zero_base_codepoint() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();

        page.set_graphemes_at(1, 0, &[0x0301]).unwrap();
    }

    #[test]
    #[should_panic]
    fn page_set_graphemes_rejects_empty_codepoints() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);

        page.set_graphemes_at(1, 0, &[]).unwrap();
    }

    #[test]
    #[should_panic]
    fn page_set_graphemes_rejects_existing_grapheme_data() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);
        page.append_grapheme_at(1, 0, 0x0301).unwrap();

        page.set_graphemes_at(1, 0, &[0x0300]).unwrap();
    }

    #[test]
    #[should_panic]
    fn page_set_graphemes_rejects_invalid_codepoint() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);

        page.set_graphemes_at(1, 0, &[0x11_0000]).unwrap();
    }

    #[test]
    fn page_set_graphemes_map_oom_rolls_back_allocation_and_flags() {
        let mut page = Page::init(Capacity::with_metadata(
            2,
            2,
            8,
            HYPERLINK_BYTES_DEFAULT,
            (GRAPHEME_CHUNK * 2) as GraphemeBytesInt,
            STRING_BYTES_DEFAULT,
        ))
        .unwrap();
        let hidden_offsets = [page.cell_offset_at(0, 1), page.cell_offset_at(1, 1)];
        page.size.rows = 1;
        {
            let mut map = page.grapheme_map_mut().unwrap();
            for offset in hidden_offsets {
                map.put_no_clobber(offset, OffsetSlice::default()).unwrap();
            }
            assert_eq!(map.count(), map.capacity());
        }

        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);
        let used_before = page.grapheme_used_bytes();
        let result = page.set_graphemes_at(0, 0, &[0x0301]);

        assert_eq!(result, Err(GraphemeError::GraphemeMapOutOfMemory));
        assert_eq!(page.grapheme_used_bytes(), used_before);
        assert!(!page.cell_copy_at(0, 0).has_grapheme());
        assert!(!page.get_row(0).grapheme());
        assert_eq!(page.verify_integrity(), Ok(()));
    }

    #[test]
    fn page_set_graphemes_alloc_oom_rolls_back_flags() {
        let mut page = Page::init(Capacity::with_metadata(
            2,
            2,
            8,
            HYPERLINK_BYTES_DEFAULT,
            0,
            STRING_BYTES_DEFAULT,
        ))
        .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);

        let result = page.set_graphemes_at(0, 0, &[0x0301, 0x0300, 0x0327, 0x0328, 0x0323]);

        assert_eq!(result, Err(GraphemeError::GraphemeAllocOutOfMemory));
        assert_eq!(page.grapheme_used_bytes(), 0);
        assert_eq!(page.grapheme_count(), 0);
        assert!(!page.cell_copy_at(0, 0).has_grapheme());
        assert!(!page.get_row(0).grapheme());
        assert_eq!(page.verify_integrity(), Ok(()));
    }

    #[test]
    fn page_move_grapheme_moves_map_entry_without_allocating() {
        let mut page = Page::init(Capacity::new(5, 1)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);
        *page.get_row_and_cell_mut(3, 0).cell = Cell::init('b' as u32);
        page.set_graphemes_at(1, 0, &[0x0301, 0x0300]).unwrap();
        let count_before = page.grapheme_count();
        let used_before = page.grapheme_used_bytes();

        page.move_grapheme_at(1, 0, 3, 0);

        assert_eq!(page.lookup_grapheme_at(1, 0), None);
        assert_eq!(page.lookup_grapheme_at(3, 0), Some(vec![0x0301, 0x0300]));
        assert_eq!(page.grapheme_count(), count_before);
        assert_eq!(page.grapheme_used_bytes(), used_before);
    }

    #[test]
    fn page_move_grapheme_leaves_cell_tags_for_caller() {
        let mut page = Page::init(Capacity::new(5, 1)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);
        *page.get_row_and_cell_mut(3, 0).cell = Cell::init('b' as u32);
        page.set_graphemes_at(1, 0, &[0x0301]).unwrap();

        page.move_grapheme_at(1, 0, 3, 0);

        assert!(page.cell_copy_at(1, 0).has_grapheme());
        assert!(!page.cell_copy_at(3, 0).has_grapheme());
        assert_eq!(
            page.verify_integrity(),
            Err(IntegrityError::MissingGraphemeData)
        );

        page.get_row_and_cell_mut(1, 0)
            .cell
            .set_content_tag(ContentTag::Codepoint);
        assert_eq!(
            page.verify_integrity(),
            Err(IntegrityError::UnmarkedGraphemeCell)
        );

        page.get_row_and_cell_mut(3, 0)
            .cell
            .set_content_tag(ContentTag::CodepointGrapheme);
        page.update_row_grapheme_flag(0);
        assert_eq!(page.verify_integrity(), Ok(()));
    }

    #[test]
    #[should_panic]
    fn page_move_grapheme_rejects_source_without_grapheme() {
        let mut page = Page::init(Capacity::new(5, 1)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);
        *page.get_row_and_cell_mut(3, 0).cell = Cell::init('b' as u32);

        page.move_grapheme_at(1, 0, 3, 0);
    }

    #[test]
    #[should_panic]
    fn page_move_grapheme_rejects_destination_with_grapheme() {
        let mut page = Page::init(Capacity::new(5, 1)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);
        *page.get_row_and_cell_mut(3, 0).cell = Cell::init('b' as u32);
        page.set_graphemes_at(1, 0, &[0x0301]).unwrap();
        page.set_graphemes_at(3, 0, &[0x0300]).unwrap();

        page.move_grapheme_at(1, 0, 3, 0);
    }

    #[test]
    fn page_move_grapheme_cross_row_requires_destination_row_flag() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);
        *page.get_row_and_cell_mut(3, 1).cell = Cell::init('b' as u32);
        page.set_graphemes_at(1, 0, &[0x0301]).unwrap();

        page.move_grapheme_at(1, 0, 3, 1);
        page.get_row_and_cell_mut(1, 0)
            .cell
            .set_content_tag(ContentTag::Codepoint);
        page.get_row_and_cell_mut(3, 1)
            .cell
            .set_content_tag(ContentTag::CodepointGrapheme);

        assert!(page.get_row(0).grapheme());
        assert!(!page.get_row(1).grapheme());
        assert_eq!(
            page.verify_integrity(),
            Err(IntegrityError::UnmarkedGraphemeRow)
        );

        page.get_row_mut(1).set_grapheme(true);
        assert_eq!(page.verify_integrity(), Ok(()));

        page.update_row_grapheme_flag(0);
        assert!(!page.get_row(0).grapheme());
        assert_eq!(page.verify_integrity(), Ok(()));
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
    fn page_hyperlink_init_and_zero_capacity_insert_fails() {
        let page = Page::init(Capacity::new(5, 5)).unwrap();
        assert_eq!(page.hyperlink_count(), 0);
        assert!(page.hyperlink_capacity() > 0);
        assert_eq!(page.hyperlink_set_count(), 0);
        assert_eq!(page.string_used_bytes(), 0);

        let mut zero = Page::init(Capacity::with_metadata(
            5,
            5,
            8,
            0,
            GRAPHEME_BYTES_DEFAULT,
            0,
        ))
        .unwrap();
        assert_eq!(
            zero.insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com",
            }),
            Err(InsertHyperlinkError::StringsOutOfMemory)
        );
        assert_eq!(zero.hyperlink_count(), 0);
        assert_eq!(zero.hyperlink_capacity(), 0);
        assert_eq!(zero.hyperlink_set_count(), 0);
    }

    #[test]
    fn page_hyperlink_insert_lookup_implicit_and_explicit() {
        let mut page = Page::init(Capacity::new(5, 5)).unwrap();

        let implicit = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(42),
                uri: b"https://example.com/a",
            })
            .unwrap();
        let explicit = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"tab-1"),
                uri: b"https://example.com/b",
            })
            .unwrap();

        page.set_hyperlink(0, 0, implicit).unwrap();
        page.set_hyperlink(1, 0, explicit).unwrap();

        assert_eq!(page.lookup_hyperlink_at(0, 0), Some(implicit));
        assert_eq!(page.lookup_hyperlink_at(1, 0), Some(explicit));
        assert_eq!(
            page.get_hyperlink(implicit),
            HyperlinkSnapshot {
                id: HyperlinkSnapshotId::Implicit(42),
                uri: b"https://example.com/a".to_vec(),
            }
        );
        assert_eq!(
            page.get_hyperlink(explicit),
            HyperlinkSnapshot {
                id: HyperlinkSnapshotId::Explicit(b"tab-1".to_vec()),
                uri: b"https://example.com/b".to_vec(),
            }
        );
    }

    #[test]
    fn page_hyperlink_set_clear_flags_and_refs() {
        let mut page = Page::init(Capacity::new(5, 5)).unwrap();
        let id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com",
            })
            .unwrap();

        page.set_hyperlink(0, 0, id).unwrap();
        assert!(page.cell_copy_at(0, 0).hyperlink());
        assert!(page.get_row(0).hyperlink());
        assert_eq!(page.lookup_hyperlink_at(0, 0), Some(id));
        assert_eq!(page.hyperlink_ref_count(id), 1);
        assert_eq!(page.hyperlink_count(), 1);

        page.clear_hyperlink(0, 0);
        page.update_row_hyperlink_flag(0);

        assert!(!page.cell_copy_at(0, 0).hyperlink());
        assert!(!page.get_row(0).hyperlink());
        assert_eq!(page.lookup_hyperlink_at(0, 0), None);
        assert_eq!(page.hyperlink_ref_count(id), 0);
        assert_eq!(page.hyperlink_count(), 0);
        assert_eq!(page.hyperlink_set_count(), 0);
    }

    #[test]
    fn page_hyperlink_replacement_releases_old_and_maps_new() {
        let mut page = Page::init(Capacity::new(5, 5)).unwrap();
        let old = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com/old",
            })
            .unwrap();
        let new = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(2),
                uri: b"https://example.com/new",
            })
            .unwrap();

        page.set_hyperlink(0, 0, old).unwrap();
        page.set_hyperlink(0, 0, new).unwrap();

        assert_eq!(page.lookup_hyperlink_at(0, 0), Some(new));
        assert_eq!(page.hyperlink_ref_count(old), 0);
        assert_eq!(page.hyperlink_ref_count(new), 1);
        assert_eq!(page.hyperlink_count(), 1);
    }

    #[test]
    fn page_hyperlink_same_id_replacement_consumes_preused_ref() {
        let mut page = Page::init(Capacity::new(5, 5)).unwrap();
        let id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com",
            })
            .unwrap();

        page.set_hyperlink(0, 0, id).unwrap();
        assert_eq!(page.hyperlink_ref_count(id), 1);
        page.use_hyperlink(id);
        assert_eq!(page.hyperlink_ref_count(id), 2);
        page.set_hyperlink(0, 0, id).unwrap();

        assert_eq!(page.hyperlink_ref_count(id), 1);
        assert_eq!(page.lookup_hyperlink_at(0, 0), Some(id));
        assert!(page.cell_copy_at(0, 0).hyperlink());
        assert!(page.get_row(0).hyperlink());
    }

    #[test]
    fn page_print_cell_writes_style_and_hyperlink() {
        let mut page = Page::init(Capacity::new(5, 5)).unwrap();
        let styled = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };

        page.write_print_cell(
            0,
            0,
            'A',
            styled,
            Some(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"id"),
                uri: b"https://example.com",
            }),
            SemanticContent::Output,
        )
        .unwrap();

        let cell = page.cell_copy_at(0, 0);
        let link_id = page.lookup_hyperlink_at(0, 0).unwrap();
        assert_eq!(cell.codepoint(), 'A' as u32);
        assert_ne!(cell.style_id(), style::DEFAULT_ID);
        assert!(cell.hyperlink());
        assert!(page.get_row(0).styled());
        assert!(page.get_row(0).hyperlink());
        assert_eq!(page.style_ref_count(cell.style_id()), 1);
        assert_eq!(page.hyperlink_ref_count(link_id), 1);
        assert_eq!(
            page.get_hyperlink(link_id),
            HyperlinkSnapshot {
                id: HyperlinkSnapshotId::Explicit(b"id".to_vec()),
                uri: b"https://example.com".to_vec(),
            }
        );
    }

    #[test]
    fn page_print_cell_dedups_replaces_and_clears_hyperlinks() {
        let mut page = Page::init(Capacity::new(5, 5)).unwrap();
        let first = hyperlink::Hyperlink {
            id: hyperlink::HyperlinkId::Implicit(1),
            uri: b"https://one",
        };
        let second = hyperlink::Hyperlink {
            id: hyperlink::HyperlinkId::Implicit(2),
            uri: b"https://two",
        };

        page.write_print_cell(
            0,
            0,
            'A',
            style::Style::default(),
            Some(first),
            SemanticContent::Output,
        )
        .unwrap();
        page.write_print_cell(
            1,
            0,
            'B',
            style::Style::default(),
            Some(first),
            SemanticContent::Output,
        )
        .unwrap();
        let first_id = page.lookup_hyperlink_at(0, 0).unwrap();
        assert_eq!(page.lookup_hyperlink_at(1, 0), Some(first_id));
        assert_eq!(page.hyperlink_ref_count(first_id), 2);

        page.write_print_cell(
            0,
            0,
            'C',
            style::Style::default(),
            Some(second),
            SemanticContent::Output,
        )
        .unwrap();
        let second_id = page.lookup_hyperlink_at(0, 0).unwrap();
        assert_ne!(first_id, second_id);
        assert_eq!(page.hyperlink_ref_count(first_id), 1);
        assert_eq!(page.hyperlink_ref_count(second_id), 1);

        page.write_print_cell(
            1,
            0,
            'D',
            style::Style::default(),
            None,
            SemanticContent::Output,
        )
        .unwrap();
        assert_eq!(page.lookup_hyperlink_at(1, 0), None);
        assert_eq!(page.hyperlink_ref_count(first_id), 0);
        assert!(page.get_row(0).hyperlink());

        page.write_print_cell(
            0,
            0,
            'E',
            style::Style::default(),
            None,
            SemanticContent::Output,
        )
        .unwrap();
        assert_eq!(page.lookup_hyperlink_at(0, 0), None);
        assert_eq!(page.hyperlink_ref_count(second_id), 0);
        assert!(!page.get_row(0).hyperlink());
        assert_eq!(page.hyperlink_count(), 0);
        assert_eq!(page.hyperlink_set_count(), 0);
    }

    #[test]
    fn page_print_cell_rolls_back_on_style_allocation_failure() {
        let mut page = Page::init(Capacity::with_metadata(
            5,
            5,
            0,
            HYPERLINK_BYTES_DEFAULT,
            GRAPHEME_BYTES_DEFAULT,
            STRING_BYTES_DEFAULT,
        ))
        .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('x' as u32);
        let styled = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };

        assert_eq!(
            page.write_print_cell(0, 0, 'A', styled, None, SemanticContent::Output),
            Err(PrintCellError::StyleOutOfMemory)
        );

        assert_eq!(page.cell_copy_at(0, 0).codepoint(), 'x' as u32);
        assert_eq!(page.style_count(), 0);
    }

    #[test]
    fn page_print_cell_rolls_back_on_hyperlink_allocation_failure() {
        let mut page = Page::init(Capacity::with_metadata(
            5,
            5,
            8,
            0,
            GRAPHEME_BYTES_DEFAULT,
            0,
        ))
        .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('x' as u32);

        assert_eq!(
            page.write_print_cell(
                0,
                0,
                'A',
                style::Style::default(),
                Some(hyperlink::Hyperlink {
                    id: hyperlink::HyperlinkId::Implicit(1),
                    uri: b"https://example.com",
                }),
                SemanticContent::Output,
            ),
            Err(PrintCellError::HyperlinkOutOfMemory)
        );

        assert_eq!(page.cell_copy_at(0, 0).codepoint(), 'x' as u32);
        assert!(!page.cell_copy_at(0, 0).hyperlink());
        assert_eq!(page.hyperlink_count(), 0);
        assert_eq!(page.hyperlink_set_count(), 0);
    }

    #[test]
    fn page_print_cell_releases_new_style_on_hyperlink_failure() {
        let mut page = Page::init(Capacity::with_metadata(
            5,
            5,
            8,
            0,
            GRAPHEME_BYTES_DEFAULT,
            1,
        ))
        .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('x' as u32);
        let styled = style::Style {
            flags: style::Flags {
                italic: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };

        assert_eq!(
            page.write_print_cell(
                0,
                0,
                'A',
                styled,
                Some(hyperlink::Hyperlink {
                    id: hyperlink::HyperlinkId::Explicit(b"id"),
                    uri: b"https://example.com",
                }),
                SemanticContent::Output,
            ),
            Err(PrintCellError::HyperlinkOutOfMemory)
        );

        assert_eq!(page.cell_copy_at(0, 0).codepoint(), 'x' as u32);
        assert_eq!(page.cell_copy_at(0, 0).style_id(), style::DEFAULT_ID);
        assert!(!page.cell_copy_at(0, 0).hyperlink());
        assert_eq!(page.style_count(), 0);
        assert_eq!(page.hyperlink_count(), 0);
        assert_eq!(page.hyperlink_set_count(), 0);
    }

    #[test]
    fn page_print_cell_rolls_back_on_hyperlink_map_failure() {
        let mut page = Page::init(Capacity::with_metadata(
            128,
            1,
            8,
            (hyperlink::Set::capacity_for_count(1) * HYPERLINK_SET_ITEM_SIZE) as HyperlinkCountInt,
            GRAPHEME_BYTES_DEFAULT,
            STRING_BYTES_DEFAULT,
        ))
        .unwrap();
        let link = hyperlink::Hyperlink {
            id: hyperlink::HyperlinkId::Implicit(1),
            uri: b"https://example.com",
        };
        let id = page.insert_hyperlink(link).unwrap();
        let map_capacity = page.hyperlink_capacity();
        assert!(map_capacity + 1 <= page.capacity().cols as usize);

        for x in 0..map_capacity {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init(b'a' as u32 + x as u32);
            if x > 0 {
                page.use_hyperlink(id);
            }
            page.set_hyperlink(x, 0, id).unwrap();
        }
        assert_eq!(page.hyperlink_count(), map_capacity);
        assert_eq!(page.hyperlink_ref_count(id), map_capacity as u16);
        assert_eq!(page.hyperlink_set_count(), 1);

        assert_eq!(
            page.write_print_cell(
                map_capacity,
                0,
                'Q',
                style::Style::default(),
                Some(link),
                SemanticContent::Output,
            ),
            Err(PrintCellError::HyperlinkOutOfMemory)
        );

        assert_eq!(page.cell_copy_at(map_capacity, 0).codepoint(), 0);
        assert!(!page.cell_copy_at(map_capacity, 0).hyperlink());
        assert_eq!(page.lookup_hyperlink_at(map_capacity, 0), None);
        assert_eq!(page.hyperlink_count(), map_capacity);
        assert_eq!(page.hyperlink_ref_count(id), map_capacity as u16);
        assert_eq!(page.hyperlink_set_count(), 1);
    }

    #[test]
    fn page_hyperlink_deduplicates_by_id_and_uri_contents() {
        let mut page = Page::init(Capacity::with_metadata(
            5,
            5,
            8,
            16 * HYPERLINK_SET_ITEM_SIZE as HyperlinkCountInt,
            GRAPHEME_BYTES_DEFAULT,
            STRING_BYTES_DEFAULT,
        ))
        .unwrap();

        let first = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com/a",
            })
            .unwrap();
        let same_implicit = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com/a",
            })
            .unwrap();
        let different_implicit = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(2),
                uri: b"https://example.com/a",
            })
            .unwrap();
        let explicit = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"id"),
                uri: b"https://example.com/a",
            })
            .unwrap();
        let same_explicit = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"id"),
                uri: b"https://example.com/a",
            })
            .unwrap();
        let different_uri = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"id"),
                uri: b"https://example.com/b",
            })
            .unwrap();
        let different_explicit = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"other"),
                uri: b"https://example.com/a",
            })
            .unwrap();

        assert_eq!(same_implicit, first);
        assert_ne!(different_implicit, first);
        assert_eq!(same_explicit, explicit);
        assert_ne!(different_uri, explicit);
        assert_ne!(different_explicit, explicit);
        assert_eq!(page.hyperlink_ref_count(first), 2);
        assert_eq!(page.hyperlink_ref_count(explicit), 2);
    }

    #[test]
    fn page_hyperlink_count_reports_linked_cells() {
        let mut page = Page::init(Capacity::new(5, 5)).unwrap();
        let id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com",
            })
            .unwrap();

        page.set_hyperlink(0, 0, id).unwrap();
        page.use_hyperlink(id);
        page.set_hyperlink(1, 0, id).unwrap();
        page.use_hyperlink(id);
        page.set_hyperlink(2, 0, id).unwrap();

        assert_eq!(page.hyperlink_count(), 3);
        assert_eq!(page.hyperlink_set_count(), 1);
        assert_eq!(page.hyperlink_ref_count(id), 3);
        assert!(page.hyperlink_capacity() >= 3);
    }

    #[test]
    fn page_hyperlink_insert_rolls_back_uri_success_id_failure() {
        let mut page = Page::init(Capacity::with_metadata(
            5,
            5,
            8,
            HYPERLINK_BYTES_DEFAULT,
            GRAPHEME_BYTES_DEFAULT,
            64,
        ))
        .unwrap();
        let uri = vec![b'u'; page.string_alloc.capacity_bytes()];

        assert_eq!(
            page.insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"id"),
                uri: &uri,
            }),
            Err(InsertHyperlinkError::StringsOutOfMemory)
        );
        assert_eq!(page.string_used_bytes(), 0);
        assert_eq!(page.hyperlink_set_count(), 0);
        assert_eq!(page.hyperlink_count(), 0);
    }

    #[test]
    fn page_hyperlink_insert_rolls_back_string_success_set_failure() {
        let mut page = Page::init(Capacity::with_metadata(
            5,
            5,
            8,
            0,
            GRAPHEME_BYTES_DEFAULT,
            1,
        ))
        .unwrap();

        assert_eq!(
            page.insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"id"),
                uri: b"https://example.com",
            }),
            Err(InsertHyperlinkError::SetOutOfMemory)
        );
        assert_eq!(page.string_used_bytes(), 0);
        assert_eq!(page.hyperlink_set_count(), 0);
        assert_eq!(page.hyperlink_count(), 0);
    }

    #[test]
    fn page_clone_hyperlinks_survive_source_release_and_drop() {
        let (page2, id) = {
            let mut page = Page::init(Capacity::new(5, 5)).unwrap();
            let id = page
                .insert_hyperlink(hyperlink::Hyperlink {
                    id: hyperlink::HyperlinkId::Explicit(b"id"),
                    uri: b"https://example.com",
                })
                .unwrap();
            page.set_hyperlink(0, 0, id).unwrap();

            let page2 = page.clone_page().unwrap();
            page.clear_hyperlink(0, 0);
            page.update_row_hyperlink_flag(0);
            assert_eq!(page.hyperlink_ref_count(id), 0);
            assert_eq!(page.hyperlink_count(), 0);

            (page2, id)
        };

        assert_eq!(page2.lookup_hyperlink_at(0, 0), Some(id));
        assert_eq!(
            page2.get_hyperlink(id),
            HyperlinkSnapshot {
                id: HyperlinkSnapshotId::Explicit(b"id".to_vec()),
                uri: b"https://example.com".to_vec(),
            }
        );
        assert_eq!(page2.hyperlink_ref_count(id), 1);
        assert_eq!(page2.hyperlink_count(), 1);
        assert_eq!(page2.hyperlink_set_count(), 1);
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
            .clone_rows_from(&page, 0, page.size.rows as usize)
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
            .clone_rows_from(&page, 0, page.size.rows as usize)
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
        page2.clone_rows_from(&page, 0, 5).unwrap();

        for y in 0..5 {
            assert_eq!(page2.get_cells(page2.get_row(y))[1].codepoint(), y as u32);
        }
        for y in 5..page2.size.rows as usize {
            assert_eq!(page2.get_cells(page2.get_row(y))[1].codepoint(), 0);
        }
    }

    #[test]
    fn page_clone_partial_row_plain_cells() {
        let mut page = Page::init(Capacity::new(10, 1)).unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
        }

        let mut page2 = Page::init(Capacity::new(10, 1)).unwrap();
        for x in 0..page2.size.cols as usize {
            *page2.get_row_and_cell_mut(x, 0).cell = Cell::init(0xbb);
        }

        page2.clone_partial_row_from(&page, 0, 0, 2, 8).unwrap();

        for x in 0..page2.size.cols as usize {
            let expected = if (2..8).contains(&x) {
                (x + 1) as u32
            } else {
                0xbb
            };
            assert_eq!(page2.cell_copy_at(x, 0).codepoint(), expected);
        }
    }

    #[test]
    fn page_clone_partial_row_omits_source_graphemes_outside_range() {
        let mut page = Page::init(Capacity::new(10, 1)).unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
        }
        page.append_grapheme_at(0, 0, 0x0301).unwrap();
        page.append_grapheme_at(9, 0, 0x0302).unwrap();

        let mut page2 = Page::init(Capacity::new(10, 1)).unwrap();
        page2.clone_partial_row_from(&page, 0, 0, 2, 8).unwrap();

        for x in 0..page2.size.cols as usize {
            assert!(!page2.cell_copy_at(x, 0).has_grapheme());
        }
        assert!(!page2.get_row(0).grapheme());
        assert_eq!(page2.grapheme_count(), 0);
    }

    #[test]
    fn page_clone_partial_row_preserves_destination_graphemes_outside_range() {
        let mut page = Page::init(Capacity::new(10, 1)).unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
        }

        let mut page2 = Page::init(Capacity::new(10, 1)).unwrap();
        for x in 0..page2.size.cols as usize {
            *page2.get_row_and_cell_mut(x, 0).cell = Cell::init(0xbb);
        }
        page2.append_grapheme_at(0, 0, 0x0301).unwrap();
        page2.append_grapheme_at(9, 0, 0x0302).unwrap();

        page2.clone_partial_row_from(&page, 0, 0, 2, 8).unwrap();

        assert_eq!(page2.lookup_grapheme_at(0, 0), Some(vec![0x0301]));
        assert_eq!(page2.lookup_grapheme_at(9, 0), Some(vec![0x0302]));
        assert!(page2.get_row(0).grapheme());
        assert_eq!(page2.grapheme_count(), 2);
    }

    #[test]
    fn page_clone_partial_row_within_page_copies_hyperlink_in_range() {
        let mut page = Page::init(Capacity::new(10, 2)).unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
        }
        let id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com",
            })
            .unwrap();
        page.set_hyperlink(7, 0, id).unwrap();
        assert_eq!(page.hyperlink_ref_count(id), 1);

        page.clone_partial_row_within_page(1, 0, 2, 8).unwrap();

        assert_eq!(page.lookup_hyperlink_at(7, 1), Some(id));
        assert_eq!(page.hyperlink_ref_count(id), 2);
        assert_eq!(page.hyperlink_count(), 2);
        assert!(page.get_row(1).hyperlink());
    }

    #[test]
    fn page_clone_partial_row_within_page_omits_hyperlink_outside_range() {
        let mut page = Page::init(Capacity::new(10, 2)).unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
        }
        let id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com",
            })
            .unwrap();
        page.set_hyperlink(7, 0, id).unwrap();

        page.clone_partial_row_within_page(1, 0, 2, 6).unwrap();

        assert_eq!(page.lookup_hyperlink_at(7, 1), None);
        assert_eq!(page.hyperlink_ref_count(id), 1);
        assert_eq!(page.hyperlink_count(), 1);
        assert!(!page.get_row(1).hyperlink());
    }

    #[test]
    fn page_clone_partial_row_within_page_reuses_style_id() {
        let mut page = Page::init(Capacity {
            cols: 10,
            rows: 2,
            styles: 8,
            ..Capacity::new(10, 2)
        })
        .unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
        }
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let id = page.add_style(bold).unwrap();
        let rac = page.get_row_and_cell_mut(4, 0);
        rac.row.set_styled(true);
        rac.cell.set_style_id(id);
        assert_eq!(page.style_ref_count(id), 1);

        page.clone_partial_row_within_page(1, 0, 2, 8).unwrap();

        assert_eq!(page.cell_copy_at(4, 1).style_id(), id);
        assert_eq!(page.style_ref_count(id), 2);
        assert_eq!(page.style_count(), 1);
        assert!(page.get_row(1).styled());
    }

    #[test]
    fn page_clone_partial_row_copies_and_preserves_styles() {
        let mut page = Page::init(Capacity {
            cols: 10,
            rows: 1,
            styles: 8,
            ..Capacity::new(10, 1)
        })
        .unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
        }
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let source_id = page.add_style(bold).unwrap();
        for x in [1, 4] {
            let rac = page.get_row_and_cell_mut(x, 0);
            rac.row.set_styled(true);
            rac.cell.set_style_id(source_id);
            page.use_style(source_id);
        }
        page.release_style(source_id);

        let mut page2 = Page::init(Capacity {
            cols: 10,
            rows: 1,
            styles: 8,
            ..Capacity::new(10, 1)
        })
        .unwrap();
        let underline = style::Style {
            flags: style::Flags {
                underline: Underline::Single,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let destination_id = page2.add_style(underline).unwrap();
        let rac = page2.get_row_and_cell_mut(9, 0);
        *rac.cell = Cell::init('z' as u32);
        rac.row.set_styled(true);
        rac.cell.set_style_id(destination_id);

        page2.clone_partial_row_from(&page, 0, 0, 2, 8).unwrap();

        let copied_id = page2.cell_copy_at(4, 0).style_id();
        assert_eq!(page2.cell_copy_at(1, 0).style_id(), style::DEFAULT_ID);
        assert_ne!(copied_id, style::DEFAULT_ID);
        assert_eq!(page2.cell_copy_at(9, 0).style_id(), destination_id);
        assert_eq!(page2.get_style(copied_id), bold);
        assert_eq!(page2.get_style(destination_id), underline);
        assert!(page2.get_row(0).styled());
    }

    #[test]
    fn page_clone_partial_row_grapheme_map_oom_leaves_valid_cells() {
        let mut page = Page::init(Capacity::new(5, 1)).unwrap();
        *page.get_row_and_cell_mut(2, 0).cell = Cell::init('g' as u32);
        page.append_grapheme_at(2, 0, 0x0301).unwrap();

        let mut page2 = Page::init(Capacity::with_metadata(
            5,
            1,
            8,
            HYPERLINK_BYTES_DEFAULT,
            0,
            STRING_BYTES_DEFAULT,
        ))
        .unwrap();
        *page2.get_row_and_cell_mut(4, 0).cell = Cell::init('z' as u32);

        assert_eq!(
            page2.clone_partial_row_from(&page, 0, 0, 2, 3),
            Err(CloneFromError::Grapheme(
                GraphemeError::GraphemeMapOutOfMemory
            ))
        );

        assert_eq!(page2.cell_copy_at(2, 0).codepoint(), 'g' as u32);
        assert!(!page2.cell_copy_at(2, 0).has_grapheme());
        assert_eq!(page2.cell_copy_at(4, 0).codepoint(), 'z' as u32);
        assert!(!page2.get_row(0).grapheme());
        assert_eq!(page2.grapheme_count(), 0);
    }

    #[test]
    fn page_clone_partial_row_style_oom_leaves_valid_cells() {
        let mut page = Page::init(Capacity {
            cols: 5,
            rows: 1,
            styles: 8,
            ..Capacity::new(5, 1)
        })
        .unwrap();
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let source_id = page.add_style(bold).unwrap();
        let rac = page.get_row_and_cell_mut(2, 0);
        *rac.cell = Cell::init('s' as u32);
        rac.row.set_styled(true);
        rac.cell.set_style_id(source_id);

        let mut page2 = Page::init(Capacity::with_metadata(
            5,
            1,
            0,
            HYPERLINK_BYTES_DEFAULT,
            GRAPHEME_BYTES_DEFAULT,
            STRING_BYTES_DEFAULT,
        ))
        .unwrap();
        *page2.get_row_and_cell_mut(4, 0).cell = Cell::init('z' as u32);

        assert_eq!(
            page2.clone_partial_row_from(&page, 0, 0, 2, 3),
            Err(CloneFromError::Style(
                super::super::ref_counted_set::AddError::OutOfMemory
            ))
        );
        assert_eq!(page2.cell_copy_at(2, 0).codepoint(), 's' as u32);
        assert_eq!(page2.cell_copy_at(2, 0).style_id(), style::DEFAULT_ID);
        assert_eq!(page2.cell_copy_at(4, 0).codepoint(), 'z' as u32);
        assert!(!page2.get_row(0).styled());
    }

    #[test]
    fn page_clone_partial_row_hyperlink_map_oom_preserves_outside_range() {
        let mut source = Page::init(Capacity::new(80, 1)).unwrap();
        *source.get_row_and_cell_mut(0, 0).cell = Cell::init('s' as u32);
        let source_id = source
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com/source",
            })
            .unwrap();
        source.set_hyperlink(0, 0, source_id).unwrap();

        let mut destination = Page::init(Capacity::new(80, 1)).unwrap();
        let destination_id = destination
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(2),
                uri: b"https://example.com/destination",
            })
            .unwrap();
        let capacity = destination.hyperlink_capacity();
        for x in 1..=capacity {
            if x > 1 {
                destination.use_hyperlink(destination_id);
            }
            *destination.get_row_and_cell_mut(x, 0).cell = Cell::init('d' as u32);
            destination.set_hyperlink(x, 0, destination_id).unwrap();
        }
        let ref_count_before = destination.hyperlink_ref_count(destination_id);

        assert_eq!(
            destination.clone_partial_row_from(&source, 0, 0, 0, 1),
            Err(CloneFromError::HyperlinkMapOutOfMemory)
        );

        assert_eq!(destination.lookup_hyperlink_at(0, 0), None);
        assert!(!destination.cell_copy_at(0, 0).hyperlink());
        assert_eq!(destination.lookup_hyperlink_at(1, 0), Some(destination_id));
        assert_eq!(
            destination.hyperlink_ref_count(destination_id),
            ref_count_before
        );
    }

    #[test]
    fn page_clone_partial_row_hyperlink_string_oom_leaves_valid_cells() {
        let mut source = Page::init(Capacity::new(5, 1)).unwrap();
        *source.get_row_and_cell_mut(2, 0).cell = Cell::init('s' as u32);
        let source_id = source
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"id"),
                uri: b"https://example.com/source",
            })
            .unwrap();
        source.set_hyperlink(2, 0, source_id).unwrap();

        let mut destination = Page::init(Capacity::with_metadata(
            5,
            1,
            8,
            HYPERLINK_BYTES_DEFAULT,
            GRAPHEME_BYTES_DEFAULT,
            0,
        ))
        .unwrap();
        *destination.get_row_and_cell_mut(4, 0).cell = Cell::init('z' as u32);

        assert_eq!(
            destination.clone_partial_row_from(&source, 0, 0, 2, 3),
            Err(CloneFromError::StringAllocOutOfMemory)
        );

        assert_eq!(destination.lookup_hyperlink_at(2, 0), None);
        assert!(!destination.cell_copy_at(2, 0).hyperlink());
        assert_eq!(destination.cell_copy_at(4, 0).codepoint(), 'z' as u32);
        assert!(!destination.get_row(0).hyperlink());
        assert_eq!(destination.hyperlink_count(), 0);
        assert_eq!(destination.hyperlink_set_count(), 0);
        assert_eq!(destination.string_used_bytes(), 0);
    }

    #[test]
    fn page_clone_partial_row_hyperlink_set_oom_frees_strings() {
        let mut source = Page::init(Capacity::new(5, 1)).unwrap();
        *source.get_row_and_cell_mut(2, 0).cell = Cell::init('s' as u32);
        let source_id = source
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(100),
                uri: b"https://example.com/source",
            })
            .unwrap();
        source.set_hyperlink(2, 0, source_id).unwrap();

        let mut destination = Page::init(Capacity::new(5, 1)).unwrap();
        *destination.get_row_and_cell_mut(4, 0).cell = Cell::init('z' as u32);
        let mut inserted = Vec::new();
        for i in 0.. {
            match destination.insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(i),
                uri: b"https://example.com/destination",
            }) {
                Ok(id) => inserted.push(id),
                Err(InsertHyperlinkError::SetOutOfMemory) => break,
                Err(err) => panic!("unexpected insert error: {err:?}"),
            }
        }
        assert!(!inserted.is_empty());
        let set_count_before = destination.hyperlink_set_count();
        let string_used_before = destination.string_used_bytes();

        assert_eq!(
            destination.clone_partial_row_from(&source, 0, 0, 2, 3),
            Err(CloneFromError::HyperlinkSetOutOfMemory)
        );

        assert_eq!(destination.lookup_hyperlink_at(2, 0), None);
        assert!(!destination.cell_copy_at(2, 0).hyperlink());
        assert_eq!(destination.cell_copy_at(4, 0).codepoint(), 'z' as u32);
        assert!(!destination.get_row(0).hyperlink());
        assert_eq!(destination.hyperlink_count(), 0);
        assert_eq!(destination.hyperlink_set_count(), set_count_before);
        assert_eq!(destination.string_used_bytes(), string_used_before);
    }

    #[test]
    fn page_move_cells_text_only_full_row() {
        let mut page = Page::init(Capacity::new(10, 2)).unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
        }

        page.move_cells(0, 0, 1, 0, page.size.cols as usize);

        for x in 0..page.size.cols as usize {
            assert_eq!(page.cell_copy_at(x, 1).codepoint(), (x + 1) as u32);
            assert_eq!(page.cell_copy_at(x, 0).codepoint(), 0);
        }
    }

    #[test]
    fn page_move_cells_text_only_partial_preserves_outside_ranges() {
        let mut page = Page::init(Capacity::new(10, 2)).unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
            *page.get_row_and_cell_mut(x, 1).cell = Cell::init(0xbb);
        }

        page.move_cells(0, 2, 1, 3, 4);

        for x in 0..page.size.cols as usize {
            let expected_dst = if (3..7).contains(&x) { x as u32 } else { 0xbb };
            assert_eq!(page.cell_copy_at(x, 1).codepoint(), expected_dst);
        }
        for x in 0..page.size.cols as usize {
            let expected_src = if (2..6).contains(&x) {
                0
            } else {
                (x + 1) as u32
            };
            assert_eq!(page.cell_copy_at(x, 0).codepoint(), expected_src);
        }
    }

    #[test]
    fn page_move_cells_graphemes_moves_map_entries_without_allocating() {
        let mut page = Page::init(Capacity::new(10, 2)).unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
            page.append_grapheme_at(x, 0, 0x0301 + x as u32).unwrap();
        }
        let count_before = page.grapheme_count();
        let used_before = page.grapheme_used_bytes();

        page.move_cells(0, 0, 1, 0, page.size.cols as usize);

        assert_eq!(page.grapheme_count(), count_before);
        assert_eq!(page.grapheme_used_bytes(), used_before);
        assert!(!page.get_row(0).grapheme());
        assert!(page.get_row(1).grapheme());
        for x in 0..page.size.cols as usize {
            assert_eq!(page.lookup_grapheme_at(x, 0), None);
            assert_eq!(page.lookup_grapheme_at(x, 1), Some(vec![0x0301 + x as u32]));
            assert_eq!(page.cell_copy_at(x, 0), Cell::default());
        }
    }

    #[test]
    fn page_move_cells_styles_preserve_moved_ref_and_release_destination() {
        let mut page = Page::init(Capacity {
            cols: 5,
            rows: 2,
            styles: 8,
            ..Capacity::new(5, 2)
        })
        .unwrap();
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let underline = style::Style {
            flags: style::Flags {
                underline: Underline::Single,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let source_id = page.add_style(bold).unwrap();
        let destination_id = page.add_style(underline).unwrap();
        let rac = page.get_row_and_cell_mut(0, 0);
        *rac.cell = Cell::init('s' as u32);
        rac.row.set_styled(true);
        rac.cell.set_style_id(source_id);
        let rac = page.get_row_and_cell_mut(1, 1);
        *rac.cell = Cell::init('d' as u32);
        rac.row.set_styled(true);
        rac.cell.set_style_id(destination_id);

        page.move_cells(0, 0, 1, 1, 1);

        assert_eq!(page.cell_copy_at(1, 1).style_id(), source_id);
        assert_eq!(page.cell_copy_at(0, 0), Cell::default());
        assert_eq!(page.style_ref_count(source_id), 1);
        assert_eq!(page.style_ref_count(destination_id), 0);
        assert!(!page.get_row(0).styled());
        assert!(page.get_row(1).styled());
    }

    #[test]
    fn page_move_cells_hyperlinks_preserve_moved_ref_and_release_destination() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        let source_id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com/source",
            })
            .unwrap();
        let destination_id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(2),
                uri: b"https://example.com/destination",
            })
            .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('s' as u32);
        page.set_hyperlink(0, 0, source_id).unwrap();
        *page.get_row_and_cell_mut(1, 1).cell = Cell::init('d' as u32);
        page.set_hyperlink(1, 1, destination_id).unwrap();

        page.move_cells(0, 0, 1, 1, 1);

        assert_eq!(page.lookup_hyperlink_at(1, 1), Some(source_id));
        assert_eq!(page.lookup_hyperlink_at(0, 0), None);
        assert_eq!(page.hyperlink_ref_count(source_id), 1);
        assert_eq!(page.hyperlink_ref_count(destination_id), 0);
        assert_eq!(page.hyperlink_count(), 1);
        assert!(!page.get_row(0).hyperlink());
        assert!(page.get_row(1).hyperlink());
    }

    #[test]
    fn page_move_cells_rejects_same_row_overlap_before_mutation() {
        let mut page = Page::init(Capacity::new(5, 1)).unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
        }

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            page.move_cells(0, 0, 0, 1, 3);
        }));

        assert!(result.is_err());
        for x in 0..page.size.cols as usize {
            assert_eq!(page.cell_copy_at(x, 0).codepoint(), (x + 1) as u32);
        }
    }

    #[test]
    fn page_delete_chars_in_row_shifts_cells_and_managed_metadata_left() {
        let mut page = Page::init(Capacity {
            cols: 6,
            rows: 1,
            styles: 8,
            ..Capacity::new(6, 1)
        })
        .unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((b'A' + x as u8) as u32);
        }
        let style_id = page
            .add_style(style::Style {
                flags: style::Flags {
                    bold: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            })
            .unwrap();
        let link_id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"id"),
                uri: b"https://example.com",
            })
            .unwrap();

        let source = page.get_row_and_cell_mut(3, 0);
        source.row.set_styled(true);
        source.cell.set_style_id(style_id);
        source.cell.set_protected(true);
        page.append_grapheme_at(3, 0, 0x0301).unwrap();
        page.set_hyperlink(3, 0, link_id).unwrap();

        page.delete_chars_in_row(0, 1, 5, 2);

        assert_eq!(page.cell_copy_at(1, 0).codepoint(), 'D' as u32);
        assert_eq!(page.cell_copy_at(1, 0).style_id(), style_id);
        assert!(page.cell_copy_at(1, 0).protected());
        assert_eq!(page.lookup_grapheme_at(1, 0), Some(vec![0x0301]));
        assert_eq!(page.lookup_hyperlink_at(1, 0), Some(link_id));
        assert_eq!(page.cell_copy_at(2, 0).codepoint(), 'E' as u32);
        assert_eq!(page.cell_copy_at(3, 0).codepoint(), 'F' as u32);
        assert_eq!(page.cell_copy_at(4, 0), Cell::default());
        assert_eq!(page.cell_copy_at(5, 0), Cell::default());
        assert_eq!(page.style_ref_count(style_id), 1);
        assert_eq!(page.grapheme_count(), 1);
        assert_eq!(page.hyperlink_ref_count(link_id), 1);
        assert_eq!(page.verify_integrity(), Ok(()));
    }

    #[test]
    fn page_insert_chars_in_row_shifts_cells_and_managed_metadata_right() {
        let mut page = Page::init(Capacity {
            cols: 6,
            rows: 1,
            styles: 8,
            ..Capacity::new(6, 1)
        })
        .unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((b'A' + x as u8) as u32);
        }
        let style_id = page
            .add_style(style::Style {
                flags: style::Flags {
                    bold: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            })
            .unwrap();
        let link_id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"id"),
                uri: b"https://example.com",
            })
            .unwrap();

        let source = page.get_row_and_cell_mut(2, 0);
        source.row.set_styled(true);
        source.cell.set_style_id(style_id);
        source.cell.set_protected(true);
        page.append_grapheme_at(2, 0, 0x0301).unwrap();
        page.set_hyperlink(2, 0, link_id).unwrap();

        page.insert_chars_in_row(0, 1, 5, 2);

        assert_eq!(page.cell_copy_at(1, 0), Cell::default());
        assert_eq!(page.cell_copy_at(2, 0), Cell::default());
        assert_eq!(page.cell_copy_at(3, 0).codepoint(), 'B' as u32);
        assert_eq!(page.cell_copy_at(4, 0).codepoint(), 'C' as u32);
        assert_eq!(page.cell_copy_at(4, 0).style_id(), style_id);
        assert!(page.cell_copy_at(4, 0).protected());
        assert_eq!(page.lookup_grapheme_at(4, 0), Some(vec![0x0301]));
        assert_eq!(page.lookup_hyperlink_at(4, 0), Some(link_id));
        assert_eq!(page.cell_copy_at(5, 0).codepoint(), 'D' as u32);
        assert_eq!(page.style_ref_count(style_id), 1);
        assert_eq!(page.grapheme_count(), 1);
        assert_eq!(page.hyperlink_ref_count(link_id), 1);
        assert_eq!(page.verify_integrity(), Ok(()));
    }

    #[test]
    fn page_move_cells_rejects_exact_self_move_before_mutation() {
        let mut page = Page::init(Capacity::new(5, 1)).unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
        }

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            page.move_cells(0, 1, 0, 1, 2);
        }));

        assert!(result.is_err());
        for x in 0..page.size.cols as usize {
            assert_eq!(page.cell_copy_at(x, 0).codepoint(), (x + 1) as u32);
        }
    }

    #[test]
    fn page_swap_cells_plain_cells() {
        let mut page = Page::init(Capacity::new(5, 1)).unwrap();
        let mut left = Cell::init('a' as u32);
        left.set_wide(Wide::Wide);
        let mut right = Cell::init('b' as u32);
        right.set_protected(true);
        *page.get_row_and_cell_mut(1, 0).cell = left;
        *page.get_row_and_cell_mut(3, 0).cell = right;

        page.swap_cells(0, 1, 0, 3);

        assert_eq!(page.cell_copy_at(1, 0).codepoint(), 'b' as u32);
        assert!(page.cell_copy_at(1, 0).protected());
        assert_eq!(page.cell_copy_at(3, 0).codepoint(), 'a' as u32);
        assert_eq!(page.cell_copy_at(3, 0).wide(), Wide::Wide);
    }

    #[test]
    fn page_swap_cells_grapheme_source_only() {
        let mut page = Page::init(Capacity::new(5, 1)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);
        *page.get_row_and_cell_mut(3, 0).cell = Cell::init('b' as u32);
        page.append_grapheme_at(1, 0, 0x0301).unwrap();
        let count_before = page.grapheme_count();
        let used_before = page.grapheme_used_bytes();

        page.swap_cells(0, 1, 0, 3);

        assert_eq!(page.grapheme_count(), count_before);
        assert_eq!(page.grapheme_used_bytes(), used_before);
        assert_eq!(page.lookup_grapheme_at(1, 0), None);
        assert_eq!(page.lookup_grapheme_at(3, 0), Some(vec![0x0301]));
        assert!(!page.cell_copy_at(1, 0).has_grapheme());
        assert!(page.cell_copy_at(3, 0).has_grapheme());
        assert!(page.get_row(0).grapheme());
    }

    #[test]
    fn page_swap_cells_grapheme_destination_only() {
        let mut page = Page::init(Capacity::new(5, 1)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);
        *page.get_row_and_cell_mut(3, 0).cell = Cell::init('b' as u32);
        page.append_grapheme_at(3, 0, 0x0302).unwrap();
        let count_before = page.grapheme_count();
        let used_before = page.grapheme_used_bytes();

        page.swap_cells(0, 1, 0, 3);

        assert_eq!(page.grapheme_count(), count_before);
        assert_eq!(page.grapheme_used_bytes(), used_before);
        assert_eq!(page.lookup_grapheme_at(1, 0), Some(vec![0x0302]));
        assert_eq!(page.lookup_grapheme_at(3, 0), None);
        assert!(page.cell_copy_at(1, 0).has_grapheme());
        assert!(!page.cell_copy_at(3, 0).has_grapheme());
    }

    #[test]
    fn page_swap_cells_grapheme_both_sides() {
        let mut page = Page::init(Capacity::new(5, 1)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);
        *page.get_row_and_cell_mut(3, 0).cell = Cell::init('b' as u32);
        page.append_grapheme_at(1, 0, 0x0301).unwrap();
        page.append_grapheme_at(3, 0, 0x0302).unwrap();
        let count_before = page.grapheme_count();
        let used_before = page.grapheme_used_bytes();

        page.swap_cells(0, 1, 0, 3);

        assert_eq!(page.grapheme_count(), count_before);
        assert_eq!(page.grapheme_used_bytes(), used_before);
        assert_eq!(page.lookup_grapheme_at(1, 0), Some(vec![0x0302]));
        assert_eq!(page.lookup_grapheme_at(3, 0), Some(vec![0x0301]));
        assert!(page.cell_copy_at(1, 0).has_grapheme());
        assert!(page.cell_copy_at(3, 0).has_grapheme());
    }

    #[test]
    fn page_swap_cells_hyperlink_source_only() {
        let mut page = Page::init(Capacity::new(5, 1)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);
        *page.get_row_and_cell_mut(3, 0).cell = Cell::init('b' as u32);
        let id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com/source",
            })
            .unwrap();
        page.set_hyperlink(1, 0, id).unwrap();

        page.swap_cells(0, 1, 0, 3);

        assert_eq!(page.lookup_hyperlink_at(1, 0), None);
        assert_eq!(page.lookup_hyperlink_at(3, 0), Some(id));
        assert_eq!(page.hyperlink_ref_count(id), 1);
        assert_eq!(page.hyperlink_count(), 1);
        assert!(!page.cell_copy_at(1, 0).hyperlink());
        assert!(page.cell_copy_at(3, 0).hyperlink());
    }

    #[test]
    fn page_swap_cells_hyperlink_destination_only() {
        let mut page = Page::init(Capacity::new(5, 1)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);
        *page.get_row_and_cell_mut(3, 0).cell = Cell::init('b' as u32);
        let id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com/destination",
            })
            .unwrap();
        page.set_hyperlink(3, 0, id).unwrap();

        page.swap_cells(0, 1, 0, 3);

        assert_eq!(page.lookup_hyperlink_at(1, 0), Some(id));
        assert_eq!(page.lookup_hyperlink_at(3, 0), None);
        assert_eq!(page.hyperlink_ref_count(id), 1);
        assert_eq!(page.hyperlink_count(), 1);
        assert!(page.cell_copy_at(1, 0).hyperlink());
        assert!(!page.cell_copy_at(3, 0).hyperlink());
    }

    #[test]
    fn page_swap_cells_hyperlink_both_sides() {
        let mut page = Page::init(Capacity::new(5, 1)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);
        *page.get_row_and_cell_mut(3, 0).cell = Cell::init('b' as u32);
        let left_id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com/left",
            })
            .unwrap();
        let right_id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(2),
                uri: b"https://example.com/right",
            })
            .unwrap();
        page.set_hyperlink(1, 0, left_id).unwrap();
        page.set_hyperlink(3, 0, right_id).unwrap();

        page.swap_cells(0, 1, 0, 3);

        assert_eq!(page.lookup_hyperlink_at(1, 0), Some(right_id));
        assert_eq!(page.lookup_hyperlink_at(3, 0), Some(left_id));
        assert_eq!(page.hyperlink_ref_count(left_id), 1);
        assert_eq!(page.hyperlink_ref_count(right_id), 1);
        assert_eq!(page.hyperlink_count(), 2);
    }

    #[test]
    fn page_swap_cells_styles_preserve_refcounts() {
        let mut page = Page::init(Capacity {
            cols: 5,
            rows: 1,
            styles: 8,
            ..Capacity::new(5, 1)
        })
        .unwrap();
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let underline = style::Style {
            flags: style::Flags {
                underline: Underline::Single,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let left_id = page.add_style(bold).unwrap();
        let right_id = page.add_style(underline).unwrap();
        let rac = page.get_row_and_cell_mut(1, 0);
        *rac.cell = Cell::init('a' as u32);
        rac.row.set_styled(true);
        rac.cell.set_style_id(left_id);
        let rac = page.get_row_and_cell_mut(3, 0);
        *rac.cell = Cell::init('b' as u32);
        rac.row.set_styled(true);
        rac.cell.set_style_id(right_id);

        page.swap_cells(0, 1, 0, 3);

        assert_eq!(page.cell_copy_at(1, 0).style_id(), right_id);
        assert_eq!(page.cell_copy_at(3, 0).style_id(), left_id);
        assert_eq!(page.style_ref_count(left_id), 1);
        assert_eq!(page.style_ref_count(right_id), 1);
        assert!(page.get_row(0).styled());
    }

    #[test]
    fn page_swap_cells_cross_row_updates_flags() {
        let mut page = Page::init(Capacity::new(5, 2)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);
        page.append_grapheme_at(1, 0, 0x0301).unwrap();
        *page.get_row_and_cell_mut(3, 1).cell = Cell::init('b' as u32);

        page.swap_cells(0, 1, 1, 3);

        assert!(!page.get_row(0).grapheme());
        assert!(page.get_row(1).grapheme());
        assert_eq!(page.lookup_grapheme_at(1, 0), None);
        assert_eq!(page.lookup_grapheme_at(3, 1), Some(vec![0x0301]));
    }

    #[test]
    fn page_swap_cells_self_swap_is_noop() {
        let mut page = Page::init(Capacity::new(5, 1)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);
        page.append_grapheme_at(1, 0, 0x0301).unwrap();
        let id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com",
            })
            .unwrap();
        page.set_hyperlink(1, 0, id).unwrap();
        let cell_before = page.cell_copy_at(1, 0);
        let grapheme_count_before = page.grapheme_count();
        let hyperlink_count_before = page.hyperlink_count();
        let hyperlink_ref_before = page.hyperlink_ref_count(id);

        page.swap_cells(0, 1, 0, 1);

        assert_eq!(page.cell_copy_at(1, 0), cell_before);
        assert_eq!(page.lookup_grapheme_at(1, 0), Some(vec![0x0301]));
        assert_eq!(page.lookup_hyperlink_at(1, 0), Some(id));
        assert_eq!(page.grapheme_count(), grapheme_count_before);
        assert_eq!(page.hyperlink_count(), hyperlink_count_before);
        assert_eq!(page.hyperlink_ref_count(id), hyperlink_ref_before);
    }

    #[test]
    fn page_clear_cells_plain_range_and_empty_noop() {
        let mut page = Page::init(Capacity::new(6, 1)).unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
        }

        page.clear_cells(0, 2, 5);

        assert_eq!(page.cell_copy_at(0, 0).codepoint(), 1);
        assert_eq!(page.cell_copy_at(1, 0).codepoint(), 2);
        assert_eq!(page.cell_copy_at(2, 0), Cell::default());
        assert_eq!(page.cell_copy_at(3, 0), Cell::default());
        assert_eq!(page.cell_copy_at(4, 0), Cell::default());
        assert_eq!(page.cell_copy_at(5, 0).codepoint(), 6);

        let before = page.get_cells(page.get_row(0)).to_vec();
        page.clear_cells(0, 3, 3);
        assert_eq!(page.get_cells(page.get_row(0)), before.as_slice());
    }

    #[test]
    fn page_clear_cells_graphemes_partial_and_full() {
        let mut page = Page::init(Capacity::new(6, 1)).unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
        }
        page.append_grapheme_at(1, 0, 0x0301).unwrap();
        page.append_grapheme_at(4, 0, 0x0302).unwrap();
        assert_eq!(page.grapheme_count(), 2);
        assert!(page.grapheme_used_bytes() > 0);

        page.clear_cells(0, 0, 3);

        assert_eq!(page.lookup_grapheme_at(1, 0), None);
        assert_eq!(page.lookup_grapheme_at(4, 0), Some(vec![0x0302]));
        assert_eq!(page.grapheme_count(), 1);
        assert!(page.get_row(0).grapheme());

        page.clear_cells(0, 0, page.size.cols as usize);

        assert_eq!(page.grapheme_count(), 0);
        assert_eq!(page.grapheme_used_bytes(), 0);
        assert!(!page.get_row(0).grapheme());
        for x in 0..page.size.cols as usize {
            assert_eq!(page.cell_copy_at(x, 0), Cell::default());
        }
    }

    #[test]
    fn page_clear_cells_hyperlinks_partial_and_full() {
        let mut page = Page::init(Capacity::new(6, 1)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('a' as u32);
        *page.get_row_and_cell_mut(4, 0).cell = Cell::init('b' as u32);
        let left = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com/left",
            })
            .unwrap();
        let right = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(2),
                uri: b"https://example.com/right",
            })
            .unwrap();
        page.set_hyperlink(1, 0, left).unwrap();
        page.set_hyperlink(4, 0, right).unwrap();

        page.clear_cells(0, 0, 3);

        assert_eq!(page.lookup_hyperlink_at(1, 0), None);
        assert_eq!(page.hyperlink_ref_count(left), 0);
        assert_eq!(page.lookup_hyperlink_at(4, 0), Some(right));
        assert_eq!(page.hyperlink_ref_count(right), 1);
        assert_eq!(page.hyperlink_count(), 1);
        assert!(page.get_row(0).hyperlink());

        page.clear_cells(0, 0, page.size.cols as usize);

        assert_eq!(page.hyperlink_ref_count(right), 0);
        assert_eq!(page.hyperlink_count(), 0);
        assert!(!page.get_row(0).hyperlink());
    }

    #[test]
    fn page_clear_cells_styles_partial_and_full() {
        let mut page = Page::init(Capacity {
            cols: 6,
            rows: 1,
            styles: 8,
            ..Capacity::new(6, 1)
        })
        .unwrap();
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let underline = style::Style {
            flags: style::Flags {
                underline: Underline::Single,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let left = page.add_style(bold).unwrap();
        let right = page.add_style(underline).unwrap();
        let rac = page.get_row_and_cell_mut(1, 0);
        *rac.cell = Cell::init('a' as u32);
        rac.row.set_styled(true);
        rac.cell.set_style_id(left);
        let rac = page.get_row_and_cell_mut(4, 0);
        *rac.cell = Cell::init('b' as u32);
        rac.row.set_styled(true);
        rac.cell.set_style_id(right);

        page.clear_cells(0, 0, 3);

        assert_eq!(page.style_ref_count(left), 0);
        assert_eq!(page.style_ref_count(right), 1);
        assert_eq!(page.cell_copy_at(1, 0), Cell::default());
        assert_eq!(page.cell_copy_at(4, 0).style_id(), right);
        assert!(page.get_row(0).styled());

        page.clear_cells(0, 0, page.size.cols as usize);

        assert_eq!(page.style_ref_count(right), 0);
        assert!(!page.get_row(0).styled());
    }

    #[test]
    fn page_clear_cells_mixed_managed_memory() {
        let mut page = Page::init(Capacity {
            cols: 6,
            rows: 1,
            styles: 8,
            ..Capacity::new(6, 1)
        })
        .unwrap();
        let style_id = page
            .add_style(style::Style {
                flags: style::Flags {
                    italic: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            })
            .unwrap();
        let link_id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"id"),
                uri: b"https://example.com",
            })
            .unwrap();

        let rac = page.get_row_and_cell_mut(1, 0);
        *rac.cell = Cell::init('s' as u32);
        rac.row.set_styled(true);
        rac.cell.set_style_id(style_id);
        *page.get_row_and_cell_mut(2, 0).cell = Cell::init('g' as u32);
        page.append_grapheme_at(2, 0, 0x0301).unwrap();
        *page.get_row_and_cell_mut(3, 0).cell = Cell::init('h' as u32);
        page.set_hyperlink(3, 0, link_id).unwrap();

        page.clear_cells(0, 1, 4);

        assert_eq!(page.cell_copy_at(1, 0), Cell::default());
        assert_eq!(page.cell_copy_at(2, 0), Cell::default());
        assert_eq!(page.cell_copy_at(3, 0), Cell::default());
        assert_eq!(page.style_ref_count(style_id), 0);
        assert_eq!(page.grapheme_count(), 0);
        assert_eq!(page.hyperlink_ref_count(link_id), 0);
        assert_eq!(page.hyperlink_count(), 0);
        assert!(!page.get_row(0).managed_memory());
    }

    #[test]
    fn page_clear_unprotected_cells_skips_protected_and_releases_managed_memory() {
        let mut page = Page::init(Capacity {
            cols: 6,
            rows: 1,
            styles: 8,
            ..Capacity::new(6, 1)
        })
        .unwrap();
        let style_id = page
            .add_style(style::Style {
                flags: style::Flags {
                    italic: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            })
            .unwrap();
        let link_id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"id"),
                uri: b"https://example.com",
            })
            .unwrap();

        let rac = page.get_row_and_cell_mut(1, 0);
        *rac.cell = Cell::init('s' as u32);
        rac.row.set_styled(true);
        rac.cell.set_style_id(style_id);
        *page.get_row_and_cell_mut(2, 0).cell = Cell::init('g' as u32);
        page.append_grapheme_at(2, 0, 0x0301).unwrap();
        *page.get_row_and_cell_mut(3, 0).cell = Cell::init('h' as u32);
        page.set_hyperlink(3, 0, link_id).unwrap();
        let protected = page.get_row_and_cell_mut(4, 0);
        *protected.cell = Cell::init('p' as u32);
        protected.cell.set_protected(true);

        page.clear_unprotected_cells(0, 1, 5);

        assert_eq!(page.cell_copy_at(1, 0), Cell::default());
        assert_eq!(page.cell_copy_at(2, 0), Cell::default());
        assert_eq!(page.cell_copy_at(3, 0), Cell::default());
        assert_eq!(page.cell_copy_at(4, 0).codepoint(), 'p' as u32);
        assert!(page.cell_copy_at(4, 0).protected());
        assert_eq!(page.style_ref_count(style_id), 0);
        assert_eq!(page.grapheme_count(), 0);
        assert_eq!(page.hyperlink_ref_count(link_id), 0);
        assert_eq!(page.hyperlink_count(), 0);
        assert!(!page.get_row(0).grapheme());
        assert!(!page.get_row(0).hyperlink());
        assert!(!page.get_row(0).styled());
    }

    #[test]
    fn page_clear_cells_preserves_unrelated_row_metadata() {
        let mut page = Page::init(Capacity::new(4, 1)).unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
        }
        {
            let row = page.get_row_mut(0);
            row.set_wrap(true);
            row.set_wrap_continuation(true);
            row.set_dirty(true);
            row.set_semantic_prompt(SemanticPrompt::Prompt);
        }

        page.clear_cells(0, 0, page.size.cols as usize);

        let row = page.get_row(0);
        assert!(row.wrap());
        assert!(row.wrap_continuation());
        assert!(row.dirty());
        assert_eq!(row.semantic_prompt(), SemanticPrompt::Prompt);
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

        page2.clone_rows_from(&page, 0, 1).unwrap();
        let cells = page2.get_cells(page2.get_row(0));
        assert_eq!(cells[0].codepoint(), 'a' as u32);
        assert_eq!(cells[1].codepoint(), 'b' as u32);
        assert_eq!(cells[2].codepoint(), 'c' as u32);
        assert_eq!(cells[2].wide(), Wide::Narrow);
        assert_eq!(cells[3].codepoint(), 'x' as u32);
        assert_eq!(cells[4].codepoint(), 'y' as u32);
    }

    #[test]
    fn page_clone_from_hyperlinks_cross_page() {
        let mut source = Page::init(Capacity {
            cols: 3,
            rows: 1,
            styles: 8,
            ..Capacity::new(3, 1)
        })
        .unwrap();
        let mut destination = Page::init(Capacity {
            cols: 3,
            rows: 1,
            styles: 8,
            ..Capacity::new(3, 1)
        })
        .unwrap();
        *source.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);
        *source.get_row_and_cell_mut(1, 0).cell = Cell::init('b' as u32);

        let implicit = source
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(7),
                uri: b"https://example.com/a",
            })
            .unwrap();
        let explicit = source
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"id"),
                uri: b"https://example.com/b",
            })
            .unwrap();
        source.set_hyperlink(0, 0, implicit).unwrap();
        source.set_hyperlink(1, 0, explicit).unwrap();

        destination.clone_rows_from(&source, 0, 1).unwrap();

        let dst_implicit = destination.lookup_hyperlink_at(0, 0).unwrap();
        let dst_explicit = destination.lookup_hyperlink_at(1, 0).unwrap();
        assert_eq!(
            destination.get_hyperlink(dst_implicit),
            HyperlinkSnapshot {
                id: HyperlinkSnapshotId::Implicit(7),
                uri: b"https://example.com/a".to_vec(),
            }
        );
        assert_eq!(
            destination.get_hyperlink(dst_explicit),
            HyperlinkSnapshot {
                id: HyperlinkSnapshotId::Explicit(b"id".to_vec()),
                uri: b"https://example.com/b".to_vec(),
            }
        );
        assert!(destination.get_row(0).hyperlink());
        assert!(destination.cell_copy_at(0, 0).hyperlink());
        assert!(destination.cell_copy_at(1, 0).hyperlink());
        assert_eq!(destination.hyperlink_count(), 2);

        source.clear_hyperlink(0, 0);
        source.clear_hyperlink(1, 0);
        source.update_row_hyperlink_flag(0);
        assert_eq!(
            destination.get_hyperlink(dst_explicit),
            HyperlinkSnapshot {
                id: HyperlinkSnapshotId::Explicit(b"id".to_vec()),
                uri: b"https://example.com/b".to_vec(),
            }
        );
    }

    #[test]
    fn page_clone_from_hyperlinks_within_page_reuses_ids() {
        let mut page = Page::init(Capacity {
            cols: 3,
            rows: 2,
            styles: 8,
            ..Capacity::new(3, 2)
        })
        .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('b' as u32);
        let id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(9),
                uri: b"https://example.com",
            })
            .unwrap();
        page.set_hyperlink(0, 0, id).unwrap();
        page.use_hyperlink(id);
        page.set_hyperlink(1, 0, id).unwrap();
        assert_eq!(page.hyperlink_ref_count(id), 2);

        page.clone_rows_within_page(0, 1, 1).unwrap();

        assert_eq!(page.lookup_hyperlink_at(0, 1), Some(id));
        assert_eq!(page.lookup_hyperlink_at(1, 1), Some(id));
        assert_eq!(page.hyperlink_ref_count(id), 4);
        assert_eq!(page.hyperlink_count(), 4);
        assert!(page.get_row(1).hyperlink());
    }

    #[test]
    fn page_clone_from_hyperlinks_dedups_destination_entry() {
        let mut source = Page::init(Capacity::new(3, 1)).unwrap();
        let mut destination = Page::init(Capacity::new(3, 1)).unwrap();
        *source.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);
        *source.get_row_and_cell_mut(1, 0).cell = Cell::init('b' as u32);

        let source_id = source
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"id"),
                uri: b"https://example.com",
            })
            .unwrap();
        source.set_hyperlink(0, 0, source_id).unwrap();
        source.use_hyperlink(source_id);
        source.set_hyperlink(1, 0, source_id).unwrap();

        let destination_id = destination
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"id"),
                uri: b"https://example.com",
            })
            .unwrap();
        let used_before = destination.string_used_bytes();

        destination.clone_rows_from(&source, 0, 1).unwrap();

        assert_eq!(destination.lookup_hyperlink_at(0, 0), Some(destination_id));
        assert_eq!(destination.lookup_hyperlink_at(1, 0), Some(destination_id));
        assert_eq!(destination.hyperlink_ref_count(destination_id), 3);
        assert_eq!(destination.hyperlink_set_count(), 1);
        assert_eq!(destination.string_used_bytes(), used_before);
    }

    #[test]
    fn page_clone_from_hyperlinks_replaces_and_preserves_trailing() {
        let mut source = Page::init(Capacity::new(2, 1)).unwrap();
        let mut destination = Page::init(Capacity::new(4, 1)).unwrap();
        *source.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);
        *source.get_row_and_cell_mut(1, 0).cell = Cell::init('b' as u32);
        *destination.get_row_and_cell_mut(2, 0).cell = Cell::init('x' as u32);

        let source_id = source
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com/source",
            })
            .unwrap();
        source.set_hyperlink(0, 0, source_id).unwrap();

        let old_id = destination
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(2),
                uri: b"https://example.com/old",
            })
            .unwrap();
        destination.set_hyperlink(0, 0, old_id).unwrap();
        destination.use_hyperlink(old_id);
        destination.set_hyperlink(2, 0, old_id).unwrap();
        assert_eq!(destination.hyperlink_ref_count(old_id), 2);

        destination.clone_rows_from(&source, 0, 1).unwrap();

        assert_ne!(destination.lookup_hyperlink_at(0, 0), Some(old_id));
        assert_eq!(destination.lookup_hyperlink_at(1, 0), None);
        assert_eq!(destination.lookup_hyperlink_at(2, 0), Some(old_id));
        assert_eq!(destination.hyperlink_ref_count(old_id), 1);
        assert!(destination.get_row(0).hyperlink());
    }

    #[test]
    fn page_clone_from_hyperlink_map_oom_leaks_no_refs_or_strings() {
        let mut source = Page::init(Capacity::new(80, 1)).unwrap();
        let mut destination = Page::init(Capacity::with_metadata(
            80,
            2,
            8,
            HYPERLINK_BYTES_DEFAULT,
            GRAPHEME_BYTES_DEFAULT,
            STRING_BYTES_DEFAULT,
        ))
        .unwrap();

        let source_id = source
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com/source",
            })
            .unwrap();
        *source.get_row_and_cell_mut(0, 0).cell = Cell::init('s' as u32);
        source.set_hyperlink(0, 0, source_id).unwrap();

        let destination_id = destination
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(2),
                uri: b"https://example.com/destination",
            })
            .unwrap();
        let capacity = destination.hyperlink_capacity();
        for i in 0..capacity {
            if i > 0 {
                destination.use_hyperlink(destination_id);
            }
            *destination.get_row_and_cell_mut(i, 1).cell = Cell::init('d' as u32);
            destination.set_hyperlink(i, 1, destination_id).unwrap();
        }
        assert_eq!(destination.hyperlink_count(), capacity);
        let ref_count_before = destination.hyperlink_ref_count(destination_id);
        let set_count_before = destination.hyperlink_set_count();
        let string_used_before = destination.string_used_bytes();

        assert_eq!(
            destination.clone_rows_from(&source, 0, 1),
            Err(CloneFromError::HyperlinkMapOutOfMemory)
        );

        assert_eq!(
            destination.hyperlink_ref_count(destination_id),
            ref_count_before
        );
        assert_eq!(destination.hyperlink_set_count(), set_count_before);
        assert_eq!(destination.string_used_bytes(), string_used_before);
        assert_eq!(destination.lookup_hyperlink_at(0, 0), None);
        assert!(!destination.cell_copy_at(0, 0).hyperlink());
    }

    #[test]
    fn page_clone_from_hyperlink_string_oom_leaks_no_state() {
        let mut source = Page::init(Capacity::new(2, 1)).unwrap();
        let mut destination = Page::init(Capacity::with_metadata(
            2,
            1,
            8,
            HYPERLINK_BYTES_DEFAULT,
            GRAPHEME_BYTES_DEFAULT,
            0,
        ))
        .unwrap();
        let source_id = source
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"id"),
                uri: b"https://example.com/source",
            })
            .unwrap();
        *source.get_row_and_cell_mut(0, 0).cell = Cell::init('s' as u32);
        source.set_hyperlink(0, 0, source_id).unwrap();

        assert_eq!(
            destination.clone_rows_from(&source, 0, 1),
            Err(CloneFromError::StringAllocOutOfMemory)
        );

        assert_eq!(destination.hyperlink_count(), 0);
        assert_eq!(destination.hyperlink_set_count(), 0);
        assert_eq!(destination.string_used_bytes(), 0);
        assert_eq!(destination.lookup_hyperlink_at(0, 0), None);
        assert!(!destination.cell_copy_at(0, 0).hyperlink());
    }

    #[test]
    fn page_clone_from_hyperlink_set_oom_frees_duplicated_strings() {
        let mut source = Page::init(Capacity::new(2, 1)).unwrap();
        let mut destination = Page::init(Capacity::new(2, 1)).unwrap();
        let source_id = source
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(100),
                uri: b"https://example.com/source",
            })
            .unwrap();
        *source.get_row_and_cell_mut(0, 0).cell = Cell::init('s' as u32);
        source.set_hyperlink(0, 0, source_id).unwrap();

        let mut inserted = Vec::new();
        for i in 0.. {
            match destination.insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(i),
                uri: b"https://example.com/destination",
            }) {
                Ok(id) => inserted.push(id),
                Err(InsertHyperlinkError::SetOutOfMemory) => break,
                Err(err) => panic!("unexpected insert error: {err:?}"),
            }
        }
        assert!(!inserted.is_empty());
        let set_count_before = destination.hyperlink_set_count();
        let string_used_before = destination.string_used_bytes();

        assert_eq!(
            destination.clone_rows_from(&source, 0, 1),
            Err(CloneFromError::HyperlinkSetOutOfMemory)
        );

        assert_eq!(destination.hyperlink_set_count(), set_count_before);
        assert_eq!(destination.string_used_bytes(), string_used_before);
        assert_eq!(destination.hyperlink_count(), 0);
        assert_eq!(destination.lookup_hyperlink_at(0, 0), None);
        assert!(!destination.cell_copy_at(0, 0).hyperlink());
    }

    #[test]
    fn page_clone_from_graphemes() {
        let mut page = Page::init(Capacity {
            cols: 10,
            rows: 10,
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();

        for y in 0..page.capacity.rows as usize {
            *page.get_row_and_cell_mut(1, y).cell = Cell::init((y + 1) as u32);
            page.append_grapheme_at(1, y, 0x0a).unwrap();
        }

        let mut page2 = Page::init(Capacity {
            cols: 10,
            rows: 10,
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        page2
            .clone_rows_from(&page, 0, page.size.rows as usize)
            .unwrap();

        for y in 0..page2.capacity.rows as usize {
            let row = page2.get_row(y);
            let cell = page2.get_cells(row)[1];
            assert_eq!(cell.codepoint(), (y + 1) as u32);
            assert!(row.grapheme());
            assert!(cell.has_grapheme());
            assert_eq!(page2.lookup_grapheme_at(1, y), Some(vec![0x0a]));
        }

        for y in 0..page.capacity.rows as usize {
            page.clear_grapheme_at(1, y);
            page.update_row_grapheme_flag(y);
            *page.get_row_and_cell_mut(1, y).cell = Cell::init(0);
        }

        for y in 0..page2.capacity.rows as usize {
            let row = page2.get_row(y);
            let cell = page2.get_cells(row)[1];
            assert_eq!(cell.codepoint(), (y + 1) as u32);
            assert!(row.grapheme());
            assert!(cell.has_grapheme());
            assert_eq!(page2.lookup_grapheme_at(1, y), Some(vec![0x0a]));
        }

        for y in 0..page.capacity.rows as usize {
            assert_eq!(page.cell_copy_at(1, y).codepoint(), 0);
        }
    }

    #[test]
    fn page_clone_from_frees_dst_graphemes() {
        let mut page = Page::init(Capacity {
            cols: 10,
            rows: 10,
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        for y in 0..page.capacity.rows as usize {
            *page.get_row_and_cell_mut(1, y).cell = Cell::init((y + 1) as u32);
        }

        let mut page2 = Page::init(Capacity {
            cols: 10,
            rows: 10,
            styles: 8,
            ..Capacity::new(10, 10)
        })
        .unwrap();
        for y in 0..page2.capacity.rows as usize {
            *page2.get_row_and_cell_mut(1, y).cell = Cell::init((y + 1) as u32);
            page2.append_grapheme_at(1, y, 0x0a).unwrap();
        }

        page2
            .clone_rows_from(&page, 0, page.size.rows as usize)
            .unwrap();

        for y in 0..page2.capacity.rows as usize {
            let row = page2.get_row(y);
            let cell = page2.get_cells(row)[1];
            assert_eq!(cell.codepoint(), (y + 1) as u32);
            assert!(!row.grapheme());
            assert!(!cell.has_grapheme());
        }
        assert_eq!(page2.grapheme_count(), 0);
    }

    #[test]
    fn page_clone_from_multi_codepoint_grapheme() {
        let mut page = Page::init(Capacity::new(2, 1)).unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);
        page.append_grapheme_at(0, 0, 0x0301).unwrap();
        page.append_grapheme_at(0, 0, 0x0302).unwrap();

        let mut page2 = Page::init(Capacity::new(2, 1)).unwrap();
        page2.clone_rows_from(&page, 0, 1).unwrap();

        assert_eq!(page2.cell_copy_at(0, 0).codepoint(), 'a' as u32);
        assert!(page2.cell_copy_at(0, 0).has_grapheme());
        assert_eq!(page2.lookup_grapheme_at(0, 0), Some(vec![0x0301, 0x0302]));
    }

    #[test]
    fn page_clone_from_preserves_trailing_destination_grapheme() {
        let mut page = Page::init(Capacity::new(3, 1)).unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('b' as u32);
        *page.get_row_and_cell_mut(2, 0).cell = Cell::init('c' as u32);

        let mut page2 = Page::init(Capacity::new(5, 1)).unwrap();
        *page2.get_row_and_cell_mut(4, 0).cell = Cell::init('z' as u32);
        page2.append_grapheme_at(4, 0, 0x0301).unwrap();

        page2.clone_rows_from(&page, 0, 1).unwrap();

        assert_eq!(page2.cell_copy_at(0, 0).codepoint(), 'a' as u32);
        assert_eq!(page2.cell_copy_at(1, 0).codepoint(), 'b' as u32);
        assert_eq!(page2.cell_copy_at(2, 0).codepoint(), 'c' as u32);
        assert_eq!(page2.cell_copy_at(4, 0).codepoint(), 'z' as u32);
        assert!(page2.get_row(0).grapheme());
        assert!(page2.cell_copy_at(4, 0).has_grapheme());
        assert_eq!(page2.lookup_grapheme_at(4, 0), Some(vec![0x0301]));
    }

    #[test]
    fn page_clone_from_source_narrower_grapheme_sets_row_flag() {
        let mut page = Page::init(Capacity::new(3, 1)).unwrap();
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('g' as u32);
        page.append_grapheme_at(1, 0, 0x0301).unwrap();

        let mut page2 = Page::init(Capacity::new(5, 1)).unwrap();
        assert!(!page2.get_row(0).grapheme());

        page2.clone_rows_from(&page, 0, 1).unwrap();

        assert!(page2.get_row(0).grapheme());
        assert!(page2.cell_copy_at(1, 0).has_grapheme());
        assert_eq!(page2.lookup_grapheme_at(1, 0), Some(vec![0x0301]));
    }

    #[test]
    fn page_clone_from_styles_preserves_requested_id() {
        let mut page = Page::init(Capacity {
            cols: 4,
            rows: 1,
            styles: 8,
            ..Capacity::new(4, 1)
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
            *rac.cell = Cell::init((x + 1) as u32);
            rac.cell.set_style_id(id);
            page.use_style(id);
        }
        page.release_style(id);

        let mut page2 = Page::init(Capacity {
            cols: 4,
            rows: 1,
            styles: 8,
            ..Capacity::new(4, 1)
        })
        .unwrap();
        page2.clone_rows_from(&page, 0, 1).unwrap();

        assert!(page2.get_row(0).styled());
        assert_eq!(page2.style_count(), 1);
        assert_eq!(page2.style_ref_count(id), page.size.cols);
        assert_eq!(page2.get_style(id), bold);
        for x in 0..page2.size.cols as usize {
            assert_eq!(page2.cell_copy_at(x, 0).style_id(), id);
        }

        for _ in 0..page.size.cols {
            page.release_style(id);
        }
        assert_eq!(page.style_count(), 0);
        assert_eq!(page2.get_style(id), bold);
        assert_eq!(page2.style_ref_count(id), page2.size.cols);
    }

    #[test]
    fn page_clone_from_plain_source_releases_destination_styles() {
        let mut page = Page::init(Capacity::new(3, 1)).unwrap();
        for x in 0..page.size.cols as usize {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init((x + 1) as u32);
        }

        let mut page2 = Page::init(Capacity {
            cols: 3,
            rows: 1,
            styles: 8,
            ..Capacity::new(3, 1)
        })
        .unwrap();
        let italic = style::Style {
            flags: style::Flags {
                italic: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let old_id = page2.add_style(italic).unwrap();
        for x in 0..page2.size.cols as usize {
            let rac = page2.get_row_and_cell_mut(x, 0);
            rac.row.set_styled(true);
            *rac.cell = Cell::init('x' as u32);
            rac.cell.set_style_id(old_id);
            page2.use_style(old_id);
        }
        page2.release_style(old_id);
        assert_eq!(page2.style_ref_count(old_id), 3);

        page2.clone_rows_from(&page, 0, 1).unwrap();

        assert!(!page2.get_row(0).styled());
        assert_eq!(page2.style_count(), 0);
        for x in 0..page2.size.cols as usize {
            assert_eq!(page2.cell_copy_at(x, 0).style_id(), style::DEFAULT_ID);
            assert_eq!(page2.cell_copy_at(x, 0).codepoint(), (x + 1) as u32);
        }
    }

    #[test]
    fn page_clone_from_replaces_destination_style_refs() {
        let mut page = Page::init(Capacity {
            cols: 2,
            rows: 1,
            styles: 8,
            ..Capacity::new(2, 1)
        })
        .unwrap();
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let source_id = page.add_style(bold).unwrap();
        for x in 0..page.size.cols as usize {
            let rac = page.get_row_and_cell_mut(x, 0);
            rac.row.set_styled(true);
            *rac.cell = Cell::init((x + 1) as u32);
            rac.cell.set_style_id(source_id);
            page.use_style(source_id);
        }
        page.release_style(source_id);

        let mut page2 = Page::init(Capacity {
            cols: 2,
            rows: 1,
            styles: 8,
            ..Capacity::new(2, 1)
        })
        .unwrap();
        let italic = style::Style {
            flags: style::Flags {
                italic: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let old_id = page2.add_style(italic).unwrap();
        for x in 0..page2.size.cols as usize {
            let rac = page2.get_row_and_cell_mut(x, 0);
            rac.row.set_styled(true);
            *rac.cell = Cell::init('x' as u32);
            rac.cell.set_style_id(old_id);
            page2.use_style(old_id);
        }
        page2.release_style(old_id);

        page2.clone_rows_from(&page, 0, 1).unwrap();

        assert_eq!(page2.style_count(), 1);
        assert_eq!(page2.get_style(source_id), bold);
        assert_eq!(page2.style_ref_count(source_id), 2);
        for x in 0..page2.size.cols as usize {
            assert_eq!(page2.cell_copy_at(x, 0).style_id(), source_id);
        }
    }

    #[test]
    fn page_clone_from_preserves_trailing_destination_style() {
        let mut page = Page::init(Capacity::new(2, 1)).unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);
        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('b' as u32);

        let mut page2 = Page::init(Capacity {
            cols: 4,
            rows: 1,
            styles: 8,
            ..Capacity::new(4, 1)
        })
        .unwrap();
        let underline = style::Style {
            flags: style::Flags {
                underline: super::super::sgr::Underline::Single,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let id = page2.add_style(underline).unwrap();
        let rac = page2.get_row_and_cell_mut(3, 0);
        rac.row.set_styled(true);
        *rac.cell = Cell::init('z' as u32);
        rac.cell.set_style_id(id);
        page2.use_style(id);
        page2.release_style(id);

        page2.clone_rows_from(&page, 0, 1).unwrap();

        assert!(page2.get_row(0).styled());
        assert_eq!(page2.cell_copy_at(0, 0).style_id(), style::DEFAULT_ID);
        assert_eq!(page2.cell_copy_at(1, 0).style_id(), style::DEFAULT_ID);
        assert_eq!(page2.cell_copy_at(3, 0).codepoint(), 'z' as u32);
        assert_eq!(page2.cell_copy_at(3, 0).style_id(), id);
        assert_eq!(page2.get_style(id), underline);
        assert_eq!(page2.style_ref_count(id), 1);
    }

    #[test]
    fn page_clone_from_style_alternate_id() {
        let mut page = Page::init(Capacity {
            cols: 1,
            rows: 1,
            styles: 8,
            ..Capacity::new(1, 1)
        })
        .unwrap();
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let source_id = page.add_style(bold).unwrap();
        let rac = page.get_row_and_cell_mut(0, 0);
        rac.row.set_styled(true);
        *rac.cell = Cell::init('b' as u32);
        rac.cell.set_style_id(source_id);
        page.use_style(source_id);
        page.release_style(source_id);

        let mut page2 = Page::init(Capacity {
            cols: 1,
            rows: 1,
            styles: 8,
            ..Capacity::new(1, 1)
        })
        .unwrap();
        let italic = style::Style {
            flags: style::Flags {
                italic: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let occupied_id = page2.add_style(italic).unwrap();
        assert_eq!(occupied_id, source_id);

        page2.clone_rows_from(&page, 0, 1).unwrap();

        let copied_id = page2.cell_copy_at(0, 0).style_id();
        assert_ne!(copied_id, source_id);
        assert_eq!(page2.get_style(copied_id), bold);
        assert_eq!(page2.style_ref_count(copied_id), 1);
        assert_eq!(page2.get_style(source_id), italic);
    }

    #[test]
    fn page_clone_from_style_insert_failure_leaves_valid_cells() {
        let mut page = Page::init(Capacity {
            cols: 1,
            rows: 1,
            styles: 8,
            ..Capacity::new(1, 1)
        })
        .unwrap();
        let bold = style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        let source_id = page.add_style(bold).unwrap();
        let rac = page.get_row_and_cell_mut(0, 0);
        rac.row.set_styled(true);
        *rac.cell = Cell::init('b' as u32);
        rac.cell.set_style_id(source_id);
        page.use_style(source_id);
        page.release_style(source_id);

        let mut page2 = Page::init(Capacity::with_metadata(
            1,
            1,
            0,
            HYPERLINK_BYTES_DEFAULT,
            GRAPHEME_BYTES_DEFAULT,
            STRING_BYTES_DEFAULT,
        ))
        .unwrap();
        *page2.get_row_and_cell_mut(0, 0).cell = Cell::init('x' as u32);

        assert_eq!(
            page2.clone_rows_from(&page, 0, 1),
            Err(CloneFromError::Style(
                super::super::ref_counted_set::AddError::OutOfMemory
            ))
        );
        assert_eq!(page2.cell_copy_at(0, 0).codepoint(), 'b' as u32);
        assert_eq!(page2.cell_copy_at(0, 0).style_id(), style::DEFAULT_ID);
        assert!(!page2.get_row(0).styled());
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
    fn page_exact_row_capacity_empty_rows() {
        let page = Page::init(Capacity::with_metadata(
            10,
            10,
            8,
            HYPERLINK_BYTES_DEFAULT,
            GRAPHEME_BYTES_DEFAULT,
            STRING_BYTES_DEFAULT,
        ))
        .unwrap();

        let cap = page.exact_row_capacity(0, 5);
        assert_eq!(cap.cols, 10);
        assert_eq!(cap.rows, 5);
        assert_eq!(cap.styles, 0);
        assert_eq!(cap.grapheme_bytes, 0);
        assert_eq!(cap.hyperlink_bytes, 0);
        assert_eq!(cap.string_bytes, 0);
    }

    #[test]
    fn page_exact_row_capacity_styles() {
        let mut page = Page::init(Capacity::new(10, 10)).unwrap();
        assert_eq!(page.exact_row_capacity(0, 5).styles, 0);

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
        let underline = style::Style {
            flags: style::Flags {
                underline: Underline::Single,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };

        let bold_id = page.add_style(bold).unwrap();
        let rac = page.get_row_and_cell_mut(0, 0);
        rac.row.set_styled(true);
        *rac.cell = Cell::init('a' as u32);
        rac.cell.set_style_id(bold_id);

        let one_style = page.exact_row_capacity(0, 5);
        assert_eq!(
            one_style.styles,
            style::Set::capacity_for_count(1) as CellCountInt
        );

        let rac = page.get_row_and_cell_mut(1, 0);
        rac.cell.set_style_id(bold_id);
        assert_eq!(page.exact_row_capacity(0, 5).styles, one_style.styles);

        let italic_id = page.add_style(italic).unwrap();
        let rac = page.get_row_and_cell_mut(2, 0);
        rac.cell.set_style_id(italic_id);
        let two_styles = page.exact_row_capacity(0, 5);
        assert_eq!(
            two_styles.styles,
            style::Set::capacity_for_count(2) as CellCountInt
        );

        let underline_id = page.add_style(underline).unwrap();
        let rac = page.get_row_and_cell_mut(0, 7);
        rac.row.set_styled(true);
        rac.cell.set_style_id(underline_id);
        assert_eq!(page.exact_row_capacity(0, 5).styles, two_styles.styles);
        assert_eq!(
            page.exact_row_capacity(0, 10).styles,
            style::Set::capacity_for_count(3) as CellCountInt
        );

        let mut cloned = Page::init(two_styles).unwrap();
        cloned.clone_rows_from(&page, 0, 5).unwrap();
        assert_eq!(cloned.exact_row_capacity(0, 5), two_styles);
    }

    #[test]
    fn page_exact_row_capacity_graphemes() {
        let mut page = Page::init(Capacity::new(10, 10)).unwrap();
        assert_eq!(page.exact_row_capacity(0, 5).grapheme_bytes, 0);

        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);
        page.append_grapheme_at(0, 0, 0x0301).unwrap();
        assert_eq!(
            page.exact_row_capacity(0, 5).grapheme_bytes,
            GRAPHEME_CHUNK as GraphemeBytesInt
        );

        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('e' as u32);
        page.append_grapheme_at(1, 0, 0x0300).unwrap();
        assert_eq!(
            page.exact_row_capacity(0, 5).grapheme_bytes,
            (GRAPHEME_CHUNK * 2) as GraphemeBytesInt
        );

        *page.get_row_and_cell_mut(2, 0).cell = Cell::init('o' as u32);
        for cp in [0x0301, 0x0302, 0x0303] {
            page.append_grapheme_at(2, 0, cp).unwrap();
        }
        assert_eq!(
            page.exact_row_capacity(0, 5).grapheme_bytes,
            (GRAPHEME_CHUNK * 3) as GraphemeBytesInt
        );

        *page.get_row_and_cell_mut(0, 7).cell = Cell::init('x' as u32);
        page.append_grapheme_at(0, 7, 0x0304).unwrap();
        assert_eq!(
            page.exact_row_capacity(0, 5).grapheme_bytes,
            (GRAPHEME_CHUNK * 3) as GraphemeBytesInt
        );
        assert_eq!(
            page.exact_row_capacity(0, 10).grapheme_bytes,
            (GRAPHEME_CHUNK * 4) as GraphemeBytesInt
        );

        *page.get_row_and_cell_mut(4, 4).cell = Cell::init('z' as u32);
        for i in 0..6 {
            page.append_grapheme_at(4, 4, 0x0320 + i).unwrap();
        }
        assert_eq!(
            page.exact_row_capacity(4, 5).grapheme_bytes,
            (GRAPHEME_CHUNK * 2) as GraphemeBytesInt
        );

        let cap = page.exact_row_capacity(0, 5);
        let mut cloned = Page::init(cap).unwrap();
        cloned.clone_rows_from(&page, 0, 5).unwrap();
        assert_eq!(cloned.exact_row_capacity(0, 5), cap);
    }

    #[test]
    fn page_exact_row_capacity_hyperlinks() {
        let mut page = Page::init(Capacity::with_metadata(
            10,
            10,
            8,
            (16 * HYPERLINK_SET_ITEM_SIZE) as HyperlinkCountInt,
            GRAPHEME_BYTES_DEFAULT,
            STRING_BYTES_DEFAULT,
        ))
        .unwrap();
        let empty = page.exact_row_capacity(0, 5);
        assert_eq!(empty.hyperlink_bytes, 0);
        assert_eq!(empty.string_bytes, 0);

        let implicit = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com",
            })
            .unwrap();
        *page.get_row_and_cell_mut(0, 0).cell = Cell::init('a' as u32);
        page.set_hyperlink(0, 0, implicit).unwrap();

        let one_link = page.exact_row_capacity(0, 5);
        assert_eq!(
            one_link.hyperlink_bytes,
            (hyperlink::Set::capacity_for_count(1) * HYPERLINK_SET_ITEM_SIZE) as HyperlinkCountInt
        );
        assert_eq!(one_link.string_bytes, STRING_CHUNK as StringBytesInt);

        *page.get_row_and_cell_mut(1, 0).cell = Cell::init('b' as u32);
        page.use_hyperlink(implicit);
        page.set_hyperlink(1, 0, implicit).unwrap();
        assert_eq!(page.exact_row_capacity(0, 5), one_link);

        let explicit = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"my-link-id"),
                uri: b"https://other.example.org/path",
            })
            .unwrap();
        *page.get_row_and_cell_mut(2, 0).cell = Cell::init('c' as u32);
        page.set_hyperlink(2, 0, explicit).unwrap();
        let two_links = page.exact_row_capacity(0, 5);
        assert_eq!(
            two_links.hyperlink_bytes,
            (hyperlink::Set::capacity_for_count(2) * HYPERLINK_SET_ITEM_SIZE) as HyperlinkCountInt
        );
        assert_eq!(two_links.string_bytes, (STRING_CHUNK * 3) as StringBytesInt);

        let outside = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(99),
                uri: b"https://outside.example.com",
            })
            .unwrap();
        *page.get_row_and_cell_mut(0, 7).cell = Cell::init('x' as u32);
        page.set_hyperlink(0, 7, outside).unwrap();
        assert_eq!(page.exact_row_capacity(0, 5), two_links);
        assert_eq!(
            page.exact_row_capacity(0, 10).string_bytes,
            (STRING_CHUNK * 4) as StringBytesInt
        );

        let mut cloned = Page::init(two_links).unwrap();
        cloned.clone_rows_from(&page, 0, 5).unwrap();
        assert_eq!(cloned.exact_row_capacity(0, 5), two_links);
    }

    #[test]
    fn page_exact_row_capacity_hyperlink_map_for_many_cells() {
        let cols = 50_usize;
        let mut page = Page::init(Capacity::new(cols as CellCountInt, 2)).unwrap();
        let id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Implicit(1),
                uri: b"https://example.com",
            })
            .unwrap();

        for x in 0..cols {
            *page.get_row_and_cell_mut(x, 0).cell = Cell::init('x' as u32);
            if x > 0 {
                page.use_hyperlink(id);
            }
            page.set_hyperlink(x, 0, id).unwrap();
        }

        let cap = page.exact_row_capacity(0, 1);
        let min_for_map = cols.div_ceil(HYPERLINK_CELL_MULTIPLIER);
        let min_bytes = min_for_map * HYPERLINK_SET_ITEM_SIZE;
        assert!(cap.hyperlink_bytes as usize >= min_bytes);

        let mut cloned = Page::init(cap).unwrap();
        cloned.clone_rows_from(&page, 0, 1).unwrap();
        assert_eq!(cloned.hyperlink_count(), cols);
        assert_eq!(cloned.exact_row_capacity(0, 1), cap);
    }

    #[test]
    fn page_exact_row_capacity_mixed_data_clone() {
        let mut page = Page::init(Capacity::new(5, 3)).unwrap();
        let style_id = page
            .add_style(style::Style {
                flags: style::Flags {
                    bold: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            })
            .unwrap();
        *page.get_row_and_cell_mut(0, 1).cell = Cell::init('s' as u32);
        page.get_row_and_cell_mut(0, 1).cell.set_style_id(style_id);
        page.get_row_mut(1).set_styled(true);

        *page.get_row_and_cell_mut(1, 1).cell = Cell::init('g' as u32);
        page.append_grapheme_at(1, 1, 0x0301).unwrap();

        let link_id = page
            .insert_hyperlink(hyperlink::Hyperlink {
                id: hyperlink::HyperlinkId::Explicit(b"id"),
                uri: b"https://example.com",
            })
            .unwrap();
        *page.get_row_and_cell_mut(2, 1).cell = Cell::init('h' as u32);
        page.set_hyperlink(2, 1, link_id).unwrap();

        let cap = page.exact_row_capacity(1, 2);
        let mut cloned = Page::init(cap).unwrap();
        cloned.clone_rows_from(&page, 1, 2).unwrap();
        assert_eq!(cloned.exact_row_capacity(0, 1), cap);
    }

    #[test]
    #[should_panic(expected = "assertion failed: y_start < y_end")]
    fn page_exact_row_capacity_rejects_empty_range() {
        let page = Page::init(Capacity::new(2, 2)).unwrap();
        let _ = page.exact_row_capacity(1, 1);
    }

    #[test]
    #[should_panic(expected = "assertion failed: y_end <= self.size.rows as usize")]
    fn page_exact_row_capacity_rejects_end_out_of_bounds() {
        let page = Page::init(Capacity::new(2, 2)).unwrap();
        let _ = page.exact_row_capacity(0, 3);
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
