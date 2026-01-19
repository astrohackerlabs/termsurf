//! CEF browser management for TermSurf 2.0
//!
//! This module handles browser creation, rendering, and input for CEF browsers
//! that overlay terminal panes.

use cef::{
    self, rc::Rc, wrap_client, wrap_context_menu_handler, wrap_render_handler,
    wrap_request_context_handler, Browser, BrowserHost, BrowserSettings, Client,
    ContextMenuHandler, ImplBrowser, ImplBrowserHost, ImplClient, ImplContextMenuHandler,
    ImplMenuModel, ImplRenderHandler, ImplRequestContextHandler, KeyEvent, KeyEventType,
    MouseButtonType, MouseEvent, PaintElementType, Rect, RenderHandler, RequestContextHandler,
    RequestContextSettings, ScreenInfo, WindowInfo, WrapClient, WrapContextMenuHandler,
    WrapRenderHandler, WrapRequestContextHandler,
};
use mux::pane::PaneId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// Texture holder type for storing the CEF render texture bind group
pub type TextureHolder = std::rc::Rc<RefCell<Option<wgpu::BindGroup>>>;

/// Pane bounds: current position and size of the pane (x, y, width, height)
/// Updated every frame from paint_browser_overlay, used directly for rendering
pub type PaneBoundsHolder = std::rc::Rc<RefCell<(f32, f32, u32, u32)>>;

/// State for a single browser instance
pub struct BrowserState {
    pub browser: Browser,
    pub pane_id: PaneId,
    pub url: String,
    pub texture_holder: TextureHolder,
    /// Current pane bounds (x, y, width, height) - updated every frame
    pane_bounds: PaneBoundsHolder,
    /// Logical size for CEF (DIP)
    size: std::rc::Rc<RefCell<(u32, u32)>>,
    /// Time of last resize call - used for settle-and-rerender logic
    /// When Some, we're waiting to do a final "settle" render after 10ms of no changes
    last_resize_time: RefCell<Option<std::time::Instant>>,
    /// Pending target size - the size we want to resize to after settling
    /// Used to detect when the target size changes vs when we're just waiting
    pending_size: RefCell<Option<(u32, u32)>>,
}

impl BrowserState {
    /// Create a new browser for the given pane
    ///
    /// Parameters:
    /// - `width`, `height`: Logical pixel dimensions (DIP), not physical pixels
    /// - `device_scale_factor`: Display scale factor (e.g., 2.0 for Retina)
    pub fn new(
        pane_id: PaneId,
        url: &str,
        width: u32,
        height: u32,
        device_scale_factor: f32,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        invalidate_callback: Arc<dyn Fn() + Send + Sync>,
    ) -> anyhow::Result<Self> {
        log::info!(
            "[CEF] Creating browser for pane {} with URL: {} ({}x{} @ {:.1}x scale)",
            pane_id,
            url,
            width,
            height,
            device_scale_factor
        );

        // Create render handler parts
        let size = std::rc::Rc::new(RefCell::new((width, height)));
        let texture_holder: TextureHolder = std::rc::Rc::new(RefCell::new(None));
        let pane_bounds: PaneBoundsHolder = std::rc::Rc::new(RefCell::new((0.0, 0.0, width, height)));

        let render_handler = CefRenderHandler {
            size: size.clone(),
            texture_holder: texture_holder.clone(),
            device: device.clone(),
            queue: queue.clone(),
            bind_group_layout: bind_group_layout.clone(),
            device_scale_factor,
            invalidate_callback,
        };

        // Window info for OSR mode
        let accelerated_osr = cfg!(all(
            any(target_os = "macos", target_os = "windows", target_os = "linux"),
            feature = "accelerated_osr"
        ));
        let window_info = WindowInfo {
            windowless_rendering_enabled: 1,
            shared_texture_enabled: accelerated_osr as i32,
            external_begin_frame_enabled: 0,
            ..Default::default()
        };

        let browser_settings = BrowserSettings {
            windowless_frame_rate: 60,
            ..Default::default()
        };

        // Create request context
        let mut context = cef::request_context_create_context(
            Some(&RequestContextSettings::default()),
            Some(&mut CefRequestContextHandlerBuilder::build()),
        );

        // Create the browser synchronously
        let browser = cef::browser_host_create_browser_sync(
            Some(&window_info),
            Some(&mut CefClientBuilder::build(render_handler)),
            Some(&url.into()),
            Some(&browser_settings),
            None,
            context.as_mut(),
        );

        let browser = browser.ok_or_else(|| anyhow::anyhow!("Failed to create CEF browser"))?;

        log::info!("[CEF] Browser created successfully for pane {}", pane_id);

        Ok(Self {
            browser,
            pane_id,
            url: url.to_string(),
            texture_holder,
            pane_bounds,
            size,
            last_resize_time: RefCell::new(None),
            pending_size: RefCell::new(None),
        })
    }

    /// Set current pane bounds (called from paint_browser_overlay every frame)
    pub fn set_pane_bounds(&self, x: f32, y: f32, width: u32, height: u32) {
        *self.pane_bounds.borrow_mut() = (x, y, width, height);
    }

    /// Get current pane bounds for rendering
    /// Returns (x, y, width, height) in physical pixels
    pub fn get_pane_bounds(&self) -> (f32, f32, u32, u32) {
        *self.pane_bounds.borrow()
    }

    /// Get the current logical size (DIP)
    pub fn get_size(&self) -> (u32, u32) {
        *self.size.borrow()
    }

    /// Mark that a resize just happened (for settle-and-rerender logic)
    pub fn mark_resize_time(&self) {
        *self.last_resize_time.borrow_mut() = Some(std::time::Instant::now());
    }

    /// Clear the resize time (after settle render is done)
    pub fn clear_resize_time(&self) {
        *self.last_resize_time.borrow_mut() = None;
    }

    /// Check if we're waiting for a settle render and if enough time has passed
    /// Returns Some(elapsed) if waiting, None if not waiting
    pub fn time_since_last_resize(&self) -> Option<std::time::Duration> {
        self.last_resize_time.borrow().map(|t| t.elapsed())
    }

    /// Get the pending target size (the size we want to resize to after settling)
    pub fn get_pending_size(&self) -> Option<(u32, u32)> {
        *self.pending_size.borrow()
    }

    /// Set the pending target size (called when pane size changes)
    pub fn set_pending_size(&self, width: u32, height: u32) {
        *self.pending_size.borrow_mut() = Some((width, height));
    }

    /// Clear the pending size (after resize is complete)
    pub fn clear_pending_size(&self) {
        *self.pending_size.borrow_mut() = None;
    }

    /// Get the browser host for sending events
    pub fn host(&self) -> Option<BrowserHost> {
        self.browser.host()
    }

    /// Resize the browser
    pub fn resize(&self, width: u32, height: u32) {
        log::info!(
            "[CEF] BrowserState::resize called for pane {}: {}x{}",
            self.pane_id,
            width,
            height
        );
        let old_size = *self.size.borrow();
        *self.size.borrow_mut() = (width, height);
        log::info!(
            "[CEF] BrowserState::resize: size updated from {:?} to ({}, {})",
            old_size,
            width,
            height
        );
        if let Some(host) = self.host() {
            log::info!("[CEF] BrowserState::resize: calling was_resized() on host");
            host.was_resized();
            log::info!("[CEF] BrowserState::resize: was_resized() completed, now calling invalidate()");
            // Force a repaint - CEF may go dormant after page load and not repaint on resize alone
            host.invalidate(PaintElementType::default());
            log::info!("[CEF] BrowserState::resize: invalidate() completed, now pumping CEF message loop");
            // Pump CEF message loop to ensure the resize is processed immediately
            // Without this, CEF may not process the resize until its next scheduled work
            cef::do_message_loop_work();
            log::info!("[CEF] BrowserState::resize: do_message_loop_work() completed");
        } else {
            log::error!("[CEF] BrowserState::resize: host() returned None, cannot call was_resized()");
        }
    }

    /// Send a key event to the browser
    pub fn send_key_event(&self, event: &CefKeyEvent) {
        if let Some(host) = self.host() {
            let key_event = KeyEvent {
                size: std::mem::size_of::<KeyEvent>(),
                type_: event.event_type,
                modifiers: event.modifiers,
                windows_key_code: event.windows_key_code,
                native_key_code: event.native_key_code,
                is_system_key: 0,
                character: event.character,
                unmodified_character: event.unmodified_character,
                focus_on_editable_field: 0,
            };
            host.send_key_event(Some(&key_event));
        }
    }

    /// Send focus to the browser
    pub fn set_focus(&self, focused: bool) {
        if let Some(host) = self.host() {
            host.set_focus(focused as i32);
        }
    }

    /// Close the browser
    pub fn close(&self) {
        if let Some(host) = self.host() {
            host.close_browser(1);
        }
    }

    /// Get the current texture bind group if available
    pub fn get_texture_bind_group(&self) -> Option<wgpu::BindGroup> {
        self.texture_holder.borrow().clone()
    }

    /// Check if texture is available
    pub fn has_texture(&self) -> bool {
        self.texture_holder.borrow().is_some()
    }
}

impl Drop for BrowserState {
    fn drop(&mut self) {
        log::info!("[CEF] Dropping browser for pane {}", self.pane_id);
        self.close();
    }
}

/// Key event data for CEF
pub struct CefKeyEvent {
    pub event_type: KeyEventType,
    pub modifiers: u32,
    pub windows_key_code: i32,
    pub native_key_code: i32,
    pub character: u16,
    pub unmodified_character: u16,
}

// CEF event flag constants
pub const EVENTFLAG_SHIFT_DOWN: u32 = 1 << 1;
pub const EVENTFLAG_CONTROL_DOWN: u32 = 1 << 2;
pub const EVENTFLAG_ALT_DOWN: u32 = 1 << 3;
pub const EVENTFLAG_COMMAND_DOWN: u32 = 1 << 7;

/// Convert a key code to Windows virtual key code
#[cfg(target_os = "macos")]
pub fn keycode_to_windows_vk(code: u32) -> i32 {
    // Basic mapping - extend as needed
    match code {
        0x00 => 0x41, // A
        0x01 => 0x53, // S
        0x02 => 0x44, // D
        0x03 => 0x46, // F
        0x08 => 0x43, // C - important for Ctrl+C
        0x24 => 0x0D, // Enter
        0x30 => 0x09, // Tab
        0x31 => 0x20, // Space
        0x33 => 0x08, // Backspace
        0x35 => 0x1B, // Escape
        _ => 0,
    }
}

#[cfg(not(target_os = "macos"))]
pub fn keycode_to_windows_vk(_code: u32) -> i32 {
    0
}

// ============================================================================
// CEF Render Handler
// ============================================================================

#[derive(Clone)]
struct CefRenderHandler {
    size: std::rc::Rc<RefCell<(u32, u32)>>,
    texture_holder: TextureHolder,
    device: wgpu::Device,
    queue: wgpu::Queue,
    bind_group_layout: wgpu::BindGroupLayout,
    device_scale_factor: f32,
    invalidate_callback: Arc<dyn Fn() + Send + Sync>,
}

wrap_render_handler! {
    struct CefRenderHandlerBuilder {
        handler: CefRenderHandler,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            let (width, height) = *self.handler.size.borrow();
            log::info!(
                "[CEF] view_rect called, returning {}x{}",
                width,
                height
            );
            if let Some(rect) = rect {
                if width > 0 && height > 0 {
                    rect.width = width as i32;
                    rect.height = height as i32;
                }
            }
        }

        fn screen_info(
            &self,
            _browser: Option<&mut Browser>,
            screen_info: Option<&mut ScreenInfo>,
        ) -> ::std::os::raw::c_int {
            if let Some(screen_info) = screen_info {
                screen_info.device_scale_factor = self.handler.device_scale_factor;
                return 1;
            }
            0
        }

        fn screen_point(
            &self,
            _browser: Option<&mut Browser>,
            _view_x: ::std::os::raw::c_int,
            _view_y: ::std::os::raw::c_int,
            _screen_x: Option<&mut ::std::os::raw::c_int>,
            _screen_y: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            0
        }

        // Accelerated OSR paint handler - uses shared GPU texture (IOSurface on macOS)
        #[cfg(target_os = "macos")]
        fn on_accelerated_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            info: Option<&cef::AcceleratedPaintInfo>,
        ) {
            log::info!("[CEF] on_accelerated_paint called");
            let Some(info) = info else {
                log::warn!("[CEF] on_accelerated_paint: no info provided");
                return;
            };

            if type_ != PaintElementType::default() {
                return;
            }

            // Import the shared texture
            use cef::osr_texture_import::shared_texture_handle::SharedTextureHandle;
            let shared_handle = SharedTextureHandle::new(info);
            if let SharedTextureHandle::Unsupported = shared_handle {
                log::warn!("[CEF] Platform does not support accelerated painting");
                return;
            }

            let src_texture = match shared_handle.import_texture(&self.handler.device) {
                Ok(texture) => texture,
                Err(e) => {
                    log::error!("[CEF] Failed to import shared texture: {:?}", e);
                    return;
                }
            };

            // Create sampler and bind group using the stored layout
            let sampler = self.handler.device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            });

            let bind_group = self
                .handler
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("CEF Texture Bind Group"),
                    layout: &self.handler.bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&src_texture.create_view(
                                &wgpu::TextureViewDescriptor {
                                    label: Some("CEF Texture View"),
                                    ..Default::default()
                                },
                            )),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                });

            // Store the bind group
            *self.handler.texture_holder.borrow_mut() = Some(bind_group);

            // Signal that we need a redraw
            (self.handler.invalidate_callback)();
        }

        // Software fallback paint handler - copies pixel buffer to GPU texture
        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            _type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: ::std::os::raw::c_int,
            height: ::std::os::raw::c_int,
        ) {
            log::info!("[CEF] on_paint called (software fallback) {}x{}", width, height);
            // Software fallback path
            use wgpu::{Extent3d, TextureDescriptor, TextureDimension, TextureUsages};

            if buffer.is_null() || width <= 0 || height <= 0 {
                log::warn!("[CEF] on_paint: invalid buffer or dimensions");
                return;
            }

            let buffer_size = (width * height * 4) as usize;
            let buffer_slice = unsafe { std::slice::from_raw_parts(buffer, buffer_size) };

            let texture_desc = TextureDescriptor {
                label: Some("CEF Paint Texture"),
                size: Extent3d {
                    width: width as u32,
                    height: height as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: wgpu::TextureFormat::Bgra8Unorm,
                usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                view_formats: &[],
            };

            let texture = self.handler.device.create_texture(&texture_desc);

            self.handler.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                buffer_slice,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width as u32),
                    rows_per_image: Some(height as u32),
                },
                texture_desc.size,
            );

            // Create sampler and bind group using the stored layout
            let sampler = self.handler.device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            });

            let bind_group = self
                .handler
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("CEF Texture Bind Group"),
                    layout: &self.handler.bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&texture.create_view(
                                &wgpu::TextureViewDescriptor {
                                    label: Some("CEF Texture View"),
                                    ..Default::default()
                                },
                            )),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                });

            *self.handler.texture_holder.borrow_mut() = Some(bind_group);

            // Signal that we need a redraw
            (self.handler.invalidate_callback)();
        }
    }
}

impl CefRenderHandlerBuilder {
    fn build(handler: CefRenderHandler) -> RenderHandler {
        Self::new(handler)
    }
}

// ============================================================================
// CEF Context Menu Handler (suppresses context menu to avoid crashes)
// ============================================================================

#[derive(Clone)]
struct CefContextMenuHandler {}

wrap_context_menu_handler! {
    struct CefContextMenuHandlerBuilder {
        handler: CefContextMenuHandler,
    }

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut cef::Frame>,
            _params: Option<&mut cef::ContextMenuParams>,
            model: Option<&mut cef::MenuModel>,
        ) {
            // Clear the menu to suppress context menu (avoids winit crash)
            if let Some(model) = model {
                model.clear();
            }
        }
    }
}

impl CefContextMenuHandlerBuilder {
    fn build() -> ContextMenuHandler {
        Self::new(CefContextMenuHandler {})
    }
}

// ============================================================================
// CEF Client
// ============================================================================

wrap_client! {
    struct CefClientBuilder {
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

impl CefClientBuilder {
    fn build(render_handler: CefRenderHandler) -> Client {
        Self::new(
            CefRenderHandlerBuilder::build(render_handler),
            CefContextMenuHandlerBuilder::build(),
        )
    }
}

// ============================================================================
// CEF Request Context Handler
// ============================================================================

#[derive(Clone)]
struct CefRequestContextHandler {}

wrap_request_context_handler! {
    struct CefRequestContextHandlerBuilder {
        handler: CefRequestContextHandler,
    }

    impl RequestContextHandler {}
}

impl CefRequestContextHandlerBuilder {
    fn build() -> RequestContextHandler {
        Self::new(CefRequestContextHandler {})
    }
}
