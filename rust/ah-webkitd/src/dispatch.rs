use std::ffi::{c_void, CString};
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::ffi::{self, TsWebContents};
use crate::proto::{self, Msg, TermSurfMessage};

// --- Tab registry ---

struct TabEntry {
    handle: TsWebContents,
    tab_id: i64,
    pane_id: String,
    inspected_tab_id: i64,
    last_url: String,
    can_go_back: bool,
    can_go_forward: bool,
    can_refresh: bool,
    pending_refresh_request_id: u64,
    refresh_request_armed: bool,
    crashed: bool,
}

static PDF_INPUT_TRACE_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
static RENDER_PROOF_TRACE_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
static LAST_LOADING_STATES: Mutex<Vec<TermSurfMessage>> = Mutex::new(Vec::new());
static LAST_NAVIGATION_STATES: Mutex<Vec<TermSurfMessage>> = Mutex::new(Vec::new());

struct DeferredHttpAuthCancel {
    wc: usize,
    request_id: u64,
}

unsafe extern "C" fn deferred_http_auth_cancel_task(data: *mut c_void) {
    let task = unsafe { Box::from_raw(data as *mut DeferredHttpAuthCancel) };
    let empty = CString::new("").unwrap();
    let ok = unsafe {
        ffi::ts_reply_http_auth(
            task.wc as TsWebContents,
            task.request_id,
            false,
            empty.as_ptr(),
            empty.as_ptr(),
        )
    };
    eprintln!(
        "[termsurf-http-auth] deferred-cancel request_id={} ok={}",
        task.request_id, ok
    );
}

fn defer_http_auth_cancel(wc: TsWebContents, request_id: u64) {
    let task = Box::new(DeferredHttpAuthCancel {
        wc: wc as usize,
        request_id,
    });
    unsafe {
        ffi::ts_post_task(
            Some(deferred_http_auth_cancel_task),
            Box::into_raw(task) as *mut c_void,
        );
    }
}

pub fn init_pdf_input_trace() {
    trace_pdf_input(format!("trace-init pid={}", std::process::id()));
}

fn trace_pdf_input(line: impl AsRef<str>) {
    let path = PDF_INPUT_TRACE_PATH.get_or_init(|| {
        if std::env::var_os("TERMSURF_PDF_INPUT_TRACE").is_none() {
            return None;
        }
        let path = std::env::var_os("TERMSURF_PDF_INPUT_TRACE_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("termsurf").join("pdf-input.log"));
        if let Some(parent) = path.parent() {
            let _ = create_dir_all(parent);
        }
        Some(path)
    });

    let Some(path) = path else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "webkit {}", line.as_ref());
    }
}

fn trace_render_probe(line: impl AsRef<str>) {
    let path = RENDER_PROOF_TRACE_PATH.get_or_init(|| {
        let path = std::env::var_os("ASTROHACKER_WEBKIT_RENDER_PROOF_TRACE_FILE")?;
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent() {
            let _ = create_dir_all(parent);
        }
        Some(path)
    });

    let Some(path) = path else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "webkit {}", line.as_ref());
    }
}

unsafe fn c_string(ptr: *const std::os::raw::c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

fn remember_loading_state(msg: &TermSurfMessage) {
    let Some(Msg::LoadingState(ref loading)) = msg.msg else {
        return;
    };
    let mut states = LAST_LOADING_STATES.lock().unwrap();
    match loading.state.as_str() {
        "loading" | "done" | "error" => {
            states.retain(|existing| {
                !matches!(
                    existing.msg,
                    Some(Msg::LoadingState(ref existing_loading)) if existing_loading.tab_id == loading.tab_id
                )
            });
            states.push(msg.clone());
        }
        "progress" => {
            states.retain(|existing| {
                !matches!(
                    existing.msg,
                    Some(Msg::LoadingState(ref existing_loading))
                        if existing_loading.tab_id == loading.tab_id
                            && existing_loading.state == "progress"
                )
            });
            states.push(msg.clone());
        }
        _ => states.push(msg.clone()),
    }
}

fn remember_navigation_state(msg: &TermSurfMessage) {
    let Some(Msg::NavigationState(ref state)) = msg.msg else {
        return;
    };
    let mut states = LAST_NAVIGATION_STATES.lock().unwrap();
    states.retain(|existing| {
        !matches!(existing.msg, Some(Msg::NavigationState(ref existing_state)) if existing_state.tab_id == state.tab_id)
    });
    states.push(msg.clone());
}

fn publish_navigation_state(
    tab_id: i64,
    can_go_back: bool,
    can_go_forward: bool,
    can_refresh: bool,
) {
    if tab_id <= 0 {
        return;
    }
    let msg = TermSurfMessage {
        msg: Some(Msg::NavigationState(proto::termsurf::NavigationState {
            tab_id,
            can_go_back,
            can_go_forward,
            can_refresh,
        })),
    };
    remember_navigation_state(&msg);
    crate::ipc::send(&msg);
}

pub fn replay_state_to_client(stream: &mut UnixStream) {
    let states = LAST_LOADING_STATES.lock().unwrap().clone();
    for msg in states {
        if let Some(Msg::LoadingState(ref loading)) = msg.msg {
            trace_pdf_input(format!(
                "loading-state-replay tab={} state={} progress={}",
                loading.tab_id, loading.state, loading.progress
            ));
        }
        let _ = crate::ipc::write_message(stream, &msg);
    }
    let states = LAST_NAVIGATION_STATES.lock().unwrap().clone();
    for msg in states {
        let _ = crate::ipc::write_message(stream, &msg);
    }
}

/// Global tab registry. Only accessed from the UI thread (via ts_post_task
/// and callbacks), so no synchronization needed — same pattern as Plusium's
/// `static std::vector<TabEntry>* g_tabs`.
fn tabs() -> &'static mut Vec<TabEntry> {
    static mut TABS: Vec<TabEntry> = Vec::new();
    unsafe { &mut *std::ptr::addr_of_mut!(TABS) }
}

fn find_by_handle(wc: TsWebContents) -> Option<&'static mut TabEntry> {
    tabs()
        .iter_mut()
        .find(|t| !t.handle.is_null() && t.handle == wc)
}

fn find_by_tab_id(tab_id: i64) -> Option<&'static mut TabEntry> {
    tabs().iter_mut().find(|t| t.tab_id == tab_id)
}

fn navigation_action_contract(action: &proto::termsurf::NavigationAction) -> bool {
    action.tab_id > 0
        && action.pane_id.is_empty()
        && match action.action.as_str() {
            "back" | "forward" => action.request_id == 0,
            "refresh" => action.request_id != 0,
            _ => false,
        }
}

fn clear_refresh_request(pending_request_id: &mut u64, armed: &mut bool) {
    *pending_request_id = 0;
    *armed = false;
}

fn navigation_action_enabled(action: &str, state: Option<(bool, bool, bool, bool)>) -> bool {
    match (action, state) {
        ("back", Some((can_go_back, _, _, false))) => can_go_back,
        ("forward", Some((_, can_go_forward, _, false))) => can_go_forward,
        ("refresh", Some((_, _, can_refresh, _))) => can_refresh,
        _ => false,
    }
}

fn cache_navigation_state(
    cached_back: &mut bool,
    cached_forward: &mut bool,
    can_go_back: bool,
    can_go_forward: bool,
) {
    *cached_back = can_go_back;
    *cached_forward = can_go_forward;
}

fn cache_navigation_crash(cached_back: &mut bool, cached_forward: &mut bool, crashed: &mut bool) {
    *cached_back = false;
    *cached_forward = false;
    *crashed = true;
}

fn cache_navigation_commit(crashed: &mut bool) {
    *crashed = false;
}

fn pdf_title_from_url(url: &str) -> Option<String> {
    let base = url.split(['?', '#']).next().unwrap_or(url);
    let raw_name = base.rsplit('/').next()?.trim();
    if raw_name.is_empty() {
        return None;
    }

    let decoded = percent_decode_path_segment(raw_name);
    if decoded.len() < 4 || !decoded[decoded.len() - 4..].eq_ignore_ascii_case(".pdf") {
        return None;
    }
    let title = decoded[..decoded.len() - 4].to_string();
    let title = title.trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

fn percent_decode_path_segment(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = bytes[i + 1];
            let lo = bytes[i + 2];
            if let (Some(hi), Some(lo)) = (hex_value(hi), hex_value(lo)) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

// --- String-to-int mappings ---

fn mouse_type(s: &str) -> i32 {
    match s {
        "down" => 0,
        "up" => 1,
        _ => 0,
    }
}

fn mouse_button(s: &str) -> i32 {
    match s {
        "left" => 0,
        "right" => 1,
        "middle" => 2,
        _ => 0,
    }
}

fn key_type(s: &str) -> i32 {
    match s {
        "down" => 0,
        "up" => 1,
        "repeat" => 2,
        _ => 0,
    }
}

// --- Message dispatch ---

pub fn handle_message(msg: &TermSurfMessage) {
    let Some(ref inner) = msg.msg else { return };
    match inner {
        Msg::CreateTab(m) => {
            let url = CString::new(m.url.as_str()).unwrap();
            trace_pdf_input(format!(
                "create-tab pane={} pixel_width={} pixel_height={} url={} dark={}",
                m.pane_id, m.pixel_width, m.pixel_height, m.url, m.dark
            ));
            tabs().push(TabEntry {
                handle: std::ptr::null_mut(),
                tab_id: 0,
                pane_id: m.pane_id.clone(),
                inspected_tab_id: 0,
                last_url: m.url.clone(),
                can_go_back: false,
                can_go_forward: false,
                can_refresh: false,
                pending_refresh_request_id: 0,
                refresh_request_armed: false,
                crashed: false,
            });
            let entry = tabs().last_mut().unwrap();
            entry.handle = unsafe {
                ffi::ts_create_web_contents(
                    crate::browser_context(),
                    url.as_ptr(),
                    m.pixel_width as i32,
                    m.pixel_height as i32,
                    m.dark,
                )
            };
            if entry.handle.is_null() {
                trace_pdf_input(format!(
                    "create-tab pane={} result=unsupported-null-handle",
                    m.pane_id
                ));
                tabs().pop();
            }
        }
        Msg::CreateDevtoolsTab(m) => {
            trace_pdf_input(format!(
                "create-devtools-tab pane={} inspected_tab_id={} pixel_width={} pixel_height={} ffi=ts_create_devtools_web_contents",
                m.pane_id, m.inspected_tab_id, m.pixel_width, m.pixel_height
            ));
            tabs().push(TabEntry {
                handle: std::ptr::null_mut(),
                tab_id: 0,
                pane_id: m.pane_id.clone(),
                inspected_tab_id: m.inspected_tab_id,
                last_url: String::new(),
                can_go_back: false,
                can_go_forward: false,
                can_refresh: false,
                pending_refresh_request_id: 0,
                refresh_request_armed: false,
                crashed: false,
            });
            let entry = tabs().last_mut().unwrap();
            entry.handle = unsafe {
                ffi::ts_create_devtools_web_contents(
                    crate::browser_context(),
                    m.inspected_tab_id as i32,
                    m.pixel_width as i32,
                    m.pixel_height as i32,
                    m.dark,
                )
            };
            if entry.handle.is_null() {
                trace_pdf_input(format!(
                    "create-devtools-tab pane={} inspected_tab_id={} result=devtools-unsupported",
                    m.pane_id, m.inspected_tab_id
                ));
                tabs().pop();
            }
        }
        Msg::Resize(m) => {
            if let Some(t) = find_by_tab_id(m.tab_id) {
                trace_pdf_input(format!(
                    "resize tab_id={} pane_id={} pixel_width={} pixel_height={} screen_x={} screen_y={} screen_width={} screen_height={} screen_scale={} ffi=ts_set_view_size",
                    m.tab_id,
                    t.pane_id,
                    m.pixel_width,
                    m.pixel_height,
                    m.screen_x,
                    m.screen_y,
                    m.screen_width,
                    m.screen_height,
                    m.screen_scale
                ));
                unsafe {
                    ffi::ts_set_view_size(
                        t.handle,
                        m.pixel_width as i32,
                        m.pixel_height as i32,
                        m.screen_x,
                        m.screen_y,
                        m.screen_width,
                        m.screen_height,
                        m.screen_scale,
                    );
                }
            } else {
                trace_pdf_input(format!(
                    "resize tab_id={} result=no-tab pixel_width={} pixel_height={}",
                    m.tab_id, m.pixel_width, m.pixel_height
                ));
            }
        }
        Msg::CloseTab(m) => {
            let tab_id = m.tab_id;
            if let Some(t) = find_by_tab_id(tab_id) {
                t.can_go_back = false;
                t.can_go_forward = false;
                t.crashed = true;
                publish_navigation_state(tab_id, false, false, false);
                trace_pdf_input(format!(
                    "close-tab tab_id={} pane_id={} result=destroying ffi=ts_destroy_web_contents",
                    tab_id, t.pane_id
                ));
                unsafe { ffi::ts_destroy_web_contents(t.handle) };
            } else {
                trace_pdf_input(format!("close-tab tab_id={} result=no-tab", tab_id));
            }
            tabs().retain(|t| t.tab_id != tab_id);
            LAST_NAVIGATION_STATES.lock().unwrap().retain(|existing| {
                !matches!(existing.msg, Some(Msg::NavigationState(ref state)) if state.tab_id == tab_id)
            });
            trace_pdf_input(format!("close-tab tab_id={} result=removed", tab_id));
            if tabs().is_empty() {
                trace_pdf_input(
                    "close-tab result=no-tabs-remaining ffi=ts_destroy_browser_context".to_string(),
                );
                unsafe { ffi::ts_destroy_browser_context(crate::browser_context()) };
                trace_pdf_input("close-tab result=no-tabs-remaining ffi=ts_quit".to_string());
                unsafe { ffi::ts_quit() };
                trace_pdf_input("close-tab result=no-tabs-remaining process-exit".to_string());
                std::process::exit(0);
            }
        }
        Msg::Navigate(m) => {
            if let Some(t) = find_by_tab_id(m.tab_id) {
                if let Ok(crash_url) = std::env::var("ASTROHACKER_WEBKIT_TEST_RENDERER_CRASH_URL") {
                    if !crash_url.is_empty() && m.url == crash_url {
                        trace_pdf_input(format!(
                            "test-renderer-crash tab={} pane={} url={} ffi=ts_webkit_test_kill_web_content_process",
                            m.tab_id, t.pane_id, m.url
                        ));
                        unsafe { ffi::ts_webkit_test_kill_web_content_process(t.handle) };
                        return;
                    }
                }
                let url = CString::new(m.url.as_str()).unwrap();
                trace_pdf_input(format!(
                    "navigate tab={} pane={} url={} ffi=ts_load_url",
                    m.tab_id, t.pane_id, m.url
                ));
                t.last_url = m.url.clone();
                unsafe { ffi::ts_load_url(t.handle, url.as_ptr()) };
            }
        }
        Msg::NavigationAction(m) => {
            if !navigation_action_contract(m) {
                trace_pdf_input(format!(
                    "navigation-action tab={} pane={} action={} result=invalid-contract",
                    m.tab_id, m.pane_id, m.action
                ));
                return;
            }
            let state = find_by_tab_id(m.tab_id)
                .map(|t| (t.can_go_back, t.can_go_forward, t.can_refresh, t.crashed));
            if state.is_none() {
                trace_pdf_input(format!(
                    "navigation-action tab={} action={} result=no-tab",
                    m.tab_id, m.action
                ));
                return;
            }
            if !navigation_action_enabled(&m.action, state) {
                trace_pdf_input(format!(
                    "navigation-action tab={} action={} result=disabled",
                    m.tab_id, m.action
                ));
                return;
            }
            let t = find_by_tab_id(m.tab_id).expect("validated WebKit navigation tab disappeared");
            if m.action == "refresh" {
                t.pending_refresh_request_id = m.request_id;
                t.refresh_request_armed = false;
            }
            let action = CString::new(m.action.as_str()).unwrap();
            let accepted = unsafe { ffi::ts_navigation_action(t.handle, action.as_ptr()) };
            if !accepted {
                match m.action.as_str() {
                    "back" => t.can_go_back = false,
                    "forward" => t.can_go_forward = false,
                    "refresh" => {
                        t.can_refresh = false;
                        clear_refresh_request(
                            &mut t.pending_refresh_request_id,
                            &mut t.refresh_request_armed,
                        );
                    }
                    _ => {}
                }
                publish_navigation_state(m.tab_id, t.can_go_back, t.can_go_forward, t.can_refresh);
            }
            trace_pdf_input(format!(
                "navigation-action tab={} pane={} action={} accepted={}",
                m.tab_id, t.pane_id, m.action, accepted
            ));
        }
        Msg::MouseEvent(m) => {
            if let Some(t) = find_by_tab_id(m.tab_id) {
                trace_pdf_input(format!(
                    "mouse-event tab={} pane={} ffi=ts_forward_mouse_event type={} button={} coords=({:.2}, {:.2}) click_count={} modifiers={}",
                    m.tab_id,
                    t.pane_id,
                    m.r#type,
                    m.button,
                    m.x,
                    m.y,
                    m.click_count,
                    m.modifiers
                ));
                unsafe {
                    ffi::ts_forward_mouse_event(
                        t.handle,
                        mouse_type(&m.r#type),
                        mouse_button(&m.button),
                        m.x as i32,
                        m.y as i32,
                        m.click_count as i32,
                        m.modifiers as i32,
                    );
                }
            } else {
                trace_pdf_input(format!(
                    "mouse-event tab={} result=no-tab type={} button={} coords=({:.2}, {:.2}) click_count={} modifiers={}",
                    m.tab_id,
                    m.r#type,
                    m.button,
                    m.x,
                    m.y,
                    m.click_count,
                    m.modifiers
                ));
            }
        }
        Msg::MouseMove(m) => {
            if let Some(t) = find_by_tab_id(m.tab_id) {
                trace_pdf_input(format!(
                    "mouse-move tab={} pane={} ffi=ts_forward_mouse_move coords=({:.2}, {:.2}) modifiers={}",
                    m.tab_id, t.pane_id, m.x, m.y, m.modifiers
                ));
                unsafe {
                    ffi::ts_forward_mouse_move(
                        t.handle,
                        m.x as i32,
                        m.y as i32,
                        m.modifiers as i32,
                    );
                }
            } else {
                trace_pdf_input(format!(
                    "mouse-move tab={} result=no-tab coords=({:.2}, {:.2}) modifiers={}",
                    m.tab_id, m.x, m.y, m.modifiers
                ));
            }
        }
        Msg::ScrollEvent(m) => {
            if let Some(t) = find_by_tab_id(m.tab_id) {
                trace_pdf_input(format!(
                    "scroll-event tab={} pane={} ffi=ts_forward_scroll_event coords=({:.2}, {:.2}) delta=({:.2}, {:.2}) phase={} momentum_phase={} precise={} modifiers={}",
                    m.tab_id,
                    t.pane_id,
                    m.x,
                    m.y,
                    m.delta_x,
                    m.delta_y,
                    m.phase,
                    m.momentum_phase,
                    m.precise,
                    m.modifiers
                ));
                unsafe {
                    ffi::ts_forward_scroll_event(
                        t.handle,
                        m.x as i32,
                        m.y as i32,
                        m.delta_x as f32,
                        m.delta_y as f32,
                        m.phase as i32,
                        m.momentum_phase as i32,
                        m.precise,
                        m.modifiers as i32,
                    );
                }
            } else {
                trace_pdf_input(format!(
                    "scroll-event tab={} result=no-tab coords=({:.2}, {:.2}) delta=({:.2}, {:.2})",
                    m.tab_id, m.x, m.y, m.delta_x, m.delta_y
                ));
            }
        }
        Msg::KeyEvent(m) => {
            if let Some(t) = find_by_tab_id(m.tab_id) {
                trace_pdf_input(format!(
                    "key-event tab={} pane={} ffi=ts_forward_key_event type={} windows_key_code={} utf8_len={} modifiers={}",
                    m.tab_id,
                    t.pane_id,
                    m.r#type,
                    m.windows_key_code,
                    m.utf8.len(),
                    m.modifiers
                ));
                let utf8 = CString::new(m.utf8.as_str()).unwrap();
                unsafe {
                    ffi::ts_forward_key_event(
                        t.handle,
                        key_type(&m.r#type),
                        m.windows_key_code as i32,
                        utf8.as_ptr(),
                        m.modifiers as i32,
                    );
                }
            } else {
                trace_pdf_input(format!(
                    "key-event tab={} result=no-tab type={} windows_key_code={} utf8_len={} modifiers={}",
                    m.tab_id,
                    m.r#type,
                    m.windows_key_code,
                    m.utf8.len(),
                    m.modifiers
                ));
            }
        }
        Msg::FocusChanged(m) => {
            if let Some(t) = find_by_tab_id(m.tab_id) {
                trace_pdf_input(format!(
                    "focus-changed tab={} pane={} ffi=ts_set_focus focused={}",
                    m.tab_id, t.pane_id, m.focused
                ));
                unsafe { ffi::ts_set_focus(t.handle, m.focused) };
            } else {
                trace_pdf_input(format!(
                    "focus-changed tab={} result=no-tab focused={}",
                    m.tab_id, m.focused
                ));
            }
        }
        Msg::SetGuiActive(m) => {
            let reason =
                CString::new(m.reason.as_str()).unwrap_or_else(|_| CString::new("").unwrap());
            if m.tab_id == 0 {
                let mut target_count = 0;
                for t in tabs().iter() {
                    if !t.handle.is_null() {
                        target_count += 1;
                        unsafe { ffi::ts_set_gui_active(t.handle, m.active, reason.as_ptr()) };
                    }
                }
                trace_pdf_input(format!(
                    "set-gui-active tab=0 active={} reason={} target_count={}",
                    m.active, m.reason, target_count
                ));
            } else if let Some(t) = find_by_tab_id(m.tab_id) {
                if !t.handle.is_null() {
                    trace_pdf_input(format!(
                        "set-gui-active tab={} pane={} active={} reason={} target_count=1",
                        m.tab_id, t.pane_id, m.active, m.reason
                    ));
                    unsafe { ffi::ts_set_gui_active(t.handle, m.active, reason.as_ptr()) };
                }
            } else {
                trace_pdf_input(format!(
                    "set-gui-active tab={} active={} reason={} result=no-tab",
                    m.tab_id, m.active, m.reason
                ));
            }
        }
        Msg::JavascriptDialogReply(m) => {
            if let Some(t) = find_by_tab_id(m.tab_id) {
                let prompt_text = CString::new(m.prompt_text.as_str())
                    .unwrap_or_else(|_| CString::new("").unwrap());
                let ok = unsafe {
                    ffi::ts_reply_javascript_dialog(
                        t.handle,
                        m.request_id,
                        m.accepted,
                        prompt_text.as_ptr(),
                    )
                };
                eprintln!(
                    "[termsurf-js-dialog] reply tab_id={} request_id={} accepted={} ok={}",
                    m.tab_id, m.request_id, m.accepted, ok
                );
                trace_pdf_input(format!(
                    "javascript-dialog-reply tab={} pane={} request_id={} accepted={} ok={}",
                    m.tab_id, t.pane_id, m.request_id, m.accepted, ok
                ));
            } else {
                eprintln!(
                    "[termsurf-js-dialog] reply-missing-tab tab_id={} request_id={}",
                    m.tab_id, m.request_id
                );
            }
        }
        Msg::HttpAuthReply(m) => {
            if let Some(t) = find_by_tab_id(m.tab_id) {
                let username =
                    CString::new(m.username.as_str()).unwrap_or_else(|_| CString::new("").unwrap());
                let password =
                    CString::new(m.password.as_str()).unwrap_or_else(|_| CString::new("").unwrap());
                let ok = unsafe {
                    ffi::ts_reply_http_auth(
                        t.handle,
                        m.request_id,
                        m.accepted,
                        username.as_ptr(),
                        password.as_ptr(),
                    )
                };
                eprintln!(
                    "[termsurf-http-auth] reply tab_id={} request_id={} accepted={} ok={}",
                    m.tab_id, m.request_id, m.accepted, ok
                );
                trace_pdf_input(format!(
                    "http-auth-reply tab={} pane={} request_id={} accepted={} username={} password_len={} ok={}",
                    m.tab_id,
                    t.pane_id,
                    m.request_id,
                    m.accepted,
                    m.username,
                    m.password.chars().count(),
                    ok
                ));
            } else {
                eprintln!(
                    "[termsurf-http-auth] reply-missing-tab tab_id={} request_id={}",
                    m.tab_id, m.request_id
                );
            }
        }
        Msg::SetColorScheme(m) => {
            if let Some(t) = find_by_tab_id(m.tab_id) {
                trace_pdf_input(format!(
                    "set-color-scheme tab={} pane={} dark={} ffi=ts_set_color_scheme",
                    m.tab_id, t.pane_id, m.dark
                ));
                unsafe { ffi::ts_set_color_scheme(t.handle, m.dark) };
            } else {
                trace_pdf_input(format!(
                    "set-color-scheme tab={} result=missing-tab dark={}",
                    m.tab_id, m.dark
                ));
            }
        }
        Msg::QueryTabsRequest(_) => {
            let mut browser_count: i64 = 0;
            let mut devtools_count: i64 = 0;
            let mut tab_infos = Vec::new();
            for t in tabs().iter() {
                if t.inspected_tab_id > 0 {
                    devtools_count += 1;
                } else {
                    browser_count += 1;
                }
                tab_infos.push(proto::termsurf::TabInfo {
                    id: t.tab_id,
                    inspected_tab_id: t.inspected_tab_id,
                    pane_id: t.pane_id.clone(),
                    url: t.last_url.clone(),
                });
            }
            let reply = TermSurfMessage {
                msg: Some(Msg::QueryTabsReply(proto::termsurf::QueryTabsReply {
                    chromium_tabs: tabs().len() as i64,
                    chromium_browser: browser_count,
                    chromium_devtools: devtools_count,
                    tabs: tab_infos,
                    gui_panes: 0,
                    error: String::new(),
                })),
            };
            crate::ipc::send(&reply);
        }
        _ => {}
    }
}

// --- Callbacks (called on UI thread) ---

pub unsafe extern "C" fn on_tab_ready(wc: TsWebContents, tab_id: i32, _user_data: *mut c_void) {
    // Try by handle first, then by null handle (sync callback).
    let t = find_by_handle(wc).or_else(|| {
        tabs().iter_mut().find(|t| t.handle.is_null()).map(|t| {
            t.handle = wc;
            t
        })
    });
    let Some(t) = t else { return };
    t.tab_id = tab_id as i64;
    t.can_go_back = false;
    t.can_go_forward = false;
    cache_navigation_commit(&mut t.crashed);
    trace_pdf_input(format!(
        "tab-ready tab={} pane={} inspected_tab_id={}",
        t.tab_id, t.pane_id, t.inspected_tab_id
    ));

    let msg = TermSurfMessage {
        msg: Some(Msg::TabReady(proto::termsurf::TabReady {
            pane_id: t.pane_id.clone(),
            tab_id: tab_id as i64,
        })),
    };
    crate::ipc::send(&msg);
    publish_navigation_state(t.tab_id, false, false, false);
}

pub unsafe extern "C" fn on_ca_context_id(
    wc: TsWebContents,
    ca_context_id: u32,
    width: i32,
    height: i32,
    _user_data: *mut c_void,
) {
    let Some(t) = find_by_handle(wc) else { return };
    trace_pdf_input(format!(
        "ca-context tab={} pane={} inspected_tab_id={} context_id={} pixel_width={} pixel_height={}",
        t.tab_id, t.pane_id, t.inspected_tab_id, ca_context_id, width, height
    ));
    let msg = TermSurfMessage {
        msg: Some(Msg::CaContext(proto::termsurf::CaContext {
            tab_id: t.tab_id,
            ca_context_id: ca_context_id as u64,
            pixel_width: width as u64,
            pixel_height: height as u64,
        })),
    };
    crate::ipc::send(&msg);
}

pub unsafe extern "C" fn on_url_changed(
    wc: TsWebContents,
    url: *const std::os::raw::c_char,
    _user_data: *mut c_void,
) {
    let Some(t) = find_by_handle(wc) else { return };
    // The wrapper emits UrlChanged from a committed navigation. This is the
    // authoritative recovery boundary; KVO state alone may still arrive after
    // a crash callback and must not clear the crash latch.
    cache_navigation_commit(&mut t.crashed);
    let url_str = unsafe { std::ffi::CStr::from_ptr(url) }
        .to_string_lossy()
        .into_owned();
    t.last_url = url_str.clone();
    trace_pdf_input(format!(
        "url-changed tab={} pane={} url={}",
        t.tab_id, t.pane_id, url_str
    ));
    let msg = TermSurfMessage {
        msg: Some(Msg::UrlChanged(proto::termsurf::UrlChanged {
            tab_id: t.tab_id,
            url: url_str,
        })),
    };
    crate::ipc::send(&msg);
}

pub unsafe extern "C" fn on_loading_state(
    wc: TsWebContents,
    url: *const std::os::raw::c_char,
    loading: i32,
    _user_data: *mut c_void,
) {
    let url_str = unsafe { std::ffi::CStr::from_ptr(url) }
        .to_string_lossy()
        .into_owned();
    let state_str = if loading != 0 { "loading" } else { "done" }.to_string();
    let progress = if loading != 0 { 1 } else { 0 };
    let Some(t) = find_by_handle(wc) else {
        let pending_null_handle = tabs().iter().any(|t| t.handle.is_null());
        trace_pdf_input(format!(
            "loading-state-callback-missing-tab handle={:p} pending_null_handle={} url={} state={} progress={}",
            wc, pending_null_handle, url_str, state_str, progress
        ));
        return;
    };
    trace_pdf_input(format!(
        "loading-state-callback tab={} pane={} url={} state={} progress={}",
        t.tab_id, t.pane_id, url_str, state_str, progress
    ));
    let mut navigation_request_id = 0;
    if state_str == "loading" && t.pending_refresh_request_id != 0 && !t.refresh_request_armed {
        t.refresh_request_armed = true;
    }
    if t.refresh_request_armed {
        navigation_request_id = t.pending_refresh_request_id;
    }
    let terminal = state_str == "done" || state_str == "error";
    let msg = TermSurfMessage {
        msg: Some(Msg::LoadingState(proto::termsurf::LoadingState {
            tab_id: t.tab_id,
            state: state_str,
            progress: progress as u64,
            navigation_request_id,
        })),
    };
    if navigation_request_id != 0 && terminal {
        clear_refresh_request(
            &mut t.pending_refresh_request_id,
            &mut t.refresh_request_armed,
        );
    }
    remember_loading_state(&msg);
    crate::ipc::send(&msg);
}

pub unsafe extern "C" fn on_navigation_state(
    wc: TsWebContents,
    can_go_back: bool,
    can_go_forward: bool,
    can_refresh: bool,
    _user_data: *mut c_void,
) {
    let Some(t) = find_by_handle(wc) else { return };
    cache_navigation_state(
        &mut t.can_go_back,
        &mut t.can_go_forward,
        can_go_back,
        can_go_forward,
    );
    t.can_refresh = can_refresh;
    trace_pdf_input(format!(
        "navigation-state tab={} pane={} can_go_back={} can_go_forward={} can_refresh={}",
        t.tab_id, t.pane_id, can_go_back, can_go_forward, can_refresh
    ));
    publish_navigation_state(t.tab_id, can_go_back, can_go_forward, can_refresh);
}

pub unsafe extern "C" fn on_renderer_crashed(
    wc: TsWebContents,
    termination_status: *const std::os::raw::c_char,
    termination_status_code: i32,
    url: *const std::os::raw::c_char,
    can_reload: bool,
    _user_data: *mut c_void,
) {
    let Some(t) = find_by_handle(wc) else { return };
    cache_navigation_crash(&mut t.can_go_back, &mut t.can_go_forward, &mut t.crashed);
    t.can_refresh = can_reload;
    clear_refresh_request(
        &mut t.pending_refresh_request_id,
        &mut t.refresh_request_armed,
    );
    publish_navigation_state(t.tab_id, false, false, can_reload);
    let termination_status = unsafe { std::ffi::CStr::from_ptr(termination_status) }
        .to_string_lossy()
        .into_owned();
    let url = unsafe { std::ffi::CStr::from_ptr(url) }
        .to_string_lossy()
        .into_owned();
    eprintln!(
        "[termsurf-renderer-crash] tab_id={} status={} code={} url={} can_reload={}",
        t.tab_id, termination_status, termination_status_code, url, can_reload
    );
    trace_pdf_input(format!(
        "renderer-crashed tab={} pane={} status={} code={} url={} can_reload={}",
        t.tab_id, t.pane_id, termination_status, termination_status_code, url, can_reload
    ));
    let msg = TermSurfMessage {
        msg: Some(Msg::RendererCrashed(proto::termsurf::RendererCrashed {
            tab_id: t.tab_id,
            termination_status,
            termination_status_code,
            url,
            can_reload,
        })),
    };
    crate::ipc::send(&msg);
}

pub unsafe extern "C" fn on_render_probe(
    wc: TsWebContents,
    method: *const std::os::raw::c_char,
    status: *const std::os::raw::c_char,
    width: i32,
    height: i32,
    magenta: i32,
    cyan: i32,
    yellow: i32,
    webkit_green: i32,
    error: *const std::os::raw::c_char,
    _user_data: *mut c_void,
) {
    let Some(t) = find_by_handle(wc) else { return };
    let method = unsafe { c_string(method) };
    let status = unsafe { c_string(status) };
    let error = unsafe { c_string(error) };
    trace_render_probe(format!(
        "render-proof tab={} pane={} url={} method={} status={} width={} height={} magenta={} cyan={} yellow={} webkit_green={} error={}",
        t.tab_id,
        t.pane_id,
        t.last_url,
        method,
        status,
        width,
        height,
        magenta,
        cyan,
        yellow,
        webkit_green,
        error
    ));
}

pub unsafe extern "C" fn on_title_changed(
    wc: TsWebContents,
    title: *const std::os::raw::c_char,
    _user_data: *mut c_void,
) {
    let Some(t) = find_by_handle(wc) else { return };
    let mut title_str = if title.is_null() {
        String::new()
    } else {
        unsafe { std::ffi::CStr::from_ptr(title) }
            .to_string_lossy()
            .into_owned()
    };
    let mut source = "webkit";
    if title_str.trim().is_empty() {
        if let Some(fallback) = pdf_title_from_url(&t.last_url) {
            title_str = fallback;
            source = "pdf-url-fallback";
        }
    }
    trace_pdf_input(format!(
        "title-changed tab={} pane={} title={} source={}",
        t.tab_id, t.pane_id, title_str, source
    ));
    let msg = TermSurfMessage {
        msg: Some(Msg::TitleChanged(proto::termsurf::TitleChanged {
            tab_id: t.tab_id,
            title: title_str,
        })),
    };
    crate::ipc::send(&msg);
}

pub unsafe extern "C" fn on_cursor_changed(
    wc: TsWebContents,
    cursor_type: i32,
    _user_data: *mut c_void,
) {
    let Some(t) = find_by_handle(wc) else { return };
    let msg = TermSurfMessage {
        msg: Some(Msg::CursorChanged(proto::termsurf::CursorChanged {
            tab_id: t.tab_id,
            cursor_type: cursor_type as i64,
        })),
    };
    crate::ipc::send(&msg);
}

pub unsafe extern "C" fn on_target_url_changed(
    wc: TsWebContents,
    url: *const std::os::raw::c_char,
    _user_data: *mut c_void,
) {
    let Some(t) = find_by_handle(wc) else { return };
    let url_str = unsafe { std::ffi::CStr::from_ptr(url) }
        .to_string_lossy()
        .into_owned();
    let msg = TermSurfMessage {
        msg: Some(Msg::TargetUrlChanged(proto::termsurf::TargetUrlChanged {
            tab_id: t.tab_id,
            url: url_str,
        })),
    };
    crate::ipc::send(&msg);
}

pub unsafe extern "C" fn on_javascript_dialog_request(
    wc: TsWebContents,
    request_id: u64,
    dialog_type: *const std::os::raw::c_char,
    origin_url: *const std::os::raw::c_char,
    message: *const std::os::raw::c_char,
    default_prompt_text: *const std::os::raw::c_char,
    _user_data: *mut c_void,
) {
    let Some(t) = find_by_handle(wc) else {
        eprintln!(
            "[termsurf-js-dialog] request-missing-tab request_id={}",
            request_id
        );
        return;
    };
    let dialog_type = unsafe { std::ffi::CStr::from_ptr(dialog_type) }
        .to_string_lossy()
        .into_owned();
    let origin_url = unsafe { std::ffi::CStr::from_ptr(origin_url) }
        .to_string_lossy()
        .into_owned();
    let message = unsafe { std::ffi::CStr::from_ptr(message) }
        .to_string_lossy()
        .into_owned();
    let default_prompt_text = unsafe { std::ffi::CStr::from_ptr(default_prompt_text) }
        .to_string_lossy()
        .into_owned();
    eprintln!(
        "[termsurf-js-dialog] request tab_id={} request_id={} type={} origin={}",
        t.tab_id, request_id, dialog_type, origin_url
    );
    trace_pdf_input(format!(
        "javascript-dialog-request tab={} pane={} request_id={} type={} origin={} message={}",
        t.tab_id, t.pane_id, request_id, dialog_type, origin_url, message
    ));
    let msg = TermSurfMessage {
        msg: Some(Msg::JavascriptDialogRequest(
            proto::termsurf::JavaScriptDialogRequest {
                tab_id: t.tab_id,
                request_id,
                dialog_type,
                origin_url,
                message,
                default_prompt_text,
            },
        )),
    };
    crate::ipc::send(&msg);
}

pub unsafe extern "C" fn on_console_message(
    wc: TsWebContents,
    level: *const std::os::raw::c_char,
    message: *const std::os::raw::c_char,
    line_no: i32,
    source_id: *const std::os::raw::c_char,
    _user_data: *mut c_void,
) {
    let Some(t) = find_by_handle(wc) else {
        eprintln!("[termsurf-console] message-missing-tab line_no={}", line_no);
        return;
    };
    let level = unsafe { std::ffi::CStr::from_ptr(level) }
        .to_string_lossy()
        .into_owned();
    let message = unsafe { std::ffi::CStr::from_ptr(message) }
        .to_string_lossy()
        .into_owned();
    let source_id = unsafe { std::ffi::CStr::from_ptr(source_id) }
        .to_string_lossy()
        .into_owned();
    eprintln!(
        "[termsurf-console] message tab_id={} level={} line_no={} source={}",
        t.tab_id, level, line_no, source_id
    );
    let msg = TermSurfMessage {
        msg: Some(Msg::ConsoleMessage(proto::termsurf::ConsoleMessage {
            tab_id: t.tab_id,
            level,
            message,
            line_no,
            source_id,
        })),
    };
    crate::ipc::send(&msg);
}

pub unsafe extern "C" fn on_http_auth_request(
    wc: TsWebContents,
    request_id: u64,
    url: *const std::os::raw::c_char,
    auth_scheme: *const std::os::raw::c_char,
    challenger: *const std::os::raw::c_char,
    realm: *const std::os::raw::c_char,
    is_proxy: bool,
    first_auth_attempt: bool,
    is_primary_main_frame_navigation: bool,
    is_navigation: bool,
    _user_data: *mut c_void,
) {
    let Some(t) = find_by_handle(wc) else {
        eprintln!(
            "[termsurf-http-auth] request-missing-tab request_id={}",
            request_id
        );
        defer_http_auth_cancel(wc, request_id);
        return;
    };
    let url = unsafe { std::ffi::CStr::from_ptr(url) }
        .to_string_lossy()
        .into_owned();
    let auth_scheme = unsafe { std::ffi::CStr::from_ptr(auth_scheme) }
        .to_string_lossy()
        .into_owned();
    let challenger = unsafe { std::ffi::CStr::from_ptr(challenger) }
        .to_string_lossy()
        .into_owned();
    let realm = unsafe { std::ffi::CStr::from_ptr(realm) }
        .to_string_lossy()
        .into_owned();
    eprintln!(
        "[termsurf-http-auth] request tab_id={} request_id={} scheme={} challenger={} realm={} proxy={} first_attempt={}",
        t.tab_id, request_id, auth_scheme, challenger, realm, is_proxy, first_auth_attempt
    );
    trace_pdf_input(format!(
        "http-auth-request tab={} pane={} request_id={} url={} scheme={} challenger={} realm={} proxy={} first_attempt={}",
        t.tab_id,
        t.pane_id,
        request_id,
        url,
        auth_scheme,
        challenger,
        realm,
        is_proxy,
        first_auth_attempt
    ));
    let msg = TermSurfMessage {
        msg: Some(Msg::HttpAuthRequest(proto::termsurf::HttpAuthRequest {
            tab_id: t.tab_id,
            request_id,
            url,
            auth_scheme,
            challenger,
            realm,
            is_proxy,
            first_auth_attempt,
            is_primary_main_frame_navigation,
            is_navigation,
        })),
    };
    if crate::ipc::send(&msg) == 0 {
        defer_http_auth_cancel(wc, request_id);
        eprintln!(
            "[termsurf-http-auth] request-no-client-cancel request_id={}",
            request_id
        );
    }
}

#[cfg(test)]
mod navigation_contract_tests {
    use super::{
        cache_navigation_commit, cache_navigation_crash, cache_navigation_state,
        clear_refresh_request, navigation_action_contract, navigation_action_enabled,
    };
    use crate::proto::{self, Msg, TermSurfMessage};
    use prost::Message;

    fn action(tab_id: i64, pane_id: &str, value: &str) -> proto::termsurf::NavigationAction {
        proto::termsurf::NavigationAction {
            tab_id,
            pane_id: pane_id.into(),
            action: value.into(),
            request_id: 0,
        }
    }

    #[test]
    fn navigation_action_requires_known_engine_direction_and_enabled_cache() {
        assert!(navigation_action_contract(&action(7, "", "back")));
        assert!(navigation_action_contract(&action(7, "", "forward")));
        let mut refresh = action(7, "", "refresh");
        refresh.request_id = 9;
        assert!(navigation_action_contract(&refresh));
        for invalid in [
            action(0, "", "back"),
            action(7, "pane-a", "back"),
            action(7, "", ""),
            action(7, "", "refresh"),
            action(7, "", "Back"),
        ] {
            assert!(!navigation_action_contract(&invalid));
        }
        assert!(navigation_action_enabled(
            "back",
            Some((true, false, false, false))
        ));
        assert!(navigation_action_enabled(
            "forward",
            Some((false, true, false, false))
        ));
        assert!(!navigation_action_enabled(
            "back",
            Some((false, true, false, false))
        ));
        assert!(!navigation_action_enabled(
            "forward",
            Some((true, false, false, false))
        ));
        assert!(!navigation_action_enabled(
            "back",
            Some((true, true, true, true))
        ));
        assert!(navigation_action_enabled(
            "refresh",
            Some((false, false, true, true))
        ));
        assert!(!navigation_action_enabled("forward", None));
    }

    #[test]
    fn protobuf_round_trip_preserves_back_and_false_state_identity() {
        let message = TermSurfMessage {
            msg: Some(Msg::NavigationAction(action(19, "", "back"))),
        };
        let decoded = TermSurfMessage::decode(message.encode_to_vec().as_slice()).unwrap();
        let Some(Msg::NavigationAction(decoded_action)) = decoded.msg else {
            panic!("expected NavigationAction");
        };
        assert!(navigation_action_contract(&decoded_action));

        let state = TermSurfMessage {
            msg: Some(Msg::NavigationState(proto::termsurf::NavigationState {
                tab_id: 19,
                can_go_back: false,
                can_go_forward: true,
                can_refresh: true,
            })),
        };
        let decoded = TermSurfMessage::decode(state.encode_to_vec().as_slice()).unwrap();
        let Some(Msg::NavigationState(decoded_state)) = decoded.msg else {
            panic!("expected NavigationState");
        };
        assert_eq!(decoded_state.tab_id, 19);
        assert!(!decoded_state.can_go_back);
        assert!(decoded_state.can_go_forward);
        assert!(decoded_state.can_refresh);

        let loading = TermSurfMessage {
            msg: Some(Msg::LoadingState(proto::termsurf::LoadingState {
                tab_id: 19,
                state: "done".into(),
                progress: 100,
                navigation_request_id: 9,
            })),
        };
        let decoded = TermSurfMessage::decode(loading.encode_to_vec().as_slice()).unwrap();
        let Some(Msg::LoadingState(decoded_loading)) = decoded.msg else {
            panic!("expected LoadingState");
        };
        assert_eq!(decoded_loading.tab_id, 19);
        assert_eq!(decoded_loading.navigation_request_id, 9);
    }

    #[test]
    fn crash_latch_survives_late_false_state_until_a_commit() {
        let mut can_go_back = true;
        let mut can_go_forward = true;
        let mut crashed = false;
        cache_navigation_crash(&mut can_go_back, &mut can_go_forward, &mut crashed);
        assert!(!can_go_back);
        assert!(!can_go_forward);
        assert!(crashed);

        cache_navigation_state(&mut can_go_back, &mut can_go_forward, false, false);
        assert!(!can_go_back);
        assert!(!can_go_forward);
        assert!(crashed);
        assert!(!navigation_action_enabled(
            "forward",
            Some((can_go_back, can_go_forward, true, crashed))
        ));

        cache_navigation_commit(&mut crashed);
        cache_navigation_state(&mut can_go_back, &mut can_go_forward, false, true);
        assert!(navigation_action_enabled(
            "forward",
            Some((can_go_back, can_go_forward, true, crashed))
        ));
    }

    #[test]
    fn crash_clears_unarmed_and_armed_refresh_correlation() {
        for armed in [false, true] {
            let mut pending_request_id = 91;
            let mut armed = armed;
            clear_refresh_request(&mut pending_request_id, &mut armed);
            assert_eq!(pending_request_id, 0);
            assert!(!armed);
        }
    }
}
