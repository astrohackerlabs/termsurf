//! Minimal XPC client for sending overlay coordinates to the TermSurf compositor.
//!
//! Issue 506: Two-step connection via the xpc-gateway daemon.
//! 1. Connect to com.termsurf.xpc-gateway (the gateway daemon)
//! 2. Send { action: "connect" } and receive the app's anonymous endpoint
//! 3. Connect directly to the app via the endpoint
//! 4. Send set_overlay messages on the direct connection

use std::ffi::{c_char, c_void, CString};

// --- FFI bindings ---

type XpcConnectionT = *mut c_void;
type XpcObjectT = *mut c_void;

extern "C" {
    fn xpc_connection_create_mach_service(
        name: *const c_char,
        targetq: *mut c_void,
        flags: u64,
    ) -> XpcConnectionT;
    fn xpc_connection_set_event_handler(conn: XpcConnectionT, handler: *mut c_void);
    fn xpc_connection_resume(conn: XpcConnectionT);
    fn xpc_connection_send_message(conn: XpcConnectionT, message: XpcObjectT);
    fn xpc_connection_send_message_with_reply_sync(
        conn: XpcConnectionT,
        message: XpcObjectT,
    ) -> XpcObjectT;
    fn xpc_connection_cancel(conn: XpcConnectionT);
    fn xpc_connection_create_from_endpoint(endpoint: XpcObjectT) -> XpcConnectionT;
    fn xpc_release(object: XpcObjectT);
    fn xpc_dictionary_create(
        keys: *const *const c_char,
        values: *const XpcObjectT,
        count: usize,
    ) -> XpcObjectT;
    fn xpc_dictionary_set_string(dict: XpcObjectT, key: *const c_char, value: *const c_char);
    fn xpc_dictionary_set_uint64(dict: XpcObjectT, key: *const c_char, value: u64);
    fn xpc_dictionary_set_int64(dict: XpcObjectT, key: *const c_char, value: i64);
    fn xpc_dictionary_get_value(dict: XpcObjectT, key: *const c_char) -> XpcObjectT;
    fn xpc_dictionary_get_string(dict: XpcObjectT, key: *const c_char) -> *const c_char;
    fn xpc_dictionary_get_bool(dict: XpcObjectT, key: *const c_char) -> bool;
    fn xpc_dictionary_set_bool(dict: XpcObjectT, key: *const c_char, value: bool);
    fn xpc_dictionary_get_uint64(dict: XpcObjectT, key: *const c_char) -> u64;
    fn xpc_dictionary_get_int64(dict: XpcObjectT, key: *const c_char) -> i64;
    fn xpc_get_type(object: XpcObjectT) -> *const c_void;
}

// XPC type constants — resolved at link time.
extern "C" {
    #[link_name = "_xpc_type_dictionary"]
    static XPC_TYPE_DICTIONARY: c_void;
}

// --- Public API ---

/// Messages received from the compositor.
pub enum CompositorMessage {
    ModeChanged { browsing: bool },
    UrlChanged { url: String },
    LoadingState { state: String, _progress: u8 },
    TitleChanged { title: String },
}

/// A direct connection to the TermSurf app via its anonymous XPC listener.
pub struct CompositorConnection {
    raw: XpcConnectionT,
}

unsafe impl Send for CompositorConnection {}

impl CompositorConnection {
    /// Connect to the TermSurf app via the xpc-gateway.
    ///
    /// 1. Connect to `com.termsurf.xpc-gateway` (the gateway daemon)
    /// 2. Send `{ action: "connect" }` and receive the app's endpoint
    /// 3. Connect directly to the app via the endpoint
    pub fn connect(tx: std::sync::mpsc::Sender<super::LoopEvent>) -> Option<Self> {
        // Step 1: Connect to the gateway.
        // Debug builds set TERMSURF_XPC_SERVICE to the debug gateway name (Issue 653).
        let service_name = std::env::var("TERMSURF_XPC_SERVICE")
            .unwrap_or_else(|_| "com.termsurf.xpc-gateway".to_string());
        let gateway_name = CString::new(service_name).unwrap();
        let gateway = unsafe {
            xpc_connection_create_mach_service(gateway_name.as_ptr(), std::ptr::null_mut(), 0)
        };
        if gateway.is_null() {
            return None;
        }

        // Set a minimal event handler (required before resume).
        let block = block2::RcBlock::new(|_event: XpcObjectT| {});
        unsafe {
            xpc_connection_set_event_handler(gateway, &*block as *const _ as *mut c_void);
        }
        unsafe { xpc_connection_resume(gateway) };

        // Step 2: Send "connect" and get the app's endpoint.
        let msg = unsafe { xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0) };
        if msg.is_null() {
            unsafe { xpc_release(gateway) };
            return None;
        }

        let action_key = CString::new("action").unwrap();
        let action_val = CString::new("connect").unwrap();
        unsafe {
            xpc_dictionary_set_string(msg, action_key.as_ptr(), action_val.as_ptr());
        }

        let reply = unsafe { xpc_connection_send_message_with_reply_sync(gateway, msg) };
        unsafe { xpc_release(msg) };

        if reply.is_null() {
            eprintln!("[web] Gateway returned null reply");
            unsafe {
                xpc_connection_cancel(gateway);
                xpc_release(gateway)
            };
            return None;
        }

        // Check if the reply is a dictionary (not an error).
        let reply_type = unsafe { xpc_get_type(reply) };
        let dict_type = unsafe { &XPC_TYPE_DICTIONARY as *const c_void };
        if reply_type != dict_type {
            eprintln!("[web] Gateway reply is not a dictionary (connection error?)");
            unsafe { xpc_release(reply) };
            unsafe {
                xpc_connection_cancel(gateway);
                xpc_release(gateway)
            };
            return None;
        }

        // Check for error field.
        let error_key = CString::new("error").unwrap();
        let error_ptr = unsafe { xpc_dictionary_get_string(reply, error_key.as_ptr()) };
        if !error_ptr.is_null() {
            let error_str = unsafe { std::ffi::CStr::from_ptr(error_ptr) };
            eprintln!("[web] Gateway error: {:?}", error_str);
            unsafe { xpc_release(reply) };
            unsafe {
                xpc_connection_cancel(gateway);
                xpc_release(gateway)
            };
            return None;
        }

        // Extract the endpoint.
        let endpoint_key = CString::new("endpoint").unwrap();
        let endpoint = unsafe { xpc_dictionary_get_value(reply, endpoint_key.as_ptr()) };
        if endpoint.is_null() {
            eprintln!("[web] Gateway reply missing endpoint");
            unsafe { xpc_release(reply) };
            unsafe {
                xpc_connection_cancel(gateway);
                xpc_release(gateway)
            };
            return None;
        }

        // Step 3: Connect directly to the app via the endpoint.
        let app_conn = unsafe { xpc_connection_create_from_endpoint(endpoint) };
        unsafe { xpc_release(reply) };

        if app_conn.is_null() {
            eprintln!("[web] Failed to create connection from endpoint");
            unsafe {
                xpc_connection_cancel(gateway);
                xpc_release(gateway)
            };
            return None;
        }

        // Set event handler that parses incoming messages (Issue 513).
        // Messages are sent directly to the unified LoopEvent channel (Issue 666).
        let handler_block = block2::RcBlock::new(move |event: XpcObjectT| {
            if event.is_null() {
                return;
            }
            let event_type = unsafe { xpc_get_type(event) };
            let dict_type = unsafe { &XPC_TYPE_DICTIONARY as *const c_void };
            if event_type != dict_type {
                return;
            }

            let action_key = CString::new("action").unwrap();
            let action_ptr = unsafe { xpc_dictionary_get_string(event, action_key.as_ptr()) };
            if action_ptr.is_null() {
                return;
            }
            let action = unsafe { std::ffi::CStr::from_ptr(action_ptr) }
                .to_str()
                .unwrap_or("");

            if action == "mode_changed" {
                let browsing_key = CString::new("browsing").unwrap();
                let browsing = unsafe { xpc_dictionary_get_bool(event, browsing_key.as_ptr()) };
                let _ = tx.send(super::LoopEvent::Xpc(CompositorMessage::ModeChanged {
                    browsing,
                }));
            } else if action == "url_changed" {
                let url_key = CString::new("url").unwrap();
                let url_ptr = unsafe { xpc_dictionary_get_string(event, url_key.as_ptr()) };
                if !url_ptr.is_null() {
                    let url = unsafe { std::ffi::CStr::from_ptr(url_ptr) }
                        .to_str()
                        .unwrap_or("")
                        .to_string();
                    let _ = tx.send(super::LoopEvent::Xpc(CompositorMessage::UrlChanged { url }));
                }
            } else if action == "loading_state" {
                let state_key = CString::new("state").unwrap();
                let state_ptr = unsafe { xpc_dictionary_get_string(event, state_key.as_ptr()) };
                if !state_ptr.is_null() {
                    let state = unsafe { std::ffi::CStr::from_ptr(state_ptr) }
                        .to_str()
                        .unwrap_or("done")
                        .to_string();
                    let progress_key = CString::new("progress").unwrap();
                    let progress =
                        unsafe { xpc_dictionary_get_uint64(event, progress_key.as_ptr()) } as u8;
                    let _ = tx.send(super::LoopEvent::Xpc(CompositorMessage::LoadingState {
                        state,
                        _progress: progress,
                    }));
                }
            } else if action == "title_changed" {
                let title_key = CString::new("title").unwrap();
                let title_ptr = unsafe { xpc_dictionary_get_string(event, title_key.as_ptr()) };
                if !title_ptr.is_null() {
                    let title = unsafe { std::ffi::CStr::from_ptr(title_ptr) }
                        .to_str()
                        .unwrap_or("")
                        .to_string();
                    let _ = tx.send(super::LoopEvent::Xpc(CompositorMessage::TitleChanged {
                        title,
                    }));
                }
            }
        });
        unsafe {
            xpc_connection_set_event_handler(app_conn, &*handler_block as *const _ as *mut c_void);
        }
        unsafe { xpc_connection_resume(app_conn) };

        // Done with the gateway connection.
        unsafe {
            xpc_connection_cancel(gateway);
            xpc_release(gateway)
        };

        Some(Self { raw: app_conn })
    }

    /// Send a `set_overlay` message to the app (direct connection).
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
    ) {
        let dict = unsafe { xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0) };
        if dict.is_null() {
            return;
        }

        let action = CString::new("set_overlay").unwrap();
        let action_key = CString::new("action").unwrap();
        let pane_id_c = CString::new(pane_id).unwrap();
        let pane_id_key = CString::new("pane_id").unwrap();
        let col_key = CString::new("col").unwrap();
        let row_key = CString::new("row").unwrap();
        let width_key = CString::new("width").unwrap();
        let height_key = CString::new("height").unwrap();

        unsafe {
            xpc_dictionary_set_string(dict, action_key.as_ptr(), action.as_ptr());
            xpc_dictionary_set_string(dict, pane_id_key.as_ptr(), pane_id_c.as_ptr());
            xpc_dictionary_set_uint64(dict, col_key.as_ptr(), col as u64);
            xpc_dictionary_set_uint64(dict, row_key.as_ptr(), row as u64);
            xpc_dictionary_set_uint64(dict, width_key.as_ptr(), width as u64);
            xpc_dictionary_set_uint64(dict, height_key.as_ptr(), height as u64);

            let url_key = CString::new("url").unwrap();
            let url_c = CString::new(url).unwrap();
            xpc_dictionary_set_string(dict, url_key.as_ptr(), url_c.as_ptr());

            let profile_key = CString::new("profile").unwrap();
            let profile_c = CString::new(profile).unwrap();
            xpc_dictionary_set_string(dict, profile_key.as_ptr(), profile_c.as_ptr());

            let browsing_key = CString::new("browsing").unwrap();
            xpc_dictionary_set_bool(dict, browsing_key.as_ptr(), browsing);

            xpc_connection_send_message(self.raw, dict);
            xpc_release(dict);
        }
    }

    /// Tell the compositor to create a DevTools overlay (Issue 684).
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
    ) {
        let dict = unsafe { xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0) };
        if dict.is_null() {
            return;
        }

        let action = CString::new("set_devtools_overlay").unwrap();
        let action_key = CString::new("action").unwrap();
        let pane_id_c = CString::new(pane_id).unwrap();
        let pane_id_key = CString::new("pane_id").unwrap();
        let col_key = CString::new("col").unwrap();
        let row_key = CString::new("row").unwrap();
        let width_key = CString::new("width").unwrap();
        let height_key = CString::new("height").unwrap();

        unsafe {
            xpc_dictionary_set_string(dict, action_key.as_ptr(), action.as_ptr());
            xpc_dictionary_set_string(dict, pane_id_key.as_ptr(), pane_id_c.as_ptr());
            xpc_dictionary_set_uint64(dict, col_key.as_ptr(), col as u64);
            xpc_dictionary_set_uint64(dict, row_key.as_ptr(), row as u64);
            xpc_dictionary_set_uint64(dict, width_key.as_ptr(), width as u64);
            xpc_dictionary_set_uint64(dict, height_key.as_ptr(), height as u64);

            let tid_key = CString::new("inspected_tab_id").unwrap();
            xpc_dictionary_set_int64(dict, tid_key.as_ptr(), inspected_tab_id);

            let profile_key = CString::new("profile").unwrap();
            let profile_c = CString::new(profile).unwrap();
            xpc_dictionary_set_string(dict, profile_key.as_ptr(), profile_c.as_ptr());

            let browsing_key = CString::new("browsing").unwrap();
            xpc_dictionary_set_bool(dict, browsing_key.as_ptr(), browsing);

            xpc_connection_send_message(self.raw, dict);
            xpc_release(dict);
        }
    }

    /// Send a synchronous `hello` message to get live config (Issue 675).
    pub fn send_hello(&self, pane_id: &str) -> Option<String> {
        let dict = unsafe { xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0) };
        if dict.is_null() {
            return None;
        }

        unsafe {
            let key = CString::new("action").unwrap();
            let val = CString::new("hello").unwrap();
            xpc_dictionary_set_string(dict, key.as_ptr(), val.as_ptr());

            let pk = CString::new("pane_id").unwrap();
            let pv = CString::new(pane_id).unwrap();
            xpc_dictionary_set_string(dict, pk.as_ptr(), pv.as_ptr());
        }

        let reply = unsafe { xpc_connection_send_message_with_reply_sync(self.raw, dict) };
        unsafe { xpc_release(dict) };

        if reply.is_null() {
            return None;
        }

        let homepage_key = CString::new("homepage").unwrap();
        let hp = unsafe { xpc_dictionary_get_string(reply, homepage_key.as_ptr()) };
        let result = if !hp.is_null() {
            Some(
                unsafe { std::ffi::CStr::from_ptr(hp) }
                    .to_str()
                    .unwrap_or("")
                    .to_string(),
            )
        } else {
            None
        };
        unsafe { xpc_release(reply) };
        result
    }

    /// Query the GUI for the last active browser pane/tab (Issue 684 Exp 4).
    pub fn send_query_last(&self, pane_id: &str, profile: &str) -> Option<(String, String, i64)> {
        let dict = unsafe { xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0) };
        if dict.is_null() {
            return None;
        }

        unsafe {
            let key = CString::new("action").unwrap();
            let val = CString::new("query_last").unwrap();
            xpc_dictionary_set_string(dict, key.as_ptr(), val.as_ptr());

            let pk = CString::new("pane_id").unwrap();
            let pv = CString::new(pane_id).unwrap();
            xpc_dictionary_set_string(dict, pk.as_ptr(), pv.as_ptr());

            let prof_key = CString::new("profile").unwrap();
            let prof_val = CString::new(profile).unwrap();
            xpc_dictionary_set_string(dict, prof_key.as_ptr(), prof_val.as_ptr());
        }

        let reply = unsafe { xpc_connection_send_message_with_reply_sync(self.raw, dict) };
        unsafe { xpc_release(dict) };

        if reply.is_null() {
            return None;
        }

        let pane_key = CString::new("pane_id").unwrap();
        let pane_ptr = unsafe { xpc_dictionary_get_string(reply, pane_key.as_ptr()) };
        if pane_ptr.is_null() {
            unsafe { xpc_release(reply) };
            return None;
        }

        let pane_str = unsafe { std::ffi::CStr::from_ptr(pane_ptr) }
            .to_str()
            .unwrap_or("")
            .to_string();

        let tab_key = CString::new("tab_id").unwrap();
        let tab_id = unsafe { xpc_dictionary_get_int64(reply, tab_key.as_ptr()) };

        let prof_key = CString::new("profile").unwrap();
        let prof_ptr = unsafe { xpc_dictionary_get_string(reply, prof_key.as_ptr()) };
        let prof_str = if !prof_ptr.is_null() {
            unsafe { std::ffi::CStr::from_ptr(prof_ptr) }
                .to_str()
                .unwrap_or("")
                .to_string()
        } else {
            String::new()
        };

        unsafe { xpc_release(reply) };
        Some((prof_str, pane_str, tab_id))
    }

    /// Validate a DevTools request before launching the TUI (Issue 687).
    /// Returns Ok(resolved_tab_id) on success, Err(error_message) on failure.
    pub fn send_query_devtools(
        &self,
        pane_id: &str,
        inspected_tab_id: i64,
        profile: &str,
    ) -> Result<i64, String> {
        let dict = unsafe { xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0) };
        if dict.is_null() {
            return Err("Failed to create XPC message".to_string());
        }

        unsafe {
            let key = CString::new("action").unwrap();
            let val = CString::new("query_devtools").unwrap();
            xpc_dictionary_set_string(dict, key.as_ptr(), val.as_ptr());

            let pk = CString::new("pane_id").unwrap();
            let pv = CString::new(pane_id).unwrap();
            xpc_dictionary_set_string(dict, pk.as_ptr(), pv.as_ptr());

            let tid_key = CString::new("inspected_tab_id").unwrap();
            xpc_dictionary_set_int64(dict, tid_key.as_ptr(), inspected_tab_id);

            let prof_key = CString::new("profile").unwrap();
            let prof_val = CString::new(profile).unwrap();
            xpc_dictionary_set_string(dict, prof_key.as_ptr(), prof_val.as_ptr());
        }

        let reply = unsafe { xpc_connection_send_message_with_reply_sync(self.raw, dict) };
        unsafe { xpc_release(dict) };

        if reply.is_null() {
            return Err("No reply from compositor".to_string());
        }

        // Check for error field first.
        let error_key = CString::new("error").unwrap();
        let error_ptr = unsafe { xpc_dictionary_get_string(reply, error_key.as_ptr()) };
        if !error_ptr.is_null() {
            let err = unsafe { std::ffi::CStr::from_ptr(error_ptr) }
                .to_str()
                .unwrap_or("Unknown error")
                .to_string();
            unsafe { xpc_release(reply) };
            return Err(err);
        }

        let tab_key = CString::new("tab_id").unwrap();
        let tab_id = unsafe { xpc_dictionary_get_int64(reply, tab_key.as_ptr()) };
        unsafe { xpc_release(reply) };
        Ok(tab_id)
    }

    /// Tell the compositor to navigate to a new URL.
    pub fn send_navigate(&self, pane_id: &str, url: &str) {
        let dict = unsafe { xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0) };
        if dict.is_null() {
            return;
        }

        unsafe {
            let action_key = CString::new("action").unwrap();
            let action_val = CString::new("navigate").unwrap();
            xpc_dictionary_set_string(dict, action_key.as_ptr(), action_val.as_ptr());

            let pane_key = CString::new("pane_id").unwrap();
            let pane_val = CString::new(pane_id).unwrap();
            xpc_dictionary_set_string(dict, pane_key.as_ptr(), pane_val.as_ptr());

            let url_key = CString::new("url").unwrap();
            let url_val = CString::new(url).unwrap();
            xpc_dictionary_set_string(dict, url_key.as_ptr(), url_val.as_ptr());

            xpc_connection_send_message(self.raw, dict);
            xpc_release(dict);
        }
    }

    /// Send a color scheme override to the compositor (Issue 680).
    pub fn send_set_color_scheme(&self, pane_id: &str, scheme: &str) {
        let dict = unsafe { xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0) };
        if dict.is_null() {
            return;
        }

        unsafe {
            let action_key = CString::new("action").unwrap();
            let action_val = CString::new("set_color_scheme").unwrap();
            xpc_dictionary_set_string(dict, action_key.as_ptr(), action_val.as_ptr());

            let pane_key = CString::new("pane_id").unwrap();
            let pane_val = CString::new(pane_id).unwrap();
            xpc_dictionary_set_string(dict, pane_key.as_ptr(), pane_val.as_ptr());

            let scheme_key = CString::new("scheme").unwrap();
            let scheme_val = CString::new(scheme).unwrap();
            xpc_dictionary_set_string(dict, scheme_key.as_ptr(), scheme_val.as_ptr());

            xpc_connection_send_message(self.raw, dict);
            xpc_release(dict);
        }
    }

    /// Notify the compositor of a mode change.
    pub fn send_mode_changed(&self, pane_id: &str, browsing: bool) {
        let dict = unsafe { xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0) };
        if dict.is_null() {
            return;
        }

        unsafe {
            let action_key = CString::new("action").unwrap();
            let action_val = CString::new("mode_changed").unwrap();
            xpc_dictionary_set_string(dict, action_key.as_ptr(), action_val.as_ptr());

            let pane_key = CString::new("pane_id").unwrap();
            let pane_val = CString::new(pane_id).unwrap();
            xpc_dictionary_set_string(dict, pane_key.as_ptr(), pane_val.as_ptr());

            let browsing_key = CString::new("browsing").unwrap();
            xpc_dictionary_set_bool(dict, browsing_key.as_ptr(), browsing);

            xpc_connection_send_message(self.raw, dict);
            xpc_release(dict);
        }
    }
}

impl Drop for CompositorConnection {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                xpc_connection_cancel(self.raw);
                xpc_release(self.raw);
            }
        }
    }
}
