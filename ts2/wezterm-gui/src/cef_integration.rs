//! CEF integration for TermSurf 2.0
//!
//! This module provides proper message pump integration between CEF and WezTerm.
//! CEF calls on_schedule_message_pump_work when it needs do_message_loop_work called.

use cef::{
    rc::Rc, wrap_app, wrap_browser_process_handler, App, BrowserProcessHandler, ImplApp,
    ImplBrowserProcessHandler, ImplCommandLine, WrapApp, WrapBrowserProcessHandler,
};
use core_foundation::runloop::{
    kCFRunLoopCommonModes, CFRunLoopAddTimer, CFRunLoopGetMain, CFRunLoopTimerCreate,
    CFRunLoopTimerInvalidate, CFRunLoopTimerRef,
};
use core_foundation_sys::date::CFAbsoluteTimeGetCurrent;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Cached dark mode setting - detected once at CEF init time
static IS_DARK_MODE: AtomicBool = AtomicBool::new(false);

/// Detect if macOS is in dark mode by querying NSAppearance
fn detect_macos_dark_mode() -> bool {
    use cocoa::appkit::NSApplication;
    use cocoa::base::{id, nil};
    use objc::{msg_send, sel, sel_impl};

    unsafe {
        let app = NSApplication::sharedApplication(nil);
        if app == nil {
            return false;
        }

        let appearance: id = msg_send![app, effectiveAppearance];
        if appearance == nil {
            return false;
        }

        let name: id = msg_send![appearance, name];
        if name == nil {
            return false;
        }

        // Convert NSString to Rust string
        let utf8: *const i8 = msg_send![name, UTF8String];
        if utf8.is_null() {
            return false;
        }

        let name_str = std::ffi::CStr::from_ptr(utf8).to_str().unwrap_or("");

        // Check for dark mode appearance names
        matches!(
            name_str,
            "NSAppearanceNameDarkAqua"
                | "NSAppearanceNameVibrantDark"
                | "NSAppearanceNameAccessibilityHighContrastDarkAqua"
                | "NSAppearanceNameAccessibilityHighContrastVibrantDark"
        )
    }
}

/// Initialize dark mode detection - call this early before CEF init
pub fn init_dark_mode_detection() {
    let is_dark = detect_macos_dark_mode();
    IS_DARK_MODE.store(is_dark, Ordering::SeqCst);
    log::info!("[CEF] System dark mode detected: {}", is_dark);
}

/// Wrapper for CFRunLoopTimerRef that implements Send.
/// This is safe because we only access the timer through CFRunLoop APIs
/// which handle thread safety internally.
struct SendableTimer(CFRunLoopTimerRef);
unsafe impl Send for SendableTimer {}

/// Global timer reference - protected by Mutex for thread-safe access
static CEF_TIMER: Mutex<Option<SendableTimer>> = Mutex::new(None);

// Define our BrowserProcessHandler
wrap_browser_process_handler! {
    struct WezTermBrowserProcessHandler;

    impl BrowserProcessHandler {
        fn on_schedule_message_pump_work(&self, delay_ms: i64) {
            schedule_cef_work(delay_ms);
        }
    }
}

// Define our App that returns the BrowserProcessHandler
wrap_app! {
    pub struct WezTermCefApp {
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

            // Add dark mode switch if system is in dark mode
            if IS_DARK_MODE.load(Ordering::SeqCst) {
                command_line.append_switch(Some(&"force-dark-mode".into()));
                log::info!("[CEF] Added --force-dark-mode (system is in dark mode)");
            }
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(self.handler.clone())
        }
    }
}

/// Create the CEF App with our BrowserProcessHandler
pub fn create_app() -> App {
    let handler = WezTermBrowserProcessHandler::new();
    WezTermCefApp::new(handler)
}

/// Schedule CEF work to be done after delay_ms milliseconds.
/// Called by CEF from any thread.
fn schedule_cef_work(delay_ms: i64) {
    // Cancel existing timer if any
    cancel_timer();

    // Calculate delay in seconds
    let delay_secs = if delay_ms <= 0 {
        0.0 // Immediate
    } else {
        delay_ms as f64 / 1000.0
    };

    unsafe {
        let timer = CFRunLoopTimerCreate(
            std::ptr::null(),                        // allocator
            CFAbsoluteTimeGetCurrent() + delay_secs, // fire time
            0.0,                                     // interval (0 = non-repeating)
            0,                                       // flags
            0,                                       // order
            timer_callback,                          // callback
            std::ptr::null_mut(),                    // context
        );

        CFRunLoopAddTimer(CFRunLoopGetMain(), timer, kCFRunLoopCommonModes);

        // Store timer reference so we can cancel it later
        *CEF_TIMER.lock().unwrap() = Some(SendableTimer(timer));
    }
}

/// Timer callback - fires on main thread
extern "C" fn timer_callback(_timer: CFRunLoopTimerRef, _info: *mut c_void) {
    // Clear our reference since this timer has fired
    *CEF_TIMER.lock().unwrap() = None;

    // Do CEF's work
    cef::do_message_loop_work();
}

/// Cancel the current timer if one is pending
fn cancel_timer() {
    if let Some(timer) = CEF_TIMER.lock().unwrap().take() {
        unsafe {
            CFRunLoopTimerInvalidate(timer.0);
        }
    }
}
