use parking_lot::Mutex;

pub struct TermSurfState {
    // Empty registries for now — future experiments will add:
    // - server connections (Chromium engines, keyed by profile)
    // - TUI connections
    // - tab mappings (pane_id → tab_id)
    // - browser pane state
}

lazy_static::lazy_static! {
    static ref STATE: Mutex<TermSurfState> = Mutex::new(TermSurfState {});
}

pub fn state() -> parking_lot::MutexGuard<'static, TermSurfState> {
    STATE.lock()
}
