//! CEF Profile Server for TermSurf.
//!
//! Renders webpages using CEF off-screen rendering and sends IOSurface
//! textures to the GUI via XPC Mach port transfer.
//!
//! Spawned by the launcher with: --profile, --url, --session-id, --width, --height, --scale
//!
//! Architecture:
//! 1. Load CEF framework, handle subprocess early return
//! 2. Claim XPC session from launcher, get direct GUI endpoint
//! 3. Initialize CEF with profile-specific cache path
//! 4. Create browser in on_context_initialized callback
//! 5. on_accelerated_paint sends IOSurface Mach port to GUI
//! 6. run_message_loop() blocks until Ctrl+C

use clap::Parser;
use std::thread;
use std::time::Duration;
use termsurf_xpc::*;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    profile: String,

    #[arg(long)]
    url: String,

    #[arg(long)]
    session_id: String,

    /// Logical width for CEF view_rect (physical pixels / scale)
    #[arg(long, default_value = "800")]
    width: u32,

    /// Logical height for CEF view_rect (physical pixels / scale)
    #[arg(long, default_value = "600")]
    height: u32,

    /// Device scale factor (e.g. 2.0 for Retina)
    #[arg(long, default_value = "2.0")]
    scale: f32,
}

fn main() {
    let args = Args::parse();
    println!(
        "Profile: Starting session='{}', url='{}', profile='{}', size={}x{}, scale={}",
        args.session_id, args.url, args.profile, args.width, args.height, args.scale
    );

    #[cfg(target_os = "macos")]
    run_profile_server(args);

    #[cfg(not(target_os = "macos"))]
    {
        let _ = args;
        eprintln!("Profile: CEF not supported on this platform");
        std::process::exit(1);
    }
}

#[cfg(target_os = "macos")]
fn run_profile_server(args: Args) {
    use cef::library_loader::LibraryLoader;

    let exe = std::env::current_exe().expect("Failed to get executable path");
    println!("Profile: Executable: {:?}", exe);

    // 1. Load CEF framework
    let _loader = LibraryLoader::new(&exe, false);
    if !_loader.load() {
        eprintln!("Profile: Failed to load CEF framework");
        std::process::exit(1);
    }
    println!("Profile: CEF framework loaded");

    // Configure CEF API version (required before creating App objects)
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);

    // 2. Subprocess check (early return for helper processes)
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

    // 3. Connect to launcher, claim session, get GUI endpoint
    let gui = connect_and_claim_session(&args.session_id);
    let gui = std::sync::Arc::new(gui);
    println!("Profile: Connected to GUI");

    // 4. Compute paths
    let app_contents = exe.parent().unwrap().parent().unwrap();
    let helper_path = app_contents
        .join("Frameworks")
        .join("WezTerm Helper.app")
        .join("Contents/MacOS/WezTerm Helper");
    println!("Profile: Helper: {:?} (exists={})", helper_path, helper_path.exists());

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let cache_path = std::path::PathBuf::from(home)
        .join(".config/termsurf/cef")
        .join(&args.profile);
    std::fs::create_dir_all(&cache_path).ok();
    println!("Profile: Cache: {:?}", cache_path);

    // 5. Initialize CEF
    let settings = cef::Settings {
        windowless_rendering_enabled: 1,
        no_sandbox: 1,
        root_cache_path: cef::CefString::from(cache_path.to_str().unwrap()),
        browser_subprocess_path: cef::CefString::from(helper_path.to_str().unwrap()),
        persist_session_cookies: 1,
        ..Default::default()
    };

    let shared = std::sync::Arc::new(SharedState {
        gui,
        url: args.url,
        width: std::sync::atomic::AtomicU32::new(args.width),
        height: std::sync::atomic::AtomicU32::new(args.height),
        scale: args.scale,
    });
    let mut app = cef_handlers::create_app(shared);

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

    // 6. Install Ctrl+C handler for clean shutdown
    ctrlc::set_handler(|| {
        println!("Profile: Ctrl+C, quitting...");
        cef::quit_message_loop();
    })
    .expect("Failed to set Ctrl+C handler");

    // 7. Run CEF message loop (blocks until quit_message_loop)
    // on_context_initialized fires during this loop, creating the browser.
    // on_accelerated_paint fires when pages render, sending IOSurface to GUI.
    println!("Profile: Running message loop...");
    cef::run_message_loop();

    // 8. Shutdown
    println!("Profile: Shutting down...");
    cef::shutdown();
    println!("Profile: Done");
    // _loader dropped here, unloading CEF framework
}

/// Shared state accessible to CEF handlers via Arc
struct SharedState {
    gui: std::sync::Arc<XpcConnection>,
    url: String,
    width: std::sync::atomic::AtomicU32,
    height: std::sync::atomic::AtomicU32,
    scale: f32,
}

/// Connect to launcher, claim session, and establish direct GUI connection
fn connect_and_claim_session(session_id: &str) -> XpcConnection {
    println!("Profile: Connecting to launcher...");
    let launcher = XpcConnection::connect_mach_service("com.termsurf.launcher")
        .expect("Failed to connect to launcher");

    set_event_handler(&launcher, |event| {
        if let Err(e) = event {
            eprintln!("Profile: Launcher error: {}", e);
        }
    });
    launcher.resume();
    thread::sleep(Duration::from_millis(100));

    // Claim session with retry (session may not be registered yet)
    println!("Profile: Claiming session '{}'...", session_id);
    let gui_endpoint =
        claim_session_with_retry(&launcher, session_id).expect("Failed to claim session");
    println!("Profile: Got GUI endpoint");

    // Connect directly to GUI
    let gui = XpcConnection::from_endpoint(gui_endpoint).expect("Failed to connect to GUI");
    set_event_handler(&gui, |event| {
        if let Err(e) = event {
            eprintln!("Profile: GUI error: {}", e);
        }
    });
    gui.resume();
    thread::sleep(Duration::from_millis(100));

    gui
}

/// Claim session with exponential backoff retry
fn claim_session_with_retry(
    launcher: &XpcConnection,
    session_id: &str,
) -> Result<XpcEndpoint> {
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
                        thread::sleep(delay);
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
                    thread::sleep(delay);
                    delay = (delay * 2).min(Duration::from_secs(2));
                    continue;
                }
                return Err(e);
            }
        }
    }

    Err(XpcError::Unknown("Max retries exceeded".into()))
}

// ============================================================================
// CEF Handlers
// ============================================================================

#[cfg(target_os = "macos")]
mod cef_handlers {
    use super::SharedState;
    use cef::rc::Rc;
    use cef::{
        wrap_app, wrap_browser_process_handler, wrap_client, wrap_context_menu_handler,
        wrap_render_handler, AcceleratedPaintInfo, App, Browser, BrowserProcessHandler,
        BrowserSettings, Client, ContextMenuHandler, ContextMenuParams, Frame, ImplApp,
        ImplBrowserProcessHandler, ImplClient, ImplCommandLine, ImplContextMenuHandler,
        ImplMenuModel, ImplRenderHandler, MenuModel, PaintElementType, Rect, RenderHandler,
        ScreenInfo, WindowInfo, WrapApp, WrapBrowserProcessHandler, WrapClient,
        WrapContextMenuHandler, WrapRenderHandler,
    };
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicPtr, Ordering};
    use std::sync::Arc;
    use termsurf_xpc::*;

    // ====== Render Handler ======
    //
    // Sends IOSurface Mach ports to the GUI via XPC when CEF paints.
    // Deduplicates by tracking the last IOSurface handle pointer.

    #[derive(Clone)]
    struct RenderHandlerInner {
        gui: Arc<XpcConnection>,
        last_handle: Arc<AtomicPtr<c_void>>,
        state: Arc<SharedState>,
    }

    wrap_render_handler! {
        pub struct ProfileRenderHandler {
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

                // Dedup: only send when IOSurface handle changes.
                // CEF calls on_accelerated_paint every frame (cursor blinks, etc.)
                // but reuses the same IOSurface buffer. We only need to send a new
                // Mach port when the buffer changes (double-buffering swap).
                let handle = info.shared_texture_io_surface as *mut c_void;
                if handle.is_null() {
                    return;
                }
                let prev = self.inner.last_handle.swap(handle, Ordering::Relaxed);
                if handle == prev {
                    return;
                }

                // Create Mach port from IOSurface handle
                let port = termsurf_xpc::iosurface::create_mach_port(handle);
                if port == 0 {
                    eprintln!("Profile: create_mach_port failed");
                    return;
                }

                let width = info.extra.coded_size.width;
                let height = info.extra.coded_size.height;
                println!("Profile: Sending IOSurface {}x{} (port={})", width, height, port);

                // Send to GUI via XPC
                let msg = XpcDictionary::new();
                msg.set_string("action", "display_surface");
                msg.set_mach_send("iosurface_port", port);
                msg.set_i64("width", width as i64);
                msg.set_i64("height", height as i64);
                self.inner.gui.send(&msg);
            }
        }
    }

    // ====== Context Menu Handler ======
    //
    // Suppresses the native context menu to avoid crashes.

    #[derive(Clone)]
    struct ContextMenuInner;

    wrap_context_menu_handler! {
        pub struct ProfileContextMenuHandler {
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

    // ====== Client ======

    wrap_client! {
        pub struct ProfileClient {
            render_handler: RenderHandler,
            context_menu_handler: ContextMenuHandler,
        }

        impl Client {
            fn render_handler(&self) -> Option<RenderHandler> {
                Some(self.render_handler.clone())
            }

            fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
                Some(self.context_menu_handler.clone())
            }
        }
    }

    // ====== Browser Process Handler ======
    //
    // Creates the browser in on_context_initialized, which fires during
    // run_message_loop() when CEF is fully ready.

    wrap_browser_process_handler! {
        pub struct ProfileBPH {
            state: Arc<SharedState>,
        }

        impl BrowserProcessHandler {
            fn on_context_initialized(&self) {
                println!("Profile: CEF context initialized, creating browser...");

                let inner = RenderHandlerInner {
                    gui: self.state.gui.clone(),
                    last_handle: Arc::new(AtomicPtr::new(std::ptr::null_mut())),
                    state: Arc::clone(&self.state),
                };

                let render_handler = ProfileRenderHandler::new(inner);
                let context_menu_handler = ProfileContextMenuHandler::new(ContextMenuInner);
                let mut client = ProfileClient::new(render_handler, context_menu_handler);

                let window_info = WindowInfo {
                    windowless_rendering_enabled: 1,
                    shared_texture_enabled: 1,
                    ..Default::default()
                };

                let browser_settings = BrowserSettings {
                    windowless_frame_rate: 60,
                    ..Default::default()
                };

                let url: cef::CefString = self.state.url.as_str().into();

                let browser = cef::browser_host_create_browser_sync(
                    Some(&window_info),
                    Some(&mut client),
                    Some(&url),
                    Some(&browser_settings),
                    None, // extra_info
                    None, // request_context (uses global with our root_cache_path)
                );

                match browser {
                    Some(_) => println!("Profile: Browser created for '{}'", self.state.url),
                    None => eprintln!("Profile: Failed to create browser"),
                }
            }
        }
    }

    // ====== App ======

    wrap_app! {
        pub struct ProfileCefApp {
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
                }
            }

            fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
                Some(self.handler.clone())
            }
        }
    }

    pub fn create_app(state: Arc<SharedState>) -> App {
        let handler = ProfileBPH::new(state);
        ProfileCefApp::new(handler)
    }
}
