use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::{mpsc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::ffi::{
    AbiRuntime, AbiView, ConsoleMessage as AbiConsoleMessage,
    JavaScriptDialogRequest as AbiJavaScriptDialogRequest, RendererCrash as AbiRendererCrash,
};
use crate::proto::{termsurf, Msg, TermSurfMessage};

const DATA_URL: &str = "data:text/html,%3Ctitle%3ELadybird%20Engine%3C/title%3E%3Ca%20href=%22https://example.com/ladybird-engine%22%20style=%22position:fixed;left:0;top:0;width:80px;height:80px;display:block%22%3Eengine%3C/a%3E%3Cscript%3Econsole.log(%22Ladybird%20console%20engine%22)%3C/script%3E";
const DIALOG_DATA_URL: &str = "data:text/html,%3Ctitle%3ELadybird%20Dialog%3C/title%3E%3Cscript%3Econst%20r%20%3D%20prompt%28%22ladybird-dialog-message%22%2C%20%22ladybird-dialog-default%22%29%3B%20console.log%28%22ladybird-dialog-result%3A%22%20%2B%20r%29%3B%3C/script%3E";
const RENDER_SURFACE_RETRY_LIMIT: u64 = 2400;

static ENGINE: OnceLock<EngineService> = OnceLock::new();

pub struct EngineService {
    tx: mpsc::Sender<Command>,
    join: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Clone, Debug)]
pub struct TabSnapshot {
    pub id: i64,
    pub inspected_tab_id: i64,
    pub pane_id: String,
    pub url: String,
    pub width: i32,
    pub height: i32,
    pub focused: bool,
    pub dark: bool,
    pub gui_active: bool,
    pub last_title: String,
    pub finished: bool,
    pub crashed: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub can_refresh: bool,
    pub mouse_button_events: u64,
    pub mouse_move_events: u64,
    pub scroll_events: u64,
    pub key_events: u64,
    pub console_message_count: u64,
    pub last_console_message: String,
    pub cursor_change_count: u64,
    pub last_cursor_type: i32,
    pub target_url_change_count: u64,
    pub last_target_url: String,
    pub last_non_empty_target_url: String,
    pub javascript_dialog_request_count: u64,
    pub javascript_dialog_reply_count: u64,
    pub last_javascript_dialog_request_id: u64,
    pub last_javascript_dialog_reply_request_id: u64,
    pub last_javascript_dialog_type: String,
    pub last_javascript_dialog_message: String,
    pub last_javascript_dialog_default_prompt_text: String,
    pub last_javascript_dialog_origin_url: String,
    pub last_javascript_dialog_reply_accepted: bool,
    pub last_javascript_dialog_reply_ok: bool,
}

#[derive(Clone, Debug)]
pub struct CloseOutcome {
    pub removed: bool,
    pub remaining_browser_tabs: usize,
}

#[derive(Clone, Debug)]
pub struct StateOutcome {
    pub affected_count: usize,
    pub snapshots: Vec<TabSnapshot>,
}

enum Command {
    CreateTab {
        url: String,
        pane_id: String,
        width: i32,
        height: i32,
        dark: bool,
        reply: mpsc::Sender<Result<TabSnapshot, String>>,
    },
    Navigate {
        tab_id: i64,
        url: String,
        reply: mpsc::Sender<Result<TabSnapshot, String>>,
    },
    NavigationAction {
        tab_id: i64,
        action: String,
        request_id: u64,
        reply: mpsc::Sender<Result<TabSnapshot, String>>,
    },
    Resize {
        tab_id: i64,
        width: i32,
        height: i32,
        reply: mpsc::Sender<Result<TabSnapshot, String>>,
    },
    MouseEvent {
        tab_id: i64,
        event_type: String,
        button: String,
        x: f64,
        y: f64,
        click_count: i32,
        modifiers: u64,
        reply: mpsc::Sender<Result<TabSnapshot, String>>,
    },
    MouseMove {
        tab_id: i64,
        x: f64,
        y: f64,
        modifiers: u64,
        reply: mpsc::Sender<Result<TabSnapshot, String>>,
    },
    ScrollEvent {
        tab_id: i64,
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
        phase: u64,
        momentum_phase: u64,
        precise: bool,
        modifiers: u64,
        reply: mpsc::Sender<Result<TabSnapshot, String>>,
    },
    KeyEvent {
        tab_id: i64,
        event_type: String,
        windows_key_code: i32,
        utf8: String,
        modifiers: u64,
        reply: mpsc::Sender<Result<TabSnapshot, String>>,
    },
    JavaScriptDialogReply {
        tab_id: i64,
        request_id: u64,
        accepted: bool,
        prompt_text: String,
        reply: mpsc::Sender<Result<TabSnapshot, String>>,
    },
    SetFocus {
        tab_id: i64,
        focused: bool,
        reply: mpsc::Sender<Result<TabSnapshot, String>>,
    },
    SetColorScheme {
        tab_id: i64,
        dark: bool,
        reply: mpsc::Sender<Result<TabSnapshot, String>>,
    },
    SetGuiActive {
        tab_id: i64,
        active: bool,
        reply: mpsc::Sender<Result<StateOutcome, String>>,
    },
    CloseTab {
        tab_id: i64,
        reply: mpsc::Sender<Result<CloseOutcome, String>>,
    },
    Snapshot {
        reply: mpsc::Sender<Vec<TabSnapshot>>,
    },
    Shutdown,
}

#[derive(Debug)]
enum Event {
    RuntimeCreated { owner_thread: String },
    ViewCreated { tab_id: i64 },
    LoadFinished { tab_id: i64, url: String },
    Failed { message: String },
    ShutdownComplete,
}

struct OwnedTab {
    id: i64,
    inspected_tab_id: i64,
    pane_id: String,
    target_url: String,
    width: i32,
    height: i32,
    focused: bool,
    dark: bool,
    gui_active: bool,
    last_title: String,
    view: AbiView,
    in_flight: bool,
    last_url: String,
    finished: bool,
    crashed: bool,
    can_go_back: bool,
    can_go_forward: bool,
    can_refresh: bool,
    refresh_request_id: u64,
    render_surface_sent: bool,
    render_surface_attempts: u64,
    mouse_button_events: u64,
    mouse_move_events: u64,
    scroll_events: u64,
    key_events: u64,
    console_message_count: u64,
    last_console_message: String,
    cursor_change_count: u64,
    last_cursor_type: i32,
    target_url_change_count: u64,
    last_target_url: String,
    last_non_empty_target_url: String,
    javascript_dialog_request_count: u64,
    javascript_dialog_reply_count: u64,
    last_javascript_dialog_request_id: u64,
    last_javascript_dialog_reply_request_id: u64,
    last_javascript_dialog_type: String,
    last_javascript_dialog_message: String,
    last_javascript_dialog_default_prompt_text: String,
    last_javascript_dialog_origin_url: String,
    last_javascript_dialog_reply_accepted: bool,
    last_javascript_dialog_reply_ok: bool,
}

impl EngineService {
    pub fn start() -> Result<Self, String> {
        let (command_tx, command_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let join = thread::spawn(move || owner_thread(command_rx, ready_tx));
        match ready_rx.recv_timeout(Duration::from_secs(30)) {
            Ok(Event::RuntimeCreated { owner_thread }) => {
                eprintln!("[Ladybird] engine owner started thread={owner_thread}");
                Ok(Self {
                    tx: command_tx,
                    join: Mutex::new(Some(join)),
                })
            }
            Ok(Event::Failed { message }) => Err(message),
            Ok(other) => Err(format!("unexpected engine startup event: {other:?}")),
            Err(error) => Err(format!("engine startup timed out: {error}")),
        }
    }

    pub fn create_tab(
        &self,
        url: String,
        pane_id: String,
        width: i32,
        height: i32,
        dark: bool,
    ) -> Result<TabSnapshot, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(Command::CreateTab {
                url,
                pane_id,
                width,
                height,
                dark,
                reply,
            })
            .map_err(|error| format!("engine create-tab send failed: {error}"))?;
        recv_reply(rx, "create-tab")
    }

    pub fn navigate(&self, tab_id: i64, url: String) -> Result<TabSnapshot, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(Command::Navigate { tab_id, url, reply })
            .map_err(|error| format!("engine navigate send failed: {error}"))?;
        recv_reply(rx, "navigate")
    }

    pub fn navigation_action(
        &self,
        tab_id: i64,
        action: String,
        request_id: u64,
    ) -> Result<TabSnapshot, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(Command::NavigationAction {
                tab_id,
                action,
                request_id,
                reply,
            })
            .map_err(|error| format!("engine navigation-action send failed: {error}"))?;
        recv_reply(rx, "navigation-action")
    }

    pub fn resize_tab(&self, tab_id: i64, width: i32, height: i32) -> Result<TabSnapshot, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(Command::Resize {
                tab_id,
                width,
                height,
                reply,
            })
            .map_err(|error| format!("engine resize send failed: {error}"))?;
        recv_reply(rx, "resize")
    }

    pub fn mouse_event(
        &self,
        tab_id: i64,
        event_type: String,
        button: String,
        x: f64,
        y: f64,
        click_count: i32,
        modifiers: u64,
    ) -> Result<TabSnapshot, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(Command::MouseEvent {
                tab_id,
                event_type,
                button,
                x,
                y,
                click_count,
                modifiers,
                reply,
            })
            .map_err(|error| format!("engine mouse-event send failed: {error}"))?;
        recv_reply(rx, "mouse-event")
    }

    pub fn mouse_move(
        &self,
        tab_id: i64,
        x: f64,
        y: f64,
        modifiers: u64,
    ) -> Result<TabSnapshot, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(Command::MouseMove {
                tab_id,
                x,
                y,
                modifiers,
                reply,
            })
            .map_err(|error| format!("engine mouse-move send failed: {error}"))?;
        recv_reply(rx, "mouse-move")
    }

    #[allow(clippy::too_many_arguments)]
    pub fn scroll_event(
        &self,
        tab_id: i64,
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
        phase: u64,
        momentum_phase: u64,
        precise: bool,
        modifiers: u64,
    ) -> Result<TabSnapshot, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(Command::ScrollEvent {
                tab_id,
                x,
                y,
                delta_x,
                delta_y,
                phase,
                momentum_phase,
                precise,
                modifiers,
                reply,
            })
            .map_err(|error| format!("engine scroll-event send failed: {error}"))?;
        recv_reply(rx, "scroll-event")
    }

    pub fn key_event(
        &self,
        tab_id: i64,
        event_type: String,
        windows_key_code: i32,
        utf8: String,
        modifiers: u64,
    ) -> Result<TabSnapshot, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(Command::KeyEvent {
                tab_id,
                event_type,
                windows_key_code,
                utf8,
                modifiers,
                reply,
            })
            .map_err(|error| format!("engine key-event send failed: {error}"))?;
        recv_reply(rx, "key-event")
    }

    pub fn javascript_dialog_reply(
        &self,
        tab_id: i64,
        request_id: u64,
        accepted: bool,
        prompt_text: String,
    ) -> Result<TabSnapshot, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(Command::JavaScriptDialogReply {
                tab_id,
                request_id,
                accepted,
                prompt_text,
                reply,
            })
            .map_err(|error| format!("engine JavaScript dialog reply send failed: {error}"))?;
        recv_reply(rx, "javascript-dialog-reply")
    }

    pub fn set_focus(&self, tab_id: i64, focused: bool) -> Result<TabSnapshot, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(Command::SetFocus {
                tab_id,
                focused,
                reply,
            })
            .map_err(|error| format!("engine focus send failed: {error}"))?;
        recv_reply(rx, "focus")
    }

    pub fn set_color_scheme(&self, tab_id: i64, dark: bool) -> Result<TabSnapshot, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(Command::SetColorScheme {
                tab_id,
                dark,
                reply,
            })
            .map_err(|error| format!("engine color-scheme send failed: {error}"))?;
        recv_reply(rx, "color-scheme")
    }

    pub fn set_gui_active(&self, tab_id: i64, active: bool) -> Result<StateOutcome, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(Command::SetGuiActive {
                tab_id,
                active,
                reply,
            })
            .map_err(|error| format!("engine gui-active send failed: {error}"))?;
        recv_reply(rx, "gui-active")
    }

    pub fn close_tab(&self, tab_id: i64) -> Result<CloseOutcome, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(Command::CloseTab { tab_id, reply })
            .map_err(|error| format!("engine close-tab send failed: {error}"))?;
        recv_reply(rx, "close-tab")
    }

    pub fn snapshot(&self) -> Vec<TabSnapshot> {
        let (reply, rx) = mpsc::channel();
        if self.tx.send(Command::Snapshot { reply }).is_err() {
            return Vec::new();
        }
        rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default()
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(join) = self.join.lock().unwrap().take() {
            if join.join().is_err() {
                eprintln!("[Ladybird] engine owner thread panicked during shutdown");
            }
        }
    }
}

pub fn init_global() -> Result<&'static EngineService, String> {
    if let Some(engine) = ENGINE.get() {
        return Ok(engine);
    }
    let engine = EngineService::start()?;
    match ENGINE.set(engine) {
        Ok(()) => Ok(ENGINE.get().expect("engine was just initialized")),
        Err(_) => Ok(ENGINE.get().expect("engine initialized concurrently")),
    }
}

pub fn global() -> Option<&'static EngineService> {
    ENGINE.get()
}

pub fn shutdown_global() {
    if let Some(engine) = ENGINE.get() {
        engine.shutdown();
    }
}

pub fn tab_info_from_snapshot(tab: &TabSnapshot) -> termsurf::TabInfo {
    termsurf::TabInfo {
        id: tab.id,
        inspected_tab_id: tab.inspected_tab_id,
        pane_id: tab.pane_id.clone(),
        url: tab.url.clone(),
    }
}

pub fn owner_thread_smoke() -> bool {
    let caller_thread = format!("{:?}", thread::current().id());
    eprintln!("[Ladybird] engine-thread-smoke caller_thread={caller_thread}");

    let (command_tx, command_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();
    let join = thread::spawn(move || owner_thread(command_rx, event_tx));

    let mut ok = true;

    match recv_event(&event_rx, "runtime created") {
        Some(Event::RuntimeCreated { owner_thread }) => {
            eprintln!("[Ladybird] engine-thread-smoke owner_thread={owner_thread}");
            if owner_thread == caller_thread {
                eprintln!("[Ladybird] engine-thread-smoke owner thread matched caller thread");
                ok = false;
            }
        }
        Some(Event::Failed { message }) => {
            eprintln!("[Ladybird] engine-thread-smoke runtime failed: {message}");
            ok = false;
        }
        other => {
            eprintln!("[Ladybird] engine-thread-smoke unexpected startup event: {other:?}");
            ok = false;
        }
    }

    let created_tab_id = if ok {
        let (empty_active_tx, empty_active_rx) = mpsc::channel();
        ok = command_tx
            .send(Command::SetGuiActive {
                tab_id: 0,
                active: false,
                reply: empty_active_tx,
            })
            .is_ok();
        if ok {
            match empty_active_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(outcome)) if outcome.affected_count == 0 && outcome.snapshots.is_empty() => {
                    eprintln!("[Ladybird] engine-thread-smoke empty gui-active broadcast ok");
                }
                other => {
                    eprintln!(
                        "[Ladybird] engine-thread-smoke empty gui-active broadcast failed: {other:?}"
                    );
                    ok = false;
                }
            }
        }

        if ok {
            let (reply, rx) = mpsc::channel();
            ok = command_tx
                .send(Command::CreateTab {
                    url: DATA_URL.to_string(),
                    pane_id: "engine-smoke-pane".to_string(),
                    width: 800,
                    height: 600,
                    dark: true,
                    reply,
                })
                .is_ok();
            if ok {
                match rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(Ok(tab)) => Some(tab.id),
                    other => {
                        eprintln!("[Ladybird] engine-thread-smoke create reply failed: {other:?}");
                        ok = false;
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    if let Some(tab_id) = created_tab_id {
        match recv_event(&event_rx, "view created") {
            Some(Event::ViewCreated {
                tab_id: event_tab_id,
            }) if event_tab_id == tab_id => {
                eprintln!("[Ladybird] engine-thread-smoke view created tab_id={event_tab_id}");
            }
            Some(Event::Failed { message }) => {
                eprintln!("[Ladybird] engine-thread-smoke create failed: {message}");
                ok = false;
            }
            other => {
                eprintln!("[Ladybird] engine-thread-smoke unexpected create event: {other:?}");
                ok = false;
            }
        }
    }

    if let Some(tab_id) = created_tab_id {
        match recv_event(&event_rx, "load finished") {
            Some(Event::LoadFinished {
                tab_id: event_tab_id,
                url,
            }) if event_tab_id == tab_id && url.starts_with("data:") => {
                eprintln!(
                    "[Ladybird] engine-thread-smoke load finished tab_id={event_tab_id} url={url}"
                );
            }
            Some(Event::Failed { message }) => {
                eprintln!("[Ladybird] engine-thread-smoke load failed: {message}");
                ok = false;
            }
            other => {
                eprintln!("[Ladybird] engine-thread-smoke unexpected load event: {other:?}");
                ok = false;
            }
        }
    }

    if let Some(tab_id) = created_tab_id {
        let (resize_tx, resize_rx) = mpsc::channel();
        ok = command_tx
            .send(Command::Resize {
                tab_id,
                width: 1024,
                height: 768,
                reply: resize_tx,
            })
            .is_ok();
        if ok {
            match resize_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(tab)) if tab.width == 1024 && tab.height == 768 => {
                    eprintln!(
                        "[Ladybird] engine-thread-smoke resized tab_id={tab_id} size={}x{}",
                        tab.width, tab.height
                    );
                }
                other => {
                    eprintln!("[Ladybird] engine-thread-smoke resize reply failed: {other:?}");
                    ok = false;
                }
            }
        }
    }

    if let Some(tab_id) = created_tab_id {
        let (focus_tx, focus_rx) = mpsc::channel();
        ok = command_tx
            .send(Command::SetFocus {
                tab_id,
                focused: true,
                reply: focus_tx,
            })
            .is_ok();
        if ok {
            match focus_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(tab)) if tab.focused => {
                    eprintln!("[Ladybird] engine-thread-smoke focused tab_id={tab_id}");
                }
                other => {
                    eprintln!("[Ladybird] engine-thread-smoke focus reply failed: {other:?}");
                    ok = false;
                }
            }
        }
    }

    if let Some(tab_id) = created_tab_id {
        let (color_tx, color_rx) = mpsc::channel();
        ok = command_tx
            .send(Command::SetColorScheme {
                tab_id,
                dark: false,
                reply: color_tx,
            })
            .is_ok();
        if ok {
            match color_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(tab)) if !tab.dark => {
                    eprintln!(
                        "[Ladybird] engine-thread-smoke color-scheme tab_id={tab_id} dark=false"
                    );
                }
                other => {
                    eprintln!(
                        "[Ladybird] engine-thread-smoke color-scheme reply failed: {other:?}"
                    );
                    ok = false;
                }
            }
        }
    }

    if let Some(tab_id) = created_tab_id {
        let (inactive_tx, inactive_rx) = mpsc::channel();
        ok = command_tx
            .send(Command::SetGuiActive {
                tab_id: 0,
                active: false,
                reply: inactive_tx,
            })
            .is_ok();
        if ok {
            match inactive_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(outcome)) if outcome.affected_count == 1 => {
                    eprintln!(
                        "[Ladybird] engine-thread-smoke gui-active broadcast active=false target_count={}",
                        outcome.affected_count
                    );
                }
                other => {
                    eprintln!(
                        "[Ladybird] engine-thread-smoke gui-active broadcast failed: {other:?}"
                    );
                    ok = false;
                }
            }
        }

        if ok {
            let (active_tx, active_rx) = mpsc::channel();
            ok = command_tx
                .send(Command::SetGuiActive {
                    tab_id,
                    active: true,
                    reply: active_tx,
                })
                .is_ok();
            if ok {
                match active_rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(Ok(outcome))
                        if outcome.affected_count == 1
                            && outcome
                                .snapshots
                                .first()
                                .map(|tab| tab.gui_active)
                                .unwrap_or(false) =>
                    {
                        eprintln!(
                            "[Ladybird] engine-thread-smoke gui-active tab_id={tab_id} active=true"
                        );
                    }
                    other => {
                        eprintln!(
                            "[Ladybird] engine-thread-smoke gui-active reply failed: {other:?}"
                        );
                        ok = false;
                    }
                }
            }
        }
    }

    if let Some(tab_id) = created_tab_id {
        let input_commands = [
            (
                "mouse-move",
                Command::MouseMove {
                    tab_id,
                    x: 24.0,
                    y: 32.0,
                    modifiers: 0,
                    reply: mpsc::channel().0,
                },
            ),
            (
                "mouse-down",
                Command::MouseEvent {
                    tab_id,
                    event_type: "down".to_string(),
                    button: "left".to_string(),
                    x: 120.0,
                    y: 120.0,
                    click_count: 1,
                    modifiers: 64,
                    reply: mpsc::channel().0,
                },
            ),
            (
                "mouse-up",
                Command::MouseEvent {
                    tab_id,
                    event_type: "up".to_string(),
                    button: "left".to_string(),
                    x: 120.0,
                    y: 120.0,
                    click_count: 1,
                    modifiers: 0,
                    reply: mpsc::channel().0,
                },
            ),
            (
                "scroll",
                Command::ScrollEvent {
                    tab_id,
                    x: 24.0,
                    y: 32.0,
                    delta_x: 0.0,
                    delta_y: -48.0,
                    phase: 1,
                    momentum_phase: 0,
                    precise: true,
                    modifiers: 0,
                    reply: mpsc::channel().0,
                },
            ),
            (
                "key-down",
                Command::KeyEvent {
                    tab_id,
                    event_type: "down".to_string(),
                    windows_key_code: 65,
                    utf8: "a".to_string(),
                    modifiers: 0,
                    reply: mpsc::channel().0,
                },
            ),
            (
                "key-up",
                Command::KeyEvent {
                    tab_id,
                    event_type: "up".to_string(),
                    windows_key_code: 65,
                    utf8: String::new(),
                    modifiers: 0,
                    reply: mpsc::channel().0,
                },
            ),
        ];

        for (label, command) in input_commands {
            let (reply, rx) = mpsc::channel();
            let command = attach_input_reply(command, reply);
            ok = command_tx.send(command).is_ok();
            if ok {
                match rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(Ok(tab)) => {
                        eprintln!(
                            "[Ladybird] engine-thread-smoke input {label} tab_id={} counters=mouse_button:{} mouse_move:{} scroll:{} key:{}",
                            tab.id,
                            tab.mouse_button_events,
                            tab.mouse_move_events,
                            tab.scroll_events,
                            tab.key_events
                        );
                    }
                    other => {
                        eprintln!(
                            "[Ladybird] engine-thread-smoke input {label} reply failed: {other:?}"
                        );
                        ok = false;
                    }
                }
            }
        }
    }

    if let Some(tab_id) = created_tab_id {
        let snapshot_deadline = Instant::now() + Duration::from_secs(2);
        let mut matched_snapshot = false;
        while Instant::now() < snapshot_deadline {
            let (snapshot_tx, snapshot_rx) = mpsc::channel();
            ok = command_tx
                .send(Command::Snapshot { reply: snapshot_tx })
                .is_ok();
            if !ok {
                break;
            }
            match snapshot_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(snapshot) => {
                    eprintln!("[Ladybird] engine-thread-smoke snapshot={snapshot:?}");
                    matched_snapshot = snapshot.iter().any(|view| {
                        view.id == tab_id
                            && view.finished
                            && !view.crashed
                            && view.url.starts_with("data:")
                            && view.width == 1024
                            && view.height == 768
                            && view.focused
                            && !view.dark
                            && view.gui_active
                            && view.last_title == "Ladybird Engine"
                            && view.mouse_button_events == 2
                            && view.mouse_move_events == 1
                            && view.scroll_events == 1
                            && view.key_events == 2
                            && view.console_message_count >= 1
                            && view
                                .last_console_message
                                .contains("Ladybird console engine")
                            && view.cursor_change_count >= 1
                            && view.last_cursor_type == 2
                            && view.target_url_change_count >= 1
                            && (view.last_target_url.is_empty()
                                || view.last_target_url.contains("ladybird-engine"))
                            && view.last_non_empty_target_url.contains("ladybird-engine")
                    });
                    if matched_snapshot {
                        break;
                    }
                }
                Err(error) => {
                    eprintln!("[Ladybird] engine-thread-smoke snapshot timed out: {error}");
                    ok = false;
                    break;
                }
            }
            thread::sleep(Duration::from_millis(50));
        }
        if ok && !matched_snapshot {
            eprintln!("[Ladybird] engine-thread-smoke snapshot never matched expected state");
            ok = false;
        }
    }

    if let Some(tab_id) = created_tab_id {
        let (navigate_tx, navigate_rx) = mpsc::channel();
        ok = command_tx
            .send(Command::Navigate {
                tab_id,
                url: DIALOG_DATA_URL.to_string(),
                reply: navigate_tx,
            })
            .is_ok();
        if ok {
            match navigate_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(tab)) if tab.url == DIALOG_DATA_URL => {
                    eprintln!("[Ladybird] engine-thread-smoke dialog navigate tab_id={tab_id}");
                }
                other => {
                    eprintln!("[Ladybird] engine-thread-smoke dialog navigate failed: {other:?}");
                    ok = false;
                }
            }
        }
    }

    if let Some(tab_id) = created_tab_id {
        let dialog_deadline = Instant::now() + Duration::from_secs(5);
        let mut dialog_request_id = 0_u64;
        while ok && Instant::now() < dialog_deadline {
            let (snapshot_tx, snapshot_rx) = mpsc::channel();
            ok = command_tx
                .send(Command::Snapshot { reply: snapshot_tx })
                .is_ok();
            if !ok {
                break;
            }
            match snapshot_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(snapshot) => {
                    if let Some(view) = snapshot.iter().find(|view| {
                        view.id == tab_id
                            && view.javascript_dialog_request_count >= 1
                            && view.last_javascript_dialog_type == "prompt"
                            && view
                                .last_javascript_dialog_message
                                .contains("ladybird-dialog-message")
                            && view
                                .last_javascript_dialog_default_prompt_text
                                .contains("ladybird-dialog-default")
                            && !view.last_javascript_dialog_origin_url.is_empty()
                    }) {
                        dialog_request_id = view.last_javascript_dialog_request_id;
                        eprintln!(
                            "[Ladybird] engine-thread-smoke dialog request tab_id={tab_id} request_id={dialog_request_id}"
                        );
                        break;
                    }
                }
                Err(error) => {
                    eprintln!("[Ladybird] engine-thread-smoke dialog snapshot timed out: {error}");
                    ok = false;
                    break;
                }
            }
            thread::sleep(Duration::from_millis(50));
        }
        if ok && dialog_request_id == 0 {
            eprintln!("[Ladybird] engine-thread-smoke dialog request never arrived");
            ok = false;
        }
        if ok {
            let (reply_tx, reply_rx) = mpsc::channel();
            ok = command_tx
                .send(Command::JavaScriptDialogReply {
                    tab_id,
                    request_id: dialog_request_id,
                    accepted: true,
                    prompt_text: "ladybird-dialog-replied".to_string(),
                    reply: reply_tx,
                })
                .is_ok();
            if ok {
                match reply_rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(Ok(tab))
                        if tab.javascript_dialog_reply_count >= 1
                            && tab.last_javascript_dialog_reply_request_id == dialog_request_id
                            && tab.last_javascript_dialog_reply_accepted
                            && tab.last_javascript_dialog_reply_ok =>
                    {
                        eprintln!(
                            "[Ladybird] engine-thread-smoke dialog reply tab_id={tab_id} request_id={dialog_request_id}"
                        );
                    }
                    other => {
                        eprintln!("[Ladybird] engine-thread-smoke dialog reply failed: {other:?}");
                        ok = false;
                    }
                }
            }
        }
    }

    if let Some(tab_id) = created_tab_id {
        match recv_event(&event_rx, "dialog load finished") {
            Some(Event::LoadFinished {
                tab_id: event_tab_id,
                url,
            }) if event_tab_id == tab_id && url.starts_with("data:") => {
                eprintln!(
                    "[Ladybird] engine-thread-smoke dialog load finished tab_id={event_tab_id} url={url}"
                );
            }
            Some(Event::Failed { message }) => {
                eprintln!("[Ladybird] engine-thread-smoke dialog load failed: {message}");
                ok = false;
            }
            other => {
                eprintln!("[Ladybird] engine-thread-smoke unexpected dialog load event: {other:?}");
                ok = false;
            }
        }
    }

    let _ = command_tx.send(Command::Shutdown);
    let saw_shutdown = match recv_event(&event_rx, "shutdown") {
        Some(Event::ShutdownComplete) => {
            eprintln!("[Ladybird] engine-thread-smoke shutdown complete");
            true
        }
        other => {
            eprintln!("[Ladybird] engine-thread-smoke unexpected shutdown event: {other:?}");
            ok = false;
            false
        }
    };

    if saw_shutdown && join.join().is_err() {
        eprintln!("[Ladybird] engine-thread-smoke owner thread panicked");
        ok = false;
    }

    ok
}

fn attach_input_reply(
    command: Command,
    reply: mpsc::Sender<Result<TabSnapshot, String>>,
) -> Command {
    match command {
        Command::MouseEvent {
            tab_id,
            event_type,
            button,
            x,
            y,
            click_count,
            modifiers,
            reply: _,
        } => Command::MouseEvent {
            tab_id,
            event_type,
            button,
            x,
            y,
            click_count,
            modifiers,
            reply,
        },
        Command::MouseMove {
            tab_id,
            x,
            y,
            modifiers,
            reply: _,
        } => Command::MouseMove {
            tab_id,
            x,
            y,
            modifiers,
            reply,
        },
        Command::ScrollEvent {
            tab_id,
            x,
            y,
            delta_x,
            delta_y,
            phase,
            momentum_phase,
            precise,
            modifiers,
            reply: _,
        } => Command::ScrollEvent {
            tab_id,
            x,
            y,
            delta_x,
            delta_y,
            phase,
            momentum_phase,
            precise,
            modifiers,
            reply,
        },
        Command::KeyEvent {
            tab_id,
            event_type,
            windows_key_code,
            utf8,
            modifiers,
            reply: _,
        } => Command::KeyEvent {
            tab_id,
            event_type,
            windows_key_code,
            utf8,
            modifiers,
            reply,
        },
        _ => command,
    }
}

fn recv_reply<T>(rx: mpsc::Receiver<Result<T, String>>, label: &str) -> Result<T, String> {
    match rx.recv_timeout(Duration::from_secs(30)) {
        Ok(result) => result,
        Err(error) => Err(format!("engine {label} reply timed out: {error}")),
    }
}

fn recv_event(rx: &mpsc::Receiver<Event>, label: &str) -> Option<Event> {
    match rx.recv_timeout(Duration::from_secs(30)) {
        Ok(event) => Some(event),
        Err(error) => {
            eprintln!("[Ladybird] engine-thread-smoke timed out waiting for {label}: {error}");
            None
        }
    }
}

fn owner_thread(rx: mpsc::Receiver<Command>, events: mpsc::Sender<Event>) {
    let owner_thread = format!("{:?}", thread::current().id());
    let runtime = match AbiRuntime::create() {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = events.send(Event::Failed {
                message: format!("runtime create failed: {error}"),
            });
            return;
        }
    };
    let _ = events.send(Event::RuntimeCreated { owner_thread });

    let mut next_tab_id = 1_i64;
    let mut tabs: HashMap<i64, OwnedTab> = HashMap::new();
    let mut idle_logged = false;
    let mut pump_until: Option<Instant> = None;

    loop {
        let has_in_flight = tabs.values().any(|tab| tab.in_flight);
        if pump_until
            .map(|deadline| Instant::now() >= deadline)
            .unwrap_or(false)
        {
            pump_until = None;
        }
        let has_pending_render_surface = tabs.values().any(tab_needs_render_surface_retry);
        let should_pump = has_in_flight || has_pending_render_surface || pump_until.is_some();
        let command = if should_pump {
            rx.recv_timeout(Duration::from_millis(5)).ok()
        } else {
            if !idle_logged {
                eprintln!("[Ladybird] engine owner idle blocking on commands");
                idle_logged = true;
            }
            match rx.recv() {
                Ok(command) => Some(command),
                Err(_) => Some(Command::Shutdown),
            }
        };

        if let Some(command) = command {
            idle_logged = false;
            if process_command(
                command,
                &runtime,
                &mut next_tab_id,
                &mut tabs,
                &events,
                &mut pump_until,
            ) {
                break;
            }
        }

        if should_pump {
            if let Err(error) = runtime.pump() {
                let _ = events.send(Event::Failed {
                    message: format!("runtime pump failed: {error}"),
                });
                break;
            }
            publish_title_changes(&mut tabs);
            publish_console_messages(&mut tabs);
            publish_javascript_dialog_requests(&mut tabs);
            publish_hover_changes(&mut tabs);
            publish_renderer_crashes(&mut tabs);
            publish_finished_loads(&mut tabs, &events);
            observe_navigation_changes(&mut tabs);
            publish_pending_render_surfaces(&mut tabs);
            thread::sleep(Duration::from_millis(1));
        }
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    tabs.clear();
    while Instant::now() < deadline {
        if runtime.pump().is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    drop(runtime);
    let _ = events.send(Event::ShutdownComplete);
}

fn process_command(
    command: Command,
    runtime: &AbiRuntime,
    next_tab_id: &mut i64,
    tabs: &mut HashMap<i64, OwnedTab>,
    events: &mpsc::Sender<Event>,
    pump_until: &mut Option<Instant>,
) -> bool {
    match command {
        Command::CreateTab {
            url,
            pane_id,
            width,
            height,
            dark,
            reply,
        } => {
            let tab_id = *next_tab_id;
            *next_tab_id += 1;
            let result = create_tab(
                runtime, tabs, events, tab_id, pane_id, url, width, height, dark,
            );
            let _ = reply.send(result);
        }
        Command::Navigate { tab_id, url, reply } => {
            let result = navigate_tab(tabs, tab_id, url);
            let _ = reply.send(result);
        }
        Command::NavigationAction {
            tab_id,
            action,
            request_id,
            reply,
        } => {
            let result = navigation_action(tabs, tab_id, action, request_id);
            if result.is_ok() {
                *pump_until = Some(Instant::now() + Duration::from_millis(3000));
            }
            let _ = reply.send(result);
        }
        Command::Resize {
            tab_id,
            width,
            height,
            reply,
        } => {
            let result = resize_tab(tabs, tab_id, width, height);
            let _ = reply.send(result);
        }
        Command::MouseEvent {
            tab_id,
            event_type,
            button,
            x,
            y,
            click_count,
            modifiers,
            reply,
        } => {
            let result = mouse_event(
                tabs,
                tab_id,
                event_type,
                button,
                x,
                y,
                click_count,
                modifiers,
            );
            if result.is_ok() {
                *pump_until = Some(Instant::now() + Duration::from_millis(3000));
            }
            let _ = reply.send(result);
        }
        Command::MouseMove {
            tab_id,
            x,
            y,
            modifiers,
            reply,
        } => {
            let result = mouse_move(tabs, tab_id, x, y, modifiers);
            if result.is_ok() {
                *pump_until = Some(Instant::now() + Duration::from_millis(1000));
            }
            let _ = reply.send(result);
        }
        Command::ScrollEvent {
            tab_id,
            x,
            y,
            delta_x,
            delta_y,
            phase,
            momentum_phase,
            precise,
            modifiers,
            reply,
        } => {
            let result = scroll_event(
                tabs,
                tab_id,
                x,
                y,
                delta_x,
                delta_y,
                phase,
                momentum_phase,
                precise,
                modifiers,
            );
            let _ = reply.send(result);
        }
        Command::KeyEvent {
            tab_id,
            event_type,
            windows_key_code,
            utf8,
            modifiers,
            reply,
        } => {
            let result = key_event(tabs, tab_id, event_type, windows_key_code, utf8, modifiers);
            if result.is_ok() {
                *pump_until = Some(Instant::now() + Duration::from_millis(3000));
            }
            let _ = reply.send(result);
        }
        Command::JavaScriptDialogReply {
            tab_id,
            request_id,
            accepted,
            prompt_text,
            reply,
        } => {
            let result = javascript_dialog_reply(tabs, tab_id, request_id, accepted, prompt_text);
            let _ = reply.send(result);
        }
        Command::SetFocus {
            tab_id,
            focused,
            reply,
        } => {
            let result = set_focus(tabs, tab_id, focused);
            let _ = reply.send(result);
        }
        Command::SetColorScheme {
            tab_id,
            dark,
            reply,
        } => {
            let result = set_color_scheme(tabs, tab_id, dark);
            let _ = reply.send(result);
        }
        Command::SetGuiActive {
            tab_id,
            active,
            reply,
        } => {
            let result = set_gui_active(tabs, tab_id, active);
            let _ = reply.send(result);
        }
        Command::CloseTab { tab_id, reply } => {
            if tabs.contains_key(&tab_id) {
                publish_navigation_state(tab_id, false, false, false);
            }
            let removed = tabs.remove(&tab_id).is_some();
            if removed {
                eprintln!("[Ladybird] engine CloseTab destroyed view tab_id={tab_id}");
            }
            let remaining_browser_tabs = tabs.len();
            let result = if removed {
                Ok(CloseOutcome {
                    removed,
                    remaining_browser_tabs,
                })
            } else {
                Err(format!("missing tab_id={tab_id}"))
            };
            let _ = reply.send(result);
        }
        Command::Snapshot { reply } => {
            let snapshot = tabs.values().map(snapshot_from_tab).collect();
            let _ = reply.send(snapshot);
        }
        Command::Shutdown => return true,
    }
    false
}

fn create_tab(
    runtime: &AbiRuntime,
    tabs: &mut HashMap<i64, OwnedTab>,
    events: &mpsc::Sender<Event>,
    tab_id: i64,
    pane_id: String,
    url: String,
    width: i32,
    height: i32,
    dark: bool,
) -> Result<TabSnapshot, String> {
    let view = runtime.create_view(width, height)?;
    view.set_color_scheme(dark)?;
    view.set_gui_active(true)?;
    view.load_url(&url)?;
    let tab = OwnedTab {
        id: tab_id,
        inspected_tab_id: 0,
        pane_id,
        target_url: url,
        width,
        height,
        focused: false,
        dark,
        gui_active: true,
        last_title: String::new(),
        view,
        in_flight: true,
        last_url: String::new(),
        finished: false,
        crashed: false,
        can_go_back: false,
        can_go_forward: false,
        can_refresh: false,
        refresh_request_id: 0,
        render_surface_sent: false,
        render_surface_attempts: 0,
        mouse_button_events: 0,
        mouse_move_events: 0,
        scroll_events: 0,
        key_events: 0,
        console_message_count: 0,
        last_console_message: String::new(),
        cursor_change_count: 0,
        last_cursor_type: 0,
        target_url_change_count: 0,
        last_target_url: String::new(),
        last_non_empty_target_url: String::new(),
        javascript_dialog_request_count: 0,
        javascript_dialog_reply_count: 0,
        last_javascript_dialog_request_id: 0,
        last_javascript_dialog_reply_request_id: 0,
        last_javascript_dialog_type: String::new(),
        last_javascript_dialog_message: String::new(),
        last_javascript_dialog_default_prompt_text: String::new(),
        last_javascript_dialog_origin_url: String::new(),
        last_javascript_dialog_reply_accepted: false,
        last_javascript_dialog_reply_ok: false,
    };
    let snapshot = snapshot_from_tab(&tab);
    tabs.insert(tab_id, tab);
    eprintln!(
        "[Ladybird] engine CreateTab created view tab_id={} pane_id={} url={} dark={} gui_active={}",
        snapshot.id, snapshot.pane_id, snapshot.url, snapshot.dark, snapshot.gui_active
    );
    publish_tab_ready(&snapshot);
    publish_navigation_state(snapshot.id, false, false, false);
    publish_loading_state(snapshot.id, "loading", 0, 0);
    publish_url_changed(snapshot.id, &snapshot.url);
    let _ = events.send(Event::ViewCreated { tab_id });
    Ok(snapshot)
}

fn navigate_tab(
    tabs: &mut HashMap<i64, OwnedTab>,
    tab_id: i64,
    url: String,
) -> Result<TabSnapshot, String> {
    let tab = tabs
        .get_mut(&tab_id)
        .ok_or_else(|| format!("missing tab_id={tab_id}"))?;
    tab.view.load_url(&url)?;
    tab.target_url = url;
    tab.last_url.clear();
    tab.in_flight = true;
    tab.finished = false;
    tab.render_surface_sent = false;
    tab.render_surface_attempts = 0;
    tab.last_console_message.clear();
    tab.last_target_url.clear();
    tab.last_non_empty_target_url.clear();
    tab.last_javascript_dialog_request_id = 0;
    tab.last_javascript_dialog_reply_request_id = 0;
    tab.last_javascript_dialog_type.clear();
    tab.last_javascript_dialog_message.clear();
    tab.last_javascript_dialog_default_prompt_text.clear();
    tab.last_javascript_dialog_origin_url.clear();
    tab.last_javascript_dialog_reply_accepted = false;
    tab.last_javascript_dialog_reply_ok = false;
    eprintln!(
        "[Ladybird] engine Navigate requested tab_id={} url={}",
        tab.id, tab.target_url
    );
    let snapshot = snapshot_from_tab(tab);
    publish_loading_state(snapshot.id, "loading", 0, 0);
    publish_url_changed(snapshot.id, &snapshot.url);
    Ok(snapshot)
}

fn navigation_action(
    tabs: &mut HashMap<i64, OwnedTab>,
    tab_id: i64,
    action: String,
    request_id: u64,
) -> Result<TabSnapshot, String> {
    let tab = tabs
        .get_mut(&tab_id)
        .ok_or_else(|| format!("missing tab_id={tab_id}"))?;
    validate_navigation_action(
        tab_id,
        &action,
        tab.can_go_back,
        tab.can_go_forward,
        tab.can_refresh,
        tab.crashed,
    )?;

    tab.view.navigation_action(&action)?;
    if action == "refresh" {
        tab.refresh_request_id = request_id;
        tab.in_flight = true;
        tab.finished = false;
        publish_loading_state(tab.id, "loading", 0, request_id);
    }
    eprintln!(
        "[Ladybird] engine NavigationAction accepted tab_id={} action={action} mode=native-start",
        tab.id
    );
    Ok(snapshot_from_tab(tab))
}

fn validate_navigation_action(
    tab_id: i64,
    action: &str,
    can_go_back: bool,
    can_go_forward: bool,
    can_refresh: bool,
    crashed: bool,
) -> Result<(), String> {
    if !matches!(action, "back" | "forward" | "refresh") {
        return Err(format!("unsupported navigation action={action}"));
    }
    if crashed && action != "refresh" {
        return Err(format!("tab_id={tab_id} is crash-latched"));
    }
    let enabled = match action {
        "back" => can_go_back,
        "forward" => can_go_forward,
        "refresh" => can_refresh,
        _ => false,
    };
    if !enabled {
        return Err(format!("tab_id={tab_id} {action} navigation is disabled"));
    }
    Ok(())
}

fn resize_tab(
    tabs: &mut HashMap<i64, OwnedTab>,
    tab_id: i64,
    width: i32,
    height: i32,
) -> Result<TabSnapshot, String> {
    let tab = tabs
        .get_mut(&tab_id)
        .ok_or_else(|| format!("missing tab_id={tab_id}"))?;
    tab.view.resize(width, height)?;
    tab.width = width;
    tab.height = height;
    eprintln!(
        "[Ladybird] engine Resize applied tab_id={} size={}x{}",
        tab.id, tab.width, tab.height
    );
    Ok(snapshot_from_tab(tab))
}

#[allow(clippy::too_many_arguments)]
fn mouse_event(
    tabs: &mut HashMap<i64, OwnedTab>,
    tab_id: i64,
    event_type: String,
    button: String,
    x: f64,
    y: f64,
    click_count: i32,
    modifiers: u64,
) -> Result<TabSnapshot, String> {
    let tab = tabs
        .get_mut(&tab_id)
        .ok_or_else(|| format!("missing tab_id={tab_id}"))?;
    tab.view
        .mouse_event(&event_type, &button, x, y, click_count, modifiers)?;
    tab.mouse_button_events += 1;
    eprintln!(
        "[Ladybird] engine MouseEvent queued tab_id={} type={} button={} coords=({:.2}, {:.2}) click_count={} modifiers={} mouse_button_events={}",
        tab.id, event_type, button, x, y, click_count, modifiers, tab.mouse_button_events
    );
    Ok(snapshot_from_tab(tab))
}

fn mouse_move(
    tabs: &mut HashMap<i64, OwnedTab>,
    tab_id: i64,
    x: f64,
    y: f64,
    modifiers: u64,
) -> Result<TabSnapshot, String> {
    let tab = tabs
        .get_mut(&tab_id)
        .ok_or_else(|| format!("missing tab_id={tab_id}"))?;
    tab.view.mouse_move(x, y, modifiers)?;
    tab.mouse_move_events += 1;
    eprintln!(
        "[Ladybird] engine MouseMove queued tab_id={} coords=({:.2}, {:.2}) modifiers={} mouse_move_events={}",
        tab.id, x, y, modifiers, tab.mouse_move_events
    );
    Ok(snapshot_from_tab(tab))
}

#[allow(clippy::too_many_arguments)]
fn scroll_event(
    tabs: &mut HashMap<i64, OwnedTab>,
    tab_id: i64,
    x: f64,
    y: f64,
    delta_x: f64,
    delta_y: f64,
    phase: u64,
    momentum_phase: u64,
    precise: bool,
    modifiers: u64,
) -> Result<TabSnapshot, String> {
    let tab = tabs
        .get_mut(&tab_id)
        .ok_or_else(|| format!("missing tab_id={tab_id}"))?;
    tab.view.scroll_event(
        x,
        y,
        delta_x,
        delta_y,
        phase,
        momentum_phase,
        precise,
        modifiers,
    )?;
    tab.scroll_events += 1;
    eprintln!(
        "[Ladybird] engine ScrollEvent queued tab_id={} coords=({:.2}, {:.2}) delta=({:.2}, {:.2}) phase={} momentum_phase={} precise={} modifiers={} scroll_events={}",
        tab.id, x, y, delta_x, delta_y, phase, momentum_phase, precise, modifiers, tab.scroll_events
    );
    Ok(snapshot_from_tab(tab))
}

fn key_event(
    tabs: &mut HashMap<i64, OwnedTab>,
    tab_id: i64,
    event_type: String,
    windows_key_code: i32,
    utf8: String,
    modifiers: u64,
) -> Result<TabSnapshot, String> {
    let tab = tabs
        .get_mut(&tab_id)
        .ok_or_else(|| format!("missing tab_id={tab_id}"))?;
    tab.view
        .key_event(&event_type, windows_key_code, &utf8, modifiers)?;
    tab.key_events += 1;
    eprintln!(
        "[Ladybird] engine KeyEvent queued tab_id={} type={} windows_key_code={} utf8_len={} modifiers={} key_events={}",
        tab.id,
        event_type,
        windows_key_code,
        utf8.len(),
        modifiers,
        tab.key_events
    );
    Ok(snapshot_from_tab(tab))
}

fn javascript_dialog_reply(
    tabs: &mut HashMap<i64, OwnedTab>,
    tab_id: i64,
    request_id: u64,
    accepted: bool,
    prompt_text: String,
) -> Result<TabSnapshot, String> {
    let tab = tabs
        .get_mut(&tab_id)
        .ok_or_else(|| format!("missing tab_id={tab_id}"))?;
    let result = tab
        .view
        .reply_javascript_dialog(request_id, accepted, &prompt_text);
    tab.javascript_dialog_reply_count += 1;
    tab.last_javascript_dialog_reply_request_id = request_id;
    tab.last_javascript_dialog_reply_accepted = accepted;
    tab.last_javascript_dialog_reply_ok = result.is_ok();
    match result {
        Ok(()) => {
            eprintln!(
                "[Ladybird] engine JavaScriptDialogReply applied tab_id={} request_id={} accepted={} prompt_text_len={}",
                tab.id,
                request_id,
                accepted,
                prompt_text.len()
            );
            Ok(snapshot_from_tab(tab))
        }
        Err(error) => {
            eprintln!(
                "[Ladybird] engine JavaScriptDialogReply failed tab_id={} request_id={} accepted={} error={error}",
                tab.id, request_id, accepted
            );
            Err(error)
        }
    }
}

fn set_focus(
    tabs: &mut HashMap<i64, OwnedTab>,
    tab_id: i64,
    focused: bool,
) -> Result<TabSnapshot, String> {
    let tab = tabs
        .get_mut(&tab_id)
        .ok_or_else(|| format!("missing tab_id={tab_id}"))?;
    tab.focused = focused;
    eprintln!(
        "[Ladybird] engine FocusChanged tracked tab_id={} focused={}",
        tab.id, tab.focused
    );
    Ok(snapshot_from_tab(tab))
}

fn set_color_scheme(
    tabs: &mut HashMap<i64, OwnedTab>,
    tab_id: i64,
    dark: bool,
) -> Result<TabSnapshot, String> {
    let tab = tabs
        .get_mut(&tab_id)
        .ok_or_else(|| format!("missing tab_id={tab_id}"))?;
    tab.view.set_color_scheme(dark)?;
    tab.dark = dark;
    eprintln!(
        "[Ladybird] engine SetColorScheme applied tab_id={} dark={}",
        tab.id, tab.dark
    );
    Ok(snapshot_from_tab(tab))
}

fn set_gui_active(
    tabs: &mut HashMap<i64, OwnedTab>,
    tab_id: i64,
    active: bool,
) -> Result<StateOutcome, String> {
    if tab_id == 0 {
        let mut snapshots = Vec::new();
        for tab in tabs.values_mut() {
            tab.view.set_gui_active(active)?;
            tab.gui_active = active;
            snapshots.push(snapshot_from_tab(tab));
        }
        eprintln!(
            "[Ladybird] engine SetGuiActive applied tab_id=0 active={} target_count={}",
            active,
            snapshots.len()
        );
        return Ok(StateOutcome {
            affected_count: snapshots.len(),
            snapshots,
        });
    }

    let tab = tabs
        .get_mut(&tab_id)
        .ok_or_else(|| format!("missing tab_id={tab_id}"))?;
    tab.view.set_gui_active(active)?;
    tab.gui_active = active;
    eprintln!(
        "[Ladybird] engine SetGuiActive applied tab_id={} active={} target_count=1",
        tab.id, tab.gui_active
    );
    Ok(StateOutcome {
        affected_count: 1,
        snapshots: vec![snapshot_from_tab(tab)],
    })
}

fn snapshot_from_tab(tab: &OwnedTab) -> TabSnapshot {
    TabSnapshot {
        id: tab.id,
        inspected_tab_id: tab.inspected_tab_id,
        pane_id: tab.pane_id.clone(),
        url: tab.target_url.clone(),
        width: tab.width,
        height: tab.height,
        focused: tab.focused,
        dark: tab.dark,
        gui_active: tab.gui_active,
        last_title: tab.last_title.clone(),
        finished: tab.finished,
        crashed: tab.crashed,
        can_go_back: tab.can_go_back,
        can_go_forward: tab.can_go_forward,
        can_refresh: tab.can_refresh,
        mouse_button_events: tab.mouse_button_events,
        mouse_move_events: tab.mouse_move_events,
        scroll_events: tab.scroll_events,
        key_events: tab.key_events,
        console_message_count: tab.console_message_count,
        last_console_message: tab.last_console_message.clone(),
        cursor_change_count: tab.cursor_change_count,
        last_cursor_type: tab.last_cursor_type,
        target_url_change_count: tab.target_url_change_count,
        last_target_url: tab.last_target_url.clone(),
        last_non_empty_target_url: tab.last_non_empty_target_url.clone(),
        javascript_dialog_request_count: tab.javascript_dialog_request_count,
        javascript_dialog_reply_count: tab.javascript_dialog_reply_count,
        last_javascript_dialog_request_id: tab.last_javascript_dialog_request_id,
        last_javascript_dialog_reply_request_id: tab.last_javascript_dialog_reply_request_id,
        last_javascript_dialog_type: tab.last_javascript_dialog_type.clone(),
        last_javascript_dialog_message: tab.last_javascript_dialog_message.clone(),
        last_javascript_dialog_default_prompt_text: tab
            .last_javascript_dialog_default_prompt_text
            .clone(),
        last_javascript_dialog_origin_url: tab.last_javascript_dialog_origin_url.clone(),
        last_javascript_dialog_reply_accepted: tab.last_javascript_dialog_reply_accepted,
        last_javascript_dialog_reply_ok: tab.last_javascript_dialog_reply_ok,
    }
}

fn publish_title_changes(tabs: &mut HashMap<i64, OwnedTab>) {
    for tab in tabs.values_mut() {
        let title = match tab.view.take_title_changed() {
            Ok(Some(title)) => title,
            Ok(None) => continue,
            Err(error) => {
                eprintln!(
                    "[Ladybird] engine title change poll failed tab_id={} error={error}",
                    tab.id
                );
                continue;
            }
        };

        tab.last_title = title;
        publish_title_changed(tab.id, &tab.last_title);
    }
}

fn publish_console_messages(tabs: &mut HashMap<i64, OwnedTab>) {
    for tab in tabs.values_mut() {
        loop {
            let message = match tab.view.take_console_message() {
                Ok(Some(message)) => message,
                Ok(None) => break,
                Err(error) => {
                    eprintln!(
                        "[Ladybird] engine console message poll failed tab_id={} error={error}",
                        tab.id
                    );
                    break;
                }
            };

            tab.console_message_count += 1;
            tab.last_console_message = message.message.clone();
            publish_console_message(tab.id, &message);
        }
    }
}

fn publish_javascript_dialog_requests(tabs: &mut HashMap<i64, OwnedTab>) {
    for tab in tabs.values_mut() {
        loop {
            let request = match tab.view.take_javascript_dialog_request() {
                Ok(Some(request)) => request,
                Ok(None) => break,
                Err(error) => {
                    eprintln!(
                        "[Ladybird] engine JavaScript dialog request poll failed tab_id={} error={error}",
                        tab.id
                    );
                    break;
                }
            };

            tab.javascript_dialog_request_count += 1;
            tab.last_javascript_dialog_request_id = request.request_id;
            tab.last_javascript_dialog_type = request.dialog_type.clone();
            tab.last_javascript_dialog_message = request.message.clone();
            tab.last_javascript_dialog_default_prompt_text = request.default_prompt_text.clone();
            tab.last_javascript_dialog_origin_url = request.origin_url.clone();
            publish_javascript_dialog_request(tab.id, &request);
        }
    }
}

fn publish_hover_changes(tabs: &mut HashMap<i64, OwnedTab>) {
    for tab in tabs.values_mut() {
        loop {
            let cursor_type = match tab.view.take_cursor_changed() {
                Ok(Some(cursor_type)) => cursor_type,
                Ok(None) => break,
                Err(error) => {
                    eprintln!(
                        "[Ladybird] engine cursor change poll failed tab_id={} error={error}",
                        tab.id
                    );
                    break;
                }
            };

            tab.cursor_change_count += 1;
            tab.last_cursor_type = cursor_type;
            publish_cursor_changed(tab.id, cursor_type);
        }

        loop {
            let target_url = match tab.view.take_target_url_changed() {
                Ok(Some(target_url)) => target_url,
                Ok(None) => break,
                Err(error) => {
                    eprintln!(
                        "[Ladybird] engine target URL poll failed tab_id={} error={error}",
                        tab.id
                    );
                    break;
                }
            };

            tab.target_url_change_count += 1;
            tab.last_target_url = target_url;
            if !tab.last_target_url.is_empty() {
                tab.last_non_empty_target_url = tab.last_target_url.clone();
            }
            publish_target_url_changed(tab.id, &tab.last_target_url);
        }
    }
}

fn publish_renderer_crashes(tabs: &mut HashMap<i64, OwnedTab>) {
    for tab in tabs.values_mut() {
        loop {
            let crash = match tab.view.take_renderer_crashed() {
                Ok(Some(crash)) => crash,
                Ok(None) => break,
                Err(error) => {
                    eprintln!(
                        "[Ladybird] engine renderer crash poll failed tab_id={} error={error}",
                        tab.id
                    );
                    break;
                }
            };

            tab.in_flight = false;
            tab.finished = false;
            tab.crashed = true;
            tab.can_go_back = false;
            tab.can_go_forward = false;
            tab.can_refresh = crash.can_reload;
            publish_navigation_state(tab.id, false, false, tab.can_refresh);
            if tab.refresh_request_id != 0 {
                publish_loading_state(tab.id, "error", 0, tab.refresh_request_id);
                tab.refresh_request_id = 0;
            }
            publish_renderer_crashed(tab.id, &crash);
        }
    }
}

fn publish_finished_loads(tabs: &mut HashMap<i64, OwnedTab>, events: &mpsc::Sender<Event>) {
    for (tab_id, tab) in tabs.iter_mut() {
        if tab.view.did_crash() {
            if !tab.crashed {
                tab.in_flight = false;
                tab.crashed = true;
                tab.can_go_back = false;
                tab.can_go_forward = false;
                tab.can_refresh = tab
                    .view
                    .navigation_state()
                    .map(|state| state.can_refresh)
                    .unwrap_or(false);
                publish_navigation_state(tab.id, false, false, tab.can_refresh);
                if tab.refresh_request_id != 0 {
                    publish_loading_state(tab.id, "error", 0, tab.refresh_request_id);
                    tab.refresh_request_id = 0;
                }
                let _ = events.send(Event::Failed {
                    message: format!("view crashed tab_id={tab_id}"),
                });
            }
            continue;
        }
        if tab.in_flight && tab.view.did_finish_load() {
            tab.in_flight = false;
            tab.finished = true;
            tab.crashed = false;
            tab.last_url = tab.view.last_url();
            eprintln!(
                "[Ladybird] engine load finished tab_id={} url={}",
                tab.id, tab.last_url
            );
            let _ = export_and_publish_render_surface(tab);
            publish_loading_state(tab.id, "done", 100, tab.refresh_request_id);
            tab.refresh_request_id = 0;
            let _ = events.send(Event::LoadFinished {
                tab_id: *tab_id,
                url: tab.last_url.clone(),
            });
        }
    }
}

fn observe_navigation_changes(tabs: &mut HashMap<i64, OwnedTab>) {
    for tab in tabs.values_mut() {
        let live_url = tab.view.last_url();
        let (native_can_go_back, native_can_go_forward, native_can_refresh) =
            match tab.view.navigation_state() {
                Ok(state) => (state.can_go_back, state.can_go_forward, state.can_refresh),
                Err(error) => {
                    eprintln!(
                        "[Ladybird] engine NavigationState poll failed tab_id={} error={error}",
                        tab.id
                    );
                    (tab.can_go_back, tab.can_go_forward, tab.can_refresh)
                }
            };
        let (url_changed, state_changed) = navigation_observation(
            &tab.last_url,
            tab.can_go_back,
            tab.can_go_forward,
            tab.crashed,
            &live_url,
            native_can_go_back,
            native_can_go_forward,
        );
        if let Some(url) = url_changed {
            tab.last_url = url;
            publish_url_changed(tab.id, &tab.last_url);
        }
        let history_changed = state_changed.is_some();
        if let Some((can_go_back, can_go_forward)) = state_changed {
            tab.can_go_back = can_go_back;
            tab.can_go_forward = can_go_forward;
        }
        let refresh_changed = native_can_refresh != tab.can_refresh;
        if refresh_changed {
            tab.can_refresh = native_can_refresh;
        }
        if history_changed || refresh_changed {
            publish_navigation_state(tab.id, tab.can_go_back, tab.can_go_forward, tab.can_refresh);
        }
    }
}

fn navigation_observation(
    last_url: &str,
    last_can_go_back: bool,
    last_can_go_forward: bool,
    crashed: bool,
    live_url: &str,
    native_can_go_back: bool,
    native_can_go_forward: bool,
) -> (Option<String>, Option<(bool, bool)>) {
    if crashed {
        return (
            None,
            (last_can_go_back || last_can_go_forward).then_some((false, false)),
        );
    }
    let url_changed = (!live_url.is_empty() && live_url != last_url).then(|| live_url.to_string());
    let state_changed = (native_can_go_back != last_can_go_back
        || native_can_go_forward != last_can_go_forward)
        .then_some((native_can_go_back, native_can_go_forward));
    (url_changed, state_changed)
}

fn tab_needs_render_surface_retry(tab: &OwnedTab) -> bool {
    tab.finished
        && !tab.crashed
        && !tab.render_surface_sent
        && tab.render_surface_attempts < RENDER_SURFACE_RETRY_LIMIT
}

fn publish_pending_render_surfaces(tabs: &mut HashMap<i64, OwnedTab>) {
    for tab in tabs
        .values_mut()
        .filter(|tab| tab_needs_render_surface_retry(tab))
    {
        let _ = export_and_publish_render_surface(tab);
    }
}

fn renderer_crashed_message(tab_id: i64, crash: &AbiRendererCrash) -> TermSurfMessage {
    TermSurfMessage {
        msg: Some(Msg::RendererCrashed(termsurf::RendererCrashed {
            tab_id,
            termination_status: crash.termination_status.clone(),
            termination_status_code: crash.termination_status_code,
            url: crash.url.clone(),
            can_reload: crash.can_reload,
        })),
    }
}

fn publish_renderer_crashed(tab_id: i64, crash: &AbiRendererCrash) {
    let msg = renderer_crashed_message(tab_id, crash);
    let sent = crate::ipc::send(&msg);
    eprintln!(
        "[Ladybird] engine RendererCrashed sent_to={sent} tab_id={tab_id} status={} code={} url={} can_reload={} mode=crash-callback",
        crash.termination_status, crash.termination_status_code, crash.url, crash.can_reload
    );
}

fn navigation_state_message(
    tab_id: i64,
    can_go_back: bool,
    can_go_forward: bool,
    can_refresh: bool,
) -> TermSurfMessage {
    TermSurfMessage {
        msg: Some(Msg::NavigationState(termsurf::NavigationState {
            tab_id,
            can_go_back,
            can_go_forward,
            can_refresh,
        })),
    }
}

fn publish_navigation_state(
    tab_id: i64,
    can_go_back: bool,
    can_go_forward: bool,
    can_refresh: bool,
) {
    let msg = navigation_state_message(tab_id, can_go_back, can_go_forward, can_refresh);
    let sent = crate::ipc::send(&msg);
    eprintln!(
        "[Ladybird] engine NavigationState sent_to={sent} tab_id={tab_id} can_go_back={can_go_back} can_go_forward={can_go_forward} can_refresh={can_refresh}"
    );
}

fn publish_cursor_changed(tab_id: i64, cursor_type: i32) {
    let msg = TermSurfMessage {
        msg: Some(Msg::CursorChanged(termsurf::CursorChanged {
            tab_id,
            cursor_type: cursor_type as i64,
        })),
    };
    let sent = crate::ipc::send(&msg);
    eprintln!(
        "[Ladybird] engine CursorChanged sent_to={sent} tab_id={tab_id} cursor_type={cursor_type} mode=hover-callback"
    );
}

fn publish_target_url_changed(tab_id: i64, url: &str) {
    let msg = TermSurfMessage {
        msg: Some(Msg::TargetUrlChanged(termsurf::TargetUrlChanged {
            tab_id,
            url: url.to_string(),
        })),
    };
    let sent = crate::ipc::send(&msg);
    eprintln!(
        "[Ladybird] engine TargetUrlChanged sent_to={sent} tab_id={tab_id} url={url} mode=hover-callback"
    );
}

fn publish_javascript_dialog_request(tab_id: i64, request: &AbiJavaScriptDialogRequest) {
    let msg = TermSurfMessage {
        msg: Some(Msg::JavascriptDialogRequest(
            termsurf::JavaScriptDialogRequest {
                tab_id,
                request_id: request.request_id,
                dialog_type: request.dialog_type.clone(),
                origin_url: request.origin_url.clone(),
                message: request.message.clone(),
                default_prompt_text: request.default_prompt_text.clone(),
            },
        )),
    };
    let sent = crate::ipc::send(&msg);
    eprintln!(
        "[Ladybird] engine JavaScriptDialogRequest sent_to={sent} tab_id={tab_id} request_id={} type={} origin={} message={} default={} mode=dialog-callback",
        request.request_id,
        request.dialog_type,
        request.origin_url,
        request.message,
        request.default_prompt_text
    );
}

fn publish_console_message(tab_id: i64, message: &AbiConsoleMessage) {
    let msg = TermSurfMessage {
        msg: Some(Msg::ConsoleMessage(termsurf::ConsoleMessage {
            tab_id,
            level: message.level.clone(),
            message: message.message.clone(),
            line_no: message.line_no,
            source_id: message.source_id.clone(),
        })),
    };
    let sent = crate::ipc::send(&msg);
    eprintln!(
        "[Ladybird] engine ConsoleMessage sent_to={sent} tab_id={tab_id} level={} line_no={} source={} message={} mode=console-callback",
        message.level, message.line_no, message.source_id, message.message
    );
}

fn publish_tab_ready(tab: &TabSnapshot) {
    let msg = TermSurfMessage {
        msg: Some(Msg::TabReady(termsurf::TabReady {
            pane_id: tab.pane_id.clone(),
            tab_id: tab.id,
        })),
    };
    let sent = crate::ipc::send(&msg);
    eprintln!(
        "[Ladybird] engine TabReady sent_to={} tab_id={} pane_id={}",
        sent, tab.id, tab.pane_id
    );
}

fn render_surface_message(
    tab_id: i64,
    metadata: crate::render_channel::SentSurfaceMetadata,
) -> TermSurfMessage {
    TermSurfMessage {
        msg: Some(Msg::RenderSurface(termsurf::RenderSurface {
            tab_id,
            pixel_width: metadata.pixel_width,
            pixel_height: metadata.pixel_height,
            bytes_per_row: metadata.bytes_per_row,
            pixel_format: metadata.pixel_format,
            generation: metadata.generation,
            attachment_id: metadata.attachment_id,
        })),
    }
}

fn export_and_publish_render_surface(tab: &mut OwnedTab) -> bool {
    if tab.render_surface_sent {
        return true;
    }
    tab.render_surface_attempts += 1;

    let attempt = tab.render_surface_attempts;
    let should_log_retry = attempt == 1 || attempt % 100 == 0;

    if attempt > RENDER_SURFACE_RETRY_LIMIT {
        eprintln!(
            "[Ladybird] engine render surface retry exhausted tab_id={} attempts={}",
            tab.id, RENDER_SURFACE_RETRY_LIMIT
        );
        return false;
    }
    let exported = match tab.view.export_render_surface() {
        Ok(exported) => exported,
        Err(error) => {
            eprintln!(
                "[Ladybird] engine render surface export failed tab_id={} error={error}",
                tab.id
            );
            return false;
        }
    };
    if !exported.has_surface
        || exported.surface_port == 0
        || exported.pixel_width == 0
        || exported.pixel_height == 0
    {
        if should_log_retry || attempt == RENDER_SURFACE_RETRY_LIMIT {
            eprintln!(
                "[Ladybird] engine render surface export pending tab_id={} attempt={} has_surface={} surface_port={} pixel={}x{}",
                tab.id,
                attempt,
                exported.has_surface,
                exported.surface_port,
                exported.pixel_width,
                exported.pixel_height
            );
        }
        if attempt == RENDER_SURFACE_RETRY_LIMIT {
            eprintln!(
                "[Ladybird] engine render surface retry exhausted tab_id={} attempts={}",
                tab.id, RENDER_SURFACE_RETRY_LIMIT
            );
        }
        return false;
    }
    let Some(metadata) = crate::render_channel::send_exported_surface_global(exported) else {
        if should_log_retry || attempt == RENDER_SURFACE_RETRY_LIMIT {
            eprintln!(
                "[Ladybird] engine render surface send skipped tab_id={} attempt={} has_surface={} surface_port={} pixel={}x{}",
                tab.id,
                attempt,
                exported.has_surface,
                exported.surface_port,
                exported.pixel_width,
                exported.pixel_height
            );
        }
        if attempt == RENDER_SURFACE_RETRY_LIMIT {
            eprintln!(
                "[Ladybird] engine render surface retry exhausted tab_id={} attempts={}",
                tab.id, RENDER_SURFACE_RETRY_LIMIT
            );
        }
        return false;
    };

    let msg = render_surface_message(tab.id, metadata);
    let sent = crate::ipc::send(&msg);
    tab.render_surface_sent = sent > 0;
    eprintln!(
        "[Ladybird] engine RenderSurface metadata sent_to={sent} tab_id={} generation={} attachment_id={}",
        tab.id, metadata.generation, metadata.attachment_id
    );
    tab.render_surface_sent
}

fn publish_url_changed(tab_id: i64, url: &str) {
    let msg = TermSurfMessage {
        msg: Some(Msg::UrlChanged(termsurf::UrlChanged {
            tab_id,
            url: url.to_string(),
        })),
    };
    let sent = crate::ipc::send(&msg);
    eprintln!("[Ladybird] engine UrlChanged sent_to={sent} tab_id={tab_id} url={url}");
}

fn publish_loading_state(tab_id: i64, state: &str, progress: u64, navigation_request_id: u64) {
    let msg = TermSurfMessage {
        msg: Some(Msg::LoadingState(termsurf::LoadingState {
            tab_id,
            state: state.to_string(),
            progress,
            navigation_request_id,
        })),
    };
    let sent = crate::ipc::send(&msg);
    eprintln!(
        "[Ladybird] engine LoadingState sent_to={sent} tab_id={tab_id} state={state} progress={progress}"
    );
}

fn publish_title_changed(tab_id: i64, title: &str) {
    let msg = TermSurfMessage {
        msg: Some(Msg::TitleChanged(termsurf::TitleChanged {
            tab_id,
            title: title.to_string(),
        })),
    };
    let sent = crate::ipc::send(&msg);
    eprintln!(
        "[Ladybird] engine TitleChanged sent_to={sent} tab_id={tab_id} title={title} mode=title-callback"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_real_render_surface_metadata() {
        let msg = render_surface_message(
            77,
            crate::render_channel::SentSurfaceMetadata {
                pixel_width: 16,
                pixel_height: 16,
                bytes_per_row: 128,
                pixel_format: 0x42475241,
                generation: 1,
                attachment_id: 1,
            },
        );
        let Some(Msg::RenderSurface(surface)) = msg.msg else {
            panic!("expected RenderSurface metadata message");
        };
        assert_eq!(surface.tab_id, 77);
        assert_eq!(surface.pixel_width, 16);
        assert_eq!(surface.pixel_height, 16);
        assert_eq!(surface.bytes_per_row, 128);
        assert_eq!(surface.pixel_format, 0x42475241);
        assert_eq!(surface.generation, 1);
        assert_eq!(surface.attachment_id, 1);
    }

    #[test]
    fn builds_renderer_crashed_message() {
        let msg = renderer_crashed_message(
            55,
            &AbiRendererCrash {
                termination_status: "crashed".to_string(),
                termination_status_code: 0,
                url: "data:text/html,<title>Crash</title>".to_string(),
                can_reload: true,
            },
        );
        let Some(Msg::RendererCrashed(crash)) = msg.msg else {
            panic!("expected RendererCrashed message");
        };
        assert_eq!(crash.tab_id, 55);
        assert_eq!(crash.termination_status, "crashed");
        assert_eq!(crash.termination_status_code, 0);
        assert_eq!(crash.url, "data:text/html,<title>Crash</title>");
        assert!(crash.can_reload);
    }

    #[test]
    fn builds_tab_addressed_navigation_state_message() {
        let message = navigation_state_message(42, true, false, true);
        let Some(Msg::NavigationState(state)) = message.msg else {
            panic!("expected NavigationState message");
        };
        assert_eq!(state.tab_id, 42);
        assert!(state.can_go_back);
        assert!(!state.can_go_forward);

        for tab_id in [1, 42, 99] {
            let message = navigation_state_message(tab_id, false, false, false);
            let Some(Msg::NavigationState(state)) = message.msg else {
                panic!("expected fail-closed NavigationState message");
            };
            assert_eq!(state.tab_id, tab_id);
            assert!(!state.can_go_back);
            assert!(!state.can_go_forward);
        }
    }

    #[test]
    fn rejects_navigation_before_ffi_when_disabled_crashed_or_future() {
        assert!(validate_navigation_action(7, "back", true, false, false, false).is_ok());
        assert!(validate_navigation_action(7, "forward", false, true, false, false).is_ok());
        assert!(validate_navigation_action(7, "refresh", false, false, true, false).is_ok());
        assert!(validate_navigation_action(7, "refresh", false, false, true, true).is_ok());
        assert!(validate_navigation_action(7, "back", false, true, false, false).is_err());
        assert!(validate_navigation_action(7, "forward", true, false, false, false).is_err());
        assert!(validate_navigation_action(7, "back", true, true, true, true).is_err());
        assert!(validate_navigation_action(7, "refresh", true, true, false, false).is_err());
        assert!(validate_navigation_action(7, "future", true, true, true, false).is_err());
    }

    #[test]
    fn observes_browser_initiated_back_without_explicit_navigate() {
        let (url, state) = navigation_observation(
            "https://fixture.test/a2",
            true,
            false,
            false,
            "https://fixture.test/a1",
            false,
            true,
        );
        assert_eq!(url.as_deref(), Some("https://fixture.test/a1"));
        assert_eq!(state, Some((false, true)));
    }

    #[test]
    fn crash_latch_forces_false_and_ignores_late_native_true() {
        let (url, state) = navigation_observation(
            "https://fixture.test/a2",
            true,
            true,
            true,
            "https://fixture.test/a1",
            true,
            true,
        );
        assert_eq!(url, None);
        assert_eq!(state, Some((false, false)));
    }
}
