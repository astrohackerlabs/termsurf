//! cef-test GUI — window with split-view wgpu rendering.
//!
//! Creates a single window and renders two browser halves side by side.
//! Phase 7: spawns two independent profile servers (github.com on left,
//! google.com on right), each in its own process with its own CEF instance.
//! Both send IOSurface Mach ports via XPC, GUI imports and renders both.

use std::sync::{Arc, Mutex};
use std::time::Duration;
use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    platform::pump_events::{EventLoopExtPumpEvents, PumpStatus},
    window::{Window, WindowAttributes, WindowId},
};

#[cfg(target_os = "macos")]
use termsurf_xpc::*;

// ============================================================================
// CFRunLoop (pump main dispatch queue for XPC callbacks)
// ============================================================================

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
    /// one source is handled or the timeout expires. This pumps the main
    /// dispatch queue, allowing XPC callbacks to fire.
    pub fn run_for(seconds: f64) -> i32 {
        unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, seconds, 1) }
    }
}

// ============================================================================
// Pending Surfaces (shared between XPC callbacks and main loop)
// ============================================================================

#[cfg(target_os = "macos")]
struct PendingSurface {
    mach_port: u32,
    width: u32,
    height: u32,
}

/// Two pending surface slots — one per browser half.
#[cfg(target_os = "macos")]
struct PendingSurfaces {
    left: Option<PendingSurface>,
    right: Option<PendingSurface>,
}

#[cfg(target_os = "macos")]
type SharedPendingSurfaces = Arc<Mutex<PendingSurfaces>>;

// ============================================================================
// Vertex
// ============================================================================

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Left half vertices: NDC x in [-1, 0]
const LEFT_QUAD: [Vertex; 4] = [
    Vertex { position: [-1.0,  1.0, 1.0], tex_coords: [0.0, 0.0] },
    Vertex { position: [ 0.0,  1.0, 1.0], tex_coords: [1.0, 0.0] },
    Vertex { position: [-1.0, -1.0, 1.0], tex_coords: [0.0, 1.0] },
    Vertex { position: [ 0.0, -1.0, 1.0], tex_coords: [1.0, 1.0] },
];

/// Right half vertices: NDC x in [0, +1]
const RIGHT_QUAD: [Vertex; 4] = [
    Vertex { position: [ 0.0,  1.0, 1.0], tex_coords: [0.0, 0.0] },
    Vertex { position: [ 1.0,  1.0, 1.0], tex_coords: [1.0, 0.0] },
    Vertex { position: [ 0.0, -1.0, 1.0], tex_coords: [0.0, 1.0] },
    Vertex { position: [ 1.0, -1.0, 1.0], tex_coords: [1.0, 1.0] },
];

// ============================================================================
// GPU State
// ============================================================================

struct GpuState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    left_vbuf: wgpu::Buffer,
    right_vbuf: wgpu::Buffer,
    left_bind_group: wgpu::BindGroup,
    right_bind_group: wgpu::BindGroup,
    size: winit::dpi::PhysicalSize<u32>,
}

impl GpuState {
    async fn new(window: Arc<Window>) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            #[cfg(target_os = "macos")]
            backends: wgpu::Backends::from_comma_list("metal"),
            #[cfg(not(target_os = "macos"))]
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();

        let size = window.inner_size();
        let surface = instance.create_surface(window).unwrap();
        let surface_format = wgpu::TextureFormat::Bgra8Unorm;

        // Bind group layout: texture + sampler
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Texture Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // Shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        // Pipeline
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent::OVER,
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Cw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        // Vertex buffers
        let left_vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Left Quad"),
            contents: bytemuck::cast_slice(&LEFT_QUAD),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let right_vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Right Quad"),
            contents: bytemuck::cast_slice(&RIGHT_QUAD),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Shared sampler
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        // Placeholder solid-color textures (BGRA format: [B, G, R, A])
        let left_bind_group = create_solid_bind_group(
            &device, &queue, &bind_group_layout, &sampler,
            [128, 0, 0, 255], // dark blue
        );
        let right_bind_group = create_solid_bind_group(
            &device, &queue, &bind_group_layout, &sampler,
            [0, 128, 0, 255], // dark green
        );

        let state = Self {
            device,
            queue,
            surface,
            surface_format,
            pipeline,
            bind_group_layout,
            sampler,
            left_vbuf,
            right_vbuf,
            left_bind_group,
            right_bind_group,
            size,
        };
        state.configure_surface();
        state
    }

    fn configure_surface(&self) {
        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.surface_format,
                view_formats: vec![self.surface_format],
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                width: self.size.width,
                height: self.size.height,
                desired_maximum_frame_latency: 2,
                present_mode: wgpu::PresentMode::AutoVsync,
            },
        );
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.configure_surface();
        }
    }

    /// Import an IOSurface from a Mach port and create a bind group.
    #[cfg(target_os = "macos")]
    fn import_surface(&self, pending: &PendingSurface) -> Option<wgpu::BindGroup> {
        use cef::osr_texture_import::iosurface::IOSurfaceImporter;
        use cef::osr_texture_import::TextureImporter;
        use cef::sys::cef_color_type_t;

        let importer = match IOSurfaceImporter::from_mach_port(
            pending.mach_port,
            cef_color_type_t::CEF_COLOR_TYPE_BGRA_8888,
            pending.width,
            pending.height,
        ) {
            Some(i) => i,
            None => {
                eprintln!(
                    "GUI: IOSurfaceLookupFromMachPort failed (port={})",
                    pending.mach_port
                );
                return None;
            }
        };

        let texture = match importer.import_to_wgpu(&self.device) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("GUI: Failed to import texture: {:?}", e);
                return None;
            }
        };

        // sRGB view for correct color interpretation
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Bgra8UnormSrgb),
            ..Default::default()
        });

        Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("IOSurface Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        }))
    }

    fn render(&self) {
        let surface_texture = self
            .surface
            .get_current_texture()
            .expect("failed to acquire swapchain texture");

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor {
                format: Some(self.surface_format),
                ..Default::default()
            });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                ..Default::default()
            });

            pass.set_pipeline(&self.pipeline);

            // Draw left half
            pass.set_bind_group(0, &self.left_bind_group, &[]);
            pass.set_vertex_buffer(0, self.left_vbuf.slice(..));
            pass.draw(0..4, 0..1);

            // Draw right half
            pass.set_bind_group(0, &self.right_bind_group, &[]);
            pass.set_vertex_buffer(0, self.right_vbuf.slice(..));
            pass.draw(0..4, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
    }
}

/// Create a bind group from a 1x1 solid-color texture (BGRA format).
fn create_solid_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    color_bgra: [u8; 4],
) -> wgpu::BindGroup {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Solid Color"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &color_bgra,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Solid Color Bind Group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

// ============================================================================
// XPC Bootstrap
// ============================================================================

/// State for tracking XPC connections to profile servers.
#[cfg(target_os = "macos")]
struct XpcState {
    /// Connection to the launcher (must keep alive)
    _launcher: XpcConnection,
    /// Listeners (must keep alive to accept connections)
    _listeners: Vec<XpcListener>,
    /// Direct connections from profile servers (must keep alive)
    _connections: Mutex<Vec<Arc<XpcConnection>>>,
}

/// Create a listener for one profile slot and wire up display_surface handling.
/// Returns the listener and the endpoint to send to the launcher.
#[cfg(target_os = "macos")]
fn create_profile_listener(
    label: &'static str,
    pending: SharedPendingSurfaces,
    is_left: bool,
) -> Option<(XpcListener, XpcEndpoint, Arc<Mutex<Vec<Arc<XpcConnection>>>>)> {
    let listener = match XpcListener::new_anonymous() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("GUI: Failed to create {} listener: {}", label, e);
            return None;
        }
    };

    let endpoint = match listener.get_endpoint() {
        Ok(ep) => ep,
        Err(e) => {
            eprintln!("GUI: Failed to get {} endpoint: {}", label, e);
            return None;
        }
    };

    let conns: Arc<Mutex<Vec<Arc<XpcConnection>>>> = Arc::new(Mutex::new(Vec::new()));
    let conns_for_handler = Arc::clone(&conns);

    set_new_connection_handler(&listener, move |conn| {
        println!("GUI: Profile '{}' connected", label);
        let conn = Arc::new(conn);
        let pending = Arc::clone(&pending);

        set_event_handler(&*conn, move |event| {
            match event {
                Ok(msg) => {
                    let action = msg.get_string("action").unwrap_or_default();

                    if action == "display_surface" {
                        let port = msg.copy_mach_send("iosurface_port");
                        let width = msg.get_i64("width") as u32;
                        let height = msg.get_i64("height") as u32;
                        let frame_id = msg.get_i64("frame_id");

                        if port == 0 {
                            eprintln!("GUI: [{}] null Mach port for frame {}", label, frame_id);
                            return;
                        }

                        let mut guard = pending.lock().unwrap();
                        let slot = if is_left {
                            &mut guard.left
                        } else {
                            &mut guard.right
                        };
                        *slot = Some(PendingSurface {
                            mach_port: port,
                            width,
                            height,
                        });
                    }
                }
                Err(e) => {
                    eprintln!("GUI: [{}] connection error: {}", label, e);
                }
            }
        });
        conn.resume();

        conns_for_handler.lock().unwrap().push(conn);
    });

    listener.resume();
    Some((listener, endpoint, conns))
}

/// Connect to the launcher and spawn both profile servers.
#[cfg(target_os = "macos")]
fn bootstrap_xpc(pending: SharedPendingSurfaces) -> Option<XpcState> {
    println!("GUI: Connecting to launcher...");

    let launcher = match XpcConnection::connect_mach_service("com.cef-test.launcher") {
        Ok(l) => l,
        Err(e) => {
            eprintln!("GUI: Failed to connect to launcher: {}", e);
            return None;
        }
    };

    set_event_handler(&launcher, |event| {
        if let Err(e) = event {
            eprintln!("GUI: Launcher connection error: {}", e);
        }
    });
    launcher.resume();
    println!("GUI: Connected to launcher");

    let mut listeners = Vec::new();
    let mut all_conns: Vec<Arc<Mutex<Vec<Arc<XpcConnection>>>>> = Vec::new();

    // Left profile: github.com
    let (left_listener, left_endpoint, left_conns) =
        create_profile_listener("left", Arc::clone(&pending), true)?;
    listeners.push(left_listener);
    all_conns.push(left_conns);

    // Right profile: google.com
    let (right_listener, right_endpoint, right_conns) =
        create_profile_listener("right", Arc::clone(&pending), false)?;
    listeners.push(right_listener);
    all_conns.push(right_conns);

    // Spawn left profile
    println!("GUI: Requesting profile 'left' (session=left-1, url=github.com)");
    let msg = XpcDictionary::new();
    msg.set_string("action", "spawn_profile");
    msg.set_string("session_id", "left-1");
    msg.set_string("url", "https://github.com");
    msg.set_string("profile", "left");
    msg.set_i64("width", 800);
    msg.set_i64("height", 800);
    msg.set_string("scale", "2.0");
    msg.set_endpoint("gui_endpoint", left_endpoint);
    launcher.send(&msg);

    // Spawn right profile
    println!("GUI: Requesting profile 'right' (session=right-1, url=google.com)");
    let msg = XpcDictionary::new();
    msg.set_string("action", "spawn_profile");
    msg.set_string("session_id", "right-1");
    msg.set_string("url", "https://google.com");
    msg.set_string("profile", "right");
    msg.set_i64("width", 800);
    msg.set_i64("height", 800);
    msg.set_string("scale", "2.0");
    msg.set_endpoint("gui_endpoint", right_endpoint);
    launcher.send(&msg);

    println!("GUI: Sent both spawn_profile requests");

    // Merge all connection trackers into one Vec
    let merged_conns: Mutex<Vec<Arc<XpcConnection>>> = Mutex::new(Vec::new());

    Some(XpcState {
        _launcher: launcher,
        _listeners: listeners,
        _connections: merged_conns,
    })
}

// ============================================================================
// App
// ============================================================================

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuState>,
    #[cfg(target_os = "macos")]
    pending: SharedPendingSurfaces,
    #[cfg(target_os = "macos")]
    _xpc: Option<XpcState>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("cef-test")
                        .with_inner_size(winit::dpi::LogicalSize::new(1600.0, 800.0)),
                )
                .unwrap(),
        );

        let gpu = pollster::block_on(GpuState::new(window.clone()));
        println!(
            "GUI: Window created ({}x{} physical)",
            gpu.size.width, gpu.size.height
        );

        // Bootstrap XPC after window creation
        #[cfg(target_os = "macos")]
        {
            self._xpc = bootstrap_xpc(Arc::clone(&self.pending));
        }

        self.window = Some(window);
        self.gpu = Some(gpu);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                println!("GUI: Window closed");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                if let Some(gpu) = &self.gpu {
                    gpu.render();
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size);
                    println!("GUI: Resized to {}x{}", size.width, size.height);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

impl App {
    /// Check for pending IOSurfaces from profile servers and import them.
    #[cfg(target_os = "macos")]
    fn process_pending_surfaces(&mut self) {
        let (left, right) = {
            let mut guard = self.pending.lock().unwrap();
            (guard.left.take(), guard.right.take())
        };

        let mut needs_redraw = false;

        if let Some(surface) = left {
            if let Some(gpu) = &mut self.gpu {
                if let Some(bind_group) = gpu.import_surface(&surface) {
                    gpu.left_bind_group = bind_group;
                    needs_redraw = true;
                }
            }
        }

        if let Some(surface) = right {
            if let Some(gpu) = &mut self.gpu {
                if let Some(bind_group) = gpu.import_surface(&surface) {
                    gpu.right_bind_group = bind_group;
                    needs_redraw = true;
                }
            }
        }

        if needs_redraw {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}

fn main() {
    println!("GUI: Starting...");
    let mut event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    #[cfg(target_os = "macos")]
    let pending: SharedPendingSurfaces = Arc::new(Mutex::new(PendingSurfaces {
        left: None,
        right: None,
    }));

    let mut app = App {
        window: None,
        gpu: None,
        #[cfg(target_os = "macos")]
        pending,
        #[cfg(target_os = "macos")]
        _xpc: None,
    };

    loop {
        let status = event_loop.pump_app_events(Some(Duration::from_millis(1)), &mut app);

        #[cfg(target_os = "macos")]
        cfrunloop::run_for(0.001);

        #[cfg(target_os = "macos")]
        app.process_pending_surfaces();

        if let PumpStatus::Exit(_) = status {
            break;
        }
    }

    println!("GUI: Done");
}
