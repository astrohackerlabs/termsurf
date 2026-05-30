use std::ffi::{c_void, CString};
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::ffi::{self, TsWebContents};
use crate::proto::{self, Msg, TermSurfMessage};

// --- Tab registry ---

struct TabEntry {
    handle: TsWebContents,
    tab_id: i64,
    pane_id: String,
    inspected_tab_id: i64,
    last_url: String,
}

static PDF_INPUT_TRACE_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

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
        let _ = writeln!(file, "roamium {}", line.as_ref());
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
            tabs().push(TabEntry {
                handle: std::ptr::null_mut(),
                tab_id: 0,
                pane_id: m.pane_id.clone(),
                inspected_tab_id: 0,
                last_url: String::new(),
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
        }
        Msg::CreateDevtoolsTab(m) => {
            tabs().push(TabEntry {
                handle: std::ptr::null_mut(),
                tab_id: 0,
                pane_id: m.pane_id.clone(),
                inspected_tab_id: m.inspected_tab_id,
                last_url: String::new(),
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
        }
        Msg::Resize(m) => {
            if let Some(t) = find_by_tab_id(m.tab_id) {
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
            }
        }
        Msg::CloseTab(m) => {
            let tab_id = m.tab_id;
            if let Some(t) = find_by_tab_id(tab_id) {
                unsafe { ffi::ts_destroy_web_contents(t.handle) };
            }
            tabs().retain(|t| t.tab_id != tab_id);
        }
        Msg::Navigate(m) => {
            if let Some(t) = find_by_tab_id(m.tab_id) {
                let url = CString::new(m.url.as_str()).unwrap();
                unsafe { ffi::ts_load_url(t.handle, url.as_ptr()) };
            }
        }
        Msg::MouseEvent(m) => {
            if let Some(t) = find_by_tab_id(m.tab_id) {
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
            }
        }
        Msg::MouseMove(m) => {
            if let Some(t) = find_by_tab_id(m.tab_id) {
                unsafe {
                    ffi::ts_forward_mouse_move(
                        t.handle,
                        m.x as i32,
                        m.y as i32,
                        m.modifiers as i32,
                    );
                }
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
            }
        }
        Msg::FocusChanged(m) => {
            if let Some(t) = find_by_tab_id(m.tab_id) {
                unsafe { ffi::ts_set_focus(t.handle, m.focused) };
            }
        }
        Msg::SetGuiActive(m) => {
            let reason =
                CString::new(m.reason.as_str()).unwrap_or_else(|_| CString::new("").unwrap());
            if m.tab_id == 0 {
                for t in tabs().iter() {
                    if !t.handle.is_null() {
                        unsafe { ffi::ts_set_gui_active(t.handle, m.active, reason.as_ptr()) };
                    }
                }
            } else if let Some(t) = find_by_tab_id(m.tab_id) {
                if !t.handle.is_null() {
                    unsafe { ffi::ts_set_gui_active(t.handle, m.active, reason.as_ptr()) };
                }
            }
        }
        Msg::SetColorScheme(m) => {
            if let Some(t) = find_by_tab_id(m.tab_id) {
                unsafe { ffi::ts_set_color_scheme(t.handle, m.dark) };
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

    let msg = TermSurfMessage {
        msg: Some(Msg::TabReady(proto::termsurf::TabReady {
            pane_id: t.pane_id.clone(),
            tab_id: tab_id as i64,
        })),
    };
    crate::ipc::send(&msg);
}

pub unsafe extern "C" fn on_ca_context_id(
    wc: TsWebContents,
    ca_context_id: u32,
    width: i32,
    height: i32,
    _user_data: *mut c_void,
) {
    let Some(t) = find_by_handle(wc) else { return };
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
    let url_str = unsafe { std::ffi::CStr::from_ptr(url) }
        .to_string_lossy()
        .into_owned();
    t.last_url = url_str.clone();
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
    state: *const std::os::raw::c_char,
    progress: i32,
    _user_data: *mut c_void,
) {
    let Some(t) = find_by_handle(wc) else { return };
    let state_str = unsafe { std::ffi::CStr::from_ptr(state) }
        .to_string_lossy()
        .into_owned();
    let msg = TermSurfMessage {
        msg: Some(Msg::LoadingState(proto::termsurf::LoadingState {
            tab_id: t.tab_id,
            state: state_str,
            progress: progress as u64,
        })),
    };
    crate::ipc::send(&msg);
}

pub unsafe extern "C" fn on_title_changed(
    wc: TsWebContents,
    title: *const std::os::raw::c_char,
    _user_data: *mut c_void,
) {
    let Some(t) = find_by_handle(wc) else { return };
    let title_str = unsafe { std::ffi::CStr::from_ptr(title) }
        .to_string_lossy()
        .into_owned();
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
