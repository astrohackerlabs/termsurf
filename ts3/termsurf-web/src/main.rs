use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;
use uuid::Uuid;

// ============================================================================
// Protocol Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
struct Request {
    id: String,
    action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Response {
    id: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl Response {
    fn ok(id: &str, data: Option<serde_json::Value>) -> Self {
        Self {
            id: id.to_string(),
            status: "ok".to_string(),
            data,
            error: None,
        }
    }

    fn error(id: &str, error: &str) -> Self {
        Self {
            id: id.to_string(),
            status: "error".to_string(),
            data: None,
            error: Some(error.to_string()),
        }
    }
}

// ============================================================================
// Profile Mode
// ============================================================================

#[derive(Debug, Clone)]
enum ProfileMode {
    Named(String),
    Incognito(String), // UUID for unique socket path
}

impl ProfileMode {
    fn cache_path(&self) -> Option<PathBuf> {
        match self {
            ProfileMode::Named(name) => {
                let home = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                Some(PathBuf::from(format!(
                    "{}/.config/termsurf/cef/{}",
                    home, name
                )))
            }
            ProfileMode::Incognito(_) => None,
        }
    }

    fn socket_path(&self) -> PathBuf {
        let home = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let sockets_dir = PathBuf::from(format!("{}/.config/termsurf/sockets", home));

        // Ensure sockets directory exists
        let _ = fs::create_dir_all(&sockets_dir);

        match self {
            ProfileMode::Named(name) => sockets_dir.join(format!("{}.sock", name)),
            ProfileMode::Incognito(uuid) => sockets_dir.join(format!("incognito-{}.sock", uuid)),
        }
    }

    fn display_name(&self) -> &str {
        match self {
            ProfileMode::Named(name) => name.as_str(),
            ProfileMode::Incognito(_) => "incognito",
        }
    }
}

// ============================================================================
// Argument Parsing
// ============================================================================

fn validate_profile_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Profile name cannot be empty".into());
    }

    let first_char = name.chars().next().unwrap();
    if !first_char.is_ascii_lowercase() {
        return Err("Profile name must start with a lowercase letter".into());
    }

    for c in name.chars() {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() {
            return Err(format!(
                "Profile name must be lowercase alphanumeric, found '{}'",
                c
            ));
        }
    }

    Ok(())
}

fn parse_args() -> Result<(bool, ProfileMode, Option<String>), String> {
    let args: Vec<String> = env::args().collect();

    let is_profile_server = args.iter().any(|a| a == "--profile-server");
    let has_incognito = args.iter().any(|a| a == "--incognito");

    // Find --profile value
    let profile_value = args
        .iter()
        .position(|a| a == "--profile")
        .and_then(|i| args.get(i + 1).cloned());

    // Find --incognito-id value (for profile server)
    let incognito_id = args
        .iter()
        .position(|a| a == "--incognito-id")
        .and_then(|i| args.get(i + 1).cloned());

    // Find URL (first arg that doesn't start with --)
    let url = args
        .iter()
        .skip(1) // Skip program name
        .find(|a| !a.starts_with("--") && !a.is_empty())
        .cloned();

    // Check for mutual exclusivity
    if has_incognito && profile_value.is_some() {
        return Err("Cannot specify both --incognito and --profile".into());
    }

    let profile_mode = if has_incognito || incognito_id.is_some() {
        // Use provided ID or generate new one
        let uuid = incognito_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        ProfileMode::Incognito(uuid)
    } else if let Some(name) = profile_value {
        validate_profile_name(&name)?;
        ProfileMode::Named(name)
    } else {
        ProfileMode::Named("default".to_string())
    };

    Ok((is_profile_server, profile_mode, url))
}

// ============================================================================
// CEF App and BrowserProcessHandler (for message pump integration)
// ============================================================================

#[cfg(target_os = "macos")]
mod cef_app {
    use cef::{
        rc::Rc, wrap_app, wrap_browser_process_handler, App, BrowserProcessHandler,
        ImplApp, ImplBrowserProcessHandler, ImplCommandLine, WrapApp, WrapBrowserProcessHandler,
    };
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Global flag indicating CEF context is initialized
    pub static CEF_CONTEXT_INITIALIZED: AtomicBool = AtomicBool::new(false);

    // BrowserProcessHandler that tracks context initialization
    wrap_browser_process_handler! {
        pub struct ProfileBrowserProcessHandler;

        impl BrowserProcessHandler {
            fn on_context_initialized(&self) {
                println!("[Profile] CEF on_context_initialized fired!");
                CEF_CONTEXT_INITIALIZED.store(true, Ordering::SeqCst);
            }

            fn on_schedule_message_pump_work(&self, delay_ms: i64) {
                // We handle message pumping manually in our socket server loop
                // Just log for debugging
                if delay_ms == 0 {
                    // Immediate work needed - will be handled by our loop
                }
            }
        }
    }

    // App that provides the BrowserProcessHandler
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
                let Some(command_line) = command_line else {
                    return;
                };

                // Add useful switches for headless operation
                // Note: Do NOT add disable-gpu-compositing - we need GPU for IOSurface sharing
                command_line.append_switch(Some(&"no-startup-window".into()));
            }

            fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
                Some(self.handler.clone())
            }
        }
    }

    /// Create the CEF App with our BrowserProcessHandler
    pub fn create_app() -> App {
        let handler = ProfileBrowserProcessHandler::new();
        ProfileCefApp::new(handler)
    }

    /// Check if CEF context is initialized and ready for browser creation
    pub fn is_context_initialized() -> bool {
        CEF_CONTEXT_INITIALIZED.load(Ordering::SeqCst)
    }
}

// ============================================================================
// CEF Loading
// ============================================================================

#[cfg(target_os = "macos")]
fn load_cef(profile: &ProfileMode) -> Result<(), String> {
    use cef::args::Args;
    use cef::library_loader::LibraryLoader;
    use cef::{api_hash, execute_process, initialize, sys, CefString, Settings};

    let exe = env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    println!("[Profile] Executable path: {:?}", exe);

    let loader = LibraryLoader::new(&exe, false);
    println!("[Profile] Loading CEF framework...");
    if !loader.load() {
        return Err("Failed to load CEF framework".into());
    }
    println!("[Profile] CEF framework loaded successfully");

    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    // Create our App with BrowserProcessHandler
    let mut app = cef_app::create_app();
    println!("[Profile] Created CEF App with BrowserProcessHandler");

    let args = Args::new();

    println!("[Profile] Calling execute_process (for subprocess check)...");
    let ret = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),  // Pass our App!
        std::ptr::null_mut(),
    );
    if ret >= 0 {
        println!("[Profile] This is a subprocess, exiting with code {}", ret);
        std::process::exit(ret);
    }
    println!("[Profile] This is the main process (ret={})", ret);

    let cache_path_str = match profile.cache_path() {
        Some(path) => {
            let _ = fs::create_dir_all(&path);
            path.to_string_lossy().to_string()
        }
        None => String::new(),
    };
    println!("[Profile] Cache path: {:?}", cache_path_str);

    let helper_path = exe
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("Frameworks")
        .join("WezTerm Helper.app")
        .join("Contents")
        .join("MacOS")
        .join("WezTerm Helper");
    let helper_path_str = helper_path.to_string_lossy().to_string();
    println!("[Profile] Helper path: {:?}", helper_path_str);
    println!("[Profile] Helper exists: {}", helper_path.exists());

    let settings = Settings {
        windowless_rendering_enabled: 1,
        external_message_pump: 1,
        no_sandbox: 1,
        root_cache_path: CefString::from(cache_path_str.as_str()),
        browser_subprocess_path: CefString::from(helper_path_str.as_str()),
        ..Default::default()
    };

    println!("[Profile] Calling CEF initialize...");
    let init_result = initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(&mut app),  // Pass our App!
        std::ptr::null_mut(),
    );
    println!("[Profile] CEF initialize returned: {}", init_result);

    if init_result != 1 {
        return Err(format!("CEF initialize failed (returned {})", init_result));
    }

    println!("[Profile] CEF initialized, waiting for context...");

    // Pump the message loop until context is initialized
    let start = std::time::Instant::now();
    while !cef_app::is_context_initialized() {
        cef::do_message_loop_work();
        if start.elapsed() > Duration::from_secs(10) {
            return Err("Timeout waiting for CEF context initialization".into());
        }
        thread::sleep(Duration::from_millis(10));
    }

    println!("[Profile] CEF context initialized successfully");
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn load_cef(_profile: &ProfileMode) -> Result<(), String> {
    Err("CEF loading not yet implemented for this platform".into())
}

// ============================================================================
// wgpu State (for CEF rendering)
// ============================================================================

#[cfg(target_os = "macos")]
struct WgpuState {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

#[cfg(target_os = "macos")]
impl WgpuState {
    fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::from_comma_list("metal"),
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|e| format!("Failed to find GPU adapter: {e}"))?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                required_limits: wgpu::Limits {
                    max_non_sampler_bindings: 2048,
                    ..Default::default()
                },
                ..Default::default()
            },
        ))
        .map_err(|e| format!("Failed to create wgpu device: {e}"))?;

        Ok(Self { device, queue })
    }
}

// ============================================================================
// CEF Browser Management
// ============================================================================

#[cfg(target_os = "macos")]
mod cef_browser {
    use cef::rc::Rc;
    use cef::{
        wrap_client, wrap_context_menu_handler, wrap_render_handler,
        Browser, BrowserSettings, Client, ContextMenuHandler,
        ContextMenuParams, Frame, ImplBrowser, ImplBrowserHost, ImplClient,
        ImplContextMenuHandler, ImplMenuModel, ImplRenderHandler,
        MenuModel, PaintElementType, Rect, RenderHandler,
        ScreenInfo, WrapClient, WrapContextMenuHandler,
        WrapRenderHandler, WindowInfo,
    };
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    /// Shared state for tracking IOSurface IDs from CEF
    pub struct BrowserState {
        /// Current IOSurface ID (updated in on_accelerated_paint)
        /// Note: This ID is informational only - cross-process lookup doesn't work
        pub iosurface_id: AtomicU32,
        /// Width of the texture
        pub width: AtomicU32,
        /// Height of the texture
        pub height: AtomicU32,
    }

    impl BrowserState {
        pub fn new() -> Self {
            Self {
                iosurface_id: AtomicU32::new(0),
                width: AtomicU32::new(0),
                height: AtomicU32::new(0),
            }
        }

        /// Update the IOSurface info (for logging/debugging only)
        pub fn set_iosurface_info(&self, id: u32, width: u32, height: u32) {
            self.iosurface_id.store(id, Ordering::SeqCst);
            self.width.store(width, Ordering::SeqCst);
            self.height.store(height, Ordering::SeqCst);
        }
    }

    /// Render handler that captures IOSurface IDs
    #[derive(Clone)]
    pub struct ProfileRenderHandler {
        device_scale_factor: f32,
        size: std::rc::Rc<RefCell<(u32, u32)>>,
        state: Arc<BrowserState>,
    }

    impl ProfileRenderHandler {
        pub fn new(
            device_scale_factor: f32,
            width: u32,
            height: u32,
            state: Arc<BrowserState>,
        ) -> Self {
            Self {
                device_scale_factor,
                size: std::rc::Rc::new(RefCell::new((width, height))),
                state,
            }
        }

        pub fn set_size(&self, width: u32, height: u32) {
            *self.size.borrow_mut() = (width, height);
        }
    }

    wrap_render_handler! {
        pub struct RenderHandlerBuilder {
            handler: ProfileRenderHandler,
        }

        impl RenderHandler {
            fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
                let (width, height) = *self.handler.size.borrow();
                println!("[Profile] view_rect called, returning {}x{}", width, height);
                if let Some(rect) = rect {
                    if width > 0 && height > 0 {
                        rect.width = width as _;
                        rect.height = height as _;
                    }
                }
            }

            fn screen_info(
                &self,
                _browser: Option<&mut Browser>,
                screen_info: Option<&mut ScreenInfo>,
            ) -> ::std::os::raw::c_int {
                println!("[Profile] screen_info called, scale={}", self.handler.device_scale_factor);
                if let Some(screen_info) = screen_info {
                    screen_info.device_scale_factor = self.handler.device_scale_factor;
                    return true as _;
                }
                false as _
            }

            fn screen_point(
                &self,
                _browser: Option<&mut Browser>,
                _view_x: ::std::os::raw::c_int,
                _view_y: ::std::os::raw::c_int,
                _screen_x: Option<&mut ::std::os::raw::c_int>,
                _screen_y: Option<&mut ::std::os::raw::c_int>,
            ) -> ::std::os::raw::c_int {
                false as _
            }

            fn on_accelerated_paint(
                &self,
                _browser: Option<&mut Browser>,
                type_: PaintElementType,
                _dirty_rects: Option<&[Rect]>,
                info: Option<&cef::AcceleratedPaintInfo>,
            ) {
                println!("[Profile] on_accelerated_paint called!");
                use cef::osr_texture_import::iosurface_ipc::get_iosurface_id;

                let Some(info) = info else {
                    println!("[Profile] on_accelerated_paint: info is None");
                    return;
                };

                if type_ != PaintElementType::default() {
                    println!("[Profile] on_accelerated_paint: skipping non-default type");
                    return;
                }

                // Get the IOSurface handle and extract the ID
                let handle = info.shared_texture_io_surface;
                println!("[Profile] on_accelerated_paint: handle={:?}", handle);
                if let Some(id) = get_iosurface_id(handle) {
                    let width = info.extra.coded_size.width as u32;
                    let height = info.extra.coded_size.height as u32;
                    // Store IOSurface info for logging/debugging (cross-process lookup doesn't work)
                    self.handler.state.set_iosurface_info(id, width, height);
                    println!("[Profile] IOSurface ID: {}, size: {}x{}", id, width, height);
                } else {
                    println!("[Profile] on_accelerated_paint: get_iosurface_id returned None");
                }
            }

            fn on_paint(
                &self,
                _browser: Option<&mut Browser>,
                _type_: PaintElementType,
                _dirty_rects: Option<&[Rect]>,
                _buffer: *const u8,
                width: ::std::os::raw::c_int,
                height: ::std::os::raw::c_int,
            ) {
                // Software fallback - log if this is being called instead of accelerated
                println!("[Profile] on_paint called (SOFTWARE FALLBACK) {}x{}", width, height);
            }
        }
    }

    impl RenderHandlerBuilder {
        pub fn build(handler: ProfileRenderHandler) -> RenderHandler {
            Self::new(handler)
        }
    }

    /// Context menu handler that suppresses context menus
    #[derive(Clone)]
    pub struct ProfileContextMenuHandler {}

    wrap_context_menu_handler! {
        pub struct ContextMenuHandlerBuilder {
            handler: ProfileContextMenuHandler,
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

    impl ContextMenuHandlerBuilder {
        pub fn build() -> ContextMenuHandler {
            Self::new(ProfileContextMenuHandler {})
        }
    }

    /// CEF Client that ties everything together
    wrap_client! {
        pub struct ClientBuilder {
            render_handler: RenderHandler,
            context_menu_handler: ContextMenuHandler,
        }

        impl Client {
            fn render_handler(&self) -> Option<cef::RenderHandler> {
                Some(self.render_handler.clone())
            }

            fn context_menu_handler(&self) -> Option<cef::ContextMenuHandler> {
                Some(self.context_menu_handler.clone())
            }
        }
    }

    impl ClientBuilder {
        pub fn build(render_handler: ProfileRenderHandler) -> cef::Client {
            Self::new(
                RenderHandlerBuilder::build(render_handler),
                ContextMenuHandlerBuilder::build(),
            )
        }
    }

    /// A managed CEF browser instance
    pub struct ManagedBrowser {
        pub browser: cef::Browser,
        pub state: Arc<BrowserState>,
    }

    impl ManagedBrowser {
        pub fn create(url: &str, width: u32, height: u32) -> Option<Self> {
            println!("[Profile] ManagedBrowser::create called with url={}, size={}x{}", url, width, height);

            let state = Arc::new(BrowserState::new());

            let window_info = WindowInfo {
                windowless_rendering_enabled: true as _,
                shared_texture_enabled: true as _,
                external_begin_frame_enabled: false as _,
                ..Default::default()
            };
            println!("[Profile] WindowInfo created: windowless={}, shared_texture={}",
                window_info.windowless_rendering_enabled,
                window_info.shared_texture_enabled);

            let render_handler = ProfileRenderHandler::new(
                1.0, // device_scale_factor
                width,
                height,
                state.clone(),
            );
            println!("[Profile] RenderHandler created");

            let browser_settings = BrowserSettings {
                windowless_frame_rate: 60,
                ..Default::default()
            };
            println!("[Profile] BrowserSettings created with frame_rate={}", browser_settings.windowless_frame_rate);

            // Note: We use the global request context (None) rather than creating a custom one.
            // CEF's Chrome-based architecture has issues with custom profile paths.
            // The global context stores data in the root_cache_path set during initialization.
            println!("[Profile] Calling browser_host_create_browser_sync (using global context)...");
            let browser = cef::browser_host_create_browser_sync(
                Some(&window_info),
                Some(&mut ClientBuilder::build(render_handler)),
                Some(&url.into()),
                Some(&browser_settings),
                None,  // extra_info
                None,  // Use global request context
            );

            match browser {
                Some(b) => {
                    println!("[Profile] Browser created successfully");
                    Some(Self { browser: b, state })
                }
                None => {
                    println!("[Profile] ERROR: browser_host_create_browser_sync returned None");
                    println!("[Profile] This usually means:");
                    println!("[Profile]   - CEF context not initialized (did on_context_initialized fire?)");
                    println!("[Profile]   - Invalid URL format");
                    println!("[Profile]   - Helper process not found or failed to start");
                    None
                }
            }
        }

        pub fn get_iosurface_id(&self) -> u32 {
            self.state.iosurface_id.load(Ordering::SeqCst)
        }

        pub fn get_size(&self) -> (u32, u32) {
            (
                self.state.width.load(Ordering::SeqCst),
                self.state.height.load(Ordering::SeqCst),
            )
        }

        pub fn close(&self) {
            if let Some(host) = ImplBrowser::host(&self.browser) {
                host.close_browser(1);
            }
        }
    }
}

// ============================================================================
// Webview State (shared across connections)
// ============================================================================

#[cfg(target_os = "macos")]
struct WebviewState {
    webview_count: AtomicUsize,
    next_webview_id: AtomicU64,
    browsers: std::sync::Mutex<std::collections::HashMap<u64, cef_browser::ManagedBrowser>>,
}

#[cfg(not(target_os = "macos"))]
struct WebviewState {
    webview_count: AtomicUsize,
    next_webview_id: AtomicU64,
}

impl WebviewState {
    fn new() -> Self {
        Self {
            webview_count: AtomicUsize::new(0),
            next_webview_id: AtomicU64::new(1),
            #[cfg(target_os = "macos")]
            browsers: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    #[cfg(target_os = "macos")]
    fn open_webview(&self, url: &str, width: u32, height: u32) -> Option<(u64, u32, u32, u32)> {
        let id = self.next_webview_id.fetch_add(1, Ordering::SeqCst);

        // Create the CEF browser
        let browser = cef_browser::ManagedBrowser::create(url, width, height)?;

        // Wait briefly for the first paint to get the IOSurface ID
        // In production, we might want a more sophisticated approach
        let mut iosurface_id = 0u32;
        let mut actual_width = width;
        let mut actual_height = height;

        // Poll for IOSurface ID with a timeout
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            cef::do_message_loop_work();
            iosurface_id = browser.get_iosurface_id();
            if iosurface_id != 0 {
                let (w, h) = browser.get_size();
                actual_width = w;
                actual_height = h;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        self.browsers.lock().unwrap().insert(id, browser);
        self.webview_count.fetch_add(1, Ordering::SeqCst);

        println!(
            "[Profile] Opened webview {} for: {} (iosurface_id={}, size={}x{})",
            id, url, iosurface_id, actual_width, actual_height
        );

        Some((id, iosurface_id, actual_width, actual_height))
    }

    #[cfg(not(target_os = "macos"))]
    fn open_webview(&self, url: &str, _width: u32, _height: u32) -> Option<(u64, u32, u32, u32)> {
        let id = self.next_webview_id.fetch_add(1, Ordering::SeqCst);
        self.webview_count.fetch_add(1, Ordering::SeqCst);
        println!("[Profile] Opened webview {} for: {} (no CEF on this platform)", id, url);
        Some((id, 0, 0, 0))
    }

    #[cfg(target_os = "macos")]
    fn get_iosurface_id(&self, webview_id: u64) -> Option<u32> {
        let browsers = self.browsers.lock().unwrap();
        browsers.get(&webview_id).map(|b| b.get_iosurface_id())
    }

    #[cfg(not(target_os = "macos"))]
    fn get_iosurface_id(&self, _webview_id: u64) -> Option<u32> {
        None
    }

    fn close_webview(&self, id: u64) -> bool {
        #[cfg(target_os = "macos")]
        {
            if let Some(browser) = self.browsers.lock().unwrap().remove(&id) {
                browser.close();
            }
        }

        let prev_count = self.webview_count.fetch_sub(1, Ordering::SeqCst);
        println!(
            "[Profile] Closed webview {} (remaining: {})",
            id,
            prev_count - 1
        );
        prev_count == 1 // Returns true if this was the last webview
    }

    fn count(&self) -> usize {
        self.webview_count.load(Ordering::SeqCst)
    }
}

// ============================================================================
// Socket Server (Profile Server)
// ============================================================================

/// Handle a request and return (response, webview_id if opened)
fn handle_request(request: &Request, state: &mut WebviewState) -> (Response, Option<u64>) {
    match request.action.as_str() {
        "ping" => (
            Response::ok(&request.id, Some(serde_json::json!({"pong": true}))),
            None,
        ),

        "open" => {
            let url = request
                .data
                .as_ref()
                .and_then(|d| d.get("url"))
                .and_then(|u| u.as_str())
                .unwrap_or("about:blank");

            let width = request
                .data
                .as_ref()
                .and_then(|d| d.get("width"))
                .and_then(|w| w.as_u64())
                .unwrap_or(800) as u32;

            let height = request
                .data
                .as_ref()
                .and_then(|d| d.get("height"))
                .and_then(|h| h.as_u64())
                .unwrap_or(600) as u32;

            println!("[Profile] Received 'open' request: url={}, size={}x{}", url, width, height);

            match state.open_webview(url, width, height) {
                Some((webview_id, iosurface_id, actual_width, actual_height)) => (
                    Response::ok(
                        &request.id,
                        Some(serde_json::json!({
                            "webview_id": webview_id,
                            "iosurface_id": iosurface_id,
                            "width": actual_width,
                            "height": actual_height
                        })),
                    ),
                    Some(webview_id),
                ),
                None => (
                    Response::error(&request.id, "Failed to create browser"),
                    None,
                ),
            }
        }

        "get_iosurface_id" => {
            let webview_id = request
                .data
                .as_ref()
                .and_then(|d| d.get("webview_id"))
                .and_then(|id| id.as_u64());

            match webview_id {
                Some(id) => {
                    let iosurface_id = state.get_iosurface_id(id).unwrap_or(0);
                    (
                        Response::ok(
                            &request.id,
                            Some(serde_json::json!({
                                "iosurface_id": iosurface_id
                            })),
                        ),
                        None,
                    )
                }
                None => (
                    Response::error(&request.id, "Missing webview_id"),
                    None,
                ),
            }
        }

        "get_status" => (
            Response::ok(
                &request.id,
                Some(serde_json::json!({
                    "webview_count": state.count(),
                    "pid": std::process::id()
                })),
            ),
            None,
        ),

        _ => (
            Response::error(&request.id, &format!("Unknown action: {}", request.action)),
            None,
        ),
    }
}

// ============================================================================
// Connection State (for single-threaded poll loop)
// ============================================================================

/// A single client connection with its read buffer and owned webviews
struct Connection {
    stream: UnixStream,
    peer_id: String,
    read_buffer: String,
    owned_webviews: Vec<u64>,
    closed: bool,
}

impl Connection {
    fn new(stream: UnixStream) -> Self {
        // Set non-blocking for poll-based reading
        stream.set_nonblocking(true).expect("Failed to set non-blocking");

        let peer_id = Uuid::new_v4().to_string()[..8].to_string();
        println!("[Profile] Client {} connected", peer_id);

        Self {
            stream,
            peer_id,
            read_buffer: String::new(),
            owned_webviews: Vec::new(),
            closed: false,
        }
    }

    /// Try to read available data and extract complete lines (requests).
    /// Returns a Vec of complete JSON request lines.
    fn try_read(&mut self) -> Vec<String> {
        let mut temp_buf = [0u8; 4096];

        // Read as much as available (non-blocking)
        loop {
            match self.stream.read(&mut temp_buf) {
                Ok(0) => {
                    // EOF - connection closed by client
                    self.closed = true;
                    break;
                }
                Ok(n) => {
                    if let Ok(s) = std::str::from_utf8(&temp_buf[..n]) {
                        self.read_buffer.push_str(s);
                    }
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    // No more data available right now
                    break;
                }
                Err(e) => {
                    eprintln!("[Profile] {} read error: {}", self.peer_id, e);
                    self.closed = true;
                    break;
                }
            }
        }

        // Extract complete lines from buffer
        let mut lines = Vec::new();
        while let Some(newline_pos) = self.read_buffer.find('\n') {
            let line: String = self.read_buffer.drain(..=newline_pos).collect();
            let line = line.trim().to_string();
            if !line.is_empty() {
                lines.push(line);
            }
        }

        lines
    }

    /// Write a response to the client
    fn write_response(&mut self, response: &Response) {
        let response_json = serde_json::to_string(response).unwrap();
        println!("[Profile] {} <- {}", self.peer_id, response_json);

        if let Err(e) = writeln!(self.stream, "{}", response_json) {
            eprintln!("[Profile] {} write error: {}", self.peer_id, e);
            self.closed = true;
            return;
        }
        let _ = self.stream.flush();
    }

    /// Track a webview owned by this connection
    fn track_webview(&mut self, webview_id: u64) {
        self.owned_webviews.push(webview_id);
    }

    /// Get owned webviews for cleanup on disconnect
    fn take_owned_webviews(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.owned_webviews)
    }
}

// Need Read trait for stream.read()
use std::io::Read;

fn run_socket_server(socket_path: PathBuf, state: &mut WebviewState) {
    // Remove stale socket if it exists
    if socket_path.exists() {
        if let Err(e) = fs::remove_file(&socket_path) {
            eprintln!(
                "[Profile] Failed to remove stale socket: {} (continuing anyway)",
                e
            );
        }
    }

    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[Profile] Failed to bind socket: {}", e);
            return;
        }
    };

    // Set non-blocking for poll-based accept
    listener
        .set_nonblocking(true)
        .expect("Failed to set non-blocking");

    println!("[Profile] Socket server listening at {:?}", socket_path);

    let mut connections: Vec<Connection> = Vec::new();
    let mut should_exit = false;

    // Single-threaded poll loop
    while !should_exit {
        // 1. Try to accept new connections (non-blocking)
        match listener.accept() {
            Ok((stream, _)) => {
                connections.push(Connection::new(stream));
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                // No connection waiting - that's fine
            }
            Err(e) => {
                eprintln!("[Profile] Accept error: {}", e);
            }
        }

        // 2. Process each connection (non-blocking reads)
        for conn in &mut connections {
            if conn.closed {
                continue;
            }

            // Try to read complete request lines
            let lines = conn.try_read();

            for line in lines {
                let (response, webview_id) = match serde_json::from_str::<Request>(&line) {
                    Ok(request) => {
                        println!("[Profile] {} -> {:?}", conn.peer_id, request);
                        handle_request(&request, state)
                    }
                    Err(e) => (
                        Response::error("unknown", &format!("Invalid JSON: {}", e)),
                        None,
                    ),
                };

                // Track opened webviews
                if let Some(id) = webview_id {
                    conn.track_webview(id);
                }

                conn.write_response(&response);
            }
        }

        // 3. Handle disconnected connections - close their webviews
        for conn in &mut connections {
            if conn.closed {
                let owned = conn.take_owned_webviews();
                if !owned.is_empty() {
                    println!(
                        "[Profile] Client {} disconnected, closing {} webview(s)",
                        conn.peer_id,
                        owned.len()
                    );
                }
                for webview_id in owned {
                    let was_last = state.close_webview(webview_id);
                    if was_last {
                        should_exit = true;
                    }
                }
            }
        }

        // 4. Remove closed connections
        connections.retain(|c| !c.closed);

        // 5. Pump CEF message loop
        #[cfg(target_os = "macos")]
        cef::do_message_loop_work();

        // 6. Brief sleep to avoid busy-waiting
        thread::sleep(Duration::from_millis(1));
    }

    // Cleanup socket
    let _ = fs::remove_file(&socket_path);
    println!("[Profile] Socket cleaned up");
}

fn run_profile_server(profile: ProfileMode) {
    let socket_path = profile.socket_path();

    println!(
        "[Profile] Starting with profile={}",
        profile.display_name()
    );

    match load_cef(&profile) {
        Ok(()) => {
            println!(
                "[Profile] CEF initialized with profile={}",
                profile.display_name()
            );
        }
        Err(e) => {
            eprintln!("[Profile] Failed to load CEF: {}", e);
            std::process::exit(1);
        }
    }

    // Create webview state (no Arc needed - single-threaded)
    let mut state = WebviewState::new();

    // Run socket server (this blocks until shutdown)
    run_socket_server(socket_path, &mut state);

    #[cfg(target_os = "macos")]
    cef::shutdown();

    println!("[Profile] Exiting");
}

// ============================================================================
// Socket Client (Coordinator / web CLI)
// ============================================================================

fn try_connect(socket_path: &PathBuf) -> Option<UnixStream> {
    if !socket_path.exists() {
        return None;
    }

    match UnixStream::connect(socket_path) {
        Ok(stream) => Some(stream),
        Err(_) => {
            // Socket exists but can't connect - stale socket
            let _ = fs::remove_file(socket_path);
            None
        }
    }
}

fn wait_for_socket(socket_path: &PathBuf, timeout: Duration) -> Result<UnixStream, String> {
    let start = std::time::Instant::now();

    loop {
        if let Some(stream) = try_connect(socket_path) {
            return Ok(stream);
        }

        if start.elapsed() > timeout {
            return Err(format!("Timeout waiting for socket at {:?}", socket_path));
        }

        thread::sleep(Duration::from_millis(50));
    }
}

fn send_request(
    stream: &mut UnixStream,
    action: &str,
    data: Option<serde_json::Value>,
) -> Result<Response, String> {
    let request = Request {
        id: Uuid::new_v4().to_string(),
        action: action.to_string(),
        data,
    };

    let request_json = serde_json::to_string(&request).unwrap();
    writeln!(stream, "{}", request_json).map_err(|e| format!("Failed to write: {}", e))?;
    stream
        .flush()
        .map_err(|e| format!("Failed to flush: {}", e))?;

    let mut reader =
        BufReader::new(stream.try_clone().map_err(|e| format!("Clone failed: {}", e))?);
    let mut response_line = String::new();
    reader
        .read_line(&mut response_line)
        .map_err(|e| format!("Failed to read: {}", e))?;

    serde_json::from_str(&response_line).map_err(|e| format!("Invalid response JSON: {}", e))
}

fn spawn_profile_server(profile: &ProfileMode) {
    let exe = env::current_exe().expect("Failed to get current executable path");

    let mut cmd = Command::new(&exe);
    cmd.arg("--profile-server");

    match profile {
        ProfileMode::Named(name) => {
            cmd.arg("--profile").arg(name);
        }
        ProfileMode::Incognito(uuid) => {
            cmd.arg("--incognito").arg("--incognito-id").arg(uuid);
        }
    }

    // Spawn in background - don't wait for it
    // NOTE: Using inherit for stdout/stderr for debugging - change to null() for production
    cmd.stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdin(Stdio::null())
        .spawn()
        .expect("Failed to start profile server");
}

fn run_coordinator(profile: ProfileMode, url: Option<String>) {
    let socket_path = profile.socket_path();
    let url = url.unwrap_or_else(|| "about:blank".to_string());

    // Read environment variables for GUI integration
    let pane_id = env::var("WEZTERM_PANE")
        .ok()
        .and_then(|s| s.parse::<u64>().ok());
    let gui_socket_path = env::var("TERMSURF_GUI_SOCKET").ok().map(PathBuf::from);

    // Try to connect to existing profile server
    let mut profile_stream = if let Some(stream) = try_connect(&socket_path) {
        println!(
            "Connected to existing profile server for profile={}",
            profile.display_name()
        );
        stream
    } else {
        println!(
            "Starting profile server for profile={}...",
            profile.display_name()
        );
        spawn_profile_server(&profile);

        match wait_for_socket(&socket_path, Duration::from_secs(10)) {
            Ok(stream) => {
                println!("Connected to profile server");
                stream
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    };

    // Get terminal size for webview dimensions
    // TODO: Get actual terminal dimensions
    let (width, height) = (800u32, 600u32);

    // Open a webview via profile server
    println!("Opening webview: {}", url);
    let response = send_request(
        &mut profile_stream,
        "open",
        Some(serde_json::json!({
            "url": url,
            "width": width,
            "height": height
        })),
    );

    let (iosurface_id, actual_width, actual_height) = match response {
        Ok(resp) if resp.status == "ok" => {
            if let Some(data) = &resp.data {
                let iosurface_id = data.get("iosurface_id").and_then(|id| id.as_u64()).unwrap_or(0);
                let webview_id = data.get("webview_id").and_then(|id| id.as_u64()).unwrap_or(0);
                let w = data.get("width").and_then(|w| w.as_u64()).unwrap_or(width as u64) as u32;
                let h = data.get("height").and_then(|h| h.as_u64()).unwrap_or(height as u64) as u32;
                println!(
                    "Webview opened: id={}, iosurface_id={}, size={}x{}",
                    webview_id, iosurface_id, w, h
                );
                (iosurface_id as u32, w, h)
            } else {
                eprintln!("Failed to open webview: no data in response");
                std::process::exit(1);
            }
        }
        Ok(resp) => {
            eprintln!(
                "Failed to open webview: {}",
                resp.error.unwrap_or_default()
            );
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to open webview: {}", e);
            std::process::exit(1);
        }
    };

    // Connect to GUI socket and send display_webview command
    let mut gui_stream = if let (Some(pane_id), Some(ref gui_socket)) = (pane_id, &gui_socket_path)
    {
        if let Some(mut gui_stream) = try_connect(gui_socket) {
            println!("Connected to GUI socket at {:?}", gui_socket);

            // Send display_webview command
            let display_response = send_request(
                &mut gui_stream,
                "display_webview",
                Some(serde_json::json!({
                    "pane_id": pane_id,
                    "iosurface_id": iosurface_id,
                    "width": actual_width,
                    "height": actual_height
                })),
            );

            match display_response {
                Ok(resp) if resp.status == "ok" => {
                    println!("Webview displayed in pane {}", pane_id);
                }
                Ok(resp) => {
                    eprintln!(
                        "Failed to display webview: {}",
                        resp.error.unwrap_or_default()
                    );
                }
                Err(e) => {
                    eprintln!("Failed to send display_webview: {}", e);
                }
            }

            Some(gui_stream)
        } else {
            println!("Warning: Could not connect to GUI socket at {:?}", gui_socket);
            None
        }
    } else {
        if pane_id.is_none() {
            println!("Note: WEZTERM_PANE not set - running standalone");
        }
        if gui_socket_path.is_none() {
            println!("Note: TERMSURF_GUI_SOCKET not set - webview will not be displayed");
        }
        None
    };

    // Get profile server status
    if let Ok(resp) = send_request(&mut profile_stream, "get_status", None) {
        if let Some(data) = resp.data {
            println!(
                "Profile server status: pid={}, webviews={}",
                data.get("pid").and_then(|p| p.as_u64()).unwrap_or(0),
                data.get("webview_count")
                    .and_then(|c| c.as_u64())
                    .unwrap_or(0)
            );
        }
    }

    // Wait for Ctrl+C
    println!("\nPress Ctrl+C to close webview...");
    let (tx, rx) = std::sync::mpsc::channel();
    ctrlc::set_handler(move || {
        let _ = tx.send(());
    })
    .expect("Error setting Ctrl+C handler");

    // Block until Ctrl+C
    let _ = rx.recv();
    println!("\nShutting down...");

    // Send close_webview to GUI if connected
    if let (Some(ref mut gui_stream), Some(pane_id)) = (&mut gui_stream, pane_id) {
        let close_response = send_request(
            gui_stream,
            "close_webview",
            Some(serde_json::json!({
                "pane_id": pane_id
            })),
        );

        match close_response {
            Ok(resp) if resp.status == "ok" => {
                println!("Webview closed in pane {}", pane_id);
            }
            Ok(resp) => {
                eprintln!(
                    "Failed to close webview: {}",
                    resp.error.unwrap_or_default()
                );
            }
            Err(e) => {
                eprintln!("Failed to send close_webview: {}", e);
            }
        }
    }

    // Profile stream is dropped here, closing the connection
    // The profile server will detect EOF and close our webview
    println!("Disconnected from profile server");
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    match parse_args() {
        Ok((is_profile_server, profile, url)) => {
            if is_profile_server {
                run_profile_server(profile);
            } else {
                run_coordinator(profile, url);
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            eprintln!();
            eprintln!("Usage: web [URL] [--profile <name>] [--incognito]");
            eprintln!();
            eprintln!("Options:");
            eprintln!("  --profile <name>  Use named profile (default: 'default')");
            eprintln!("  --incognito       Use incognito mode (no persistent storage)");
            eprintln!();
            eprintln!("Profile names must be lowercase alphanumeric and start with a letter.");
            std::process::exit(1);
        }
    }
}
