//! cef-test profile server — headless CEF process with XPC connection.
//!
//! Loads a URL and renders it off-screen. Sends IOSurface Mach ports
//! to the GUI via XPC for cross-process texture sharing.
//!
//! Usage:
//!   cef-test-profile --session-id left-1 --url https://google.com \
//!     --profile left --service com.cef-test.launcher \
//!     [--width 800] [--height 600] [--scale 2.0]

use clap::Parser;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use termsurf_xpc::XpcConnection;

static FRAME_COUNTER: AtomicU64 = AtomicU64::new(0);
static START_TIME: OnceLock<Instant> = OnceLock::new();
static QUIT_FLAG: AtomicBool = AtomicBool::new(false);
/// Set to true when the page finishes loading (is_loading=0).
static PAGE_LOADED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
mod cfrunloop {
    use std::ffi::c_void;

    type CFStringRef = *const c_void;
    type CFTimeInterval = f64;

    extern "C" {
        static kCFRunLoopDefaultMode: CFStringRef;
        fn CFRunLoopRunInMode(
            mode: CFStringRef,
            seconds: CFTimeInterval,
            return_after_source_handled: u8,
        ) -> i32;
    }

    /// Run the main thread's CFRunLoop for up to `seconds`, returning after
    /// one source is handled or the timeout expires.
    pub fn run_for(seconds: f64) -> i32 {
        unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, seconds, 1) }
    }
}

#[derive(Parser)]
struct Args {
    /// Session ID for XPC endpoint claim
    #[arg(long)]
    session_id: String,

    #[arg(long)]
    url: String,

    /// Profile name (used for cache path isolation)
    #[arg(long, default_value = "default")]
    profile: String,

    /// Launcher Mach service name
    #[arg(long, default_value = "com.cef-test.launcher")]
    service: String,

    /// Logical width for CEF view_rect
    #[arg(long, default_value = "800")]
    width: u32,

    /// Logical height for CEF view_rect
    #[arg(long, default_value = "600")]
    height: u32,

    /// Device scale factor (e.g. 2.0 for Retina)
    #[arg(long, default_value = "2.0")]
    scale: f32,
}

fn main() {
    let args = Args::parse();
    println!(
        "Profile: session='{}', url='{}', profile='{}', size={}x{}, scale={}",
        args.session_id, args.url, args.profile, args.width, args.height, args.scale
    );

    #[cfg(target_os = "macos")]
    run(args);

    #[cfg(not(target_os = "macos"))]
    {
        let _ = args;
        eprintln!("Profile: CEF not supported on this platform");
        std::process::exit(1);
    }
}

struct ProfileState {
    url: String,
    width: std::sync::atomic::AtomicU32,
    height: std::sync::atomic::AtomicU32,
    scale: f32,
    #[cfg(target_os = "macos")]
    gui_conn: Arc<XpcConnection>,
    /// Stored after browser creation so the message loop can send scroll events.
    #[cfg(target_os = "macos")]
    browser: Mutex<Option<cef::Browser>>,
}

#[cfg(target_os = "macos")]
fn run(args: Args) {
    use cef::library_loader::LibraryLoader;
    use termsurf_xpc::*;

    let exe = std::env::current_exe().expect("Failed to get executable path");
    println!("Profile: exe={:?}", exe);

    // Load CEF framework (false = main process, not a helper)
    let _loader = LibraryLoader::new(&exe, false);
    if !_loader.load() {
        eprintln!("Profile: Failed to load CEF framework");
        std::process::exit(1);
    }
    println!("Profile: CEF framework loaded");

    // Required before creating App objects
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);

    // Subprocess check (early return for helper processes)
    let cef_args = cef::args::Args::new();
    let exit_code = cef::execute_process(
        Some(cef_args.as_main_args()),
        None::<&mut cef::App>,
        std::ptr::null_mut(),
    );
    if exit_code >= 0 {
        std::process::exit(exit_code);
    }
    println!("Profile: Main process (ret={})", exit_code);

    // Connect to launcher
    println!("Profile: Connecting to launcher '{}'...", args.service);
    let launcher = XpcConnection::connect_mach_service(&args.service)
        .expect("Failed to connect to launcher");

    set_event_handler(&launcher, |event| {
        if let Err(e) = event {
            eprintln!("Profile: Launcher error: {}", e);
        }
    });
    launcher.resume();
    std::thread::sleep(Duration::from_millis(100));

    // Claim session (gets GUI endpoint)
    println!("Profile: Claiming session '{}'...", args.session_id);
    let gui_endpoint = claim_session_with_retry(&launcher, &args.session_id)
        .expect("Failed to claim session");
    println!("Profile: Got GUI endpoint");

    // Connect directly to GUI via endpoint
    let gui_conn = Arc::new(
        XpcConnection::from_endpoint(gui_endpoint)
            .expect("Failed to connect to GUI"),
    );
    set_event_handler(&*gui_conn, |event| {
        if let Ok(msg) = event {
            let action = msg.get_string("action").unwrap_or_default();
            println!("Profile: Received from GUI: {}", action);
        } else if let Err(e) = event {
            eprintln!("Profile: GUI connection error: {}", e);
        }
    });
    gui_conn.resume();
    println!("Profile: Connected to GUI");

    // Profile state
    let state = Arc::new(ProfileState {
        url: args.url.clone(),
        width: std::sync::atomic::AtomicU32::new(args.width),
        height: std::sync::atomic::AtomicU32::new(args.height),
        scale: args.scale,
        gui_conn,
        browser: Mutex::new(None),
    });

    // Cache path (per-profile isolation)
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let cache_path = std::path::PathBuf::from(home)
        .join(".config/cef-test")
        .join(&args.profile);
    std::fs::create_dir_all(&cache_path).ok();
    println!("Profile: cache={:?}", cache_path);

    // CEF settings
    let settings = cef::Settings {
        windowless_rendering_enabled: 1,
        no_sandbox: 1,
        log_severity: cef::LogSeverity::VERBOSE,
        log_file: cef::CefString::from("/tmp/cef-test-debug.log"),
        root_cache_path: cef::CefString::from(cache_path.to_str().unwrap()),
        persist_session_cookies: 1,
        ..Default::default()
    };

    let mut app = cef_handlers::create_app(Arc::clone(&state));

    let init_result = cef::initialize(
        Some(cef_args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if init_result != 1 {
        eprintln!("Profile: CEF initialize failed (returned {})", init_result);
        std::process::exit(1);
    }
    println!("Profile: CEF initialized");

    // Ctrl+C handler
    ctrlc::set_handler(move || {
        println!("Profile: Ctrl+C, shutting down...");
        QUIT_FLAG.store(true, Ordering::Relaxed);
    })
    .expect("Failed to set Ctrl+C handler");

    // Message loop (matching ts3's pattern exactly)
    println!("Profile: Running message loop...");

    // Scroll simulation state
    let scroll_interval = Duration::from_millis(8); // ~125Hz, matching Apple mouse polling
    let mut last_scroll_time = Instant::now();
    let mut scroll_started = false;
    let mut scroll_direction: i32 = -1; // -1 = down, +1 = up
    let scroll_delta: i32 = 120; // standard scroll unit (one "notch")
    let direction_switch_every: u64 = 25; // reverse every 25 events (~200ms)
    let mut events_since_switch: u64 = 0;

    while !QUIT_FLAG.load(Ordering::Relaxed) {
        cef::do_message_loop_work();
        cfrunloop::run_for(0.0);

        // Simulated scroll input at ~125Hz
        if PAGE_LOADED.load(Ordering::Relaxed) {
            if !scroll_started {
                println!("[SCROLL] Page loaded, starting simulated scroll at ~125Hz");
                scroll_started = true;
                last_scroll_time = Instant::now();
            }

            let now = Instant::now();

            // Send scroll event at ~125Hz
            if now.duration_since(last_scroll_time) >= scroll_interval {
                last_scroll_time = now;

                // Reverse direction every N events to oscillate over a small region
                events_since_switch += 1;
                if events_since_switch >= direction_switch_every {
                    scroll_direction *= -1;
                    events_since_switch = 0;
                }
                if let Some(browser) = state.browser.lock().unwrap().as_ref() {
                    use cef::{ImplBrowser, ImplBrowserHost};
                    if let Some(host) = browser.host() {
                        let mouse_event = cef::MouseEvent {
                            x: 400, // center of 800px logical width
                            y: 400, // center of 800px logical height
                            modifiers: 0,
                        };
                        host.send_mouse_wheel_event(
                            Some(&mouse_event),
                            0,
                            scroll_delta * scroll_direction,
                        );
                    }
                }
            }
        }

    }

    // Shutdown
    println!("Profile: Shutting down...");
    cef::shutdown();
    println!("Profile: Done");
}

/// Claim a session from the launcher with exponential backoff retry.
#[cfg(target_os = "macos")]
fn claim_session_with_retry(
    launcher: &termsurf_xpc::XpcConnection,
    session_id: &str,
) -> termsurf_xpc::Result<termsurf_xpc::XpcEndpoint> {
    use termsurf_xpc::*;

    let max_retries = 10;
    let mut delay = Duration::from_millis(100);

    for attempt in 1..=max_retries {
        let msg = XpcDictionary::new();
        msg.set_string("action", "claim_session");
        msg.set_string("session_id", session_id);

        match launcher.send_with_reply_sync(&msg) {
            Ok(reply) => {
                if let Some(err) = reply.get_string("error") {
                    println!("Profile: Attempt {}/{}: {}", attempt, max_retries, err);
                    if attempt < max_retries {
                        std::thread::sleep(delay);
                        delay = (delay * 2).min(Duration::from_secs(2));
                        continue;
                    }
                    return Err(XpcError::Unknown(err));
                }
                if let Some(endpoint) = reply.get_endpoint("endpoint") {
                    return Ok(endpoint);
                }
                return Err(XpcError::Unknown("No endpoint in reply".into()));
            }
            Err(e) => {
                println!("Profile: Attempt {}/{}: {:?}", attempt, max_retries, e);
                if attempt < max_retries {
                    std::thread::sleep(delay);
                    delay = (delay * 2).min(Duration::from_secs(2));
                    continue;
                }
                return Err(e);
            }
        }
    }

    Err(termsurf_xpc::XpcError::Unknown(
        "Max retries exceeded".into(),
    ))
}

// ============================================================================
// CEF Handlers
// ============================================================================

#[cfg(target_os = "macos")]
mod cef_handlers {
    use super::ProfileState;
    use cef::rc::Rc;
    use cef::{
        wrap_app, wrap_browser_process_handler, wrap_client, wrap_context_menu_handler,
        wrap_load_handler, wrap_render_handler, AcceleratedPaintInfo, App, Browser,
        BrowserProcessHandler, BrowserSettings, CefString, Client, ContextMenuHandler,
        ContextMenuParams, Frame, ImplApp, ImplBrowser, ImplBrowserHost,
        ImplBrowserProcessHandler, ImplClient, ImplCommandLine, ImplContextMenuHandler,
        ImplLoadHandler, ImplMenuModel, ImplRenderHandler, LoadHandler, MenuModel,
        PaintElementType, Rect, RenderHandler, ScreenInfo, WindowInfo, WrapApp,
        WrapBrowserProcessHandler, WrapClient, WrapContextMenuHandler, WrapLoadHandler,
        WrapRenderHandler,
    };
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    // ====== Render Handler ======

    #[derive(Clone)]
    struct RenderHandlerInner {
        state: Arc<ProfileState>,
    }

    wrap_render_handler! {
        pub struct TestRenderHandler {
            inner: RenderHandlerInner,
        }

        impl RenderHandler {
            fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
                if let Some(rect) = rect {
                    rect.width = self.inner.state.width.load(Ordering::Relaxed) as i32;
                    rect.height = self.inner.state.height.load(Ordering::Relaxed) as i32;
                }
            }

            fn screen_info(
                &self,
                _browser: Option<&mut Browser>,
                screen_info: Option<&mut ScreenInfo>,
            ) -> ::std::os::raw::c_int {
                if let Some(info) = screen_info {
                    info.device_scale_factor = self.inner.state.scale;
                    return 1;
                }
                0
            }

            fn on_accelerated_paint(
                &self,
                _browser: Option<&mut Browser>,
                type_: PaintElementType,
                _dirty_rects: Option<&[Rect]>,
                info: Option<&AcceleratedPaintInfo>,
            ) {
                let Some(info) = info else { return };

                // Only handle PET_VIEW (skip popups)
                if type_ != PaintElementType::default() {
                    return;
                }

                let handle = info.shared_texture_io_surface as *mut std::ffi::c_void;
                if handle.is_null() {
                    return;
                }

                let now = std::time::Instant::now();
                let frame_id =
                    crate::FRAME_COUNTER.fetch_add(1, crate::Ordering::Relaxed);
                let start = *crate::START_TIME.get_or_init(|| now);
                let t_ms = start.elapsed().as_millis() as i64;
                let w = info.extra.coded_size.width;
                let h = info.extra.coded_size.height;

                // Create Mach port from IOSurface handle
                let port = termsurf_xpc::iosurface::create_mach_port(handle);
                if port == 0 {
                    eprintln!("[FRAME-TX] frame={} create_mach_port failed", frame_id);
                    return;
                }

                // Send to GUI via XPC
                let msg = termsurf_xpc::XpcDictionary::new();
                msg.set_string("action", "display_surface");
                msg.set_mach_send("iosurface_port", port);
                msg.set_i64("width", w as i64);
                msg.set_i64("height", h as i64);
                msg.set_i64("frame_id", frame_id as i64);
                msg.set_i64("tx_time_ms", t_ms);
                self.inner.state.gui_conn.send(&msg);
            }
        }
    }

    // ====== Context Menu Handler ======

    #[derive(Clone)]
    struct ContextMenuInner;

    wrap_context_menu_handler! {
        pub struct TestContextMenuHandler {
            inner: ContextMenuInner,
        }

        impl ContextMenuHandler {
            fn on_before_context_menu(
                &self,
                _browser: Option<&mut Browser>,
                _frame: Option<&mut Frame>,
                _params: Option<&mut ContextMenuParams>,
                model: Option<&mut MenuModel>,
            ) {
                if let Some(model) = model {
                    model.clear();
                }
            }
        }
    }

    // ====== Load Handler ======

    #[derive(Clone)]
    struct LoadHandlerInner;

    wrap_load_handler! {
        pub struct TestLoadHandler {
            inner: LoadHandlerInner,
        }

        impl LoadHandler {
            fn on_loading_state_change(
                &self,
                _browser: Option<&mut Browser>,
                is_loading: ::std::os::raw::c_int,
                _can_go_back: ::std::os::raw::c_int,
                _can_go_forward: ::std::os::raw::c_int,
            ) {
                if is_loading == 0 {
                    println!("[LOAD] Page finished loading");
                    crate::PAGE_LOADED.store(true, crate::Ordering::Relaxed);
                }
            }
        }
    }

    // ====== Client ======

    wrap_client! {
        pub struct TestClient {
            render_handler: RenderHandler,
            context_menu_handler: ContextMenuHandler,
            load_handler: LoadHandler,
        }

        impl Client {
            fn render_handler(&self) -> Option<RenderHandler> {
                Some(self.render_handler.clone())
            }

            fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
                Some(self.context_menu_handler.clone())
            }

            fn load_handler(&self) -> Option<LoadHandler> {
                Some(self.load_handler.clone())
            }
        }
    }

    // ====== Browser Process Handler ======

    wrap_browser_process_handler! {
        pub struct TestBPH {
            state: Arc<ProfileState>,
        }

        impl BrowserProcessHandler {
            fn on_context_initialized(&self) {
                println!("Profile: CEF context initialized, creating browser...");

                let render_inner = RenderHandlerInner {
                    state: Arc::clone(&self.state),
                };
                let render_handler = TestRenderHandler::new(render_inner);
                let context_menu_handler = TestContextMenuHandler::new(ContextMenuInner);
                let load_handler = TestLoadHandler::new(LoadHandlerInner);
                let mut client = TestClient::new(render_handler, context_menu_handler, load_handler);

                let window_info = WindowInfo {
                    windowless_rendering_enabled: 1,
                    shared_texture_enabled: 1,
                    ..Default::default()
                };

                let browser_settings = BrowserSettings {
                    windowless_frame_rate: 60,
                    background_color: 0xFFFFFFFF,
                    ..Default::default()
                };

                let url: CefString = self.state.url.as_str().into();

                let browser = cef::browser_host_create_browser_sync(
                    Some(&window_info),
                    Some(&mut client),
                    Some(&url),
                    Some(&browser_settings),
                    None,
                    None,
                );

                match browser {
                    Some(b) => {
                        let id = b.identifier();
                        println!(
                            "Profile: Browser {} created for '{}'",
                            id, self.state.url
                        );
                        if let Some(host) = b.host() {
                            host.set_focus(0);
                            host.set_focus(1);
                        }
                        // Store browser so the message loop can send scroll events
                        *self.state.browser.lock().unwrap() = Some(b);
                    }
                    None => eprintln!("Profile: Failed to create browser"),
                }
            }
        }
    }

    // ====== App ======

    wrap_app! {
        pub struct TestApp {
            handler: BrowserProcessHandler,
        }

        impl App {
            fn on_before_command_line_processing(
                &self,
                _process_type: Option<&cef::CefStringUtf16>,
                command_line: Option<&mut cef::CommandLine>,
            ) {
                if let Some(command_line) = command_line {
                    command_line.append_switch(Some(&"no-startup-window".into()));
                    command_line.append_switch(Some(&"enable-logging".into()));
                    command_line.append_switch_with_value(
                        Some(&"v".into()),
                        Some(&"1".into()),
                    );
                }
            }

            fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
                Some(self.handler.clone())
            }
        }
    }

    pub fn create_app(state: Arc<ProfileState>) -> App {
        let handler = TestBPH::new(state);
        TestApp::new(handler)
    }
}
