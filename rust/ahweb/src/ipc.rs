//! Unix socket + protobuf client for communicating with the TermSurf compositor.
//!
//! Issue 26030312000700: Replaces xpc.rs. Same public API, pure Rust — no ObjC FFI.
//! Connects to the GUI's Unix domain socket at $TMPDIR/termsurf/gui.sock.
//! Wire format: 4-byte LE length prefix + serialized TermSurfMessage.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Mutex;

use prost::Message;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/termsurf.rs"));
}

use proto::term_surf_message::Msg;
use proto::TermSurfMessage;

/// Soft vs hard (ignore-cache) refresh for NavigationAction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshKind {
    Soft,
    IgnoreCache,
}

impl RefreshKind {
    pub fn action(self) -> &'static str {
        match self {
            RefreshKind::Soft => "refresh",
            RefreshKind::IgnoreCache => "refresh_ignore_cache",
        }
    }
}

/// Shared nonzero request-id allocator for all direct-engine refresh kinds.
fn next_refresh_request_id() -> u64 {
    static NEXT_REFRESH_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
    let mut request_id = NEXT_REFRESH_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    if request_id == 0 {
        request_id = NEXT_REFRESH_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    }
    request_id
}

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
        tab_id: i64,
        state: String,
        _progress: u8,
        navigation_request_id: u64,
    },
    NavigationState {
        tab_id: i64,
        can_go_back: bool,
        can_go_forward: bool,
        can_refresh: bool,
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

    /// Send a `set_devtools_overlay` message (Issue 26030112000684).
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

    /// Send a synchronous `hello` message to get live config (Issue 26022812000675).
    /// Returns (homepage, browsers) — Issue 26030612000712.
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

    /// Query the GUI for the last active browser pane/tab (Issue 26030112000684).
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

    /// Validate a DevTools request before launching the TUI (Issue 26030112000687).
    /// Returns (tab_id, browser, profile) on success (Issue 26030412000705 Exp 10).
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

    /// Query the GUI for the Chromium tab inventory (Issue 26030112000689).
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

    /// Request semantic Back through the compositor-owned pane route.
    pub fn send_back(&self, pane_id: &str) -> bool {
        if pane_id.is_empty() {
            return false;
        }
        self.send(Msg::NavigationAction(proto::NavigationAction {
            tab_id: 0,
            pane_id: pane_id.into(),
            action: "back".into(),
            request_id: 0,
        }));
        true
    }

    /// Request semantic Forward through the compositor-owned pane route.
    pub fn send_forward(&self, pane_id: &str) -> bool {
        if pane_id.is_empty() {
            return false;
        }
        self.send(Msg::NavigationAction(proto::NavigationAction {
            tab_id: 0,
            pane_id: pane_id.into(),
            action: "forward".into(),
            request_id: 0,
        }));
        true
    }

    /// Request semantic Refresh through the compositor-owned pane route.
    pub fn send_refresh(&self, pane_id: &str) -> bool {
        self.send_refresh_kind(pane_id, RefreshKind::Soft)
    }

    /// Request hard refresh (ignore cache) through the compositor-owned pane route.
    pub fn send_refresh_ignore_cache(&self, pane_id: &str) -> bool {
        self.send_refresh_kind(pane_id, RefreshKind::IgnoreCache)
    }

    fn send_refresh_kind(&self, pane_id: &str, kind: RefreshKind) -> bool {
        if pane_id.is_empty() {
            return false;
        }
        self.send(Msg::NavigationAction(proto::NavigationAction {
            tab_id: 0,
            pane_id: pane_id.into(),
            action: kind.action().into(),
            request_id: 0,
        }));
        true
    }

    /// Send a color scheme override (Issue 26022812000680).
    pub fn send_set_color_scheme(&self, pane_id: &str, scheme: &str) {
        let dark = scheme == "dark";
        self.send(Msg::SetColorScheme(proto::SetColorScheme {
            tab_id: 0,
            pane_id: pane_id.into(),
            dark,
        }));
    }

    /// Tell the compositor to open a split with a command (Issue 26030112000690).
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
            Ok(0) => {
                emit_disconnected_back_state(&event_tx, tab_id);
                return;
            }
            Ok(n) => n,
            Err(_) => {
                emit_disconnected_back_state(&event_tx, tab_id);
                return;
            }
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

fn emit_disconnected_back_state(event_tx: &mpsc::Sender<super::LoopEvent>, tab_id: i64) {
    if tab_id <= 0 {
        return;
    }
    let _ = event_tx.send(super::LoopEvent::Ipc(CompositorMessage::NavigationState {
        tab_id,
        can_go_back: false,
        can_go_forward: false,
        can_refresh: false,
    }));
}

/// A direct connection to a browser engine process (Chromium) via Unix socket.
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

    /// Request semantic Back directly from this connection's native tab.
    pub fn send_back(&self) -> bool {
        if self.tab_id <= 0 {
            return false;
        }
        self.send(Msg::NavigationAction(proto::NavigationAction {
            tab_id: self.tab_id,
            pane_id: String::new(),
            action: "back".into(),
            request_id: 0,
        }));
        true
    }

    /// Request semantic Forward directly from this connection's native tab.
    pub fn send_forward(&self) -> bool {
        if self.tab_id <= 0 {
            return false;
        }
        self.send(Msg::NavigationAction(proto::NavigationAction {
            tab_id: self.tab_id,
            pane_id: String::new(),
            action: "forward".into(),
            request_id: 0,
        }));
        true
    }

    /// Request semantic Refresh directly from this connection's native tab.
    pub fn send_refresh(&self) -> Option<u64> {
        self.send_refresh_kind(RefreshKind::Soft)
    }

    /// Request hard refresh (ignore cache) directly from this connection's native tab.
    pub fn send_refresh_ignore_cache(&self) -> Option<u64> {
        self.send_refresh_kind(RefreshKind::IgnoreCache)
    }

    fn send_refresh_kind(&self, kind: RefreshKind) -> Option<u64> {
        if self.tab_id <= 0 {
            return None;
        }
        let request_id = next_refresh_request_id();
        self.send(Msg::NavigationAction(proto::NavigationAction {
            tab_id: self.tab_id,
            pane_id: String::new(),
            action: kind.action().into(),
            request_id,
        }));
        Some(request_id)
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
                tab_id: m.tab_id,
                state: m.state.clone(),
                _progress: m.progress as u8,
                navigation_request_id: m.navigation_request_id,
            }));
        }
        Some(Msg::NavigationState(m)) => {
            if m.tab_id <= 0 || (tab_id != 0 && m.tab_id != tab_id) {
                return;
            }
            let _ = event_tx.send(super::LoopEvent::Ipc(CompositorMessage::NavigationState {
                tab_id: m.tab_id,
                can_go_back: m.can_go_back,
                can_go_forward: m.can_go_forward,
                can_refresh: m.can_refresh,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn decode_frame(stream: &mut UnixStream) -> TermSurfMessage {
        let mut len = [0u8; 4];
        stream.read_exact(&mut len).unwrap();
        let mut payload = vec![0u8; u32::from_le_bytes(len) as usize];
        stream.read_exact(&mut payload).unwrap();
        TermSurfMessage::decode(payload.as_slice()).unwrap()
    }

    fn compositor_connection(stream: UnixStream) -> CompositorConnection {
        let (_reply_tx, reply_rx) = mpsc::channel();
        CompositorConnection {
            stream: Mutex::new(stream),
            reply_rx: Mutex::new(reply_rx),
        }
    }

    #[test]
    fn compositor_back_is_pane_addressed() {
        let (client, mut peer) = UnixStream::pair().unwrap();
        let connection = compositor_connection(client);

        assert!(connection.send_back("pane-a"));
        let message = decode_frame(&mut peer);
        let Some(Msg::NavigationAction(action)) = message.msg else {
            panic!("expected NavigationAction");
        };
        assert_eq!(action.tab_id, 0);
        assert_eq!(action.pane_id, "pane-a");
        assert_eq!(action.action, "back");
        assert_eq!(action.request_id, 0);
        assert!(!connection.send_back(""));
    }

    #[test]
    fn compositor_forward_is_pane_addressed() {
        let (client, mut peer) = UnixStream::pair().unwrap();
        let connection = compositor_connection(client);

        assert!(connection.send_forward("pane-a"));
        let message = decode_frame(&mut peer);
        let Some(Msg::NavigationAction(action)) = message.msg else {
            panic!("expected NavigationAction");
        };
        assert_eq!(action.tab_id, 0);
        assert_eq!(action.pane_id, "pane-a");
        assert_eq!(action.action, "forward");
        assert_eq!(action.request_id, 0);
        assert!(!connection.send_forward(""));
    }

    #[test]
    fn direct_back_is_tab_addressed() {
        let (client, mut peer) = UnixStream::pair().unwrap();
        let connection = BrowserConnection {
            stream: Mutex::new(client),
            tab_id: 42,
        };

        assert!(connection.send_back());
        let message = decode_frame(&mut peer);
        let Some(Msg::NavigationAction(action)) = message.msg else {
            panic!("expected NavigationAction");
        };
        assert_eq!(action.tab_id, 42);
        assert!(action.pane_id.is_empty());
        assert_eq!(action.action, "back");
        assert_eq!(action.request_id, 0);

        let (invalid, _peer) = UnixStream::pair().unwrap();
        let invalid = BrowserConnection {
            stream: Mutex::new(invalid),
            tab_id: 0,
        };
        assert!(!invalid.send_back());
    }

    #[test]
    fn direct_forward_is_tab_addressed() {
        let (client, mut peer) = UnixStream::pair().unwrap();
        let connection = BrowserConnection {
            stream: Mutex::new(client),
            tab_id: 42,
        };

        assert!(connection.send_forward());
        let message = decode_frame(&mut peer);
        let Some(Msg::NavigationAction(action)) = message.msg else {
            panic!("expected NavigationAction");
        };
        assert_eq!(action.tab_id, 42);
        assert!(action.pane_id.is_empty());
        assert_eq!(action.action, "forward");
        assert_eq!(action.request_id, 0);
    }

    #[test]
    fn refresh_uses_zero_for_compositor_and_nonzero_for_direct_engine() {
        let (client, mut peer) = UnixStream::pair().unwrap();
        let compositor = compositor_connection(client);
        assert!(compositor.send_refresh("pane-a"));
        let message = decode_frame(&mut peer);
        let Some(Msg::NavigationAction(action)) = message.msg else {
            panic!("expected compositor Refresh");
        };
        assert_eq!(
            (
                action.tab_id,
                action.pane_id.as_str(),
                action.action.as_str()
            ),
            (0, "pane-a", "refresh")
        );
        assert_eq!(action.request_id, 0);

        let (client, mut peer) = UnixStream::pair().unwrap();
        let direct = BrowserConnection {
            stream: Mutex::new(client),
            tab_id: 42,
        };
        let request_id = direct.send_refresh().expect("direct Refresh id");
        assert_ne!(request_id, 0);
        let message = decode_frame(&mut peer);
        let Some(Msg::NavigationAction(action)) = message.msg else {
            panic!("expected direct Refresh");
        };
        assert_eq!(
            (
                action.tab_id,
                action.pane_id.as_str(),
                action.action.as_str()
            ),
            (42, "", "refresh")
        );
        assert_eq!(action.request_id, request_id);
    }

    #[test]
    fn hard_refresh_compositor_and_direct_actions() {
        let (client, mut peer) = UnixStream::pair().unwrap();
        let compositor = compositor_connection(client);
        assert!(compositor.send_refresh_ignore_cache("pane-hard"));
        let message = decode_frame(&mut peer);
        let Some(Msg::NavigationAction(action)) = message.msg else {
            panic!("expected compositor hard Refresh");
        };
        assert_eq!(
            (
                action.tab_id,
                action.pane_id.as_str(),
                action.action.as_str(),
                action.request_id
            ),
            (0, "pane-hard", "refresh_ignore_cache", 0)
        );

        let (client, mut peer) = UnixStream::pair().unwrap();
        let direct = BrowserConnection {
            stream: Mutex::new(client),
            tab_id: 7,
        };
        let id1 = direct.send_refresh().expect("soft id");
        let id2 = direct.send_refresh_ignore_cache().expect("hard id");
        assert_ne!(id1, 0);
        assert_ne!(id2, 0);
        assert_ne!(id1, id2);
        let m1 = decode_frame(&mut peer);
        let m2 = decode_frame(&mut peer);
        let Some(Msg::NavigationAction(a1)) = m1.msg else {
            panic!("soft");
        };
        let Some(Msg::NavigationAction(a2)) = m2.msg else {
            panic!("hard");
        };
        assert_eq!(a1.action, "refresh");
        assert_eq!(a2.action, "refresh_ignore_cache");
        assert_eq!(a1.request_id, id1);
        assert_eq!(a2.request_id, id2);
    }

    #[test]
    fn loading_state_preserves_tab_and_refresh_request_identity() {
        let (event_tx, event_rx) = mpsc::channel();
        let (reply_tx, _reply_rx) = mpsc::channel();
        dispatch_message(
            TermSurfMessage {
                msg: Some(Msg::LoadingState(proto::LoadingState {
                    tab_id: 42,
                    state: "loading".into(),
                    progress: 7,
                    navigation_request_id: 99,
                })),
            },
            &event_tx,
            &reply_tx,
            42,
        );
        let super::super::LoopEvent::Ipc(CompositorMessage::LoadingState {
            tab_id,
            state,
            _progress,
            navigation_request_id,
        }) = event_rx.recv_timeout(Duration::from_secs(1)).unwrap()
        else {
            panic!("expected LoadingState");
        };
        assert_eq!(
            (tab_id, state.as_str(), _progress, navigation_request_id),
            (42, "loading", 7, 99)
        );
    }

    #[test]
    fn navigation_state_is_authoritative_and_tab_filtered() {
        let (event_tx, event_rx) = mpsc::channel();
        let (reply_tx, _reply_rx) = mpsc::channel();

        dispatch_message(
            TermSurfMessage {
                msg: Some(Msg::NavigationState(proto::NavigationState {
                    tab_id: 42,
                    can_go_back: true,
                    can_go_forward: false,
                    can_refresh: true,
                })),
            },
            &event_tx,
            &reply_tx,
            42,
        );
        let super::super::LoopEvent::Ipc(CompositorMessage::NavigationState {
            tab_id,
            can_go_back,
            can_go_forward,
            can_refresh,
        }) = event_rx.recv_timeout(Duration::from_secs(1)).unwrap()
        else {
            panic!("expected NavigationState event");
        };
        assert_eq!(tab_id, 42);
        assert!(can_go_back);
        assert!(!can_go_forward);
        assert!(can_refresh);

        for state_tab in [0, 41] {
            dispatch_message(
                TermSurfMessage {
                    msg: Some(Msg::NavigationState(proto::NavigationState {
                        tab_id: state_tab,
                        can_go_back: false,
                        can_go_forward: true,
                        can_refresh: true,
                    })),
                },
                &event_tx,
                &reply_tx,
                42,
            );
        }
        assert!(event_rx.try_recv().is_err());
    }

    #[test]
    fn direct_reader_eof_emits_false_for_its_tab() {
        let (reader, writer) = UnixStream::pair().unwrap();
        let (event_tx, event_rx) = mpsc::channel();
        let (reply_tx, _reply_rx) = mpsc::channel();
        let thread = std::thread::spawn(move || reader_loop(reader, event_tx, reply_tx, 77));

        drop(writer);
        let super::super::LoopEvent::Ipc(CompositorMessage::NavigationState {
            tab_id,
            can_go_back,
            can_go_forward,
            can_refresh,
        }) = event_rx.recv_timeout(Duration::from_secs(1)).unwrap()
        else {
            panic!("expected EOF-derived NavigationState event");
        };
        assert_eq!(tab_id, 77);
        assert!(!can_go_back);
        assert!(!can_go_forward);
        assert!(!can_refresh);
        thread.join().unwrap();
    }
}
