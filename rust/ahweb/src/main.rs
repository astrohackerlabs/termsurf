mod ipc;

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
    MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use edtui::actions::{Execute, SelectLine, SwitchMode};
use edtui::clipboard::ClipboardTrait;
use edtui::events::{KeyEventHandler, KeyEventRegister, KeyInput};
use edtui::{
    EditorEventHandler, EditorMode, EditorState, EditorTheme, EditorView, Lines, RowIndex,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

// Tokyo Night palette.
const BG: Color = Color::Rgb(0x1a, 0x1b, 0x26);
const FG: Color = Color::Rgb(0xc0, 0xca, 0xf5);
const COMMENT: Color = Color::Rgb(0x73, 0x7a, 0xa2);
const CYAN: Color = Color::Rgb(0x7d, 0xcf, 0xff);
const BORDER: Color = Color::Rgb(0x56, 0x5f, 0x89);
const DIM: Color = Color::Rgb(0x90, 0x9a, 0xb8);
const SELECTION: Color = Color::Rgb(0x28, 0x34, 0x57);
const PURPLE: Color = Color::Rgb(0xbb, 0x9a, 0xf7);
const YELLOW: Color = Color::Rgb(0xe0, 0xaf, 0x68);
const BLUE: Color = Color::Rgb(0x7a, 0xa2, 0xf7);
const GREEN: Color = Color::Rgb(0x9e, 0xce, 0x6a);
const RED: Color = Color::Rgb(0xf7, 0x76, 0x8e);

fn submode_color(mode: &EditorMode) -> Color {
    match mode {
        EditorMode::Normal => BLUE,
        EditorMode::Insert => GREEN,
        EditorMode::Visual => PURPLE,
        EditorMode::Search => YELLOW,
    }
}

#[derive(Clone, PartialEq, Debug)]
enum Mode {
    Browse,
    Control,
    Edit,
    Command,
    Dialog,
    Auth,
}

/// Initial UI mode at process start (Issue 26071922533901 Exp 1).
/// Browse = content-first keys to page; reverses Issue 649 Control default
/// after Ghostty Browse chrome allowlist (ahcalc Exp 7). Esc still → Control.
fn initial_mode() -> Mode {
    Mode::Browse
}

/// Browsing flag sent on SetOverlay / ModeChanged (host browse-forward).
fn mode_is_browsing(mode: &Mode) -> bool {
    matches!(mode, Mode::Browse)
}

#[derive(Clone)]
struct PendingJsDialog {
    tab_id: i64,
    request_id: u64,
    dialog_type: String,
    origin_url: String,
    message: String,
    default_prompt_text: String,
    input: String,
    previous_mode: Mode,
}

#[derive(Clone, PartialEq)]
enum AuthField {
    Username,
    Password,
}

#[derive(Clone)]
struct PendingHttpAuth {
    tab_id: i64,
    request_id: u64,
    url: String,
    auth_scheme: String,
    challenger: String,
    realm: String,
    is_proxy: bool,
    first_auth_attempt: bool,
    username: String,
    password: String,
    field: AuthField,
    previous_mode: Mode,
}

struct StateTrace {
    file: File,
}

impl StateTrace {
    fn from_env() -> Option<Self> {
        let path = std::env::var_os("TERMSURF_WEBTUI_STATE_TRACE_FILE")?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()?;
        Some(Self { file })
    }

    fn write(&mut self, event: &str, fields: &[(&str, String)]) {
        let _ = write!(self.file, "event={}", trace_field(event));
        for (key, value) in fields {
            let _ = write!(self.file, "\t{}={}", trace_field(key), trace_field(value));
        }
        let _ = writeln!(self.file);
        let _ = self.file.flush();
    }
}

fn trace_field(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

fn trace_rect(rect: Rect) -> String {
    format!("{},{},{},{}", rect.x, rect.y, rect.width, rect.height)
}

#[derive(Clone)]
struct ConsoleLogEntry {
    tab_id: i64,
    level: String,
    message: String,
    line_no: i32,
    source_id: String,
}

#[derive(Clone)]
struct RendererCrashState {
    tab_id: i64,
    termination_status: String,
    termination_status_code: i32,
    url: String,
    can_reload: bool,
}

// Loading screen stages (Issue 26040512000773).
#[derive(Clone)]
enum LoadingStage {
    ConnectingToGui,
    StartingBrowser,
    WaitingForBrowser,
    LoadingPage,
    Ready,
}

#[derive(Clone)]
enum StageStatus {
    InProgress,
    Done,
    Error(String),
}

impl LoadingStage {
    fn label(&self) -> &'static str {
        match self {
            LoadingStage::ConnectingToGui => "Connected to GUI",
            LoadingStage::StartingBrowser => "Starting browser engine",
            LoadingStage::WaitingForBrowser => "Waiting for browser",
            LoadingStage::LoadingPage => "Loading page",
            LoadingStage::Ready => "Ready",
        }
    }
}

enum LoopEvent {
    Terminal(Event),
    Ipc(ipc::CompositorMessage),
}

const BACK_SYMBOL: &str = "←";
const FORWARD_SYMBOL: &str = "→";
const REFRESH_IDLE_SYMBOL: &str = "\u{E348}";
const REFRESH_ANIMATION_FRAMES: [&str; 4] = ["⟳", "↻", "↺", "⟲"];
const ENABLE_ANY_MOUSE_MOTION: &str = "\x1b[?1003h";
const DISABLE_ANY_MOUSE_MOTION: &str = "\x1b[?1003l";

#[derive(Clone, Debug, PartialEq, Eq)]
enum BackRoute {
    Compositor(String),
    Direct(i64),
}

impl BackRoute {
    fn label(&self) -> &'static str {
        match self {
            Self::Compositor(_) => "compositor",
            Self::Direct(_) => "direct-browser",
        }
    }
}

fn current_back_route(
    current_tab_id: i64,
    compositor_available: bool,
    pane_id: Option<&str>,
    direct_tab_id: Option<i64>,
) -> Option<BackRoute> {
    if current_tab_id <= 0 {
        return None;
    }
    if compositor_available {
        if let Some(pane_id) = pane_id.filter(|pane_id| !pane_id.is_empty()) {
            return Some(BackRoute::Compositor(pane_id.to_string()));
        }
    }
    direct_tab_id
        .filter(|tab_id| *tab_id > 0 && *tab_id == current_tab_id)
        .map(BackRoute::Direct)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BackPress {
    tab_id: i64,
    route: BackRoute,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct BackControlState {
    active_tab_id: i64,
    can_go_back: bool,
    hovered: bool,
    pressed: Option<BackPress>,
}

impl BackControlState {
    fn browser_ready(&mut self, tab_id: i64) {
        self.active_tab_id = tab_id;
        self.can_go_back = false;
        self.clear_interaction();
    }

    fn apply_navigation_state(&mut self, tab_id: i64, can_go_back: bool) -> bool {
        if tab_id <= 0 || tab_id != self.active_tab_id {
            return false;
        }
        self.can_go_back = can_go_back;
        if !can_go_back {
            self.clear_interaction();
        }
        true
    }

    fn renderer_crashed(&mut self, tab_id: i64) -> bool {
        if tab_id <= 0 || tab_id != self.active_tab_id {
            return false;
        }
        self.can_go_back = false;
        self.clear_interaction();
        true
    }

    fn clear_interaction(&mut self) {
        self.hovered = false;
        self.pressed = None;
    }

    fn reconcile_route(&mut self, route: Option<&BackRoute>) -> bool {
        let valid_press = self
            .pressed
            .as_ref()
            .map(|press| {
                press.tab_id == self.active_tab_id
                    && route.map(|route| route == &press.route).unwrap_or(false)
            })
            .unwrap_or(true);
        if route.is_none() || !valid_press {
            let changed = self.hovered || self.pressed.is_some();
            self.clear_interaction();
            changed
        } else {
            false
        }
    }

    fn actionable(&self, route: Option<&BackRoute>) -> bool {
        self.active_tab_id > 0 && self.can_go_back && route.is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ForwardPress {
    tab_id: i64,
    route: BackRoute,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ForwardControlState {
    active_tab_id: i64,
    can_go_forward: bool,
    hovered: bool,
    pressed: Option<ForwardPress>,
}

impl ForwardControlState {
    fn browser_ready(&mut self, tab_id: i64) {
        self.active_tab_id = tab_id;
        self.can_go_forward = false;
        self.clear_interaction();
    }

    fn apply_navigation_state(&mut self, tab_id: i64, can_go_forward: bool) -> bool {
        if tab_id <= 0 || tab_id != self.active_tab_id {
            return false;
        }
        self.can_go_forward = can_go_forward;
        if !can_go_forward {
            self.clear_interaction();
        }
        true
    }

    fn renderer_crashed(&mut self, tab_id: i64) -> bool {
        if tab_id <= 0 || tab_id != self.active_tab_id {
            return false;
        }
        self.can_go_forward = false;
        self.clear_interaction();
        true
    }

    fn clear_interaction(&mut self) {
        self.hovered = false;
        self.pressed = None;
    }

    fn reconcile_route(&mut self, route: Option<&BackRoute>) -> bool {
        let valid_press = self
            .pressed
            .as_ref()
            .map(|press| {
                press.tab_id == self.active_tab_id
                    && route.map(|route| route == &press.route).unwrap_or(false)
            })
            .unwrap_or(true);
        if route.is_none() || !valid_press {
            let changed = self.hovered || self.pressed.is_some();
            self.clear_interaction();
            changed
        } else {
            false
        }
    }

    fn actionable(&self, route: Option<&BackRoute>) -> bool {
        self.active_tab_id > 0 && self.can_go_forward && route.is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RefreshPress {
    tab_id: i64,
    route: BackRoute,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RefreshControlState {
    active_tab_id: i64,
    can_refresh: bool,
    hovered: bool,
    pressed: Option<RefreshPress>,
}

impl RefreshControlState {
    fn browser_ready(&mut self, tab_id: i64) {
        self.active_tab_id = tab_id;
        self.can_refresh = false;
        self.clear_interaction();
    }

    fn apply_navigation_state(&mut self, tab_id: i64, can_refresh: bool) -> bool {
        if tab_id <= 0 || tab_id != self.active_tab_id {
            return false;
        }
        self.can_refresh = can_refresh;
        if !can_refresh {
            self.clear_interaction();
        }
        true
    }

    fn renderer_crashed(&mut self, tab_id: i64) -> bool {
        if tab_id <= 0 || tab_id != self.active_tab_id {
            return false;
        }
        self.clear_interaction();
        true
    }

    fn clear_interaction(&mut self) {
        self.hovered = false;
        self.pressed = None;
    }

    fn reconcile_route(&mut self, route: Option<&BackRoute>) -> bool {
        let valid_press = self
            .pressed
            .as_ref()
            .map(|press| {
                press.tab_id == self.active_tab_id
                    && route.map(|route| route == &press.route).unwrap_or(false)
            })
            .unwrap_or(true);
        if route.is_none() || !valid_press {
            let changed = self.hovered || self.pressed.is_some();
            self.clear_interaction();
            changed
        } else {
            false
        }
    }

    fn actionable(&self, route: Option<&BackRoute>) -> bool {
        self.active_tab_id > 0 && self.can_refresh && route.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RefreshAnimation {
    tab_id: i64,
    request_id: u64,
    started_at: Option<Instant>,
    completed_at: Option<Instant>,
}

impl RefreshAnimation {
    const FRAME_DURATION: Duration = Duration::from_millis(120);
    const MIN_VISIBLE: Duration = Duration::from_millis(240);
    const TIMEOUT: Duration = Duration::from_secs(4);

    fn start(&mut self, tab_id: i64, request_id: u64, now: Instant) -> bool {
        if tab_id <= 0 || request_id == 0 {
            return false;
        }
        *self = Self {
            tab_id,
            request_id,
            started_at: Some(now),
            completed_at: None,
        };
        true
    }

    fn complete(&mut self, tab_id: i64, request_id: u64, now: Instant) -> bool {
        if self.started_at.is_none() || self.tab_id != tab_id || self.request_id != request_id {
            return false;
        }
        self.completed_at = Some(now);
        true
    }

    fn stop(&mut self) {
        *self = Self::default();
    }

    fn tick(&mut self, now: Instant) -> bool {
        let Some(started_at) = self.started_at else {
            return false;
        };
        if now.duration_since(started_at) >= Self::TIMEOUT
            || (self.completed_at.is_some() && now.duration_since(started_at) >= Self::MIN_VISIBLE)
        {
            self.stop();
            return true;
        }
        false
    }

    fn active(&self) -> bool {
        self.started_at.is_some()
    }

    fn frame(&self, now: Instant) -> usize {
        self.started_at
            .map(|started_at| {
                (now.duration_since(started_at).as_millis() / Self::FRAME_DURATION.as_millis())
                    as usize
                    % REFRESH_ANIMATION_FRAMES.len()
            })
            .unwrap_or(0)
    }
}

fn refresh_symbol(animation: &RefreshAnimation, now: Instant) -> &'static str {
    if animation.active() {
        REFRESH_ANIMATION_FRAMES[animation.frame(now)]
    } else {
        REFRESH_IDLE_SYMBOL
    }
}

fn reset_back_for_browser_ready<T>(
    state: &mut BackControlState,
    direct_connection: &mut Option<T>,
    tab_id: i64,
) {
    state.browser_ready(tab_id);
    *direct_connection = None;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BackMouseResult {
    changed: bool,
    activate: bool,
}

type ForwardMouseResult = BackMouseResult;
type RefreshMouseResult = BackMouseResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BackVisualState {
    actionable: bool,
    hovered: bool,
    pressed: bool,
}

fn back_visual_state(state: &BackControlState, route: Option<&BackRoute>) -> BackVisualState {
    let actionable = state.actionable(route);
    BackVisualState {
        actionable,
        hovered: actionable && state.hovered,
        pressed: actionable && state.pressed.is_some(),
    }
}

fn forward_visual_state(state: &ForwardControlState, route: Option<&BackRoute>) -> BackVisualState {
    let actionable = state.actionable(route);
    BackVisualState {
        actionable,
        hovered: actionable && state.hovered,
        pressed: actionable && state.pressed.is_some(),
    }
}

fn refresh_visual_state(state: &RefreshControlState, route: Option<&BackRoute>) -> BackVisualState {
    let actionable = state.actionable(route);
    BackVisualState {
        actionable,
        hovered: actionable && state.hovered,
        pressed: actionable && state.pressed.is_some(),
    }
}

fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

fn update_back_mouse(
    state: &mut BackControlState,
    rect: Rect,
    route: Option<&BackRoute>,
    mouse: MouseEvent,
) -> BackMouseResult {
    let before = state.clone();
    let hit = rect_contains(rect, mouse.column, mouse.row);
    let actionable_hit = hit && state.actionable(route);
    let mut activate = false;

    match mouse.kind {
        MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left) => {
            state.hovered = actionable_hit;
            if state.pressed.is_some() && !actionable_hit {
                state.pressed = None;
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            state.hovered = actionable_hit;
            state.pressed = if actionable_hit {
                route.cloned().map(|route| BackPress {
                    tab_id: state.active_tab_id,
                    route,
                })
            } else {
                None
            };
        }
        MouseEventKind::Up(MouseButton::Left) => {
            let pressed = state.pressed.take();
            state.hovered = actionable_hit;
            activate = actionable_hit
                && pressed
                    .map(|press| {
                        press.tab_id == state.active_tab_id
                            && route.map(|route| route == &press.route).unwrap_or(false)
                    })
                    .unwrap_or(false);
        }
        _ => {}
    }

    BackMouseResult {
        changed: before != *state,
        activate,
    }
}

fn update_forward_mouse(
    state: &mut ForwardControlState,
    rect: Rect,
    route: Option<&BackRoute>,
    mouse: MouseEvent,
) -> ForwardMouseResult {
    let before = state.clone();
    let hit = rect_contains(rect, mouse.column, mouse.row);
    let actionable_hit = hit && state.actionable(route);
    let mut activate = false;

    match mouse.kind {
        MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left) => {
            state.hovered = actionable_hit;
            if state.pressed.is_some() && !actionable_hit {
                state.pressed = None;
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            state.hovered = actionable_hit;
            state.pressed = if actionable_hit {
                route.cloned().map(|route| ForwardPress {
                    tab_id: state.active_tab_id,
                    route,
                })
            } else {
                None
            };
        }
        MouseEventKind::Up(MouseButton::Left) => {
            let pressed = state.pressed.take();
            state.hovered = actionable_hit;
            activate = actionable_hit
                && pressed
                    .map(|press| {
                        press.tab_id == state.active_tab_id
                            && route.map(|route| route == &press.route).unwrap_or(false)
                    })
                    .unwrap_or(false);
        }
        _ => {}
    }

    ForwardMouseResult {
        changed: before != *state,
        activate,
    }
}

fn update_refresh_mouse(
    state: &mut RefreshControlState,
    rect: Rect,
    route: Option<&BackRoute>,
    mouse: MouseEvent,
) -> RefreshMouseResult {
    let before = state.clone();
    let hit = rect_contains(rect, mouse.column, mouse.row);
    let actionable_hit = hit && state.actionable(route);
    let mut activate = false;

    match mouse.kind {
        MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left) => {
            state.hovered = actionable_hit;
            if state.pressed.is_some() && !actionable_hit {
                state.pressed = None;
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            state.hovered = actionable_hit;
            state.pressed = if actionable_hit {
                route.cloned().map(|route| RefreshPress {
                    tab_id: state.active_tab_id,
                    route,
                })
            } else {
                None
            };
        }
        MouseEventKind::Up(MouseButton::Left) => {
            let pressed = state.pressed.take();
            state.hovered = actionable_hit;
            activate = actionable_hit
                && pressed
                    .map(|press| {
                        press.tab_id == state.active_tab_id
                            && route.map(|route| route == &press.route).unwrap_or(false)
                    })
                    .unwrap_or(false);
        }
        _ => {}
    }

    RefreshMouseResult {
        changed: before != *state,
        activate,
    }
}

fn local_back_key(mode: &Mode, key: KeyEvent) -> bool {
    matches!(mode, Mode::Control | Mode::Browse)
        && key.modifiers.contains(KeyModifiers::SUPER)
        && key.code == KeyCode::Char('[')
}

fn local_forward_key(mode: &Mode, key: KeyEvent) -> bool {
    matches!(mode, Mode::Control | Mode::Browse)
        && key.modifiers.contains(KeyModifiers::SUPER)
        && key.code == KeyCode::Char(']')
}

fn local_refresh_key(mode: &Mode, key: KeyEvent) -> bool {
    matches!(mode, Mode::Control | Mode::Browse)
        && key.modifiers.contains(KeyModifiers::SUPER)
        && matches!(key.code, KeyCode::Char('r') | KeyCode::Char('R'))
}

fn needs_event_polling(
    page_loaded: bool,
    page_loaded_at: Option<Instant>,
    copy_url_feedback_until: Option<Instant>,
    refresh_animation_active: bool,
    now: Instant,
) -> bool {
    !page_loaded
        || refresh_animation_active
        || page_loaded_at
            .map(|at| now.saturating_duration_since(at) < Duration::from_secs(2))
            .unwrap_or(false)
        || copy_url_feedback_until
            .map(|until| now < until)
            .unwrap_or(false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum BackDispatchDecision {
    Send(BackRoute),
    BlockedDisabled,
    BlockedUnavailable,
}

fn back_dispatch_decision(
    state: &BackControlState,
    route: Option<&BackRoute>,
) -> BackDispatchDecision {
    if !state.can_go_back || state.active_tab_id <= 0 {
        return BackDispatchDecision::BlockedDisabled;
    }
    match route {
        Some(route) => BackDispatchDecision::Send(route.clone()),
        None => BackDispatchDecision::BlockedUnavailable,
    }
}

fn dispatch_back(
    source: &str,
    state: &BackControlState,
    route: Option<&BackRoute>,
    compositor: &Option<ipc::CompositorConnection>,
    browser_conn: &Option<ipc::BrowserConnection>,
    state_trace: &mut Option<StateTrace>,
) -> bool {
    let decision = back_dispatch_decision(state, route);
    let (sent, route_label, blocked_reason) = match decision {
        BackDispatchDecision::Send(BackRoute::Compositor(ref pane_id)) => {
            let sent = compositor
                .as_ref()
                .map(|conn| conn.send_back(pane_id))
                .unwrap_or(false);
            (sent, "compositor", None)
        }
        BackDispatchDecision::Send(BackRoute::Direct(tab_id)) => {
            let sent = browser_conn
                .as_ref()
                .filter(|conn| conn.tab_id == tab_id && tab_id == state.active_tab_id)
                .map(|conn| conn.send_back())
                .unwrap_or(false);
            (sent, "direct-browser", None)
        }
        BackDispatchDecision::BlockedDisabled => (false, "none", Some("disabled")),
        BackDispatchDecision::BlockedUnavailable => (false, "none", Some("unavailable")),
    };

    if let Some(trace) = state_trace.as_mut() {
        let event = if sent {
            "navigation_action"
        } else {
            "navigation_action_blocked"
        };
        let mut fields = vec![
            ("action", "back".to_string()),
            ("source", source.to_string()),
            ("route", route_label.to_string()),
            ("tab_id", state.active_tab_id.to_string()),
            ("can_go_back", state.can_go_back.to_string()),
        ];
        if let Some(reason) = blocked_reason {
            fields.push(("reason", reason.to_string()));
        } else if !sent {
            fields.push(("reason", "stale-route".to_string()));
        }
        trace.write(event, &fields);
    }
    sent
}

fn dispatch_forward(
    source: &str,
    state: &ForwardControlState,
    route: Option<&BackRoute>,
    compositor: &Option<ipc::CompositorConnection>,
    browser_conn: &Option<ipc::BrowserConnection>,
    state_trace: &mut Option<StateTrace>,
) -> bool {
    let (sent, route_label, blocked_reason) = if !state.can_go_forward || state.active_tab_id <= 0 {
        (false, "none", Some("disabled"))
    } else {
        match route {
            Some(BackRoute::Compositor(pane_id)) => {
                let sent = compositor
                    .as_ref()
                    .map(|conn| conn.send_forward(pane_id))
                    .unwrap_or(false);
                (sent, "compositor", None)
            }
            Some(BackRoute::Direct(tab_id)) => {
                let sent = browser_conn
                    .as_ref()
                    .filter(|conn| conn.tab_id == *tab_id && *tab_id == state.active_tab_id)
                    .map(|conn| conn.send_forward())
                    .unwrap_or(false);
                (sent, "direct-browser", None)
            }
            None => (false, "none", Some("unavailable")),
        }
    };

    if let Some(trace) = state_trace.as_mut() {
        let event = if sent {
            "navigation_action"
        } else {
            "navigation_action_blocked"
        };
        let mut fields = vec![
            ("action", "forward".to_string()),
            ("source", source.to_string()),
            ("route", route_label.to_string()),
            ("tab_id", state.active_tab_id.to_string()),
            ("can_go_forward", state.can_go_forward.to_string()),
        ];
        if let Some(reason) = blocked_reason {
            fields.push(("reason", reason.to_string()));
        } else if !sent {
            fields.push(("reason", "stale-route".to_string()));
        }
        trace.write(event, &fields);
    }
    sent
}

fn dispatch_refresh(
    source: &str,
    state: &RefreshControlState,
    route: Option<&BackRoute>,
    compositor: &Option<ipc::CompositorConnection>,
    browser_conn: &Option<ipc::BrowserConnection>,
    state_trace: &mut Option<StateTrace>,
) -> bool {
    let (sent, route_label, request_id, blocked_reason) =
        if !state.can_refresh || state.active_tab_id <= 0 {
            (false, "none", 0, Some("disabled"))
        } else {
            match route {
                Some(BackRoute::Compositor(pane_id)) => {
                    let sent = compositor
                        .as_ref()
                        .map(|conn| conn.send_refresh(pane_id))
                        .unwrap_or(false);
                    (sent, "compositor", 0, None)
                }
                Some(BackRoute::Direct(tab_id)) => {
                    let request_id = browser_conn
                        .as_ref()
                        .filter(|conn| conn.tab_id == *tab_id && *tab_id == state.active_tab_id)
                        .and_then(|conn| conn.send_refresh())
                        .unwrap_or(0);
                    (request_id != 0, "direct-browser", request_id, None)
                }
                None => (false, "none", 0, Some("unavailable")),
            }
        };

    if let Some(trace) = state_trace.as_mut() {
        let event = if sent {
            "navigation_action"
        } else {
            "navigation_action_blocked"
        };
        let mut fields = vec![
            ("action", "refresh".to_string()),
            ("source", source.to_string()),
            ("route", route_label.to_string()),
            ("tab_id", state.active_tab_id.to_string()),
            ("request_id", request_id.to_string()),
            ("can_refresh", state.can_refresh.to_string()),
        ];
        if let Some(reason) = blocked_reason {
            fields.push(("reason", reason.to_string()));
        } else if !sent {
            fields.push(("reason", "stale-route".to_string()));
        }
        trace.write(event, &fields);
    }
    sent
}

// Command dispatch (Issues 659, 772).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DarkAction {
    Toggle,
    On,
    Off,
    System,
}

#[derive(Debug, PartialEq, Eq)]
struct ResolvedDarkAction {
    dark: bool,
    source: &'static str,
}

fn parse_macos_interface_style_dark(output: &str) -> Option<bool> {
    match output.trim().to_ascii_lowercase().as_str() {
        "dark" => Some(true),
        "light" => Some(false),
        "" => None,
        _ => None,
    }
}

fn macos_defaults_color_scheme(status_success: bool, stdout: &[u8]) -> Option<bool> {
    if !status_success {
        return Some(false);
    }

    parse_macos_interface_style_dark(&String::from_utf8_lossy(stdout))
}

#[cfg(target_os = "macos")]
fn current_system_dark_mode() -> Option<(bool, &'static str)> {
    let output = std::process::Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
        .ok()?;

    macos_defaults_color_scheme(output.status.success(), &output.stdout)
        .map(|dark| (dark, "macos-defaults"))
}

#[cfg(not(target_os = "macos"))]
fn current_system_dark_mode() -> Option<(bool, &'static str)> {
    None
}

fn resolve_dark_action(
    action: DarkAction,
    current_is_dark: bool,
    system_resolver: impl FnOnce() -> Option<(bool, &'static str)>,
) -> ResolvedDarkAction {
    match action {
        DarkAction::Toggle => ResolvedDarkAction {
            dark: !current_is_dark,
            source: "toggle",
        },
        DarkAction::On => ResolvedDarkAction {
            dark: true,
            source: "explicit-on",
        },
        DarkAction::Off => ResolvedDarkAction {
            dark: false,
            source: "explicit-off",
        },
        DarkAction::System => {
            if let Some((dark, source)) = system_resolver() {
                ResolvedDarkAction { dark, source }
            } else {
                ResolvedDarkAction {
                    dark: current_is_dark,
                    source: "current-state-fallback",
                }
            }
        }
    }
}

enum ViewportCommand {
    Height(u16),
    Reset,
}

enum CommandResult {
    Quit,
    Dark(DarkAction),
    Viewport(ViewportCommand),
    DevTools(String), // direction: "right", "down", "left", "up" (Issue 26030112000690).
    Error(String),    // error message for command bar (Issue 26030112000690).
    None,
}

struct Command {
    names: &'static [&'static str],
    exec: fn(args: &[&str]) -> CommandResult,
}

const COMMANDS: &[Command] = &[
    Command {
        names: &["quit", "q"],
        exec: |_| CommandResult::Quit,
    },
    Command {
        names: &["dark"],
        exec: |args| match args.first().copied() {
            None => CommandResult::Dark(DarkAction::Toggle),
            Some("on" | "yes" | "y") => CommandResult::Dark(DarkAction::On),
            Some("off" | "no" | "n") => CommandResult::Dark(DarkAction::Off),
            Some("system" | "s") => CommandResult::Dark(DarkAction::System),
            Some(other) => CommandResult::Error(format!("Unknown: {}", other)),
        },
    },
    Command {
        names: &["viewport", "vp"],
        exec: |args| match args.first().copied() {
            Some("height" | "h") => match args.get(1).copied() {
                Some(rows) => match rows.parse::<u16>() {
                    Ok(0) => CommandResult::Error("Viewport height must be greater than 0".into()),
                    Ok(rows) => CommandResult::Viewport(ViewportCommand::Height(rows)),
                    Err(_) => CommandResult::Error(format!("Invalid viewport height: {}", rows)),
                },
                None => CommandResult::Error("Usage: viewport height <rows>".into()),
            },
            Some("reset" | "r") => CommandResult::Viewport(ViewportCommand::Reset),
            Some(other) => CommandResult::Error(format!("Unknown viewport command: {}", other)),
            None => CommandResult::Error("Usage: viewport height <rows> | viewport reset".into()),
        },
    },
    Command {
        names: &["devtools", "dev"],
        exec: |args| match args.first().copied() {
            Some("right" | "r") | None => CommandResult::DevTools("right".into()),
            Some("down" | "d") => CommandResult::DevTools("down".into()),
            Some("left" | "l") => CommandResult::DevTools("left".into()),
            Some("up" | "u") => CommandResult::DevTools("up".into()),
            Some(other) => CommandResult::Error(format!("Unknown direction: {}", other)),
        },
    },
];

fn dispatch(input: &str) -> CommandResult {
    let mut parts = input.trim().splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    if cmd.is_empty() {
        return CommandResult::None;
    }
    let args: Vec<&str> = parts
        .next()
        .map(|s| s.split_whitespace().collect())
        .unwrap_or_default();
    for command in COMMANDS {
        if command.names.contains(&cmd) {
            return (command.exec)(&args);
        }
    }
    CommandResult::None
}

/// Clipboard wrapper that strips leading newlines from edtui's line-mode yanks
/// (Issue 26022712000658).
struct UrlClipboard(arboard::Clipboard);

impl UrlClipboard {
    fn new() -> Self {
        Self(arboard::Clipboard::new().expect("failed to open system clipboard"))
    }
}

impl ClipboardTrait for UrlClipboard {
    fn set_text(&mut self, text: String) {
        let clean = text.trim_start_matches('\n').to_string();
        let _ = self.0.set_text(clean);
    }

    fn get_text(&mut self) -> String {
        self.0.get_text().unwrap_or_default()
    }
}

#[derive(Parser)]
#[command(
    name = "ahweb",
    about = "Astrohacker Web — open URLs in Terminal browser panes",
    version = env!("ASTROHACKER_CLI_VERSION")
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// URL to open (fallback when no subcommand given)
    url: Option<String>,

    /// Browser profile name
    #[arg(short, long, global = true)]
    profile: Option<String>,

    /// Open an ephemeral private browser profile
    #[arg(long, global = true)]
    incognito: bool,

    /// Browser engine to use ("chromium", "webkit", "ladybird", "gecko") or absolute path
    #[arg(short, long, global = true)]
    browser: Option<String>,

    /// Render in the primary terminal screen instead of the alternate screen
    #[arg(long, global = true)]
    primary_screen: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Open a URL in the browser pane
    Url {
        /// The URL to open
        url: String,
    },
    /// Show the last active browser pane/tab
    Last,
    /// Show Chromium tab inventory for the current profile
    Status,
    /// Open a local file in the browser pane
    File {
        /// Path to the file (relative or absolute)
        path: String,
    },
}

fn main() -> io::Result<()> {
    if std::env::args().skip(1).any(is_version_arg) {
        println!("Astrohacker Web {}", env!("ASTROHACKER_CLI_VERSION"));
        return Ok(());
    }

    let cli = Cli::parse();

    let profile_arg = cli.profile; // Option<String> — None if no --profile given
    if cli.incognito && profile_arg.as_deref().is_some_and(|p| p != "incognito") {
        eprintln!(
            "Error: --incognito cannot be combined with --profile unless the profile is incognito"
        );
        std::process::exit(1);
    }
    let mut profile = if cli.incognito {
        "incognito".to_string()
    } else {
        profile_arg.clone().unwrap_or_else(|| "default".to_string())
    };
    let mut browser = cli.browser.unwrap_or_default();

    // Validate profile name: lowercase alphanumeric, starts with a letter.
    if profile.is_empty()
        || !profile.bytes().next().unwrap().is_ascii_lowercase()
        || !profile
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    {
        eprintln!("Error: profile name must be lowercase alphanumeric, starting with a letter");
        std::process::exit(1);
    }

    // Connect to the TermSurf compositor via XPC (Issue 26021512000505).
    let pane_id = std::env::var("TERMSURF_PANE_ID").ok();

    let (tx, rx) = std::sync::mpsc::channel();
    let compositor = pane_id
        .as_ref()
        .and_then(|_| ipc::CompositorConnection::connect(tx.clone()));

    // Handle `web last` subcommand — print last active browser pane and exit (Issue 26030112000684 Exp 4).
    if let Some(Commands::Last) = cli.command {
        if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
            let query_profile = if cli.incognito {
                "incognito"
            } else {
                profile_arg.as_deref().unwrap_or("")
            };
            match conn.send_query_last(pid, query_profile) {
                Some((prof, pane, tab)) => {
                    println!("profile: {}", prof);
                    println!("pane_id: {}", pane);
                    println!("tab_id:  {}", tab);
                }
                None => {
                    println!("No active browser tab found.");
                }
            }
        } else {
            println!("Not running inside TermSurf.");
        }
        return Ok(());
    }

    // Handle `web status` — print tab inventory and exit (Issue 26030112000689).
    if let Some(Commands::Status) = cli.command {
        if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
            match conn.send_query_tabs(pid, &profile) {
                Ok(status) => println!("{}", status),
                Err(e) => eprintln!("Error: {}", e),
            }
        } else {
            println!("Not running inside TermSurf.");
        }
        return Ok(());
    }

    // Send hello to get live config from the GUI (Issue 26022812000675).
    // Returns (homepage, browsers) — Issue 26030612000712.
    let (hello_homepage, hello_browsers) = compositor
        .as_ref()
        .and_then(|conn| pane_id.as_ref().and_then(|pid| conn.send_hello(pid)))
        .map(|(hp, br)| (Some(hp), br))
        .unwrap_or((None, vec![]));

    // Default browser from hello reply when --browser not specified (Issue 26030612000712).
    if browser.is_empty() {
        if let Some(first) = hello_browsers.first() {
            browser = first.clone();
        }
    }

    // Detect devtools://N before normalizing (Issue 26030112000684).
    let raw_url = match cli.command {
        Some(Commands::Url { url }) => url,
        Some(Commands::File { path }) => {
            let absolute = std::fs::canonicalize(&path).unwrap_or_else(|e| {
                eprintln!("Error: {}: {}", path, e);
                std::process::exit(1);
            });
            format!("file://{}", absolute.display())
        }
        Some(Commands::Last) | Some(Commands::Status) => unreachable!(), // Handled above.
        None => cli.url.unwrap_or_else(|| {
            hello_homepage
                .filter(|hp| !hp.is_empty())
                .unwrap_or_else(|| "https://astrohacker.com/welcome".to_string())
        }),
    };
    let mut inspected_tab_id: i64 = if let Some(id) = raw_url.strip_prefix("devtools://") {
        id.parse::<i64>().unwrap_or(0)
    } else if raw_url == "devtools" {
        eprintln!(
            "Error: DevTools requires opening from a browser pane or an explicit devtools://<tab_id> target with --browser and --profile"
        );
        return Ok(());
    } else {
        -1 // Not a DevTools request.
    };
    let is_devtools = inspected_tab_id >= 0;
    let mut url = if is_devtools {
        raw_url // Keep devtools://N as-is.
    } else {
        match resolve_input(&raw_url) {
            Some(resolved) => resolved,
            None => {
                eprintln!("Error: '{}' is not a URL, file, or command", raw_url);
                std::process::exit(1);
            }
        }
    };

    // Validate DevTools request before entering the UI (Issue 26030112000687).
    // The reply includes the inspected tab's browser and profile (Issue 26030412000705 Exp 10).
    if is_devtools {
        if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
            match conn.send_query_devtools(pid, inspected_tab_id, &profile, &browser) {
                Ok((resolved_tab_id, resolved_browser, resolved_profile)) => {
                    inspected_tab_id = resolved_tab_id;
                    if !resolved_browser.is_empty() {
                        browser = resolved_browser;
                    }
                    if !resolved_profile.is_empty() {
                        profile = resolved_profile;
                    }
                }
                Err(err) => {
                    eprintln!("Error: {}", err);
                    return Ok(());
                }
            }
        }
    }

    let use_alternate_screen = !cli.primary_screen;

    // Enter raw mode and the requested terminal screen.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if use_alternate_screen {
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    } else {
        execute!(stdout, EnableMouseCapture)?;
    }
    // Crossterm's capture enables click and drag reports. The Back control also
    // needs pointer motion with no button held so its hover state is observable.
    write!(stdout, "{ENABLE_ANY_MOUSE_MOTION}")?;
    stdout.flush()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    // Crossterm reader thread — forwards relevant terminal events (Issue 26022812000668).
    // Back-button hover and activation require mouse events. Focus remains local.
    let browser_tx = tx.clone();
    let key_tx = tx;
    std::thread::spawn(move || loop {
        match event::read() {
            Ok(ev @ (Event::Key(_) | Event::Resize(_, _) | Event::Paste(_) | Event::Mouse(_))) => {
                if key_tx.send(LoopEvent::Terminal(ev)).is_err() {
                    break;
                }
            }
            Ok(_) => {} // FocusGained, FocusLost — drop silently.
            Err(_) => break,
        }
    });

    // Capture executable path for `:devtools` split command (Issue 26030112000690).
    let current_exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "web".to_string());

    let mut mode = initial_mode();
    let mut is_dark = true;
    let mut command_error: Option<String> = None; // Command bar error (Issue 26030112000690).
    let mut browser_ready = false;
    let mut current_tab_id: i64 = 0;
    let mut page_loaded = false;
    let mut page_loaded_at: Option<Instant> = None;
    let mut loading_log: Vec<(LoadingStage, StageStatus)> = Vec::new();
    let mut console_log: Vec<ConsoleLogEntry> = Vec::new();
    let mut renderer_crash: Option<RendererCrashState> = None;
    let mut renderer_crash_recovery_load_started = false;
    let mut browser_wait_start: Option<Instant> = None;

    // Populate initial loading stages (Issue 26040512000773).
    if compositor.is_some() {
        loading_log.push((LoadingStage::ConnectingToGui, StageStatus::Done));
    } else if pane_id.is_some() {
        loading_log.push((
            LoadingStage::ConnectingToGui,
            StageStatus::Error("Failed to connect to GUI".into()),
        ));
    }
    let mut last_viewport = Rect::default();
    let mut loading_bar_active = false;
    let mut loading_bar_start: Option<Instant> = None;
    const LOADING_TIMEOUT: Duration = Duration::from_secs(30);
    let mut page_title = String::new();
    let mut target_url = String::new();
    let mut browser_conn: Option<ipc::BrowserConnection> = None;
    let mut pending_dialog: Option<PendingJsDialog> = None;
    let mut pending_auth: Option<PendingHttpAuth> = None;
    let mut handled_dialogs: Vec<(i64, u64)> = Vec::new();
    let mut handled_auth: Vec<(i64, u64)> = Vec::new();
    let mut copy_url_feedback_until: Option<Instant> = None;
    let mut state_trace = StateTrace::from_env();
    let mut last_render_trace = String::new();
    // edtui state (Issue 26022412000637, 658).
    let mut editor_state = EditorState::new(Lines::from(url.as_str()));
    editor_state.set_clipboard(UrlClipboard::new());
    let mut editor_url = url.clone(); // Track which URL the editor has.
    let make_single_line_handler = || {
        let mut kh = KeyEventHandler::vim_mode();
        // Remove newline keybindings for single-line mode.
        kh.remove(&KeyEventRegister::i(vec![KeyInput::new(KeyCode::Enter)]));
        kh.remove(&KeyEventRegister::n(vec![KeyInput::new('o')]));
        kh.remove(&KeyEventRegister::n(vec![KeyInput::shift('O')]));
        EditorEventHandler::new(kh)
    };
    let mut editor_handler = make_single_line_handler();

    // Command mode editor state (Issue 26022712000659).
    let mut cmd_state = EditorState::new(Lines::from(""));
    cmd_state.set_clipboard(UrlClipboard::new());
    let mut cmd_handler = make_single_line_handler();
    let mut viewport_height_override: Option<u16> = None;
    let mut back_control = BackControlState::default();
    let mut forward_control = ForwardControlState::default();
    let mut refresh_control = RefreshControlState::default();
    let mut refresh_animation = RefreshAnimation::default();
    let mut last_back_visual: Option<BackVisualState> = None;
    let mut last_forward_visual: Option<BackVisualState> = None;
    let mut last_refresh_visual: Option<BackVisualState> = None;

    // Event loop.
    loop {
        let mut viewport_rect = Rect::default();
        let mut back_rect = Rect::default();
        let mut forward_rect = Rect::default();
        let mut refresh_rect = Rect::default();
        let mut url_rect = Rect::default();
        let mut frame_area = Rect::default();
        let browser_label = browser_display_label(&browser);
        let now = Instant::now();
        refresh_animation.tick(now);
        let back_route = current_back_route(
            current_tab_id,
            compositor.is_some(),
            pane_id.as_deref(),
            browser_conn.as_ref().map(|conn| conn.tab_id),
        );
        back_control.reconcile_route(back_route.as_ref());
        forward_control.reconcile_route(back_route.as_ref());
        refresh_control.reconcile_route(back_route.as_ref());
        let back_visual = back_visual_state(&back_control, back_route.as_ref());
        let forward_visual = forward_visual_state(&forward_control, back_route.as_ref());
        let refresh_visual = refresh_visual_state(&refresh_control, back_route.as_ref());
        let navigation_visual_changed = last_back_visual
            .map(|previous| previous != back_visual)
            .unwrap_or(false)
            || last_forward_visual
                .map(|previous| previous != forward_visual)
                .unwrap_or(false)
            || last_refresh_visual
                .map(|previous| previous != refresh_visual)
                .unwrap_or(false);
        if navigation_visual_changed {
            // Ghostty can retain style-only cell damage beneath a browser
            // overlay. Navigation state changes are infrequent, so force one full
            // terminal redraw at that boundary to make the visual feedback
            // observable without turning steady-state rendering into polling.
            terminal.clear()?;
            if let Some(trace) = state_trace.as_mut() {
                trace.write(
                    "back_visual_redraw",
                    &[
                        ("actionable", back_visual.actionable.to_string()),
                        ("hovered", back_visual.hovered.to_string()),
                        ("pressed", back_visual.pressed.to_string()),
                        ("forward_actionable", forward_visual.actionable.to_string()),
                        ("forward_hovered", forward_visual.hovered.to_string()),
                        ("forward_pressed", forward_visual.pressed.to_string()),
                        ("refresh_actionable", refresh_visual.actionable.to_string()),
                        ("refresh_hovered", refresh_visual.hovered.to_string()),
                        ("refresh_pressed", refresh_visual.pressed.to_string()),
                    ],
                );
            }
        }
        terminal.draw(|frame| {
            frame_area = frame.area();
            let geometry = ui(
                frame,
                &url,
                &profile,
                &mode,
                &mut editor_state,
                &mut cmd_state,
                &page_title,
                is_devtools,
                inspected_tab_id,
                current_tab_id,
                &command_error,
                browser_label,
                &target_url,
                &pending_dialog,
                &pending_auth,
                copy_url_feedback_until,
                &loading_log,
                &renderer_crash,
                browser_ready,
                browser_wait_start,
                viewport_height_override,
                &back_control,
                &forward_control,
                &refresh_control,
                &refresh_animation,
                now,
                back_route.is_some(),
            );
            viewport_rect = geometry.viewport;
            back_rect = geometry.back;
            forward_rect = geometry.forward;
            refresh_rect = geometry.refresh;
            url_rect = geometry.url;
        })?;
        last_back_visual = Some(back_visual);
        last_forward_visual = Some(forward_visual);
        last_refresh_visual = Some(refresh_visual);
        if let Some(trace) = state_trace.as_mut() {
            let latest_console = console_log.last();
            let latest_console_summary = latest_console
                .map(|entry| {
                    format!(
                        "{}:{} #{} {} {}",
                        entry
                            .source_id
                            .rsplit('/')
                            .next()
                            .unwrap_or(&entry.source_id),
                        entry.line_no,
                        entry.tab_id,
                        entry.level,
                        entry.message
                    )
                })
                .unwrap_or_default();
            let (
                renderer_crash_active,
                renderer_crash_tab_id,
                renderer_crash_status,
                renderer_crash_can_reload,
            ) = renderer_crash
                .as_ref()
                .map(|crash| {
                    (
                        true,
                        crash.tab_id.to_string(),
                        crash.termination_status.clone(),
                        crash.can_reload.to_string(),
                    )
                })
                .unwrap_or_else(|| (false, String::new(), String::new(), String::new()));
            let identity_label = viewport_identity_label(
                browser_label,
                &profile,
                is_devtools,
                inspected_tab_id,
                current_tab_id,
            );
            let back_actionable = back_control.actionable(back_route.as_ref());
            let back_pressed = back_control.pressed.is_some();
            let forward_actionable = forward_control.actionable(back_route.as_ref());
            let forward_pressed = forward_control.pressed.is_some();
            let refresh_actionable = refresh_control.actionable(back_route.as_ref());
            let refresh_pressed = refresh_control.pressed.is_some();
            let refresh_symbol = refresh_symbol(&refresh_animation, now);
            let back_route_label = back_route.as_ref().map(BackRoute::label).unwrap_or("none");
            let base_render_trace = format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                url,
                page_title,
                target_url,
                page_loaded,
                browser_ready,
                loading_bar_active,
                renderer_crash_active,
                renderer_crash_tab_id,
                renderer_crash_status,
                renderer_crash_can_reload,
                latest_console_summary,
                identity_label,
                browser_label,
                profile,
                is_devtools,
                current_tab_id,
                inspected_tab_id,
                back_control.can_go_back,
                back_actionable,
                back_control.hovered,
                back_pressed,
                back_route_label,
                BACK_SYMBOL,
                trace_rect(back_rect),
                trace_rect(url_rect),
                trace_rect(viewport_rect)
            );
            let render_trace = format!(
                "{base_render_trace}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                forward_control.can_go_forward,
                forward_actionable,
                forward_control.hovered,
                forward_pressed,
                FORWARD_SYMBOL,
                trace_rect(forward_rect),
                refresh_control.can_refresh,
                refresh_actionable,
                refresh_control.hovered,
                refresh_pressed,
                refresh_animation.active(),
                refresh_animation.request_id,
                refresh_symbol,
            );
            if render_trace != last_render_trace {
                trace.write(
                    "render_state",
                    &[
                        ("url", url.clone()),
                        ("title", page_title.clone()),
                        ("target_url", target_url.clone()),
                        ("page_loaded", page_loaded.to_string()),
                        ("browser_ready", browser_ready.to_string()),
                        ("loading_bar_active", loading_bar_active.to_string()),
                        ("renderer_crash_active", renderer_crash_active.to_string()),
                        ("renderer_crash_tab_id", renderer_crash_tab_id),
                        ("renderer_crash_status", renderer_crash_status),
                        ("renderer_crash_can_reload", renderer_crash_can_reload),
                        ("latest_console", latest_console_summary),
                        ("identity_label", identity_label),
                        ("browser_label", browser_label.to_string()),
                        ("profile", profile.clone()),
                        ("is_devtools", is_devtools.to_string()),
                        ("current_tab_id", current_tab_id.to_string()),
                        ("inspected_tab_id", inspected_tab_id.to_string()),
                        ("can_go_back", back_control.can_go_back.to_string()),
                        ("back_actionable", back_actionable.to_string()),
                        ("back_hovered", back_control.hovered.to_string()),
                        ("back_pressed", back_pressed.to_string()),
                        ("back_route", back_route_label.to_string()),
                        ("back_symbol", BACK_SYMBOL.to_string()),
                        ("back_rect", trace_rect(back_rect)),
                        ("can_go_forward", forward_control.can_go_forward.to_string()),
                        ("forward_actionable", forward_actionable.to_string()),
                        ("forward_hovered", forward_control.hovered.to_string()),
                        ("forward_pressed", forward_pressed.to_string()),
                        ("forward_route", back_route_label.to_string()),
                        ("forward_symbol", FORWARD_SYMBOL.to_string()),
                        ("forward_rect", trace_rect(forward_rect)),
                        ("can_refresh", refresh_control.can_refresh.to_string()),
                        ("refresh_actionable", refresh_actionable.to_string()),
                        ("refresh_hovered", refresh_control.hovered.to_string()),
                        ("refresh_pressed", refresh_pressed.to_string()),
                        ("refresh_route", back_route_label.to_string()),
                        (
                            "refresh_animation_active",
                            refresh_animation.active().to_string(),
                        ),
                        (
                            "refresh_request_id",
                            refresh_animation.request_id.to_string(),
                        ),
                        ("refresh_symbol", refresh_symbol.to_string()),
                        ("refresh_rect", trace_rect(refresh_rect)),
                        ("url_rect", trace_rect(url_rect)),
                        ("viewport_rect", trace_rect(viewport_rect)),
                    ],
                );
                last_render_trace = render_trace;
            }
        }

        // Send overlay coordinates to compositor (only when changed).
        if viewport_rect != last_viewport {
            let first_overlay = last_viewport == Rect::default();
            if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                if is_devtools {
                    // DevTools pane (Issue 26030112000684).
                    conn.send_set_devtools_overlay(
                        pid,
                        viewport_rect.x,
                        viewport_rect.y,
                        viewport_rect.width,
                        viewport_rect.height,
                        inspected_tab_id,
                        &profile,
                        mode_is_browsing(&mode),
                        &browser,
                    );
                } else {
                    conn.send_set_overlay(
                        pid,
                        viewport_rect.x,
                        viewport_rect.y,
                        viewport_rect.width,
                        viewport_rect.height,
                        &url,
                        &profile,
                        mode_is_browsing(&mode),
                        &browser,
                    );
                }
            }
            last_viewport = viewport_rect;

            // Emit indeterminate pulse immediately on first overlay (cold-start coverage).
            if first_overlay {
                let mut stdout = io::stdout();
                let _ = write!(stdout, "\x1b]9;4;3\x1b\\");
                let _ = stdout.flush();
                loading_bar_active = true;
                loading_bar_start = Some(Instant::now());

                // Loading stages (Issue 26040512000773).
                loading_log.push((LoadingStage::StartingBrowser, StageStatus::Done));
                loading_log.push((LoadingStage::WaitingForBrowser, StageStatus::InProgress));
                browser_wait_start = Some(Instant::now());
            }
        }

        // Unified event channel.
        // During loading, use a short timeout for smooth spinner animation and
        // to keep the GUI repainting (so the CALayerHost overlay appears).
        // After the page has fully loaded, keep polling for a 2-second grace
        // period so the GUI has time to create and display the overlay.
        // Then switch to blocking recv (Issue 26022812000668, 773).
        let needs_polling = needs_event_polling(
            page_loaded,
            page_loaded_at,
            copy_url_feedback_until,
            refresh_animation.active(),
            Instant::now(),
        );
        let event = if needs_polling {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(e) => Ok(e),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            rx.recv()
        };
        match event {
            Ok(LoopEvent::Terminal(Event::Key(key))) => {
                // Ctrl+C quits from any mode.
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    break;
                }

                if local_back_key(&mode, key) {
                    dispatch_back(
                        "keyboard",
                        &back_control,
                        back_route.as_ref(),
                        &compositor,
                        &browser_conn,
                        &mut state_trace,
                    );
                    continue;
                }
                if local_forward_key(&mode, key) {
                    dispatch_forward(
                        "keyboard",
                        &forward_control,
                        back_route.as_ref(),
                        &compositor,
                        &browser_conn,
                        &mut state_trace,
                    );
                    continue;
                }
                if local_refresh_key(&mode, key) {
                    dispatch_refresh(
                        "keyboard",
                        &refresh_control,
                        back_route.as_ref(),
                        &compositor,
                        &browser_conn,
                        &mut state_trace,
                    );
                    continue;
                }

                if let Some(dialog) = pending_dialog.as_mut() {
                    let mut reply: Option<(bool, String)> = None;
                    match dialog.dialog_type.as_str() {
                        "alert" => {
                            if key.code == KeyCode::Enter {
                                reply = Some((true, String::new()));
                            } else if key.code == KeyCode::Esc {
                                reply = Some((false, String::new()));
                            }
                        }
                        "confirm" | "beforeunload" => match key.code {
                            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                                reply = Some((true, String::new()));
                            }
                            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                                reply = Some((false, String::new()));
                            }
                            _ => {}
                        },
                        "prompt" => match key.code {
                            KeyCode::Enter => {
                                reply = Some((true, dialog.input.clone()));
                            }
                            KeyCode::Esc => {
                                reply = Some((false, String::new()));
                            }
                            KeyCode::Backspace => {
                                dialog.input.pop();
                            }
                            KeyCode::Char(ch) => {
                                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT
                                {
                                    dialog.input.push(ch);
                                }
                            }
                            _ => {}
                        },
                        _ => {
                            reply = Some((false, String::new()));
                        }
                    }

                    if let Some((accepted, prompt_text)) = reply {
                        let tab_id = dialog.tab_id;
                        let request_id = dialog.request_id;
                        let dialog_type = dialog.dialog_type.clone();
                        let message = dialog.message.clone();
                        let previous_mode = dialog.previous_mode.clone();
                        if let Some(ref bc) = browser_conn {
                            bc.send_javascript_dialog_reply(request_id, accepted, &prompt_text);
                        }
                        if let Some(ref conn) = compositor {
                            conn.send_javascript_dialog_reply(
                                tab_id,
                                request_id,
                                accepted,
                                &prompt_text,
                            );
                        }
                        if let Some(trace) = state_trace.as_mut() {
                            trace.write(
                                "javascript_dialog_reply",
                                &[
                                    ("tab_id", tab_id.to_string()),
                                    ("request_id", request_id.to_string()),
                                    ("dialog_type", dialog_type),
                                    ("message", message),
                                    ("accepted", accepted.to_string()),
                                    ("prompt_text", prompt_text.clone()),
                                ],
                            );
                        }
                        pending_dialog = None;
                        handled_dialogs.push((tab_id, request_id));
                        if handled_dialogs.len() > 32 {
                            handled_dialogs.remove(0);
                        }
                        mode = previous_mode;
                    }
                    continue;
                }

                if let Some(auth) = pending_auth.as_mut() {
                    let mut reply: Option<bool> = None;
                    match key.code {
                        KeyCode::Esc => reply = Some(false),
                        KeyCode::Enter => {
                            if auth.field == AuthField::Username {
                                auth.field = AuthField::Password;
                            } else {
                                reply = Some(true);
                            }
                        }
                        KeyCode::Tab => {
                            auth.field = if auth.field == AuthField::Username {
                                AuthField::Password
                            } else {
                                AuthField::Username
                            };
                        }
                        KeyCode::Backspace => {
                            if auth.field == AuthField::Username {
                                auth.username.pop();
                            } else {
                                auth.password.pop();
                            }
                        }
                        KeyCode::Char(ch) => {
                            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                                if auth.field == AuthField::Username {
                                    auth.username.push(ch);
                                } else {
                                    auth.password.push(ch);
                                }
                            }
                        }
                        _ => {}
                    }

                    if let Some(accepted) = reply {
                        let tab_id = auth.tab_id;
                        let request_id = auth.request_id;
                        let url = auth.url.clone();
                        let auth_scheme = auth.auth_scheme.clone();
                        let realm = auth.realm.clone();
                        let previous_mode = auth.previous_mode.clone();
                        let username = if accepted {
                            auth.username.clone()
                        } else {
                            String::new()
                        };
                        let password = if accepted {
                            auth.password.clone()
                        } else {
                            String::new()
                        };
                        if let Some(ref bc) = browser_conn {
                            bc.send_http_auth_reply(request_id, accepted, &username, &password);
                        }
                        if let Some(ref conn) = compositor {
                            conn.send_http_auth_reply(
                                tab_id, request_id, accepted, &username, &password,
                            );
                        }
                        if let Some(trace) = state_trace.as_mut() {
                            trace.write(
                                "http_auth_reply",
                                &[
                                    ("tab_id", tab_id.to_string()),
                                    ("request_id", request_id.to_string()),
                                    ("url", url),
                                    ("auth_scheme", auth_scheme),
                                    ("realm", realm),
                                    ("accepted", accepted.to_string()),
                                    ("username", username.clone()),
                                    ("password_len", password.chars().count().to_string()),
                                ],
                            );
                        }
                        pending_auth = None;
                        handled_auth.push((tab_id, request_id));
                        if handled_auth.len() > 32 {
                            handled_auth.remove(0);
                        }
                        mode = previous_mode;
                    }
                    continue;
                }

                match mode {
                    Mode::Browse => {
                        if key.code == KeyCode::Esc {
                            mode = Mode::Control;
                            if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                conn.send_mode_changed(pid, false);
                            }
                        }
                    }
                    Mode::Control => {
                        // Sync editor content if URL changed externally (Issue 26022712000658).
                        let enter_edit =
                            |editor_state: &mut EditorState,
                             editor_url: &mut String,
                             url: &str,
                             mode: &mut Mode| {
                                if *editor_url != url {
                                    *editor_state = EditorState::new(Lines::from(url));
                                    editor_state.set_clipboard(UrlClipboard::new());
                                    let len = url.len();
                                    editor_state.cursor =
                                        edtui::Index2::new(0, len.saturating_sub(1));
                                    *editor_url = url.to_string();
                                }
                                *mode = Mode::Edit;
                            };
                        match key.code {
                            // Edit mode keys are disabled in DevTools (Issue 26030112000687).
                            KeyCode::Char('i') if !is_devtools => {
                                // Insert mode, cursor at last position (Issue 26022712000658).
                                enter_edit(&mut editor_state, &mut editor_url, &url, &mut mode);
                                editor_state.mode = EditorMode::Insert;
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, false);
                                }
                            }
                            KeyCode::Char('A') if !is_devtools => {
                                // Insert mode, cursor at end of line (Issue 26022712000658).
                                enter_edit(&mut editor_state, &mut editor_url, &url, &mut mode);
                                editor_state.cursor.col =
                                    editor_state.lines.len_col(0).unwrap_or(0);
                                editor_state.mode = EditorMode::Insert;
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, false);
                                }
                            }
                            KeyCode::Char('I') if !is_devtools => {
                                // Insert mode, cursor at start of line (Issue 26022712000658).
                                enter_edit(&mut editor_state, &mut editor_url, &url, &mut mode);
                                editor_state.cursor = edtui::Index2::new(0, 0);
                                editor_state.mode = EditorMode::Insert;
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, false);
                                }
                            }
                            KeyCode::Char('n') if !is_devtools => {
                                // Normal mode, cursor at last position (Issue 26022712000658).
                                enter_edit(&mut editor_state, &mut editor_url, &url, &mut mode);
                                editor_state.mode = EditorMode::Normal;
                                editor_state.selection = None;
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, false);
                                }
                            }
                            KeyCode::Char('v') if !is_devtools => {
                                // Visual mode, cursor at last position (Issue 26022712000658).
                                enter_edit(&mut editor_state, &mut editor_url, &url, &mut mode);
                                SwitchMode(EditorMode::Visual).execute(&mut editor_state);
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, false);
                                }
                            }
                            KeyCode::Char('V') if !is_devtools => {
                                // Visual mode, entire line selected (Issue 26022712000658).
                                enter_edit(&mut editor_state, &mut editor_url, &url, &mut mode);
                                SelectLine.execute(&mut editor_state);
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, false);
                                }
                            }
                            KeyCode::Char(':') => {
                                // Enter Command mode with fresh editor (Issue 26022712000659).
                                cmd_state = EditorState::new(Lines::from(""));
                                cmd_state.set_clipboard(UrlClipboard::new());
                                cmd_state.mode = EditorMode::Insert;
                                mode = Mode::Command;
                            }
                            KeyCode::Char('c') | KeyCode::Char('C')
                                if key.modifiers.contains(KeyModifiers::SUPER) && !is_devtools =>
                            {
                                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                    let _ = clipboard.set_text(url.clone());
                                }
                                copy_url_feedback_until =
                                    Some(Instant::now() + Duration::from_millis(1500));
                                if let Some(trace) = state_trace.as_mut() {
                                    trace.write(
                                        "copy_current_url",
                                        &[
                                            ("tab_id", current_tab_id.to_string()),
                                            ("url", url.clone()),
                                            ("mode", "control".to_string()),
                                        ],
                                    );
                                }
                            }
                            KeyCode::Enter => {
                                mode = Mode::Browse;
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, true);
                                }
                            }
                            _ => {}
                        }
                    }
                    Mode::Edit => {
                        // Esc in Normal mode exits Edit → Control (Issue 26022812000665).
                        if key.code == KeyCode::Esc && editor_state.mode == EditorMode::Normal {
                            mode = Mode::Control;
                        } else if key.code == KeyCode::Enter
                            && editor_state.mode != EditorMode::Search
                            && !is_devtools
                        // Safety guard: no navigation in DevTools (Issue 26030112000687).
                        {
                            // Extract URL from editor, navigate, switch to Browse.
                            let new_url: String = editor_state
                                .lines
                                .get(RowIndex::new(0))
                                .map(|line| line.iter().collect())
                                .unwrap_or_default();
                            match resolve_input(&new_url) {
                                Some(resolved) => {
                                    url = resolved;
                                    editor_url = url.clone();
                                    mode = Mode::Browse;
                                    if let Some(ref bc) = browser_conn {
                                        bc.send_navigate(&url);
                                    } else if let (Some(ref conn), Some(ref pid)) =
                                        (&compositor, &pane_id)
                                    {
                                        conn.send_navigate(pid, &url);
                                    }
                                    if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id)
                                    {
                                        conn.send_mode_changed(pid, true);
                                    }
                                }
                                None => {
                                    command_error =
                                        Some(format!("'{}' is not a URL or file", new_url));
                                    mode = Mode::Command;
                                }
                            }
                        } else {
                            // Pass everything else to edtui (including Escape).
                            editor_handler.on_key_event(key, &mut editor_state);
                        }
                    }
                    Mode::Command => {
                        // Esc in Normal mode exits Command → Control (Issue 26022812000665).
                        if key.code == KeyCode::Esc && cmd_state.mode == EditorMode::Normal {
                            command_error = None;
                            mode = Mode::Control;
                        } else if key.code == KeyCode::Enter && cmd_state.mode != EditorMode::Search
                        {
                            // Extract command text and dispatch (Issue 26022712000659).
                            let cmd_text: String = cmd_state
                                .lines
                                .get(RowIndex::new(0))
                                .map(|line| line.iter().collect())
                                .unwrap_or_default();
                            match dispatch(&cmd_text) {
                                CommandResult::Quit => break,
                                CommandResult::Dark(action) => {
                                    let resolved = resolve_dark_action(
                                        action,
                                        is_dark,
                                        current_system_dark_mode,
                                    );
                                    let action_label = match action {
                                        DarkAction::Toggle => "toggle",
                                        DarkAction::On => "on",
                                        DarkAction::Off => "off",
                                        DarkAction::System => "system",
                                    };
                                    let dark = resolved.dark;
                                    is_dark = dark;
                                    let scheme = if dark { "dark" } else { "light" };
                                    if let Some(trace) = state_trace.as_mut() {
                                        trace.write(
                                            "color_scheme_command",
                                            &[
                                                ("action", action_label.to_string()),
                                                ("scheme", scheme.to_string()),
                                                ("dark", dark.to_string()),
                                                ("source", resolved.source.to_string()),
                                                ("tab_id", current_tab_id.to_string()),
                                            ],
                                        );
                                    }
                                    if let Some(ref bc) = browser_conn {
                                        bc.send_set_color_scheme(scheme);
                                    }
                                    if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id)
                                    {
                                        conn.send_set_color_scheme(pid, scheme);
                                    }
                                }
                                CommandResult::DevTools(direction) => {
                                    if is_devtools {
                                        command_error = Some(
                                            "Cannot open DevTools from a DevTools pane".into(),
                                        );
                                    } else if let (Some(ref conn), Some(ref pid)) =
                                        (&compositor, &pane_id)
                                    {
                                        if !browser_ready || current_tab_id == 0 {
                                            command_error = Some(
                                                "Browser is still loading; try again in a moment"
                                                    .into(),
                                            );
                                            continue;
                                        }
                                        match conn.send_query_devtools(
                                            pid,
                                            current_tab_id,
                                            &profile,
                                            &browser,
                                        ) {
                                            Err(msg) => {
                                                command_error = Some(msg);
                                            }
                                            Ok(_) => {
                                                let cmd = format!(
                                                    "{} --browser {} --profile {} devtools://{}",
                                                    current_exe,
                                                    shell_quote_arg(&browser),
                                                    shell_quote_arg(&profile),
                                                    current_tab_id
                                                );
                                                conn.send_open_split(pid, &direction, &cmd);
                                            }
                                        }
                                    }
                                }
                                CommandResult::Viewport(command) => match command {
                                    ViewportCommand::Height(rows) => {
                                        viewport_height_override = Some(rows);
                                    }
                                    ViewportCommand::Reset => {
                                        viewport_height_override = None;
                                    }
                                },
                                CommandResult::Error(msg) => {
                                    command_error = Some(msg);
                                }
                                CommandResult::None => {}
                            }
                            if command_error.is_none() {
                                mode = Mode::Control;
                            }
                        } else {
                            // Clear command error on any non-Enter keystroke (Issue 26030112000690).
                            command_error = None;
                            // Pass everything else to command edtui.
                            cmd_handler.on_key_event(key, &mut cmd_state);
                        }
                    }
                    Mode::Dialog | Mode::Auth => {}
                }
            }
            Ok(LoopEvent::Terminal(Event::Mouse(mouse))) => {
                let back_hit = rect_contains(back_rect, mouse.column, mouse.row);
                let forward_hit = rect_contains(forward_rect, mouse.column, mouse.row);
                let refresh_hit = rect_contains(refresh_rect, mouse.column, mouse.row);
                let back_actionable = back_control.actionable(back_route.as_ref());
                let forward_actionable = forward_control.actionable(back_route.as_ref());
                let refresh_actionable = refresh_control.actionable(back_route.as_ref());
                if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                    if back_hit {
                        forward_control.clear_interaction();
                        refresh_control.clear_interaction();
                    } else if forward_hit {
                        back_control.clear_interaction();
                        refresh_control.clear_interaction();
                    } else if refresh_hit {
                        back_control.clear_interaction();
                        forward_control.clear_interaction();
                    }
                }
                let back_result =
                    update_back_mouse(&mut back_control, back_rect, back_route.as_ref(), mouse);
                let forward_result = update_forward_mouse(
                    &mut forward_control,
                    forward_rect,
                    back_route.as_ref(),
                    mouse,
                );
                let refresh_result = update_refresh_mouse(
                    &mut refresh_control,
                    refresh_rect,
                    back_route.as_ref(),
                    mouse,
                );
                if back_result.changed {
                    if let Some(trace) = state_trace.as_mut() {
                        trace.write(
                            "back_pointer_state",
                            &[
                                ("column", mouse.column.to_string()),
                                ("row", mouse.row.to_string()),
                                ("kind", format!("{:?}", mouse.kind)),
                                ("hit", back_hit.to_string()),
                                ("actionable", back_actionable.to_string()),
                                ("hovered", back_control.hovered.to_string()),
                                ("pressed", back_control.pressed.is_some().to_string()),
                                ("tab_id", back_control.active_tab_id.to_string()),
                            ],
                        );
                    }
                }
                if forward_result.changed {
                    if let Some(trace) = state_trace.as_mut() {
                        trace.write(
                            "forward_pointer_state",
                            &[
                                ("column", mouse.column.to_string()),
                                ("row", mouse.row.to_string()),
                                ("kind", format!("{:?}", mouse.kind)),
                                ("hit", forward_hit.to_string()),
                                ("actionable", forward_actionable.to_string()),
                                ("hovered", forward_control.hovered.to_string()),
                                ("pressed", forward_control.pressed.is_some().to_string()),
                                ("tab_id", forward_control.active_tab_id.to_string()),
                            ],
                        );
                    }
                }
                if refresh_result.changed {
                    if let Some(trace) = state_trace.as_mut() {
                        trace.write(
                            "refresh_pointer_state",
                            &[
                                ("column", mouse.column.to_string()),
                                ("row", mouse.row.to_string()),
                                ("kind", format!("{:?}", mouse.kind)),
                                ("hit", refresh_hit.to_string()),
                                ("actionable", refresh_actionable.to_string()),
                                ("hovered", refresh_control.hovered.to_string()),
                                ("pressed", refresh_control.pressed.is_some().to_string()),
                                ("tab_id", refresh_control.active_tab_id.to_string()),
                            ],
                        );
                    }
                }
                if back_result.activate {
                    dispatch_back(
                        "mouse",
                        &back_control,
                        back_route.as_ref(),
                        &compositor,
                        &browser_conn,
                        &mut state_trace,
                    );
                } else if forward_result.activate {
                    dispatch_forward(
                        "mouse",
                        &forward_control,
                        back_route.as_ref(),
                        &compositor,
                        &browser_conn,
                        &mut state_trace,
                    );
                } else if refresh_result.activate {
                    dispatch_refresh(
                        "mouse",
                        &refresh_control,
                        back_route.as_ref(),
                        &compositor,
                        &browser_conn,
                        &mut state_trace,
                    );
                } else if back_hit
                    && matches!(mouse.kind, MouseEventKind::Up(MouseButton::Left))
                    && !back_actionable
                {
                    if let Some(trace) = state_trace.as_mut() {
                        let reason = if back_control.can_go_back {
                            "unavailable"
                        } else {
                            "disabled"
                        };
                        trace.write(
                            "navigation_action_blocked",
                            &[
                                ("action", "back".to_string()),
                                ("source", "mouse".to_string()),
                                ("reason", reason.to_string()),
                                ("tab_id", back_control.active_tab_id.to_string()),
                                ("can_go_back", back_control.can_go_back.to_string()),
                            ],
                        );
                    }
                } else if forward_hit
                    && matches!(mouse.kind, MouseEventKind::Up(MouseButton::Left))
                    && !forward_actionable
                {
                    if let Some(trace) = state_trace.as_mut() {
                        let reason = if forward_control.can_go_forward {
                            "unavailable"
                        } else {
                            "disabled"
                        };
                        trace.write(
                            "navigation_action_blocked",
                            &[
                                ("action", "forward".to_string()),
                                ("source", "mouse".to_string()),
                                ("reason", reason.to_string()),
                                ("tab_id", forward_control.active_tab_id.to_string()),
                                ("can_go_forward", forward_control.can_go_forward.to_string()),
                            ],
                        );
                    }
                } else if refresh_hit
                    && matches!(mouse.kind, MouseEventKind::Up(MouseButton::Left))
                    && !refresh_actionable
                {
                    if let Some(trace) = state_trace.as_mut() {
                        let reason = if refresh_control.can_refresh {
                            "unavailable"
                        } else {
                            "disabled"
                        };
                        trace.write(
                            "navigation_action_blocked",
                            &[
                                ("action", "refresh".to_string()),
                                ("source", "mouse".to_string()),
                                ("reason", reason.to_string()),
                                ("tab_id", refresh_control.active_tab_id.to_string()),
                                ("can_refresh", refresh_control.can_refresh.to_string()),
                            ],
                        );
                    }
                }
            }
            Ok(LoopEvent::Terminal(_)) => {
                // Resize, focus, paste, etc. — just redraw.
            }
            Ok(LoopEvent::Ipc(msg)) => {
                match msg {
                    ipc::CompositorMessage::ModeChanged { browsing } => {
                        mode = if browsing {
                            Mode::Browse
                        } else {
                            Mode::Control
                        };
                        if let Some(trace) = state_trace.as_mut() {
                            let mode_name = match mode {
                                Mode::Browse => "browse",
                                Mode::Control => "control",
                                Mode::Edit => "edit",
                                Mode::Command => "command",
                                Mode::Dialog => "dialog",
                                Mode::Auth => "auth",
                            };
                            trace.write(
                                "mode_changed",
                                &[
                                    ("source", "gui".to_string()),
                                    ("browsing", browsing.to_string()),
                                    ("mode", mode_name.to_string()),
                                ],
                            );
                        }
                    }
                    ipc::CompositorMessage::UrlChanged { url: new_url } => {
                        url = new_url;
                        if let Some(trace) = state_trace.as_mut() {
                            trace.write("url_changed", &[("url", url.clone())]);
                        }
                        // Mark editor_url stale so enter_edit re-syncs (Issue 26022712000658).
                        editor_url.clear();
                    }
                    ipc::CompositorMessage::LoadingState {
                        tab_id,
                        state,
                        _progress: progress,
                        navigation_request_id,
                    } => {
                        if let Some(trace) = state_trace.as_mut() {
                            trace.write(
                                "loading_state",
                                &[
                                    ("tab_id", tab_id.to_string()),
                                    ("state", state.clone()),
                                    ("progress", progress.to_string()),
                                    ("navigation_request_id", navigation_request_id.to_string()),
                                ],
                            );
                        }
                        let now = Instant::now();
                        if state == "loading"
                            && tab_id == current_tab_id
                            && navigation_request_id != 0
                            && refresh_control.can_refresh
                        {
                            refresh_animation.start(tab_id, navigation_request_id, now);
                        } else if matches!(state.as_str(), "done" | "error") {
                            refresh_animation.complete(tab_id, navigation_request_id, now);
                        }
                        let mut stdout = io::stdout();
                        let _ = match state.as_str() {
                            "loading" => {
                                if renderer_crash.is_some() {
                                    renderer_crash = None;
                                    renderer_crash_recovery_load_started = true;
                                }
                                loading_bar_active = true;
                                loading_bar_start = Some(Instant::now());
                                write!(stdout, "\x1b]9;4;3\x1b\\")
                            }
                            "progress" => Ok(()),
                            "done" => {
                                if renderer_crash_recovery_load_started {
                                    renderer_crash = None;
                                    renderer_crash_recovery_load_started = false;
                                }
                                loading_bar_active = false;
                                loading_bar_start = None;
                                // Loading stages (Issue 26040512000773).
                                for entry in loading_log.iter_mut() {
                                    if matches!(entry.0, LoadingStage::LoadingPage)
                                        && matches!(entry.1, StageStatus::InProgress)
                                    {
                                        entry.1 = StageStatus::Done;
                                    }
                                }
                                loading_log.push((LoadingStage::Ready, StageStatus::Done));
                                page_loaded = true;
                                page_loaded_at = Some(Instant::now());
                                write!(stdout, "\x1b]9;4;0\x1b\\")
                            }
                            "error" => {
                                loading_bar_active = false;
                                loading_bar_start = None;
                                write!(stdout, "\x1b]9;4;2\x1b\\")
                            }
                            _ => Ok(()),
                        };
                        let _ = stdout.flush();
                    }
                    ipc::CompositorMessage::NavigationState {
                        tab_id,
                        can_go_back,
                        can_go_forward,
                        can_refresh,
                    } => {
                        let back_applied = back_control.apply_navigation_state(tab_id, can_go_back);
                        let forward_applied =
                            forward_control.apply_navigation_state(tab_id, can_go_forward);
                        let refresh_applied =
                            refresh_control.apply_navigation_state(tab_id, can_refresh);
                        if refresh_applied && !can_refresh {
                            refresh_animation.stop();
                        }
                        if let Some(trace) = state_trace.as_mut() {
                            trace.write(
                                "navigation_state",
                                &[
                                    ("tab_id", tab_id.to_string()),
                                    ("current_tab_id", current_tab_id.to_string()),
                                    ("can_go_back", can_go_back.to_string()),
                                    ("can_go_forward", can_go_forward.to_string()),
                                    ("can_refresh", can_refresh.to_string()),
                                    ("back_applied", back_applied.to_string()),
                                    ("forward_applied", forward_applied.to_string()),
                                    ("refresh_applied", refresh_applied.to_string()),
                                ],
                            );
                        }
                    }
                    ipc::CompositorMessage::TitleChanged { title } => {
                        page_title = title;
                        if let Some(trace) = state_trace.as_mut() {
                            trace.write("title_changed", &[("title", page_title.clone())]);
                        }
                    }
                    ipc::CompositorMessage::TargetUrlChanged { url: new_target } => {
                        target_url = new_target;
                        if let Some(trace) = state_trace.as_mut() {
                            trace.write("target_url_changed", &[("url", target_url.clone())]);
                        }
                    }
                    ipc::CompositorMessage::ConsoleMessage {
                        tab_id,
                        level,
                        message,
                        line_no,
                        source_id,
                    } => {
                        if let Some(trace) = state_trace.as_mut() {
                            trace.write(
                                "console_message",
                                &[
                                    ("tab_id", tab_id.to_string()),
                                    ("level", level.clone()),
                                    ("message", message.clone()),
                                    ("line_no", line_no.to_string()),
                                    ("source_id", source_id.clone()),
                                ],
                            );
                        }
                        console_log.push(ConsoleLogEntry {
                            tab_id,
                            level,
                            message,
                            line_no,
                            source_id,
                        });
                        if console_log.len() > 100 {
                            let drain_count = console_log.len() - 100;
                            console_log.drain(0..drain_count);
                        }
                    }
                    ipc::CompositorMessage::RendererCrashed {
                        tab_id,
                        termination_status,
                        termination_status_code,
                        url,
                        can_reload,
                    } => {
                        back_control.renderer_crashed(tab_id);
                        forward_control.renderer_crashed(tab_id);
                        refresh_control.renderer_crashed(tab_id);
                        refresh_animation.stop();
                        loading_bar_active = false;
                        loading_bar_start = None;
                        renderer_crash_recovery_load_started = false;
                        if let Some(trace) = state_trace.as_mut() {
                            trace.write(
                                "renderer_crashed",
                                &[
                                    ("tab_id", tab_id.to_string()),
                                    ("status", termination_status.clone()),
                                    ("code", termination_status_code.to_string()),
                                    ("url", url.clone()),
                                    ("can_reload", can_reload.to_string()),
                                ],
                            );
                        }
                        renderer_crash = Some(RendererCrashState {
                            tab_id,
                            termination_status,
                            termination_status_code,
                            url,
                            can_reload,
                        });
                    }
                    ipc::CompositorMessage::BrowserReady {
                        tab_id,
                        browser_socket,
                        browser: resolved_browser,
                    } => {
                        reset_back_for_browser_ready(&mut back_control, &mut browser_conn, tab_id);
                        forward_control.browser_ready(tab_id);
                        refresh_control.browser_ready(tab_id);
                        refresh_animation.stop();
                        current_tab_id = tab_id;
                        if !resolved_browser.is_empty() {
                            browser = resolved_browser;
                        }
                        // Connect directly to the browser engine.
                        if let Some(conn) = ipc::BrowserConnection::connect(
                            &browser_socket,
                            tab_id,
                            browser_tx.clone(),
                        ) {
                            browser_conn = Some(conn);
                        }

                        // Loading stages (Issue 26040512000773).
                        browser_ready = true;
                        // Mark WaitingForBrowser as done.
                        for entry in loading_log.iter_mut() {
                            if matches!(entry.0, LoadingStage::WaitingForBrowser)
                                && matches!(entry.1, StageStatus::InProgress)
                            {
                                entry.1 = StageStatus::Done;
                            }
                        }
                        loading_log.push((LoadingStage::LoadingPage, StageStatus::InProgress));
                    }
                    ipc::CompositorMessage::JavaScriptDialogRequest {
                        tab_id,
                        request_id,
                        dialog_type,
                        origin_url,
                        message,
                        default_prompt_text,
                    } => {
                        let duplicate = pending_dialog
                            .as_ref()
                            .map(|dialog| {
                                dialog.tab_id == tab_id && dialog.request_id == request_id
                            })
                            .unwrap_or(false)
                            || handled_dialogs.contains(&(tab_id, request_id));
                        if !duplicate {
                            let previous_mode = mode.clone();
                            mode = Mode::Dialog;
                            if let Some(trace) = state_trace.as_mut() {
                                trace.write(
                                    "javascript_dialog_request",
                                    &[
                                        ("tab_id", tab_id.to_string()),
                                        ("request_id", request_id.to_string()),
                                        ("dialog_type", dialog_type.clone()),
                                        ("origin_url", origin_url.clone()),
                                        ("message", message.clone()),
                                        ("default_prompt_text", default_prompt_text.clone()),
                                    ],
                                );
                            }
                            pending_dialog = Some(PendingJsDialog {
                                tab_id,
                                request_id,
                                dialog_type,
                                origin_url,
                                message,
                                input: default_prompt_text.clone(),
                                default_prompt_text,
                                previous_mode,
                            });
                        }
                    }
                    ipc::CompositorMessage::HttpAuthRequest {
                        tab_id,
                        request_id,
                        url,
                        auth_scheme,
                        challenger,
                        realm,
                        is_proxy,
                        first_auth_attempt,
                    } => {
                        let duplicate = pending_auth
                            .as_ref()
                            .map(|auth| auth.tab_id == tab_id && auth.request_id == request_id)
                            .unwrap_or(false)
                            || handled_auth.contains(&(tab_id, request_id));
                        if !duplicate {
                            let previous_mode = mode.clone();
                            mode = Mode::Auth;
                            if let Some(trace) = state_trace.as_mut() {
                                trace.write(
                                    "http_auth_request",
                                    &[
                                        ("tab_id", tab_id.to_string()),
                                        ("request_id", request_id.to_string()),
                                        ("url", url.clone()),
                                        ("auth_scheme", auth_scheme.clone()),
                                        ("challenger", challenger.clone()),
                                        ("realm", realm.clone()),
                                        ("is_proxy", is_proxy.to_string()),
                                        ("first_auth_attempt", first_auth_attempt.to_string()),
                                    ],
                                );
                            }
                            pending_auth = Some(PendingHttpAuth {
                                tab_id,
                                request_id,
                                url,
                                auth_scheme,
                                challenger,
                                realm,
                                is_proxy,
                                first_auth_attempt,
                                username: String::new(),
                                password: String::new(),
                                field: AuthField::Username,
                                previous_mode,
                            });
                        }
                    }
                }
            }
            Err(_) => break,
        }

        // Safety timeout: clear stuck loading bar after 30 seconds (Issue 26022112000616).
        if loading_bar_active {
            if let Some(start) = loading_bar_start {
                if start.elapsed() >= LOADING_TIMEOUT {
                    let mut stdout = io::stdout();
                    let _ = write!(stdout, "\x1b]9;4;2\x1b\\");
                    let _ = stdout.flush();
                    std::thread::sleep(Duration::from_millis(500));
                    let _ = write!(stdout, "\x1b]9;4;0\x1b\\");
                    let _ = stdout.flush();
                    loading_bar_active = false;
                    loading_bar_start = None;
                }
            }
        }

        // Loading timeout: mark error if browser never connects (Issue 26040512000773).
        if !browser_ready {
            if let Some(start) = browser_wait_start {
                if start.elapsed() >= Duration::from_secs(120) {
                    for entry in loading_log.iter_mut() {
                        if matches!(entry.0, LoadingStage::WaitingForBrowser)
                            && matches!(entry.1, StageStatus::InProgress)
                        {
                            entry.1 =
                                StageStatus::Error("Timeout — is the browser installed?".into());
                        }
                    }
                    browser_wait_start = None; // Don't keep re-triggering.
                }
            }
        }
    }

    // Clear loading bar on exit (Issue 26022112000616).
    if loading_bar_active {
        let mut stdout = io::stdout();
        let _ = write!(stdout, "\x1b]9;4;0\x1b\\");
        let _ = stdout.flush();
    }
    // Restore terminal. The compositor connection drops here, which closes
    // the XPC connection and triggers overlay cleanup.
    disable_raw_mode()?;
    write!(terminal.backend_mut(), "{DISABLE_ANY_MOUSE_MOTION}")?;
    if use_alternate_screen {
        execute!(
            terminal.backend_mut(),
            DisableMouseCapture,
            LeaveAlternateScreen
        )?;
    } else {
        execute!(terminal.backend_mut(), DisableMouseCapture)?;
    }
    Ok(())
}

fn is_version_arg(arg: String) -> bool {
    arg == "--version" || arg == "-V"
}

/// Resolve bare input to a URL or file:// path (Issue 26030112000693).
///
/// Returns `None` if the input is not recognizable as a URL, file, or command.
/// Callers should show an error for `None`.
fn resolve_input(input: &str) -> Option<String> {
    let trimmed = input.trim();

    // Step 1: Has a scheme — use as-is.
    if trimmed.contains("://") {
        return Some(trimmed.to_string());
    }

    // Step 3: Explicit file paths (/, ./, ../).
    if trimmed.starts_with('/') || trimmed.starts_with("./") || trimmed.starts_with("../") {
        if let Ok(absolute) = std::fs::canonicalize(trimmed) {
            return Some(format!("file://{}", absolute.display()));
        }
    }

    // Step 4: Contains ":" — treat as host:port URL.
    if trimmed.contains(':') {
        let host = trimmed.split(':').next().unwrap_or(trimmed);
        if host.ends_with("localhost") || host.contains("localhost") {
            return Some(format!("http://{trimmed}"));
        }
        return Some(format!("https://{trimmed}"));
    }

    // Step 5: File exists — open as file.
    if let Ok(absolute) = std::fs::canonicalize(trimmed) {
        return Some(format!("file://{}", absolute.display()));
    }

    // Step 6: URL fallback (has a dot — looks like a domain).
    if trimmed.contains('.') {
        let host = trimmed.split('/').next().unwrap_or(trimmed);
        if host.ends_with("localhost") {
            return Some(format!("http://{trimmed}"));
        }
        return Some(format!("https://{trimmed}"));
    }

    // Step 7: Nothing matched.
    None
}

fn shell_quote_arg(value: &str) -> String {
    if value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/' | b':'))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn viewport_identity_label(
    browser_label: &str,
    profile: &str,
    is_devtools: bool,
    inspected_tab_id: i64,
    current_tab_id: i64,
) -> String {
    if is_devtools {
        format!("{}/{}#{}", browser_label, profile, inspected_tab_id)
    } else if current_tab_id > 0 {
        format!("{}/{}#{}", browser_label, profile, current_tab_id)
    } else {
        format!("{}/{}#loading", browser_label, profile)
    }
}

fn browser_display_label(browser: &str) -> &str {
    let basename = browser.rsplit('/').next().unwrap_or(browser);
    match basename {
        "chromium" | "ah-chromiumd" => "chromium",
        "webkit" | "ah-webkitd" => "webkit",
        "ladybird" | "ah-ladybirdd" => "ladybird",
        "gecko" | "ah-geckod" => "gecko",
        _ => basename,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BrowserLayout {
    back_area: Rect,
    forward_area: Rect,
    refresh_area: Rect,
    viewport_area: Rect,
    url_area: Rect,
    status_area: Rect,
}

fn browser_layout(area: Rect, viewport_height_override: Option<u16>) -> BrowserLayout {
    let layout = if let Some(rows) = viewport_height_override {
        let available = area.height.saturating_sub(4);
        let viewport_height = rows.saturating_add(2).clamp(1, available.max(1));
        Layout::vertical([
            Constraint::Length(3),               // URL bar
            Constraint::Length(1),               // Status bar
            Constraint::Length(viewport_height), // Viewport override
            Constraint::Min(0),                  // Filler
        ])
        .split(area)
    } else {
        Layout::vertical([
            Constraint::Length(3), // URL bar (1 line + top/bottom border)
            Constraint::Length(1), // Status bar
            Constraint::Min(1),    // Viewport (fill remaining)
        ])
        .split(area)
    };

    let top = layout[0];
    let nav_budget = top.width.saturating_sub(1).min(15);
    let back_width = nav_budget.saturating_add(2) / 3;
    let forward_width = nav_budget.saturating_add(1) / 3;
    let refresh_width = nav_budget / 3;
    let back_area = Rect::new(top.x, top.y, back_width, top.height);
    let forward_area = Rect::new(
        top.x.saturating_add(back_width),
        top.y,
        forward_width,
        top.height,
    );
    let refresh_area = Rect::new(
        top.x
            .saturating_add(back_width)
            .saturating_add(forward_width),
        top.y,
        refresh_width,
        top.height,
    );
    let chrome_width = back_width
        .saturating_add(forward_width)
        .saturating_add(refresh_width);
    let url_area = Rect::new(
        top.x.saturating_add(chrome_width),
        top.y,
        top.width.saturating_sub(chrome_width),
        top.height,
    );

    BrowserLayout {
        back_area,
        forward_area,
        refresh_area,
        url_area,
        status_area: layout[1],
        viewport_area: layout[2],
    }
}

fn viewport_inner_rect(viewport_area: Rect) -> Rect {
    Block::default().borders(Borders::ALL).inner(viewport_area)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct UiGeometry {
    viewport: Rect,
    back: Rect,
    forward: Rect,
    refresh: Rect,
    url: Rect,
}

/// Render the UI and return browser overlay plus Back hit geometry.
fn ui(
    frame: &mut Frame,
    url: &str,
    profile: &str,
    mode: &Mode,
    editor_state: &mut EditorState,
    cmd_state: &mut EditorState,
    page_title: &str,
    is_devtools: bool,
    inspected_tab_id: i64,
    current_tab_id: i64,
    command_error: &Option<String>,
    browser_label: &str,
    target_url: &str,
    pending_dialog: &Option<PendingJsDialog>,
    pending_auth: &Option<PendingHttpAuth>,
    copy_url_feedback_until: Option<Instant>,
    loading_log: &[(LoadingStage, StageStatus)],
    renderer_crash: &Option<RendererCrashState>,
    browser_ready: bool,
    browser_wait_start: Option<Instant>,
    viewport_height_override: Option<u16>,
    back_control: &BackControlState,
    forward_control: &ForwardControlState,
    refresh_control: &RefreshControlState,
    refresh_animation: &RefreshAnimation,
    now: Instant,
    back_route_available: bool,
) -> UiGeometry {
    // Paint full background.
    frame.render_widget(
        Block::default().style(Style::default().bg(BG)),
        frame.area(),
    );

    let layout = browser_layout(frame.area(), viewport_height_override);
    let viewport_area = layout.viewport_area;
    let back_area = layout.back_area;
    let forward_area = layout.forward_area;
    let refresh_area = layout.refresh_area;
    let url_area = layout.url_area;
    let status_area = layout.status_area;

    // Border colors based on mode.
    let (url_border, viewport_border) = match mode {
        Mode::Browse => (BORDER, CYAN),
        Mode::Control => (CYAN, BORDER),
        Mode::Edit => (PURPLE, BORDER),
        Mode::Command => (YELLOW, BORDER),
        Mode::Dialog => (YELLOW, YELLOW),
        Mode::Auth => (YELLOW, YELLOW),
    };

    render_back_button(frame, back_area, back_control, back_route_available);
    render_forward_button(frame, forward_area, forward_control, back_route_available);
    render_refresh_button(
        frame,
        refresh_area,
        refresh_control,
        refresh_animation,
        now,
        back_route_available,
    );

    // URL bar / Command bar (Issue 26022712000659).
    if *mode == Mode::Command {
        // Submode indicator in top-right of command bar.
        let submode_text = match cmd_state.mode {
            EditorMode::Normal => "\u{EA85} NORMAL",
            EditorMode::Insert => "\u{F040} INSERT",
            EditorMode::Visual => "\u{F14A} VISUAL",
            EditorMode::Search => "\u{F002} SEARCH",
        };
        let sc = submode_color(&cmd_state.mode);
        let submode_label =
            Line::from(vec![Span::raw(submode_text).style(Style::default().fg(sc))]);
        // Red border on error, yellow otherwise (Issue 26030112000690).
        let border_color = if command_error.is_some() {
            RED
        } else {
            url_border
        };
        let cmd_title = Line::from(vec![
            Span::raw("COMMAND").style(Style::default().fg(border_color))
        ]);
        let mut cmd_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color).bg(BG))
            .title_style(Style::default().fg(border_color))
            .title_top(cmd_title)
            .title_top(submode_label.alignment(Alignment::Right))
            .style(Style::default().bg(BG));
        if let Some(ref err) = command_error {
            cmd_block =
                cmd_block.title_bottom(Line::from(err.as_str()).style(Style::default().fg(RED)));
        }
        let cmd_inner = cmd_block.inner(url_area);
        frame.render_widget(cmd_block, url_area);

        // Split inner area: ":" prefix + editor.
        let cmd_layout =
            Layout::horizontal([Constraint::Length(1), Constraint::Min(0)]).split(cmd_inner);
        frame.render_widget(
            Paragraph::new(":").style(Style::default().fg(YELLOW).bg(BG)),
            cmd_layout[0],
        );
        let theme = EditorTheme::default()
            .base(Style::default().fg(FG).bg(BG))
            .cursor_style(Style::default().fg(BG).bg(FG))
            .selection_style(Style::default().fg(FG).bg(SELECTION))
            .hide_status_line();
        frame.render_widget(
            EditorView::new(cmd_state).theme(theme).wrap(false),
            cmd_layout[1],
        );
    } else if *mode == Mode::Edit {
        // Submode indicator in top-right of URL bar (Issue 26022712000658).
        let submode_text = match editor_state.mode {
            EditorMode::Normal => "\u{EA85} NORMAL",
            EditorMode::Insert => "\u{F040} INSERT",
            EditorMode::Visual => "\u{F14A} VISUAL",
            EditorMode::Search => "\u{F002} SEARCH",
        };
        let sc = submode_color(&editor_state.mode);
        let submode_label =
            Line::from(vec![Span::raw(submode_text).style(Style::default().fg(sc))]);
        let url_title = Line::from(vec![Span::raw("URL").style(Style::default().fg(url_border))]);
        let theme = EditorTheme::default()
            .base(Style::default().fg(FG).bg(BG))
            .cursor_style(Style::default().fg(BG).bg(FG))
            .selection_style(Style::default().fg(FG).bg(SELECTION))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(url_border).bg(BG))
                    .title_style(Style::default().fg(url_border))
                    .title_top(url_title)
                    .title_top(submode_label.alignment(Alignment::Right))
                    .style(Style::default().bg(BG)),
            )
            .hide_status_line();
        frame.render_widget(
            EditorView::new(editor_state).theme(theme).wrap(false),
            url_area,
        );
    } else {
        let url_title = Line::from(vec![Span::raw("URL").style(Style::default().fg(url_border))]);
        let url_bar = Paragraph::new(url).style(Style::default().fg(FG)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(url_border).bg(BG))
                .title_style(Style::default().fg(url_border))
                .title_top(url_title)
                .style(Style::default().bg(BG)),
        );
        frame.render_widget(url_bar, url_area);
    }

    // Viewport.
    let identity_label = viewport_identity_label(
        browser_label,
        profile,
        is_devtools,
        inspected_tab_id,
        current_tab_id,
    );

    let viewport_title = if is_devtools {
        format!("DevTools \u{00B7} {}", identity_label)
    } else if page_title.is_empty() {
        "Viewport".to_string()
    } else {
        page_title.to_string()
    };
    let engine_label = Line::from(vec![
        Span::raw("\u{F007} ").style(Style::default().fg(COMMENT)),
        Span::raw(identity_label).style(Style::default().fg(DIM)),
    ]);
    let mut viewport_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(viewport_title)
        .title_bottom(engine_label.alignment(Alignment::Right))
        .border_style(Style::default().fg(viewport_border).bg(BG))
        .title_style(Style::default().fg(viewport_border))
        .style(Style::default().bg(BG));
    if !target_url.is_empty() {
        let hover_label = Line::from(Span::raw(target_url).style(Style::default().fg(DIM)));
        viewport_block = viewport_block.title_bottom(hover_label);
    }
    let inner = viewport_inner_rect(viewport_area);

    if let Some(dialog) = pending_dialog {
        let prompt_line = match dialog.dialog_type.as_str() {
            "alert" => "Enter accepts, Esc cancels".to_string(),
            "confirm" => "Enter/y accepts, n/Esc cancels".to_string(),
            "prompt" => format!(
                "Input: {}{}",
                dialog.input,
                if dialog.default_prompt_text.is_empty() {
                    ""
                } else {
                    " "
                }
            ),
            "beforeunload" => "Enter/y proceeds, n/Esc stays".to_string(),
            _ => "Enter accepts, Esc cancels".to_string(),
        };
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  JavaScript ").style(Style::default().fg(YELLOW).bg(BG)),
                Span::raw(dialog.dialog_type.as_str()).style(Style::default().fg(CYAN).bg(BG)),
            ]),
            Line::from(
                Span::raw(format!("  {}", dialog.origin_url)).style(Style::default().fg(DIM)),
            ),
            Line::from(""),
            Line::from(Span::raw(format!("  {}", dialog.message)).style(Style::default().fg(FG))),
            Line::from(""),
            Line::from(Span::raw(format!("  {}", prompt_line)).style(Style::default().fg(COMMENT))),
        ];
        let dialog_widget = Paragraph::new(lines)
            .style(Style::default().fg(FG).bg(BG))
            .block(viewport_block);
        frame.render_widget(dialog_widget, viewport_area);
    } else if let Some(auth) = pending_auth {
        let password_mask = "*".repeat(auth.password.chars().count());
        let username_style = if auth.field == AuthField::Username {
            Style::default().fg(CYAN).bg(BG)
        } else {
            Style::default().fg(FG).bg(BG)
        };
        let password_style = if auth.field == AuthField::Password {
            Style::default().fg(CYAN).bg(BG)
        } else {
            Style::default().fg(FG).bg(BG)
        };
        let retry = if auth.first_auth_attempt {
            ""
        } else {
            " retry"
        };
        let target = if auth.is_proxy { "proxy" } else { "origin" };
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  HTTP Auth").style(Style::default().fg(YELLOW).bg(BG)),
                Span::raw(retry).style(Style::default().fg(RED).bg(BG)),
            ]),
            Line::from(Span::raw(format!("  {}", auth.url)).style(Style::default().fg(DIM).bg(BG))),
            Line::from(Span::raw(format!(
                "  {} {} {} realm={}",
                target, auth.auth_scheme, auth.challenger, auth.realm
            ))),
            Line::from(""),
            Line::from(vec![
                Span::raw("  Username: ").style(Style::default().fg(COMMENT).bg(BG)),
                Span::raw(auth.username.as_str()).style(username_style),
            ]),
            Line::from(vec![
                Span::raw("  Password: ").style(Style::default().fg(COMMENT).bg(BG)),
                Span::raw(password_mask).style(password_style),
            ]),
            Line::from(""),
            Line::from(
                Span::raw("  Enter advances/submits, Tab switches fields, Esc cancels")
                    .style(Style::default().fg(COMMENT).bg(BG)),
            ),
        ];
        let auth_widget = Paragraph::new(lines)
            .style(Style::default().fg(FG).bg(BG))
            .block(viewport_block);
        frame.render_widget(auth_widget, viewport_area);
    } else if let Some(crash) = renderer_crash {
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::raw("  ").style(Style::default()),
            Span::raw("Renderer crashed").style(Style::default().fg(RED)),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(
            Span::raw("  Press Cmd+R to reload, or enter a new URL.")
                .style(Style::default().fg(FG)),
        ));
        let detail = if crash.url.is_empty() {
            format!(
                "  status={} code={} tab={}",
                crash.termination_status, crash.termination_status_code, crash.tab_id
            )
        } else {
            format!(
                "  status={} code={} tab={} url={}",
                crash.termination_status, crash.termination_status_code, crash.tab_id, crash.url
            )
        };
        lines.push(Line::from(
            Span::raw(detail).style(Style::default().fg(COMMENT)),
        ));
        if !crash.can_reload {
            lines.push(Line::from(
                Span::raw("  Reload is not available for this tab.")
                    .style(Style::default().fg(YELLOW)),
            ));
        }
        let crash_widget = Paragraph::new(lines)
            .style(Style::default().fg(FG).bg(BG))
            .block(viewport_block);
        frame.render_widget(crash_widget, viewport_area);
    } else if !browser_ready && !loading_log.is_empty() {
        // Render loading log (Issue 26040512000773).
        const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let spinner_frame = browser_wait_start
            .map(|s| (s.elapsed().as_millis() / 100) as usize % SPINNER.len())
            .unwrap_or(0);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from("")); // top padding

        for (stage, status) in loading_log {
            let (icon, color) = match status {
                StageStatus::Done => ("✓", GREEN),
                StageStatus::InProgress => (SPINNER[spinner_frame], CYAN),
                StageStatus::Error(_) => ("✗", RED),
            };
            let mut spans = vec![
                Span::raw("  ").style(Style::default()),
                Span::raw(icon).style(Style::default().fg(color)),
                Span::raw(" ").style(Style::default()),
            ];
            match status {
                StageStatus::Error(msg) => {
                    spans.push(Span::raw(msg.clone()).style(Style::default().fg(color)));
                }
                _ => {
                    let mut label = stage.label().to_string();
                    // Show elapsed time for WaitingForBrowser.
                    if matches!(stage, LoadingStage::WaitingForBrowser)
                        && matches!(status, StageStatus::InProgress)
                    {
                        label = format!("Waiting for {browser_label}");
                        if let Some(start) = browser_wait_start {
                            let elapsed = start.elapsed().as_secs();
                            label = format!("{} ({}s)", label, elapsed);
                        }
                    }
                    spans.push(Span::raw(label).style(Style::default().fg(color)));
                }
            }
            lines.push(Line::from(spans));
        }

        // Warnings based on elapsed time.
        if let Some(start) = browser_wait_start {
            let elapsed = start.elapsed().as_secs();
            if elapsed < 120 {
                lines.push(Line::from(""));
                lines.push(Line::from(
                    Span::raw(
                        "    The first time you load a web browser it may take up to two minutes.",
                    )
                    .style(Style::default().fg(COMMENT)),
                ));
            } else {
                // This is handled in the timeout below, but show inline too.
            }
        }

        let loading_widget = Paragraph::new(lines)
            .style(Style::default().fg(FG).bg(BG))
            .block(viewport_block);
        frame.render_widget(loading_widget, viewport_area);
    } else {
        let viewport = Paragraph::new("")
            .style(Style::default().fg(FG).bg(BG))
            .block(viewport_block);
        frame.render_widget(viewport, viewport_area);
    }

    // Status bar.
    let status_layout = Layout::horizontal([
        Constraint::Fill(1),    // Key hints (left)
        Constraint::Length(14), // Mode label (right)
    ])
    .split(status_area);

    let d = Style::default().fg(DIM).bg(BG);
    let f = Style::default().fg(FG).bg(BG);

    let hints = if let Some(crash) = renderer_crash.as_ref() {
        Line::from(vec![
            Span::styled("renderer crashed ", Style::default().fg(RED).bg(BG)),
            Span::styled(
                format!(
                    "{} code={} #{}",
                    crash.termination_status, crash.termination_status_code, crash.tab_id
                ),
                d,
            ),
        ])
    } else {
        match mode {
            Mode::Browse => Line::from(vec![
                Span::styled("\u{2318}[ ", f),
                Span::styled("back  ", d),
                Span::styled("\u{2318}] ", f),
                Span::styled("fwd  ", d),
                Span::styled("\u{2318}r ", f),
                Span::styled("reload  ", d),
                Span::styled("esc ", f),
                Span::styled("control", d),
            ]),
            Mode::Control => {
                if copy_url_feedback_until
                    .map(|until| Instant::now() < until)
                    .unwrap_or(false)
                {
                    Line::from(vec![
                        Span::styled("url copied ", Style::default().fg(GREEN).bg(BG)),
                        Span::styled("\u{2318}c", d),
                    ])
                } else if is_devtools {
                    // DevTools: no edit keys (Issue 26030112000687).
                    Line::from(vec![
                        Span::styled(":q\u{23CE} ", f),
                        Span::styled("quit  ", d),
                        Span::styled("\u{23CE} ", f),
                        Span::styled("browse", d),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled(":q\u{23CE} ", f),
                        Span::styled("quit  ", d),
                        Span::styled("i ", f),
                        Span::styled("edit url  ", d),
                        Span::styled("\u{23CE} ", f),
                        Span::styled("browse", d),
                    ])
                }
            }
            Mode::Edit => Line::from(vec![
                Span::styled("\u{23CE} ", f),
                Span::styled("navigate  ", d),
                Span::styled("esc ", f),
                Span::styled("control", d),
            ]),
            Mode::Command => Line::from(vec![
                Span::styled("\u{23CE} ", f),
                Span::styled("execute  ", d),
                Span::styled("esc ", f),
                Span::styled("control", d),
            ]),
            Mode::Dialog => Line::from(vec![
                Span::styled("\u{23CE}/y ", f),
                Span::styled("accept  ", d),
                Span::styled("n/esc ", f),
                Span::styled("cancel", d),
            ]),
            Mode::Auth => Line::from(vec![
                Span::styled("\u{23CE} ", f),
                Span::styled("next/submit  ", d),
                Span::styled("tab ", f),
                Span::styled("field  ", d),
                Span::styled("esc ", f),
                Span::styled("cancel", d),
            ]),
        }
    };

    let label = match mode {
        Mode::Browse => "\u{F059F} BROWSE".to_string(),
        Mode::Control => "\u{F11C} CONTROL".to_string(),
        Mode::Edit => "\u{F044} EDIT".to_string(),
        Mode::Command => "\u{F120} COMMAND".to_string(),
        Mode::Dialog => "\u{F27A} DIALOG".to_string(),
        Mode::Auth => "\u{F023} AUTH".to_string(),
    };

    let hints_widget = Paragraph::new(hints);
    frame.render_widget(hints_widget, status_layout[0]);

    let label_widget = Paragraph::new(label)
        .alignment(Alignment::Right)
        .style(Style::default().fg(FG).bg(BG));
    frame.render_widget(label_widget, status_layout[1]);

    UiGeometry {
        viewport: inner,
        back: back_area,
        forward: forward_area,
        refresh: refresh_area,
        url: url_area,
    }
}

/// Shared chrome border geometry: always rounded. Interaction/disabled state
/// is color (and fill) only — never Plain/Double/Thick shape changes.
fn chrome_border_type() -> BorderType {
    BorderType::Rounded
}

fn nav_button_colors(actionable: bool, hovered: bool, pressed: bool) -> (Color, Color, Color) {
    if !actionable {
        (DIM, BG, BORDER)
    } else if pressed {
        (BG, CYAN, CYAN)
    } else if hovered {
        (FG, SELECTION, CYAN)
    } else {
        (FG, BG, CYAN)
    }
}

fn render_back_button(
    frame: &mut Frame,
    area: Rect,
    state: &BackControlState,
    route_available: bool,
) {
    let actionable = state.can_go_back && state.active_tab_id > 0 && route_available;
    let hovered = actionable && state.hovered;
    let pressed = actionable && state.pressed.is_some();
    let (fg, bg, border) = nav_button_colors(actionable, hovered, pressed);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(chrome_border_type())
        .border_style(Style::default().fg(border).bg(bg))
        .style(Style::default().fg(fg).bg(bg));
    let button = Paragraph::new(BACK_SYMBOL)
        .alignment(Alignment::Center)
        .style(Style::default().fg(fg).bg(bg))
        .block(block);
    frame.render_widget(button, area);
}

fn render_forward_button(
    frame: &mut Frame,
    area: Rect,
    state: &ForwardControlState,
    route_available: bool,
) {
    let actionable = state.can_go_forward && state.active_tab_id > 0 && route_available;
    let hovered = actionable && state.hovered;
    let pressed = actionable && state.pressed.is_some();
    let (fg, bg, border) = nav_button_colors(actionable, hovered, pressed);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(chrome_border_type())
        .border_style(Style::default().fg(border).bg(bg))
        .style(Style::default().fg(fg).bg(bg));
    let button = Paragraph::new(FORWARD_SYMBOL)
        .alignment(Alignment::Center)
        .style(Style::default().fg(fg).bg(bg))
        .block(block);
    frame.render_widget(button, area);
}

fn render_refresh_button(
    frame: &mut Frame,
    area: Rect,
    state: &RefreshControlState,
    animation: &RefreshAnimation,
    now: Instant,
    route_available: bool,
) {
    let actionable = state.can_refresh && state.active_tab_id > 0 && route_available;
    let hovered = actionable && state.hovered;
    let pressed = actionable && state.pressed.is_some();
    let (fg, bg, border) = nav_button_colors(actionable, hovered, pressed);
    let symbol = refresh_symbol(animation, now);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(chrome_border_type())
        .border_style(Style::default().fg(border).bg(bg))
        .style(Style::default().fg(fg).bg(bg));
    let button = Paragraph::new(symbol)
        .alignment(Alignment::Center)
        .style(Style::default().fg(fg).bg(bg))
        .block(block);
    frame.render_widget(button, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;

    #[test]
    fn version_flags_are_intercepted_before_runtime_setup() {
        assert!(is_version_arg("--version".to_string()));
        assert!(is_version_arg("-V".to_string()));
        assert!(!is_version_arg("--help".to_string()));
    }

    /// Issue 26071922533901 Exp 1: shipped startup mode is Browse (not Control).
    #[test]
    fn initial_mode_is_browse_and_advertises_browsing() {
        assert_eq!(initial_mode(), Mode::Browse);
        assert!(mode_is_browsing(&initial_mode()));
        assert!(!mode_is_browsing(&Mode::Control));
        assert!(!mode_is_browsing(&Mode::Edit));
    }

    #[test]
    fn render_trace_rectangles_are_exact_and_machine_readable() {
        assert_eq!(trace_rect(Rect::new(2, 3, 5, 7)), "2,3,5,7");
        assert_eq!(BACK_SYMBOL, "←");
        assert_eq!(REFRESH_IDLE_SYMBOL, "\u{E348}");
        assert_eq!(ENABLE_ANY_MOUSE_MOTION, "\x1b[?1003h");
        assert_eq!(DISABLE_ANY_MOUSE_MOTION, "\x1b[?1003l");
    }

    struct RenderProbe {
        viewport: Rect,
        back: Rect,
        forward: Rect,
        refresh: Rect,
        url: Rect,
        capture: String,
        buffer: Buffer,
    }

    fn render_probe(
        mode: Mode,
        width: u16,
        height: u16,
        override_rows: Option<u16>,
    ) -> RenderProbe {
        render_probe_with_back(
            mode,
            width,
            height,
            override_rows,
            BackControlState::default(),
            false,
        )
    }

    fn render_probe_with_back(
        mode: Mode,
        width: u16,
        height: u16,
        override_rows: Option<u16>,
        back_control: BackControlState,
        back_route_available: bool,
    ) -> RenderProbe {
        render_probe_with_navigation(
            mode,
            width,
            height,
            override_rows,
            back_control,
            ForwardControlState::default(),
            RefreshControlState::default(),
            RefreshAnimation::default(),
            Instant::now(),
            back_route_available,
        )
    }

    fn render_probe_with_navigation(
        mode: Mode,
        width: u16,
        height: u16,
        override_rows: Option<u16>,
        back_control: BackControlState,
        forward_control: ForwardControlState,
        refresh_control: RefreshControlState,
        refresh_animation: RefreshAnimation,
        now: Instant,
        route_available: bool,
    ) -> RenderProbe {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut editor_state = EditorState::new(Lines::from("https://example.test"));
        let mut cmd_state = EditorState::new(Lines::from("open https://example.test"));
        let mut viewport = Rect::default();
        let mut back = Rect::default();
        let mut forward = Rect::default();
        let mut refresh = Rect::default();
        let mut url = Rect::default();

        terminal
            .draw(|frame| {
                let geometry = ui(
                    frame,
                    "https://example.test",
                    "default",
                    &mode,
                    &mut editor_state,
                    &mut cmd_state,
                    "Viewport",
                    false,
                    -1,
                    1,
                    &None,
                    "chromium",
                    "",
                    &None,
                    &None,
                    None,
                    &[],
                    &None,
                    true,
                    None,
                    override_rows,
                    &back_control,
                    &forward_control,
                    &refresh_control,
                    &refresh_animation,
                    now,
                    route_available,
                );
                viewport = geometry.viewport;
                back = geometry.back;
                forward = geometry.forward;
                refresh = geometry.refresh;
                url = geometry.url;
            })
            .unwrap();

        RenderProbe {
            viewport,
            back,
            forward,
            refresh,
            url,
            capture: numbered_buffer_capture(terminal.backend().buffer()),
            buffer: terminal.backend().buffer().clone(),
        }
    }

    fn render_loading_probe(browser_label: &str) -> RenderProbe {
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut editor_state = EditorState::new(Lines::from("https://example.test"));
        let mut cmd_state = EditorState::new(Lines::from("open https://example.test"));
        let mut viewport = Rect::default();
        let mut back = Rect::default();
        let mut forward = Rect::default();
        let mut refresh = Rect::default();
        let mut url = Rect::default();
        let loading_log = vec![
            (LoadingStage::ConnectingToGui, StageStatus::Done),
            (LoadingStage::StartingBrowser, StageStatus::Done),
            (LoadingStage::WaitingForBrowser, StageStatus::InProgress),
        ];

        terminal
            .draw(|frame| {
                let geometry = ui(
                    frame,
                    "https://example.test",
                    "default",
                    &Mode::Control,
                    &mut editor_state,
                    &mut cmd_state,
                    "Viewport",
                    false,
                    -1,
                    0,
                    &None,
                    browser_label,
                    "",
                    &None,
                    &None,
                    None,
                    &loading_log,
                    &None,
                    false,
                    Some(Instant::now()),
                    None,
                    &BackControlState::default(),
                    &ForwardControlState::default(),
                    &RefreshControlState::default(),
                    &RefreshAnimation::default(),
                    Instant::now(),
                    false,
                );
                viewport = geometry.viewport;
                back = geometry.back;
                forward = geometry.forward;
                refresh = geometry.refresh;
                url = geometry.url;
            })
            .unwrap();

        RenderProbe {
            viewport,
            back,
            forward,
            refresh,
            url,
            capture: numbered_buffer_capture(terminal.backend().buffer()),
            buffer: terminal.backend().buffer().clone(),
        }
    }

    fn numbered_buffer_capture(buffer: &Buffer) -> String {
        let mut out = String::new();
        for y in 0..buffer.area.height {
            let mut row = String::new();
            for x in 0..buffer.area.width {
                row.push_str(buffer[(x, y)].symbol());
            }
            out.push_str(&format!("{y:02}: {row}\n"));
        }
        out
    }

    fn row_containing(capture: &str, needle: &str) -> u16 {
        capture
            .lines()
            .find_map(|line| {
                line.contains(needle)
                    .then(|| line[..2].parse::<u16>().unwrap())
            })
            .unwrap_or_else(|| panic!("missing {needle:?} in capture:\n{capture}"))
    }

    fn assert_controls_before_viewport(capture: &str, chrome_marker: &str, status_marker: &str) {
        let chrome_row = row_containing(capture, chrome_marker);
        let status_row = row_containing(capture, status_marker);
        let viewport_row = row_containing(capture, "Viewport");
        assert!(
            chrome_row < status_row,
            "chrome row should precede status row\n{capture}"
        );
        assert!(
            status_row < viewport_row,
            "status row should precede viewport row\n{capture}"
        );
    }

    fn assert_layout_invariants(mode: Mode, area: Rect, override_rows: Option<u16>) {
        let layout = browser_layout(area, override_rows);
        let inner = viewport_inner_rect(layout.viewport_area);
        assert!(
            layout.url_area.y < layout.status_area.y,
            "URL area should be above status area: {layout:?}"
        );
        assert!(
            layout.status_area.y < layout.viewport_area.y,
            "status area should be above viewport area: {layout:?}"
        );
        assert!(
            inner.y > layout.status_area.y,
            "inner viewport should start below controls: inner={inner:?} layout={layout:?}"
        );
        assert!(
            inner.width > 0 && inner.height > 0,
            "inner viewport should not collapse: inner={inner:?} layout={layout:?}"
        );

        let rendered = render_probe(mode, area.width, area.height, override_rows);
        assert_eq!(
            rendered.viewport, inner,
            "ui() return value must be the rect sent as overlay geometry"
        );
    }

    fn assert_devtools_command(input: &str, expected_direction: &str) {
        match dispatch(input) {
            CommandResult::DevTools(direction) => assert_eq!(direction, expected_direction),
            _ => panic!("{input:?} did not dispatch DevTools"),
        }
    }

    fn assert_dark_command(input: &str, expected: DarkAction) {
        match (dispatch(input), expected) {
            (CommandResult::Dark(DarkAction::Toggle), DarkAction::Toggle)
            | (CommandResult::Dark(DarkAction::On), DarkAction::On)
            | (CommandResult::Dark(DarkAction::Off), DarkAction::Off)
            | (CommandResult::Dark(DarkAction::System), DarkAction::System) => {}
            _ => panic!("{input:?} did not dispatch expected dark command"),
        }
    }

    fn find_cell(buffer: &Buffer, symbol: &str) -> (u16, u16) {
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                if buffer[(x, y)].symbol() == symbol {
                    return (x, y);
                }
            }
        }
        panic!(
            "missing symbol {symbol:?} in buffer:\n{}",
            numbered_buffer_capture(buffer)
        );
    }

    fn test_mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::empty(),
        }
    }

    fn enabled_back_state() -> BackControlState {
        BackControlState {
            active_tab_id: 7,
            can_go_back: true,
            hovered: false,
            pressed: None,
        }
    }

    fn enabled_forward_state() -> ForwardControlState {
        ForwardControlState {
            active_tab_id: 7,
            can_go_forward: true,
            hovered: false,
            pressed: None,
        }
    }

    fn enabled_refresh_state() -> RefreshControlState {
        RefreshControlState {
            active_tab_id: 7,
            can_refresh: true,
            hovered: false,
            pressed: None,
        }
    }

    fn compositor_route() -> BackRoute {
        BackRoute::Compositor("pane-7".to_string())
    }

    #[test]
    fn command_aliases_follow_current_policy() {
        assert_devtools_command("dev", "right");
        assert_devtools_command("devtools", "right");

        assert!(matches!(dispatch("de"), CommandResult::None));
        assert!(matches!(dispatch("da"), CommandResult::None));

        assert_dark_command("dark", DarkAction::Toggle);
    }

    #[test]
    fn devtools_preserves_full_and_shorthand_directions() {
        for (input, expected) in [
            ("devtools right", "right"),
            ("devtools down", "down"),
            ("devtools left", "left"),
            ("devtools up", "up"),
            ("dev r", "right"),
            ("dev d", "down"),
            ("dev l", "left"),
            ("dev u", "up"),
        ] {
            assert_devtools_command(input, expected);
        }
    }

    #[test]
    fn dark_preserves_subcommand_shorthands() {
        assert_dark_command("dark on", DarkAction::On);
        assert_dark_command("dark y", DarkAction::On);
        assert_dark_command("dark off", DarkAction::Off);
        assert_dark_command("dark n", DarkAction::Off);
        assert_dark_command("dark system", DarkAction::System);
        assert_dark_command("dark s", DarkAction::System);
    }

    #[test]
    fn parses_macos_interface_style() {
        assert_eq!(parse_macos_interface_style_dark("Dark\n"), Some(true));
        assert_eq!(parse_macos_interface_style_dark("dark"), Some(true));
        assert_eq!(parse_macos_interface_style_dark("Light\n"), Some(false));
        assert_eq!(parse_macos_interface_style_dark(""), None);
        assert_eq!(parse_macos_interface_style_dark("Graphite"), None);
    }

    #[test]
    fn maps_missing_macos_interface_style_to_light() {
        assert_eq!(macos_defaults_color_scheme(false, b""), Some(false));
        assert_eq!(macos_defaults_color_scheme(true, b"Dark\n"), Some(true));
        assert_eq!(macos_defaults_color_scheme(true, b"Light\n"), Some(false));
        assert_eq!(macos_defaults_color_scheme(true, b"Graphite\n"), None);
    }

    #[test]
    fn resolves_system_dark_action_from_injected_resolver() {
        assert_eq!(
            resolve_dark_action(DarkAction::System, false, || {
                Some((true, "test-system"))
            }),
            ResolvedDarkAction {
                dark: true,
                source: "test-system",
            }
        );
        assert_eq!(
            resolve_dark_action(DarkAction::System, true, || {
                Some((false, "test-system"))
            }),
            ResolvedDarkAction {
                dark: false,
                source: "test-system",
            }
        );
    }

    #[test]
    fn system_dark_action_falls_back_to_current_state() {
        assert_eq!(
            resolve_dark_action(DarkAction::System, true, || None),
            ResolvedDarkAction {
                dark: true,
                source: "current-state-fallback",
            }
        );
        assert_eq!(
            resolve_dark_action(DarkAction::System, false, || None),
            ResolvedDarkAction {
                dark: false,
                source: "current-state-fallback",
            }
        );
    }

    #[test]
    fn loading_screen_uses_browser_label_and_immediate_warning() {
        for browser_label in ["webkit", "chromium", "ladybird"] {
            let rendered = render_loading_probe(browser_label);
            assert!(
                rendered
                    .capture
                    .contains(&format!("Waiting for {browser_label}")),
                "loading screen should name {browser_label}\n{}",
                rendered.capture
            );
            assert!(
                rendered
                    .capture
                    .contains("The first time you load a web browser"),
                "loading screen should show immediate engine-neutral warning\n{}",
                rendered.capture
            );
            assert!(
                !rendered.capture.contains("Chromium"),
                "loading screen should not mention Chromium\n{}",
                rendered.capture
            );
        }
    }

    #[test]
    fn browser_display_label_maps_known_engines_and_helpers() {
        for (input, expected) in [
            ("chromium", "chromium"),
            ("webkit", "webkit"),
            ("ladybird", "ladybird"),
            ("gecko", "gecko"),
            ("ah-chromiumd", "chromium"),
            ("ah-webkitd", "webkit"),
            ("ah-ladybirdd", "ladybird"),
            ("ah-geckod", "gecko"),
            ("/opt/homebrew/bin/ah-chromiumd", "chromium"),
            ("/opt/homebrew/bin/ah-webkitd", "webkit"),
            ("/opt/homebrew/bin/ah-ladybirdd", "ladybird"),
            ("/tmp/custom-engine", "custom-engine"),
        ] {
            assert_eq!(browser_display_label(input), expected, "{input}");
        }
    }

    #[test]
    fn explicit_dark_actions_do_not_call_system_resolver() {
        let resolver = || -> Option<(bool, &'static str)> {
            panic!("explicit dark actions should not query system appearance")
        };

        assert_eq!(
            resolve_dark_action(DarkAction::On, false, resolver),
            ResolvedDarkAction {
                dark: true,
                source: "explicit-on",
            }
        );
        assert_eq!(
            resolve_dark_action(DarkAction::Off, true, resolver),
            ResolvedDarkAction {
                dark: false,
                source: "explicit-off",
            }
        );
        assert_eq!(
            resolve_dark_action(DarkAction::Toggle, false, resolver),
            ResolvedDarkAction {
                dark: true,
                source: "toggle",
            }
        );
    }

    #[test]
    fn default_control_layout_places_controls_above_viewport() {
        let rendered = render_probe(Mode::Control, 80, 18, None);
        assert_controls_before_viewport(&rendered.capture, "URL", "edit url");
        assert_layout_invariants(Mode::Control, Rect::new(0, 0, 80, 18), None);
    }

    #[test]
    fn default_browse_layout_places_controls_above_viewport() {
        let rendered = render_probe(Mode::Browse, 80, 18, None);
        assert_controls_before_viewport(&rendered.capture, "URL", "back");
        assert_layout_invariants(Mode::Browse, Rect::new(0, 0, 80, 18), None);
    }

    #[test]
    fn issue_836_capture_documents_top_controls() {
        let control = render_probe(Mode::Control, 80, 18, None);
        let browse = render_probe(Mode::Browse, 80, 18, None);

        assert_controls_before_viewport(&control.capture, "URL", "edit url");
        assert_controls_before_viewport(&browse.capture, "URL", "back");

        println!("CONTROL MODE\n{}", control.capture);
        println!("BROWSE MODE\n{}", browse.capture);
    }

    #[test]
    fn edit_and_command_layouts_keep_chrome_above_viewport() {
        let edit = render_probe(Mode::Edit, 80, 18, None);
        assert_controls_before_viewport(&edit.capture, "URL", "navigate");
        assert_layout_invariants(Mode::Edit, Rect::new(0, 0, 80, 18), None);

        let command = render_probe(Mode::Command, 80, 18, None);
        assert_controls_before_viewport(&command.capture, "COMMAND", "execute");
        assert_layout_invariants(Mode::Command, Rect::new(0, 0, 80, 18), None);
    }

    #[test]
    fn viewport_override_keeps_controls_above_viewport() {
        let rendered = render_probe(Mode::Control, 80, 20, Some(5));
        assert_controls_before_viewport(&rendered.capture, "URL", "edit url");
        assert_layout_invariants(Mode::Control, Rect::new(0, 0, 80, 20), Some(5));
    }

    #[test]
    fn small_and_large_panes_keep_non_collapsed_viewport_below_controls() {
        assert_layout_invariants(Mode::Control, Rect::new(0, 0, 24, 7), None);
        assert_layout_invariants(Mode::Browse, Rect::new(0, 0, 120, 40), None);
    }

    #[test]
    fn navigation_buttons_are_symbol_only_and_ordered_in_every_mode() {
        let state = enabled_back_state();
        let mut expected_geometry = None;
        for mode in [
            Mode::Browse,
            Mode::Control,
            Mode::Edit,
            Mode::Command,
            Mode::Dialog,
            Mode::Auth,
        ] {
            let rendered = render_probe_with_back(mode, 80, 18, None, state.clone(), true);
            let (back_x, back_y) = find_cell(&rendered.buffer, BACK_SYMBOL);
            let (forward_x, forward_y) = find_cell(&rendered.buffer, FORWARD_SYMBOL);
            let (refresh_x, refresh_y) = find_cell(&rendered.buffer, REFRESH_IDLE_SYMBOL);
            assert!(
                back_x < forward_x && forward_x < refresh_x && refresh_x < rendered.url.x,
                "{}",
                rendered.capture
            );
            assert!(back_y >= rendered.back.y && back_y < rendered.back.bottom());
            assert!(forward_y >= rendered.forward.y && forward_y < rendered.forward.bottom());
            assert!(refresh_y >= rendered.refresh.y && refresh_y < rendered.refresh.bottom());
            assert_eq!(refresh_x, rendered.refresh.x + rendered.refresh.width / 2);
            assert_eq!(refresh_y, rendered.refresh.y + rendered.refresh.height / 2);
            assert_eq!(rendered.back.right(), rendered.forward.x);
            assert_eq!(rendered.forward.right(), rendered.refresh.x);
            assert_eq!(rendered.refresh.right(), rendered.url.x);
            let mut back_text = String::new();
            for y in rendered.back.y..rendered.back.bottom() {
                for x in rendered.back.x..rendered.back.right() {
                    back_text.push_str(rendered.buffer[(x, y)].symbol());
                }
            }
            assert!(!back_text.to_ascii_lowercase().contains("back"));
            let mut forward_text = String::new();
            for y in rendered.forward.y..rendered.forward.bottom() {
                for x in rendered.forward.x..rendered.forward.right() {
                    forward_text.push_str(rendered.buffer[(x, y)].symbol());
                }
            }
            assert!(!forward_text.to_ascii_lowercase().contains("forward"));
            let mut refresh_text = String::new();
            for y in rendered.refresh.y..rendered.refresh.bottom() {
                for x in rendered.refresh.x..rendered.refresh.right() {
                    refresh_text.push_str(rendered.buffer[(x, y)].symbol());
                }
            }
            assert!(!refresh_text.to_ascii_lowercase().contains("refresh"));
            let geometry = (
                rendered.back,
                rendered.forward,
                rendered.refresh,
                rendered.url,
                rendered.viewport,
            );
            if let Some(expected) = expected_geometry {
                assert_eq!(geometry, expected, "mode changes must not move chrome");
            } else {
                expected_geometry = Some(geometry);
            }
        }
    }

    #[test]
    fn back_button_buffer_styles_cover_disabled_normal_hover_and_pressed() {
        let route = compositor_route();
        let mut disabled_state = enabled_back_state();
        disabled_state.can_go_back = false;
        disabled_state.hovered = true;
        disabled_state.pressed = Some(BackPress {
            tab_id: 7,
            route: route.clone(),
        });
        let disabled = render_probe_with_back(Mode::Control, 80, 18, None, disabled_state, true);
        let (x, y) = find_cell(&disabled.buffer, BACK_SYMBOL);
        assert_eq!(disabled.buffer[(x, y)].fg, DIM);
        assert_eq!(disabled.buffer[(x, y)].bg, BG);
        assert_eq!(
            disabled.buffer[(disabled.back.x, disabled.back.y)].fg,
            BORDER
        );
        // Disabled keeps rounded geometry; color only.
        assert_eq!(
            disabled.buffer[(disabled.back.x, disabled.back.y)].symbol(),
            "╭"
        );

        let normal =
            render_probe_with_back(Mode::Control, 80, 18, None, enabled_back_state(), true);
        let (x, y) = find_cell(&normal.buffer, BACK_SYMBOL);
        assert_eq!(normal.buffer[(x, y)].fg, FG);
        assert_eq!(normal.buffer[(x, y)].bg, BG);
        assert_eq!(normal.buffer[(normal.back.x, normal.back.y)].fg, CYAN);
        assert_eq!(normal.buffer[(normal.back.x, normal.back.y)].symbol(), "╭");

        let mut hover_state = enabled_back_state();
        hover_state.hovered = true;
        let hover = render_probe_with_back(Mode::Control, 80, 18, None, hover_state, true);
        let (x, y) = find_cell(&hover.buffer, BACK_SYMBOL);
        assert_eq!(hover.buffer[(x, y)].fg, FG);
        assert_eq!(hover.buffer[(x, y)].bg, SELECTION);
        assert_eq!(hover.buffer[(hover.back.x, hover.back.y)].fg, CYAN);
        // Hover is fill/color only — same rounded corner as idle.
        assert_eq!(hover.buffer[(hover.back.x, hover.back.y)].symbol(), "╭");

        let mut pressed_state = enabled_back_state();
        pressed_state.hovered = true;
        pressed_state.pressed = Some(BackPress { tab_id: 7, route });
        let pressed = render_probe_with_back(Mode::Control, 80, 18, None, pressed_state, true);
        let (x, y) = find_cell(&pressed.buffer, BACK_SYMBOL);
        assert_eq!(pressed.buffer[(x, y)].fg, BG);
        assert_eq!(pressed.buffer[(x, y)].bg, CYAN);
        assert_eq!(pressed.buffer[(pressed.back.x, pressed.back.y)].fg, CYAN);
        assert_eq!(
            pressed.buffer[(pressed.back.x, pressed.back.y)].symbol(),
            "╭"
        );

        let route_missing =
            render_probe_with_back(Mode::Control, 80, 18, None, enabled_back_state(), false);
        let (x, y) = find_cell(&route_missing.buffer, BACK_SYMBOL);
        assert_eq!(route_missing.buffer[(x, y)].fg, DIM);
        assert_eq!(route_missing.buffer[(x, y)].bg, BG);
    }

    #[test]
    fn forward_button_buffer_styles_cover_independent_disabled_hover_and_pressed() {
        let route = compositor_route();
        let disabled = render_probe_with_navigation(
            Mode::Control,
            80,
            18,
            None,
            enabled_back_state(),
            ForwardControlState {
                active_tab_id: 7,
                can_go_forward: false,
                hovered: true,
                pressed: Some(ForwardPress {
                    tab_id: 7,
                    route: route.clone(),
                }),
            },
            RefreshControlState::default(),
            RefreshAnimation::default(),
            Instant::now(),
            true,
        );
        let (x, y) = find_cell(&disabled.buffer, FORWARD_SYMBOL);
        assert_eq!(disabled.buffer[(x, y)].fg, DIM);
        assert_eq!(disabled.buffer[(x, y)].bg, BG);

        let mut hover_state = enabled_forward_state();
        hover_state.hovered = true;
        let hover = render_probe_with_navigation(
            Mode::Control,
            80,
            18,
            None,
            BackControlState::default(),
            hover_state,
            RefreshControlState::default(),
            RefreshAnimation::default(),
            Instant::now(),
            true,
        );
        let (x, y) = find_cell(&hover.buffer, FORWARD_SYMBOL);
        assert_eq!(hover.buffer[(x, y)].fg, FG);
        assert_eq!(hover.buffer[(x, y)].bg, SELECTION);
        assert_eq!(
            hover.buffer[(hover.forward.x, hover.forward.y)].symbol(),
            "╭"
        );

        let mut pressed_state = enabled_forward_state();
        pressed_state.hovered = true;
        pressed_state.pressed = Some(ForwardPress { tab_id: 7, route });
        let pressed = render_probe_with_navigation(
            Mode::Control,
            80,
            18,
            None,
            BackControlState::default(),
            pressed_state,
            RefreshControlState::default(),
            RefreshAnimation::default(),
            Instant::now(),
            true,
        );
        let (x, y) = find_cell(&pressed.buffer, FORWARD_SYMBOL);
        assert_eq!(pressed.buffer[(x, y)].fg, BG);
        assert_eq!(pressed.buffer[(x, y)].bg, CYAN);
        assert_eq!(
            pressed.buffer[(pressed.forward.x, pressed.forward.y)].symbol(),
            "╭"
        );
    }

    #[test]
    fn url_and_viewport_share_rounded_chrome_corners_with_nav_buttons() {
        let probe = render_probe_with_back(Mode::Control, 80, 18, None, enabled_back_state(), true);
        // UiGeometry.viewport is the *inner* content rect; outer frame is inset by 1.
        let viewport_frame = (
            probe.viewport.x.saturating_sub(1),
            probe.viewport.y.saturating_sub(1),
        );
        // Control mode: cyan URL, dim viewport border; both rounded top-left.
        assert_eq!(probe.buffer[(probe.url.x, probe.url.y)].symbol(), "╭");
        assert_eq!(probe.buffer[(probe.url.x, probe.url.y)].fg, CYAN);
        assert_eq!(
            probe.buffer[(viewport_frame.0, viewport_frame.1)].symbol(),
            "╭"
        );
        assert_eq!(
            probe.buffer[(viewport_frame.0, viewport_frame.1)].fg,
            BORDER
        );
        assert_eq!(probe.buffer[(probe.back.x, probe.back.y)].symbol(), "╭");
        assert_eq!(
            probe.buffer[(probe.forward.x, probe.forward.y)].symbol(),
            "╭"
        );
        assert_eq!(
            probe.buffer[(probe.refresh.x, probe.refresh.y)].symbol(),
            "╭"
        );
    }

    #[test]
    fn refresh_button_styles_and_animation_frames_are_deterministic() {
        let now = Instant::now();
        let route = compositor_route();
        let mut pressed_state = enabled_refresh_state();
        pressed_state.hovered = true;
        pressed_state.pressed = Some(RefreshPress {
            tab_id: 7,
            route: route.clone(),
        });
        let pressed = render_probe_with_navigation(
            Mode::Control,
            80,
            18,
            None,
            BackControlState::default(),
            ForwardControlState::default(),
            pressed_state,
            RefreshAnimation::default(),
            now,
            true,
        );
        let (x, y) = find_cell(&pressed.buffer, REFRESH_IDLE_SYMBOL);
        assert_eq!(pressed.buffer[(x, y)].fg, BG);
        assert_eq!(pressed.buffer[(x, y)].bg, CYAN);

        let mut animation = RefreshAnimation::default();
        assert!(animation.start(7, 41, now));
        let frame0 = render_probe_with_navigation(
            Mode::Control,
            80,
            18,
            None,
            BackControlState::default(),
            ForwardControlState::default(),
            enabled_refresh_state(),
            animation,
            now,
            true,
        );
        let frame1 = render_probe_with_navigation(
            Mode::Control,
            80,
            18,
            None,
            BackControlState::default(),
            ForwardControlState::default(),
            enabled_refresh_state(),
            animation,
            now + RefreshAnimation::FRAME_DURATION,
            true,
        );
        assert!(frame0.capture.contains(REFRESH_ANIMATION_FRAMES[0]));
        assert!(frame1.capture.contains(REFRESH_ANIMATION_FRAMES[1]));
        assert_ne!(REFRESH_ANIMATION_FRAMES[0], REFRESH_ANIMATION_FRAMES[1]);
        assert_eq!(
            refresh_symbol(&RefreshAnimation::default(), now),
            REFRESH_IDLE_SYMBOL
        );
        assert_eq!(frame0.refresh, frame1.refresh);
    }

    #[test]
    fn refresh_press_cancels_on_release_outside_capability_and_route_changes() {
        let route = compositor_route();
        let rect = Rect::new(10, 2, 5, 3);
        let mut state = enabled_refresh_state();
        let down = update_refresh_mouse(
            &mut state,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Down(MouseButton::Left), 12, 3),
        );
        assert!(down.changed && !down.activate && state.pressed.is_some());
        let outside = update_refresh_mouse(
            &mut state,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Up(MouseButton::Left), 20, 3),
        );
        assert!(!outside.activate);

        update_refresh_mouse(
            &mut state,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Down(MouseButton::Left), 12, 3),
        );
        assert!(state.apply_navigation_state(7, false));
        assert!(state.pressed.is_none());

        state.can_refresh = true;
        update_refresh_mouse(
            &mut state,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Down(MouseButton::Left), 12, 3),
        );
        assert!(state.reconcile_route(Some(&BackRoute::Direct(7))));
        assert!(state.pressed.is_none());
    }

    #[test]
    fn renderer_crash_does_not_source_refresh_capability() {
        let route = compositor_route();
        let mut state = enabled_refresh_state();
        state.hovered = true;
        state.pressed = Some(RefreshPress { tab_id: 7, route });

        assert!(!state.renderer_crashed(8));
        assert!(state.can_refresh);
        assert!(state.renderer_crashed(7));
        assert!(state.can_refresh);
        assert!(!state.hovered);
        assert!(state.pressed.is_none());

        assert!(state.apply_navigation_state(7, false));
        assert!(!state.can_refresh);
    }

    #[test]
    fn refresh_animation_correlates_replacement_completion_minimum_and_timeout() {
        let now = Instant::now();
        let mut animation = RefreshAnimation::default();
        assert!(animation.start(7, 10, now));
        assert_eq!(animation.frame(now), 0);
        assert_eq!(animation.frame(now + RefreshAnimation::FRAME_DURATION), 1);
        assert!(!animation.complete(7, 9, now + Duration::from_millis(50)));
        assert!(animation.start(7, 11, now + Duration::from_millis(60)));
        assert_eq!(animation.request_id, 11);
        assert!(!animation.complete(7, 10, now + Duration::from_millis(70)));
        assert!(animation.complete(7, 11, now + Duration::from_millis(80)));
        assert!(!animation.tick(now + Duration::from_millis(299)));
        assert!(animation.active());
        assert!(animation.tick(now + Duration::from_millis(300)));
        assert!(!animation.active());

        assert!(animation.start(7, 12, now));
        assert!(animation.tick(now + RefreshAnimation::TIMEOUT));
        assert!(!animation.active());
    }

    #[test]
    fn forward_click_requires_same_tab_route_and_uncancelled_release() {
        let rect = Rect::new(5, 0, 5, 3);
        let route = compositor_route();
        let mut state = enabled_forward_state();
        let down = update_forward_mouse(
            &mut state,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Down(MouseButton::Left), 7, 1),
        );
        assert!(down.changed);
        assert!(!down.activate);
        let up = update_forward_mouse(
            &mut state,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Up(MouseButton::Left), 7, 1),
        );
        assert!(up.activate);

        let mut canceled = enabled_forward_state();
        update_forward_mouse(
            &mut canceled,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Down(MouseButton::Left), 7, 1),
        );
        update_forward_mouse(
            &mut canceled,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Drag(MouseButton::Left), 20, 1),
        );
        let returned = update_forward_mouse(
            &mut canceled,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Up(MouseButton::Left), 7, 1),
        );
        assert!(!returned.activate);

        let mut stale = enabled_forward_state();
        update_forward_mouse(
            &mut stale,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Down(MouseButton::Left), 7, 1),
        );
        assert!(!stale.apply_navigation_state(8, false));
        assert!(stale.pressed.is_some());
        assert!(stale.apply_navigation_state(7, false));
        assert!(stale.pressed.is_none());
        assert!(!stale.can_go_forward);
    }

    #[test]
    fn back_visual_signature_tracks_only_actionable_style_boundaries() {
        let route = compositor_route();
        let mut state = enabled_back_state();
        assert_eq!(
            back_visual_state(&state, Some(&route)),
            BackVisualState {
                actionable: true,
                hovered: false,
                pressed: false,
            }
        );

        state.hovered = true;
        state.pressed = Some(BackPress {
            tab_id: 7,
            route: route.clone(),
        });
        assert_eq!(
            back_visual_state(&state, Some(&route)),
            BackVisualState {
                actionable: true,
                hovered: true,
                pressed: true,
            }
        );

        assert_eq!(
            back_visual_state(&state, None),
            BackVisualState {
                actionable: false,
                hovered: false,
                pressed: false,
            }
        );
    }

    #[test]
    fn navigation_and_url_geometry_is_exact_at_normal_narrow_and_degenerate_widths() {
        for (width, back_width, forward_width, refresh_width, url_width) in [
            (0, 0, 0, 0, 0),
            (1, 0, 0, 0, 1),
            (2, 1, 0, 0, 1),
            (3, 1, 1, 0, 1),
            (4, 1, 1, 1, 1),
            (5, 2, 1, 1, 1),
            (6, 2, 2, 1, 1),
            (7, 2, 2, 2, 1),
            (8, 3, 2, 2, 1),
            (9, 3, 3, 2, 1),
            (10, 3, 3, 3, 1),
            (11, 4, 3, 3, 1),
            (12, 4, 4, 3, 1),
            (13, 4, 4, 4, 1),
            (14, 5, 4, 4, 1),
            (15, 5, 5, 4, 1),
            (16, 5, 5, 5, 1),
            (17, 5, 5, 5, 2),
            (80, 5, 5, 5, 65),
        ] {
            let layout = browser_layout(Rect::new(0, 0, width, 18), None);
            assert_eq!(layout.back_area.width, back_width, "width={width}");
            assert_eq!(layout.forward_area.width, forward_width, "width={width}");
            assert_eq!(layout.refresh_area.width, refresh_width, "width={width}");
            assert_eq!(layout.url_area.width, url_width, "width={width}");
            assert_eq!(layout.back_area.right(), layout.forward_area.x);
            assert_eq!(layout.forward_area.right(), layout.refresh_area.x);
            assert_eq!(layout.refresh_area.right(), layout.url_area.x);
            assert_eq!(
                layout.back_area.width
                    + layout.forward_area.width
                    + layout.refresh_area.width
                    + layout.url_area.width,
                width,
                "width={width}"
            );
        }

        let narrow = render_probe_with_back(Mode::Control, 6, 7, None, enabled_back_state(), true);
        assert_eq!(narrow.back, Rect::new(0, 0, 2, 3));
        assert_eq!(narrow.forward, Rect::new(2, 0, 2, 3));
        assert_eq!(narrow.refresh, Rect::new(4, 0, 1, 3));
        assert_eq!(narrow.url, Rect::new(5, 0, 1, 3));
    }

    #[test]
    fn back_hit_test_edges_and_disabled_pointer_state_are_fail_closed() {
        let rect = Rect::new(5, 2, 5, 3);
        assert!(rect_contains(rect, 5, 2));
        assert!(rect_contains(rect, 9, 4));
        assert!(!rect_contains(rect, 10, 4));
        assert!(!rect_contains(rect, 9, 5));
        assert!(!rect_contains(rect, 4, 2));

        let route = compositor_route();
        let mut disabled = BackControlState {
            active_tab_id: 7,
            ..BackControlState::default()
        };
        let moved = update_back_mouse(
            &mut disabled,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Moved, 7, 3),
        );
        assert_eq!(moved, BackMouseResult::default());
        let clicked = update_back_mouse(
            &mut disabled,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Down(MouseButton::Left), 7, 3),
        );
        assert!(!clicked.activate);
        assert!(!disabled.hovered);
        assert!(disabled.pressed.is_none());

        let mut enabled = enabled_back_state();
        let right_click = update_back_mouse(
            &mut enabled,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Down(MouseButton::Right), 7, 3),
        );
        assert_eq!(right_click, BackMouseResult::default());
    }

    #[test]
    fn back_click_requires_same_tab_route_and_uncancelled_release() {
        let rect = Rect::new(0, 0, 5, 3);
        let route = compositor_route();
        let mut state = enabled_back_state();
        assert!(
            !update_back_mouse(
                &mut state,
                rect,
                Some(&route),
                test_mouse(MouseEventKind::Down(MouseButton::Left), 2, 1),
            )
            .activate
        );
        assert!(
            update_back_mouse(
                &mut state,
                rect,
                Some(&route),
                test_mouse(MouseEventKind::Up(MouseButton::Left), 2, 1),
            )
            .activate
        );

        update_back_mouse(
            &mut state,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Down(MouseButton::Left), 2, 1),
        );
        assert!(
            !update_back_mouse(
                &mut state,
                rect,
                Some(&route),
                test_mouse(MouseEventKind::Up(MouseButton::Left), 8, 1),
            )
            .activate
        );
        assert!(state.pressed.is_none());

        update_back_mouse(
            &mut state,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Down(MouseButton::Left), 2, 1),
        );
        update_back_mouse(
            &mut state,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Drag(MouseButton::Left), 8, 1),
        );
        update_back_mouse(
            &mut state,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Drag(MouseButton::Left), 2, 1),
        );
        assert!(
            !update_back_mouse(
                &mut state,
                rect,
                Some(&route),
                test_mouse(MouseEventKind::Up(MouseButton::Left), 2, 1),
            )
            .activate
        );

        update_back_mouse(
            &mut state,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Down(MouseButton::Left), 2, 1),
        );
        let replacement = BackRoute::Direct(7);
        assert!(
            !update_back_mouse(
                &mut state,
                rect,
                Some(&replacement),
                test_mouse(MouseEventKind::Up(MouseButton::Left), 2, 1),
            )
            .activate
        );
    }

    #[test]
    fn authoritative_back_state_filters_tabs_and_clears_interaction() {
        let route = compositor_route();
        let mut state = BackControlState::default();
        state.browser_ready(7);
        assert!(!state.can_go_back);
        assert!(!state.apply_navigation_state(8, true));
        assert!(!state.can_go_back);
        assert!(state.apply_navigation_state(7, true));
        assert!(state.can_go_back);

        state.hovered = true;
        state.pressed = Some(BackPress {
            tab_id: 7,
            route: route.clone(),
        });
        assert!(state.apply_navigation_state(7, false));
        assert!(!state.can_go_back);
        assert!(!state.hovered);
        assert!(state.pressed.is_none());

        assert!(state.apply_navigation_state(7, true));
        state.hovered = true;
        state.pressed = Some(BackPress {
            tab_id: 7,
            route: route.clone(),
        });
        let mut stale_connection = Some("tab-7");
        reset_back_for_browser_ready(&mut state, &mut stale_connection, 8);
        assert!(stale_connection.is_none());
        assert_eq!(state.active_tab_id, 8);
        assert!(!state.can_go_back);
        assert!(!state.hovered);
        assert!(state.pressed.is_none());
        assert!(!state.apply_navigation_state(7, true));
        assert!(state.apply_navigation_state(8, true));

        assert!(!state.renderer_crashed(7));
        assert!(state.can_go_back);
        state.hovered = true;
        state.pressed = Some(BackPress { tab_id: 8, route });
        assert!(state.renderer_crashed(8));
        assert!(!state.can_go_back);
        assert!(!state.hovered);
        assert!(state.pressed.is_none());
    }

    #[test]
    fn unavailable_or_replaced_routes_cancel_press_without_rewriting_history() {
        let route = compositor_route();
        let mut state = enabled_back_state();
        state.hovered = true;
        state.pressed = Some(BackPress {
            tab_id: 7,
            route: route.clone(),
        });
        assert!(state.reconcile_route(None));
        assert!(state.can_go_back);
        assert!(!state.hovered);
        assert!(state.pressed.is_none());

        state.hovered = true;
        state.pressed = Some(BackPress { tab_id: 7, route });
        assert!(state.reconcile_route(Some(&BackRoute::Direct(7))));
        assert!(state.can_go_back);
        assert!(state.pressed.is_none());
    }

    #[test]
    fn compositor_is_preferred_and_direct_route_must_match_active_tab() {
        assert_eq!(
            current_back_route(7, true, Some("pane-7"), Some(7)),
            Some(compositor_route())
        );
        assert_eq!(
            current_back_route(7, false, None, Some(7)),
            Some(BackRoute::Direct(7))
        );
        assert_eq!(current_back_route(7, false, None, Some(8)), None);
        assert_eq!(current_back_route(0, true, Some("pane-7"), Some(7)), None);

        let mut disabled = enabled_back_state();
        disabled.can_go_back = false;
        assert_eq!(
            back_dispatch_decision(&disabled, Some(&compositor_route())),
            BackDispatchDecision::BlockedDisabled
        );
        assert_eq!(
            back_dispatch_decision(&enabled_back_state(), None),
            BackDispatchDecision::BlockedUnavailable
        );
        assert_eq!(
            back_dispatch_decision(&enabled_back_state(), Some(&compositor_route())),
            BackDispatchDecision::Send(compositor_route())
        );
    }

    #[test]
    fn injected_chrome_key_and_mouse_converge_only_in_control_and_browse() {
        let key = KeyEvent::new(KeyCode::Char('['), KeyModifiers::SUPER);
        let forward_key = KeyEvent::new(KeyCode::Char(']'), KeyModifiers::SUPER);
        let refresh_key = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::SUPER);
        assert!(local_back_key(&Mode::Control, key));
        assert!(local_back_key(&Mode::Browse, key));
        assert!(local_forward_key(&Mode::Control, forward_key));
        assert!(local_forward_key(&Mode::Browse, forward_key));
        assert!(local_refresh_key(&Mode::Control, refresh_key));
        assert!(local_refresh_key(&Mode::Browse, refresh_key));
        for mode in [Mode::Edit, Mode::Command, Mode::Dialog, Mode::Auth] {
            assert!(!local_back_key(&mode, key));
            assert!(!local_forward_key(&mode, forward_key));
            assert!(!local_refresh_key(&mode, refresh_key));
        }
        assert!(!local_back_key(
            &Mode::Control,
            KeyEvent::new(KeyCode::Char('['), KeyModifiers::CONTROL)
        ));

        let route = compositor_route();
        let state = enabled_back_state();
        let keyboard_decision = back_dispatch_decision(&state, Some(&route));
        let mut mouse_state = state.clone();
        let rect = Rect::new(0, 0, 5, 3);
        update_back_mouse(
            &mut mouse_state,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Down(MouseButton::Left), 2, 1),
        );
        let mouse = update_back_mouse(
            &mut mouse_state,
            rect,
            Some(&route),
            test_mouse(MouseEventKind::Up(MouseButton::Left), 2, 1),
        );
        assert!(mouse.activate);
        assert_eq!(
            back_dispatch_decision(&mouse_state, Some(&route)),
            keyboard_decision
        );
    }

    #[test]
    fn event_polling_active_animation_overrides_expired_page_grace() {
        let now = Instant::now();
        assert!(needs_event_polling(
            true,
            Some(now - Duration::from_secs(3)),
            None,
            true,
            now,
        ));
    }

    #[test]
    fn event_polling_old_completed_page_blocks_without_active_reason() {
        let now = Instant::now();
        assert!(!needs_event_polling(
            true,
            Some(now - Duration::from_secs(3)),
            None,
            false,
            now,
        ));
    }

    #[test]
    fn event_polling_cold_load_grace_and_copy_feedback_are_independent() {
        let now = Instant::now();
        assert!(needs_event_polling(false, None, None, false, now));
        assert!(needs_event_polling(
            true,
            Some(now - Duration::from_secs(1)),
            None,
            false,
            now,
        ));
        assert!(needs_event_polling(
            true,
            None,
            Some(now + Duration::from_secs(1)),
            false,
            now,
        ));
    }

    #[test]
    fn event_polling_expired_copy_feedback_cannot_mask_animation() {
        let now = Instant::now();
        assert!(!needs_event_polling(
            true,
            None,
            Some(now - Duration::from_secs(1)),
            false,
            now,
        ));
        assert!(needs_event_polling(
            true,
            Some(now - Duration::from_secs(3)),
            Some(now - Duration::from_secs(1)),
            true,
            now,
        ));
    }
}
