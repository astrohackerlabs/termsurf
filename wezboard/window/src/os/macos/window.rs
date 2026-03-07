// let () = msg_send! is a common pattern for objc
#![allow(clippy::let_unit_value)]

use super::keycodes::*;
use super::{nsstring, nsstring_to_str};
use crate::clipboard::Clipboard as ClipboardContext;
use crate::connection::ConnectionOps;
use crate::os::macos::menu::{MenuItem, RepresentedItem};
use crate::parameters::{Border, Parameters, TitleBar};
use crate::{
    Clipboard, Connection, DeadKeyStatus, Dimensions, Handled, KeyCode, KeyEvent, Modifiers,
    MouseButtons, MouseCursor, MouseEvent, MouseEventKind, MousePress, Point, RawKeyEvent, Rect,
    RequestedWindowGeometry, ResizeIncrement, ResolvedGeometry, ScreenPoint, Size, ULength,
    WindowDecorations, WindowEvent, WindowEventSender, WindowOps, WindowState,
};
use anyhow::{anyhow, bail};
use async_trait::async_trait;
// Function key constants (from NSEvent.h)
const NS_UP_ARROW_FUNCTION_KEY: u16 = 0xF700;
const NS_DOWN_ARROW_FUNCTION_KEY: u16 = 0xF701;
const NS_LEFT_ARROW_FUNCTION_KEY: u16 = 0xF702;
const NS_RIGHT_ARROW_FUNCTION_KEY: u16 = 0xF703;
const NS_F1_FUNCTION_KEY: u16 = 0xF704;
const NS_F35_FUNCTION_KEY: u16 = 0xF726;
const NS_INSERT_FUNCTION_KEY: u16 = 0xF727;
const NS_DELETE_FUNCTION_KEY: u16 = 0xF728;
const NS_HOME_FUNCTION_KEY: u16 = 0xF729;
const NS_END_FUNCTION_KEY: u16 = 0xF72B;
const NS_PAGE_UP_FUNCTION_KEY: u16 = 0xF72C;
const NS_PAGE_DOWN_FUNCTION_KEY: u16 = 0xF72D;
const NS_PRINT_SCREEN_FUNCTION_KEY: u16 = 0xF72E;
const NS_SCROLL_LOCK_FUNCTION_KEY: u16 = 0xF72F;
const NS_PAUSE_FUNCTION_KEY: u16 = 0xF730;
const NS_BREAK_FUNCTION_KEY: u16 = 0xF732;
const NS_PRINT_FUNCTION_KEY: u16 = 0xF738;
const NS_CLEAR_LINE_FUNCTION_KEY: u16 = 0xF739;

// NSOpenGL context parameters
const NS_OPENGL_CP_SURFACE_OPACITY: isize = 236;
const NS_OPENGL_CP_SWAP_INTERVAL: isize = 222;

// NSWindow tabbing mode
const NS_WINDOW_TABBING_MODE_DISALLOWED: isize = 2;

// NSWindow standard button indices
const NS_WINDOW_CLOSE_BUTTON: u64 = 0;
const NS_WINDOW_MINIATURIZE_BUTTON: u64 = 1;
const NS_WINDOW_ZOOM_BUTTON: u64 = 2;
use config::window::WindowLevel;
use config::{ConfigHandle, RgbaColor, SrgbaTuple};
use core_foundation::base::{CFTypeID, TCFType};
use core_foundation::bundle::{CFBundleGetBundleWithIdentifier, CFBundleGetFunctionPointerForName};
use core_foundation::data::{CFData, CFDataGetBytePtr, CFDataRef};
use core_foundation::string::{CFString, CFStringRef, UniChar};
use core_foundation::{declare_TCFType, impl_TCFType};
use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyClass, AnyObject, AnyProtocol, Bool, ClassBuilder, Sel};
use objc2_app_kit::{
    NSApplicationPresentationOptions, NSAutoresizingMaskOptions, NSBackingStoreType,
    NSEventModifierFlags, NSWindowStyleMask,
};

#[allow(non_camel_case_types)]
type id = *mut AnyObject;
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGContext;
use objc2_foundation::NSRange;
use promise::Future;
use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, DisplayHandle, HandleError, HasDisplayHandle,
    HasWindowHandle, RawDisplayHandle, RawWindowHandle, WindowHandle,
};
use std::any::Any;
use std::cell::RefCell;
use std::ffi::{CStr, c_void};
use std::path::PathBuf;
use std::ptr::NonNull;
use std::rc::Rc;
use std::str::FromStr;
use std::time::Instant;
use wezboard_font::FontConfiguration;
use wezboard_input_types::{IntegratedTitleButtonStyle, KeyboardLedStatus, is_ascii_control};

#[allow(non_upper_case_globals)]
const NSViewLayerContentsPlacementTopLeft: isize = 11;
#[allow(non_upper_case_globals)]
const NSViewLayerContentsRedrawDuringViewResize: isize = 2;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGSMainConnectionID() -> id;
    fn CGSSetWindowBackgroundBlurRadius(connection_id: id, window_id: isize, radius: i64) -> i32;
}

fn round_away_from_zerof(value: f64) -> f64 {
    if value > 0. {
        value.max(1.).round()
    } else {
        value.min(-1.).round()
    }
}

fn round_away_from_zero(value: f64) -> i16 {
    if value > 0. {
        value.max(1.).round() as i16
    } else {
        value.min(-1.).round() as i16
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ImeDisposition {
    /// Nothing happened
    None,
    /// IME triggered an action
    Acted,
    /// We decided to continue with key dispatch
    Continue,
}

#[derive(Clone)]
pub enum BackendImpl {
    Cgl(Rc<cglbits::GlState>),
    Egl(Rc<crate::egl::GlState>),
}

impl BackendImpl {
    pub fn update(&self) {
        if let Self::Cgl(be) = self {
            be.update();
        }
    }
}

#[derive(Clone)]
pub struct GlContextPair {
    pub context: Rc<glium::backend::Context>,
    pub backend: BackendImpl,
}

impl GlContextPair {
    /// on macOS we first try to initialize EGL by dynamically loading it.
    /// The system doesn't provide an EGL implementation, but the ANGLE
    /// project (and MetalANGLE) both provide implementations.
    /// The ANGLE EGL implementation wants a CALayer descendant passed
    /// as the EGLNativeWindowType.
    pub fn create(view: id) -> anyhow::Result<Self> {
        let behavior = if cfg!(debug_assertions) {
            glium::debug::DebugCallbackBehavior::DebugMessageOnError
        } else {
            glium::debug::DebugCallbackBehavior::Ignore
        };

        // Let's first try to initialize EGL...
        let (context, backend) = match if config::configuration().prefer_egl {
            // ANGLE wants a layer, so tell the view to create one.
            // Importantly, we must set its scale to 1.0 prior to initializing
            // EGL to prevent undesirable scaling.
            let layer: id;
            unsafe {
                let _: () =
                    objc2::msg_send![view as *const _ as *const AnyObject, setWantsLayer: true];
                layer = {
                    let __r: *mut AnyObject =
                        objc2::msg_send![view as *const _ as *const AnyObject, layer];
                    __r as id
                };
                let _: () = objc2::msg_send![layer as *const _ as *const AnyObject, setContentsScale: 1.0f64];
                let _: () =
                    objc2::msg_send![layer as *const _ as *const AnyObject, setOpaque: false];
            };

            let conn = Connection::get().unwrap();

            let state = match conn.gl_connection.borrow().as_ref() {
                None => crate::egl::GlState::create(None, layer as *const c_void),
                Some(glconn) => crate::egl::GlState::create_with_existing_connection(
                    glconn,
                    layer as *const c_void,
                ),
            };

            if state.is_ok() {
                conn.gl_connection
                    .borrow_mut()
                    .replace(Rc::clone(state.as_ref().unwrap().get_connection()));

                // ANGLE will create a CAMetalLayer as a sublayer of our provided
                // layer.  Even though CALayer defaults to !opaque, CAMetalLayer
                // defaults to opaque, so we need to find that layer and fix
                // the opacity so that our alpha values are respected.
                unsafe {
                    let sublayers: id = {
                        let __r: *mut AnyObject =
                            objc2::msg_send![layer as *const _ as *const AnyObject, sublayers];
                        __r as id
                    };
                    let layer_count: usize =
                        objc2::msg_send![sublayers as *const _ as *const AnyObject, count];
                    for i in 0..layer_count {
                        let layer: *mut AnyObject = objc2::msg_send![sublayers as *const _ as *const AnyObject, objectAtIndex: i];
                        let _: () = objc2::msg_send![layer as *const _ as *const AnyObject, setOpaque: false];
                    }
                }
            }

            state
        } else {
            Err(anyhow!("prefers not to use EGL"))
        } {
            Ok(backend) => {
                let backend = Rc::new(backend);
                let context =
                    unsafe { glium::backend::Context::new(Rc::clone(&backend), true, behavior) }?;
                (context, BackendImpl::Egl(backend))
            }
            // ... and then fallback to the deprecated platform provided CGL
            Err(err) => {
                log::debug!("EGL init failed: {:#}, falling back to CGL", err);
                let backend = Rc::new(cglbits::GlState::create(view)?);
                let context =
                    unsafe { glium::backend::Context::new(Rc::clone(&backend), true, behavior) }?;
                (context, BackendImpl::Cgl(backend))
            }
        };

        Ok(Self { context, backend })
    }
}

mod cglbits {
    use super::*;

    pub struct GlState {
        _pixel_format: Retained<AnyObject>,
        gl_context: Retained<AnyObject>,
    }

    impl GlState {
        pub fn create(view: id) -> anyhow::Result<Self> {
            log::trace!("Calling NSOpenGLPixelFormat::initWithAttributes");
            let pixel_format = unsafe {
                let attrs: [u32; 15] = [
                    99,     // NSOpenGLPFAOpenGLProfile
                    0x3200, // NSOpenGLProfileVersion3_2Core
                    74,     // NSOpenGLPFAClosestPolicy
                    8,      // NSOpenGLPFAColorSize
                    32, 11, // NSOpenGLPFAAlphaSize
                    8, 12, // NSOpenGLPFADepthSize
                    24, 13, // NSOpenGLPFAStencilSize
                    8, 96, // NSOpenGLPFAAllowOfflineRenderers
                    73, // NSOpenGLPFAAccelerated
                    5,  // NSOpenGLPFADoubleBuffer
                    0,
                ];
                let pf: id = {
                    let __r: *mut AnyObject =
                        objc2::msg_send![objc2::class!(NSOpenGLPixelFormat), alloc];
                    __r as id
                };
                let pf: id = {
                    let __r: *mut AnyObject = objc2::msg_send![pf as *const _ as *const AnyObject, initWithAttributes: attrs.as_ptr()];
                    __r as id
                };
                Retained::from_raw(pf)
            };
            log::trace!("NSOpenGLPixelFormat::initWithAttributes returned");
            let pixel_format =
                pixel_format.ok_or_else(|| anyhow!("failed to create NSOpenGLPixelFormat"))?;

            // Allow using retina resolutions; without this we're forced into low res
            // and the system will scale us up, resulting in blurry rendering
            unsafe {
                let _: () = objc2::msg_send![view as *const _ as *const AnyObject, setWantsBestResolutionOpenGLSurface: true];
            }

            let gl_context = unsafe {
                Retained::from_raw({
                    let ctx: id = {
                        let __r: *mut AnyObject =
                            objc2::msg_send![objc2::class!(NSOpenGLContext), alloc];
                        __r as id
                    };
                    let __r: *mut AnyObject = objc2::msg_send![ctx as *const _ as *const AnyObject, initWithFormat: Retained::as_ptr(&pixel_format) as *const _ as *mut AnyObject, shareContext: std::ptr::null::<AnyObject>()];
                    __r
                })
            };
            let gl_context =
                gl_context.ok_or_else(|| anyhow!("failed to create NSOpenGLContext"))?;

            unsafe {
                let opaque: cgl::GLint = 0;
                let _: () = objc2::msg_send![Retained::as_ptr(&gl_context) as *const AnyObject, setValues: &opaque as *const cgl::GLint, forParameter: NS_OPENGL_CP_SURFACE_OPACITY];

                let _: () = objc2::msg_send![Retained::as_ptr(&gl_context) as *const AnyObject, setView: view as *const _ as *const AnyObject];

                // Explicitly disable vsync; we'll manage throttling frames at
                // the application level
                let swap_interval: cgl::GLint = 0;
                let _: () = objc2::msg_send![Retained::as_ptr(&gl_context) as *const AnyObject, setValues: &swap_interval as *const cgl::GLint, forParameter: NS_OPENGL_CP_SWAP_INTERVAL];
            }

            Ok(Self {
                _pixel_format: pixel_format,
                gl_context,
            })
        }

        /// Calls NSOpenGLContext update; we need to do this on resize
        pub fn update(&self) {
            unsafe {
                let _: () = objc2::msg_send![
                    Retained::as_ptr(&self.gl_context) as *const _ as *const _ as *const AnyObject,
                    update
                ];
            }
        }
    }

    unsafe impl glium::backend::Backend for GlState {
        fn resize(&self, _: (u32, u32)) {
            todo!()
        }

        fn swap_buffers(&self) -> Result<(), glium::SwapBuffersError> {
            unsafe {
                let pool: id = {
                    let __r: *mut AnyObject =
                        objc2::msg_send![objc2::class!(NSAutoreleasePool), new];
                    __r as id
                };
                let _: () = objc2::msg_send![
                    Retained::as_ptr(&self.gl_context) as *const _ as *const AnyObject,
                    flushBuffer
                ];
                let _: () = objc2::msg_send![pool as *const _ as *const AnyObject, release];
            }
            Ok(())
        }

        unsafe fn get_proc_address(&self, symbol: &str) -> *const c_void {
            let symbol_name: CFString = FromStr::from_str(symbol).unwrap();
            let framework_name: CFString = FromStr::from_str("com.apple.opengl").unwrap();
            let framework = CFBundleGetBundleWithIdentifier(framework_name.as_concrete_TypeRef());
            let symbol =
                CFBundleGetFunctionPointerForName(framework, symbol_name.as_concrete_TypeRef());
            symbol as *const _
        }

        fn get_framebuffer_dimensions(&self) -> (u32, u32) {
            unsafe {
                let view: id = {
                    let __r: *mut AnyObject = objc2::msg_send![
                        Retained::as_ptr(&self.gl_context) as *const _ as *const AnyObject,
                        view
                    ];
                    __r as id
                };
                let frame: CGRect = objc2::msg_send![view as *const _ as *const AnyObject, frame];
                let backing_frame: CGRect = objc2::msg_send![view as *const _ as *const AnyObject, convertRectToBacking: frame];
                (
                    backing_frame.size.width as u32,
                    backing_frame.size.height as u32,
                )
            }
        }

        fn is_current(&self) -> bool {
            unsafe {
                let pool: id = {
                    let __r: *mut AnyObject =
                        objc2::msg_send![objc2::class!(NSAutoreleasePool), new];
                    __r as id
                };
                let current: id = {
                    let __r: *mut AnyObject =
                        objc2::msg_send![objc2::class!(NSOpenGLContext), currentContext];
                    __r as id
                };
                let res = if !current.is_null() {
                    let is_equal: bool = objc2::msg_send![current as *const _ as *const AnyObject, isEqual: Retained::as_ptr(&self.gl_context) as *const _ as *mut AnyObject];
                    is_equal
                } else {
                    false
                };
                let _: () = objc2::msg_send![pool as *const _ as *const AnyObject, release];
                res
            }
        }

        unsafe fn make_current(&self) {
            let _: () = objc2::msg_send![
                Retained::as_ptr(&self.gl_context) as *const _ as *const _ as *const AnyObject,
                update
            ];
            let _: () = objc2::msg_send![
                Retained::as_ptr(&self.gl_context) as *const _ as *const AnyObject,
                makeCurrentContext
            ];
        }
    }
}

pub(crate) struct WindowInner {
    view: Retained<AnyObject>,
    window: Retained<AnyObject>,
    config: ConfigHandle,
}

fn function_key_to_keycode(function_key: char) -> KeyCode {
    // FIXME: CTRL-C is 0x3, should it be normalized to C here
    // using the unmod string?  Or should be normalize the 0x3
    // as the canonical representation of that input?
    match function_key as u16 {
        NS_UP_ARROW_FUNCTION_KEY => KeyCode::UpArrow,
        NS_DOWN_ARROW_FUNCTION_KEY => KeyCode::DownArrow,
        NS_LEFT_ARROW_FUNCTION_KEY => KeyCode::LeftArrow,
        NS_RIGHT_ARROW_FUNCTION_KEY => KeyCode::RightArrow,
        NS_HOME_FUNCTION_KEY => KeyCode::Home,
        NS_END_FUNCTION_KEY => KeyCode::End,
        NS_PAGE_UP_FUNCTION_KEY => KeyCode::PageUp,
        NS_PAGE_DOWN_FUNCTION_KEY => KeyCode::PageDown,
        NS_CLEAR_LINE_FUNCTION_KEY => KeyCode::NumLock,
        value @ NS_F1_FUNCTION_KEY..=NS_F35_FUNCTION_KEY => {
            KeyCode::Function((value - NS_F1_FUNCTION_KEY + 1) as u8)
        }
        NS_INSERT_FUNCTION_KEY => KeyCode::Insert,
        NS_DELETE_FUNCTION_KEY => KeyCode::Char('\u{7f}'),
        NS_PRINT_SCREEN_FUNCTION_KEY => KeyCode::PrintScreen,
        NS_SCROLL_LOCK_FUNCTION_KEY => KeyCode::ScrollLock,
        NS_PAUSE_FUNCTION_KEY => KeyCode::Pause,
        NS_BREAK_FUNCTION_KEY => KeyCode::Cancel,
        NS_PRINT_FUNCTION_KEY => KeyCode::Print,
        _ => KeyCode::Char(function_key),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct Window {
    id: usize,
    ns_window: *mut AnyObject,
    ns_view: *mut AnyObject,
}

unsafe impl Send for Window {}
unsafe impl Sync for Window {}

fn set_window_position(window: *mut AnyObject, coords: ScreenPoint) {
    unsafe {
        let cartesian = screen_point_to_cartesian(coords);
        let frame: CGRect = objc2::msg_send![window as *const _ as *const AnyObject, frame];
        let content_frame: CGRect = objc2::msg_send![window as *const _ as *const AnyObject, contentRectForFrameRect: frame];
        let delta_x = content_frame.origin.x - frame.origin.x;
        let delta_y = content_frame.origin.y - frame.origin.y;
        let point = CGPoint::new(
            cartesian.x as f64 - delta_x,
            cartesian.y as f64 - delta_y - content_frame.size.height,
        );
        let point_cg = CGPoint::new(point.x, point.y);
        let _: () =
            objc2::msg_send![window as *const _ as *const AnyObject, setFrameOrigin: point_cg];
    }
}

impl Window {
    pub async fn new_window<F>(
        _class_name: &str,
        name: &str,
        geometry: RequestedWindowGeometry,
        config: Option<&ConfigHandle>,
        _font_config: Rc<FontConfiguration>,
        event_handler: F,
    ) -> anyhow::Result<Window>
    where
        F: 'static + FnMut(WindowEvent, &Window),
    {
        let config = match config {
            Some(c) => c.clone(),
            None => config::configuration(),
        };

        let conn = Connection::get().expect("new_window called on gui thread");
        let ResolvedGeometry {
            width,
            height,
            x,
            y,
        } = conn.resolve_geometry(geometry);

        let scale_factor = (conn.default_dpi() / crate::DEFAULT_DPI) as usize;
        let width = width / scale_factor;
        let height = height / scale_factor;
        let x = x.map(|x| x / scale_factor as i32);
        let y = y.map(|y| y / scale_factor as i32);

        let initial_pos = match (x, y) {
            (Some(x), Some(y)) => Some(ScreenPoint::new(x as isize, y as isize)),
            _ => None,
        };

        unsafe {
            let style_mask = decoration_to_mask(
                config.window_decorations,
                config.integrated_title_button_style,
            );
            let rect = CGRect::new(
                CGPoint::new(0., 0.),
                CGSize::new(width as f64, height as f64),
            );

            let conn = Connection::get().expect("Connection::init has not been called");

            let window_id = conn.next_window_id();
            let events = WindowEventSender::new(event_handler);

            let inner = Rc::new(RefCell::new(Inner {
                events,
                view_id: None,
                window_id,
                window: None,
                screen_changed: false,
                paint_throttled: false,
                invalidated: true,
                gl_context_pair: None,
                text_cursor_position: Rect::new(Point::new(0, 0), Size::new(0, 0)),
                tracking_rect_tag: 0,
                hscroll_remainder: 0.,
                vscroll_remainder: 0.,
                last_wheel: Instant::now(),
                key_is_down: None,
                dead_pending: None,
                fullscreen: None,
                config: config.clone(),
                ime_state: ImeDisposition::None,
                ime_last_event: None,
                live_resizing: false,
                ime_text: String::new(),
            }));

            let window: id = {
                let __r: *mut AnyObject = objc2::msg_send![get_window_class(), alloc];
                __r as id
            };
            let cg_rect = CGRect::new(
                CGPoint::new(rect.origin.x, rect.origin.y),
                CGSize::new(rect.size.width, rect.size.height),
            );
            let window: *mut AnyObject = objc2::msg_send![
                window as *const _ as *const AnyObject,
                initWithContentRect: cg_rect,
                styleMask: style_mask,
                backing: NSBackingStoreType::Buffered,
                defer: false
            ];
            let window = Retained::from_raw(window)
                .ok_or_else(|| anyhow::anyhow!("NSWindow initWithContentRect returned nil"))?;

            apply_decorations_to_window(
                &window,
                config.window_decorations,
                config.integrated_title_button_style,
            );

            // Prevent Cocoa native tabs from being used
            let _: () = objc2::msg_send![Retained::as_ptr(&window) as *const AnyObject, setTabbingMode: NS_WINDOW_TABBING_MODE_DISALLOWED];
            let _: () = objc2::msg_send![Retained::as_ptr(&window) as *const AnyObject, setRestorable: false];

            let _: () = objc2::msg_send![Retained::as_ptr(&window) as *const AnyObject, setReleasedWhenClosed: false];
            let clear_color: id = {
                let __r: *mut AnyObject = objc2::msg_send![objc2::class!(NSColor), clearColor];
                __r as id
            };
            let _: () = objc2::msg_send![Retained::as_ptr(&window) as *const AnyObject, setBackgroundColor: clear_color as *const _ as *const AnyObject];

            // Tell Cocoa that we output in sRGB, so it handles color space
            // conversion for non-sRGB displays.
            let srgb_color_space: id = {
                let __r: *mut AnyObject =
                    objc2::msg_send![objc2::class!(NSColorSpace), sRGBColorSpace];
                __r as id
            };
            let _: () = objc2::msg_send![Retained::as_ptr(&window) as *const AnyObject, setColorSpace: srgb_color_space as *const _ as *const AnyObject];

            // We could set this, but it makes the entire window, including
            // its titlebar, opaque to this fixed degree.
            // window.setAlphaValue_(0.4);

            // Window positioning: the first window opens up in the center of
            // the screen.  Subsequent windows will be offset from the position
            // of the prior window at the time it was created.  It's not a
            // perfect algorithm by any means, and doesn't take in account
            // windows moving and closing since the last creation, but it is
            // better than creating them all centered which is what we used
            // to do here.
            thread_local! {
                static LAST_POSITION: RefCell<Option<CGPoint>> = RefCell::new(None);
            }

            let frame: CGRect =
                objc2::msg_send![Retained::as_ptr(&window) as *const AnyObject, frame];
            let active_screen: id = {
                let __r: *mut AnyObject = objc2::msg_send![objc2::class!(NSScreen), mainScreen];
                __r as id
            };
            let active_screen_frame: CGRect =
                objc2::msg_send![active_screen as *const _ as *const AnyObject, frame];

            fn point_in_rect(pt: CGPoint, rect: CGRect) -> bool {
                let rect: euclid::Rect<f64, ()> = euclid::rect(
                    rect.origin.x,
                    rect.origin.y,
                    rect.size.width,
                    rect.size.height,
                );
                rect.contains(euclid::point2(pt.x, pt.y))
            }

            LAST_POSITION.with(|last_pos| {
                if let Some(pos) = initial_pos {
                    // Put it where they asked it to be, without influencing
                    // future positioning info
                    set_window_position(Retained::as_ptr(&window) as *mut AnyObject, pos);
                    return;
                }
                let pos = last_pos.borrow_mut().take();
                let next_pos = match pos {
                    Some(pos) if point_in_rect(pos, active_screen_frame) => {
                        // Only continue the cascade if the prior point is
                        // still within the currently active screen
                        let pos_cg = CGPoint::new(pos.x, pos.y);
                        let np: CGPoint = objc2::msg_send![Retained::as_ptr(&window) as *const AnyObject, cascadeTopLeftFromPoint: pos_cg];
                        CGPoint::new(np.x, np.y)
                    }
                    _ => {
                        // Otherwise, position as if it is the first time
                        // we're displaying on this screen
                        let _: () = objc2::msg_send![Retained::as_ptr(&window) as *const AnyObject, center];
                        let np: CGPoint = objc2::msg_send![Retained::as_ptr(&window) as *const AnyObject, cascadeTopLeftFromPoint: frame.origin];
                        CGPoint::new(np.x, np.y)
                    }
                };
                last_pos.borrow_mut().replace(next_pos);
            });

            let _: () = objc2::msg_send![Retained::as_ptr(&window) as *const AnyObject, setTitle: Retained::as_ptr(&nsstring(&name)) as *const _ as *mut AnyObject];
            let _: () = objc2::msg_send![Retained::as_ptr(&window) as *const AnyObject, setAcceptsMouseMovedEvents: true];

            let view = WindowView::init_with_frame(&inner, rect)?;
            let _: () = objc2::msg_send![
                Retained::as_ptr(&view) as *const AnyObject,
                setAutoresizingMask: NSAutoresizingMaskOptions::ViewHeightSizable
                    | NSAutoresizingMaskOptions::ViewWidthSizable
            ];

            let () = objc2::msg_send![
                Retained::as_ptr(&view) as *const AnyObject,
                setLayerContentsPlacement: NSViewLayerContentsPlacementTopLeft
            ];

            let wn: isize =
                objc2::msg_send![Retained::as_ptr(&window) as *const AnyObject, windowNumber];
            CGSSetWindowBackgroundBlurRadius(
                CGSMainConnectionID(),
                wn,
                config.macos_window_background_blur,
            );
            let _: () = objc2::msg_send![Retained::as_ptr(&window) as *const AnyObject, setContentView: Retained::as_ptr(&view) as *mut AnyObject];
            let _: () = objc2::msg_send![Retained::as_ptr(&window) as *const AnyObject, setDelegate: Retained::as_ptr(&view) as *mut AnyObject];

            let _: () =
                objc2::msg_send![Retained::as_ptr(&view) as *const AnyObject, setWantsLayer: true];
            let () = objc2::msg_send![
                Retained::as_ptr(&view) as *const AnyObject,
                setLayerContentsRedrawPolicy: NSViewLayerContentsRedrawDuringViewResize
            ];

            // register for drag and drop operations.
            let () = objc2::msg_send![
                Retained::as_ptr(&window) as *const AnyObject,
                registerForDraggedTypes:
                    {
                        let filenames_type = Retained::as_ptr(&nsstring("NSFilenamesPboardType")) as *const _ as *mut AnyObject;
                        let __r: *mut AnyObject = objc2::msg_send![objc2::class!(NSArray), arrayWithObject: filenames_type];
                        __r
                    }
            ];

            let frame: CGRect =
                objc2::msg_send![Retained::as_ptr(&view) as *const AnyObject, frame];
            let backing_frame: CGRect = objc2::msg_send![Retained::as_ptr(&view) as *const AnyObject, convertRectToBacking: frame];
            let width = backing_frame.size.width;
            let height = backing_frame.size.height;

            let dpi = dpi_for_window_screen(Retained::as_ptr(&window) as *mut AnyObject, &config)
                .unwrap_or(crate::DEFAULT_DPI * (backing_frame.size.width / frame.size.width))
                as usize;

            let weak_window = Weak::from_retained(&window);
            let window_handle = Window {
                id: window_id,
                ns_window: Retained::as_ptr(&window) as *mut AnyObject,
                ns_view: Retained::as_ptr(&view) as *mut AnyObject,
            };
            let window_inner = Rc::new(RefCell::new(WindowInner {
                window,
                view,
                config: config.clone(),
            }));
            inner.borrow_mut().window.replace(weak_window);
            conn.windows
                .borrow_mut()
                .insert(window_id, Rc::clone(&window_inner));

            inner
                .borrow_mut()
                .events
                .assign_window(window_handle.clone());

            window_handle.config_did_change(&config);

            // Synthesize a resize event immediately; this allows
            // the embedding application an opportunity to discover
            // the dpi and adjust for display scaling
            inner.borrow_mut().events.dispatch(WindowEvent::Resized {
                dimensions: Dimensions {
                    pixel_width: width as usize,
                    pixel_height: height as usize,
                    dpi,
                },
                window_state: WindowState::default(),
                live_resizing: false,
            });

            Ok(window_handle)
        }
    }
}

impl HasDisplayHandle for Window {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        unsafe {
            Ok(DisplayHandle::borrow_raw(RawDisplayHandle::AppKit(
                AppKitDisplayHandle::new(),
            )))
        }
    }
}

impl HasWindowHandle for Window {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let handle =
            AppKitWindowHandle::new(NonNull::new(self.ns_view as *mut _).expect("non-null"));
        unsafe { Ok(WindowHandle::borrow_raw(RawWindowHandle::AppKit(handle))) }
    }
}

/// @see https://developer.apple.com/documentation/appkit/nswindow/level
pub type NSWindowLevel = i64;

pub fn nswindow_level_to_window_level(nswindow_level: NSWindowLevel) -> WindowLevel {
    match nswindow_level {
        -1 => WindowLevel::AlwaysOnBottom,
        0 => WindowLevel::Normal,
        3 => WindowLevel::AlwaysOnTop,
        _ => panic!("Invalid window level: {}", nswindow_level),
    }
}

pub fn window_level_to_nswindow_level(level: WindowLevel) -> NSWindowLevel {
    match level {
        WindowLevel::AlwaysOnBottom => -1,
        WindowLevel::Normal => 0,
        WindowLevel::AlwaysOnTop => 3,
    }
}

#[async_trait(?Send)]
impl WindowOps for Window {
    async fn enable_opengl(&self) -> anyhow::Result<Rc<glium::backend::Context>> {
        let window_id = self.id;
        promise::spawn::spawn(async move {
            if let Some(handle) = Connection::get().unwrap().window_by_id(window_id) {
                let mut inner = handle.borrow_mut();
                inner.enable_opengl()
            } else {
                bail!("invalid window");
            }
        })
        .await
    }

    fn notify<T: Any + Send + Sync>(&self, t: T)
    where
        Self: Sized,
    {
        Connection::with_window_inner(self.id, move |inner| {
            if let Some(window_view) = WindowView::get_this(&*inner.view) {
                window_view
                    .inner
                    .borrow_mut()
                    .events
                    .dispatch(WindowEvent::Notification(Box::new(t)));
            }
            Ok(())
        });
    }

    fn close(&self) {
        Connection::with_window_inner(self.id, |inner| {
            inner.close();
            Ok(())
        });
    }

    fn focus(&self) {
        Connection::with_window_inner(self.id, |inner| {
            inner.focus();
            Ok(())
        });
    }

    fn hide(&self) {
        Connection::with_window_inner(self.id, |inner| {
            inner.hide();
            Ok(())
        });
    }

    fn show(&self) {
        Connection::with_window_inner(self.id, |inner| {
            inner.show();
            Ok(())
        });
    }

    fn set_cursor(&self, cursor: Option<MouseCursor>) {
        Connection::with_window_inner(self.id, move |inner| {
            let _ = inner.set_cursor(cursor);
            Ok(())
        });
    }

    fn invalidate(&self) {
        Connection::with_window_inner(self.id, |inner| {
            inner.invalidate();
            Ok(())
        });
    }

    fn set_title(&self, title: &str) {
        let title = title.to_owned();
        Connection::with_window_inner(self.id, move |inner| {
            inner.set_title(&title);
            Ok(())
        });
    }

    fn set_window_level(&self, level: WindowLevel) {
        Connection::with_window_inner(self.id, move |inner| {
            inner.set_window_level(level);
            Ok(())
        });
    }

    fn set_inner_size(&self, width: usize, height: usize) {
        Connection::with_window_inner(self.id, move |inner| {
            inner.set_inner_size(width, height);
            if let Some(window_view) = WindowView::get_this(&*inner.view) {
                window_view
                    .inner
                    .borrow_mut()
                    .events
                    .dispatch(WindowEvent::SetInnerSizeCompleted);
            }
            Ok(())
        });
    }

    fn set_window_position(&self, coords: ScreenPoint) {
        Connection::with_window_inner(self.id, move |inner| {
            inner.set_window_position(coords);
            Ok(())
        });
    }

    fn set_text_cursor_position(&self, cursor: Rect) {
        Connection::with_window_inner(self.id, move |inner| {
            inner.set_text_cursor_position(cursor);
            Ok(())
        });
    }

    fn get_clipboard(&self, _clipboard: Clipboard) -> Future<String> {
        Future::result(
            ClipboardContext::new()
                .read()
                .map_err(|e| anyhow!("Failed to get clipboard:{}", e)),
        )
    }

    fn set_clipboard(&self, _clipboard: Clipboard, text: String) {
        ClipboardContext::new().write(text).ok();
    }

    fn toggle_fullscreen(&self) {
        Connection::with_window_inner(self.id, move |inner| {
            inner.toggle_fullscreen();
            Ok(())
        });
    }

    fn maximize(&self) {
        Connection::with_window_inner(self.id, move |inner| {
            inner.maximize();
            Ok(())
        });
    }

    fn restore(&self) {
        Connection::with_window_inner(self.id, move |inner| {
            inner.restore();
            Ok(())
        });
    }

    fn set_resize_increments(&self, incr: ResizeIncrement) {
        Connection::with_window_inner(self.id, move |inner| {
            inner.set_resize_increments(incr);
            Ok(())
        });
    }

    fn config_did_change(&self, config: &ConfigHandle) {
        let config = config.clone();
        Connection::with_window_inner(self.id, move |inner| {
            inner.config_did_change(&config);
            Ok(())
        });
    }

    fn get_os_parameters(
        &self,
        config: &ConfigHandle,
        window_state: WindowState,
    ) -> anyhow::Result<Option<Parameters>> {
        // We implement this method primarily to provide Notch-avoidance for
        // systems with a notch.
        // We only need this for non-native full screen mode.

        let native_full_screen = {
            let style_mask: NSWindowStyleMask = unsafe {
                objc2::msg_send![self.ns_window as *const _ as *const AnyObject, styleMask]
            };
            style_mask.contains(NSWindowStyleMask::FullScreen)
        };

        let border_dimensions = if window_state.contains(WindowState::FULL_SCREEN)
            && !native_full_screen
            && !config.macos_fullscreen_extend_behind_notch
        {
            let main_screen: id = unsafe {
                let __r: *mut AnyObject = objc2::msg_send![objc2::class!(NSScreen), mainScreen];
                __r as id
            };
            let has_safe_area_insets: bool = unsafe {
                objc2::msg_send![main_screen as *const _ as *const AnyObject, respondsToSelector: objc2::sel!(safeAreaInsets)]
            };
            if has_safe_area_insets {
                #[derive(Debug)]
                struct NSEdgeInsets {
                    top: CGFloat,
                    left: CGFloat,
                    bottom: CGFloat,
                    right: CGFloat,
                }

                unsafe impl objc2::Encode for NSEdgeInsets {
                    const ENCODING: objc2::Encoding = objc2::Encoding::Struct(
                        "NSEdgeInsets",
                        &[
                            objc2::Encoding::Double,
                            objc2::Encoding::Double,
                            objc2::Encoding::Double,
                            objc2::Encoding::Double,
                        ],
                    );
                }
                let insets: NSEdgeInsets = unsafe {
                    objc2::msg_send![main_screen as *const _ as *const AnyObject, safeAreaInsets]
                };
                log::trace!("{:?}", insets);

                let scale = unsafe {
                    let frame: CGRect =
                        objc2::msg_send![main_screen as *const _ as *const AnyObject, frame];
                    let backing_frame: CGRect = objc2::msg_send![main_screen as *const _ as *const AnyObject, convertRectToBacking: frame];
                    backing_frame.size.height / frame.size.height
                };

                let top = (insets.top.ceil() * scale) as usize;
                Some(Border {
                    top: ULength::new(top),
                    left: ULength::new(insets.left.ceil() as usize),
                    right: ULength::new(insets.right.ceil() as usize),
                    bottom: ULength::new(insets.bottom.ceil() as usize),
                    color: crate::color::LinearRgba::with_components(0., 0., 0., 1.),
                })
            } else {
                None
            }
        } else {
            None
        };

        Ok(Some(Parameters {
            title_bar: TitleBar {
                padding_left: ULength::new(0),
                padding_right: ULength::new(0),
                height: None,
                font_and_size: None,
            },
            border_dimensions,
        }))
    }
}

/// Convert from a macOS screen coordinate with the origin in the bottom left
/// to a pixel coordinate with its origin in the top left
fn cartesian_to_screen_point(cartesian: CGPoint) -> ScreenPoint {
    unsafe {
        let screens: id = {
            let __r: *mut AnyObject = objc2::msg_send![objc2::class!(NSScreen), screens];
            __r as id
        };
        let primary: *mut AnyObject =
            objc2::msg_send![screens as *const _ as *const AnyObject, objectAtIndex: 0usize];
        let frame: CGRect = objc2::msg_send![primary as *const _ as *const AnyObject, frame];
        let backing_frame: CGRect =
            objc2::msg_send![primary as *const _ as *const AnyObject, convertRectToBacking: frame];
        let scale = backing_frame.size.height / frame.size.height;
        ScreenPoint::new(
            (cartesian.x * scale) as isize,
            ((frame.size.height - cartesian.y) * scale) as isize,
        )
    }
}

/// Convert from a pixel coordinate in the top left to a macOS screen
/// coordinate with its origin in the bottom left
fn screen_point_to_cartesian(point: ScreenPoint) -> CGPoint {
    unsafe {
        let screens: id = {
            let __r: *mut AnyObject = objc2::msg_send![objc2::class!(NSScreen), screens];
            __r as id
        };
        let primary: *mut AnyObject =
            objc2::msg_send![screens as *const _ as *const AnyObject, objectAtIndex: 0usize];
        let frame: CGRect = objc2::msg_send![primary as *const _ as *const AnyObject, frame];
        let backing_frame: CGRect =
            objc2::msg_send![primary as *const _ as *const AnyObject, convertRectToBacking: frame];
        let scale = backing_frame.size.height / frame.size.height;
        CGPoint::new(
            point.x as f64 / scale,
            frame.size.height - (point.y as f64 / scale),
        )
    }
}

impl WindowInner {
    fn enable_opengl(&mut self) -> anyhow::Result<Rc<glium::backend::Context>> {
        if let Some(window_view) = WindowView::get_this(&*self.view) {
            window_view.inner.borrow_mut().enable_opengl()
        } else {
            anyhow::bail!("window invalid");
        }
    }

    fn is_fullscreen(&mut self) -> bool {
        if self.is_native_fullscreen() {
            true
        } else if let Some(window_view) = WindowView::get_this(&*self.view) {
            window_view.inner.borrow().fullscreen.is_some()
        } else {
            false
        }
    }

    fn apply_decorations(&mut self) {
        if !self.is_fullscreen() {
            apply_decorations_to_window(
                &self.window,
                self.config.window_decorations,
                self.config.integrated_title_button_style,
            );
        }
    }

    fn toggle_native_fullscreen(&mut self) {
        unsafe {
            let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, toggleFullScreen: std::ptr::null::<AnyObject>()];
        }
    }

    fn is_native_fullscreen(&self) -> bool {
        let style_mask: NSWindowStyleMask = unsafe {
            objc2::msg_send![
                Retained::as_ptr(&self.window) as *const AnyObject,
                styleMask
            ]
        };
        style_mask.contains(NSWindowStyleMask::FullScreen)
    }

    /// If we were in native full screen mode, exit it and return true.
    /// Otherwise, return false
    fn exit_native_fullscreen(&mut self) -> bool {
        if self.is_native_fullscreen() {
            self.toggle_native_fullscreen();
            true
        } else {
            false
        }
    }

    /// If we were in simple full screen mode, exit it and return true.
    /// Otherwise, return false
    fn exit_simple_fullscreen(&mut self) -> bool {
        if let Some(window_view) = WindowView::get_this(&*self.view) {
            let is_fullscreen = window_view.inner.borrow().fullscreen.is_some();
            if is_fullscreen {
                self.toggle_simple_fullscreen();
            }
            is_fullscreen
        } else {
            false
        }
    }

    fn toggle_simple_fullscreen(&mut self) {
        let current_app: id = unsafe {
            let __r: *mut AnyObject =
                objc2::msg_send![objc2::class!(NSApplication), sharedApplication];
            __r as id
        };

        if let Some(window_view) = WindowView::get_this(&*self.view) {
            let fullscreen = window_view.inner.borrow_mut().fullscreen.take();
            match fullscreen {
                Some(saved_rect) => unsafe {
                    // Restore prior dimensions
                    let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, orderOut: std::ptr::null::<AnyObject>()];
                    apply_decorations_to_window(
                        &self.window,
                        self.config.window_decorations,
                        self.config.integrated_title_button_style,
                    );
                    let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, setFrame: saved_rect, display: true];
                    let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, makeKeyAndOrderFront: std::ptr::null::<AnyObject>()];
                    let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, setOpaque: false];
                    let _: () = objc2::msg_send![
                        current_app as *const _ as *const AnyObject,
                        setPresentationOptions: NSApplicationPresentationOptions::empty()
                    ];
                },
                None => unsafe {
                    // Go full screen
                    let saved_rect_cg: CGRect =
                        objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, frame];
                    window_view
                        .inner
                        .borrow_mut()
                        .fullscreen
                        .replace(saved_rect_cg);

                    let main_screen: id = {
                        let __r: *mut AnyObject =
                            objc2::msg_send![objc2::class!(NSScreen), mainScreen];
                        __r as id
                    };
                    let screen_rect: CGRect =
                        objc2::msg_send![main_screen as *const _ as *const AnyObject, frame];

                    let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, orderOut: std::ptr::null::<AnyObject>()];
                    let _: () = objc2::msg_send![
                        Retained::as_ptr(&self.window) as *const AnyObject,
                        setStyleMask: NSWindowStyleMask::Borderless
                    ];
                    let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, setFrame: screen_rect, display: true];
                    let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, makeKeyAndOrderFront: std::ptr::null::<AnyObject>()];
                    let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, setOpaque: true];
                    let _: () = objc2::msg_send![
                        current_app as *const _ as *const AnyObject,
                        setPresentationOptions:
                            NSApplicationPresentationOptions::AutoHideMenuBar
                                | NSApplicationPresentationOptions::AutoHideDock
                    ];
                },
            }
        }
    }

    fn update_window_shadow(&mut self) {
        let is_opaque = self.config.window_background_opacity >= 1.0;
        unsafe {
            let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, setOpaque: is_opaque];
            // when transparent, also turn off the window shadow,
            // because having the shadow enabled seems to correlate
            // with ghostly remnants see:
            // https://github.com/termsurf/termsurf/issues/310.
            // But allow overriding the shadows independent of opacity as well:
            // <https://github.com/termsurf/termsurf/issues/2669>
            let shadow = if self
                .config
                .window_decorations
                .contains(WindowDecorations::MACOS_FORCE_ENABLE_SHADOW)
            {
                true
            } else if self
                .config
                .window_decorations
                .contains(WindowDecorations::MACOS_FORCE_DISABLE_SHADOW)
            {
                false
            } else {
                is_opaque
            };
            let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, setHasShadow: shadow];
        }
    }

    fn update_titlebar_background(&self) {
        if !self
            .config
            .window_decorations
            .contains(WindowDecorations::MACOS_USE_BACKGROUND_COLOR_AS_TITLEBAR_COLOR)
        {
            return;
        }

        // Set the titlebar background to the theme color falling back to black if there is no
        // specified color scheme
        let color = self
            .config
            .resolved_palette
            .background
            .unwrap_or(RgbaColor::from(SrgbaTuple(0., 0., 0., 255.)));

        unsafe {
            if let Some(titlebar_view_container) = get_titlebar_view_container(&self.window) {
                let layer: id = {
                    let __r: *mut AnyObject = objc2::msg_send![
                        Retained::as_ptr(&titlebar_view_container) as *const AnyObject,
                        layer
                    ];
                    __r as id
                };

                if layer.is_null() {
                    return;
                }

                // We need to make sure to convert the config color into an sRGB CGColor or the color will be slightly off
                let srgb_cgcolor = objc2_core_graphics::CGColor::new_srgb(
                    color.0.into(),
                    color.1.into(),
                    color.2.into(),
                    color.3.into(),
                );

                let _: () = objc2::msg_send![layer as *const _ as *const AnyObject, setBackgroundColor: &*srgb_cgcolor];
            } else {
                log::trace!("failed to get titlebar view container from window");
            }
        }
    }

    fn update_window_background_blur(&mut self) {
        unsafe {
            let wn: isize = objc2::msg_send![
                Retained::as_ptr(&self.window) as *const AnyObject,
                windowNumber
            ];
            CGSSetWindowBackgroundBlurRadius(
                CGSMainConnectionID(),
                wn,
                self.config.macos_window_background_blur,
            );
        }
    }
}

impl WindowInner {
    fn show(&mut self) {
        unsafe {
            let current_app: id = {
                let __r: *mut AnyObject =
                    objc2::msg_send![objc2::class!(NSRunningApplication), currentApplication];
                __r as id
            };
            let _: bool = objc2::msg_send![
                current_app as *const _ as *const AnyObject,
                activateWithOptions: 2usize
            ];

            // Stupid hack: adjust the window style mask and set it back
            // to what it was.
            // Without this, the CAMetalLayer used by webgpu seems to get
            // stuck with a scale factor of 2 despite us having configured 1.
            let _: () = objc2::msg_send![
                Retained::as_ptr(&self.window) as *const AnyObject,
                setStyleMask: NSWindowStyleMask::Borderless
            ];

            apply_decorations_to_window(
                &self.window,
                self.config.window_decorations,
                self.config.integrated_title_button_style,
            );

            self.update_titlebar_background();

            let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, makeKeyAndOrderFront: std::ptr::null::<AnyObject>()];
        }
    }

    fn close(&mut self) {
        unsafe {
            let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, close];
        }
    }

    fn focus(&mut self) {
        unsafe {
            let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, makeKeyAndOrderFront: std::ptr::null::<AnyObject>()];
        }
    }

    fn hide(&mut self) {
        unsafe {
            let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, miniaturize: Retained::as_ptr(&self.window) as *mut AnyObject];
            // We could literally set it invisible like this, but
            // then there is no UI to make it visible again later.
            //let () = msg_send![*self.window, setIsVisible: NO];
        }
    }

    fn set_cursor(&mut self, cursor: Option<MouseCursor>) {
        unsafe {
            let ns_cursor_cls = AnyClass::get(c"NSCursor").unwrap();
            if let Some(cursor) = cursor {
                // Unconditionally apply the requested cursor, as there are
                // cases where macOS can decide to change the cursor to something
                // that we don't know about.
                let instance: id = match cursor {
                    MouseCursor::Arrow => {
                        let __r: *mut AnyObject = objc2::msg_send![ns_cursor_cls, arrowCursor];
                        __r as id
                    }
                    MouseCursor::Text => {
                        let __r: *mut AnyObject = objc2::msg_send![ns_cursor_cls, IBeamCursor];
                        __r as id
                    }
                    MouseCursor::Hand => {
                        let __r: *mut AnyObject =
                            objc2::msg_send![ns_cursor_cls, pointingHandCursor];
                        __r as id
                    }
                    MouseCursor::SizeUpDown => {
                        let __r: *mut AnyObject =
                            objc2::msg_send![ns_cursor_cls, resizeUpDownCursor];
                        __r as id
                    }
                    MouseCursor::SizeLeftRight => {
                        let __r: *mut AnyObject =
                            objc2::msg_send![ns_cursor_cls, resizeLeftRightCursor];
                        __r as id
                    }
                };
                let () = objc2::msg_send![ns_cursor_cls, setHiddenUntilMouseMoves: false];
                let () = objc2::msg_send![instance as *const _ as *const AnyObject, set];
            } else {
                let () = objc2::msg_send![ns_cursor_cls, setHiddenUntilMouseMoves: true];
            }
        }
    }

    fn invalidate(&mut self) {
        unsafe {
            let () = objc2::msg_send![Retained::as_ptr(&self.view) as *const AnyObject, setNeedsDisplay: true];
            if let Some(window_view) = WindowView::get_this(&*self.view) {
                window_view.inner.borrow_mut().invalidated = true;
            }
        }
    }
    fn set_title(&mut self, title: &str) {
        let title = nsstring(title);
        unsafe {
            let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, setTitle: Retained::as_ptr(&title) as *const _ as *mut AnyObject];
        }
    }

    fn set_window_level(&mut self, level: WindowLevel) {
        unsafe {
            let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, setLevel: window_level_to_nswindow_level(level)];
            // Dispatch a resize event with the updated window state
            WindowView::did_resize(
                Retained::as_ptr(&self.view) as *mut AnyObject,
                objc2::sel!(windowDidResize:),
                std::ptr::null_mut(),
            );
        }
    }

    fn set_inner_size(&mut self, width: usize, height: usize) {
        unsafe {
            let frame: CGRect =
                objc2::msg_send![Retained::as_ptr(&self.view) as *const AnyObject, frame];
            let backing_frame: CGRect = objc2::msg_send![Retained::as_ptr(&self.view) as *const AnyObject, convertRectToBacking: frame];
            let scale = backing_frame.size.width / frame.size.width;

            let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, setContentSize: CGSize::new(width as f64 / scale, height as f64 / scale)];

            // setContentSize_ doesn't explicitly invalidate,
            // so we need to do it ourselves
            self.invalidate();
        }
    }

    fn set_window_position(&self, coords: ScreenPoint) {
        set_window_position(Retained::as_ptr(&self.window) as *mut AnyObject, coords);
    }

    fn set_text_cursor_position(&mut self, cursor: Rect) {
        if let Some(window_view) = WindowView::get_this(&*self.view) {
            window_view.inner.borrow_mut().text_cursor_position = cursor;
        }
        if self.config.use_ime {
            unsafe {
                let input_context: id = {
                    let __r: *mut AnyObject = objc2::msg_send![
                        Retained::as_ptr(&self.view) as *const AnyObject,
                        inputContext
                    ];
                    __r as id
                };
                let () = objc2::msg_send![
                    input_context as *const _ as *const AnyObject,
                    invalidateCharacterCoordinates
                ];
            }
        }
    }

    fn is_zoomed(&self) -> bool {
        unsafe { objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, isZoomed] }
    }

    fn maximize(&mut self) {
        if !self.is_zoomed() {
            unsafe {
                let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, zoom: std::ptr::null::<AnyObject>()];
            }
        }
    }

    fn restore(&mut self) {
        if self.is_zoomed() {
            unsafe {
                let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, zoom: std::ptr::null::<AnyObject>()];
            }
        }
    }

    fn toggle_fullscreen(&mut self) {
        let native_fullscreen = self.config.native_macos_fullscreen_mode;

        // If they changed their config since going full screen, be sure
        // to undo whichever fullscreen mode they had active rather than
        // trying to undo the one they have configured.

        if native_fullscreen {
            if !self.exit_simple_fullscreen() {
                self.toggle_native_fullscreen();
            }
        } else {
            if !self.exit_native_fullscreen() {
                self.toggle_simple_fullscreen();
            }
        }
    }

    fn set_resize_increments(&self, incr: ResizeIncrement) {
        let min_width = incr.base_width + incr.x;
        let min_height = incr.base_height + incr.y;
        unsafe {
            let _: () = objc2::msg_send![Retained::as_ptr(&self.window) as *const AnyObject, setResizeIncrements: CGSize::new(incr.x.into(), incr.y.into())];
            let () = objc2::msg_send![
                Retained::as_ptr(&self.window) as *const AnyObject,
                setContentMinSize: CGSize::new(min_width.into(), min_height.into())
            ];
        }
    }

    fn config_did_change(&mut self, config: &ConfigHandle) {
        let dpi_changed =
            self.config.dpi != config.dpi || self.config.dpi_by_screen != config.dpi_by_screen;

        self.config = config.clone();
        if let Some(window_view) = WindowView::get_this(&*self.view) {
            let mut inner = window_view.inner.borrow_mut();
            inner.config = config.clone();
            if dpi_changed {
                inner.screen_changed = true;
            }
        }
        self.update_window_shadow();
        self.update_window_background_blur();
        self.update_titlebar_background();
        self.apply_decorations();
    }
}

fn effective_decorations(
    mut decorations: WindowDecorations,
    integrated_title_button_style: IntegratedTitleButtonStyle,
) -> WindowDecorations {
    if integrated_title_button_style != IntegratedTitleButtonStyle::MacOsNative {
        decorations.remove(WindowDecorations::INTEGRATED_BUTTONS);
    }
    decorations
}

fn apply_decorations_to_window(
    window: &Retained<AnyObject>,
    decorations: WindowDecorations,
    integrated_title_button_style: IntegratedTitleButtonStyle,
) {
    let mask = decoration_to_mask(decorations, integrated_title_button_style);
    let decorations = effective_decorations(decorations, integrated_title_button_style);
    unsafe {
        let _: () = objc2::msg_send![
            Retained::as_ptr(window) as *const AnyObject,
            setStyleMask: mask
        ];

        let hidden = !(decorations.contains(WindowDecorations::TITLE)
            || decorations.contains(WindowDecorations::INTEGRATED_BUTTONS));

        for titlebar_button in &[
            NS_WINDOW_MINIATURIZE_BUTTON,
            NS_WINDOW_CLOSE_BUTTON,
            NS_WINDOW_ZOOM_BUTTON,
        ] {
            let button: id = {
                let __r: *mut AnyObject = objc2::msg_send![Retained::as_ptr(window) as *const AnyObject, standardWindowButton: *titlebar_button];
                __r as id
            };
            let _: () = objc2::msg_send![button as *const _ as *const AnyObject, setHidden: hidden];
        }

        let title_visibility: isize = if decorations.contains(WindowDecorations::TITLE) {
            0 // NSWindowTitleVisible
        } else {
            1 // NSWindowTitleHidden
        };
        let _: () = objc2::msg_send![Retained::as_ptr(window) as *const AnyObject, setTitleVisibility: title_visibility];

        if decorations.contains(WindowDecorations::INTEGRATED_BUTTONS)
            || decorations.contains(WindowDecorations::MACOS_USE_BACKGROUND_COLOR_AS_TITLEBAR_COLOR)
        {
            let _: () = objc2::msg_send![Retained::as_ptr(window) as *const AnyObject, setTitlebarAppearsTransparent: true];
        } else {
            let _: () = objc2::msg_send![Retained::as_ptr(window) as *const AnyObject, setTitlebarAppearsTransparent: hidden];
        }
    }
}

fn decoration_to_mask(
    decorations: WindowDecorations,
    integrated_title_button_style: IntegratedTitleButtonStyle,
) -> NSWindowStyleMask {
    let decorations = effective_decorations(decorations, integrated_title_button_style);
    let decorations = decorations.difference(
        WindowDecorations::MACOS_FORCE_DISABLE_SHADOW
            | WindowDecorations::MACOS_FORCE_ENABLE_SHADOW,
    );
    if decorations == WindowDecorations::TITLE | WindowDecorations::RESIZE {
        NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable
    } else if decorations
        == WindowDecorations::MACOS_FORCE_SQUARE_CORNERS | WindowDecorations::RESIZE
    {
        NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable
            | NSWindowStyleMask::FullSizeContentView
    } else if decorations == WindowDecorations::RESIZE
        || decorations == WindowDecorations::INTEGRATED_BUTTONS
        || decorations == WindowDecorations::INTEGRATED_BUTTONS | WindowDecorations::RESIZE
    {
        NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable
            | NSWindowStyleMask::FullSizeContentView
    } else if decorations == WindowDecorations::NONE {
        NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::FullSizeContentView
    } else if decorations == WindowDecorations::TITLE {
        NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable
    } else if decorations == WindowDecorations::MACOS_FORCE_SQUARE_CORNERS {
        NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::FullSizeContentView
    } else {
        NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable
    }
}

unsafe fn get_view_class_name(id: id) -> Option<String> {
    if id.is_null() {
        return None;
    }

    let class_name: id = {
        let __r: *mut AnyObject = objc2::msg_send![id as *const _ as *const AnyObject, className];
        __r as id
    };

    if class_name.is_null() {
        return None;
    }

    let utf8: *const std::ffi::c_char =
        objc2::msg_send![class_name as *const _ as *const AnyObject, UTF8String];
    let cstr = CStr::from_ptr(utf8).to_str();

    match cstr {
        Ok(s) => Some(s.to_string()),
        Err(_) => None,
    }
}

fn get_titlebar_view_container(window: &Retained<AnyObject>) -> Option<Retained<AnyObject>> {
    // The view container for the titlebar on macos is found next to the primary window view
    // so we need to traverse up to the super view to find it
    let super_view = get_view_superview(window)?;

    let sub_views = get_view_subviews(&super_view)?;

    let count: usize =
        unsafe { objc2::msg_send![Retained::as_ptr(&sub_views) as *const AnyObject, count] };

    for i in 0..count {
        let sub_view: id = unsafe {
            let __r: *mut AnyObject = objc2::msg_send![Retained::as_ptr(&sub_views) as *const AnyObject, objectAtIndex: i];
            __r as id
        };

        if sub_view.is_null() {
            continue;
        }

        let class_name = unsafe { get_view_class_name(sub_view)? };

        if class_name == TITLEBAR_VIEW_NAME {
            let titlebar_view = unsafe { Retained::retain(sub_view) };
            return titlebar_view;
        }
    }

    None
}

fn get_view_superview(window: &Retained<AnyObject>) -> Option<Retained<AnyObject>> {
    let super_view_id: id = unsafe {
        let content_view: *mut AnyObject =
            objc2::msg_send![Retained::as_ptr(window) as *const AnyObject, contentView];
        let __r: *mut AnyObject =
            objc2::msg_send![content_view as *const _ as *const AnyObject, superview];
        __r as id
    };

    if super_view_id.is_null() {
        return None;
    }

    unsafe { Retained::retain(super_view_id) }
}

fn get_view_subviews(view: &Retained<AnyObject>) -> Option<Retained<AnyObject>> {
    let sub_views_id: id = unsafe {
        let __r: *mut AnyObject =
            objc2::msg_send![Retained::as_ptr(view) as *const AnyObject, subviews];
        __r as id
    };
    if sub_views_id.is_null() {
        return None;
    }

    unsafe { Retained::retain(sub_views_id) }
}

#[derive(Debug)]
struct DeadKeyState {
    /// The private dead key state preserved from UCKeyTranslate
    dead_state: u32,
}

struct Inner {
    events: WindowEventSender,
    view_id: Option<Weak<AnyObject>>,
    window: Option<Weak<AnyObject>>,
    screen_changed: bool,
    paint_throttled: bool,
    window_id: usize,
    invalidated: bool,
    gl_context_pair: Option<GlContextPair>,
    text_cursor_position: Rect,
    tracking_rect_tag: isize,
    hscroll_remainder: f64,
    vscroll_remainder: f64,
    last_wheel: Instant,
    /// We use this to avoid double-emitting events when
    /// procesing key-up events.
    key_is_down: Option<bool>,

    /// First in a dead-key sequence
    dead_pending: Option<DeadKeyState>,

    /// When using simple fullscreen mode, this tracks
    /// the window dimensions that need to be restored
    fullscreen: Option<CGRect>,

    config: ConfigHandle,

    /// Used to signal when IME really just swallowed a key
    ime_state: ImeDisposition,
    /// Captures the last event that had ImeDisposition::Acted,
    /// so that we can use it to generate a repeat in the cases
    /// where the IME mysteriously swallows repeats but only
    /// for certain keys.
    ime_last_event: Option<KeyEvent>,

    /// Whether we're in live resize
    live_resizing: bool,

    ime_text: String,
}

#[repr(C)]
pub struct __InputSource {
    _dummy: i32,
}
pub type InputSourceRef = *const __InputSource;

declare_TCFType!(InputSource, InputSourceRef);
impl_TCFType!(InputSource, InputSourceRef, TISInputSourceGetTypeID);

#[repr(C)]
struct UCKeyboardLayout {
    _dummy: i32,
}

type UniCharCount = std::os::raw::c_ulong;

/// key is going down
#[allow(non_upper_case_globals)]
const kUCKeyActionDown: u16 = 0;
/// key is going up
#[allow(non_upper_case_globals, dead_code)]
const kUCKeyActionUp: u16 = 1;
/// auto-key down
#[allow(non_upper_case_globals, dead_code)]
const kUCKeyActionAutoKey: u16 = 2;
/// get information for key display (as in Key Caps)
#[allow(non_upper_case_globals)]
const kUCKeyActionDisplay: u16 = 3;

extern "C" {
    fn TISInputSourceGetTypeID() -> CFTypeID;
    fn TISCopyCurrentKeyboardInputSource() -> InputSourceRef;
    fn TISGetInputSourceProperty(source: InputSourceRef, propertyKey: CFStringRef) -> CFDataRef;

    static kTISPropertyUnicodeKeyLayoutData: CFStringRef;

    fn UCKeyTranslate(
        layout: *const UCKeyboardLayout,
        virtualKeyCode: u16,
        keyAction: u16,
        modifierKeyState: u32,
        keyboardType: u32,
        keyTranslateOptions: u32,
        deadKeyState: *mut u32,
        maxStringLength: UniCharCount,
        actualStringLength: *mut UniCharCount,
        unicodeString: *mut UniChar,
    ) -> u32;

    fn LMGetKbdType() -> u8;
}

#[derive(Debug)]
enum TranslateStatus {
    Composing(String),
    Composed(String),
    NotDead,
}

/// Represents the current keyboard layout.
/// Holds state needed to perform keymap translation.
struct Keyboard {
    _kbd: InputSource,
    layout_data: Option<CFData>,
}

/// Slightly more intelligible parameters for keymap translation
struct TranslateParams {
    virtual_key_code: u16,
    modifier_flags: NSEventModifierFlags,
    dead_state: u32,
    ignore_dead_keys: bool,
    display: bool,
}

/// The results of a keymap translation
#[derive(Debug)]
struct TranslateResults {
    dead_state: u32,
    text: String,
}

impl Keyboard {
    pub fn new() -> Self {
        let _kbd =
            unsafe { InputSource::wrap_under_create_rule(TISCopyCurrentKeyboardInputSource()) };

        let layout_data = unsafe {
            let data = TISGetInputSourceProperty(
                _kbd.as_concrete_TypeRef(),
                kTISPropertyUnicodeKeyLayoutData,
            );
            if data.is_null() {
                None
            } else {
                Some(CFData::wrap_under_get_rule(data))
            }
        };
        Self { _kbd, layout_data }
    }

    /// A wrapper around UCKeyTranslate
    pub fn translate(&self, params: TranslateParams) -> anyhow::Result<TranslateResults> {
        let layout_data = match &self.layout_data {
            Some(data) => unsafe {
                CFDataGetBytePtr(data.as_concrete_TypeRef()) as *const UCKeyboardLayout
            },
            None => std::ptr::null(),
        };

        let modifier_key_state: u32 = (params.modifier_flags.0 >> 16) as u32 & 0xFF;

        let kbd_type = unsafe { LMGetKbdType() } as _;
        #[allow(non_upper_case_globals)]
        const kUCKeyTranslateNoDeadKeysBit: u32 = 0;

        let mut unicode_buffer = [0u16; 32];
        let mut length = 0;
        let mut dead_state = params.dead_state;
        unsafe {
            UCKeyTranslate(
                layout_data,
                params.virtual_key_code,
                if params.display {
                    kUCKeyActionDisplay
                } else {
                    kUCKeyActionDown
                },
                modifier_key_state,
                kbd_type,
                if params.ignore_dead_keys {
                    1 << kUCKeyTranslateNoDeadKeysBit
                } else {
                    0
                },
                &mut dead_state,
                unicode_buffer.len() as _,
                &mut length,
                unicode_buffer.as_mut_ptr(),
            )
        };

        let text = String::from_utf16(unsafe {
            std::slice::from_raw_parts(unicode_buffer.as_mut_ptr(), length as _)
        })?;

        Ok(TranslateResults { text, dead_state })
    }
}

impl Inner {
    fn enable_opengl(&mut self) -> anyhow::Result<Rc<glium::backend::Context>> {
        let view = self
            .view_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("view_id not set"))?
            .load()
            .ok_or_else(|| anyhow::anyhow!("NSView has been deallocated"))?;
        let glium_context = GlContextPair::create(Retained::as_ptr(&view) as id)?;

        self.gl_context_pair.replace(glium_context.clone());

        Ok(glium_context.context)
    }

    /// <https://stackoverflow.com/a/22677690>
    /// <https://stackoverflow.com/a/12548163>
    /// <https://stackoverflow.com/a/8263841>
    /// <https://developer.apple.com/documentation/coreservices/1390584-uckeytranslate?language=objc>
    fn translate_key_event(
        &mut self,
        virtual_key_code: u16,
        modifier_flags: NSEventModifierFlags,
    ) -> anyhow::Result<TranslateStatus> {
        let keyboard = Keyboard::new();

        let mods = key_modifiers(modifier_flags);

        let config = &self.config;

        let use_dead_keys = if !config.use_dead_keys {
            false
        } else if mods.contains(Modifiers::LEFT_ALT) {
            config.send_composed_key_when_left_alt_is_pressed
        } else if mods.contains(Modifiers::RIGHT_ALT) {
            config.send_composed_key_when_right_alt_is_pressed
        } else {
            true
        };

        if let Some(DeadKeyState { dead_state }) = self.dead_pending.take() {
            let result = keyboard.translate(TranslateParams {
                virtual_key_code,
                modifier_flags,
                dead_state,
                ignore_dead_keys: false,
                display: true,
            })?;

            // If length == 0 it means that they double-pressed the dead key.
            // We treat that the same as the dead key disabled state:
            // we want to clock through a space keypress so that we clear
            // the state and output the original keypress.
            let generate_space = !use_dead_keys || result.text.len() == 0;

            if generate_space {
                // synthesize a SPACE press to
                // elicit the underlying key code and get out
                // of the dead key state
                let result = keyboard.translate(TranslateParams {
                    virtual_key_code,
                    modifier_flags,
                    dead_state: result.dead_state,
                    ignore_dead_keys: false,
                    display: false,
                })?;
                Ok(TranslateStatus::Composed(result.text))
            } else {
                Ok(TranslateStatus::Composed(result.text))
            }
        } else if use_dead_keys {
            let result = keyboard.translate(TranslateParams {
                virtual_key_code,
                modifier_flags,
                dead_state: 0,
                ignore_dead_keys: false,
                display: false,
            })?;

            self.dead_pending.replace(DeadKeyState {
                dead_state: result.dead_state,
            });

            // Get the non-dead-key rendition to show as the composing state
            let composing = keyboard.translate(TranslateParams {
                virtual_key_code,
                modifier_flags,
                dead_state: 0,
                ignore_dead_keys: true,
                display: true,
            })?;

            Ok(TranslateStatus::Composing(composing.text))
        } else {
            Ok(TranslateStatus::NotDead)
        }
    }
}

const VIEW_CLS_CNAME: &CStr = c"WezboardWindowView";
const WINDOW_CLS_CNAME: &CStr = c"WezboardWindow";
const TITLEBAR_VIEW_NAME: &str = "NSTitlebarContainerView";

struct WindowView {
    inner: Rc<RefCell<Inner>>,
}

// SAFETY: caller must ensure `obj` is a WezboardWindowView instance with the
// VIEW_CLS_CNAME ivar, and the returned pointer is only used while `obj` is alive.
unsafe fn get_view_ivar(obj: &AnyObject) -> *mut c_void {
    let ivar = obj.class().instance_variable(VIEW_CLS_CNAME).unwrap();
    let ptr = (obj as *const AnyObject as *const u8).offset(ivar.offset());
    *(ptr as *const *mut c_void)
}

// SAFETY: caller must ensure `obj` is a WezboardWindowView instance with the
// VIEW_CLS_CNAME ivar, and `value` is a valid pointer to a WindowView.
unsafe fn set_view_ivar(obj: &mut AnyObject, value: *mut c_void) {
    let ivar = obj.class().instance_variable(VIEW_CLS_CNAME).unwrap();
    let ptr = (obj as *mut AnyObject as *mut u8).offset(ivar.offset());
    *(ptr as *mut *mut c_void) = value;
}

pub fn superclass(this: &AnyObject) -> &'static AnyClass {
    unsafe {
        let superclass: *const AnyClass =
            objc2::msg_send![this as *const _ as *const _ as *const AnyObject, superclass];
        &*superclass
    }
}

fn dpi_for_window_screen(ns_window: *mut AnyObject, config: &ConfigHandle) -> Option<f64> {
    if config.dpi_by_screen.is_empty() {
        return config.dpi;
    }

    let screen: id = unsafe {
        let __r: *mut AnyObject =
            objc2::msg_send![ns_window as *const _ as *const AnyObject, screen];
        __r as id
    };
    let info = crate::os::macos::connection::nsscreen_to_screen_info(unsafe {
        &*(screen as *const objc2_app_kit::NSScreen)
    });

    config.dpi_by_screen.get(&info.name).copied()
}

#[allow(clippy::identity_op)]
fn decode_mouse_buttons(mask: u64) -> MouseButtons {
    let mut buttons = MouseButtons::NONE;

    if (mask & (1 << 0)) != 0 {
        buttons |= MouseButtons::LEFT;
    }
    if (mask & (1 << 1)) != 0 {
        buttons |= MouseButtons::RIGHT;
    }
    if (mask & (1 << 2)) != 0 {
        buttons |= MouseButtons::MIDDLE;
    }
    if (mask & (1 << 3)) != 0 {
        buttons |= MouseButtons::X1;
    }
    if (mask & (1 << 4)) != 0 {
        buttons |= MouseButtons::X2;
    }
    buttons
}

fn key_modifiers(flags: NSEventModifierFlags) -> Modifiers {
    let mut mods = Modifiers::NONE;

    if flags.contains(NSEventModifierFlags::Shift) {
        mods |= Modifiers::SHIFT;
    }
    if flags.contains(NSEventModifierFlags::Option) && (flags.0 & 0x20) != 0 {
        mods |= Modifiers::LEFT_ALT | Modifiers::ALT;
    }
    if flags.contains(NSEventModifierFlags::Option) && (flags.0 & 0x40) != 0 {
        mods |= Modifiers::RIGHT_ALT | Modifiers::ALT;
    }
    if flags.contains(NSEventModifierFlags::Control) {
        mods |= Modifiers::CTRL;
    }
    if flags.contains(NSEventModifierFlags::Command) {
        mods |= Modifiers::SUPER;
    }

    mods
}

/// We register our own subclass of NSWindow so that we can override
/// canBecomeKeyWindow so that our simple fullscreen style can keep
/// focus once the titlebar has been removed; the default behavior of
/// NSWindow is to reject focus when it doesn't have a titlebar!
fn get_window_class() -> &'static AnyClass {
    AnyClass::get(WINDOW_CLS_CNAME).unwrap_or_else(|| {
        let mut cls = ClassBuilder::new(WINDOW_CLS_CNAME, AnyClass::get(c"NSWindow").unwrap())
            .expect("Unable to register Window class");

        extern "C" fn yes(_: *mut AnyObject, _: Sel) -> Bool {
            Bool::YES
        }

        unsafe {
            cls.add_method(
                objc2::sel!(canBecomeKeyWindow),
                yes as extern "C" fn(*mut AnyObject, Sel) -> Bool,
            );
            cls.add_method(
                objc2::sel!(canBecomeMainWindow),
                yes as extern "C" fn(*mut AnyObject, Sel) -> Bool,
            );
        }

        cls.register()
    })
}

impl WindowView {
    extern "C" fn dealloc(this: *mut AnyObject, _sel: Sel) {
        let this = unsafe { &mut *(this as *mut AnyObject) };
        Self::drop_inner(this);
        unsafe {
            let superclass = superclass(this);
            let () = objc2::msg_send![
                super(this as *const _ as *const _ as *const AnyObject, superclass),
                dealloc
            ];
        }
    }

    fn drop_inner(this: &mut AnyObject) {
        unsafe {
            let myself: *mut c_void = get_view_ivar(this);
            set_view_ivar(this, std::ptr::null_mut());

            if !myself.is_null() {
                let myself = Box::from_raw(myself as *mut Self);
                drop(myself);
            }
        }
    }

    // Called by the inputContext manager when the IME processes events.
    // We need to translate the selector back into appropriate key
    // sequences
    extern "C" fn do_command_by_selector(this_raw: *mut AnyObject, _sel: Sel, a_selector: Sel) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let selector = format!("{:?}", a_selector);
        log::trace!("do_command_by_selector {:?}", selector);

        if let Some(myself) = Self::get_this(this) {
            let mut inner = myself.inner.borrow_mut();
            inner.ime_state = ImeDisposition::Continue;
            inner.ime_last_event.take();
        }
    }

    extern "C" fn has_marked_text(this_raw: *mut AnyObject, _sel: Sel) -> Bool {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        if let Some(myself) = Self::get_this(this) {
            let inner = myself.inner.borrow();
            if inner.ime_text.is_empty() {
                Bool::NO
            } else {
                Bool::YES
            }
        } else {
            Bool::NO
        }
    }

    extern "C" fn marked_range(this_raw: *mut AnyObject, _sel: Sel) -> NSRange {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        if let Some(myself) = Self::get_this(this) {
            let inner = myself.inner.borrow();
            log::trace!("marked_range {:?}", inner.ime_text);
            if inner.ime_text.is_empty() {
                NSRange::new(isize::MAX as usize, 0)
            } else {
                NSRange::new(0, inner.ime_text.len())
            }
        } else {
            NSRange::new(isize::MAX as usize, 0)
        }
    }

    extern "C" fn selected_range(_this_raw: *mut AnyObject, _sel: Sel) -> NSRange {
        let _this = unsafe { &mut *(_this_raw as *mut AnyObject) };
        NSRange::new(isize::MAX as usize, 0)
    }

    // Called by the IME when inserting composed text and/or emoji
    extern "C" fn insert_text_replacement_range(
        this_raw: *mut AnyObject,
        _sel: Sel,
        astring_ao: *mut AnyObject,
        replacement_range: NSRange,
    ) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let astring = astring_ao as id;
        let s = unsafe { nsstring_to_str(astring as *mut AnyObject) };
        log::trace!(
            "insert_text_replacement_range {} {:?}",
            s,
            replacement_range
        );
        if let Some(myself) = Self::get_this(this) {
            let mut inner = myself.inner.borrow_mut();

            let key_is_down = inner.key_is_down.take().unwrap_or(true);

            let key = KeyCode::composed(s);

            let event = KeyEvent {
                key,
                modifiers: Modifiers::NONE,
                leds: KeyboardLedStatus::empty(),
                repeat_count: 1,
                key_is_down,
                raw: None,
            };

            inner.ime_text.clear();
            inner
                .events
                .dispatch(WindowEvent::AdviseDeadKeyStatus(DeadKeyStatus::None));
            inner.ime_last_event.replace(event.clone());
            inner.events.dispatch(WindowEvent::KeyEvent(event));
            inner.ime_state = ImeDisposition::Acted;
        }
    }

    extern "C" fn set_marked_text_selected_range_replacement_range(
        this_raw: *mut AnyObject,
        _sel: Sel,
        astring_ao: *mut AnyObject,
        selected_range: NSRange,
        replacement_range: NSRange,
    ) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let astring = astring_ao as id;
        let s = unsafe { nsstring_to_str(astring as *mut AnyObject) };
        log::trace!(
            "set_marked_text_selected_range_replacement_range {} {:?} {:?}",
            s,
            selected_range,
            replacement_range
        );
        if let Some(myself) = Self::get_this(this) {
            let mut inner = myself.inner.borrow_mut();
            inner.ime_text = s.to_string();

            /*
            let key_is_down = inner.key_is_down.take().unwrap_or(true);

            let key = KeyCode::composed(s);

            let event = KeyEvent {
                key,
                modifiers: Modifiers::NONE,
                repeat_count: 1,
                key_is_down,
            }
            .normalize_shift();

            inner.ime_last_event.replace(event.clone());
            inner.events.dispatch(WindowEvent::KeyEvent(event));
            */
            inner.ime_last_event.take();
            inner.ime_state = ImeDisposition::Acted;
        }
    }

    extern "C" fn unmark_text(this_raw: *mut AnyObject, _sel: Sel) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        log::trace!("unmarkText");
        if let Some(myself) = Self::get_this(this) {
            let mut inner = myself.inner.borrow_mut();
            // FIXME: docs say to insert the text here,
            // but iterm doesn't... and we've never seen
            // this get called so far?
            inner.ime_text.clear();
            inner.ime_last_event.take();
            inner.ime_state = ImeDisposition::Acted;
        }
    }

    extern "C" fn valid_attributes_for_marked_text(
        _this_raw: *mut AnyObject,
        _sel: Sel,
    ) -> *mut AnyObject {
        let _this = unsafe { &mut *(_this_raw as *mut AnyObject) };
        // FIXME: returns NSArray<NSAttributedStringKey> *
        // log::trace!("valid_attributes_for_marked_text");
        // nil
        unsafe {
            let __r: *mut AnyObject = objc2::msg_send![objc2::class!(NSArray), arrayWithObjects: std::ptr::null::<*mut AnyObject>(), count: 0usize];
            __r
        }
    }

    extern "C" fn attributed_substring_for_proposed_range(
        _this: *mut AnyObject,
        _sel: Sel,
        _proposed_range: NSRange,
        _actual_range: *mut NSRange,
    ) -> *mut AnyObject {
        log::trace!(
            "attributedSubstringForProposedRange {:?} {:?}",
            _proposed_range,
            _actual_range
        );
        std::ptr::null_mut()
    }

    extern "C" fn character_index_for_point(
        _this: *mut AnyObject,
        _sel: Sel,
        _point: CGPoint,
    ) -> usize {
        isize::MAX as usize
    }

    extern "C" fn first_rect_for_character_range(
        this: *mut AnyObject,
        _sel: Sel,
        range: NSRange,
        actual: *mut NSRange,
    ) -> CGRect {
        let this_obj = unsafe { &mut *(this as *mut AnyObject) };
        // Returns a rect in screen coordinates; this is used to place
        // the input method editor
        log::trace!(
            "firstRectForCharacterRange: range:{:?} actual:{:?}",
            range,
            actual
        );
        let window: id = unsafe {
            let __r: *mut AnyObject = objc2::msg_send![this, window];
            __r as id
        };
        let frame: CGRect =
            unsafe { objc2::msg_send![window as *const _ as *const AnyObject, frame] };
        let content: CGRect = unsafe {
            objc2::msg_send![window as *const _ as *const AnyObject, contentRectForFrameRect: frame]
        };
        let backing_frame: CGRect = unsafe { objc2::msg_send![this, convertRectToBacking: frame] };
        let scale = frame.size.width / backing_frame.size.width;

        if let Some(this) = Self::get_this(this_obj) {
            let cursor_pos = this
                .inner
                .borrow()
                .text_cursor_position
                .to_f64()
                .scale(scale, scale);

            CGRect::new(
                CGPoint::new(
                    content.origin.x + cursor_pos.min_x(),
                    content.origin.y + content.size.height - cursor_pos.max_y(),
                ),
                CGSize::new(cursor_pos.size.width, cursor_pos.size.height),
            )
        } else {
            CGRect::new(
                CGPoint::new(frame.origin.x, frame.origin.y),
                CGSize::new(frame.size.width, frame.size.height),
            )
        }
    }

    extern "C" fn accepts_first_mouse(
        _this_raw: *mut AnyObject,
        _sel: Sel,
        _nsevent_ao: *mut AnyObject,
    ) -> Bool {
        Bool::YES
    }

    extern "C" fn accepts_first_responder(_this_raw: *mut AnyObject, _sel: Sel) -> Bool {
        Bool::YES
    }

    extern "C" fn view_did_change_effective_appearance(this_raw: *mut AnyObject, _sel: Sel) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        if let Some(this) = Self::get_this(this) {
            let appearance = Connection::get().unwrap().get_appearance();
            this.inner
                .borrow_mut()
                .events
                .dispatch(WindowEvent::AppearanceChanged(appearance));
        }
    }

    extern "C" fn update_tracking_areas(this_raw: *mut AnyObject, _sel: Sel) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let frame: CGRect =
            unsafe { objc2::msg_send![this as *const _ as *const AnyObject, frame] };

        if let Some(this) = Self::get_this(this) {
            let mut inner = this.inner.borrow_mut();
            if let Some(ref weak_view) = inner.view_id {
                if let Some(view) = weak_view.load() {
                    let tag = inner.tracking_rect_tag;
                    if tag != 0 {
                        unsafe {
                            let () = objc2::msg_send![Retained::as_ptr(&view) as *const AnyObject, removeTrackingRect: tag];
                        }
                    }

                    inner.tracking_rect_tag = unsafe {
                        let cg_rect = CGRect::new(
                            CGPoint::new(0.0, 0.0),
                            CGSize::new(frame.size.width, frame.size.height),
                        );
                        objc2::msg_send![Retained::as_ptr(&view) as *const AnyObject, addTrackingRect: cg_rect, owner: Retained::as_ptr(&view) as *mut AnyObject, userData: std::ptr::null::<std::ffi::c_void>(), assumeInside: false]
                    };
                }
            }
        }
    }

    extern "C" fn window_should_close(
        this_raw: *mut AnyObject,
        _sel: Sel,
        _id_ao: *mut AnyObject,
    ) -> Bool {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        unsafe {
            let () = objc2::msg_send![this as *const _ as *const _ as *const AnyObject, setNeedsDisplay: true];
        }

        if let Some(this) = Self::get_this(this) {
            this.inner
                .borrow_mut()
                .events
                .dispatch(WindowEvent::CloseRequested);
            Bool::NO
        } else {
            Bool::YES
        }
    }

    /// Ensure that the menubar is shown when we transition from a fullscreen window
    /// to either a non-fullscreen window or no windows.
    /// Without this, we can end up in a state where the menu bar is invisible when
    /// it should otherwise be visible, and it is especially confusing when there
    /// are no windows.
    fn update_application_presentation(&self, is_key: bool) {
        let is_simple_full_screen;
        let native_full_screen;

        {
            let inner = self.inner.borrow();
            native_full_screen = inner.config.native_macos_fullscreen_mode;
            is_simple_full_screen = inner.fullscreen.is_some();
        }

        if !native_full_screen {
            let current_app: id = unsafe {
                let __r: *mut AnyObject =
                    objc2::msg_send![objc2::class!(NSApplication), sharedApplication];
                __r as id
            };
            let target_options = match (is_key, is_simple_full_screen) {
                (true, true) => {
                    NSApplicationPresentationOptions::AutoHideMenuBar
                        | NSApplicationPresentationOptions::AutoHideDock
                }
                (true, false) | (false, _) => NSApplicationPresentationOptions::empty(),
            };
            unsafe {
                let current_options: NSApplicationPresentationOptions = objc2::msg_send![
                    current_app as *const _ as *const AnyObject,
                    presentationOptions
                ];
                if current_options != target_options {
                    let _: () = objc2::msg_send![
                        current_app as *const _ as *const AnyObject,
                        setPresentationOptions: target_options
                    ];
                }
            }
        }
    }

    extern "C" fn did_become_key(this_raw: *mut AnyObject, _sel: Sel, _id_ao: *mut AnyObject) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let _id = _id_ao as id;
        if let Some(this) = Self::get_this(this) {
            this.inner
                .borrow_mut()
                .events
                .dispatch(WindowEvent::FocusChanged(true));
            this.update_application_presentation(true);
        }
    }

    extern "C" fn did_resign_key(this_raw: *mut AnyObject, _sel: Sel, _id_ao: *mut AnyObject) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let _id = _id_ao as id;
        if let Some(this) = Self::get_this(this) {
            this.inner
                .borrow_mut()
                .events
                .dispatch(WindowEvent::FocusChanged(false));
            this.update_application_presentation(true);
        }
    }

    // Switch the coordinate system to have 0,0 in the top left
    extern "C" fn is_flipped(_this: *mut AnyObject, _sel: Sel) -> Bool {
        Bool::YES
    }

    // Tell the window/view/layer stuff that we only have a single opaque
    // thing in the window so that it can optimize rendering
    extern "C" fn is_opaque(_this: *mut AnyObject, _sel: Sel) -> Bool {
        Bool::NO
    }

    // Don't use Cocoa native window tabbing
    extern "C" fn allow_automatic_tabbing(_this: *mut AnyObject, _sel: Sel) -> Bool {
        Bool::NO
    }

    extern "C" fn wezboard_perform_key_assignment(
        this_raw: *mut AnyObject,
        _sel: Sel,
        menu_item_ao: *mut AnyObject,
    ) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let menu_item = menu_item_ao as *mut AnyObject;
        let menu_item = MenuItem::with_menu_item(menu_item as *mut _ as *mut AnyObject);
        // Safe because wezboardPerformKeyAssignment: is only used with KeyAssignment
        let action = menu_item.get_represented_item();
        log::debug!("wezboard_perform_key_assignment {action:?}",);
        match action {
            Some(RepresentedItem::KeyAssignment(action)) => {
                if let Some(this) = Self::get_this(this) {
                    this.inner
                        .borrow_mut()
                        .events
                        .dispatch(WindowEvent::PerformKeyAssignment(action));
                }
            }
            None => {}
        }
    }

    extern "C" fn window_will_close(this_raw: *mut AnyObject, _sel: Sel, _id_ao: *mut AnyObject) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let _id = _id_ao as id;
        if let Some(this) = Self::get_this(this) {
            // Advise the window of its impending death
            this.inner
                .borrow_mut()
                .events
                .dispatch(WindowEvent::Destroyed);
            this.update_application_presentation(false);
            let conn = Connection::get().unwrap();
            let window_id = this.inner.borrow_mut().window_id;
            conn.windows.borrow_mut().remove(&window_id);
        }
    }

    fn mouse_common(this: &mut AnyObject, nsevent: id, kind: MouseEventKind) {
        let view = this as id;
        let coords;
        let mouse_buttons;
        let modifiers;
        let screen_coords;
        unsafe {
            let location: CGPoint =
                objc2::msg_send![nsevent as *const _ as *const AnyObject, locationInWindow];
            let point: CGPoint = objc2::msg_send![view as *const _ as *const AnyObject, convertPoint: location, fromView: std::ptr::null::<AnyObject>()];
            let rect = CGRect::new(CGPoint::new(0., 0.), CGSize::new(point.x, point.y));
            let backing_rect: CGRect =
                objc2::msg_send![view as *const _ as *const AnyObject, convertRectToBacking: rect];
            // backing_rect computes abs() values, so we need to restore the sign
            // from the original point
            coords = CGPoint::new(
                f64::copysign(backing_rect.size.width, point.x),
                f64::copysign(backing_rect.size.height, point.y),
            );
            let pressed: u64 = objc2::msg_send![objc2::class!(NSEvent), pressedMouseButtons];
            mouse_buttons = decode_mouse_buttons(pressed);
            let modifier_flags: NSEventModifierFlags =
                objc2::msg_send![nsevent as *const _ as *const AnyObject, modifierFlags];
            modifiers = key_modifiers(modifier_flags);
            let mouse_loc: CGPoint = objc2::msg_send![objc2::class!(NSEvent), mouseLocation];
            screen_coords = CGPoint::new(mouse_loc.x, mouse_loc.y);
        }
        let event = MouseEvent {
            kind,
            coords: Point::new(coords.x as isize, coords.y as isize),
            screen_coords: cartesian_to_screen_point(screen_coords),
            mouse_buttons,
            modifiers,
        };

        if let Some(myself) = Self::get_this(this) {
            let mut inner = myself.inner.borrow_mut();
            inner.events.dispatch(WindowEvent::MouseEvent(event));
        }
    }

    extern "C" fn mouse_up(this_raw: *mut AnyObject, _sel: Sel, nsevent_ao: *mut AnyObject) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let nsevent = nsevent_ao as id;
        Self::mouse_common(this, nsevent, MouseEventKind::Release(MousePress::Left));
    }

    extern "C" fn mouse_down(this_raw: *mut AnyObject, _sel: Sel, nsevent_ao: *mut AnyObject) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let nsevent = nsevent_ao as id;
        Self::mouse_common(this, nsevent, MouseEventKind::Press(MousePress::Left));
    }
    extern "C" fn right_mouse_up(this_raw: *mut AnyObject, _sel: Sel, nsevent_ao: *mut AnyObject) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let nsevent = nsevent_ao as id;
        Self::mouse_common(this, nsevent, MouseEventKind::Release(MousePress::Right));
    }

    extern "C" fn other_mouse_up(this_raw: *mut AnyObject, _sel: Sel, nsevent_ao: *mut AnyObject) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let nsevent = nsevent_ao as id;
        // Safety: We know this is an button event
        unsafe {
            let button_number: isize =
                objc2::msg_send![nsevent as *const _ as *const AnyObject, buttonNumber];
            // Button 2 is the middle mouse button (scroll wheel)
            // but is the dedicated middle mouse button on 4 button mouses
            if button_number == 2 {
                Self::mouse_common(this, nsevent, MouseEventKind::Release(MousePress::Middle));
            }
        }
    }

    extern "C" fn scroll_wheel(this_raw: *mut AnyObject, _sel: Sel, nsevent_ao: *mut AnyObject) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let nsevent = nsevent_ao as id;
        let precise: bool = unsafe {
            objc2::msg_send![
                nsevent as *const _ as *const AnyObject,
                hasPreciseScrollingDeltas
            ]
        };
        let scale = if precise {
            // Devices with precise deltas report number of pixels scrolled.
            // At this layer we don't know how many pixels comprise a cell
            // in the terminal widget, and our abstraction doesn't allow being
            // told what that amount should be, so we come up with a hard
            // coded factor based on the likely default font size and dpi
            // to make the scroll speed feel a bit better.
            15.0
        } else {
            // Whereas imprecise deltas report the number of lines scrolled,
            // so we want to report those lines here wholesale.
            1.0
        };
        let mut vert_delta: CGFloat =
            unsafe { objc2::msg_send![nsevent as *const _ as *const AnyObject, scrollingDeltaY] };
        vert_delta /= scale;
        let mut horz_delta: CGFloat =
            unsafe { objc2::msg_send![nsevent as *const _ as *const AnyObject, scrollingDeltaX] };
        horz_delta /= scale;

        if let Some(myself) = Self::get_this(this) {
            let mut inner = myself.inner.borrow_mut();

            let elapsed = inner.last_wheel.elapsed();

            // If it's been a while since the last wheel movement,
            // we want to clear out any accumulated fractional amount
            // and round this event up to 1 line so that we get an
            // immediate scroll on the first move.
            let stale = std::time::Duration::from_millis(250);
            if elapsed >= stale {
                if vert_delta != 0.0 && vert_delta.abs() < 1.0 {
                    vert_delta = round_away_from_zerof(vert_delta);
                }
                if horz_delta != 0.0 && horz_delta.abs() < 1.0 {
                    horz_delta = round_away_from_zerof(horz_delta);
                }
                inner.vscroll_remainder = 0.;
                inner.hscroll_remainder = 0.;
            }

            inner.last_wheel = Instant::now();

            // Reset remainder when changing scroll direction
            if vert_delta.signum() != inner.vscroll_remainder.signum() {
                inner.vscroll_remainder = 0.;
            }
            if horz_delta.signum() != inner.hscroll_remainder.signum() {
                inner.hscroll_remainder = 0.;
            }

            vert_delta += inner.vscroll_remainder;
            horz_delta += inner.hscroll_remainder;

            inner.vscroll_remainder = vert_delta.fract();
            inner.hscroll_remainder = horz_delta.fract();

            vert_delta = vert_delta.trunc();
            horz_delta = horz_delta.trunc();
        }

        if vert_delta.abs() < 1.0 && horz_delta.abs() < 1.0 {
            return;
        }

        let kind = if vert_delta.abs() > horz_delta.abs() {
            MouseEventKind::VertWheel(round_away_from_zero(vert_delta))
        } else {
            MouseEventKind::HorzWheel(round_away_from_zero(horz_delta))
        };
        Self::mouse_common(this, nsevent, kind);
    }

    extern "C" fn right_mouse_down(
        this_raw: *mut AnyObject,
        _sel: Sel,
        nsevent_ao: *mut AnyObject,
    ) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let nsevent = nsevent_ao as id;
        Self::mouse_common(this, nsevent, MouseEventKind::Press(MousePress::Right));
    }

    extern "C" fn other_mouse_down(
        this_raw: *mut AnyObject,
        _sel: Sel,
        nsevent_ao: *mut AnyObject,
    ) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let nsevent = nsevent_ao as id;
        // Safety: See `other_mouse_up`
        unsafe {
            let button_number: isize =
                objc2::msg_send![nsevent as *const _ as *const AnyObject, buttonNumber];
            // See `other_mouse_up`
            if button_number == 2 {
                Self::mouse_common(this, nsevent, MouseEventKind::Press(MousePress::Middle));
            }
        }
    }

    extern "C" fn mouse_moved_or_dragged(
        this_raw: *mut AnyObject,
        _sel: Sel,
        nsevent_ao: *mut AnyObject,
    ) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let nsevent = nsevent_ao as id;
        Self::mouse_common(this, nsevent, MouseEventKind::Move);
    }

    extern "C" fn mouse_exited(this_raw: *mut AnyObject, _sel: Sel, _nsevent_ao: *mut AnyObject) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let _nsevent = _nsevent_ao as id;
        if let Some(myself) = Self::get_this(this) {
            myself
                .inner
                .borrow_mut()
                .events
                .dispatch(WindowEvent::MouseLeave);
        }
    }

    fn key_common(this: &mut AnyObject, nsevent: id, key_is_down: bool) {
        let is_a_repeat: bool =
            unsafe { objc2::msg_send![nsevent as *const _ as *const AnyObject, isARepeat] };
        let chars = unsafe {
            let c: *mut AnyObject =
                objc2::msg_send![nsevent as *const _ as *const AnyObject, characters];
            nsstring_to_str(c)
        };
        let unmod = unsafe {
            let c: *mut AnyObject = objc2::msg_send![
                nsevent as *const _ as *const AnyObject,
                charactersIgnoringModifiers
            ];
            nsstring_to_str(c)
        };
        let modifier_flags: NSEventModifierFlags =
            unsafe { objc2::msg_send![nsevent as *const _ as *const AnyObject, modifierFlags] };
        let modifiers = key_modifiers(modifier_flags);
        let leds = if modifier_flags.0 & (1 << 16) != 0 {
            KeyboardLedStatus::CAPS_LOCK
        } else {
            KeyboardLedStatus::empty()
        };
        let virtual_key: u16 =
            unsafe { objc2::msg_send![nsevent as *const _ as *const AnyObject, keyCode] };

        log::debug!(
            "key_common: chars=`{}` unmod=`{}` modifiers=`{:?}` virtual_key={:?} key_is_down:{}",
            chars.escape_debug(),
            unmod.escape_debug(),
            modifiers,
            virtual_key,
            key_is_down
        );

        // `Delete` on macos is really Backspace and emits BS.
        // `Fn-Delete` emits DEL.
        // Alt-Delete is mapped by the IME to be equivalent to Fn-Delete.
        // We want to emit Alt-BS in that situation.
        let (prefer_vkey, unmod) =
            if virtual_key == kVK_Delete && modifiers.contains(Modifiers::ALT) {
                (true, "\x08")
            } else if virtual_key == kVK_Tab {
                (true, "\t")
            } else if virtual_key == kVK_Delete {
                (true, "\x08")
            } else if virtual_key == kVK_ANSI_KeypadEnter {
                // https://github.com/termsurf/termsurf/issues/739
                // Keypad enter sends ctrl-c for some reason; explicitly
                // treat that as enter here.
                (true, "\r")
            } else {
                (false, unmod)
            };

        // Shift-Tab on macOS produces \x19 for some reason.
        // Rewrite it to something we understand.
        // <https://github.com/termsurf/termsurf/issues/1902>
        let chars = if virtual_key == kVK_Tab && modifiers.contains(Modifiers::SHIFT) {
            "\t"
        } else {
            chars
        };

        let phys_code = vkey_to_phys(virtual_key);
        let raw_key_handled = Handled::new();
        let raw_key_event = RawKeyEvent {
            key: if unmod.is_empty() {
                match phys_code {
                    Some(phys) => KeyCode::Physical(phys),
                    None => KeyCode::RawCode(virtual_key as _),
                }
            } else {
                KeyCode::composed(unmod)
            },
            phys_code,
            raw_code: virtual_key as _,
            leds,
            modifiers,
            repeat_count: 1,
            key_is_down,
            handled: raw_key_handled.clone(),
        };
        if let Some(myself) = Self::get_this(this) {
            let mut inner = myself.inner.borrow_mut();
            inner
                .events
                .dispatch(WindowEvent::RawKeyEvent(raw_key_event.clone()));
        }

        if raw_key_handled.is_handled() {
            log::trace!("raw key was handled; not processing further");
            return;
        }

        let chars = if let Some(myself) = Self::get_this(this) {
            let mut inner = myself.inner.borrow_mut();

            if chars.is_empty() || inner.dead_pending.is_some() {
                // Dead key!
                if !key_is_down {
                    return;
                }

                match inner.translate_key_event(virtual_key, modifier_flags) {
                    Ok(TranslateStatus::Composing(composing)) => {
                        // Next key press in dead key sequence is pending.
                        inner.events.dispatch(WindowEvent::AdviseDeadKeyStatus(
                            DeadKeyStatus::Composing(composing),
                        ));

                        return;
                    }
                    Ok(TranslateStatus::Composed(translated)) => {
                        inner
                            .events
                            .dispatch(WindowEvent::AdviseDeadKeyStatus(DeadKeyStatus::None));
                        let event = KeyEvent {
                            key: KeyCode::composed(&translated),
                            modifiers: Modifiers::NONE,
                            leds: KeyboardLedStatus::empty(),
                            repeat_count: 1,
                            key_is_down,
                            raw: None,
                        };
                        inner.events.dispatch(WindowEvent::KeyEvent(event));
                        return;
                    }
                    Ok(TranslateStatus::NotDead) => {
                        // Turned out that while it would have been a dead
                        // key combo, our send_composed_key_when_XXX settings
                        // said otherwise. Let's continue as if it was not
                        // a dead key.
                        unmod
                    }
                    Err(e) => {
                        log::error!("Failed to translate dead key: {}", e);
                        return;
                    }
                }
            } else {
                chars
            }
        } else {
            return;
        };

        let config_handle = config::configuration();
        let use_ime = config_handle.use_ime;
        let send_composed_key_when_left_alt_is_pressed =
            config_handle.send_composed_key_when_left_alt_is_pressed;
        let send_composed_key_when_right_alt_is_pressed =
            config_handle.send_composed_key_when_right_alt_is_pressed;

        // If unmod is empty it most likely means that the user has selected
        // an alternate keymap that has a chorded representation of eg: an ASCII
        // character.  One example of this is selecting a Norwegian keymap on
        // a US keyboard.  The `~` symbol is produced by pressing CTRL-].
        // That shows up here as unmod=`` with modifiers=CTRL.  In this situation
        // we want to cancel the modifiers out so that we just focus on
        // `chars` instead.
        let modifiers = if unmod.is_empty() {
            Modifiers::NONE
        } else {
            modifiers
        };

        let alt_mods = Modifiers::LEFT_ALT | Modifiers::RIGHT_ALT | Modifiers::ALT;
        let only_left_alt = (modifiers & alt_mods) == (Modifiers::LEFT_ALT | Modifiers::ALT);
        let only_right_alt = (modifiers & alt_mods) == (Modifiers::RIGHT_ALT | Modifiers::ALT);

        // Also respect `send_composed_key_when_(left|right)_alt_is_pressed` configs
        // when `use_ime` is true.
        let forward_to_ime = {
            if only_left_alt && !send_composed_key_when_left_alt_is_pressed {
                false
            } else if only_right_alt && !send_composed_key_when_right_alt_is_pressed {
                false
            } else {
                modifiers.is_empty()
                    || modifiers.intersects(config_handle.macos_forward_to_ime_modifier_mask)
            }
        };

        if key_is_down && use_ime && forward_to_ime {
            if let Some(myself) = Self::get_this(this) {
                let mut inner = myself.inner.borrow_mut();
                inner.key_is_down.replace(key_is_down);
                inner.ime_state = ImeDisposition::None;
                inner.ime_text.clear();
            }

            unsafe {
                let array: id = {
                    let __r: *mut AnyObject = objc2::msg_send![AnyClass::get(c"NSArray").unwrap(), arrayWithObject: nsevent as *mut AnyObject];
                    __r as id
                };
                let _: () = objc2::msg_send![this as *const _ as *const _ as *const AnyObject, interpretKeyEvents: array as *mut AnyObject];

                if let Some(myself) = Self::get_this(this) {
                    let mut inner = myself.inner.borrow_mut();
                    log::trace!(
                        "IME state: {:?}, last_event: {:?}",
                        inner.ime_state,
                        inner.ime_last_event
                    );
                    match inner.ime_state {
                        ImeDisposition::Continue => {
                            // IME handled the event by generating NOOP;
                            // let's continue with our normal handling
                            // code below.
                            inner.ime_last_event.take();
                        }
                        ImeDisposition::Acted => {
                            // The key caused the IME to call one of our
                            // callbacks, which may have generated an event and
                            // stashed it into ime_last_event.
                            // If it didn't generate an event, then a composition
                            // is pending.
                            let status = if inner.ime_last_event.is_none() {
                                DeadKeyStatus::Composing(inner.ime_text.clone())
                            } else {
                                DeadKeyStatus::None
                            };
                            inner
                                .events
                                .dispatch(WindowEvent::AdviseDeadKeyStatus(status));
                            return;
                        }
                        ImeDisposition::None => {
                            // The IME clocked something in its state,
                            // but didn't call one of our callbacks.
                            // In theory, we should stop here, but the IME
                            // mysteriously swallows key repeats for certain
                            // keys (i.e. b, f, j, m, p, q, v, x) but not others.
                            // To compensate for that, if the current event
                            // is a repeat, and the IME previously generated
                            // `Acted`, we will assume that we're safe to replay
                            // that last action.
                            if is_a_repeat {
                                if let Some(event) =
                                    inner.ime_last_event.as_ref().map(|e| e.clone())
                                {
                                    inner.events.dispatch(WindowEvent::KeyEvent(event));
                                    return;
                                }
                            }
                            let status = if inner.ime_text.is_empty() {
                                DeadKeyStatus::None
                            } else {
                                DeadKeyStatus::Composing(inner.ime_text.clone())
                            };
                            inner
                                .events
                                .dispatch(WindowEvent::AdviseDeadKeyStatus(status));
                            return;
                        }
                    }
                }
            }
        }

        fn key_string_to_key_code(s: &str) -> Option<KeyCode> {
            let mut char_iter = s.chars();
            if let Some(first_char) = char_iter.next() {
                if char_iter.next().is_none() {
                    // A single unicode char
                    Some(function_key_to_keycode(first_char))
                } else {
                    Some(KeyCode::Composed(s.to_owned()))
                }
            } else {
                None
            }
        }

        // When both shift and alt are pressed, macos appears to swap `chars` with `unmod`,
        // which isn't particularly helpful. eg: ALT+SHIFT+` produces chars='`' and unmod='~'
        // In this case, we take the key from unmod.
        // We leave `raw` set to None as we want to preserve the value of modifiers.
        // <https://github.com/termsurf/termsurf/issues/1706>.
        // We can't do this for every ALT+SHIFT combo, as the weird behavior doesn't
        // apply to eg: ALT+SHIFT+789 for Norwegian layouts
        // <https://github.com/termsurf/termsurf/issues/760>
        let swap_unmod_and_chars = (modifiers.contains(Modifiers::SHIFT | Modifiers::ALT)
            && virtual_key == kVK_ANSI_Grave)
            ||
            // <https://github.com/termsurf/termsurf/issues/1907>
            (modifiers.contains(Modifiers::SHIFT | Modifiers::CTRL)
                && virtual_key == kVK_ANSI_Slash);

        if let Some(key) = key_string_to_key_code(chars).or_else(|| key_string_to_key_code(unmod)) {
            let (key, raw_key) = if prefer_vkey {
                match phys_code {
                    Some(phys) => (phys.to_key_code(), None),
                    None => {
                        log::error!(
                            "prefer_vkey=true, but phys_code is None. {:?}",
                            raw_key_event
                        );
                        return;
                    }
                }
            } else if (only_left_alt && !send_composed_key_when_left_alt_is_pressed)
                || (only_right_alt && !send_composed_key_when_right_alt_is_pressed)
            {
                // Take the unmodified key only!
                match key_string_to_key_code(unmod) {
                    Some(key) => (key, None),
                    None => return,
                }
            } else if chars.is_empty() || chars == unmod {
                (key, None)
            } else if swap_unmod_and_chars {
                match key_string_to_key_code(unmod) {
                    Some(key) => (key, None),
                    None => return,
                }
            } else {
                let raw = key_string_to_key_code(unmod);
                match (&key, &raw) {
                    // Avoid eg: \x01 when we can use CTRL-A.
                    // This also helps to keep the correct sequence for backspace/delete.
                    // But take care: on German layouts CTRL-Backslash has unmod="/"
                    // but chars="\x1c"; we only want to do this transformation when
                    // chars and unmod have that base ASCII relationship.
                    // <https://github.com/termsurf/termsurf/issues/1891>
                    (KeyCode::Char(c), Some(KeyCode::Char(raw)))
                        if is_ascii_control(*c) == Some(raw.to_ascii_lowercase()) =>
                    {
                        (KeyCode::Char(*raw), None)
                    }
                    _ => (key, raw),
                }
            };

            let modifiers = if raw_key.is_some() {
                Modifiers::NONE
            } else {
                modifiers
            };

            let event = KeyEvent {
                key,
                modifiers,
                leds,
                repeat_count: 1,
                key_is_down,
                raw: Some(raw_key_event),
            }
            .normalize_shift()
            .resurface_positional_modifier_key();

            log::debug!(
                "key_common {:?} (chars={:?} unmod={:?} modifiers={:?})",
                event,
                chars,
                unmod,
                modifiers
            );

            if let Some(myself) = Self::get_this(this) {
                let mut inner = myself.inner.borrow_mut();
                // Don't clear the last IME event when a key is up otherwise it
                // could mess up the succeeding key repeats.
                if key_is_down {
                    inner.ime_last_event.take();
                }
                inner.events.dispatch(WindowEvent::KeyEvent(event));
            }
        }
    }

    extern "C" fn perform_key_equivalent(
        this_raw: *mut AnyObject,
        _sel: Sel,
        nsevent_ao: *mut AnyObject,
    ) -> Bool {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let nsevent = nsevent_ao as id;
        #[allow(unused_macros)]
        macro_rules! yes_no {
            (YES) => {
                Bool::YES
            };
            (NO) => {
                Bool::NO
            };
        }
        let chars = unsafe {
            let c: *mut AnyObject =
                objc2::msg_send![nsevent as *const _ as *const AnyObject, characters];
            nsstring_to_str(c)
        };
        let modifier_flags: NSEventModifierFlags =
            unsafe { objc2::msg_send![nsevent as *const _ as *const AnyObject, modifierFlags] };
        let modifiers = key_modifiers(modifier_flags);

        log::trace!(
            "perform_key_equivalent: chars=`{}` modifiers=`{:?}`",
            chars.escape_debug(),
            modifiers,
        );

        if (chars == "." && modifiers == Modifiers::SUPER)
            || (chars == "\u{1b}" && modifiers == Modifiers::CTRL)
            || (chars == "\t" && modifiers == Modifiers::CTRL)
            || (chars == "\x19"/* Shift-Tab: See issue #1902 */)
        {
            // Synthesize a key down event for this, because macOS will
            // not do that, even though we tell it that we handled this event.
            // <https://github.com/termsurf/termsurf/issues/1867>
            Self::key_common(this, nsevent, true);

            // Prevent macOS from calling doCommandBySelector(cancel:)
            Bool::YES
        } else {
            // Allow macOS to process built-in shortcuts like CMD-`
            // to cycle though windows
            Bool::NO
        }
    }

    extern "C" fn flags_changed(this_raw: *mut AnyObject, _sel: Sel, nsevent_ao: *mut AnyObject) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let nsevent = nsevent_ao as id;
        let modifier_flags: NSEventModifierFlags =
            unsafe { objc2::msg_send![nsevent as *const _ as *const AnyObject, modifierFlags] };
        let modifiers = key_modifiers(modifier_flags);
        let leds = if modifier_flags.0 & (1 << 16) != 0 {
            KeyboardLedStatus::CAPS_LOCK
        } else {
            KeyboardLedStatus::empty()
        };

        if let Some(myself) = Self::get_this(this) {
            let mut inner = myself.inner.borrow_mut();
            inner
                .events
                .dispatch(WindowEvent::AdviseModifiersLedStatus(modifiers, leds));
        }
    }

    extern "C" fn key_down(this_raw: *mut AnyObject, _sel: Sel, nsevent_ao: *mut AnyObject) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let nsevent = nsevent_ao as id;
        Self::key_common(this, nsevent, true);
    }

    extern "C" fn key_up(this_raw: *mut AnyObject, _sel: Sel, nsevent_ao: *mut AnyObject) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let nsevent = nsevent_ao as id;
        Self::key_common(this, nsevent, false);
    }

    extern "C" fn did_change_screen(
        this_raw: *mut AnyObject,
        _sel: Sel,
        _notification_ao: *mut AnyObject,
    ) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let _notification = _notification_ao as id;
        log::trace!("did_change_screen");
        if let Some(this) = Self::get_this(this) {
            // Just set a flag; we don't want to react immediately
            // as this even fires as part of a live move and the
            // resize flow may try to re-position the window to
            // the wrong place.
            this.inner.borrow_mut().screen_changed = true;
        }
    }

    extern "C" fn will_start_live_resize(
        this_raw: *mut AnyObject,
        _sel: Sel,
        _notification_ao: *mut AnyObject,
    ) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let _notification = _notification_ao as id;
        if let Some(this) = Self::get_this(this) {
            let mut inner = this.inner.borrow_mut();
            inner.live_resizing = true;
        }
    }

    extern "C" fn did_end_live_resize(
        this_raw: *mut AnyObject,
        _sel: Sel,
        _notification_ao: *mut AnyObject,
    ) {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let _notification = _notification_ao as id;
        if let Some(this) = Self::get_this(this) {
            let mut inner = this.inner.borrow_mut();
            inner.live_resizing = false;
        }
    }

    extern "C" fn did_resize(this: *mut AnyObject, _sel: Sel, _notification: *mut AnyObject) {
        let this_obj = unsafe { &mut *(this as *mut AnyObject) };
        if let Some(this) = Self::get_this(this_obj) {
            let inner = this.inner.borrow_mut();

            if let Some(gl_context_pair) = inner.gl_context_pair.as_ref() {
                gl_context_pair.backend.update();
            }
        }

        let frame: CGRect =
            unsafe { objc2::msg_send![this as *const _ as *const AnyObject, frame] };
        let backing_frame: CGRect = unsafe {
            objc2::msg_send![this as *const _ as *const AnyObject, convertRectToBacking: frame]
        };
        let width = backing_frame.size.width;
        let height = backing_frame.size.height;
        if let Some(this) = Self::get_this(this_obj) {
            let mut inner = this.inner.borrow_mut();

            // This is a little gross; ideally we'd call
            // WindowInner:is_fullscreen to determine this, but
            // we can't get a mutable reference to it from here
            // as we can be called in a context where something
            // higher up the callstack already has a mutable
            // reference and we'd panic.
            let is_full_screen = inner.fullscreen.is_some()
                || inner
                    .window
                    .as_ref()
                    .and_then(|w| w.load())
                    .map_or(false, |window| {
                        let style_mask: NSWindowStyleMask = unsafe {
                            objc2::msg_send![
                                Retained::as_ptr(&window) as *const AnyObject,
                                styleMask
                            ]
                        };
                        style_mask.contains(NSWindowStyleMask::FullScreen)
                    });

            let live_resizing = inner.live_resizing;

            // Note: isZoomed can falsely return YES in situations such as
            // the current screen changing. We cannot detect that case here.
            // There is some logic to compensate for this in
            // wezboard-gui/src/termwindow/resize.rs.
            // <https://github.com/termsurf/termsurf/issues/3503>
            let is_zoomed = !is_full_screen
                && inner
                    .window
                    .as_ref()
                    .and_then(|w| w.load())
                    .map_or(false, |window| unsafe {
                        objc2::msg_send![Retained::as_ptr(&window) as *const AnyObject, isZoomed]
                    });

            let window_level = inner
                .window
                .as_ref()
                .and_then(|w| w.load())
                .map(|window| {
                    let level: i64 = unsafe {
                        objc2::msg_send![Retained::as_ptr(&window) as *const AnyObject, level]
                    };
                    nswindow_level_to_window_level(level)
                })
                .unwrap_or_default();

            let level_state = match window_level {
                WindowLevel::AlwaysOnBottom => WindowState::ALWAYS_ON_BOTTOM,
                WindowLevel::AlwaysOnTop => WindowState::ALWAYS_ON_TOP,
                WindowLevel::Normal => WindowState::default(),
            };

            let screen_state = match (is_full_screen, is_zoomed) {
                (true, _) => WindowState::FULL_SCREEN,
                (_, true) => WindowState::MAXIMIZED,
                _ => WindowState::default(),
            };

            let dpi = inner
                .window
                .as_ref()
                .and_then(|w| w.load())
                .and_then(|window| {
                    dpi_for_window_screen(
                        Retained::as_ptr(&window) as *mut AnyObject,
                        &inner.config,
                    )
                })
                .unwrap_or(crate::DEFAULT_DPI * (backing_frame.size.width / frame.size.width))
                as usize;

            inner.events.dispatch(WindowEvent::Resized {
                dimensions: Dimensions {
                    pixel_width: width as usize,
                    pixel_height: height as usize,
                    dpi,
                },
                window_state: screen_state | level_state,
                live_resizing,
            });
        }
    }

    extern "C" fn update_layer(_view_raw: *mut AnyObject, _sel: Sel) {
        let _view = unsafe { &mut *(_view_raw as *mut AnyObject) };
        log::trace!("update_layer called");
    }

    extern "C" fn wants_update_layer(_view_raw: *mut AnyObject, _sel: Sel) -> Bool {
        log::trace!("wants_update_layer called");
        Bool::YES
    }

    extern "C" fn display_layer(view: *mut AnyObject, sel: Sel, _layer_id: *mut AnyObject) {
        Self::draw_rect(
            view,
            sel,
            CGRect::new(CGPoint::new(0., 0.), CGSize::new(0., 0.)),
        )
    }

    extern "C" fn draw_layer_in_context(
        _view: *mut AnyObject,
        _sel: Sel,
        _layer_id: *mut AnyObject,
        _context: *mut CGContext,
    ) {
    }

    extern "C" fn layer_should_inherit_contents_scale_from_window(
        _: *mut AnyObject,
        _: Sel,
        layer: *mut AnyObject,
        _: CGFloat,
        _: *mut AnyObject,
    ) -> Bool {
        log::trace!("layer_should_inherit_contents_scale_from_window");
        unsafe {
            let () = objc2::msg_send![layer, setContentsScale: 1.0];
        }
        Bool::YES
    }

    extern "C" fn make_backing_layer(view: *mut AnyObject, _: Sel) -> *mut AnyObject {
        log::trace!("make_backing_layer");
        let class = AnyClass::get(c"CAMetalLayer").unwrap();
        unsafe {
            // Use type method to get a instance of CAMetalLayer.
            // So that we don't have to worry about retaining/releasing it.
            let layer: *mut AnyObject = objc2::msg_send![class, layer];
            let () = objc2::msg_send![layer, setDelegate: view];
            let () = objc2::msg_send![layer, setContentsScale: 1.0];
            let () = objc2::msg_send![layer, setOpaque: false];
            layer
        }
    }

    extern "C" fn draw_rect(view: *mut AnyObject, sel: Sel, _dirty_rect: CGRect) {
        let view_obj = unsafe { &mut *(view as *mut AnyObject) };
        if let Some(this) = Self::get_this(view_obj) {
            let mut inner = this.inner.borrow_mut();

            if inner.screen_changed {
                // If the screen resolution changed (which can also
                // happen if the window was dragged to another monitor
                // with different dpi), then we treat this as a resize
                // event that will in turn trigger an invalidation
                // and a repaint.
                inner.screen_changed = false;
                drop(inner);
                Self::did_resize(view, sel, std::ptr::null_mut());
                return;
            }

            if inner.paint_throttled {
                inner.invalidated = true;
            } else {
                inner.events.dispatch(WindowEvent::NeedRepaint);
                inner.invalidated = false;
                inner.paint_throttled = true;

                let window_id = inner.window_id;
                let max_fps = inner.config.max_fps;
                promise::spawn::spawn(async move {
                    async_io::Timer::after(std::time::Duration::from_millis(1000 / max_fps as u64))
                        .await;
                    Connection::with_window_inner(window_id, move |inner| {
                        if let Some(window_view) = WindowView::get_this(&*inner.view) {
                            let mut state = window_view.inner.borrow_mut();
                            state.paint_throttled = false;
                            if state.invalidated {
                                unsafe {
                                    let () = objc2::msg_send![Retained::as_ptr(&inner.view) as *const AnyObject, setNeedsDisplay: true];
                                }
                            }
                        }
                        Ok(())
                    });
                })
                .detach();
            }
        }
    }

    extern "C" fn dragging_entered(
        this_raw: *mut AnyObject,
        _: Sel,
        sender_ao: *mut AnyObject,
    ) -> usize {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let sender = sender_ao as id;
        // NSDragOperationNone = 0, NSDragOperationCopy = 1
        if let Some(this) = Self::get_this(this) {
            let mut inner = this.inner.borrow_mut();

            let pb: id = unsafe {
                let __r: *mut AnyObject =
                    objc2::msg_send![sender as *const _ as *const AnyObject, draggingPasteboard];
                __r as id
            };
            if pb.is_null() {
                return 0; // NSDragOperationNone
            }

            let filenames = unsafe {
                let __r: *mut AnyObject = objc2::msg_send![pb as *const _ as *const AnyObject, propertyListForType: {let s = nsstring("NSFilenamesPboardType"); objc2::rc::Retained::as_ptr(&s) as *const _ as *const AnyObject}];
                __r as id
            };
            if filenames.is_null() {
                return 0; // NSDragOperationNone
            }

            let count: usize =
                unsafe { objc2::msg_send![filenames as *const _ as *const AnyObject, count] };
            let paths = (0..count)
                .map(|i| unsafe {
                    let file: *mut AnyObject = objc2::msg_send![filenames as *const _ as *const AnyObject, objectAtIndex: i];
                    let path = nsstring_to_str(file);
                    PathBuf::from(path)
                })
                .collect::<Vec<_>>();
            inner.events.dispatch(WindowEvent::DraggedFile(paths));
        }
        1 // NSDragOperationCopy
    }

    extern "C" fn perform_drag_operation(
        this_raw: *mut AnyObject,
        _: Sel,
        sender_ao: *mut AnyObject,
    ) -> Bool {
        let this = unsafe { &mut *(this_raw as *mut AnyObject) };
        let sender = sender_ao as id;
        if let Some(this) = Self::get_this(this) {
            let mut inner = this.inner.borrow_mut();

            let pb: id = unsafe {
                let __r: *mut AnyObject =
                    objc2::msg_send![sender as *const _ as *const AnyObject, draggingPasteboard];
                __r as id
            };
            if pb.is_null() {
                return Bool::NO;
            }

            let filenames = unsafe {
                let __r: *mut AnyObject = objc2::msg_send![pb as *const _ as *const AnyObject, propertyListForType: {let s = nsstring("NSFilenamesPboardType"); objc2::rc::Retained::as_ptr(&s) as *const _ as *const AnyObject}];
                __r as id
            };
            if filenames.is_null() {
                return Bool::NO;
            }

            let count: usize =
                unsafe { objc2::msg_send![filenames as *const _ as *const AnyObject, count] };
            let paths = (0..count)
                .map(|i| unsafe {
                    let file: *mut AnyObject = objc2::msg_send![filenames as *const _ as *const AnyObject, objectAtIndex: i];
                    let path = nsstring_to_str(file);
                    PathBuf::from(path)
                })
                .collect::<Vec<_>>();
            inner.events.dispatch(WindowEvent::DroppedFile(paths));
        }
        Bool::YES
    }

    fn get_this(this: &AnyObject) -> Option<&mut Self> {
        unsafe {
            let myself: *mut c_void = get_view_ivar(this);
            if myself.is_null() {
                None
            } else {
                Some(&mut *(myself as *mut Self))
            }
        }
    }

    fn init_with_frame(
        inner: &Rc<RefCell<Inner>>,
        rect: CGRect,
    ) -> anyhow::Result<Retained<AnyObject>> {
        let cls = Self::get_class();

        let view_id: id = unsafe {
            let __r: *mut AnyObject = objc2::msg_send![cls, alloc];
            __r as id
        };
        // SAFETY: view_id was just allocated above; initWithFrame may return nil on failure.
        let view_id: Retained<AnyObject> = unsafe {
            let __r: *mut AnyObject =
                objc2::msg_send![view_id as *const _ as *const AnyObject, initWithFrame: rect];
            Retained::from_raw(__r)
                .ok_or_else(|| anyhow::anyhow!("NSView initWithFrame returned nil"))?
        };
        inner
            .borrow_mut()
            .view_id
            .replace(Weak::from_retained(&view_id));

        let view = Box::into_raw(Box::new(Self {
            inner: Rc::clone(&inner),
        }));

        unsafe {
            set_view_ivar(
                &mut *(Retained::as_ptr(&view_id) as *mut AnyObject),
                view as *mut c_void,
            );
        }

        Ok(view_id)
    }

    fn get_class() -> &'static AnyClass {
        AnyClass::get(VIEW_CLS_CNAME).unwrap_or_else(|| Self::define_class())
    }

    fn define_class() -> &'static AnyClass {
        let mut cls = ClassBuilder::new(VIEW_CLS_CNAME, AnyClass::get(c"NSView").unwrap())
            .expect("Unable to register WindowView class");

        cls.add_ivar::<*mut c_void>(VIEW_CLS_CNAME);
        cls.add_protocol(
            AnyProtocol::get(c"NSTextInputClient")
                .expect("failed to get NSTextInputClient protocol"),
        );

        cls.add_protocol(
            AnyProtocol::get(c"CALayerDelegate").expect("CALayerDelegate not defined"),
        );

        // All callbacks use *mut AnyObject for self and pointer args to satisfy
        // ClassBuilder::add_method's MethodImplementation trait bound.
        // id (= *mut AnyObject) args become *mut AnyObject; BOOL returns become Bool.
        unsafe {
            cls.add_method(
                objc2::sel!(dealloc),
                Self::dealloc as extern "C" fn(*mut AnyObject, Sel),
            );
            cls.add_method(
                objc2::sel!(wezboardPerformKeyAssignment:),
                Self::wezboard_perform_key_assignment
                    as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(windowWillClose:),
                Self::window_will_close as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(windowShouldClose:),
                Self::window_should_close
                    as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> Bool,
            );
            cls.add_method(
                objc2::sel!(makeBackingLayer),
                Self::make_backing_layer as extern "C" fn(*mut AnyObject, Sel) -> *mut AnyObject,
            );
            cls.add_method(
                objc2::sel!(layer:shouldInheritContentsScale:fromWindow:),
                Self::layer_should_inherit_contents_scale_from_window
                    as extern "C" fn(
                        *mut AnyObject,
                        Sel,
                        *mut AnyObject,
                        CGFloat,
                        *mut AnyObject,
                    ) -> Bool,
            );
            cls.add_method(
                objc2::sel!(drawRect:),
                Self::draw_rect as extern "C" fn(*mut AnyObject, Sel, CGRect),
            );
            cls.add_method(
                objc2::sel!(updateLayer),
                Self::update_layer as extern "C" fn(*mut AnyObject, Sel),
            );
            cls.add_method(
                objc2::sel!(wantsUpdateLayer),
                Self::wants_update_layer as extern "C" fn(*mut AnyObject, Sel) -> Bool,
            );
            cls.add_method(
                objc2::sel!(displayLayer:),
                Self::display_layer as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(drawLayer:inContext:),
                Self::draw_layer_in_context
                    as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject, *mut CGContext),
            );
            cls.add_method(
                objc2::sel!(isFlipped),
                Self::is_flipped as extern "C" fn(*mut AnyObject, Sel) -> Bool,
            );
            cls.add_method(
                objc2::sel!(isOpaque),
                Self::is_opaque as extern "C" fn(*mut AnyObject, Sel) -> Bool,
            );
            cls.add_method(
                objc2::sel!(allowsAutomaticWindowTabbing),
                Self::allow_automatic_tabbing as extern "C" fn(*mut AnyObject, Sel) -> Bool,
            );
            cls.add_method(
                objc2::sel!(windowWillStartLiveResize:),
                Self::will_start_live_resize as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(windowDidEndLiveResize:),
                Self::did_end_live_resize as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(windowDidResize:),
                Self::did_resize as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(windowDidChangeScreen:),
                Self::did_change_screen as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(windowDidBecomeKey:),
                Self::did_become_key as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(windowDidResignKey:),
                Self::did_resign_key as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(mouseMoved:),
                Self::mouse_moved_or_dragged as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(mouseDragged:),
                Self::mouse_moved_or_dragged as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(rightMouseDragged:),
                Self::mouse_moved_or_dragged as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(mouseDown:),
                Self::mouse_down as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(mouseUp:),
                Self::mouse_up as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(rightMouseDown:),
                Self::right_mouse_down as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(rightMouseUp:),
                Self::right_mouse_up as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(otherMouseDragged:),
                Self::mouse_moved_or_dragged as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(otherMouseDown:),
                Self::other_mouse_down as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(otherMouseUp:),
                Self::other_mouse_up as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(scrollWheel:),
                Self::scroll_wheel as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(mouseExited:),
                Self::mouse_exited as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(keyDown:),
                Self::key_down as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(keyUp:),
                Self::key_up as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            cls.add_method(
                objc2::sel!(performKeyEquivalent:),
                Self::perform_key_equivalent
                    as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> Bool,
            );
            cls.add_method(
                objc2::sel!(acceptsFirstResponder),
                Self::accepts_first_responder as extern "C" fn(*mut AnyObject, Sel) -> Bool,
            );
            cls.add_method(
                objc2::sel!(acceptsFirstMouse:),
                Self::accepts_first_mouse
                    as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> Bool,
            );
            cls.add_method(
                objc2::sel!(viewDidChangeEffectiveAppearance),
                Self::view_did_change_effective_appearance as extern "C" fn(*mut AnyObject, Sel),
            );
            cls.add_method(
                objc2::sel!(updateTrackingAreas),
                Self::update_tracking_areas as extern "C" fn(*mut AnyObject, Sel),
            );
            cls.add_method(
                objc2::sel!(flagsChanged:),
                Self::flags_changed as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            // NSTextInputClient
            cls.add_method(
                objc2::sel!(hasMarkedText),
                Self::has_marked_text as extern "C" fn(*mut AnyObject, Sel) -> Bool,
            );
            cls.add_method(
                objc2::sel!(markedRange),
                Self::marked_range as extern "C" fn(*mut AnyObject, Sel) -> NSRange,
            );
            cls.add_method(
                objc2::sel!(selectedRange),
                Self::selected_range as extern "C" fn(*mut AnyObject, Sel) -> NSRange,
            );
            cls.add_method(
                objc2::sel!(setMarkedText:selectedRange:replacementRange:),
                Self::set_marked_text_selected_range_replacement_range
                    as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject, NSRange, NSRange),
            );
            cls.add_method(
                objc2::sel!(unmarkText),
                Self::unmark_text as extern "C" fn(*mut AnyObject, Sel),
            );
            cls.add_method(
                objc2::sel!(validAttributesForMarkedText),
                Self::valid_attributes_for_marked_text
                    as extern "C" fn(*mut AnyObject, Sel) -> *mut AnyObject,
            );
            cls.add_method(
                objc2::sel!(doCommandBySelector:),
                Self::do_command_by_selector as extern "C" fn(*mut AnyObject, Sel, Sel),
            );
            cls.add_method(
                objc2::sel!(attributedSubstringForProposedRange:actualRange:),
                Self::attributed_substring_for_proposed_range
                    as extern "C" fn(*mut AnyObject, Sel, NSRange, *mut NSRange) -> *mut AnyObject,
            );
            cls.add_method(
                objc2::sel!(insertText:replacementRange:),
                Self::insert_text_replacement_range
                    as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject, NSRange),
            );
            cls.add_method(
                objc2::sel!(characterIndexForPoint:),
                Self::character_index_for_point
                    as extern "C" fn(*mut AnyObject, Sel, CGPoint) -> usize,
            );
            cls.add_method(
                objc2::sel!(firstRectForCharacterRange:actualRange:),
                Self::first_rect_for_character_range
                    as extern "C" fn(*mut AnyObject, Sel, NSRange, *mut NSRange) -> CGRect,
            );
            cls.add_method(
                objc2::sel!(draggingEntered:),
                Self::dragging_entered
                    as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize,
            );
            cls.add_method(
                objc2::sel!(performDragOperation:),
                Self::perform_drag_operation
                    as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> Bool,
            );
        }

        cls.register()
    }
}
