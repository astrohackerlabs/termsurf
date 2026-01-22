//! Handles a single client connection to the Unix domain socket.

use super::protocol::{TermsurfEvent, TermsurfRequest, TermsurfResponse};
use mux::pane::PaneId;
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};

/// Per-connection state
pub struct TermsurfConnection {
    id: String,
    stream: Arc<Mutex<UnixStream>>,
    /// Pane IDs this connection is subscribed to for events
    subscribed_panes: Arc<Mutex<HashSet<PaneId>>>,
}

impl TermsurfConnection {
    pub fn new(stream: UnixStream) -> Self {
        // Generate a simple unique ID using timestamp + random bits
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let id = format!("{:x}", timestamp % 0xFFFFFFFF);
        log::info!("[TermsurfSocket] New connection: {}", id);
        Self {
            id,
            stream: Arc::new(Mutex::new(stream)),
            subscribed_panes: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Subscribe this connection to events for a pane
    pub fn subscribe_to_pane(&self, pane_id: PaneId) {
        self.subscribed_panes.lock().unwrap().insert(pane_id);
    }

    /// Check if this connection is subscribed to a pane
    pub fn is_subscribed_to(&self, pane_id: PaneId) -> bool {
        self.subscribed_panes.lock().unwrap().contains(&pane_id)
    }

    /// Send an event to this connection (if subscribed)
    pub fn send_event(&self, event: &TermsurfEvent, pane_id: PaneId) -> std::io::Result<()> {
        if !self.is_subscribed_to(pane_id) {
            return Ok(());
        }
        self.send_message(event)
    }

    /// Send a response to this connection
    pub fn send_response(&self, response: &TermsurfResponse) -> std::io::Result<()> {
        self.send_message(response)
    }

    /// Send any serializable message
    fn send_message<T: serde::Serialize>(&self, message: &T) -> std::io::Result<()> {
        let json = serde_json::to_string(message)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut stream = self.stream.lock().unwrap();
        writeln!(stream, "{}", json)?;
        stream.flush()
    }

    /// Read and process messages from this connection.
    /// Returns when the connection is closed.
    pub fn read_loop<F>(&self, mut handler: F)
    where
        F: FnMut(&TermsurfConnection, TermsurfRequest),
    {
        let stream = match self.stream.lock().unwrap().try_clone() {
            Ok(s) => s,
            Err(e) => {
                log::error!("[TermsurfSocket] Failed to clone stream: {}", e);
                return;
            }
        };

        let reader = BufReader::new(stream);
        for line in reader.lines() {
            match line {
                Ok(line) if line.is_empty() => continue,
                Ok(line) => {
                    match serde_json::from_str::<TermsurfRequest>(&line) {
                        Ok(request) => {
                            log::debug!(
                                "[TermsurfSocket] Connection {} received: action={} id={}",
                                self.id,
                                request.action,
                                request.id
                            );
                            handler(self, request);
                        }
                        Err(e) => {
                            log::error!("[TermsurfSocket] Failed to parse request: {}", e);
                            let _ = self.send_response(&TermsurfResponse::error(
                                "unknown".to_string(),
                                format!("Invalid JSON: {}", e),
                            ));
                        }
                    }
                }
                Err(e) => {
                    log::info!("[TermsurfSocket] Connection {} closed: {}", self.id, e);
                    break;
                }
            }
        }
        log::info!("[TermsurfSocket] Connection {} read loop ended", self.id);
    }
}

impl Drop for TermsurfConnection {
    fn drop(&mut self) {
        log::info!("[TermsurfSocket] Connection {} dropped", self.id);
    }
}
