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
    ModeChanged {
        browsing: bool,
    },
    UrlChanged {
        url: String,
    },
    LoadingState {
        state: String,
        _progress: u8,
    },
    TitleChanged {
        title: String,
    },
    TargetUrlChanged {
        url: String,
    },
    BrowserReady {
        tab_id: i64,
        browser_socket: String,
        browser: String,
    },
    JavaScriptDialogRequest {
        tab_id: i64,
        request_id: u64,
        dialog_type: String,
        origin_url: String,
        message: String,
        default_prompt_text: String,
    },
    ConsoleMessage {
        tab_id: i64,
        level: String,
        message: String,
        line_no: i32,
        source_id: String,
    },
    HttpAuthRequest {
        tab_id: i64,
        request_id: u64,
        url: String,
        auth_scheme: String,
        challenger: String,
        realm: String,
        is_proxy: bool,
        first_auth_attempt: bool,
    },
    RendererCrashed {
        tab_id: i64,
        termination_status: String,
        termination_status_code: i32,
        url: String,
        can_reload: bool,
    },
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
        // tab_id=0: GUI messages don't need tab filtering.
        std::thread::spawn(move || {
            reader_loop(reader, tx, reply_tx, 0);
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
    /// Returns (homepage, browsers) — Issue 712.
    pub fn send_hello(&self, pane_id: &str) -> Option<(String, Vec<String>)> {
        self.send(Msg::HelloRequest(proto::HelloRequest {
            pane_id: pane_id.into(),
        }));

        let reply = self.recv_reply()?;
        match reply.msg? {
            Msg::HelloReply(r) => Some((r.homepage, r.browsers)),
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
        browser: &str,
    ) -> Result<(i64, String, String), String> {
        self.send(Msg::QueryDevtoolsRequest(proto::QueryDevtoolsRequest {
            pane_id: pane_id.into(),
            inspected_tab_id,
            profile: profile.into(),
            browser: browser.into(),
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

    pub fn send_javascript_dialog_reply(
        &self,
        tab_id: i64,
        request_id: u64,
        accepted: bool,
        prompt_text: &str,
    ) {
        self.send(Msg::JavascriptDialogReply(proto::JavaScriptDialogReply {
            tab_id,
            request_id,
            accepted,
            prompt_text: prompt_text.into(),
        }));
    }

    pub fn send_http_auth_reply(
        &self,
        tab_id: i64,
        request_id: u64,
        accepted: bool,
        username: &str,
        password: &str,
    ) {
        self.send(Msg::HttpAuthReply(proto::HttpAuthReply {
            tab_id,
            request_id,
            accepted,
            username: username.into(),
            password: password.into(),
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
    tab_id: i64,
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
                dispatch_message(msg, &event_tx, &reply_tx, tab_id);
            }
            buf.drain(..4 + msg_len);
        }
    }
}

/// A direct connection to a browser engine process (Roamium) via Unix socket.
/// Enables the TUI to send Navigate/SetColorScheme directly to the browser,
/// bypassing the GUI for content messages.
pub struct BrowserConnection {
    stream: Mutex<UnixStream>,
    pub tab_id: i64,
}

impl BrowserConnection {
    /// Connect to a browser engine's listen socket and spawn a reader thread.
    pub fn connect(path: &str, tab_id: i64, tx: mpsc::Sender<super::LoopEvent>) -> Option<Self> {
        let stream = UnixStream::connect(path).ok()?;
        let reader = stream.try_clone().ok()?;

        // Dummy reply_tx — browser doesn't do request/reply with TUI.
        let (reply_tx, _reply_rx) = mpsc::channel();

        let id = tab_id;
        std::thread::spawn(move || {
            reader_loop(reader, tx, reply_tx, id);
        });

        Some(Self {
            stream: Mutex::new(stream),
            tab_id,
        })
    }

    /// Send a Navigate message directly to the browser.
    pub fn send_navigate(&self, url: &str) {
        self.send(Msg::Navigate(proto::Navigate {
            tab_id: self.tab_id,
            pane_id: String::new(),
            url: url.into(),
        }));
    }

    /// Send a SetColorScheme message directly to the browser.
    pub fn send_set_color_scheme(&self, scheme: &str) {
        let dark = scheme == "dark";
        self.send(Msg::SetColorScheme(proto::SetColorScheme {
            tab_id: self.tab_id,
            pane_id: String::new(),
            dark,
        }));
    }

    pub fn send_javascript_dialog_reply(&self, request_id: u64, accepted: bool, prompt_text: &str) {
        self.send(Msg::JavascriptDialogReply(proto::JavaScriptDialogReply {
            tab_id: self.tab_id,
            request_id,
            accepted,
            prompt_text: prompt_text.into(),
        }));
    }

    pub fn send_http_auth_reply(
        &self,
        request_id: u64,
        accepted: bool,
        username: &str,
        password: &str,
    ) {
        self.send(Msg::HttpAuthReply(proto::HttpAuthReply {
            tab_id: self.tab_id,
            request_id,
            accepted,
            username: username.into(),
            password: password.into(),
        }));
    }

    fn send(&self, msg: Msg) {
        let wrapper = TermSurfMessage { msg: Some(msg) };
        let payload = wrapper.encode_to_vec();
        let len = (payload.len() as u32).to_le_bytes();

        if let Ok(mut stream) = self.stream.lock() {
            let _ = stream.write_all(&len);
            let _ = stream.write_all(&payload);
        }
    }
}

fn dispatch_message(
    msg: TermSurfMessage,
    event_tx: &mpsc::Sender<super::LoopEvent>,
    reply_tx: &mpsc::Sender<TermSurfMessage>,
    tab_id: i64,
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
            if tab_id != 0 && m.tab_id != 0 && m.tab_id != tab_id {
                return;
            }
            let _ = event_tx.send(super::LoopEvent::Ipc(CompositorMessage::UrlChanged {
                url: m.url.clone(),
            }));
        }
        Some(Msg::LoadingState(m)) => {
            if tab_id != 0 && m.tab_id != 0 && m.tab_id != tab_id {
                return;
            }
            let _ = event_tx.send(super::LoopEvent::Ipc(CompositorMessage::LoadingState {
                state: m.state.clone(),
                _progress: m.progress as u8,
            }));
        }
        Some(Msg::TitleChanged(m)) => {
            if tab_id != 0 && m.tab_id != 0 && m.tab_id != tab_id {
                return;
            }
            let _ = event_tx.send(super::LoopEvent::Ipc(CompositorMessage::TitleChanged {
                title: m.title.clone(),
            }));
        }
        Some(Msg::TargetUrlChanged(m)) => {
            if tab_id != 0 && m.tab_id != 0 && m.tab_id != tab_id {
                return;
            }
            let _ = event_tx.send(super::LoopEvent::Ipc(CompositorMessage::TargetUrlChanged {
                url: m.url.clone(),
            }));
        }
        Some(Msg::BrowserReady(m)) => {
            let _ = event_tx.send(super::LoopEvent::Ipc(CompositorMessage::BrowserReady {
                tab_id: m.tab_id,
                browser_socket: m.browser_socket.clone(),
                browser: m.browser.clone(),
            }));
        }
        Some(Msg::JavascriptDialogRequest(m)) => {
            if tab_id != 0 && m.tab_id != 0 && m.tab_id != tab_id {
                return;
            }
            let _ = event_tx.send(super::LoopEvent::Ipc(
                CompositorMessage::JavaScriptDialogRequest {
                    tab_id: m.tab_id,
                    request_id: m.request_id,
                    dialog_type: m.dialog_type.clone(),
                    origin_url: m.origin_url.clone(),
                    message: m.message.clone(),
                    default_prompt_text: m.default_prompt_text.clone(),
                },
            ));
        }
        Some(Msg::ConsoleMessage(m)) => {
            if tab_id != 0 && m.tab_id != 0 && m.tab_id != tab_id {
                return;
            }
            let _ = event_tx.send(super::LoopEvent::Ipc(CompositorMessage::ConsoleMessage {
                tab_id: m.tab_id,
                level: m.level.clone(),
                message: m.message.clone(),
                line_no: m.line_no,
                source_id: m.source_id.clone(),
            }));
        }
        Some(Msg::HttpAuthRequest(m)) => {
            if tab_id != 0 && m.tab_id != 0 && m.tab_id != tab_id {
                return;
            }
            let _ = event_tx.send(super::LoopEvent::Ipc(CompositorMessage::HttpAuthRequest {
                tab_id: m.tab_id,
                request_id: m.request_id,
                url: m.url.clone(),
                auth_scheme: m.auth_scheme.clone(),
                challenger: m.challenger.clone(),
                realm: m.realm.clone(),
                is_proxy: m.is_proxy,
                first_auth_attempt: m.first_auth_attempt,
            }));
        }
        Some(Msg::RendererCrashed(m)) => {
            if tab_id != 0 && m.tab_id != 0 && m.tab_id != tab_id {
                return;
            }
            let _ = event_tx.send(super::LoopEvent::Ipc(CompositorMessage::RendererCrashed {
                tab_id: m.tab_id,
                termination_status: m.termination_status.clone(),
                termination_status_code: m.termination_status_code,
                url: m.url.clone(),
                can_reload: m.can_reload,
            }));
        }

        _ => {} // Ignore unexpected messages.
    }
}
