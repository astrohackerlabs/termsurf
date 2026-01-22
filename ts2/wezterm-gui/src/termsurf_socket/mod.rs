//! Unix domain socket server for CLI-to-GUI communication.
//! Creates a socket at `/tmp/termsurf-{pid}.sock` and listens for connections.

mod connection;
pub mod protocol;

use connection::TermsurfConnection;
use mux::pane::PaneId;
use mux::Mux;
use protocol::{TermsurfEvent, TermsurfRequest, TermsurfResponse};
use std::collections::HashMap;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::thread;

/// Global socket server instance
static SERVER: OnceLock<Arc<TermsurfSocketServer>> = OnceLock::new();

/// Get the global socket server instance
pub fn get_server() -> Option<Arc<TermsurfSocketServer>> {
    SERVER.get().cloned()
}

/// Start the global socket server
pub fn start_server() -> anyhow::Result<()> {
    let server = TermsurfSocketServer::new()?;
    let server = Arc::new(server);

    // Store globally
    SERVER
        .set(server.clone())
        .map_err(|_| anyhow::anyhow!("Socket server already started"))?;

    // Set environment variable for child processes
    std::env::set_var("TERMSURF_SOCKET", server.socket_path());

    // Start accept loop in background thread
    server.start();

    Ok(())
}

/// Unix domain socket server for TermSurf CLI communication
pub struct TermsurfSocketServer {
    socket_path: PathBuf,
    listener: UnixListener,
    connections: RwLock<HashMap<String, Arc<TermsurfConnection>>>,
    running: Mutex<bool>,
}

impl TermsurfSocketServer {
    /// Create a new socket server
    fn new() -> anyhow::Result<Self> {
        let pid = std::process::id();
        let socket_path = PathBuf::from(format!("/tmp/termsurf-{}.sock", pid));

        // Remove existing socket file if present
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        // Create and bind the socket
        let listener = UnixListener::bind(&socket_path)?;

        // Set socket permissions (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;
        }

        log::info!("[TermsurfSocket] Server created at {:?}", socket_path);

        Ok(Self {
            socket_path,
            listener,
            connections: RwLock::new(HashMap::new()),
            running: Mutex::new(false),
        })
    }

    /// Get the socket path
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    /// Start accepting connections in a background thread
    fn start(self: &Arc<Self>) {
        let mut running = self.running.lock().unwrap();
        if *running {
            log::warn!("[TermsurfSocket] Server already running");
            return;
        }
        *running = true;
        drop(running);

        let server = self.clone();
        thread::spawn(move || {
            server.accept_loop();
        });

        log::info!("[TermsurfSocket] Server started");
    }

    /// Accept loop - runs in background thread
    fn accept_loop(self: &Arc<Self>) {
        log::info!("[TermsurfSocket] Accept loop started");

        for stream in self.listener.incoming() {
            if !*self.running.lock().unwrap() {
                break;
            }

            match stream {
                Ok(stream) => {
                    let conn = Arc::new(TermsurfConnection::new(stream));
                    let conn_id = conn.id().to_string();

                    // Store connection
                    self.connections
                        .write()
                        .unwrap()
                        .insert(conn_id.clone(), conn.clone());

                    // Handle connection in a new thread
                    let server = self.clone();
                    thread::spawn(move || {
                        conn.read_loop(|conn, request| {
                            server.handle_request(conn, request);
                        });

                        // Remove connection when done
                        server.connections.write().unwrap().remove(&conn_id);
                    });
                }
                Err(e) => {
                    log::error!("[TermsurfSocket] Accept error: {}", e);
                }
            }
        }

        log::info!("[TermsurfSocket] Accept loop ended");
    }

    /// Handle a request from a client
    fn handle_request(&self, conn: &TermsurfConnection, request: TermsurfRequest) {
        let response = match request.action.as_str() {
            "ping" => TermsurfResponse::ok(request.id.clone(), Some(serde_json::json!({"pong": true}))),
            "open" => self.handle_open(conn, &request),
            "close" => self.handle_close(&request),
            _ => TermsurfResponse::error(
                request.id.clone(),
                format!("Unknown action: {}", request.action),
            ),
        };

        if let Err(e) = conn.send_response(&response) {
            log::error!("[TermsurfSocket] Failed to send response: {}", e);
        }
    }

    /// Handle "open" action - open a URL in a browser pane
    fn handle_open(&self, conn: &TermsurfConnection, request: &TermsurfRequest) -> TermsurfResponse {
        let url = match request.get_string("url") {
            Some(url) => url.to_string(),
            None => {
                return TermsurfResponse::error(request.id.clone(), "Missing 'url' in data".to_string())
            }
        };

        let pane_id = match request.pane_id {
            Some(id) => id as PaneId,
            None => {
                return TermsurfResponse::error(
                    request.id.clone(),
                    "Missing 'pane_id'".to_string(),
                )
            }
        };

        // Validate pane exists
        let mux = Mux::get();
        if mux.get_pane(pane_id).is_none() {
            return TermsurfResponse::error(
                request.id.clone(),
                format!("Pane {} not found", pane_id),
            );
        }

        // Subscribe connection to events for this pane
        conn.subscribe_to_pane(pane_id);

        // Notify GUI to open browser (using existing MuxNotification)
        mux.notify(mux::MuxNotification::WebOpen {
            pane_id,
            url: url.clone(),
        });

        TermsurfResponse::ok(
            request.id.clone(),
            Some(serde_json::json!({"message": format!("Opening {}", url)})),
        )
    }

    /// Handle "close" action - close browser in a pane
    fn handle_close(&self, request: &TermsurfRequest) -> TermsurfResponse {
        let pane_id = match request.pane_id {
            Some(id) => id as PaneId,
            None => {
                return TermsurfResponse::error(
                    request.id.clone(),
                    "Missing 'pane_id'".to_string(),
                )
            }
        };

        // Notify GUI to close browser
        let mux = Mux::get();
        mux.notify(mux::MuxNotification::WebClosed { pane_id });

        TermsurfResponse::ok(request.id.clone(), None)
    }

    /// Broadcast an event to all connections subscribed to a pane
    pub fn broadcast_event(&self, pane_id: PaneId, event: &TermsurfEvent) {
        let connections = self.connections.read().unwrap();
        for conn in connections.values() {
            if let Err(e) = conn.send_event(event, pane_id) {
                log::error!("[TermsurfSocket] Failed to send event: {}", e);
            }
        }
    }
}

impl Drop for TermsurfSocketServer {
    fn drop(&mut self) {
        *self.running.lock().unwrap() = false;

        // Remove socket file
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }

        log::info!("[TermsurfSocket] Server stopped");
    }
}
