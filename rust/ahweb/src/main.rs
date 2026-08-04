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

/// Last-resort open URL when CLI has no URL and Hello homepage is empty.
const DEFAULT_HOMEPAGE_URL: &str = "https://astrohacker.com/";

// Product page ground (Austin Night / darker Tokyo Night plate) + accents.
const BG: Color = Color::Rgb(0x09, 0x09, 0x0d);
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
/// Top-right quit chrome (Issue 26072709578702). Prefer ×; layout may squeeze width.
const QUIT_SYMBOL: &str = "×";
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
type QuitMouseResult = BackMouseResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BackVisualState {
    actionable: bool,
    hovered: bool,
    pressed: bool,
}

/// Always-actionable quit control (process exit), left of no history gate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct QuitControlState {
    hovered: bool,
    pressed: bool,
}

impl QuitControlState {
    fn clear_interaction(&mut self) {
        self.hovered = false;
        self.pressed = false;
    }

    fn actionable(&self) -> bool {
        true
    }
}

fn quit_visual_state(state: &QuitControlState) -> BackVisualState {
    BackVisualState {
        actionable: true,
        hovered: state.hovered,
        pressed: state.pressed,
    }
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

/// Inner text area of a rounded chrome block (URL bar, viewport frame).
fn chrome_inner_rect(area: Rect) -> Rect {
    Block::default().borders(Borders::ALL).inner(area)
}

/// Map a mouse column to a URL editor cursor column (edtui Index2.col).
/// Uses the **inner** URL rect so borders do not steal columns; clamps to `[0, url.len()]`.
fn url_click_cursor_col(url: &str, url_inner: Rect, mouse_col: u16) -> usize {
    let len = url.len();
    if url_inner.width == 0 {
        return 0;
    }
    if mouse_col <= url_inner.x {
        return 0;
    }
    let offset = usize::from(mouse_col.saturating_sub(url_inner.x));
    offset.min(len)
}

/// Control-mode URL bar click: Edit + Insert with cursor under the click.
fn enter_url_insert_from_click(
    editor_state: &mut EditorState,
    editor_url: &mut String,
    url: &str,
    mode: &mut Mode,
    url_rect: Rect,
    mouse_col: u16,
) {
    if *editor_url != url {
        *editor_state = EditorState::new(Lines::from(url));
        editor_state.set_clipboard(UrlClipboard::new());
        *editor_url = url.to_string();
    }
    *mode = Mode::Edit;
    editor_state.mode = EditorMode::Insert;
    editor_state.selection = None;
    let inner = chrome_inner_rect(url_rect);
    let col = url_click_cursor_col(url, inner, mouse_col);
    editor_state.cursor = edtui::Index2::new(0, col);
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

/// Mouse press/release for the top-right quit control.
/// Activate means process quit (same as `q` / Ctrl+C / `:quit`).
fn update_quit_mouse(
    state: &mut QuitControlState,
    rect: Rect,
    mouse: MouseEvent,
) -> QuitMouseResult {
    let before = state.clone();
    let hit = rect_contains(rect, mouse.column, mouse.row);
    let actionable_hit = hit && state.actionable();
    let mut activate = false;

    match mouse.kind {
        MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left) => {
            state.hovered = actionable_hit;
            if state.pressed && !actionable_hit {
                state.pressed = false;
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            state.hovered = actionable_hit;
            state.pressed = actionable_hit;
        }
        MouseEventKind::Up(MouseButton::Left) => {
            let was_pressed = state.pressed;
            state.pressed = false;
            state.hovered = actionable_hit;
            activate = actionable_hit && was_pressed;
        }
        _ => {}
    }

    QuitMouseResult {
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

/// Soft refresh: Super+R without Shift (Control|Browse).
fn local_refresh_key(mode: &Mode, key: KeyEvent) -> bool {
    matches!(mode, Mode::Control | Mode::Browse)
        && key.modifiers.contains(KeyModifiers::SUPER)
        && !key.modifiers.contains(KeyModifiers::SHIFT)
        && matches!(key.code, KeyCode::Char('r') | KeyCode::Char('R'))
}

/// Hard refresh: Shift+R (Super optional). Control|Browse when events reach ahweb.
fn local_hard_refresh_key(mode: &Mode, key: KeyEvent) -> bool {
    matches!(mode, Mode::Control | Mode::Browse)
        && key.modifiers.contains(KeyModifiers::SHIFT)
        && matches!(key.code, KeyCode::Char('r') | KeyCode::Char('R'))
}

fn needs_event_polling(
    page_loaded: bool,
    page_loaded_at: Option<Instant>,
    copy_url_feedback_until: Option<Instant>,
    now: Instant,
) -> bool {
    !page_loaded
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
    dispatch_refresh_kind(
        source,
        state,
        route,
        compositor,
        browser_conn,
        state_trace,
        ipc::RefreshKind::Soft,
    )
}

fn dispatch_refresh_ignore_cache(
    source: &str,
    state: &RefreshControlState,
    route: Option<&BackRoute>,
    compositor: &Option<ipc::CompositorConnection>,
    browser_conn: &Option<ipc::BrowserConnection>,
    state_trace: &mut Option<StateTrace>,
) -> bool {
    dispatch_refresh_kind(
        source,
        state,
        route,
        compositor,
        browser_conn,
        state_trace,
        ipc::RefreshKind::IgnoreCache,
    )
}

fn dispatch_refresh_kind(
    source: &str,
    state: &RefreshControlState,
    route: Option<&BackRoute>,
    compositor: &Option<ipc::CompositorConnection>,
    browser_conn: &Option<ipc::BrowserConnection>,
    state_trace: &mut Option<StateTrace>,
    kind: ipc::RefreshKind,
) -> bool {
    let (sent, route_label, request_id, blocked_reason) = if !state.can_refresh
        || state.active_tab_id <= 0
    {
        (false, "none", 0, Some("disabled"))
    } else {
        match route {
            Some(BackRoute::Compositor(pane_id)) => {
                let sent = compositor
                    .as_ref()
                    .map(|conn| match kind {
                        ipc::RefreshKind::Soft => conn.send_refresh(pane_id),
                        ipc::RefreshKind::IgnoreCache => conn.send_refresh_ignore_cache(pane_id),
                    })
                    .unwrap_or(false);
                (sent, "compositor", 0, None)
            }
            Some(BackRoute::Direct(tab_id)) => {
                let request_id = browser_conn
                    .as_ref()
                    .filter(|conn| conn.tab_id == *tab_id && *tab_id == state.active_tab_id)
                    .and_then(|conn| match kind {
                        ipc::RefreshKind::Soft => conn.send_refresh(),
                        ipc::RefreshKind::IgnoreCache => conn.send_refresh_ignore_cache(),
                    })
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
            ("action", kind.action().to_string()),
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

#[derive(Debug)]
enum ViewportCommand {
    Height(u16),
    Reset,
}

#[derive(Debug)]
enum CommandResult {
    Quit,
    Dark(DarkAction),
    Viewport(ViewportCommand),
    DevTools(String), // direction: "right", "down", "left", "up" (Issue 26030112000690).
    /// Soft / hard refresh from Control command bar (Issue 26072209562907 Exp 2).
    RefreshSoft,
    RefreshHard,
    Error(String), // error message for command bar (Issue 26030112000690).
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
    Command {
        // Issue 26072209562907 Exp 2/3: reliable soft/hard without Shift+mouse.
        // `:r` soft alias (Exp 3); `:refresh hard` / `:rh` hard.
        names: &["refresh", "r"],
        exec: |args| match args.first().copied() {
            None => CommandResult::RefreshSoft,
            Some("hard" | "h" | "ignore-cache") => CommandResult::RefreshHard,
            Some(other) => CommandResult::Error(format!(
                "Unknown: {other} (Usage: refresh | refresh hard | r | rh)"
            )),
        },
    },
    Command {
        // Issue 26072209562907 Exp 3: `:rh` → hard refresh (ignore cache).
        names: &["rh"],
        exec: |args| match args.first().copied() {
            None => CommandResult::RefreshHard,
            Some(other) => CommandResult::Error(format!(
                "Unknown: {other} (Usage: rh — hard refresh; or refresh hard)"
            )),
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

    /// Browser engine to use ("chromium") or absolute path to a helper binary
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
                .unwrap_or_else(|| DEFAULT_HOMEPAGE_URL.to_string())
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
    // Download progress owns the surface bar while active (Issue 26073112048720 Exp 3).
    let mut download_bar_active = false;
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
    let mut quit_control = QuitControlState::default();
    let mut last_back_visual: Option<BackVisualState> = None;
    let mut last_forward_visual: Option<BackVisualState> = None;
    let mut last_refresh_visual: Option<BackVisualState> = None;
    let mut last_quit_visual: Option<BackVisualState> = None;

    // Event loop.
    loop {
        let mut viewport_rect = Rect::default();
        let mut back_rect = Rect::default();
        let mut forward_rect = Rect::default();
        let mut refresh_rect = Rect::default();
        let mut quit_rect = Rect::default();
        let mut url_rect = Rect::default();
        let mut frame_area = Rect::default();
        let browser_label = browser_display_label(&browser);
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
        let quit_visual = quit_visual_state(&quit_control);
        let navigation_visual_changed = last_back_visual
            .map(|previous| previous != back_visual)
            .unwrap_or(false)
            || last_forward_visual
                .map(|previous| previous != forward_visual)
                .unwrap_or(false)
            || last_refresh_visual
                .map(|previous| previous != refresh_visual)
                .unwrap_or(false)
            || last_quit_visual
                .map(|previous| previous != quit_visual)
                .unwrap_or(false);
        if navigation_visual_changed {
            // Ghostty can retain style-only cell damage beneath a browser
            // overlay. Navigation state changes are infrequent, so force one full
            // terminal redraw at that boundary to make the visual feedback
            // observable without turning steady-state rendering into polling.
            //
            // Do not use Terminal::clear(): on ratatui-core ≥ 0.1.2 it queries
            // cursor position (CSI 6n) and races the concurrent event::read
            // thread (Issue 26080220381373 Exp 2).
            force_full_redraw(&mut terminal)?;
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
                &quit_control,
                back_route.is_some(),
            );
            viewport_rect = geometry.viewport;
            back_rect = geometry.back;
            forward_rect = geometry.forward;
            refresh_rect = geometry.refresh;
            quit_rect = geometry.quit;
            url_rect = geometry.url;
        })?;
        last_back_visual = Some(back_visual);
        last_forward_visual = Some(forward_visual);
        last_refresh_visual = Some(refresh_visual);
        last_quit_visual = Some(quit_visual);
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
                "{base_render_trace}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
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
                REFRESH_IDLE_SYMBOL,
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
                        ("refresh_symbol", REFRESH_IDLE_SYMBOL.to_string()),
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
                // Hard before soft: Super+Shift+R is hard (Shift wins).
                if local_hard_refresh_key(&mode, key) {
                    dispatch_refresh_ignore_cache(
                        "keyboard",
                        &refresh_control,
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
                                CommandResult::RefreshSoft => {
                                    let sent = dispatch_refresh(
                                        "command",
                                        &refresh_control,
                                        back_route.as_ref(),
                                        &compositor,
                                        &browser_conn,
                                        &mut state_trace,
                                    );
                                    if !sent {
                                        command_error = Some(
                                            if !refresh_control.can_refresh
                                                || refresh_control.active_tab_id <= 0
                                            {
                                                "Refresh is unavailable".into()
                                            } else {
                                                "Refresh failed (no route)".into()
                                            },
                                        );
                                    }
                                }
                                CommandResult::RefreshHard => {
                                    let sent = dispatch_refresh_ignore_cache(
                                        "command",
                                        &refresh_control,
                                        back_route.as_ref(),
                                        &compositor,
                                        &browser_conn,
                                        &mut state_trace,
                                    );
                                    if !sent {
                                        command_error = Some(
                                            if !refresh_control.can_refresh
                                                || refresh_control.active_tab_id <= 0
                                            {
                                                "Hard refresh is unavailable".into()
                                            } else {
                                                "Hard refresh failed (no route)".into()
                                            },
                                        );
                                    }
                                }
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
                let quit_hit = rect_contains(quit_rect, mouse.column, mouse.row);
                let url_hit = rect_contains(url_rect, mouse.column, mouse.row);
                let back_actionable = back_control.actionable(back_route.as_ref());
                let forward_actionable = forward_control.actionable(back_route.as_ref());
                let refresh_actionable = refresh_control.actionable(back_route.as_ref());
                // Control + click URL bar → Edit + Insert at click (not nav buttons).
                if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
                    && matches!(mode, Mode::Control)
                    && !is_devtools
                    && url_hit
                    && !back_hit
                    && !forward_hit
                    && !refresh_hit
                    && !quit_hit
                {
                    enter_url_insert_from_click(
                        &mut editor_state,
                        &mut editor_url,
                        &url,
                        &mut mode,
                        url_rect,
                        mouse.column,
                    );
                    back_control.clear_interaction();
                    forward_control.clear_interaction();
                    refresh_control.clear_interaction();
                    quit_control.clear_interaction();
                    if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                        conn.send_mode_changed(pid, false);
                    }
                    if let Some(trace) = state_trace.as_mut() {
                        let inner = chrome_inner_rect(url_rect);
                        let col = url_click_cursor_col(&url, inner, mouse.column);
                        trace.write(
                            "url_click_insert",
                            &[
                                ("column", mouse.column.to_string()),
                                ("row", mouse.row.to_string()),
                                ("cursor_col", col.to_string()),
                                ("url_len", url.len().to_string()),
                            ],
                        );
                    }
                    continue;
                }
                if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                    if back_hit {
                        forward_control.clear_interaction();
                        refresh_control.clear_interaction();
                        quit_control.clear_interaction();
                    } else if forward_hit {
                        back_control.clear_interaction();
                        refresh_control.clear_interaction();
                        quit_control.clear_interaction();
                    } else if refresh_hit {
                        back_control.clear_interaction();
                        forward_control.clear_interaction();
                        quit_control.clear_interaction();
                    } else if quit_hit {
                        back_control.clear_interaction();
                        forward_control.clear_interaction();
                        refresh_control.clear_interaction();
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
                let quit_result = update_quit_mouse(&mut quit_control, quit_rect, mouse);
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
                if quit_result.activate {
                    // Same process quit as Ctrl+C / :quit / q (not tab-close).
                    break;
                } else if back_result.activate {
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
                    // Toolbar is always soft. Terminal Shift+mouse is not reported
                    // to the TUI (Ghostty mouse-shift-capture); hard refresh is
                    // keyboard + `:refresh hard` (Issue 26072209562907 Exp 2).
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
                        // Page-load progress does not override an active download bar.
                        if download_bar_active {
                            continue;
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
                    ipc::CompositorMessage::DownloadProgress {
                        tab_id,
                        state,
                        received_bytes,
                        total_bytes,
                    } => {
                        // Download progress wins over page-load bar (Exp 3).
                        if tab_id != 0 && tab_id != current_tab_id {
                            continue;
                        }
                        let mut stdout = io::stdout();
                        let _ = match state.as_str() {
                            "active" => {
                                download_bar_active = true;
                                loading_bar_active = false;
                                loading_bar_start = None;
                                if total_bytes > 0 {
                                    let pct = ((received_bytes.min(total_bytes) as f64
                                        / total_bytes as f64)
                                        * 100.0)
                                        .round()
                                        .clamp(0.0, 100.0)
                                        as u8;
                                    // OSC 9;4;1;N — ConEmu determinate progress.
                                    write!(stdout, "\x1b]9;4;1;{}\x1b\\", pct)
                                } else {
                                    // OSC 9;4;3 — indeterminate bounce.
                                    write!(stdout, "\x1b]9;4;3\x1b\\")
                                }
                            }
                            "done" | "cancelled" => {
                                download_bar_active = false;
                                write!(stdout, "\x1b]9;4;0\x1b\\")
                            }
                            "error" => {
                                download_bar_active = false;
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

/// Expand a bare TCP port to `http://localhost:<port>` (Issue 26072812482260).
///
/// `trimmed` must already be whitespace-trimmed. Matches only when every byte is
/// an ASCII digit and the value parses as `u16` in `1..=65535`. Result uses the
/// parsed port (canonical decimal). Returns `None` for non-matches so callers
/// continue normal resolution.
fn expand_port_shortcut(trimmed: &str) -> Option<String> {
    if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let port: u16 = trimmed.parse().ok()?;
    if (1..=65535).contains(&port) {
        Some(format!("http://localhost:{port}"))
    } else {
        None
    }
}

/// Resolve bare input to a URL or file:// path (Issue 26030112000693).
///
/// Returns `None` if the input is not recognizable as a URL, file, or command.
/// Callers should show an error for `None`.
fn resolve_input(input: &str) -> Option<String> {
    let trimmed = input.trim();

    // Step 0: Bare port → http://localhost:<port> (Issue 26072812482260).
    // Shared by CLI open and URL-bar navigate.
    if let Some(url) = expand_port_shortcut(trimmed) {
        return Some(url);
    }

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

/// Force a full terminal repaint without cursor-position CPR (`CSI 6n`).
///
/// On ratatui-core ≥ 0.1.2, [`Terminal::clear`] calls `get_cursor_position` so
/// it can restore the cursor. That races ahweb's background `event::read`
/// thread and times out (Issue 26080220381373). For a fullscreen viewport,
/// [`Terminal::resize`] to the current size clears via `clear_viewport` /
/// `ClearType::All` and resets buffers **without** a CPR query.
fn force_full_redraw<B: Backend>(terminal: &mut Terminal<B>) -> Result<(), B::Error> {
    let area: Rect = terminal.size()?.into();
    terminal.resize(area)
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

/// Product chrome display label for a browser selector or helper path.
///
/// Public helper naming convention (display-only; does not select engines):
/// 1. Take the path **basename** (last `/` segment).
/// 2. If the basename is `{prefix}-{stem}d` (last hyphen, non-empty stem, trailing
///    `d`), display `{stem}` — e.g. `ah-chromiumd` → `chromium`,
///    `my-cool-webkitd` → `webkit`, `vendor-foobard` → `foobar`.
/// 3. Otherwise display the basename unchanged (`chromium`, `custom-engine`).
fn browser_display_label(browser: &str) -> &str {
    let basename = browser.rsplit('/').next().unwrap_or(browser);
    if let Some(hyphen) = basename.rfind('-') {
        let after = &basename[hyphen + 1..];
        // Non-empty stem + trailing 'd' (stem is everything before the final d).
        if after.len() >= 2 && after.as_bytes().last() == Some(&b'd') {
            let stem = &after[..after.len() - 1];
            if !stem.is_empty() {
                return stem;
            }
        }
    }
    basename
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BrowserLayout {
    back_area: Rect,
    forward_area: Rect,
    refresh_area: Rect,
    quit_area: Rect,
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
    // Four equal chrome controls: back, forward, refresh (left) + quit (right).
    // Same max cell budget as the old three-button row (5 each → 20), leave ≥1
    // for the URL when possible.
    let chrome_budget = top.width.saturating_sub(1).min(20);
    let back_width = chrome_budget.saturating_add(3) / 4;
    let forward_width = chrome_budget.saturating_add(2) / 4;
    let refresh_width = chrome_budget.saturating_add(1) / 4;
    let quit_width = chrome_budget / 4;
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
    let left_chrome = back_width
        .saturating_add(forward_width)
        .saturating_add(refresh_width);
    let quit_area = Rect::new(
        top.x.saturating_add(top.width.saturating_sub(quit_width)),
        top.y,
        quit_width,
        top.height,
    );
    let url_area = Rect::new(
        top.x.saturating_add(left_chrome),
        top.y,
        top.width
            .saturating_sub(left_chrome)
            .saturating_sub(quit_width),
        top.height,
    );

    BrowserLayout {
        back_area,
        forward_area,
        refresh_area,
        quit_area,
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
    quit: Rect,
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
    quit_control: &QuitControlState,
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
    let quit_area = layout.quit_area;
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
    render_refresh_button(frame, refresh_area, refresh_control, back_route_available);
    render_quit_button(frame, quit_area, quit_control);

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
        quit: quit_area,
        url: url_area,
    }
}

/// Shared chrome border geometry: always rounded. Interaction/disabled state
/// is color (and fill) only — never Plain/Double/Thick shape changes.
fn chrome_border_type() -> BorderType {
    BorderType::Rounded
}

/// Nav chrome colors: disabled / pressed / idle only.
/// Pointer hover is tracked for hit-testing but must not change paint (flicker).
fn nav_button_colors(actionable: bool, pressed: bool) -> (Color, Color, Color) {
    if !actionable {
        (DIM, BG, BORDER)
    } else if pressed {
        (BG, CYAN, CYAN)
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
    let pressed = actionable && state.pressed.is_some();
    let (fg, bg, border) = nav_button_colors(actionable, pressed);
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
    let pressed = actionable && state.pressed.is_some();
    let (fg, bg, border) = nav_button_colors(actionable, pressed);
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
    route_available: bool,
) {
    let actionable = state.can_refresh && state.active_tab_id > 0 && route_available;
    let pressed = actionable && state.pressed.is_some();
    let (fg, bg, border) = nav_button_colors(actionable, pressed);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(chrome_border_type())
        .border_style(Style::default().fg(border).bg(bg))
        .style(Style::default().fg(fg).bg(bg));
    let button = Paragraph::new(REFRESH_IDLE_SYMBOL)
        .alignment(Alignment::Center)
        .style(Style::default().fg(fg).bg(bg))
        .block(block);
    frame.render_widget(button, area);
}

fn render_quit_button(frame: &mut Frame, area: Rect, state: &QuitControlState) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let actionable = state.actionable();
    let pressed = actionable && state.pressed;
    let (fg, bg, border) = nav_button_colors(actionable, pressed);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(chrome_border_type())
        .border_style(Style::default().fg(border).bg(bg))
        .style(Style::default().fg(fg).bg(bg));
    let button = Paragraph::new(QUIT_SYMBOL)
        .alignment(Alignment::Center)
        .style(Style::default().fg(fg).bg(bg))
        .block(block);
    frame.render_widget(button, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::{Backend, ClearType, TestBackend, WindowSize};
    use ratatui::buffer::{Buffer, Cell};
    use ratatui::layout::{Position, Size};
    use ratatui::Terminal;
    use std::io;

    /// Counts `get_cursor_position` calls so CPR-free paths can be asserted.
    struct CursorQueryCounter {
        inner: TestBackend,
        queries: u32,
    }

    impl CursorQueryCounter {
        fn new(width: u16, height: u16) -> Self {
            Self {
                inner: TestBackend::new(width, height),
                queries: 0,
            }
        }
    }

    impl Backend for CursorQueryCounter {
        type Error = io::Error;

        fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
        where
            I: Iterator<Item = (u16, u16, &'a Cell)>,
        {
            self.inner
                .draw(content)
                .map_err(|e| io::Error::other(e.to_string()))
        }

        fn hide_cursor(&mut self) -> io::Result<()> {
            self.inner
                .hide_cursor()
                .map_err(|e| io::Error::other(e.to_string()))
        }

        fn show_cursor(&mut self) -> io::Result<()> {
            self.inner
                .show_cursor()
                .map_err(|e| io::Error::other(e.to_string()))
        }

        fn get_cursor_position(&mut self) -> io::Result<Position> {
            self.queries = self.queries.saturating_add(1);
            self.inner
                .get_cursor_position()
                .map_err(|e| io::Error::other(e.to_string()))
        }

        fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
            self.inner
                .set_cursor_position(position)
                .map_err(|e| io::Error::other(e.to_string()))
        }

        fn clear(&mut self) -> io::Result<()> {
            self.inner
                .clear()
                .map_err(|e| io::Error::other(e.to_string()))
        }

        fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
            self.inner
                .clear_region(clear_type)
                .map_err(|e| io::Error::other(e.to_string()))
        }

        fn size(&self) -> io::Result<Size> {
            self.inner
                .size()
                .map_err(|e| io::Error::other(e.to_string()))
        }

        fn window_size(&mut self) -> io::Result<WindowSize> {
            self.inner
                .window_size()
                .map_err(|e| io::Error::other(e.to_string()))
        }

        fn flush(&mut self) -> io::Result<()> {
            self.inner
                .flush()
                .map_err(|e| io::Error::other(e.to_string()))
        }
    }

    #[test]
    fn force_full_redraw_does_not_query_cursor_position() {
        let backend = CursorQueryCounter::new(40, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(Paragraph::new("before"), frame.area());
            })
            .unwrap();
        // Draw may hide/show cursor; reset after paint so only redraw is counted.
        terminal.backend_mut().queries = 0;

        force_full_redraw(&mut terminal).expect("force_full_redraw");

        assert_eq!(
            terminal.backend().queries, 0,
            "force_full_redraw must not call get_cursor_position (CPR / CSI 6n)"
        );

        // Next draw still works (buffers were invalidated).
        terminal
            .draw(|frame| {
                frame.render_widget(Paragraph::new("after"), frame.area());
            })
            .unwrap();
    }

    #[test]
    fn nav_redraw_source_does_not_call_terminal_clear() {
        // Structural: production main (before tests) must not call Terminal::clear —
        // that API issues CPR on ratatui-core 0.1.2+ and races event::read.
        let main_src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs"));
        let production = main_src
            .split("#[cfg(test)]")
            .next()
            .expect("production section before tests");
        assert!(
            production.contains("force_full_redraw"),
            "expected force_full_redraw helper in production main.rs"
        );
        // Match the call form only (not prose comments that mention the API).
        let call_sites: Vec<_> = production
            .lines()
            .filter(|line| {
                let trimmed = line.trim_start();
                !trimmed.starts_with("//") && trimmed.contains("terminal.clear")
            })
            .collect();
        assert!(
            call_sites.is_empty(),
            "production must not call terminal.clear (found: {call_sites:?})"
        );
    }

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
        quit: Rect,
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
        let mut quit = Rect::default();
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
                    &QuitControlState::default(),
                    route_available,
                );
                viewport = geometry.viewport;
                back = geometry.back;
                forward = geometry.forward;
                refresh = geometry.refresh;
                quit = geometry.quit;
                url = geometry.url;
            })
            .unwrap();

        RenderProbe {
            viewport,
            back,
            forward,
            refresh,
            quit,
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
        let mut quit = Rect::default();
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
                    &QuitControlState::default(),
                    false,
                );
                viewport = geometry.viewport;
                back = geometry.back;
                forward = geometry.forward;
                refresh = geometry.refresh;
                quit = geometry.quit;
                url = geometry.url;
            })
            .unwrap();

        RenderProbe {
            viewport,
            back,
            forward,
            refresh,
            quit,
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
    fn refresh_command_soft_and_hard_dispatch() {
        // Issue 26072209562907 Exp 2 — real dispatch() entry, not reimplemented.
        assert!(matches!(dispatch("refresh"), CommandResult::RefreshSoft));
        assert!(matches!(
            dispatch("refresh hard"),
            CommandResult::RefreshHard
        ));
        assert!(matches!(dispatch("refresh h"), CommandResult::RefreshHard));
        assert!(matches!(
            dispatch("refresh ignore-cache"),
            CommandResult::RefreshHard
        ));
        match dispatch("refresh soft") {
            CommandResult::Error(msg) => {
                assert!(msg.contains("Usage: refresh | refresh hard"), "{msg}");
            }
            other => panic!("expected usage error, got {other:?}"),
        }
        assert!(matches!(dispatch("refres"), CommandResult::None));
    }

    #[test]
    fn refresh_command_short_aliases_r_and_rh() {
        // Issue 26072209562907 Exp 3 — :r soft, :rh hard via real dispatch().
        assert!(matches!(dispatch("r"), CommandResult::RefreshSoft));
        assert!(matches!(dispatch("rh"), CommandResult::RefreshHard));
        // :r hard still maps through refresh family alias.
        assert!(matches!(dispatch("r hard"), CommandResult::RefreshHard));
        match dispatch("rh x") {
            CommandResult::Error(msg) => assert!(msg.contains("Usage: rh"), "{msg}"),
            other => panic!("expected usage error, got {other:?}"),
        }
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
    fn chrome_ground_matches_product_austin_night_plate() {
        // Exp darker-page-ground / Exp 4: shipped BG is the real chrome ground constant.
        assert_eq!(BG, Color::Rgb(0x09, 0x09, 0x0d));
        assert_ne!(BG, Color::Rgb(0x1a, 0x1b, 0x26));
        // Accents stay Tokyo Night–family (unchanged this exp).
        assert_eq!(FG, Color::Rgb(0xc0, 0xca, 0xf5));
        assert_eq!(CYAN, Color::Rgb(0x7d, 0xcf, 0xff));
        assert_eq!(BORDER, Color::Rgb(0x56, 0x5f, 0x89));
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
        // Same path as product: raw selector → browser_display_label → ui.
        let chromium = render_loading_probe(browser_display_label("ah-chromiumd"));
        assert!(
            chromium.capture.contains("Waiting for chromium"),
            "loading should show stem for ah-chromiumd\n{}",
            chromium.capture
        );
        // Non-chromium convention name so a hard-coded chromium-only map fails.
        let third_party = render_loading_probe(browser_display_label("vendor-foobard"));
        assert!(
            third_party.capture.contains("Waiting for foobar"),
            "loading should show convention stem for vendor-foobard\n{}",
            third_party.capture
        );
        assert!(
            !third_party.capture.contains("Waiting for vendor-foobard"),
            "loading must not show raw helper basename when convention applies\n{}",
            third_party.capture
        );
        // Engine-neutral warning must not hardcode a different engine family.
        assert!(
            chromium
                .capture
                .contains("The first time you load a web browser"),
            "loading screen should show immediate engine-neutral warning\n{}",
            chromium.capture
        );
    }

    #[test]
    fn browser_display_label_follows_stem_d_convention() {
        for (input, expected) in [
            // Product + path
            ("ah-chromiumd", "chromium"),
            ("/opt/homebrew/bin/ah-chromiumd", "chromium"),
            // Third-party / multi-hyphen (last -…d wins)
            ("asdf-webkitd", "webkit"),
            ("my-cool-webkitd", "webkit"),
            ("vendor-foobard", "foobar"),
            ("ah-ladybirdd", "ladybird"),
            ("ah-geckod", "gecko"),
            // Bare names (no convention match)
            ("chromium", "chromium"),
            ("webkit", "webkit"),
            ("custom-engine", "custom-engine"),
            ("/tmp/custom-engine", "custom-engine"),
            // Non-matches
            ("somethingd", "somethingd"), // no hyphen
            ("ah-d", "ah-d"),             // empty stem after strip
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
            let (quit_x, quit_y) = find_cell(&rendered.buffer, QUIT_SYMBOL);
            assert!(
                back_x < forward_x
                    && forward_x < refresh_x
                    && refresh_x < rendered.url.x
                    && rendered.url.right() == rendered.quit.x
                    && quit_x >= rendered.quit.x,
                "{}",
                rendered.capture
            );
            assert!(back_y >= rendered.back.y && back_y < rendered.back.bottom());
            assert!(forward_y >= rendered.forward.y && forward_y < rendered.forward.bottom());
            assert!(refresh_y >= rendered.refresh.y && refresh_y < rendered.refresh.bottom());
            assert!(quit_y >= rendered.quit.y && quit_y < rendered.quit.bottom());
            assert_eq!(refresh_x, rendered.refresh.x + rendered.refresh.width / 2);
            assert_eq!(refresh_y, rendered.refresh.y + rendered.refresh.height / 2);
            assert_eq!(rendered.back.right(), rendered.forward.x);
            assert_eq!(rendered.forward.right(), rendered.refresh.x);
            assert_eq!(rendered.refresh.right(), rendered.url.x);
            assert_eq!(rendered.url.right(), rendered.quit.x);
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
    fn back_button_buffer_styles_cover_disabled_normal_hover_noop_and_pressed() {
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

        // Hover must not change paint vs idle (no SELECTION fill).
        let mut hover_state = enabled_back_state();
        hover_state.hovered = true;
        let hover = render_probe_with_back(Mode::Control, 80, 18, None, hover_state, true);
        let (x, y) = find_cell(&hover.buffer, BACK_SYMBOL);
        assert_eq!(hover.buffer[(x, y)].fg, FG);
        assert_eq!(hover.buffer[(x, y)].bg, BG);
        assert_eq!(hover.buffer[(hover.back.x, hover.back.y)].fg, CYAN);
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
    fn forward_button_buffer_styles_cover_disabled_hover_noop_and_pressed() {
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
            true,
        );
        let (x, y) = find_cell(&hover.buffer, FORWARD_SYMBOL);
        assert_eq!(hover.buffer[(x, y)].fg, FG);
        assert_eq!(hover.buffer[(x, y)].bg, BG);
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
    fn refresh_button_always_paints_static_idle_glyph() {
        // Former spinner frames must not appear on the Control refresh control.
        const FORMER_SPINNER_FRAMES: [&str; 4] = ["⟳", "↻", "↺", "⟲"];
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
            true,
        );
        let (x, y) = find_cell(&pressed.buffer, REFRESH_IDLE_SYMBOL);
        assert_eq!(pressed.buffer[(x, y)].fg, BG);
        assert_eq!(pressed.buffer[(x, y)].bg, CYAN);
        assert_eq!(pressed.buffer[(x, y)].symbol(), REFRESH_IDLE_SYMBOL);

        let idle = render_probe_with_navigation(
            Mode::Control,
            80,
            18,
            None,
            BackControlState::default(),
            ForwardControlState::default(),
            enabled_refresh_state(),
            true,
        );
        let (idle_x, idle_y) = find_cell(&idle.buffer, REFRESH_IDLE_SYMBOL);
        assert_eq!(idle.buffer[(idle_x, idle_y)].symbol(), REFRESH_IDLE_SYMBOL);
        for frame in FORMER_SPINNER_FRAMES {
            assert!(
                !idle.capture.contains(frame),
                "idle refresh probe must not contain former spinner frame {frame}"
            );
            assert!(
                !pressed.capture.contains(frame),
                "pressed refresh probe must not contain former spinner frame {frame}"
            );
        }
        assert_eq!(idle.refresh, pressed.refresh);
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
    fn navigation_url_and_quit_geometry_invariants_hold_across_widths() {
        for width in 0..=120u16 {
            let area = Rect::new(0, 0, width, 18);
            let layout = browser_layout(area, None);
            let top = Rect::new(0, 0, width, 3);
            assert_eq!(layout.back_area.x, top.x, "width={width}");
            if width > 0 {
                assert_eq!(
                    layout.quit_area.right(),
                    top.right(),
                    "quit right-aligned width={width}"
                );
            }
            // Quit matches the equal chrome slice of the four-button budget.
            let chrome_budget = width.saturating_sub(1).min(20);
            assert_eq!(
                layout.quit_area.width,
                chrome_budget / 4,
                "quit same unit as chrome budget width={width}"
            );
            assert_eq!(
                layout.back_area.right(),
                layout.forward_area.x,
                "width={width}"
            );
            assert_eq!(
                layout.forward_area.right(),
                layout.refresh_area.x,
                "width={width}"
            );
            assert_eq!(
                layout.refresh_area.right(),
                layout.url_area.x,
                "width={width}"
            );
            assert_eq!(
                layout.url_area.right(),
                layout.quit_area.x,
                "url ends at quit width={width}"
            );
            assert_eq!(
                layout.back_area.width
                    + layout.forward_area.width
                    + layout.refresh_area.width
                    + layout.url_area.width
                    + layout.quit_area.width,
                width,
                "width={width}"
            );
            // On wide rows, all four chrome buttons share the same width.
            if width >= 21 {
                assert_eq!(
                    layout.back_area.width, layout.quit_area.width,
                    "back==quit width={width}"
                );
                assert_eq!(
                    layout.forward_area.width, layout.quit_area.width,
                    "forward==quit width={width}"
                );
                assert_eq!(
                    layout.refresh_area.width, layout.quit_area.width,
                    "refresh==quit width={width}"
                );
            }
        }

        // Spot-check typical 80-col: four chrome cells of 5, url 60.
        let wide = browser_layout(Rect::new(0, 0, 80, 18), None);
        assert_eq!(wide.back_area.width, 5);
        assert_eq!(wide.forward_area.width, 5);
        assert_eq!(wide.refresh_area.width, 5);
        assert_eq!(wide.quit_area, Rect::new(75, 0, 5, 3));
        assert_eq!(wide.url_area, Rect::new(15, 0, 60, 3));

        // width=6: chrome_budget=5 → widths (2,1,1,1) + url=1
        let narrow = render_probe_with_back(Mode::Control, 6, 7, None, enabled_back_state(), true);
        assert_eq!(narrow.back, Rect::new(0, 0, 2, 3));
        assert_eq!(narrow.forward, Rect::new(2, 0, 1, 3));
        assert_eq!(narrow.refresh, Rect::new(3, 0, 1, 3));
        assert_eq!(narrow.url, Rect::new(4, 0, 1, 3));
        assert_eq!(narrow.quit, Rect::new(5, 0, 1, 3));
    }

    #[test]
    fn quit_mouse_activate_requests_process_quit_not_tab_close() {
        let rect = Rect::new(70, 0, 3, 3);
        let mut state = QuitControlState::default();
        assert!(state.actionable());
        assert!(
            !update_quit_mouse(
                &mut state,
                rect,
                test_mouse(MouseEventKind::Down(MouseButton::Left), 71, 1),
            )
            .activate
        );
        assert!(state.pressed);
        let up = update_quit_mouse(
            &mut state,
            rect,
            test_mouse(MouseEventKind::Up(MouseButton::Left), 71, 1),
        );
        assert!(up.activate, "completed click must signal process quit");
        assert!(!state.pressed);

        // Leave-release cancels.
        update_quit_mouse(
            &mut state,
            rect,
            test_mouse(MouseEventKind::Down(MouseButton::Left), 71, 1),
        );
        assert!(
            !update_quit_mouse(
                &mut state,
                rect,
                test_mouse(MouseEventKind::Up(MouseButton::Left), 10, 1),
            )
            .activate
        );
        assert!(!state.pressed);

        // :quit command remains process quit (not tab-close).
        assert!(matches!(
            (COMMANDS
                .iter()
                .find(|c| c.names.contains(&"quit"))
                .unwrap()
                .exec)(&[]),
            CommandResult::Quit
        ));
    }

    #[test]
    fn quit_button_paints_idle_and_pressed_without_hover_wash() {
        let area = Rect::new(0, 0, 5, 3);
        let backend = TestBackend::new(5, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        let idle = QuitControlState::default();
        terminal
            .draw(|frame| render_quit_button(frame, area, &idle))
            .unwrap();
        let idle_buf = terminal.backend().buffer().clone();
        let (x, y) = find_cell(&idle_buf, QUIT_SYMBOL);
        let idle_style = idle_buf[(x, y)].style();

        let mut hover = QuitControlState {
            hovered: true,
            pressed: false,
        };
        terminal
            .draw(|frame| render_quit_button(frame, area, &hover))
            .unwrap();
        let hover_buf = terminal.backend().buffer().clone();
        let hover_style = hover_buf[(x, y)].style();
        assert_eq!(idle_style, hover_style, "hover must not change quit paint");

        hover.pressed = true;
        terminal
            .draw(|frame| render_quit_button(frame, area, &hover))
            .unwrap();
        let pressed_buf = terminal.backend().buffer().clone();
        let pressed_style = pressed_buf[(x, y)].style();
        assert_ne!(idle_style.bg, pressed_style.bg, "pressed must change fill");
    }

    #[test]
    fn url_click_cursor_col_clamps_to_url_line_bounds() {
        // Outer chrome: x=10,width=20 → inner x=11,width=18 (1-cell borders).
        let outer = Rect::new(10, 0, 20, 3);
        let inner = chrome_inner_rect(outer);
        assert_eq!(inner.x, 11);
        assert_eq!(inner.width, 18);

        let url = "https://example.com/path";
        assert_eq!(url.len(), 24);
        // Left of / on inner → 0
        assert_eq!(url_click_cursor_col(url, inner, 0), 0);
        assert_eq!(url_click_cursor_col(url, inner, inner.x), 0);
        // First text cell
        assert_eq!(url_click_cursor_col(url, inner, inner.x + 1), 1);
        // Mid-string (offset 10)
        assert_eq!(url_click_cursor_col(url, inner, inner.x + 10), 10);
        // Past end of URL but inside bar
        assert_eq!(url_click_cursor_col(url, inner, inner.x + 100), url.len());
        // Empty URL
        assert_eq!(url_click_cursor_col("", inner, inner.x + 5), 0);
    }

    #[test]
    fn enter_url_insert_from_click_sets_edit_insert_and_cursor() {
        let url = DEFAULT_HOMEPAGE_URL;
        let outer = Rect::new(0, 0, 40, 3);
        let mut editor_state = EditorState::new(Lines::from("stale"));
        editor_state.set_clipboard(UrlClipboard::new());
        let mut editor_url = "stale".to_string();
        let mut mode = Mode::Control;
        // Click at first character of inner (offset 0 from first text cell is col 0 at x=inner.x
        // Using mouse_col = inner.x + 7 → col 7
        let inner = chrome_inner_rect(outer);
        let mouse_col = inner.x + 7;
        enter_url_insert_from_click(
            &mut editor_state,
            &mut editor_url,
            url,
            &mut mode,
            outer,
            mouse_col,
        );
        assert!(matches!(mode, Mode::Edit));
        assert!(matches!(editor_state.mode, EditorMode::Insert));
        assert_eq!(editor_url, url);
        assert_eq!(editor_state.cursor.row, 0);
        assert_eq!(editor_state.cursor.col, 7);
        assert!(editor_state.selection.is_none());
        // Same URL again: still repositions cursor
        enter_url_insert_from_click(
            &mut editor_state,
            &mut editor_url,
            url,
            &mut mode,
            outer,
            inner.x + 3,
        );
        assert_eq!(editor_state.cursor.col, 3);
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
        let hard_shift_r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::SHIFT);
        let hard_cmd_shift_r = KeyEvent::new(
            KeyCode::Char('R'),
            KeyModifiers::SUPER | KeyModifiers::SHIFT,
        );
        assert!(local_back_key(&Mode::Control, key));
        assert!(local_back_key(&Mode::Browse, key));
        assert!(local_forward_key(&Mode::Control, forward_key));
        assert!(local_forward_key(&Mode::Browse, forward_key));
        assert!(local_refresh_key(&Mode::Control, refresh_key));
        assert!(local_refresh_key(&Mode::Browse, refresh_key));
        assert!(!local_hard_refresh_key(&Mode::Control, refresh_key));
        assert!(local_hard_refresh_key(&Mode::Control, hard_shift_r));
        assert!(local_hard_refresh_key(&Mode::Browse, hard_shift_r));
        assert!(local_hard_refresh_key(&Mode::Control, hard_cmd_shift_r));
        assert!(!local_refresh_key(&Mode::Control, hard_cmd_shift_r));
        for mode in [Mode::Edit, Mode::Command, Mode::Dialog, Mode::Auth] {
            assert!(!local_back_key(&mode, key));
            assert!(!local_forward_key(&mode, forward_key));
            assert!(!local_refresh_key(&mode, refresh_key));
            assert!(!local_hard_refresh_key(&mode, hard_shift_r));
        }
        assert!(!local_back_key(
            &Mode::Control,
            KeyEvent::new(KeyCode::Char('['), KeyModifiers::CONTROL)
        ));

        // Toolbar no longer hard-refreshes via Shift (Exp 2); hard is keyboard
        // + command bar only. Soft Super+R still soft.
        assert!(local_refresh_key(
            &Mode::Control,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::SUPER)
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
    fn event_polling_old_completed_page_blocks_without_active_reason() {
        let now = Instant::now();
        assert!(!needs_event_polling(
            true,
            Some(now - Duration::from_secs(3)),
            None,
            now,
        ));
    }

    #[test]
    fn event_polling_cold_load_grace_and_copy_feedback_are_independent() {
        let now = Instant::now();
        assert!(needs_event_polling(false, None, None, now));
        assert!(needs_event_polling(
            true,
            Some(now - Duration::from_secs(1)),
            None,
            now,
        ));
        assert!(needs_event_polling(
            true,
            None,
            Some(now + Duration::from_secs(1)),
            now,
        ));
    }

    #[test]
    fn event_polling_expired_copy_feedback_does_not_force_poll() {
        let now = Instant::now();
        assert!(!needs_event_polling(
            true,
            None,
            Some(now - Duration::from_secs(1)),
            now,
        ));
        // Expired page-load grace + expired copy feedback → no poll.
        assert!(!needs_event_polling(
            true,
            Some(now - Duration::from_secs(3)),
            Some(now - Duration::from_secs(1)),
            now,
        ));
    }

    /// Issue 26072812482260: bare port → http://localhost via shared resolve_input.
    #[test]
    fn resolve_input_bare_port_expands_to_localhost_http() {
        let cases = [
            ("3456", "http://localhost:3456"),
            ("1", "http://localhost:1"),
            ("65535", "http://localhost:65535"),
            ("8080", "http://localhost:8080"),
            ("03456", "http://localhost:3456"), // leading zeros → parsed port
            ("  8080  ", "http://localhost:8080"), // outer trim
        ];
        for (input, want) in cases {
            assert_eq!(
                resolve_input(input).as_deref(),
                Some(want),
                "input {input:?}"
            );
        }
    }

    #[test]
    fn resolve_input_rejects_invalid_bare_ports_without_localhost_rewrite() {
        // Not rewritten to localhost; bare junk stays unresolvable (None).
        for input in ["0", "65536", "80a", "", "   ", "12 34", "+80", "-1"] {
            assert_eq!(
                resolve_input(input),
                None,
                "invalid bare port/token {input:?} must not expand to localhost"
            );
        }
    }

    #[test]
    fn resolve_input_non_port_forms_unchanged() {
        assert_eq!(
            resolve_input("https://example.com/path").as_deref(),
            Some("https://example.com/path")
        );
        assert_eq!(
            resolve_input("localhost:3456").as_deref(),
            Some("http://localhost:3456")
        );
        assert_eq!(
            resolve_input("example.com").as_deref(),
            Some("https://example.com")
        );
        // host:port non-localhost still https-prefixed (existing rule)
        assert_eq!(
            resolve_input("example.com:443").as_deref(),
            Some("https://example.com:443")
        );
    }

    #[test]
    fn expand_port_shortcut_matches_resolve_input_port_contract() {
        assert_eq!(
            expand_port_shortcut("3456").as_deref(),
            Some("http://localhost:3456")
        );
        assert_eq!(expand_port_shortcut("0"), None);
        assert_eq!(expand_port_shortcut("65536"), None);
        assert_eq!(expand_port_shortcut("80a"), None);
        assert_eq!(expand_port_shortcut(""), None);
        // Leading zeros: parse path only on pure digits
        assert_eq!(
            expand_port_shortcut("03456").as_deref(),
            Some("http://localhost:3456")
        );
    }

}
