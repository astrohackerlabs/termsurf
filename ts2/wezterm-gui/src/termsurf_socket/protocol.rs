//! Protocol types for CLI-to-GUI communication over Unix domain sockets.
//! All messages are JSON-encoded with newline delimiters.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Request from CLI to GUI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermsurfRequest {
    /// Unique request ID for matching responses
    pub id: String,
    /// Action to perform: "open", "close", "ping"
    pub action: String,
    /// Target pane ID
    pub pane_id: Option<u64>,
    /// Action-specific data
    pub data: Option<Value>,
}

/// Response from GUI to CLI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermsurfResponse {
    /// Request ID this is responding to
    pub id: String,
    /// Status: "ok" or "error"
    pub status: String,
    /// Response data (action-specific)
    pub data: Option<Value>,
    /// Error message if status is "error"
    pub error: Option<String>,
}

impl TermsurfResponse {
    pub fn ok(id: String, data: Option<Value>) -> Self {
        Self {
            id,
            status: "ok".to_string(),
            data,
            error: None,
        }
    }

    pub fn error(id: String, message: String) -> Self {
        Self {
            id,
            status: "error".to_string(),
            data: None,
            error: Some(message),
        }
    }
}

/// Event from GUI to CLI (for streaming, e.g., console output)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermsurfEvent {
    /// Request ID this event relates to
    pub id: String,
    /// Event type: "console", "closed", etc.
    pub event: String,
    /// Event-specific data
    pub data: Option<Value>,
}

impl TermsurfEvent {
    pub fn new(id: String, event: String, data: Option<Value>) -> Self {
        Self { id, event, data }
    }

    pub fn console(id: String, level: &str, message: &str) -> Self {
        Self {
            id,
            event: "console".to_string(),
            data: Some(serde_json::json!({
                "level": level,
                "message": message,
            })),
        }
    }

    pub fn closed(id: String) -> Self {
        Self {
            id,
            event: "closed".to_string(),
            data: None,
        }
    }
}

/// Helper to get string from request data
impl TermsurfRequest {
    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.data.as_ref()?.get(key)?.as_str()
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.data.as_ref()?.get(key)?.as_bool()
    }
}
