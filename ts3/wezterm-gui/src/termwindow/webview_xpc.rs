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
    /// Stored connections (must keep alive)
    connections: Mutex<Vec<Arc<XpcConnection>>>,
    /// Stored listeners (must keep alive to accept connections)
    listeners: Mutex<Vec<XpcListener>>,
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
            connections: Mutex::new(Vec::new()),
            listeners: Mutex::new(Vec::new()),
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
            let manager_for_storage = Arc::clone(&manager);

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
                                };

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

            // Store connection to keep it alive
            manager_for_storage.connections.lock().unwrap().push(conn);
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
