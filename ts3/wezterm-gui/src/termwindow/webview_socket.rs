//! Socket server for webview overlay communication with the coordinator.
//!
//! This module provides a Unix domain socket server that allows the `web` CLI
//! coordinator to send webview display requests to the GUI process.
//!
//! The GUI spawns and manages profile server processes, enabling IOSurface
//! sharing through process ancestry (GUI is ancestor of CEF GPU process).
//!
//! Socket path: /tmp/termsurf-gui-{pid}.sock

use mux::pane::PaneId;
use mux::tab::TabId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::thread;
use std::time::{Duration, Instant};

// ============================================================================
// Cell Size for Pane Dimension Calculation
// ============================================================================

/// Global cell size for pane dimension calculations.
/// Updated by TermWindow when render_metrics changes.
static CELL_WIDTH: AtomicU32 = AtomicU32::new(8);
static CELL_HEIGHT: AtomicU32 = AtomicU32::new(16);

/// Update the global cell size. Called by TermWindow.
pub fn set_cell_size(width: u32, height: u32) {
    CELL_WIDTH.store(width, Ordering::Relaxed);
    CELL_HEIGHT.store(height, Ordering::Relaxed);
    log::debug!("[GUI Socket] Cell size updated: {}x{}", width, height);
}

fn get_cell_size() -> (u32, u32) {
    (
        CELL_WIDTH.load(Ordering::Relaxed),
        CELL_HEIGHT.load(Ordering::Relaxed),
    )
}

// ============================================================================
// Global Server Instance
// ============================================================================

/// Global socket server instance
static SERVER: OnceLock<Arc<WebviewSocketServer>> = OnceLock::new();

/// Get the global socket server instance
pub fn get_server() -> Option<Arc<WebviewSocketServer>> {
    SERVER.get().cloned()
}

/// Start the global socket server. Call this early in main() before any windows are created.
pub fn start_server() -> anyhow::Result<()> {
    let server = WebviewSocketServer::new()?;
    let server = Arc::new(server);

    // Store globally
    SERVER
        .set(server.clone())
        .map_err(|_| anyhow::anyhow!("Socket server already started"))?;

    // Set environment variable for child processes BEFORE starting accept loop
    std::env::set_var("TERMSURF_GUI_SOCKET", server.socket_path());
    log::info!(
        "[GUI Socket] Set TERMSURF_GUI_SOCKET={}",
        server.socket_path().display()
    );

    // Start XPC manager for IOSurface Mach port transfer
    if let Err(e) = super::webview_xpc::start_xpc_manager() {
        log::warn!("[GUI Socket] Failed to start XPC manager: {}", e);
        // Continue anyway - XPC is optional for now
    }

    // Start accept loop in background thread
    server.start();

    Ok(())
}

// ============================================================================
// Protocol Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
struct Request {
    id: String,
    action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Response {
    id: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl Response {
    fn ok(id: &str, data: Option<serde_json::Value>) -> Self {
        Self {
            id: id.to_string(),
            status: "ok".to_string(),
            data,
            error: None,
        }
    }

    fn error(id: &str, error: &str) -> Self {
        Self {
            id: id.to_string(),
            status: "error".to_string(),
            data: None,
            error: Some(error.to_string()),
        }
    }
}

// ============================================================================
// Profile Server Management
// ============================================================================

/// Key for tracking profile servers: (engine_path, profile_name)
type ProfileServerKey = (PathBuf, String);

/// A managed profile server process
struct ProfileServer {
    /// The child process handle
    _process: Child,
    /// Socket path for this profile server
    socket_path: PathBuf,
    /// Connection to the profile server (lazy, established on first use)
    connection: Option<UnixStream>,
    /// Counter for generating request IDs
    next_request_id: u64,
}

impl ProfileServer {
    /// Spawn a new profile server and wait for its socket to become available
    fn spawn(engine: &PathBuf, profile: &str) -> Result<Self, String> {
        // Determine socket path based on profile
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let socket_path = PathBuf::from(format!(
            "{}/.config/termsurf/sockets/{}.sock",
            home, profile
        ));

        // Ensure sockets directory exists
        if let Some(parent) = socket_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        log::info!(
            "[GUI Socket] Spawning profile server: engine={:?}, profile={}",
            engine,
            profile
        );

        // Spawn the profile server process
        let process = Command::new(engine)
            .arg("--profile-server")
            .arg("--profile")
            .arg(profile)
            .stdout(Stdio::inherit()) // For debugging
            .stderr(Stdio::inherit())
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn profile server: {}", e))?;

        log::info!(
            "[GUI Socket] Profile server spawned, pid={}",
            process.id()
        );

        Ok(Self {
            _process: process,
            socket_path,
            connection: None,
            next_request_id: 1,
        })
    }

    /// Wait for the socket to become available and establish a connection
    fn connect(&mut self) -> Result<&mut UnixStream, String> {
        if self.connection.is_some() {
            return Ok(self.connection.as_mut().unwrap());
        }

        log::info!(
            "[GUI Socket] Waiting for profile server socket at {:?}",
            self.socket_path
        );

        // Retry with exponential backoff (up to 5 seconds total)
        let start = Instant::now();
        let max_wait = Duration::from_secs(5);
        let mut delay = Duration::from_millis(50);

        while start.elapsed() < max_wait {
            if self.socket_path.exists() {
                match UnixStream::connect(&self.socket_path) {
                    Ok(stream) => {
                        log::info!("[GUI Socket] Connected to profile server");
                        self.connection = Some(stream);
                        return Ok(self.connection.as_mut().unwrap());
                    }
                    Err(e) => {
                        log::debug!(
                            "[GUI Socket] Connection attempt failed: {}, retrying...",
                            e
                        );
                    }
                }
            }

            thread::sleep(delay);
            delay = std::cmp::min(delay * 2, Duration::from_millis(500));
        }

        Err(format!(
            "Timeout waiting for profile server socket at {:?}",
            self.socket_path
        ))
    }

    /// Send a request to the profile server and get a response
    fn send_request(
        &mut self,
        action: &str,
        data: Option<serde_json::Value>,
    ) -> Result<Response, String> {
        // Get request ID first before borrowing stream
        let request_id = self.next_request_id;
        self.next_request_id += 1;

        let request = Request {
            id: format!("req-{}", request_id),
            action: action.to_string(),
            data,
        };

        let request_json =
            serde_json::to_string(&request).map_err(|e| format!("JSON serialize error: {}", e))?;

        log::debug!("[GUI Socket] -> Profile server: {}", request_json);

        // Now borrow stream
        let stream = self.connect()?;
        writeln!(stream, "{}", request_json).map_err(|e| format!("Write error: {}", e))?;
        stream.flush().map_err(|e| format!("Flush error: {}", e))?;

        // Read response
        let mut reader = BufReader::new(stream.try_clone().map_err(|e| format!("Clone error: {}", e))?);
        let mut response_line = String::new();
        reader
            .read_line(&mut response_line)
            .map_err(|e| format!("Read error: {}", e))?;

        log::debug!("[GUI Socket] <- Profile server: {}", response_line.trim());

        serde_json::from_str(&response_line).map_err(|e| format!("JSON parse error: {}", e))
    }
}

/// Manages all profile server processes
struct ProfileServerManager {
    servers: HashMap<ProfileServerKey, ProfileServer>,
    next_webview_id: u64,
}

impl ProfileServerManager {
    fn new() -> Self {
        Self {
            servers: HashMap::new(),
            next_webview_id: 1,
        }
    }

    /// Get or spawn a profile server for the given engine and profile
    fn get_or_spawn(
        &mut self,
        engine: &PathBuf,
        profile: &str,
    ) -> Result<&mut ProfileServer, String> {
        let key = (engine.clone(), profile.to_string());

        if !self.servers.contains_key(&key) {
            let server = ProfileServer::spawn(engine, profile)?;
            self.servers.insert(key.clone(), server);
        }

        Ok(self.servers.get_mut(&key).unwrap())
    }

    /// Allocate a new webview ID
    fn next_webview_id(&mut self) -> u64 {
        let id = self.next_webview_id;
        self.next_webview_id += 1;
        id
    }
}

// ============================================================================
// Webview Overlay State
// ============================================================================

/// Mode for webview input handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebviewMode {
    /// Browser is focused, receiving input (future: forward to CEF)
    Browse,
    /// Control panel is focused, browser dimmed
    Control,
}

impl Default for WebviewMode {
    fn default() -> Self {
        WebviewMode::Browse
    }
}

/// Marker that a pane has an active webview.
/// Texture data (mach_port, dimensions) is stored in XpcManager::received_surfaces.
/// This struct only serves as a registry of "which panes have webviews".
#[derive(Debug, Clone)]
pub struct WebviewOverlay {
    /// Session ID for this webview (for cleanup/debugging)
    pub session_id: String,
    /// Tab ID that owns this webview (for filtering during render)
    pub tab_id: TabId,
    /// Current input mode (Browse or Control)
    pub mode: WebviewMode,
}

/// Tracks active webview overlays (global, not per-window)
#[derive(Default)]
pub struct WebviewOverlayState {
    /// Map from pane_id to active webview overlay
    pub overlays: HashMap<PaneId, WebviewOverlay>,
}

impl WebviewOverlayState {
    pub fn new() -> Self {
        Self {
            overlays: HashMap::new(),
        }
    }

    pub fn add_overlay(&mut self, pane_id: PaneId, overlay: WebviewOverlay) {
        log::info!(
            "Marking pane {} as webview (session={})",
            pane_id,
            overlay.session_id
        );
        self.overlays.insert(pane_id, overlay);
    }

    pub fn remove_overlay(&mut self, pane_id: PaneId) -> Option<WebviewOverlay> {
        log::info!("Removing webview overlay from pane {}", pane_id);
        self.overlays.remove(&pane_id)
    }

    pub fn get_overlay(&self, pane_id: PaneId) -> Option<&WebviewOverlay> {
        self.overlays.get(&pane_id)
    }

    pub fn has_overlay(&self, pane_id: PaneId) -> bool {
        self.overlays.contains_key(&pane_id)
    }
}

// ============================================================================
// Socket Server
// ============================================================================

/// Handle a single request and return a response
fn handle_request(
    request: &Request,
    state: &RwLock<WebviewOverlayState>,
    profile_manager: &Mutex<ProfileServerManager>,
) -> Response {
    match request.action.as_str() {
        "ping" => Response::ok(&request.id, Some(serde_json::json!({"pong": true}))),

        "open_webview" => {
            let data = match &request.data {
                Some(d) => d,
                None => return Response::error(&request.id, "Missing data"),
            };

            let pane_id = match data.get("pane_id").and_then(|v| v.as_u64()) {
                Some(id) => id as PaneId,
                None => return Response::error(&request.id, "Missing pane_id"),
            };

            let url = data
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("about:blank");

            log::info!(
                "[GUI Socket] open_webview: pane={}, url={}",
                pane_id,
                url
            );

            // Use XPC to spawn profile server via launcher
            let xpc_manager = match super::webview_xpc::get_xpc_manager() {
                Some(m) => m,
                None => return Response::error(&request.id, "XPC manager not available"),
            };

            let profile = data
                .get("profile")
                .and_then(|v| v.as_str())
                .unwrap_or("default");

            // Look up pane grid dimensions from Mux, compute pixel size using cell size
            let (logical_width, logical_height, scale) = match mux::Mux::try_get() {
                Some(mux) => match mux.get_pane(pane_id) {
                    Some(pane) => {
                        let dims = pane.get_dimensions();
                        let (cell_width, cell_height) = get_cell_size();

                        // Compute scale factor (macOS base DPI = 72)
                        let scale = dims.dpi as f32 / 72.0;
                        let scale = if scale <= 0.0 { 2.0 } else { scale };

                        // Physical pixels = grid cells × cell size
                        let physical_width = dims.cols as f32 * cell_width as f32;
                        let physical_height = dims.viewport_rows as f32 * cell_height as f32;

                        // Logical pixels = physical / scale (CEF expects DIP coordinates)
                        let lw = (physical_width / scale) as u32;
                        let lh = (physical_height / scale) as u32;

                        log::info!(
                            "[GUI Socket] Pane {} dimensions: {}cols x {}rows, cell={}x{}, physical={}x{}, scale={}, logical={}x{}",
                            pane_id, dims.cols, dims.viewport_rows, cell_width, cell_height,
                            physical_width, physical_height, scale, lw, lh
                        );
                        log::info!(
                            "[SPAWN-SIZE] pane={} grid={}x{} cell={}x{} physical={}x{} scale={:.2} logical={}x{}",
                            pane_id,
                            dims.cols,
                            dims.viewport_rows,
                            cell_width,
                            cell_height,
                            physical_width,
                            physical_height,
                            scale,
                            lw,
                            lh
                        );
                        (lw, lh, scale)
                    }
                    None => {
                        log::warn!(
                            "[GUI Socket] Pane {} not found, using default 800x600",
                            pane_id
                        );
                        (800u32, 600u32, 2.0f32)
                    }
                },
                None => {
                    log::warn!("[GUI Socket] Mux not available, using default 800x600");
                    (800u32, 600u32, 2.0f32)
                }
            };

            // Request profile spawn (this triggers launcher -> termsurf-profile -> Mach port transfer)
            let session_id =
                match xpc_manager.request_profile_spawn(pane_id, url, profile, logical_width, logical_height, scale) {
                Ok(id) => id,
                Err(e) => {
                    log::error!("[GUI Socket] Failed to request profile spawn: {}", e);
                    return Response::error(&request.id, &format!("Failed to spawn: {}", e));
                }
            };

            log::info!("[GUI Socket] Spawned profile with session_id={}", session_id);

            // Wait for surface to be received via XPC (poll with timeout)
            let max_wait = std::time::Duration::from_secs(5);
            let poll_interval = std::time::Duration::from_millis(100);
            let start = std::time::Instant::now();

            let surface = loop {
                if let Some(s) = xpc_manager.get_received_surface(pane_id) {
                    break s;
                }
                if start.elapsed() > max_wait {
                    log::error!("[GUI Socket] Timeout waiting for surface from XPC");
                    return Response::error(&request.id, "Timeout waiting for surface");
                }
                std::thread::sleep(poll_interval);
            };

            log::info!(
                "[GUI Socket] Received surface via XPC: mach_port={}, size={}x{}",
                surface.mach_port,
                surface.width,
                surface.height
            );

            // Allocate webview ID and mark pane as webview
            // (texture data stays in XpcManager::received_surfaces)
            let webview_id = profile_manager.lock().unwrap().next_webview_id();

            // Get tab_id for this pane (used for filtering during render)
            let tab_id = match mux::Mux::try_get() {
                Some(mux) => match mux.resolve_pane_id(pane_id) {
                    Some((_domain_id, _window_id, tab_id)) => tab_id,
                    None => {
                        log::warn!("[GUI Socket] Could not resolve tab_id for pane {}", pane_id);
                        return Response::error(&request.id, "Could not resolve pane to tab");
                    }
                },
                None => {
                    log::warn!("[GUI Socket] Mux not available for tab_id lookup");
                    return Response::error(&request.id, "Mux not available");
                }
            };

            let overlay = WebviewOverlay {
                session_id: session_id.clone(),
                tab_id,
                mode: WebviewMode::default(),
            };

            state.write().unwrap().add_overlay(pane_id, overlay);

            Response::ok(
                &request.id,
                Some(serde_json::json!({
                    "webview_id": webview_id,
                    "mach_port": surface.mach_port,
                    "width": surface.width,
                    "height": surface.height
                })),
            )
        }

        "display_webview" => {
            // Legacy action - accepts mach_port for direct overlay display
            // WARNING: This bypasses XPC and won't work correctly with resize.
            // Texture data should come through XpcManager for resize support.
            log::warn!("[GUI Socket] display_webview is deprecated - use spawn_browser for resize support");

            let data = match &request.data {
                Some(d) => d,
                None => return Response::error(&request.id, "Missing data"),
            };

            let pane_id = match data.get("pane_id").and_then(|v| v.as_u64()) {
                Some(id) => id as PaneId,
                None => return Response::error(&request.id, "Missing pane_id"),
            };

            // Note: mach_port, width, height are ignored - texture data must come via XPC
            let _mach_port = data.get("mach_port").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let _width = data.get("width").and_then(|v| v.as_u64()).unwrap_or(800) as u32;
            let _height = data.get("height").and_then(|v| v.as_u64()).unwrap_or(600) as u32;

            // Get tab_id for this pane (used for filtering during render)
            let tab_id = match mux::Mux::try_get() {
                Some(mux) => match mux.resolve_pane_id(pane_id) {
                    Some((_domain_id, _window_id, tab_id)) => tab_id,
                    None => {
                        log::warn!("[GUI Socket] Could not resolve tab_id for pane {}", pane_id);
                        return Response::error(&request.id, "Could not resolve pane to tab");
                    }
                },
                None => {
                    log::warn!("[GUI Socket] Mux not available for tab_id lookup");
                    return Response::error(&request.id, "Mux not available");
                }
            };

            let overlay = WebviewOverlay {
                session_id: format!("legacy-pane-{}", pane_id),
                tab_id,
                mode: WebviewMode::default(),
            };

            state.write().unwrap().add_overlay(pane_id, overlay);

            Response::ok(&request.id, Some(serde_json::json!({"status": "displayed"})))
        }

        "close_webview" => {
            let data = match &request.data {
                Some(d) => d,
                None => return Response::error(&request.id, "Missing data"),
            };

            let pane_id = match data.get("pane_id").and_then(|v| v.as_u64()) {
                Some(id) => id as PaneId,
                None => return Response::error(&request.id, "Missing pane_id"),
            };

            let removed = state.write().unwrap().remove_overlay(pane_id).is_some();

            Response::ok(
                &request.id,
                Some(serde_json::json!({"removed": removed})),
            )
        }

        "get_status" => {
            let overlay_count = state.read().unwrap().overlays.len();
            let manager = profile_manager.lock().unwrap();
            let server_count = manager.servers.len();
            Response::ok(
                &request.id,
                Some(serde_json::json!({
                    "overlay_count": overlay_count,
                    "server_count": server_count,
                    "pid": std::process::id()
                })),
            )
        }

        "test_xpc" => {
            // Test XPC Mach port transfer (Experiment 2)
            let data = match &request.data {
                Some(d) => d,
                None => return Response::error(&request.id, "Missing data"),
            };

            let pane_id = match data.get("pane_id").and_then(|v| v.as_u64()) {
                Some(id) => id as PaneId,
                None => return Response::error(&request.id, "Missing pane_id"),
            };

            log::info!("[GUI Socket] test_xpc: requesting profile spawn for pane {}", pane_id);

            // Get XPC manager and request profile spawn
            let xpc_manager = match super::webview_xpc::get_xpc_manager() {
                Some(m) => m,
                None => return Response::error(&request.id, "XPC manager not available"),
            };

            match xpc_manager.request_profile_spawn(pane_id, "about:blank", "default", 800, 600, 2.0) {
                Ok(session_id) => {
                    log::info!("[GUI Socket] test_xpc: spawned with session_id={}", session_id);
                    Response::ok(
                        &request.id,
                        Some(serde_json::json!({
                            "session_id": session_id,
                            "pane_id": pane_id
                        })),
                    )
                }
                Err(e) => {
                    log::error!("[GUI Socket] test_xpc: failed to spawn: {}", e);
                    Response::error(&request.id, &format!("Failed to spawn: {}", e))
                }
            }
        }

        "check_xpc_surface" => {
            // Check if an IOSurface has been received via XPC for a pane
            let data = match &request.data {
                Some(d) => d,
                None => return Response::error(&request.id, "Missing data"),
            };

            let pane_id = match data.get("pane_id").and_then(|v| v.as_u64()) {
                Some(id) => id as PaneId,
                None => return Response::error(&request.id, "Missing pane_id"),
            };

            let xpc_manager = match super::webview_xpc::get_xpc_manager() {
                Some(m) => m,
                None => return Response::error(&request.id, "XPC manager not available"),
            };

            match xpc_manager.get_received_surface(pane_id) {
                Some(surface) => {
                    Response::ok(
                        &request.id,
                        Some(serde_json::json!({
                            "found": true,
                            "mach_port": surface.mach_port,
                            "width": surface.width,
                            "height": surface.height
                        })),
                    )
                }
                None => {
                    Response::ok(
                        &request.id,
                        Some(serde_json::json!({
                            "found": false
                        })),
                    )
                }
            }
        }

        _ => Response::error(&request.id, &format!("Unknown action: {}", request.action)),
    }
}

/// Handle a single client connection
fn handle_connection(
    mut stream: UnixStream,
    state: Arc<RwLock<WebviewOverlayState>>,
    profile_manager: Arc<Mutex<ProfileServerManager>>,
) {
    log::info!("[GUI Socket] Client connected");

    let reader = match stream.try_clone() {
        Ok(r) => BufReader::new(r),
        Err(e) => {
            log::error!("[GUI Socket] Failed to clone stream: {}", e);
            return;
        }
    };

    for line in reader.lines() {
        match line {
            Ok(line) if line.is_empty() => continue,
            Ok(line) => {
                let response = match serde_json::from_str::<Request>(&line) {
                    Ok(request) => {
                        log::debug!("[GUI Socket] Request: {:?}", request);
                        handle_request(&request, &state, &profile_manager)
                    }
                    Err(e) => Response::error("unknown", &format!("Invalid JSON: {}", e)),
                };

                let response_json = match serde_json::to_string(&response) {
                    Ok(json) => json,
                    Err(e) => {
                        log::error!("[GUI Socket] Failed to serialize response: {}", e);
                        continue;
                    }
                };

                log::debug!("[GUI Socket] Response: {}", response_json);

                if let Err(e) = writeln!(stream, "{}", response_json) {
                    log::error!("[GUI Socket] Failed to write response: {}", e);
                    break;
                }
                let _ = stream.flush();
            }
            Err(e) => {
                log::error!("[GUI Socket] Error reading: {}", e);
                break;
            }
        }
    }

    log::info!("[GUI Socket] Client disconnected");
}

/// Socket server for webview overlay communication
pub struct WebviewSocketServer {
    socket_path: PathBuf,
    listener: Mutex<Option<UnixListener>>,
    state: Arc<RwLock<WebviewOverlayState>>,
    profile_manager: Arc<Mutex<ProfileServerManager>>,
}

impl WebviewSocketServer {
    /// Create a new socket server (but don't start accepting yet)
    fn new() -> anyhow::Result<Self> {
        let pid = std::process::id();
        let socket_path = PathBuf::from(format!("/tmp/termsurf-gui-{}.sock", pid));

        // Remove stale socket if it exists
        if socket_path.exists() {
            fs::remove_file(&socket_path)?;
        }

        let listener = UnixListener::bind(&socket_path)?;

        // Set socket permissions (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))?;
        }

        log::info!("[GUI Socket] Server created at {:?}", socket_path);

        Ok(Self {
            socket_path,
            listener: Mutex::new(Some(listener)),
            state: Arc::new(RwLock::new(WebviewOverlayState::new())),
            profile_manager: Arc::new(Mutex::new(ProfileServerManager::new())),
        })
    }

    /// Start the accept loop in a background thread
    fn start(self: &Arc<Self>) {
        let listener = self.listener.lock().unwrap().take();
        let Some(listener) = listener else {
            log::error!("[GUI Socket] Server already started");
            return;
        };

        let state = Arc::clone(&self.state);
        let profile_manager = Arc::clone(&self.profile_manager);
        let socket_path = self.socket_path.clone();

        thread::spawn(move || {
            log::info!("[GUI Socket] Accept loop started");

            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let state_clone = Arc::clone(&state);
                        let profile_manager_clone = Arc::clone(&profile_manager);
                        thread::spawn(move || {
                            handle_connection(stream, state_clone, profile_manager_clone);
                        });
                    }
                    Err(e) => {
                        log::error!("[GUI Socket] Accept error: {}", e);
                    }
                }
            }

            // Cleanup socket on exit
            let _ = fs::remove_file(&socket_path);
            log::info!("[GUI Socket] Server stopped");
        });
    }

    /// Get the socket path
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    /// Get a reference to the overlay state for rendering
    pub fn state(&self) -> &Arc<RwLock<WebviewOverlayState>> {
        &self.state
    }
}

impl Drop for WebviewSocketServer {
    fn drop(&mut self) {
        // Cleanup socket file
        let _ = fs::remove_file(&self.socket_path);
    }
}
