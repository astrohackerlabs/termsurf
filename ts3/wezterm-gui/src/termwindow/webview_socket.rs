//! Socket server for webview overlay communication with the coordinator.
//!
//! This module provides a Unix domain socket server that allows the `web` CLI
//! coordinator to send webview display requests to the GUI process.
//!
//! Socket path: /tmp/termsurf-gui-{pid}.sock

use mux::pane::PaneId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::thread;

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
// Webview Overlay State
// ============================================================================

/// Information about an active webview overlay on a pane
#[derive(Debug, Clone)]
pub struct WebviewOverlay {
    /// IOSurface ID for texture import
    pub iosurface_id: u32,
    /// Width of the webview texture
    pub width: u32,
    /// Height of the webview texture
    pub height: u32,
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
            "Adding webview overlay to pane {}: iosurface_id={}, size={}x{}",
            pane_id,
            overlay.iosurface_id,
            overlay.width,
            overlay.height
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
fn handle_request(request: &Request, state: &RwLock<WebviewOverlayState>) -> Response {
    match request.action.as_str() {
        "ping" => Response::ok(&request.id, Some(serde_json::json!({"pong": true}))),

        "display_webview" => {
            let data = match &request.data {
                Some(d) => d,
                None => return Response::error(&request.id, "Missing data"),
            };

            let pane_id = match data.get("pane_id").and_then(|v| v.as_u64()) {
                Some(id) => id as PaneId,
                None => return Response::error(&request.id, "Missing pane_id"),
            };

            let iosurface_id = match data.get("iosurface_id").and_then(|v| v.as_u64()) {
                Some(id) => id as u32,
                None => return Response::error(&request.id, "Missing iosurface_id"),
            };

            let width = data.get("width").and_then(|v| v.as_u64()).unwrap_or(800) as u32;
            let height = data.get("height").and_then(|v| v.as_u64()).unwrap_or(600) as u32;

            let overlay = WebviewOverlay {
                iosurface_id,
                width,
                height,
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
            Response::ok(
                &request.id,
                Some(serde_json::json!({
                    "overlay_count": overlay_count,
                    "pid": std::process::id()
                })),
            )
        }

        _ => Response::error(&request.id, &format!("Unknown action: {}", request.action)),
    }
}

/// Handle a single client connection
fn handle_connection(mut stream: UnixStream, state: Arc<RwLock<WebviewOverlayState>>) {
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
                        handle_request(&request, &state)
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
        let socket_path = self.socket_path.clone();

        thread::spawn(move || {
            log::info!("[GUI Socket] Accept loop started");

            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let state_clone = Arc::clone(&state);
                        thread::spawn(move || {
                            handle_connection(stream, state_clone);
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
