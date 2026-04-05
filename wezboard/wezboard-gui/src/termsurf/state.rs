use std::collections::HashMap;
use std::process::Child;
use std::sync::{Arc, Mutex, OnceLock};

use smol::channel::Sender;

/// Per-pane state. One pane = one browser overlay in one terminal pane.
pub struct Pane {
    pub pane_id: String,
    pub profile: String,
    pub browser: String,
    pub url: String,
    pub col: u64,
    pub row: u64,
    pub pixel_width: u64,
    pub pixel_height: u64,
    pub tab_id: i64,
    pub tui_tx: Sender<Vec<u8>>,
    pub browsing: bool,
    pub dark: bool,
    pub inspected_tab_id: i64,
    pub ca_context_id: u32,
    pub pending_context_id: Option<u32>,
    pub ca_layer_host: usize,
    pub ca_layer_flipped: usize,
    pub ca_layer_positioning: usize,
    pub overlay_origin_x: f64,
    pub overlay_origin_y: f64,
    pub overlay_scale: f64,
    pub cursor_type: i64,
    pub visible: bool,
}

/// Per-server state. One server = one Roamium process = one profile.
pub struct Server {
    pub profile: String,
    pub browser: String,
    #[allow(dead_code)]
    pub process: Option<Child>,
    pub tx: Option<Sender<Vec<u8>>>,
    pub listen_socket: String,
    pub pane_count: usize,
}

/// Global shared state for the TermSurf protocol.
pub struct TermSurfState {
    /// pane_id → Pane
    pub panes: HashMap<String, Pane>,
    /// "{profile}\0{browser}" → Server
    pub servers: HashMap<String, Server>,
    /// tab_id → pane_id
    pub tab_to_pane: HashMap<i64, String>,
    /// Currently focused pane (only one at a time)
    pub focused_pane: Option<String>,
    /// Last browser pane (for DevTools auto-targeting)
    pub last_browser_pane: Option<String>,
    /// mux_window_id → overlay NSView pointer (macOS only)
    pub overlay_views: HashMap<usize, usize>,
}

impl TermSurfState {
    pub fn new() -> Self {
        Self {
            panes: HashMap::new(),
            servers: HashMap::new(),
            tab_to_pane: HashMap::new(),
            focused_pane: None,
            last_browser_pane: None,
            overlay_views: HashMap::new(),
        }
    }

    pub fn server_key(profile: &str, browser: &str) -> String {
        format!("{}\0{}", profile, browser)
    }
}

pub type SharedState = Arc<Mutex<TermSurfState>>;

static GLOBAL_STATE: OnceLock<SharedState> = OnceLock::new();

pub fn init_global(state: SharedState) {
    GLOBAL_STATE.set(state).ok();
}

pub fn global() -> Option<&'static SharedState> {
    GLOBAL_STATE.get()
}
