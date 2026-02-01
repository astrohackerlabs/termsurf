//! XPC manager for cross-process IOSurface Mach port transfer.
//!
//! This module handles receiving IOSurface Mach ports from profile servers
//! via XPC. Unlike IOSurface IDs, Mach ports can be transferred cross-process.
//!
//! Architecture:
//! 1. GUI creates an XPC listener and registers a session with the launcher
//! 2. Profile server (or test sender) claims the session from the launcher
//! 3. Profile server sends IOSurface Mach port directly to GUI
//! 4. GUI imports the IOSurface and creates a texture for rendering

#[cfg(target_os = "macos")]
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::sync::{Arc, Mutex, OnceLock};

#[cfg(target_os = "macos")]
use mux::pane::PaneId;

#[cfg(target_os = "macos")]
use termsurf_xpc::*;

// ============================================================================
// Global XPC Manager
// ============================================================================

#[cfg(target_os = "macos")]
static XPC_MANAGER: OnceLock<Arc<XpcManager>> = OnceLock::new();

/// Get the global XPC manager instance
#[cfg(target_os = "macos")]
pub fn get_xpc_manager() -> Option<Arc<XpcManager>> {
    XPC_MANAGER.get().cloned()
}

/// Start the global XPC manager. Call this early in main().
#[cfg(target_os = "macos")]
pub fn start_xpc_manager() -> anyhow::Result<()> {
    let manager = XpcManager::new()?;
    let manager = Arc::new(manager);

    XPC_MANAGER
        .set(manager.clone())
        .map_err(|_| anyhow::anyhow!("XPC manager already started"))?;

    log::info!("[XPC Manager] Started");

    Ok(())
}

// ============================================================================
// Received Surface Info
// ============================================================================

/// Information about an IOSurface received via XPC Mach port transfer
#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
pub struct ReceivedSurface {
    /// The Mach port for the IOSurface (can be used with IOSurfaceLookupFromMachPort)
    pub mach_port: u32,
    /// Width of the surface
    pub width: u32,
    /// Height of the surface
    pub height: u32,
    /// Current URL (for control panel display)
    pub url: String,
}

// ============================================================================
// XPC Manager
// ============================================================================

/// Manages XPC communication for IOSurface sharing
#[cfg(target_os = "macos")]
pub struct XpcManager {
    /// Connection to the launcher XPC service
    _launcher: XpcConnection,
    /// Pending sessions waiting for surfaces
    /// Map from session_id to pane_id
    pending_sessions: Mutex<HashMap<String, PaneId>>,
    /// Received surfaces ready for rendering
    /// Map from pane_id to surface info
    received_surfaces: Mutex<HashMap<PaneId, ReceivedSurface>>,
    /// Peer connections from profile servers, keyed by pane_id.
    /// Used to send commands (resize, input) back to the browser.
    peer_connections: Mutex<HashMap<PaneId, Arc<XpcConnection>>>,
    /// Stored listeners (must keep alive to accept connections)
    listeners: Mutex<Vec<XpcListener>>,
    /// Callbacks to invalidate windows when new textures arrive.
    /// Registered by TermWindow during first render of each pane.
    /// Called from XPC event handler to trigger redraw after texture receipt.
    invalidate_callbacks: Mutex<HashMap<PaneId, Arc<dyn Fn() + Send + Sync>>>,
}

#[cfg(target_os = "macos")]
impl XpcManager {
    fn new() -> anyhow::Result<Self> {
        log::info!("[XPC Manager] Connecting to launcher...");

        // Connect to the launcher XPC service
        let launcher = XpcConnection::connect_mach_service("com.termsurf.launcher")
            .map_err(|e| anyhow::anyhow!("Failed to connect to launcher: {}", e))?;

        set_event_handler(&launcher, |event| {
            if let Err(e) = event {
                log::error!("[XPC Manager] Launcher connection error: {}", e);
            }
        });
        launcher.resume();

        log::info!("[XPC Manager] Connected to launcher");

        Ok(Self {
            _launcher: launcher,
            pending_sessions: Mutex::new(HashMap::new()),
            received_surfaces: Mutex::new(HashMap::new()),
            peer_connections: Mutex::new(HashMap::new()),
            listeners: Mutex::new(Vec::new()),
            invalidate_callbacks: Mutex::new(HashMap::new()),
        })
    }

    /// Request a profile to be spawned for a pane.
    /// Returns a session_id that will be used to correlate the incoming surface.
    pub fn request_profile_spawn(
        self: &Arc<Self>,
        pane_id: PaneId,
        url: &str,
        profile: &str,
        width: u32,
        height: u32,
        scale: f32,
    ) -> anyhow::Result<String> {
        let session_id = format!("pane-{}-{}", pane_id, std::process::id());

        log::info!(
            "[XPC Manager] Requesting profile spawn for pane {}, session={}, size={}x{}, scale={}",
            pane_id,
            session_id,
            width,
            height,
            scale
        );

        // Create a listener for this session
        let listener = XpcListener::new_anonymous()
            .map_err(|e| anyhow::anyhow!("Failed to create listener: {}", e))?;

        let endpoint = listener
            .get_endpoint()
            .map_err(|e| anyhow::anyhow!("Failed to get endpoint: {}", e))?;

        // Set up handler for incoming connections
        let session_id_clone = session_id.clone();
        let self_clone = Arc::clone(self);

        set_new_connection_handler(&listener, move |conn| {
            log::info!(
                "[XPC Manager] New connection for session {}",
                session_id_clone
            );

            let conn = Arc::new(conn);
            let session_id = session_id_clone.clone();
            let manager = Arc::clone(&self_clone);

            // Look up pane_id from session BEFORE setting event handler
            // This works because pending_sessions.insert() happens before spawn_profile
            let pane_id = manager.pending_sessions.lock().unwrap()
                .get(&session_id).copied();

            // Store connection by pane_id for sending commands back
            if let Some(pane_id) = pane_id {
                let mut peer_connections = manager.peer_connections.lock().unwrap();
                // Issue 319, experiment 2: Log when connection is replaced
                if let Some(old_conn) = peer_connections.get(&pane_id) {
                    log::warn!(
                        "[XPC-CONN] Replacing connection for pane {}: old={:p} new={:p}",
                        pane_id, Arc::as_ptr(old_conn), Arc::as_ptr(&conn)
                    );
                }
                peer_connections.insert(pane_id, Arc::clone(&conn));
                log::info!(
                    "[XPC-CONN] Stored connection for pane {}: {:p}",
                    pane_id, Arc::as_ptr(&conn)
                );
            }

            set_event_handler(&*conn, move |event| {
                match event {
                    Ok(msg) => {
                        let action = msg.get_string("action").unwrap_or_default();
                        log::info!("[XPC Manager] Received action: {}", action);

                        if action == "display_surface" {
                            // Get the Mach port
                            let port = msg.copy_mach_send("iosurface_port");
                            let width = msg.get_i64("width") as u32;
                            let height = msg.get_i64("height") as u32;
                            let width = if width == 0 { 100 } else { width };
                            let height = if height == 0 { 100 } else { height };
                            let url = msg.get_string("url").unwrap_or_default();

                            if port == 0 {
                                log::error!("[XPC Manager] Received null Mach port");
                                return;
                            }

                            log::info!(
                                "[XPC Manager] Received IOSurface: port={}, size={}x{}",
                                port,
                                width,
                                height
                            );

                            // Look up pane_id early for logging
                            let pane_id_for_log = {
                                let pending = manager.pending_sessions.lock().unwrap();
                                pending.get(&session_id).copied()
                            };
                            if let Some(pid) = pane_id_for_log {
                                log::info!(
                                    "[TEXTURE-SIZE] pane={} size={}x{} timestamp={:?}",
                                    pid,
                                    width,
                                    height,
                                    std::time::SystemTime::now()
                                );
                            }

                            // Look up pane_id from session
                            let pane_id = {
                                let pending = manager.pending_sessions.lock().unwrap();
                                pending.get(&session_id).copied()
                            };

                            if let Some(pane_id) = pane_id {
                                let surface = ReceivedSurface {
                                    mach_port: port,
                                    width,
                                    height,
                                    url: url.clone(),
                                };
                                log::info!(
                                    "[XPC Manager] Surface URL: '{}'",
                                    url
                                );

                                manager
                                    .received_surfaces
                                    .lock()
                                    .unwrap()
                                    .insert(pane_id, surface);

                                log::info!(
                                    "[XPC Manager] Stored surface for pane {} (session {})",
                                    pane_id,
                                    session_id
                                );

                                // Trigger window invalidate to display the new texture
                                if let Some(callback) = manager
                                    .invalidate_callbacks
                                    .lock()
                                    .unwrap()
                                    .get(&pane_id)
                                    .cloned()
                                {
                                    log::info!(
                                        "[XPC Manager] Calling invalidate callback for pane {}",
                                        pane_id
                                    );
                                    callback();
                                }
                            } else {
                                log::warn!(
                                    "[XPC Manager] No pane_id found for session {}",
                                    session_id
                                );
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("[XPC Manager] Connection error: {}", e);
                    }
                }
            });

            conn.resume();
        });

        listener.resume();

        // Store listener to keep it alive (CRITICAL: without this, the listener
        // would be dropped and the sender couldn't connect)
        self.listeners.lock().unwrap().push(listener);

        // Register session with pending map
        self.pending_sessions
            .lock()
            .unwrap()
            .insert(session_id.clone(), pane_id);

        // Send spawn request to launcher
        let msg = XpcDictionary::new();
        msg.set_string("action", "spawn_profile");
        msg.set_string("session_id", &session_id);
        msg.set_string("url", url);
        msg.set_string("profile", profile);
        msg.set_i64("width", width as i64);
        msg.set_i64("height", height as i64);
        msg.set_string("scale", &format!("{}", scale));
        msg.set_endpoint("gui_endpoint", endpoint);

        self._launcher.send(&msg);

        log::info!(
            "[XPC Manager] Sent spawn_profile request for session {}",
            session_id
        );

        Ok(session_id)
    }

    /// Check if a surface has been received for a pane
    pub fn get_received_surface(&self, pane_id: PaneId) -> Option<ReceivedSurface> {
        self.received_surfaces.lock().unwrap().get(&pane_id).cloned()
    }

    /// Remove a received surface (e.g., when webview is closed)
    pub fn remove_surface(&self, pane_id: PaneId) {
        self.received_surfaces.lock().unwrap().remove(&pane_id);
        // Also clean up pending sessions
        self.pending_sessions
            .lock()
            .unwrap()
            .retain(|_, &mut pid| pid != pane_id);
    }

    /// Send a command to the browser in the given pane
    pub fn send_command(&self, pane_id: PaneId, msg: &XpcDictionary) -> bool {
        let connections = self.peer_connections.lock().unwrap();
        if let Some(conn) = connections.get(&pane_id) {
            // Issue 319, experiment 2: Log connection pointer and action for debugging
            let action = msg.get_string("action").unwrap_or_default();
            log::info!(
                "[XPC-SEND] pane={} action={} conn={:p}",
                pane_id, action, Arc::as_ptr(conn)
            );
            conn.send(msg);
            true
        } else {
            log::warn!("[XPC] No connection for pane {}", pane_id);
            false
        }
    }

    /// Send a resize command to the browser in the given pane (legacy logical pixels)
    pub fn send_resize(&self, pane_id: PaneId, width: u32, height: u32) -> bool {
        let msg = XpcDictionary::new();
        msg.set_string("action", "resize_browser");
        msg.set_i64("width", width as i64);
        msg.set_i64("height", height as i64);

        if self.send_command(pane_id, &msg) {
            log::info!("[XPC] Sent resize to pane {}: {}x{}", pane_id, width, height);
            true
        } else {
            false
        }
    }

    /// Send a resize command using physical pixel dimensions.
    /// Profile server will convert to logical using ceil() to ensure texture >= viewport.
    pub fn send_resize_physical(&self, pane_id: PaneId, width: u32, height: u32, scale: f32) -> bool {
        let msg = XpcDictionary::new();
        msg.set_string("action", "resize_browser");
        msg.set_i64("physical_width", width as i64);
        msg.set_i64("physical_height", height as i64);
        msg.set_string("scale", &format!("{}", scale));

        if self.send_command(pane_id, &msg) {
            log::info!(
                "[XPC] Sent resize_physical to pane {}: {}x{} scale={}",
                pane_id, width, height, scale
            );
            true
        } else {
            false
        }
    }

    /// Send a key event to the browser in the given pane
    pub fn send_key_event(&self, pane_id: PaneId, key: &::window::KeyEvent) {
        use ::window::{KeyCode, Modifiers};

        let msg = XpcDictionary::new();
        msg.set_string("action", "key_event");
        msg.set_bool("key_is_down", key.key_is_down);

        // Include raw keycode if available (for accurate VK conversion)
        if let Some(raw) = &key.raw {
            msg.set_i64("raw_code", raw.raw_code as i64);
        }

        // Serialize the key for character extraction
        match &key.key {
            KeyCode::Char(c) => {
                msg.set_string("key_type", "char");
                msg.set_i64("char_code", *c as i64);
            }
            KeyCode::LeftArrow => msg.set_string("key_type", "left"),
            KeyCode::RightArrow => msg.set_string("key_type", "right"),
            KeyCode::UpArrow => msg.set_string("key_type", "up"),
            KeyCode::DownArrow => msg.set_string("key_type", "down"),
            KeyCode::Home => msg.set_string("key_type", "home"),
            KeyCode::End => msg.set_string("key_type", "end"),
            KeyCode::PageUp => msg.set_string("key_type", "pageup"),
            KeyCode::PageDown => msg.set_string("key_type", "pagedown"),
            KeyCode::Insert => msg.set_string("key_type", "insert"),
            KeyCode::Function(n) => {
                msg.set_string("key_type", "function");
                msg.set_i64("function_num", *n as i64);
            }
            _ => msg.set_string("key_type", "unknown"),
        }

        // Serialize modifiers
        let mods = key.modifiers;
        msg.set_bool("shift", mods.contains(Modifiers::SHIFT));
        msg.set_bool("ctrl", mods.contains(Modifiers::CTRL));
        msg.set_bool("alt", mods.contains(Modifiers::ALT));
        msg.set_bool("meta", mods.contains(Modifiers::SUPER));

        if self.send_command(pane_id, &msg) {
            log::debug!(
                "[XPC] Sent key_event to pane {}: {:?} down={}",
                pane_id,
                key.key,
                key.key_is_down
            );
        }
    }

    /// Send clipboard text to paste into the browser (experiment 6)
    pub fn send_paste_text(&self, pane_id: PaneId, text: &str) -> bool {
        let msg = XpcDictionary::new();
        msg.set_string("action", "paste_text");
        msg.set_string("text", text);

        if self.send_command(pane_id, &msg) {
            log::info!("[XPC] Sent paste_text to pane {} ({} chars)", pane_id, text.len());
            true
        } else {
            false
        }
    }

    /// Send copy command to the browser (issue 318, experiment 1)
    pub fn send_copy(&self, pane_id: PaneId) -> bool {
        let msg = XpcDictionary::new();
        msg.set_string("action", "do_copy");

        if self.send_command(pane_id, &msg) {
            log::info!("[XPC] Sent do_copy to pane {}", pane_id);
            true
        } else {
            false
        }
    }

    /// Send cut command to the browser (issue 318, experiment 2)
    pub fn send_cut(&self, pane_id: PaneId) -> bool {
        let msg = XpcDictionary::new();
        msg.set_string("action", "do_cut");

        if self.send_command(pane_id, &msg) {
            log::info!("[XPC] Sent do_cut to pane {}", pane_id);
            true
        } else {
            false
        }
    }

    /// Send select_all command to the browser (issue 318, experiment 3)
    pub fn send_select_all(&self, pane_id: PaneId) -> bool {
        let msg = XpcDictionary::new();
        msg.set_string("action", "do_select_all");

        if self.send_command(pane_id, &msg) {
            log::info!("[XPC] Sent do_select_all to pane {}", pane_id);
            true
        } else {
            false
        }
    }

    /// Send mouse move event to the browser (issue 319, experiment 1)
    pub fn send_mouse_move(&self, pane_id: PaneId, x: i32, y: i32, modifiers: u32) -> bool {
        let msg = XpcDictionary::new();
        msg.set_string("action", "mouse_move");
        msg.set_i64("x", x as i64);
        msg.set_i64("y", y as i64);
        msg.set_i64("modifiers", modifiers as i64);

        if self.send_command(pane_id, &msg) {
            log::trace!("[XPC] Sent mouse_move to pane {}: ({}, {})", pane_id, x, y);
            true
        } else {
            false
        }
    }

    /// Send mouse click event to the browser (issue 319, experiment 1)
    pub fn send_mouse_click(
        &self,
        pane_id: PaneId,
        x: i32,
        y: i32,
        button: u32,
        is_up: bool,
        click_count: i32,
        modifiers: u32,
    ) -> bool {
        let msg = XpcDictionary::new();
        msg.set_string("action", "mouse_click");
        msg.set_i64("x", x as i64);
        msg.set_i64("y", y as i64);
        msg.set_i64("button", button as i64);
        msg.set_bool("is_up", is_up);
        msg.set_i64("click_count", click_count as i64);
        msg.set_i64("modifiers", modifiers as i64);

        if self.send_command(pane_id, &msg) {
            log::debug!(
                "[XPC] Sent mouse_click to pane {}: ({}, {}) btn={} up={} count={}",
                pane_id, x, y, button, is_up, click_count
            );
            true
        } else {
            false
        }
    }

    /// Send mouse wheel event to the browser (issue 321, experiment 1)
    pub fn send_mouse_wheel(
        &self,
        pane_id: PaneId,
        x: i32,
        y: i32,
        delta_x: i32,
        delta_y: i32,
        modifiers: u32,
    ) -> bool {
        let msg = XpcDictionary::new();
        msg.set_string("action", "mouse_wheel");
        msg.set_i64("x", x as i64);
        msg.set_i64("y", y as i64);
        msg.set_i64("delta_x", delta_x as i64);
        msg.set_i64("delta_y", delta_y as i64);
        msg.set_i64("modifiers", modifiers as i64);

        if self.send_command(pane_id, &msg) {
            log::debug!(
                "[XPC] Sent mouse_wheel to pane {}: ({}, {}) delta=({}, {})",
                pane_id, x, y, delta_x, delta_y
            );
            true
        } else {
            false
        }
    }

    /// Remove a peer connection (e.g., when webview pane is closed)
    pub fn remove_connection(&self, pane_id: PaneId) {
        self.peer_connections.lock().unwrap().remove(&pane_id);
        log::info!("[XPC] Removed connection for pane {}", pane_id);
    }

    /// Register a callback to invalidate the window when a new texture arrives.
    /// Called by TermWindow during first render of each webview pane.
    /// The callback is only registered once per pane (no-op if already registered).
    pub fn register_invalidate_callback(
        &self,
        pane_id: PaneId,
        callback: Arc<dyn Fn() + Send + Sync>,
    ) {
        use std::collections::hash_map::Entry;
        let mut callbacks = self.invalidate_callbacks.lock().unwrap();
        if let Entry::Vacant(e) = callbacks.entry(pane_id) {
            e.insert(callback);
            log::info!("[XPC] Registered invalidate callback for pane {}", pane_id);
        }
    }

    /// Check if an invalidate callback is registered for a pane.
    pub fn has_invalidate_callback(&self, pane_id: PaneId) -> bool {
        self.invalidate_callbacks.lock().unwrap().contains_key(&pane_id)
    }

    /// Remove invalidate callback (e.g., when webview pane is closed)
    pub fn remove_invalidate_callback(&self, pane_id: PaneId) {
        self.invalidate_callbacks.lock().unwrap().remove(&pane_id);
        log::info!("[XPC] Removed invalidate callback for pane {}", pane_id);
    }
}

// ============================================================================
// Non-macOS stubs
// ============================================================================

#[cfg(not(target_os = "macos"))]
pub fn get_xpc_manager() -> Option<()> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn start_xpc_manager() -> anyhow::Result<()> {
    Ok(())
}
