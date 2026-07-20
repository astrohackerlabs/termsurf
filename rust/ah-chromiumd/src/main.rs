mod dispatch;
mod ffi;
mod ipc;
mod proto;

use std::ffi::{c_void, CString};
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::ptr;
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use proto::{Msg, TermSurfMessage};

// --- Globals (set before ts_content_main, read on UI thread) ---

static SOCKET_PATH: OnceLock<String> = OnceLock::new();
static LISTEN_PATH: OnceLock<String> = OnceLock::new();
static PROFILE_NAME: OnceLock<String> = OnceLock::new();
static BROWSER_NAME: OnceLock<String> = OnceLock::new();
static INCOGNITO: OnceLock<bool> = OnceLock::new();
static STARTUP_TRACE_START: OnceLock<Instant> = OnceLock::new();

static mut BROWSER_CONTEXT: ffi::TsBrowserContext = ptr::null_mut();

pub fn browser_context() -> ffi::TsBrowserContext {
    unsafe { BROWSER_CONTEXT }
}

// --- Callbacks ---

unsafe extern "C" fn on_initialized(_user_data: *mut c_void) {
    startup_trace("on_initialized_entry");

    // Create browser context.
    unsafe {
        BROWSER_CONTEXT = if *INCOGNITO.get().unwrap_or(&false) {
            ffi::ts_create_incognito_browser_context()
        } else {
            ffi::ts_create_browser_context(ptr::null())
        };
    }
    startup_trace("browser_context_created");

    // Connect to GUI socket.
    let Some(path) = SOCKET_PATH.get() else {
        eprintln!("[Chromium] No --ipc-socket, skipping IPC");
        startup_trace("ipc_socket_missing");
        return;
    };

    startup_trace("ipc_connect_start");
    let Some(reader) = ipc::connect(path) else {
        startup_trace("ipc_connect_failed");
        return;
    };
    startup_trace("ipc_connect_done");

    // Send ServerRegister.
    let profile = PROFILE_NAME.get().cloned().unwrap_or_default();
    let browser = BROWSER_NAME
        .get()
        .cloned()
        .unwrap_or_else(|| "chromium".to_string());
    let msg = TermSurfMessage {
        msg: Some(Msg::ServerRegister(proto::termsurf::ServerRegister {
            profile,
            browser,
        })),
    };
    ipc::send(&msg);
    startup_trace("server_register_sent");

    // Start reader thread.
    std::thread::spawn(move || {
        ipc::reader_loop(reader);
    });
    startup_trace("reader_thread_started");

    // Start listener if --listen-socket was provided.
    if let Some(path) = LISTEN_PATH.get() {
        startup_trace("listener_start");
        ipc::listen(path);
        startup_trace("listener_exit");
    }
}

fn startup_trace_enabled() -> bool {
    matches!(
        std::env::var("TERMSURF_ENGINE_STARTUP_TRACE"),
        Ok(value) if value != "0" && value != "false"
    )
}

fn startup_wall_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn startup_trace(event: &str) {
    let start = STARTUP_TRACE_START.get_or_init(Instant::now);
    if !startup_trace_enabled() {
        return;
    }

    let elapsed_ms = start.elapsed().as_millis();
    let pid = std::process::id();
    let profile = PROFILE_NAME.get().map(String::as_str).unwrap_or("");
    let browser = BROWSER_NAME.get().map(String::as_str).unwrap_or("chromium");
    let listen_socket = LISTEN_PATH.get().map(String::as_str).unwrap_or("");
    let ipc_socket = SOCKET_PATH.get().map(String::as_str).unwrap_or("");
    let line = format!(
        "TermSurfEngineStartup event={event} engine=chromium browser={browser} profile={profile} pid={pid} wall_ms={} elapsed_ms={elapsed_ms} listen_socket={listen_socket} ipc_socket={ipc_socket}",
        startup_wall_ms()
    );

    eprintln!("{line}");

    if let Some(path) = std::env::var_os("TERMSURF_ENGINE_STARTUP_TRACE_FILE") {
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent() {
            let _ = create_dir_all(parent);
        }
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{line}");
        }
    }
}

// --- main ---

fn main() {
    if handle_identity_arg(std::env::args().skip(1)) {
        return;
    }

    let _ = STARTUP_TRACE_START.set(Instant::now());
    startup_trace("main_entry");
    dispatch::init_pdf_input_trace();
    startup_trace("dispatch_trace_initialized");

    // Parse --ipc-socket= and --user-data-dir= from argv.
    for arg in std::env::args().skip(1) {
        if let Some(val) = arg.strip_prefix("--ipc-socket=") {
            let _ = SOCKET_PATH.set(val.to_string());
        } else if let Some(val) = arg.strip_prefix("--listen-socket=") {
            let _ = LISTEN_PATH.set(val.to_string());
        } else if let Some(val) = arg.strip_prefix("--user-data-dir=") {
            let name = val.rsplit('/').next().unwrap_or(val);
            let _ = PROFILE_NAME.set(name.to_string());
        } else if let Some(val) = arg.strip_prefix("--browser-name=") {
            let _ = BROWSER_NAME.set(val.to_string());
        } else if arg == "--incognito" {
            let _ = INCOGNITO.set(true);
        }
    }

    if *INCOGNITO.get().unwrap_or(&false) && PROFILE_NAME.get().is_none() {
        let _ = PROFILE_NAME.set("incognito".to_string());
    }
    startup_trace("args_parsed");

    if std::env::args().any(|arg| arg == "--termsurf-warmup") {
        startup_trace("warmup_exit");
        return;
    }

    // Build argc/argv for ts_content_main.
    let args: Vec<CString> = std::env::args().map(|a| CString::new(a).unwrap()).collect();
    let argv: Vec<*const i8> = args.iter().map(|a| a.as_ptr()).collect();
    startup_trace("argv_built");

    // Register callbacks before entering the message loop.
    unsafe {
        ffi::ts_set_on_initialized(Some(on_initialized), ptr::null_mut());
        ffi::ts_set_on_tab_ready(Some(dispatch::on_tab_ready), ptr::null_mut());
        ffi::ts_set_on_ca_context_id(Some(dispatch::on_ca_context_id), ptr::null_mut());
        ffi::ts_set_on_url_changed(Some(dispatch::on_url_changed), ptr::null_mut());
        ffi::ts_set_on_loading_state(Some(dispatch::on_loading_state), ptr::null_mut());
        ffi::ts_set_on_navigation_state(Some(dispatch::on_navigation_state), ptr::null_mut());
        ffi::ts_set_on_title_changed(Some(dispatch::on_title_changed), ptr::null_mut());
        ffi::ts_set_on_cursor_changed(Some(dispatch::on_cursor_changed), ptr::null_mut());
        ffi::ts_set_on_target_url_changed(Some(dispatch::on_target_url_changed), ptr::null_mut());
        ffi::ts_set_on_javascript_dialog_request(
            Some(dispatch::on_javascript_dialog_request),
            ptr::null_mut(),
        );
        ffi::ts_set_on_console_message(Some(dispatch::on_console_message), ptr::null_mut());
        ffi::ts_set_on_http_auth_request(Some(dispatch::on_http_auth_request), ptr::null_mut());
        ffi::ts_set_on_renderer_crashed(Some(dispatch::on_renderer_crashed), ptr::null_mut());
    }
    startup_trace("callbacks_registered");

    // Enter Chromium's message loop (blocks until shutdown).
    startup_trace("ts_content_main_entry");
    let ret = unsafe { ffi::ts_content_main(argv.len() as i32, argv.as_ptr()) };
    startup_trace("ts_content_main_exit");
    std::process::exit(ret);
}

fn handle_identity_arg<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    for arg in args {
        match arg.as_ref() {
            "--version" => {
                println!(
                    "Astrohacker Chromium Engine {}",
                    env!("ASTROHACKER_CLI_VERSION")
                );
                return true;
            }
            "--help" | "-h" => {
                print!(
                    "Astrohacker Chromium Engine — Chromium support helper for Astrohacker TermSurf\n\n\
Usage: ah-chromiumd [OPTIONS]\n\n\
Options:\n      --ipc-socket=<PATH>       Connect to an Astrohacker TermSurf IPC socket\n      --listen-socket=<PATH>    Listen for browser IPC clients\n      --user-data-dir=<PATH>    Browser profile data directory\n      --browser-name=<NAME>     Browser identity to register\n      --incognito               Use an incognito browser context\n      --termsurf-warmup         Warm runtime dependencies and exit\n  -h, --help                    Print help\n      --version                 Print version\n"
                );
                return true;
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::handle_identity_arg;

    #[test]
    fn identity_args_exit_before_runtime_setup() {
        assert!(handle_identity_arg(["--version"]));
        assert!(handle_identity_arg(["--help"]));
        assert!(handle_identity_arg(["-h"]));
        assert!(!handle_identity_arg(["--termsurf-warmup"]));
    }
}
