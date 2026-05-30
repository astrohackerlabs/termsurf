mod dispatch;
mod ffi;
mod ipc;
mod proto;

use std::ffi::{c_void, CString};
use std::ptr;
use std::sync::OnceLock;

use proto::{Msg, TermSurfMessage};

// --- Globals (set before ts_content_main, read on UI thread) ---

static SOCKET_PATH: OnceLock<String> = OnceLock::new();
static LISTEN_PATH: OnceLock<String> = OnceLock::new();
static PROFILE_NAME: OnceLock<String> = OnceLock::new();

static mut BROWSER_CONTEXT: ffi::TsBrowserContext = ptr::null_mut();

pub fn browser_context() -> ffi::TsBrowserContext {
    unsafe { BROWSER_CONTEXT }
}

// --- Callbacks ---

unsafe extern "C" fn on_initialized(_user_data: *mut c_void) {
    // Create browser context.
    unsafe {
        BROWSER_CONTEXT = ffi::ts_create_browser_context(ptr::null());
    }

    // Connect to GUI socket.
    let Some(path) = SOCKET_PATH.get() else {
        eprintln!("[Roamium] No --ipc-socket, skipping IPC");
        return;
    };

    let Some(reader) = ipc::connect(path) else {
        return;
    };

    // Send ServerRegister.
    let profile = PROFILE_NAME.get().cloned().unwrap_or_default();
    let msg = TermSurfMessage {
        msg: Some(Msg::ServerRegister(proto::termsurf::ServerRegister {
            profile,
        })),
    };
    ipc::send(&msg);

    // Start reader thread.
    std::thread::spawn(move || {
        ipc::reader_loop(reader);
    });

    // Start listener if --listen-socket was provided.
    if let Some(path) = LISTEN_PATH.get() {
        ipc::listen(path);
    }
}

// --- main ---

fn main() {
    dispatch::init_pdf_input_trace();

    // Parse --ipc-socket= and --user-data-dir= from argv.
    for arg in std::env::args().skip(1) {
        if let Some(val) = arg.strip_prefix("--ipc-socket=") {
            let _ = SOCKET_PATH.set(val.to_string());
        } else if let Some(val) = arg.strip_prefix("--listen-socket=") {
            let _ = LISTEN_PATH.set(val.to_string());
        } else if let Some(val) = arg.strip_prefix("--user-data-dir=") {
            let name = val.rsplit('/').next().unwrap_or(val);
            let _ = PROFILE_NAME.set(name.to_string());
        }
    }

    // Build argc/argv for ts_content_main.
    let args: Vec<CString> = std::env::args().map(|a| CString::new(a).unwrap()).collect();
    let argv: Vec<*const i8> = args.iter().map(|a| a.as_ptr()).collect();

    // Register callbacks before entering the message loop.
    unsafe {
        ffi::ts_set_on_initialized(Some(on_initialized), ptr::null_mut());
        ffi::ts_set_on_tab_ready(Some(dispatch::on_tab_ready), ptr::null_mut());
        ffi::ts_set_on_ca_context_id(Some(dispatch::on_ca_context_id), ptr::null_mut());
        ffi::ts_set_on_url_changed(Some(dispatch::on_url_changed), ptr::null_mut());
        ffi::ts_set_on_loading_state(Some(dispatch::on_loading_state), ptr::null_mut());
        ffi::ts_set_on_title_changed(Some(dispatch::on_title_changed), ptr::null_mut());
        ffi::ts_set_on_cursor_changed(Some(dispatch::on_cursor_changed), ptr::null_mut());
        ffi::ts_set_on_target_url_changed(Some(dispatch::on_target_url_changed), ptr::null_mut());
    }

    // Enter Chromium's message loop (blocks until shutdown).
    let ret = unsafe { ffi::ts_content_main(argv.len() as i32, argv.as_ptr()) };
    std::process::exit(ret);
}
