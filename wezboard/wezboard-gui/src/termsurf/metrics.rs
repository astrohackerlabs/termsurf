use std::sync::atomic::{AtomicU32, Ordering};

static CELL_WIDTH: AtomicU32 = AtomicU32::new(0);
static CELL_HEIGHT: AtomicU32 = AtomicU32::new(0);
static PADDING_LEFT: AtomicU32 = AtomicU32::new(0);
static PADDING_TOP: AtomicU32 = AtomicU32::new(0);

pub fn set(cell_width: u32, cell_height: u32, padding_left: u32, padding_top: u32) {
    CELL_WIDTH.store(cell_width, Ordering::Relaxed);
    CELL_HEIGHT.store(cell_height, Ordering::Relaxed);
    PADDING_LEFT.store(padding_left, Ordering::Relaxed);
    PADDING_TOP.store(padding_top, Ordering::Relaxed);
}

pub fn get() -> (u32, u32, u32, u32) {
    (
        CELL_WIDTH.load(Ordering::Relaxed),
        CELL_HEIGHT.load(Ordering::Relaxed),
        PADDING_LEFT.load(Ordering::Relaxed),
        PADDING_TOP.load(Ordering::Relaxed),
    )
}
