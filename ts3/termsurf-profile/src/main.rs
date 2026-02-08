//! CEF Profile Server for TermSurf.
//!
//! Renders webpages using CEF off-screen rendering and sends IOSurface
//! textures to the GUI via XPC Mach port transfer.
//!
//! Spawned by the launcher with: --profile, --url, --session-id, --width, --height, --scale
//!
//! Architecture:
//! 1. Load CEF framework, handle subprocess early return
//! 2. Connect to launcher, claim initial session
//! 3. Initialize CEF with profile-specific cache path
//! 4. Create command listener and register with launcher
//! 5. Create initial browser in on_context_initialized callback
//! 6. Handle create_browser commands from launcher for additional browsers
//! 7. on_accelerated_paint sends IOSurface Mach port to GUI (per-browser)
//! 8. run_message_loop() blocks until Ctrl+C

use clap::Parser;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

// Issue 325, Experiment 2: Diagnostic logging for frame rate analysis
static FRAME_COUNTER: AtomicU64 = AtomicU64::new(0);
static PROFILE_START_TIME: OnceLock<Instant> = OnceLock::new();

// Issue 326, Experiment 1: Global quit flag for graceful shutdown on GUI disconnect
static QUIT_FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// Issue 345: Set to true when the page finishes loading (for benchmark scroll start)
static PAGE_LOADED: AtomicBool = AtomicBool::new(false);

// Issue 346, Experiment 1: Count mouse_move events received by the profile server
static MOUSE_MOVE_COUNT: AtomicU64 = AtomicU64::new(0);

// Issue 346, Experiment 2: Count cursor change callbacks from CEF
static CURSOR_CHANGE_COUNT: AtomicU64 = AtomicU64::new(0);

// Issue 330, Experiment 3: Track active connections by ID for idempotent cleanup
// Replaces the old GUI_CONNECTION_COUNT counter which could be decremented multiple times
static CONNECTION_ID: AtomicU64 = AtomicU64::new(0);
static ACTIVE_CONNECTIONS: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();

// Issue 332, Experiment 2: Store launcher connection for unregister notification
static LAUNCHER_CONNECTION: OnceLock<Arc<XpcConnection>> = OnceLock::new();

fn active_connections() -> &'static Mutex<HashSet<u64>> {
    ACTIVE_CONNECTIONS.get_or_init(|| Mutex::new(HashSet::new()))
}

// Issue 345: Benchmark frame statistics (only initialized in benchmark mode)
static BENCHMARK_STATS: OnceLock<Mutex<FrameStats>> = OnceLock::new();

struct FrameStats {
    frame_count: u64,
    last_rx_time: Option<Instant>,
    first_rx_time: Option<Instant>,
    intervals_us: Vec<u64>,
}

impl FrameStats {
    fn new() -> Self {
        Self {
            frame_count: 0,
            last_rx_time: None,
            first_rx_time: None,
            intervals_us: Vec::with_capacity(8000),
        }
    }

    fn record_frame(&mut self) {
        let now = Instant::now();
        self.frame_count += 1;
        if self.first_rx_time.is_none() {
            self.first_rx_time = Some(now);
        }
        if let Some(prev) = self.last_rx_time {
            let interval_us = now.duration_since(prev).as_micros() as u64;
            self.intervals_us.push(interval_us);
        }
        self.last_rx_time = Some(now);
    }

    fn print_summary(&self) {
        let n = self.intervals_us.len();
        if n == 0 {
            println!("[PERF] No frames recorded");
            return;
        }

        let total_duration = match (self.first_rx_time, self.last_rx_time) {
            (Some(first), Some(last)) => last.duration_since(first),
            _ => Duration::ZERO,
        };
        let total_secs = total_duration.as_secs_f64();
        let avg_fps = if total_secs > 0.0 {
            self.frame_count as f64 / total_secs
        } else {
            0.0
        };

        // Count frames at 60fps (interval 10-20ms to allow some jitter)
        let at_60fps = self
            .intervals_us
            .iter()
            .filter(|&&i| i >= 10_000 && i <= 20_000)
            .count();
        let pct_60fps = (at_60fps as f64 / n as f64) * 100.0;

        // Max consecutive 60fps streak
        let mut max_streak = 0u64;
        let mut current_streak = 0u64;
        for &interval in &self.intervals_us {
            if interval >= 10_000 && interval <= 20_000 {
                current_streak += 1;
                if current_streak > max_streak {
                    max_streak = current_streak;
                }
            } else {
                current_streak = 0;
            }
        }

        // Percentiles
        let mut sorted = self.intervals_us.clone();
        sorted.sort();
        let p50 = sorted[n / 2];
        let p95 = sorted[(n as f64 * 0.95) as usize];
        let p99 = sorted[(n as f64 * 0.99) as usize];
        let min_interval = sorted[0];
        let max_interval = sorted[n - 1];

        println!(
            "[PERF] frames={} duration={:.1}s avg_fps={:.1} 60fps%={:.1} max_streak={}",
            self.frame_count, total_secs, avg_fps, pct_60fps, max_streak
        );
        println!(
            "[PERF] intervals: min={}us p50={}us p95={}us p99={}us max={}us",
            min_interval, p50, p95, p99, max_interval
        );
    }
}

// Issue 342, Experiment 5: Minimal CFRunLoop FFI for servicing run loop sources.
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

    /// Enable benchmark mode: simulate scrolling at ~125Hz and collect frame stats
    #[arg(long)]
    benchmark: bool,

    /// Benchmark trial duration in seconds (default: 70)
    #[arg(long, default_value = "70")]
    benchmark_duration: u64,
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

// ============================================================================
// Profile State (multi-browser support)
// ============================================================================

/// Info for creating the initial browser (from CLI args)
struct InitialBrowserInfo {
    url: String,
    session_id: String,
    gui_endpoint: XpcEndpoint,
    width: u32,
    height: u32,
}

/// Per-browser state
struct BrowserState {
    session_id: String,
    gui: Arc<XpcConnection>,
    width: AtomicU32,
    height: AtomicU32,
    /// Browser reference for resize operations
    browser: Mutex<Option<cef::Browser>>,
    /// Current URL (updated on navigation via DisplayHandler)
    url: Mutex<String>,
    /// Issue 328: Whether initial focus has been set (must wait for first paint)
    initial_focus_set: AtomicBool,
}

/// Profile-wide state (shared across all browsers in this process)
struct ProfileState {
    scale: f32,
    profile: String,
    initial_browser_info: Mutex<Option<InitialBrowserInfo>>,
    browsers: Mutex<HashMap<i32, Arc<BrowserState>>>,
    command_connections: Mutex<Vec<Arc<XpcConnection>>>,
    /// Pending browser creation requests (session_id, url, gui_endpoint, width, height)
    pending_browsers: Mutex<Vec<(String, String, XpcEndpoint, u32, u32)>>,
}

/// Global profile state, set once during initialization
static PROFILE_STATE: OnceLock<Arc<ProfileState>> = OnceLock::new();

#[cfg(target_os = "macos")]
fn run_profile_server(args: Args) {
    use cef::library_loader::LibraryLoader;

    let exe = std::env::current_exe().expect("Failed to get executable path");
    println!("Profile: Executable: {:?}", exe);

    // 1. Load CEF framework (helper=true because we're in a helper app bundle)
    let _loader = LibraryLoader::new(&exe, true);
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

    // 3. Connect to launcher
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

    // 4. Claim initial session (gets gui_endpoint for first browser)
    println!("Profile: Claiming session '{}'...", args.session_id);
    let initial_gui_endpoint =
        claim_session_with_retry(&launcher, &args.session_id).expect("Failed to claim session");
    println!("Profile: Got GUI endpoint for initial browser");

    // 5. Initialize ProfileState BEFORE CEF init
    let profile_state = Arc::new(ProfileState {
        scale: args.scale,
        profile: args.profile.clone(),
        initial_browser_info: Mutex::new(Some(InitialBrowserInfo {
            url: args.url.clone(),
            session_id: args.session_id.clone(),
            gui_endpoint: initial_gui_endpoint,
            width: args.width,
            height: args.height,
        })),
        browsers: Mutex::new(HashMap::new()),
        command_connections: Mutex::new(Vec::new()),
        pending_browsers: Mutex::new(Vec::new()),
    });
    // Store in global state (panics if already set, which shouldn't happen)
    let _ = PROFILE_STATE.set(Arc::clone(&profile_state));

    // 6. Compute paths
    // Navigate from helper app binary to main app's Contents/
    // Binary is at: Contents/Frameworks/TermSurf Profile Helper.app/Contents/MacOS/termsurf-profile
    let app_contents = exe
        .parent()
        .unwrap() // MacOS/
        .parent()
        .unwrap() // Contents/ (of helper app)
        .parent()
        .unwrap() // TermSurf Profile Helper.app/
        .parent()
        .unwrap() // Frameworks/
        .parent()
        .unwrap(); // Contents/ (of main app)
    let helper_path = app_contents
        .join("Frameworks")
        .join("TermSurf Helper.app")
        .join("Contents/MacOS/TermSurf Helper");
    println!(
        "Profile: Helper: {:?} (exists={})",
        helper_path,
        helper_path.exists()
    );

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let cache_path = std::path::PathBuf::from(home)
        .join(".config/termsurf/cef")
        .join(&args.profile);
    std::fs::create_dir_all(&cache_path).ok();
    println!("Profile: Cache: {:?}", cache_path);

    // 7. Initialize CEF
    // Issue 342, Experiment 1: Enable CEF debug logging to diagnose frame scheduling.
    let settings = cef::Settings {
        windowless_rendering_enabled: 1,
        no_sandbox: 1,
        log_severity: cef::LogSeverity::VERBOSE,
        log_file: cef::CefString::from("/tmp/cef-debug.log"),
        root_cache_path: cef::CefString::from(cache_path.to_str().unwrap()),
        browser_subprocess_path: cef::CefString::from(helper_path.to_str().unwrap()),
        persist_session_cookies: 1,
        ..Default::default()
    };

    let mut app = cef_handlers::create_app(Arc::clone(&profile_state));

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

    // 8. Create command listener and register with launcher (AFTER CEF init, BEFORE message loop)
    let command_listener = XpcListener::new_anonymous().expect("Failed to create command listener");
    let command_endpoint = command_listener
        .get_endpoint()
        .expect("Failed to get command endpoint");

    // Set up handler for create_browser commands from launcher
    let profile_state_for_handler = Arc::clone(&profile_state);
    let launcher_for_claim = Arc::new(launcher);
    // Issue 332, Experiment 2: Store launcher connection for unregister notification on exit
    let _ = LAUNCHER_CONNECTION.set(Arc::clone(&launcher_for_claim));
    let launcher_for_handler = Arc::clone(&launcher_for_claim);

    set_new_connection_handler(&command_listener, move |conn| {
        println!("Profile: New command connection from launcher");
        let conn = Arc::new(conn);
        let state = Arc::clone(&profile_state_for_handler);
        let state_for_event = Arc::clone(&state);
        let launcher = Arc::clone(&launcher_for_handler);

        set_event_handler(&*conn, move |event| match event {
            Ok(msg) => {
                let action = msg.get_string("action").unwrap_or_default();
                println!("Profile: Received command action: {}", action);

                if action == "create_browser" {
                    handle_create_browser(&msg, &state_for_event, &launcher);
                }
            }
            Err(e) => {
                eprintln!("Profile: Command connection error: {}", e);
            }
        });
        conn.resume();

        state.command_connections.lock().unwrap().push(conn);
    });
    command_listener.resume();
    println!("Profile: Command listener ready");

    // Register with launcher so it can forward subsequent create_browser commands
    let register_msg = XpcDictionary::new();
    register_msg.set_string("action", "register_profile");
    register_msg.set_string("profile", &args.profile);
    register_msg.set_endpoint("endpoint", command_endpoint);
    launcher_for_claim.send(&register_msg);
    println!("Profile: Registered with launcher as '{}'", args.profile);

    // 9. Install Ctrl+C handler for clean shutdown
    ctrlc::set_handler(move || {
        println!("Profile: Ctrl+C, setting quit flag...");
        QUIT_FLAG.store(true, std::sync::atomic::Ordering::Relaxed);
    })
    .expect("Failed to set Ctrl+C handler");

    // 10. Run CEF message loop with high-frequency polling
    // Issue 342, Experiment 5: Service the CFRunLoop instead of dead sleeping.
    // Issue 343, Experiment 3: Instrumented with microsecond timing.
    println!("Profile: Running message loop...");

    // Issue 345: Scroll simulation state (benchmark mode)
    let scroll_interval = Duration::from_millis(8); // ~125Hz, matching Apple mouse polling
    let mut last_scroll_time = Instant::now();
    let mut scroll_started = false;
    let mut scroll_direction: i32 = -1; // -1 = down, +1 = up
    let scroll_delta: i32 = 120; // standard scroll unit (one "notch")
    let direction_switch_every: u64 = 25; // reverse every 25 events (~200ms)
    let mut events_since_switch: u64 = 0;
    let mut scroll_event_count: u64 = 0;

    // Issue 345: Initialize benchmark stats and timing
    if args.benchmark {
        let _ = BENCHMARK_STATS.set(Mutex::new(FrameStats::new()));
    }
    let mut benchmark_start: Option<Instant> = None;
    let mut next_summary_time = Duration::from_secs(10);

    // Issue 346, Experiments 1 & 2: Event rate tracking
    let mut last_mouse_rate_time = Instant::now();
    let mut last_mouse_rate_count: u64 = 0;
    let mut last_cursor_rate_count: u64 = 0;

    while !QUIT_FLAG.load(std::sync::atomic::Ordering::Relaxed) {
        cef::do_message_loop_work();

        #[cfg(target_os = "macos")]
        cfrunloop::run_for(0.000);
        #[cfg(not(target_os = "macos"))]
        std::thread::sleep(std::time::Duration::from_millis(1));

        // Issue 345: Scroll simulation in benchmark mode
        if args.benchmark && PAGE_LOADED.load(std::sync::atomic::Ordering::Relaxed) {
            if !scroll_started {
                println!("[SCROLL] Page loaded, starting simulated scroll at ~125Hz");
                scroll_started = true;
                last_scroll_time = Instant::now();
                benchmark_start = Some(Instant::now());
            }

            let now = Instant::now();
            if now.duration_since(last_scroll_time) >= scroll_interval {
                last_scroll_time = now;

                // Reverse direction every N events to oscillate without hitting page boundaries
                events_since_switch += 1;
                if events_since_switch >= direction_switch_every {
                    scroll_direction *= -1;
                    events_since_switch = 0;
                }

                // Get first browser and send scroll event directly
                let browsers = profile_state.browsers.lock().unwrap();
                if let Some((_, bs)) = browsers.iter().next() {
                    if let Some(browser) = bs.browser.lock().unwrap().as_ref() {
                        use cef::{ImplBrowser, ImplBrowserHost};
                        if let Some(host) = browser.host() {
                            let mouse_event = cef::MouseEvent {
                                x: (args.width / 2) as i32,
                                y: (args.height / 2) as i32,
                                modifiers: 0,
                            };
                            host.send_mouse_wheel_event(
                                Some(&mouse_event),
                                0,
                                scroll_delta * scroll_direction,
                            );
                            scroll_event_count += 1;
                            if scroll_event_count % 125 == 0 {
                                println!("[SCROLL] {} events sent", scroll_event_count);
                            }
                        }
                    }
                }
            }
        }

        // Issue 346, Experiment 1: Log mouse event rate every second
        {
            let now = Instant::now();
            if now.duration_since(last_mouse_rate_time) >= Duration::from_secs(1) {
                let current_count = MOUSE_MOVE_COUNT.load(std::sync::atomic::Ordering::Relaxed);
                let events_this_second = current_count - last_mouse_rate_count;
                if events_this_second > 0 {
                    println!(
                        "[MOUSE-RATE] {} mouse_move events in last second",
                        events_this_second
                    );
                }
                last_mouse_rate_count = current_count;

                // Issue 346, Experiment 2: Cursor change rate
                let current_cursor = CURSOR_CHANGE_COUNT.load(std::sync::atomic::Ordering::Relaxed);
                let cursors_this_second = current_cursor - last_cursor_rate_count;
                if cursors_this_second > 0 {
                    println!(
                        "[CURSOR-RATE] {} cursor_change callbacks in last second",
                        cursors_this_second
                    );
                }
                last_cursor_rate_count = current_cursor;

                last_mouse_rate_time = now;
            }
        }

        // Issue 345: Benchmark periodic summary and auto-quit
        if let Some(start) = benchmark_start {
            let elapsed = start.elapsed();

            // Print summary every 10 seconds
            if elapsed >= next_summary_time {
                if let Some(stats) = BENCHMARK_STATS.get() {
                    stats.lock().unwrap().print_summary();
                }
                next_summary_time += Duration::from_secs(10);
            }

            // Auto-quit after benchmark duration
            if elapsed >= Duration::from_secs(args.benchmark_duration) {
                println!("[BENCHMARK-DONE] {} seconds elapsed", args.benchmark_duration);
                if let Some(stats) = BENCHMARK_STATS.get() {
                    stats.lock().unwrap().print_summary();
                }
                QUIT_FLAG.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }

    // 11. Shutdown
    println!("Profile: Shutting down...");

    // Unregister from launcher BEFORE cef::shutdown() (which is slow).
    // This lets the launcher spawn a fresh profile for the next benchmark trial
    // without waiting for CEF teardown to complete.
    if let Some(launcher) = LAUNCHER_CONNECTION.get() {
        let msg = termsurf_xpc::XpcDictionary::new();
        msg.set_string("action", "unregister_profile");
        msg.set_string("profile", &args.profile);
        launcher.send(&msg);
        println!("Profile: Sent unregister_profile to launcher");
    }

    cef::shutdown();
    println!("Profile: Done");
    // _loader dropped here, unloading CEF framework
}

/// Handle create_browser command from launcher (for additional browsers in this profile)
#[cfg(target_os = "macos")]
fn handle_create_browser(msg: &XpcDictionary, state: &Arc<ProfileState>, launcher: &XpcConnection) {
    let session_id = msg.get_string("session_id").unwrap_or_default();
    let url = msg.get_string("url").unwrap_or_default();
    let width = msg.get_i64("width") as u32;
    let height = msg.get_i64("height") as u32;

    println!(
        "Profile: create_browser session={}, url={}, size={}x{}",
        session_id, url, width, height
    );

    // Claim GUI endpoint from launcher
    let gui_endpoint = match claim_session_with_retry(launcher, &session_id) {
        Ok(ep) => ep,
        Err(e) => {
            eprintln!("Profile: Failed to claim session for browser: {}", e);
            return;
        }
    };

    // Store pending browser request and post task to process it on UI thread
    state.pending_browsers.lock().unwrap().push((
        session_id.to_string(),
        url.to_string(),
        gui_endpoint,
        width,
        height,
    ));

    // Post task to CEF UI thread to process pending browsers
    let mut task = cef_handlers::CreateBrowserTask::new(Arc::clone(state));
    cef::post_task(cef::ThreadId::UI, Some(&mut task));
}

/// Claim session with exponential backoff retry
fn claim_session_with_retry(launcher: &XpcConnection, session_id: &str) -> Result<XpcEndpoint> {
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
    use super::{BrowserState, ProfileState};
    use cef::rc::Rc;
    use cef::{
        wrap_app, wrap_browser_process_handler, wrap_client, wrap_context_menu_handler,
        wrap_display_handler, wrap_load_handler, wrap_render_handler, wrap_task,
        AcceleratedPaintInfo, App, Browser, BrowserProcessHandler, BrowserSettings, CefString,
        Client, ContextMenuHandler, ContextMenuParams, CursorInfo, CursorType, DisplayHandler,
        Frame, ImplApp, ImplBrowser, ImplBrowserHost, ImplBrowserProcessHandler, ImplClient,
        ImplCommandLine, ImplContextMenuHandler, ImplDisplayHandler, ImplFrame, ImplLoadHandler,
        ImplMenuModel, ImplRenderHandler, ImplTask, LoadHandler, MenuModel, PaintElementType, Rect,
        RenderHandler, ScreenInfo, Task, WindowInfo, WrapApp, WrapBrowserProcessHandler,
        WrapClient, WrapContextMenuHandler, WrapDisplayHandler, WrapLoadHandler, WrapRenderHandler,
        WrapTask,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use termsurf_xpc::*;

    // ====== Render Handler ======
    //
    // Sends IOSurface Mach ports to the GUI via XPC when CEF paints.
    // Each browser gets its own render handler with per-browser state.
    // Deduplicates by tracking the last IOSurface handle pointer.

    #[derive(Clone)]
    struct RenderHandlerInner {
        state: Arc<BrowserState>, // Per-browser state
        scale: f32,               // From profile-wide state
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
                    println!(
                        "[VIEW_RECT] session={} returning {}x{}",
                        self.inner.state.session_id,
                        rect.width,
                        rect.height
                    );
                }
            }

            fn screen_info(
                &self,
                _browser: Option<&mut Browser>,
                screen_info: Option<&mut ScreenInfo>,
            ) -> ::std::os::raw::c_int {
                if let Some(info) = screen_info {
                    info.device_scale_factor = self.inner.scale;
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

                // Issue 328: Set initial focus on first paint (browser is now ready)
                // Toggle unfocus/refocus to properly initialize focus state (from ts2)
                if !self.inner.state.initial_focus_set.load(Ordering::Relaxed) {
                    if let Some(browser) = self.inner.state.browser.lock().unwrap().as_ref() {
                        if let Some(host) = browser.host() {
                            println!("[FOCUS] First paint: toggling focus (0 then 1) for caret");
                            host.set_focus(0);
                            host.set_focus(1);
                            self.inner.state.initial_focus_set.store(true, Ordering::Relaxed);
                        }
                    }
                }

                // Send every frame — GUI needs to know when content changes,
                // not just when the IOSurface handle changes. (Issue 325)
                let handle = info.shared_texture_io_surface as *mut std::ffi::c_void;
                if handle.is_null() {
                    return;
                }

                // Issue 325, Experiment 2: Frame timing diagnostics
                let frame_id = crate::FRAME_COUNTER.fetch_add(1, crate::AtomicOrdering::Relaxed);
                let start = *crate::PROFILE_START_TIME.get_or_init(std::time::Instant::now);
                let tx_time_ms = start.elapsed().as_millis() as i64;
                // Issue 346, Experiment 3: [FRAME-TX] logging removed from hot path

                // Issue 345: Record frame for benchmark stats
                if let Some(stats) = crate::BENCHMARK_STATS.get() {
                    stats.lock().unwrap().record_frame();
                }

                // Create Mach port from IOSurface handle
                let port = termsurf_xpc::iosurface::create_mach_port(handle);
                if port == 0 {
                    eprintln!("Profile: create_mach_port failed");
                    return;
                }

                let width = info.extra.coded_size.width;
                let height = info.extra.coded_size.height;

                // Send to this browser's GUI connection via XPC
                let msg = XpcDictionary::new();
                msg.set_string("action", "display_surface");
                msg.set_mach_send("iosurface_port", port);
                msg.set_i64("width", width as i64);
                msg.set_i64("height", height as i64);
                msg.set_string("url", &self.inner.state.url.lock().unwrap());
                // Issue 325, Experiment 2: Include frame timing in XPC message
                msg.set_i64("frame_id", frame_id as i64);
                msg.set_i64("tx_time_ms", tx_time_ms);
                self.inner.state.gui.send(&msg);
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

    // ====== Display Handler ======
    //
    // Tracks URL changes for the control panel display.

    #[derive(Clone)]
    struct DisplayHandlerInner {
        state: Arc<BrowserState>,
    }

    wrap_display_handler! {
        pub struct ProfileDisplayHandler {
            inner: DisplayHandlerInner,
        }

        impl DisplayHandler {
            fn on_address_change(
                &self,
                _browser: Option<&mut Browser>,
                _frame: Option<&mut Frame>,
                url: Option<&CefString>,
            ) {
                if let Some(url) = url {
                    let url_str = url.to_string();
                    println!("Profile: URL changed to '{}'", url_str);
                    *self.inner.state.url.lock().unwrap() = url_str;
                }
            }

            // Issue 324: Cursor feedback
            fn on_cursor_change(
                &self,
                _browser: Option<&mut Browser>,
                _cursor: *mut u8,
                type_: CursorType,
                _custom_cursor_info: Option<&CursorInfo>,
            ) -> ::std::os::raw::c_int {
                // Issue 346, Experiment 2: Count cursor change callbacks
                crate::CURSOR_CHANGE_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                // Convert CursorType to i64 for XPC
                let cef_type: cef::sys::cef_cursor_type_t = type_.into();
                let cursor_type = cef_type as i64;
                // Issue 346, Experiment 3: cursor type logging removed from hot path

                // Send to GUI via XPC
                let msg = XpcDictionary::new();
                msg.set_string("action", "cursor_change");
                msg.set_i64("cursor_type", cursor_type);
                self.inner.state.gui.send(&msg);

                0 // Return value not used
            }
        }
    }

    // ====== Load Handler ======
    //
    // Issue 345: Detects page load completion for benchmark scroll start.

    #[derive(Clone)]
    struct LoadHandlerInner;

    wrap_load_handler! {
        pub struct ProfileLoadHandler {
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
                    crate::PAGE_LOADED.store(true, Ordering::Relaxed);
                }
            }
        }
    }

    // ====== Client ======

    wrap_client! {
        pub struct ProfileClient {
            render_handler: RenderHandler,
            context_menu_handler: ContextMenuHandler,
            display_handler: DisplayHandler,
            load_handler: LoadHandler,
        }

        impl Client {
            fn render_handler(&self) -> Option<RenderHandler> {
                Some(self.render_handler.clone())
            }

            fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
                Some(self.context_menu_handler.clone())
            }

            fn display_handler(&self) -> Option<DisplayHandler> {
                Some(self.display_handler.clone())
            }

            fn load_handler(&self) -> Option<LoadHandler> {
                Some(self.load_handler.clone())
            }
        }
    }

    // ====== Browser Process Handler ======
    //
    // Creates the initial browser in on_context_initialized, which fires during
    // run_message_loop() when CEF is fully ready.

    wrap_browser_process_handler! {
        pub struct ProfileBPH {
            state: Arc<ProfileState>,
        }

        impl BrowserProcessHandler {
            fn on_context_initialized(&self) {
                println!("Profile: CEF context initialized");

                // Take the initial browser info (only runs once)
                let info = self.state.initial_browser_info.lock().unwrap().take();

                if let Some(info) = info {
                    println!(
                        "Profile: Creating initial browser for session '{}', url='{}'",
                        info.session_id, info.url
                    );
                    create_browser_on_ui_thread(
                        &info.url,
                        &info.session_id,
                        info.gui_endpoint,
                        info.width,
                        info.height,
                        &self.state,
                    );
                } else {
                    println!("Profile: No initial browser info (unexpected)");
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
                    // Issue 342, Experiment 1: Enable Chromium internal logging.
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
        let handler = ProfileBPH::new(state);
        ProfileCefApp::new(handler)
    }

    // ====== Create Browser Task ======
    //
    // Task for creating browsers on the UI thread when requested via XPC.

    wrap_task! {
        pub struct CreateBrowserTask {
            state: Arc<ProfileState>,
        }

        impl Task {
            fn execute(&self) {
                // Process all pending browser creation requests
                let pending: Vec<_> = self.state.pending_browsers.lock().unwrap().drain(..).collect();

                for (session_id, url, gui_endpoint, width, height) in pending {
                    println!(
                        "Profile: Processing pending browser: session='{}', url='{}'",
                        session_id, url
                    );
                    create_browser_on_ui_thread(
                        &url,
                        &session_id,
                        gui_endpoint,
                        width,
                        height,
                        &self.state,
                    );
                }
            }
        }
    }

    /// Create a browser on the CEF UI thread.
    /// Used by both initial browser creation (on_context_initialized) and
    /// subsequent browser creation (create_browser command from launcher).
    pub fn create_browser_on_ui_thread(
        url: &str,
        session_id: &str,
        gui_endpoint: XpcEndpoint,
        width: u32,
        height: u32,
        state: &Arc<ProfileState>,
    ) {
        use std::sync::atomic::AtomicPtr;

        // 1. Connect to GUI (don't resume yet)
        let gui = match XpcConnection::from_endpoint(gui_endpoint) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                eprintln!("Profile: Failed to connect to GUI: {}", e);
                return;
            }
        };

        // Issue 330, Experiment 3: Track active connections by ID for idempotent cleanup
        let conn_id = crate::CONNECTION_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        {
            let mut conns = crate::active_connections().lock().unwrap();
            conns.insert(conn_id);
            println!(
                "[CONN-{}] GUI connection established (active: {:?})",
                conn_id, conns
            );
        }

        // 2. Create deferred state wrapper (will be populated after browser creation)
        let deferred_state: Arc<Mutex<Option<Arc<BrowserState>>>> = Arc::new(Mutex::new(None));

        // 3. Set event handler BEFORE resume
        let deferred_for_handler = Arc::clone(&deferred_state);
        let scale_for_handler = state.scale;
        let conn_id_for_handler = conn_id; // Issue 330, Experiment 3: Capture for idempotent error handler
        set_event_handler(&*gui, move |event| {
            match event {
                Ok(msg) => {
                    let action = msg.get_string("action").unwrap_or_default();

                    match action.as_str() {
                        "resize_browser" => {
                            // Get state from deferred wrapper
                            let state_guard = deferred_for_handler.lock().unwrap();
                            let Some(bs) = state_guard.as_ref() else {
                                println!("Profile: resize_browser ignored (state not ready)");
                                return;
                            };

                            // Check for physical dimensions first (new protocol)
                            let (width, height) = if msg.get_i64("physical_width") != 0 {
                                let physical_w = msg.get_i64("physical_width") as u32;
                                let physical_h = msg.get_i64("physical_height") as u32;
                                let scale_str = msg.get_string("scale").unwrap_or_default();
                                let scale: f32 = scale_str.parse().unwrap_or(scale_for_handler);
                                // Convert to logical, rounding up to ensure texture >= viewport
                                let logical_w = (physical_w as f32 / scale).ceil() as u32;
                                let logical_h = (physical_h as f32 / scale).ceil() as u32;
                                println!(
                                    "[RESIZE-RECV] physical={}x{} scale={:.2} -> logical={}x{} (ceil) (prev={}x{})",
                                    physical_w, physical_h, scale,
                                    logical_w, logical_h,
                                    bs.width.load(Ordering::Relaxed),
                                    bs.height.load(Ordering::Relaxed)
                                );
                                (logical_w, logical_h)
                            } else {
                                // Fallback to legacy logical dimensions
                                let width = msg.get_i64("width") as u32;
                                let height = msg.get_i64("height") as u32;
                                println!(
                                    "[RESIZE-RECV] logical={}x{} scale={:.2} -> physical={}x{} (prev={}x{})",
                                    width, height, scale_for_handler,
                                    (width as f32 * scale_for_handler) as u32,
                                    (height as f32 * scale_for_handler) as u32,
                                    bs.width.load(Ordering::Relaxed),
                                    bs.height.load(Ordering::Relaxed)
                                );
                                (width, height)
                            };

                            let bs = Arc::clone(bs);
                            drop(state_guard); // Release lock before post_task

                            let mut task = ResizeBrowserTask::new(bs, width, height);
                            cef::post_task(cef::ThreadId::UI, Some(&mut task));
                        }
                        "key_event" => {
                            // Get state from deferred wrapper
                            let state_guard = deferred_for_handler.lock().unwrap();
                            let Some(bs) = state_guard.as_ref() else {
                                println!("Profile: key_event ignored (state not ready)");
                                return;
                            };

                            let key_is_down = msg.get_bool("key_is_down");
                            let key_type = msg.get_string("key_type").unwrap_or_default();
                            let raw_code = msg.get_i64("raw_code") as u32;
                            let char_code = msg.get_i64("char_code") as u32;

                            let shift = msg.get_bool("shift");
                            let ctrl = msg.get_bool("ctrl");
                            let alt = msg.get_bool("alt");
                            let meta = msg.get_bool("meta");

                            let bs = Arc::clone(bs);
                            drop(state_guard); // Release lock before post_task

                            // Post to CEF UI thread
                            let mut task = KeyEventTask::new(
                                bs,
                                key_is_down,
                                key_type,
                                raw_code,
                                char_code,
                                shift,
                                ctrl,
                                alt,
                                meta,
                            );
                            cef::post_task(cef::ThreadId::UI, Some(&mut task));
                        }
                        "paste_text" => {
                            // Get state from deferred wrapper
                            let state_guard = deferred_for_handler.lock().unwrap();
                            let Some(bs) = state_guard.as_ref() else {
                                println!("Profile: paste_text ignored (state not ready)");
                                return;
                            };

                            let text = msg.get_string("text").unwrap_or_default();
                            println!("[CLIPBOARD] Received paste_text: {} chars", text.len());

                            let bs = Arc::clone(bs);
                            let text = text.to_string();
                            drop(state_guard); // Release lock before post_task

                            // Post to CEF UI thread
                            let mut task = PasteTextTask::new(bs, text);
                            cef::post_task(cef::ThreadId::UI, Some(&mut task));
                        }
                        "do_copy" => {
                            // Issue 318, experiment 1: Copy selection to clipboard via CEF
                            let state_guard = deferred_for_handler.lock().unwrap();
                            let Some(bs) = state_guard.as_ref() else {
                                println!("Profile: do_copy ignored (state not ready)");
                                return;
                            };

                            println!("[CLIPBOARD] Received do_copy command");

                            let bs = Arc::clone(bs);
                            drop(state_guard); // Release lock before post_task

                            // Post to CEF UI thread
                            let mut task = CopyTask::new(bs);
                            cef::post_task(cef::ThreadId::UI, Some(&mut task));
                        }
                        "do_cut" => {
                            // Issue 318, experiment 2: Cut selection to clipboard via CEF
                            let state_guard = deferred_for_handler.lock().unwrap();
                            let Some(bs) = state_guard.as_ref() else {
                                println!("Profile: do_cut ignored (state not ready)");
                                return;
                            };

                            println!("[CLIPBOARD] Received do_cut command");

                            let bs = Arc::clone(bs);
                            drop(state_guard); // Release lock before post_task

                            // Post to CEF UI thread
                            let mut task = CutTask::new(bs);
                            cef::post_task(cef::ThreadId::UI, Some(&mut task));
                        }
                        "do_select_all" => {
                            // Issue 318, experiment 3: Select all via CEF
                            let state_guard = deferred_for_handler.lock().unwrap();
                            let Some(bs) = state_guard.as_ref() else {
                                println!("Profile: do_select_all ignored (state not ready)");
                                return;
                            };

                            println!("[CLIPBOARD] Received do_select_all command");

                            let bs = Arc::clone(bs);
                            drop(state_guard); // Release lock before post_task

                            // Post to CEF UI thread
                            let mut task = SelectAllTask::new(bs);
                            cef::post_task(cef::ThreadId::UI, Some(&mut task));
                        }
                        "go_back" => {
                            // Issue 335: Navigate back in browser history
                            let state_guard = deferred_for_handler.lock().unwrap();
                            let Some(bs) = state_guard.as_ref() else {
                                println!("Profile: go_back ignored (state not ready)");
                                return;
                            };

                            println!("[NAV] Received go_back command");

                            let bs = Arc::clone(bs);
                            drop(state_guard);

                            let mut task = GoBackTask::new(bs);
                            cef::post_task(cef::ThreadId::UI, Some(&mut task));
                        }
                        "go_forward" => {
                            // Issue 335: Navigate forward in browser history
                            let state_guard = deferred_for_handler.lock().unwrap();
                            let Some(bs) = state_guard.as_ref() else {
                                println!("Profile: go_forward ignored (state not ready)");
                                return;
                            };

                            println!("[NAV] Received go_forward command");

                            let bs = Arc::clone(bs);
                            drop(state_guard);

                            let mut task = GoForwardTask::new(bs);
                            cef::post_task(cef::ThreadId::UI, Some(&mut task));
                        }
                        "reload" => {
                            // Issue 337: Reload page
                            let state_guard = deferred_for_handler.lock().unwrap();
                            let Some(bs) = state_guard.as_ref() else {
                                println!("Profile: reload ignored (state not ready)");
                                return;
                            };

                            println!("[NAV] Received reload command");

                            let bs = Arc::clone(bs);
                            drop(state_guard);

                            let mut task = ReloadTask::new(bs);
                            cef::post_task(cef::ThreadId::UI, Some(&mut task));
                        }
                        "reload_ignore_cache" => {
                            // Issue 337: Hard reload (bypass cache)
                            let state_guard = deferred_for_handler.lock().unwrap();
                            let Some(bs) = state_guard.as_ref() else {
                                println!("Profile: reload_ignore_cache ignored (state not ready)");
                                return;
                            };

                            println!("[NAV] Received reload_ignore_cache command");

                            let bs = Arc::clone(bs);
                            drop(state_guard);

                            let mut task = ReloadIgnoreCacheTask::new(bs);
                            cef::post_task(cef::ThreadId::UI, Some(&mut task));
                        }
                        "mouse_move" => {
                            // Issue 346, Experiment 1: Count mouse_move events
                            crate::MOUSE_MOVE_COUNT
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                            let state_guard = deferred_for_handler.lock().unwrap();
                            let Some(bs) = state_guard.as_ref() else {
                                return;
                            };

                            let x = msg.get_i64("x") as i32;
                            let y = msg.get_i64("y") as i32;
                            let modifiers = msg.get_i64("modifiers") as u32;

                            let bs = Arc::clone(bs);
                            drop(state_guard);

                            let mut task = MouseMoveTask::new(bs, x, y, modifiers);
                            cef::post_task(cef::ThreadId::UI, Some(&mut task));
                        }
                        "mouse_click" => {
                            let state_guard = deferred_for_handler.lock().unwrap();
                            let Some(bs) = state_guard.as_ref() else {
                                return;
                            };

                            let x = msg.get_i64("x") as i32;
                            let y = msg.get_i64("y") as i32;
                            let button = msg.get_i64("button") as u32;
                            let is_up = msg.get_bool("is_up");
                            let click_count = msg.get_i64("click_count") as i32;
                            let modifiers = msg.get_i64("modifiers") as u32;

                            let bs = Arc::clone(bs);
                            drop(state_guard);

                            let mut task = MouseClickTask::new(
                                bs,
                                x,
                                y,
                                button,
                                is_up,
                                click_count,
                                modifiers,
                            );
                            cef::post_task(cef::ThreadId::UI, Some(&mut task));
                        }
                        "mouse_wheel" => {
                            let state_guard = deferred_for_handler.lock().unwrap();
                            let Some(bs) = state_guard.as_ref() else {
                                return;
                            };

                            let x = msg.get_i64("x") as i32;
                            let y = msg.get_i64("y") as i32;
                            let delta_x = msg.get_i64("delta_x") as i32;
                            let delta_y = msg.get_i64("delta_y") as i32;
                            let modifiers = msg.get_i64("modifiers") as u32;

                            let bs = Arc::clone(bs);
                            drop(state_guard);

                            let mut task =
                                MouseWheelTask::new(bs, x, y, delta_x, delta_y, modifiers);
                            cef::post_task(cef::ThreadId::UI, Some(&mut task));
                        }
                        "focus" => {
                            // Issue 329: Focus/unfocus browser for caret control
                            let state_guard = deferred_for_handler.lock().unwrap();
                            let Some(bs) = state_guard.as_ref() else {
                                println!("Profile: focus ignored (state not ready)");
                                return;
                            };

                            let focused = msg.get_bool("focused");
                            println!("[FOCUS] Received focus command: {}", focused);

                            let bs = Arc::clone(bs);
                            drop(state_guard);

                            let mut task = FocusTask::new(bs, focused);
                            cef::post_task(cef::ThreadId::UI, Some(&mut task));
                        }
                        // Issue 319, experiment 2: Log unhandled actions
                        other => {
                            println!("[XPC-RECV] Unhandled action: {:?}", other);
                        }
                    }
                }
                Err(e) => {
                    // Issue 330, Experiment 3: Idempotent disconnect handling
                    // Only act if this connection is still in the active set
                    match e {
                        XpcError::ConnectionInterrupted | XpcError::ConnectionInvalid => {
                            let mut conns = crate::active_connections().lock().unwrap();
                            if conns.remove(&conn_id_for_handler) {
                                // This connection was still active, now it's gone
                                println!(
                                    "[CONN-{}] GUI disconnected (remaining: {:?})",
                                    conn_id_for_handler, conns
                                );
                                if conns.is_empty() {
                                    println!(
                                        "[CONN-{}] No more GUI connections, exiting gracefully",
                                        conn_id_for_handler
                                    );
                                    drop(conns); // Release lock before sending

                                    // Issue 332, Experiment 2: Notify launcher to unregister this profile
                                    if let Some(launcher) = crate::LAUNCHER_CONNECTION.get() {
                                        if let Some(state) = crate::PROFILE_STATE.get() {
                                            let msg = XpcDictionary::new();
                                            msg.set_string("action", "unregister_profile");
                                            msg.set_string("profile", &state.profile);
                                            launcher.send(&msg);
                                            println!(
                                                "[CONN-{}] Sent unregister_profile to launcher",
                                                conn_id_for_handler
                                            );
                                        }
                                    }

                                    crate::QUIT_FLAG
                                        .store(true, std::sync::atomic::Ordering::Relaxed);
                                }
                            } else {
                                // Already disconnected - ignore duplicate error
                                println!(
                                    "[CONN-{}] Ignoring duplicate disconnect (already removed)",
                                    conn_id_for_handler
                                );
                            }
                        }
                        _ => {
                            eprintln!("[CONN-{}] GUI connection error: {}", conn_id_for_handler, e)
                        }
                    }
                }
            }
        });

        // 4. NOW resume the connection (handler is ready)
        // Issue 319, experiment 2: Log that event handler is registered
        println!("[XPC] Event handler registered on GUI connection");
        gui.resume();

        // 5. Create per-browser state
        let browser_state = Arc::new(BrowserState {
            session_id: session_id.to_string(),
            gui: Arc::clone(&gui),
            width: std::sync::atomic::AtomicU32::new(width),
            height: std::sync::atomic::AtomicU32::new(height),
            browser: Mutex::new(None),
            url: Mutex::new(url.to_string()),
            // Issue 328: Initialize to false; will be set on first paint
            initial_focus_set: AtomicBool::new(false),
        });

        // Create render handler with browser-specific state
        let render_inner = RenderHandlerInner {
            state: Arc::clone(&browser_state),
            scale: state.scale,
        };
        let render_handler = ProfileRenderHandler::new(render_inner);

        // Create display handler to track URL changes
        let display_inner = DisplayHandlerInner {
            state: Arc::clone(&browser_state),
        };
        let display_handler = ProfileDisplayHandler::new(display_inner);

        let context_menu_handler = ProfileContextMenuHandler::new(ContextMenuInner);
        let load_handler = ProfileLoadHandler::new(LoadHandlerInner);
        let mut client = ProfileClient::new(
            render_handler,
            context_menu_handler,
            display_handler,
            load_handler,
        );

        let window_info = WindowInfo {
            windowless_rendering_enabled: 1,
            shared_texture_enabled: 1,
            ..Default::default()
        };

        let browser_settings = BrowserSettings {
            windowless_frame_rate: 60,
            background_color: 0xFFFFFFFF, // Opaque white (issue 336)
            ..Default::default()
        };

        let url_cef: cef::CefString = url.into();

        let browser = cef::browser_host_create_browser_sync(
            Some(&window_info),
            Some(&mut client),
            Some(&url_cef),
            Some(&browser_settings),
            None, // extra_info
            None, // request_context (uses global with our root_cache_path)
        );

        match browser {
            Some(b) => {
                let browser_id = b.identifier();
                println!(
                    "Profile: Browser {} created for '{}' (session='{}')",
                    browser_id, url, session_id
                );

                // Issue 328: Removed early set_focus(1) call. Focus is now set on first
                // paint in on_accelerated_paint with unfocus/refocus toggle (from ts2).

                // Store browser reference for resize operations
                *browser_state.browser.lock().unwrap() = Some(b);

                // 6. Populate deferred state (handler can now access it)
                *deferred_state.lock().unwrap() = Some(Arc::clone(&browser_state));

                // Store browser state by ID
                state
                    .browsers
                    .lock()
                    .unwrap()
                    .insert(browser_id, browser_state);
            }
            None => eprintln!("Profile: Failed to create browser for '{}'", url),
        }
    }

    // ====== Resize Browser Task ======
    //
    // Task for resizing browsers on the UI thread when resize commands arrive via XPC.

    wrap_task! {
        pub struct ResizeBrowserTask {
            state: Arc<BrowserState>,
            width: u32,
            height: u32,
        }

        impl Task {
            fn execute(&self) {
                resize_browser_on_ui_thread(&self.state, self.width, self.height);
            }
        }
    }

    /// Resize a browser (called on CEF UI thread via post_task)
    fn resize_browser_on_ui_thread(state: &BrowserState, width: u32, height: u32) {
        // Update stored dimensions (used by get_view_rect)
        state.width.store(width, Ordering::Relaxed);
        state.height.store(height, Ordering::Relaxed);

        // Notify CEF of size change
        if let Some(ref browser) = *state.browser.lock().unwrap() {
            if let Some(host) = browser.host() {
                println!("Profile: was_resized {}x{}", width, height);
                host.was_resized();
                // PaintElementType::default() is PET_VIEW (0)
                host.invalidate(PaintElementType::default());
            }
        }
    }

    // ====== Focus Task ======
    //
    // Issue 329: Task for setting browser focus state on the UI thread.

    wrap_task! {
        pub struct FocusTask {
            state: Arc<BrowserState>,
            focused: bool,
        }

        impl Task {
            fn execute(&self) {
                if let Some(ref browser) = *self.state.browser.lock().unwrap() {
                    if let Some(host) = browser.host() {
                        println!("[FOCUS] Setting focus to {}", self.focused);
                        host.set_focus(if self.focused { 1 } else { 0 });
                    }
                }
            }
        }
    }

    // ====== Key Event Task ======
    //
    // Task for sending key events to CEF on the UI thread.

    wrap_task! {
        pub struct KeyEventTask {
            state: Arc<BrowserState>,
            key_is_down: bool,
            key_type: String,
            raw_code: u32,
            char_code: u32,
            shift: bool,
            ctrl: bool,
            alt: bool,
            meta: bool,
        }

        impl Task {
            fn execute(&self) {
                send_key_event_to_cef(
                    &self.state,
                    self.key_is_down,
                    &self.key_type,
                    self.raw_code,
                    self.char_code,
                    self.shift,
                    self.ctrl,
                    self.alt,
                    self.meta,
                );
            }
        }
    }

    // ====== Paste Text Task ======
    //
    // Task for pasting clipboard text into the browser via JavaScript injection.
    // This bypasses macOS clipboard restrictions by receiving text from GUI via XPC.

    wrap_task! {
        pub struct PasteTextTask {
            state: Arc<BrowserState>,
            text: String,
        }

        impl Task {
            fn execute(&self) {
                paste_text_to_browser(&self.state, &self.text);
            }
        }
    }

    // ====== Copy Task ======
    //
    // Task for copying selected text to clipboard via CEF's native frame.copy().
    // Issue 318, experiment 1: Unlike paste (which requires proxying), copy writes
    // to the clipboard which may work from a background process.

    wrap_task! {
        pub struct CopyTask {
            state: Arc<BrowserState>,
        }

        impl Task {
            fn execute(&self) {
                if let Some(browser) = self.state.browser.lock().unwrap().as_ref() {
                    if let Some(frame) = browser.main_frame() {
                        println!("[CLIPBOARD] Calling frame.copy()");
                        frame.copy();
                    } else {
                        println!("[CLIPBOARD] CopyTask: no main frame");
                    }
                } else {
                    println!("[CLIPBOARD] CopyTask: no browser");
                }
            }
        }
    }

    // ====== Cut Task ======
    //
    // Task for cutting selected text to clipboard via CEF's native frame.cut().
    // Issue 318, experiment 2: Like copy, but also deletes the selection.

    wrap_task! {
        pub struct CutTask {
            state: Arc<BrowserState>,
        }

        impl Task {
            fn execute(&self) {
                if let Some(browser) = self.state.browser.lock().unwrap().as_ref() {
                    if let Some(frame) = browser.main_frame() {
                        println!("[CLIPBOARD] Calling frame.cut()");
                        frame.cut();
                    } else {
                        println!("[CLIPBOARD] CutTask: no main frame");
                    }
                } else {
                    println!("[CLIPBOARD] CutTask: no browser");
                }
            }
        }
    }

    // ====== Select All Task ======
    //
    // Task for selecting all content via CEF's native frame.select_all().
    // Issue 318, experiment 3: Doesn't touch clipboard, just modifies selection.

    wrap_task! {
        pub struct SelectAllTask {
            state: Arc<BrowserState>,
        }

        impl Task {
            fn execute(&self) {
                if let Some(browser) = self.state.browser.lock().unwrap().as_ref() {
                    if let Some(frame) = browser.main_frame() {
                        println!("[CLIPBOARD] Calling frame.select_all()");
                        frame.select_all();
                    } else {
                        println!("[CLIPBOARD] SelectAllTask: no main frame");
                    }
                } else {
                    println!("[CLIPBOARD] SelectAllTask: no browser");
                }
            }
        }
    }

    // ====== Go Back Task ======
    //
    // Task for navigating back in browser history via CEF's browser.go_back().
    // Issue 335: Browser navigation.

    wrap_task! {
        pub struct GoBackTask {
            state: Arc<BrowserState>,
        }

        impl Task {
            fn execute(&self) {
                if let Some(browser) = self.state.browser.lock().unwrap().as_ref() {
                    println!("[NAV] Calling browser.go_back()");
                    browser.go_back();
                } else {
                    println!("[NAV] GoBackTask: no browser");
                }
            }
        }
    }

    // ====== Go Forward Task ======
    //
    // Task for navigating forward in browser history via CEF's browser.go_forward().
    // Issue 335: Browser navigation.

    wrap_task! {
        pub struct GoForwardTask {
            state: Arc<BrowserState>,
        }

        impl Task {
            fn execute(&self) {
                if let Some(browser) = self.state.browser.lock().unwrap().as_ref() {
                    println!("[NAV] Calling browser.go_forward()");
                    browser.go_forward();
                } else {
                    println!("[NAV] GoForwardTask: no browser");
                }
            }
        }
    }

    // ====== Reload Task ======
    //
    // Task for reloading the page via CEF's browser.reload().
    // Issue 337: Browser refresh.

    wrap_task! {
        pub struct ReloadTask {
            state: Arc<BrowserState>,
        }

        impl Task {
            fn execute(&self) {
                if let Some(browser) = self.state.browser.lock().unwrap().as_ref() {
                    println!("[NAV] Calling browser.reload()");
                    browser.reload();
                } else {
                    println!("[NAV] ReloadTask: no browser");
                }
            }
        }
    }

    // ====== Reload Ignore Cache Task ======
    //
    // Task for hard reload via CEF's browser.reload_ignore_cache().
    // Issue 337: Browser refresh (bypass cache).

    wrap_task! {
        pub struct ReloadIgnoreCacheTask {
            state: Arc<BrowserState>,
        }

        impl Task {
            fn execute(&self) {
                if let Some(browser) = self.state.browser.lock().unwrap().as_ref() {
                    println!("[NAV] Calling browser.reload_ignore_cache()");
                    browser.reload_ignore_cache();
                } else {
                    println!("[NAV] ReloadIgnoreCacheTask: no browser");
                }
            }
        }
    }

    // ====== Mouse Move Task ======
    //
    // Task for sending mouse move events to CEF on the UI thread.
    // Issue 319, experiment 3: Deep task execution logging.

    wrap_task! {
        pub struct MouseMoveTask {
            state: Arc<BrowserState>,
            x: i32,
            y: i32,
            modifiers: u32,
        }

        impl Task {
            fn execute(&self) {
                let browser_guard = self.state.browser.lock().unwrap();
                let Some(browser) = browser_guard.as_ref() else {
                    return;
                };
                let Some(host) = browser.host() else {
                    return;
                };

                let mouse_event = cef::MouseEvent {
                    x: self.x,
                    y: self.y,
                    modifiers: self.modifiers,
                };
                // mouse_leave = 0 (mouse is over the view)
                host.send_mouse_move_event(Some(&mouse_event), 0);
            }
        }
    }

    // ====== Mouse Click Task ======
    //
    // Task for sending mouse click events to CEF on the UI thread.
    // Issue 319, experiment 3: Deep task execution logging.

    wrap_task! {
        pub struct MouseClickTask {
            state: Arc<BrowserState>,
            x: i32,
            y: i32,
            button: u32,
            is_up: bool,
            click_count: i32,
            modifiers: u32,
        }

        impl Task {
            fn execute(&self) {
                let browser_guard = self.state.browser.lock().unwrap();
                let Some(browser) = browser_guard.as_ref() else {
                    return;
                };
                let Some(host) = browser.host() else {
                    return;
                };

                let mouse_event = cef::MouseEvent {
                    x: self.x,
                    y: self.y,
                    modifiers: self.modifiers,
                };
                // button: 0=left, 1=middle, 2=right
                let button_type = match self.button {
                    0 => cef::MouseButtonType::LEFT,
                    1 => cef::MouseButtonType::MIDDLE,
                    2 => cef::MouseButtonType::RIGHT,
                    _ => cef::MouseButtonType::LEFT,
                };
                let mouse_up = if self.is_up { 1 } else { 0 };
                host.send_mouse_click_event(
                    Some(&mouse_event),
                    button_type,
                    mouse_up,
                    self.click_count,
                );
            }
        }
    }

    // Issue 321, experiment 1: Scroll support.

    wrap_task! {
        pub struct MouseWheelTask {
            state: Arc<BrowserState>,
            x: i32,
            y: i32,
            delta_x: i32,
            delta_y: i32,
            modifiers: u32,
        }

        impl Task {
            fn execute(&self) {
                let browser_guard = self.state.browser.lock().unwrap();
                let Some(browser) = browser_guard.as_ref() else {
                    return;
                };
                let Some(host) = browser.host() else {
                    return;
                };

                let mouse_event = cef::MouseEvent {
                    x: self.x,
                    y: self.y,
                    modifiers: self.modifiers,
                };
                host.send_mouse_wheel_event(
                    Some(&mouse_event),
                    self.delta_x,
                    self.delta_y,
                );
            }
        }
    }

    /// Paste text into the browser by executing JavaScript (called on CEF UI thread)
    fn paste_text_to_browser(state: &BrowserState, text: &str) {
        let browser = match state.browser.lock().unwrap().as_ref() {
            Some(b) => b.clone(),
            None => {
                println!("[CLIPBOARD] paste_text_to_browser: no browser");
                return;
            }
        };

        let frame = match browser.main_frame() {
            Some(f) => f,
            None => {
                println!("[CLIPBOARD] paste_text_to_browser: no main frame");
                return;
            }
        };

        // Escape text for JavaScript string literal
        let escaped = text
            .replace('\\', "\\\\")
            .replace('\'', "\\'")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t");

        // Use execCommand to insert text at the current cursor position
        // This works in input fields, textareas, and contenteditable elements
        let js = format!("document.execCommand('insertText', false, '{}');", escaped);

        println!("[CLIPBOARD] Executing JS to paste {} chars", text.len());
        let js_cef: CefString = js.as_str().into();
        frame.execute_java_script(Some(&js_cef), None, 0);
    }

    // CEF event flags (same values as CEF uses internally)
    const EVENTFLAG_SHIFT_DOWN: u32 = 1 << 1;
    const EVENTFLAG_CONTROL_DOWN: u32 = 1 << 2;
    const EVENTFLAG_ALT_DOWN: u32 = 1 << 3;
    const EVENTFLAG_COMMAND_DOWN: u32 = 1 << 7;

    /// Send a key event to CEF (called on CEF UI thread via post_task)
    fn send_key_event_to_cef(
        state: &BrowserState,
        key_is_down: bool,
        key_type: &str,
        raw_code: u32,
        char_code: u32,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) {
        use cef::{KeyEvent, KeyEventType};

        // Detect clipboard shortcuts for diagnostic logging
        let is_potential_paste = meta && (char_code == 'v' as u32 || char_code == 'V' as u32);
        let is_potential_copy = meta && (char_code == 'c' as u32 || char_code == 'C' as u32);
        let is_clipboard_op = is_potential_paste || is_potential_copy;

        if is_clipboard_op {
            println!(
                "[CLIPBOARD-DEBUG] {} received: key_is_down={}, raw_code={:#x}, char_code={} ('{}'), modifiers=[shift={}, ctrl={}, alt={}, meta={}]",
                if is_potential_paste { "Cmd+V" } else { "Cmd+C" },
                key_is_down,
                raw_code,
                char_code,
                char::from_u32(char_code).unwrap_or('?'),
                shift, ctrl, alt, meta
            );
        }

        let browser = match state.browser.lock().unwrap().as_ref() {
            Some(b) => b.clone(),
            None => {
                if is_clipboard_op {
                    println!("[CLIPBOARD-DEBUG] ERROR: No browser instance!");
                }
                return;
            }
        };
        let host = match browser.host() {
            Some(h) => h,
            None => {
                if is_clipboard_op {
                    println!("[CLIPBOARD-DEBUG] ERROR: No browser host!");
                }
                return;
            }
        };

        // Build CEF modifiers
        let mut modifiers = 0u32;
        if shift {
            modifiers |= EVENTFLAG_SHIFT_DOWN;
        }
        if ctrl {
            modifiers |= EVENTFLAG_CONTROL_DOWN;
        }
        if alt {
            modifiers |= EVENTFLAG_ALT_DOWN;
        }
        if meta {
            modifiers |= EVENTFLAG_COMMAND_DOWN;
        }

        // Convert to Windows VK code
        let windows_vk = macos_keycode_to_windows_vk(raw_code);
        let native_code = raw_code as i32;

        if is_clipboard_op {
            println!(
                "[CLIPBOARD-DEBUG] CEF event: windows_vk={:#x} (expected V={:#x}, C={:#x}), native={:#x}, modifiers={:#x} (COMMAND_DOWN={:#x})",
                windows_vk,
                0x56, // VK_V
                0x43, // VK_C
                native_code,
                modifiers,
                EVENTFLAG_COMMAND_DOWN
            );
        }

        // Determine if this is an action key (skip KEYUP to avoid double actions)
        let is_action_key = matches!(
            key_type,
            "left" | "right" | "up" | "down" | "home" | "end" | "pageup" | "pagedown" | "insert"
        ) || (key_type == "char"
            && matches!(
                char_code,
                0x08 | 0x7f | 0x09 | 0x1b | 0x0d | 0x20 // BS, DEL, TAB, ESC, ENTER, SPACE
            ));

        if is_action_key && !key_is_down {
            return; // Skip KEYUP for action keys
        }

        // Send KEYDOWN or KEYUP
        let event_type = if key_is_down {
            KeyEventType::KEYDOWN
        } else {
            KeyEventType::KEYUP
        };

        // For clipboard operations, ensure browser has focus before sending key
        if is_clipboard_op && key_is_down {
            println!("[CLIPBOARD-DEBUG] Calling send_focus_event(true) before clipboard operation");
            host.set_focus(1); // 1 = focused
        }

        let key_event = KeyEvent {
            size: std::mem::size_of::<KeyEvent>(),
            type_: event_type,
            modifiers,
            windows_key_code: windows_vk,
            native_key_code: native_code,
            is_system_key: 0,
            character: 0,
            unmodified_character: 0,
            focus_on_editable_field: 0,
        };
        host.send_key_event(Some(&key_event));

        if is_clipboard_op {
            println!(
                "[CLIPBOARD-DEBUG] send_key_event called for {:?}",
                event_type
            );
        }

        // For key-down of printable characters, also send CHAR event
        if key_is_down && key_type == "char" && char_code > 0 && char_code < 0x10000 {
            let char_event = KeyEvent {
                size: std::mem::size_of::<KeyEvent>(),
                type_: KeyEventType::CHAR,
                modifiers,
                windows_key_code: char_code as i32,
                native_key_code: 0,
                is_system_key: 0,
                character: char_code as u16,
                unmodified_character: char_code as u16,
                focus_on_editable_field: 0,
            };
            host.send_key_event(Some(&char_event));

            if is_clipboard_op {
                println!("[CLIPBOARD-DEBUG] CHAR event also sent");
            }
        }

        println!(
            "Profile: key_event type={} vk={:#x} native={:#x} char={} down={}",
            key_type, windows_vk, native_code, char_code, key_is_down
        );
    }

    /// Convert macOS keycode to Windows virtual key code
    fn macos_keycode_to_windows_vk(code: u32) -> i32 {
        match code {
            // Letters
            0x00 => 0x41, // A
            0x0B => 0x42, // B
            0x08 => 0x43, // C
            0x02 => 0x44, // D
            0x0E => 0x45, // E
            0x03 => 0x46, // F
            0x05 => 0x47, // G
            0x04 => 0x48, // H
            0x22 => 0x49, // I
            0x26 => 0x4A, // J
            0x28 => 0x4B, // K
            0x25 => 0x4C, // L
            0x2E => 0x4D, // M
            0x2D => 0x4E, // N
            0x1F => 0x4F, // O
            0x23 => 0x50, // P
            0x0C => 0x51, // Q
            0x0F => 0x52, // R
            0x01 => 0x53, // S
            0x11 => 0x54, // T
            0x20 => 0x55, // U
            0x09 => 0x56, // V
            0x0D => 0x57, // W
            0x07 => 0x58, // X
            0x10 => 0x59, // Y
            0x06 => 0x5A, // Z
            // Numbers
            0x1D => 0x30, // 0
            0x12 => 0x31, // 1
            0x13 => 0x32, // 2
            0x14 => 0x33, // 3
            0x15 => 0x34, // 4
            0x17 => 0x35, // 5
            0x16 => 0x36, // 6
            0x1A => 0x37, // 7
            0x1C => 0x38, // 8
            0x19 => 0x39, // 9
            // Special keys
            0x24 => 0x0D, // Return -> VK_RETURN
            0x30 => 0x09, // Tab -> VK_TAB
            0x31 => 0x20, // Space -> VK_SPACE
            0x33 => 0x08, // Delete (backspace) -> VK_BACK
            0x35 => 0x1B, // Escape -> VK_ESCAPE
            0x75 => 0x2E, // Forward Delete -> VK_DELETE
            // Arrow keys
            0x7B => 0x25, // Left -> VK_LEFT
            0x7C => 0x27, // Right -> VK_RIGHT
            0x7E => 0x26, // Up -> VK_UP
            0x7D => 0x28, // Down -> VK_DOWN
            // Navigation
            0x73 => 0x24, // Home -> VK_HOME
            0x77 => 0x23, // End -> VK_END
            0x74 => 0x21, // PageUp -> VK_PRIOR
            0x79 => 0x22, // PageDown -> VK_NEXT
            // Function keys
            0x7A => 0x70, // F1
            0x78 => 0x71, // F2
            0x63 => 0x72, // F3
            0x76 => 0x73, // F4
            0x60 => 0x74, // F5
            0x61 => 0x75, // F6
            0x62 => 0x76, // F7
            0x64 => 0x77, // F8
            0x65 => 0x78, // F9
            0x6D => 0x79, // F10
            0x67 => 0x7A, // F11
            0x6F => 0x7B, // F12
            // Punctuation (common ones)
            0x29 => 0xBA, // ; -> VK_OEM_1
            0x18 => 0xBB, // = -> VK_OEM_PLUS
            0x2B => 0xBC, // , -> VK_OEM_COMMA
            0x1B => 0xBD, // - -> VK_OEM_MINUS
            0x2F => 0xBE, // . -> VK_OEM_PERIOD
            0x2C => 0xBF, // / -> VK_OEM_2
            0x32 => 0xC0, // ` -> VK_OEM_3
            0x21 => 0xDB, // [ -> VK_OEM_4
            0x2A => 0xDC, // \ -> VK_OEM_5
            0x1E => 0xDD, // ] -> VK_OEM_6
            0x27 => 0xDE, // ' -> VK_OEM_7
            _ => 0,
        }
    }
}
