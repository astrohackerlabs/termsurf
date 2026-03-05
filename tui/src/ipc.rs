//! Unix socket + protobuf client for communicating with the TermSurf compositor.
//!
//! Issue 700: Replaces xpc.rs. Same public API, pure Rust — no ObjC FFI.
//! Connects to the GUI's Unix domain socket at $TMPDIR/termsurf/gui.sock.
//! Wire format: 4-byte LE length prefix + serialized TermSurfMessage.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::sync::Mutex;

use prost::Message;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/termsurf.rs"));
}

use proto::term_surf_message::Msg;
use proto::TermSurfMessage;

// --- Public API ---

/// Messages received from the compositor.
pub enum CompositorMessage {
    ModeChanged { browsing: bool },
    UrlChanged { url: String },
    LoadingState { state: String, _progress: u8 },
    TitleChanged { title: String },
}

/// A direct connection to the TermSurf app via Unix domain socket.
pub struct CompositorConnection {
    stream: Mutex<UnixStream>,
    reply_rx: Mutex<mpsc::Receiver<TermSurfMessage>>,
}

impl CompositorConnection {
    /// Connect to the TermSurf app via Unix domain socket.
    pub fn connect(tx: mpsc::Sender<super::LoopEvent>) -> Option<Self> {
        let sock_path = match std::env::var("TERMSURF_SOCKET") {
            Ok(p) => p,
            Err(_) => {
                eprintln!("TERMSURF_SOCKET not set — is TermSurf running?");
                return None;
            }
        };

        let stream = UnixStream::connect(&sock_path).ok()?;
        let reader = stream.try_clone().ok()?;

        let (reply_tx, reply_rx) = mpsc::channel();

        // Reader thread: reads length-prefixed protobuf messages.
        std::thread::spawn(move || {
            reader_loop(reader, tx, reply_tx);
        });

        Some(Self {
            stream: Mutex::new(stream),
            reply_rx: Mutex::new(reply_rx),
        })
    }

    /// Send a `set_overlay` message.
    pub fn send_set_overlay(
        &self,
        pane_id: &str,
        col: u16,
        row: u16,
        width: u16,
        height: u16,
        url: &str,
        profile: &str,
        browsing: bool,
        browser: &str,
    ) {
        self.send(Msg::SetOverlay(proto::SetOverlay {
            pane_id: pane_id.into(),
            col: col as u64,
            row: row as u64,
            width: width as u64,
            height: height as u64,
            url: url.into(),
            profile: profile.into(),
            browsing,
            browser: browser.into(),
        }));
    }

    /// Send a `set_devtools_overlay` message (Issue 684).
    pub fn send_set_devtools_overlay(
        &self,
        pane_id: &str,
        col: u16,
        row: u16,
        width: u16,
        height: u16,
        inspected_tab_id: i64,
        profile: &str,
        browsing: bool,
        browser: &str,
    ) {
        self.send(Msg::SetDevtoolsOverlay(proto::SetDevtoolsOverlay {
            pane_id: pane_id.into(),
            col: col as u64,
            row: row as u64,
            width: width as u64,
            height: height as u64,
            profile: profile.into(),
            browsing,
            inspected_tab_id,
            browser: browser.into(),
        }));
    }

    /// Send a synchronous `hello` message to get live config (Issue 675).
    pub fn send_hello(&self, pane_id: &str) -> Option<String> {
        self.send(Msg::HelloRequest(proto::HelloRequest {
            pane_id: pane_id.into(),
        }));

        let reply = self.recv_reply()?;
        match reply.msg? {
            Msg::HelloReply(r) => {
                if r.homepage.is_empty() {
                    None
                } else {
                    Some(r.homepage)
                }
            }
            _ => None,
        }
    }

    /// Query the GUI for the last active browser pane/tab (Issue 684).
    pub fn send_query_last(&self, pane_id: &str, profile: &str) -> Option<(String, String, i64)> {
        self.send(Msg::QueryLastRequest(proto::QueryLastRequest {
            pane_id: pane_id.into(),
            profile: profile.into(),
        }));

        let reply = self.recv_reply()?;
        match reply.msg? {
            Msg::QueryLastReply(r) => {
                if !r.error.is_empty() {
                    return None;
                }
                Some((r.profile, r.pane_id, r.tab_id))
            }
            _ => None,
        }
    }

    /// Validate a DevTools request before launching the TUI (Issue 687).
    /// Returns (tab_id, browser, profile) on success (Issue 705 Exp 10).
    pub fn send_query_devtools(
        &self,
        pane_id: &str,
        inspected_tab_id: i64,
        profile: &str,
    ) -> Result<(i64, String, String), String> {
        self.send(Msg::QueryDevtoolsRequest(proto::QueryDevtoolsRequest {
            pane_id: pane_id.into(),
            inspected_tab_id,
            profile: profile.into(),
        }));

        let reply = self
            .recv_reply()
            .ok_or_else(|| "No reply from compositor".to_string())?;
        match reply.msg {
            Some(Msg::QueryDevtoolsReply(r)) => {
                if !r.error.is_empty() {
                    Err(r.error)
                } else {
                    Ok((r.tab_id, r.browser, r.profile))
                }
            }
            _ => Err("Unexpected reply type".to_string()),
        }
    }

    /// Query the GUI for the Chromium tab inventory (Issue 689).
    pub fn send_query_tabs(&self, pane_id: &str, profile: &str) -> Result<String, String> {
        self.send(Msg::QueryTabsRequest(proto::QueryTabsRequest {
            pane_id: pane_id.into(),
            profile: profile.into(),
        }));

        let reply = self
            .recv_reply()
            .ok_or_else(|| "No reply from compositor".to_string())?;
        match reply.msg {
            Some(Msg::QueryTabsReply(r)) => {
                if !r.error.is_empty() {
                    return Err(r.error);
                }

                let mut out = format!("Chromium tabs (profile: {}):\n", profile);
                for tab in &r.tabs {
                    let kind = if tab.inspected_tab_id != 0 {
                        "devtools"
                    } else {
                        "browser"
                    };
                    out.push_str(&format!(
                        "  [{}] tab={} pane={} {}\n",
                        kind, tab.id, tab.pane_id, tab.url
                    ));
                }
                out.push_str("  ---\n");
                out.push_str(&format!(
                    "  browser: {}  devtools: {}  total: {}\n",
                    r.chromium_browser, r.chromium_devtools, r.chromium_tabs
                ));
                out.push_str(&format!("\nGUI panes: {}", r.gui_panes));
                Ok(out)
            }
            _ => Err("Unexpected reply type".to_string()),
        }
    }

    /// Tell the compositor to navigate to a new URL.
    pub fn send_navigate(&self, pane_id: &str, url: &str) {
        self.send(Msg::Navigate(proto::Navigate {
            tab_id: 0,
            pane_id: pane_id.into(),
            url: url.into(),
        }));
    }

    /// Send a color scheme override (Issue 680).
    pub fn send_set_color_scheme(&self, pane_id: &str, scheme: &str) {
        let dark = scheme == "dark";
        self.send(Msg::SetColorScheme(proto::SetColorScheme {
            tab_id: 0,
            pane_id: pane_id.into(),
            dark,
        }));
    }

    /// Tell the compositor to open a split with a command (Issue 690).
    pub fn send_open_split(&self, pane_id: &str, direction: &str, command: &str) {
        self.send(Msg::OpenSplit(proto::OpenSplit {
            pane_id: pane_id.into(),
            direction: direction.into(),
            command: command.into(),
        }));
    }

    /// Notify the compositor of a mode change.
    pub fn send_mode_changed(&self, pane_id: &str, browsing: bool) {
        self.send(Msg::ModeChanged(proto::ModeChanged {
            browsing,
            pane_id: pane_id.into(),
        }));
    }

    // --- Internals ---

    fn send(&self, msg: Msg) {
        let wrapper = TermSurfMessage { msg: Some(msg) };
        let payload = wrapper.encode_to_vec();
        let len = (payload.len() as u32).to_le_bytes();

        if let Ok(mut stream) = self.stream.lock() {
            let _ = stream.write_all(&len);
            let _ = stream.write_all(&payload);
        }
    }

    fn recv_reply(&self) -> Option<TermSurfMessage> {
        self.reply_rx
            .lock()
            .ok()?
            .recv_timeout(std::time::Duration::from_secs(5))
            .ok()
    }
}

// --- Reader thread ---

fn reader_loop(
    mut stream: UnixStream,
    event_tx: mpsc::Sender<super::LoopEvent>,
    reply_tx: mpsc::Sender<TermSurfMessage>,
) {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];

    loop {
        let n = match stream.read(&mut tmp) {
            Ok(0) => return, // EOF
            Ok(n) => n,
            Err(_) => return,
        };
        buf.extend_from_slice(&tmp[..n]);

        // Extract complete messages.
        while buf.len() >= 4 {
            let msg_len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
            if buf.len() < 4 + msg_len {
                break;
            }

            let payload = &buf[4..4 + msg_len];
            if let Ok(msg) = TermSurfMessage::decode(payload) {
                dispatch_message(msg, &event_tx, &reply_tx);
            }
            buf.drain(..4 + msg_len);
        }
    }
}

fn dispatch_message(
    msg: TermSurfMessage,
    event_tx: &mpsc::Sender<super::LoopEvent>,
    reply_tx: &mpsc::Sender<TermSurfMessage>,
) {
    match &msg.msg {
        // Reply messages → reply channel (sync queries block on this).
        Some(
            Msg::HelloReply(_)
            | Msg::QueryLastReply(_)
            | Msg::QueryDevtoolsReply(_)
            | Msg::QueryTabsReply(_),
        ) => {
            let _ = reply_tx.send(msg);
        }

        // Event messages → LoopEvent channel.
        Some(Msg::ModeChanged(m)) => {
            let _ = event_tx.send(super::LoopEvent::Ipc(CompositorMessage::ModeChanged {
                browsing: m.browsing,
            }));
        }
        Some(Msg::UrlChanged(m)) => {
            let _ = event_tx.send(super::LoopEvent::Ipc(CompositorMessage::UrlChanged {
                url: m.url.clone(),
            }));
        }
        Some(Msg::LoadingState(m)) => {
            let _ = event_tx.send(super::LoopEvent::Ipc(CompositorMessage::LoadingState {
                state: m.state.clone(),
                _progress: m.progress as u8,
            }));
        }
        Some(Msg::TitleChanged(m)) => {
            let _ = event_tx.send(super::LoopEvent::Ipc(CompositorMessage::TitleChanged {
                title: m.title.clone(),
            }));
        }

        _ => {} // Ignore unexpected messages.
    }
}
