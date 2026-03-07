// let () = msg_send! is a common pattern for objc
#![allow(clippy::let_unit_value)]

use super::nsstring_to_str;
use super::window::WindowInner;
use crate::connection::ConnectionOps;
use crate::os::macos::app::create_app_delegate;
use crate::screen::{ScreenInfo, Screens};
use crate::spawn::*;
use crate::Appearance;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSScreen};
use objc2_foundation::{MainThreadMarker, NSInteger};
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::AtomicUsize;

pub struct Connection {
    ns_app: Retained<NSApplication>,
    pub(crate) windows: RefCell<HashMap<usize, Rc<RefCell<WindowInner>>>>,
    pub(crate) next_window_id: AtomicUsize,
    pub(crate) gl_connection: RefCell<Option<Rc<crate::egl::GlConnection>>>,
}

impl Connection {
    pub(crate) fn create_new() -> anyhow::Result<Self> {
        // Ensure that the SPAWN_QUEUE is created; it will have nothing
        // to run right now.
        SPAWN_QUEUE.run();

        let mtm = MainThreadMarker::new().unwrap();
        let ns_app = NSApplication::sharedApplication(mtm);
        ns_app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

        let delegate = create_app_delegate()?;
        unsafe {
            let () = objc2::msg_send![&*ns_app, setDelegate: &*delegate];
        }

        let conn = Self {
            ns_app,
            windows: RefCell::new(HashMap::new()),
            next_window_id: AtomicUsize::new(1),
            gl_connection: RefCell::new(None),
        };
        Ok(conn)
    }

    pub(crate) fn next_window_id(&self) -> usize {
        self.next_window_id
            .fetch_add(1, ::std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn window_by_id(&self, window_id: usize) -> Option<Rc<RefCell<WindowInner>>> {
        self.windows.borrow().get(&window_id).map(Rc::clone)
    }

    pub(crate) fn with_window_inner<
        R,
        F: FnOnce(&mut WindowInner) -> anyhow::Result<R> + Send + 'static,
    >(
        window_id: usize,
        f: F,
    ) -> promise::Future<R>
    where
        R: Send + 'static,
    {
        let mut prom = promise::Promise::new();
        let future = prom.get_future().unwrap();
        promise::spawn::spawn_into_main_thread(async move {
            if let Some(handle) = Connection::get().unwrap().window_by_id(window_id) {
                let mut inner = handle.borrow_mut();
                prom.result(f(&mut inner));
            }
        })
        .detach();

        future
    }
}

/// `/System/Library/CoreServices/SystemVersion.plist`
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct SoftwareVersion {
    product_build_version: String,
    product_user_visible_version: String,
    product_name: String,
}

impl SoftwareVersion {
    fn load() -> anyhow::Result<Self> {
        let vers: Self = plist::from_file("/System/Library/CoreServices/SystemVersion.plist")?;
        Ok(vers)
    }
}

impl ConnectionOps for Connection {
    fn name(&self) -> String {
        if let Ok(vers) = SoftwareVersion::load() {
            format!(
                "{} {} ({})",
                vers.product_name, vers.product_user_visible_version, vers.product_build_version
            )
        } else {
            "macOS".to_string()
        }
    }

    fn default_dpi(&self) -> f64 {
        if let Ok(screens) = self.screens() {
            screens.active.effective_dpi.unwrap_or(crate::DEFAULT_DPI)
        } else {
            crate::DEFAULT_DPI
        }
    }

    fn terminate_message_loop(&self) {
        // bounce via an event callback to encourage stop to apply
        // to the correct level of run loop
        promise::spawn::spawn_into_main_thread(async move {
            let mtm = MainThreadMarker::new().unwrap();
            let ns_app = NSApplication::sharedApplication(mtm);
            unsafe {
                let () = objc2::msg_send![&*ns_app, stop: std::ptr::null::<AnyObject>()];
                // Generate a UI event so that the run loop breaks out
                // after receiving the stop
                let () = objc2::msg_send![&*ns_app, abortModal];
            }
        })
        .detach();
    }

    fn get_appearance(&self) -> Appearance {
        let name = unsafe {
            let appearance: *mut AnyObject = objc2::msg_send![&*self.ns_app, effectiveAppearance];
            let name_obj: *mut AnyObject = objc2::msg_send![appearance, name];
            nsstring_to_str(name_obj)
        };
        log::debug!("NSAppearanceName is {name}");
        match name {
            "NSAppearanceNameVibrantDark" | "NSAppearanceNameDarkAqua" => Appearance::Dark,
            "NSAppearanceNameVibrantLight" | "NSAppearanceNameAqua" => Appearance::Light,
            "NSAppearanceNameAccessibilityHighContrastVibrantLight"
            | "NSAppearanceNameAccessibilityHighContrastAqua" => Appearance::LightHighContrast,
            "NSAppearanceNameAccessibilityHighContrastVibrantDark"
            | "NSAppearanceNameAccessibilityHighContrastDarkAqua" => Appearance::DarkHighContrast,
            _ => {
                log::warn!("Unknown NSAppearanceName {name}, assume Light");
                Appearance::Light
            }
        }
    }

    fn run_message_loop(&self) -> anyhow::Result<()> {
        self.ns_app.run();
        self.windows.borrow_mut().clear();
        Ok(())
    }

    fn hide_application(&self) {
        unsafe {
            let () = objc2::msg_send![&*self.ns_app, hide: &*self.ns_app];
        }
    }

    fn beep(&self) {
        unsafe {
            NSBeep();
        }
    }

    fn screens(&self) -> anyhow::Result<Screens> {
        let mut by_name = HashMap::new();
        let mut virtual_rect = euclid::rect(0, 0, 0, 0);

        let mtm = MainThreadMarker::new().unwrap();
        let screens = NSScreen::screens(mtm);
        let count = screens.count();
        for idx in 0..count {
            let screen = screens.objectAtIndex(idx);
            let screen = nsscreen_to_screen_info(&screen);
            virtual_rect = virtual_rect.union(&screen.rect);
            by_name.insert(screen.name.clone(), screen);
        }

        // The screen with the menu bar is always index 0
        let main = nsscreen_to_screen_info(&screens.objectAtIndex(0));

        // The active screen is known as the "main" screen in macOS
        let active = NSScreen::mainScreen(mtm)
            .map(|s| nsscreen_to_screen_info(&s))
            .unwrap_or_else(|| nsscreen_to_screen_info(&screens.objectAtIndex(0)));

        Ok(Screens {
            by_name,
            active,
            main,
            virtual_rect,
        })
    }
}

pub fn nsscreen_to_screen_info(screen: &NSScreen) -> ScreenInfo {
    let frame = screen.frame();
    let backing_frame = screen.convertRectToBacking(frame);
    let rect = euclid::rect(
        backing_frame.origin.x as isize,
        backing_frame.origin.y as isize,
        backing_frame.size.width as isize,
        backing_frame.size.height as isize,
    );

    let name = {
        let name_obj = screen.localizedName();
        let ptr = Retained::as_ptr(&name_obj) as *mut AnyObject;
        unsafe { nsstring_to_str(ptr) }.to_string()
    };

    let max_fps: NSInteger = unsafe { objc2::msg_send![screen, maximumFramesPerSecond] };
    let max_fps = Some(max_fps as usize);

    let scale = backing_frame.size.width / frame.size.width;

    let config = config::configuration();
    let effective_dpi = if let Some(dpi) = config.dpi_by_screen.get(&name).copied() {
        Some(dpi)
    } else if let Some(dpi) = config.dpi {
        Some(dpi)
    } else {
        Some(crate::DEFAULT_DPI * scale)
    };

    ScreenInfo {
        name,
        rect,
        scale,
        max_fps,
        effective_dpi,
    }
}

extern "C" {
    fn NSBeep();
}
