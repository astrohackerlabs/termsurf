//! Web command - opens URLs in browser panes using Unix socket communication.

use anyhow::{anyhow, Context};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

#[derive(Debug, Parser, Clone)]
pub struct WebCommand {
    #[command(subcommand)]
    pub sub: WebSubCommand,
}

#[derive(Debug, Subcommand, Clone)]
pub enum WebSubCommand {
    /// Open a URL in a browser pane
    #[command(name = "open")]
    Open(WebOpen),
}

#[derive(Debug, Parser, Clone)]
pub struct WebOpen {
    /// The URL to open
    url: String,
}

// Protocol types (matching GUI's termsurf_socket/protocol.rs)

#[derive(Debug, Serialize)]
struct TermsurfRequest {
    id: String,
    action: String,
    pane_id: Option<u64>,
    data: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct TermsurfResponse {
    #[allow(dead_code)]
    id: String,
    status: String,
    data: Option<Value>,
    error: Option<String>,
}

impl WebCommand {
    /// Run the web command using Unix socket (not RPC)
    pub fn run(&self) -> anyhow::Result<()> {
        match &self.sub {
            WebSubCommand::Open(cmd) => cmd.run(),
        }
    }
}

impl WebOpen {
    pub fn run(&self) -> anyhow::Result<()> {
        // Get socket path from environment
        let socket_path = std::env::var("TERMSURF_SOCKET").map_err(|_| {
            anyhow!(
                "TERMSURF_SOCKET not set. Are you running inside TermSurf?\n\
                 The 'web' command must be run from within a TermSurf terminal."
            )
        })?;

        // Get pane ID from environment
        let pane_id: u64 = std::env::var("WEZTERM_PANE")
            .map_err(|_| anyhow!("WEZTERM_PANE not set"))?
            .parse()
            .map_err(|_| anyhow!("WEZTERM_PANE is not a valid number"))?;

        // Connect to socket
        let mut stream = UnixStream::connect(&socket_path)
            .with_context(|| format!("Failed to connect to socket at {}", socket_path))?;

        // Generate a simple request ID
        let request_id = format!("{}", std::process::id());

        // Build request
        let request = TermsurfRequest {
            id: request_id,
            action: "open".to_string(),
            pane_id: Some(pane_id),
            data: Some(serde_json::json!({
                "url": self.url,
            })),
        };

        // Send request (newline-delimited JSON)
        let request_json = serde_json::to_string(&request)?;
        writeln!(stream, "{}", request_json)?;
        stream.flush()?;

        // Read response
        let mut reader = BufReader::new(stream);
        let mut response_line = String::new();
        reader.read_line(&mut response_line)?;

        let response: TermsurfResponse = serde_json::from_str(&response_line)
            .with_context(|| format!("Failed to parse response: {}", response_line))?;

        // Handle response
        if response.status == "ok" {
            if let Some(data) = response.data {
                if let Some(message) = data.get("message").and_then(|m| m.as_str()) {
                    println!("{}", message);
                }
            }
            Ok(())
        } else {
            Err(anyhow!(
                "{}",
                response.error.unwrap_or_else(|| "Unknown error".to_string())
            ))
        }
    }
}
