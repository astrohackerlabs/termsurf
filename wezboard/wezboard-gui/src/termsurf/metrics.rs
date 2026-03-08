use std::sync::atomic::{AtomicU32, Ordering};

static CELL_WIDTH: AtomicU32 = AtomicU32::new(0);
static CELL_HEIGHT: AtomicU32 = AtomicU32::new(0);
static CONTENT_ORIGIN_X: AtomicU32 = AtomicU32::new(0);
static CONTENT_ORIGIN_Y: AtomicU32 = AtomicU32::new(0);

pub fn set(cell_width: u32, cell_height: u32, origin_x: u32, origin_y: u32) {
    CELL_WIDTH.store(cell_width, Ordering::Relaxed);
    CELL_HEIGHT.store(cell_height, Ordering::Relaxed);
    CONTENT_ORIGIN_X.store(origin_x, Ordering::Relaxed);
    CONTENT_ORIGIN_Y.store(origin_y, Ordering::Relaxed);
}

pub fn get() -> (u32, u32, u32, u32) {
    (
        CELL_WIDTH.load(Ordering::Relaxed),
        CELL_HEIGHT.load(Ordering::Relaxed),
        CONTENT_ORIGIN_X.load(Ordering::Relaxed),
        CONTENT_ORIGIN_Y.load(Ordering::Relaxed),
    )
}
