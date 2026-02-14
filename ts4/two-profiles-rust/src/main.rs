// Copyright 2025 TermSurf
// Rust receiver: receives IOSurface Mach ports via XPC and renders them with wgpu.
// Part of Issue 416 Experiment 3: two-pane side-by-side receiver.

#[macro_use]
extern crate objc;

use std::ffi::c_void;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use termsurf_xpc::{
    iosurface, set_event_handler, set_new_connection_handler, XpcConnection,
    XpcListener,
};
use winit::dpi::LogicalSize;

// ---------------------------------------------------------------------------
// IOSurface FFI (not covered by termsurf-xpc)
// ---------------------------------------------------------------------------

#[link(name = "IOSurface", kind = "framework")]
extern "C" {
    fn IOSurfaceGetWidth(buffer: *const c_void) -> usize;
    fn IOSurfaceGetHeight(buffer: *const c_void) -> usize;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *mut c_void);
}

// ---------------------------------------------------------------------------
// Send wrapper for raw pointers (IOSurface handles are kernel objects,
// safe to access from any thread)
// ---------------------------------------------------------------------------

struct SendPtr(*mut c_void);
unsafe impl Send for SendPtr {}
unsafe impl Sync for SendPtr {}

// ---------------------------------------------------------------------------
// Pane mapping
// ---------------------------------------------------------------------------

const LEFT: usize = 0;
const RIGHT: usize = 1;

fn pane_for_session(session_id: Option<&str>) -> usize {
    if session_id == Some("profile-b") { RIGHT } else { LEFT }
}

// ---------------------------------------------------------------------------
// Global state shared between XPC queue and main thread
// ---------------------------------------------------------------------------

static PENDING_SURFACE_LEFT: Mutex<Option<SendPtr>> = Mutex::new(None);
static PENDING_SURFACE_RIGHT: Mutex<Option<SendPtr>> = Mutex::new(None);
static EVENT_PROXY: OnceLock<winit::event_loop::EventLoopProxy<()>> = OnceLock::new();
static PEERS: Mutex<Vec<XpcConnection>> = Mutex::new(Vec::new());
static FRAME_COUNT: Mutex<u32> = Mutex::new(0);
static LAST_LOG_TIME: Mutex<Option<Instant>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// XPC message handler
// ---------------------------------------------------------------------------

fn handle_message(dict: termsurf_xpc::XpcDictionary) {
    let action = dict.get_string("action");
    match action.as_deref() {
        Some("display_surface") => {}
        Some("register") => {
            let sid = dict.get_string("session_id");
            eprintln!(
                "[Receiver] Profile server registered: {}",
                sid.as_deref().unwrap_or("(no session_id)")
            );
            return;
        }
        _ => return,
    }

    let port = dict.copy_mach_send("iosurface_port");
    if port == 0 {
        eprintln!("[Receiver] null Mach port");
        return;
    }

    let surface = iosurface::lookup_from_mach_port(port);
    iosurface::deallocate_mach_port(port);

    let Some(surface) = surface else {
        eprintln!("[Receiver] IOSurfaceLookupFromMachPort failed");
        return;
    };

    // FPS logging.
    {
        let mut count = FRAME_COUNT.lock().unwrap();
        *count += 1;
        let mut last = LAST_LOG_TIME.lock().unwrap();
        let now = Instant::now();
        if last.is_none() {
            *last = Some(now);
        }
        let elapsed = now.duration_since(last.unwrap()).as_secs_f64();
        if elapsed >= 1.0 {
            let w = unsafe { IOSurfaceGetWidth(surface) };
            let h = unsafe { IOSurfaceGetHeight(surface) };
            let fps = *count as f64 / elapsed;
            eprintln!(
                "[Receiver] {} frames ({:.1} fps) | IOSurface {}x{}",
                *count, fps, w, h
            );
            *count = 0;
            *last = Some(now);
        }
    }

    // Route to correct pane based on session_id.
    let session_id = dict.get_string("session_id");
    let pane = pane_for_session(session_id.as_deref());
    let slot = if pane == LEFT {
        &PENDING_SURFACE_LEFT
    } else {
        &PENDING_SURFACE_RIGHT
    };
    let old = slot.lock().unwrap().replace(SendPtr(surface));
    if let Some(SendPtr(old_ptr)) = old {
        unsafe { CFRelease(old_ptr) };
    }

    // Wake event loop to trigger redraw.
    if let Some(proxy) = EVENT_PROXY.get() {
        proxy.send_event(()).ok();
    }
}

// ---------------------------------------------------------------------------
// XPC listener
// ---------------------------------------------------------------------------

fn start_xpc_listener() {
    let listener =
        XpcListener::new_mach_service("com.termsurf.two-profiles-rust")
            .expect("Failed to create XPC Mach service listener");

    set_new_connection_handler(&listener, |peer: XpcConnection| {
        set_event_handler(&peer, |result| match result {
            Ok(dict) => handle_message(dict),
            Err(e) => eprintln!("[Receiver] XPC error: {:?}", e),
        });
        peer.resume();

        let mut peers = PEERS.lock().unwrap();
        peers.push(peer);
        eprintln!(
            "[Receiver] Profile server connected ({} total)",
            peers.len()
        );
    });

    listener.resume();
    eprintln!("[Receiver] Listening on com.termsurf.two-profiles-rust...");

    // Leak to prevent Drop from canceling the listener.
    std::mem::forget(listener);
}

// ---------------------------------------------------------------------------
// IOSurface -> wgpu texture import (adapted from cef-rs)
// ---------------------------------------------------------------------------

fn import_iosurface(
    device: &wgpu::Device,
    surface: *mut c_void,
) -> Option<wgpu::Texture> {
    use metal::{MTLPixelFormat, MTLTextureType, MTLTextureUsage};

    let width = unsafe { IOSurfaceGetWidth(surface) } as u32;
    let height = unsafe { IOSurfaceGetHeight(surface) } as u32;
    if width == 0 || height == 0 {
        return None;
    }

    let texture_desc = wgpu::TextureDescriptor {
        label: Some("IOSurface"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    };

    unsafe {
        // 1. Get Metal device from wgpu HAL.
        let hal_device_guard = device.as_hal::<wgpu::wgc::api::Metal>();
        let hal_device = hal_device_guard?;
        let raw_device = hal_device.raw_device();

        // 2. Create Metal texture descriptor.
        let metal_desc = metal::TextureDescriptor::new();
        metal_desc.set_width(width as u64);
        metal_desc.set_height(height as u64);
        metal_desc.set_pixel_format(MTLPixelFormat::BGRA8Unorm_sRGB);
        metal_desc.set_texture_type(MTLTextureType::D2);
        metal_desc.set_usage(MTLTextureUsage::ShaderRead);

        // 3. Create Metal texture from IOSurface.
        let device_ref: &metal::DeviceRef = raw_device;
        let desc_ref: &metal::TextureDescriptorRef = metal_desc.as_ref();
        let metal_tex: metal::Texture = objc::msg_send![
            device_ref,
            newTextureWithDescriptor:desc_ref
            iosurface:surface
            plane:0usize
        ];

        // 4. Wrap as wgpu HAL texture.
        let hal_tex =
            <wgpu::wgc::api::Metal as wgpu::hal::Api>::Device::texture_from_raw(
                metal_tex,
                texture_desc.format,
                MTLTextureType::D2,
                texture_desc.array_layer_count(),
                texture_desc.mip_level_count,
                wgpu::hal::CopyExtent {
                    width,
                    height,
                    depth: texture_desc.array_layer_count(),
                },
            );

        // 5. Wrap as wgpu texture.
        Some(
            device.create_texture_from_hal::<wgpu::wgc::api::Metal>(
                hal_tex,
                &texture_desc,
            ),
        )
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    // Start XPC listener before the event loop.
    start_xpc_listener();

    // Create event loop with user events for XPC -> render signaling.
    let event_loop = winit::event_loop::EventLoop::<()>::builder()
        .build()
        .expect("Failed to create event loop");

    // Store proxy for XPC thread.
    EVENT_PROXY
        .set(event_loop.create_proxy())
        .expect("Failed to store event proxy");

    // wgpu instance.
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        ..Default::default()
    });

    // State initialized on first Resumed event.
    let mut window: Option<Arc<winit::window::Window>> = None;
    let mut wgpu_surface: Option<wgpu::Surface<'static>> = None;
    let mut device: Option<wgpu::Device> = None;
    let mut queue: Option<wgpu::Queue> = None;
    let mut pipeline: Option<wgpu::RenderPipeline> = None;
    let mut bind_group_layout: Option<wgpu::BindGroupLayout> = None;
    let mut sampler: Option<wgpu::Sampler> = None;
    let mut surface_format = wgpu::TextureFormat::Bgra8UnormSrgb;
    let mut current_texture: [Option<wgpu::Texture>; 2] = [None, None];

    event_loop
        .run(move |event, elwt| {
            use winit::event::{Event, WindowEvent};

            match event {
                Event::Resumed => {
                    if window.is_some() {
                        return;
                    }

                    // Create window.
                    let attrs = winit::window::Window::default_attributes()
                        .with_title("Rust Receiver")
                        .with_inner_size(LogicalSize::new(1600.0, 600.0));
                    let win = Arc::new(
                        elwt.create_window(attrs)
                            .expect("Failed to create window"),
                    );

                    // Create wgpu surface.
                    let surf = instance
                        .create_surface(win.clone())
                        .expect("Failed to create wgpu surface");

                    // Adapter.
                    let adapter = pollster::block_on(instance.request_adapter(
                        &wgpu::RequestAdapterOptions {
                            power_preference: wgpu::PowerPreference::default(),
                            compatible_surface: Some(&surf),
                            force_fallback_adapter: false,
                        },
                    ))
                    .expect("Failed to find GPU adapter");

                    // Device + queue.
                    let (dev, q) = pollster::block_on(adapter.request_device(
                        &wgpu::DeviceDescriptor {
                            label: Some("device"),
                            required_features: wgpu::Features::empty(),
                            required_limits: wgpu::Limits::default(),
                            memory_hints: wgpu::MemoryHints::default(),
                            trace: Default::default(),
                            experimental_features: Default::default(),
                        },
                    ))
                    .expect("Failed to create device");

                    // Configure surface.
                    let caps = surf.get_capabilities(&adapter);
                    surface_format = caps
                        .formats
                        .iter()
                        .find(|f| f.is_srgb())
                        .copied()
                        .unwrap_or(caps.formats[0]);
                    let size = win.inner_size();
                    surf.configure(
                        &dev,
                        &wgpu::SurfaceConfiguration {
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                            format: surface_format,
                            width: size.width,
                            height: size.height,
                            present_mode: wgpu::PresentMode::AutoVsync,
                            alpha_mode: caps.alpha_modes[0],
                            view_formats: vec![],
                            desired_maximum_frame_latency: 2,
                        },
                    );

                    // Shader.
                    let shader_module =
                        dev.create_shader_module(wgpu::ShaderModuleDescriptor {
                            label: Some("shader"),
                            source: wgpu::ShaderSource::Wgsl(
                                include_str!("shaders.wgsl").into(),
                            ),
                        });

                    // Bind group layout.
                    let bgl = dev.create_bind_group_layout(
                        &wgpu::BindGroupLayoutDescriptor {
                            label: Some("bind_group_layout"),
                            entries: &[
                                wgpu::BindGroupLayoutEntry {
                                    binding: 0,
                                    visibility: wgpu::ShaderStages::FRAGMENT,
                                    ty: wgpu::BindingType::Texture {
                                        sample_type:
                                            wgpu::TextureSampleType::Float {
                                                filterable: true,
                                            },
                                        view_dimension:
                                            wgpu::TextureViewDimension::D2,
                                        multisampled: false,
                                    },
                                    count: None,
                                },
                                wgpu::BindGroupLayoutEntry {
                                    binding: 1,
                                    visibility: wgpu::ShaderStages::FRAGMENT,
                                    ty: wgpu::BindingType::Sampler(
                                        wgpu::SamplerBindingType::Filtering,
                                    ),
                                    count: None,
                                },
                            ],
                        },
                    );

                    // Pipeline.
                    let pl = dev.create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("pipeline_layout"),
                            bind_group_layouts: &[&bgl],
                            immediate_size: 0,
                        },
                    );
                    let rp = dev.create_render_pipeline(
                        &wgpu::RenderPipelineDescriptor {
                            label: Some("render_pipeline"),
                            layout: Some(&pl),
                            vertex: wgpu::VertexState {
                                module: &shader_module,
                                entry_point: Some("vs_main"),
                                buffers: &[],
                                compilation_options: Default::default(),
                            },
                            fragment: Some(wgpu::FragmentState {
                                module: &shader_module,
                                entry_point: Some("fs_main"),
                                targets: &[Some(wgpu::ColorTargetState {
                                    format: surface_format,
                                    blend: None,
                                    write_mask: wgpu::ColorWrites::ALL,
                                })],
                                compilation_options: Default::default(),
                            }),
                            primitive: wgpu::PrimitiveState {
                                topology:
                                    wgpu::PrimitiveTopology::TriangleStrip,
                                ..Default::default()
                            },
                            depth_stencil: None,
                            multisample: wgpu::MultisampleState::default(),
                            multiview_mask: None,
                            cache: None,
                        },
                    );

                    // Sampler.
                    let s = dev.create_sampler(&wgpu::SamplerDescriptor {
                        label: Some("sampler"),
                        mag_filter: wgpu::FilterMode::Linear,
                        min_filter: wgpu::FilterMode::Linear,
                        ..Default::default()
                    });

                    eprintln!("[Receiver] Window and wgpu pipeline ready");

                    window = Some(win);
                    wgpu_surface = Some(surf);
                    device = Some(dev);
                    queue = Some(q);
                    pipeline = Some(rp);
                    bind_group_layout = Some(bgl);
                    sampler = Some(s);
                }

                Event::UserEvent(()) => {
                    if let Some(ref win) = window {
                        win.request_redraw();
                    }
                }

                Event::WindowEvent {
                    event: WindowEvent::RedrawRequested,
                    ..
                } => {
                    let Some(ref win) = window else { return };
                    let Some(ref dev) = device else { return };
                    let Some(ref q) = queue else { return };
                    let Some(ref surf) = wgpu_surface else { return };
                    let Some(ref pip) = pipeline else { return };
                    let Some(ref bgl) = bind_group_layout else { return };
                    let Some(ref samp) = sampler else { return };

                    // Check for new IOSurfaces (both panes).
                    for (i, slot) in [
                        &PENDING_SURFACE_LEFT,
                        &PENDING_SURFACE_RIGHT,
                    ]
                    .iter()
                    .enumerate()
                    {
                        if let Some(SendPtr(ptr)) =
                            slot.lock().unwrap().take()
                        {
                            if let Some(tex) = import_iosurface(dev, ptr) {
                                current_texture[i] = Some(tex);
                            }
                            unsafe { CFRelease(ptr) };
                        }
                    }

                    // Need at least one pane to render.
                    if current_texture[LEFT].is_none()
                        && current_texture[RIGHT].is_none()
                    {
                        return;
                    }

                    let output = match surf.get_current_texture() {
                        Ok(t) => t,
                        Err(e) => {
                            eprintln!("[Receiver] Surface error: {:?}", e);
                            return;
                        }
                    };

                    let view = output
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());

                    let mut encoder = dev.create_command_encoder(
                        &wgpu::CommandEncoderDescriptor {
                            label: Some("encoder"),
                        },
                    );

                    {
                        let mut pass = encoder.begin_render_pass(
                            &wgpu::RenderPassDescriptor {
                                label: Some("render_pass"),
                                color_attachments: &[Some(
                                    wgpu::RenderPassColorAttachment {
                                        view: &view,
                                        resolve_target: None,
                                        ops: wgpu::Operations {
                                            load: wgpu::LoadOp::Clear(
                                                wgpu::Color::BLACK,
                                            ),
                                            store: wgpu::StoreOp::Store,
                                        },
                                        depth_slice: None,
                                    },
                                )],
                                depth_stencil_attachment: None,
                                multiview_mask: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                            },
                        );

                        pass.set_pipeline(pip);

                        let size = win.inner_size();
                        let half_w = size.width as f32 / 2.0;
                        let full_h = size.height as f32;

                        for (i, tex) in
                            current_texture.iter().enumerate()
                        {
                            if let Some(ref tex) = tex {
                                let x = i as f32 * half_w;
                                pass.set_viewport(
                                    x, 0.0, half_w, full_h,
                                    0.0, 1.0,
                                );

                                let tex_view = tex.create_view(
                                    &Default::default(),
                                );
                                let bind_group = dev.create_bind_group(
                                    &wgpu::BindGroupDescriptor {
                                        label: Some("bind_group"),
                                        layout: bgl,
                                        entries: &[
                                            wgpu::BindGroupEntry {
                                                binding: 0,
                                                resource:
                                                    wgpu::BindingResource::TextureView(
                                                        &tex_view,
                                                    ),
                                            },
                                            wgpu::BindGroupEntry {
                                                binding: 1,
                                                resource:
                                                    wgpu::BindingResource::Sampler(
                                                        samp,
                                                    ),
                                            },
                                        ],
                                    },
                                );
                                pass.set_bind_group(
                                    0, &bind_group, &[],
                                );
                                pass.draw(0..4, 0..1);
                            }
                        }
                    }

                    q.submit(std::iter::once(encoder.finish()));
                    output.present();
                }

                Event::WindowEvent {
                    event: WindowEvent::Resized(size),
                    ..
                } => {
                    if size.width > 0 && size.height > 0 {
                        if let Some(ref dev) = device {
                            if let Some(ref surf) = wgpu_surface {
                                surf.configure(
                                    dev,
                                    &wgpu::SurfaceConfiguration {
                                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                                        format: surface_format,
                                        width: size.width,
                                        height: size.height,
                                        present_mode:
                                            wgpu::PresentMode::AutoVsync,
                                        alpha_mode:
                                            wgpu::CompositeAlphaMode::Auto,
                                        view_formats: vec![],
                                        desired_maximum_frame_latency: 2,
                                    },
                                );
                            }
                        }
                    }
                }

                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    elwt.exit();
                }

                _ => {}
            }
        })
        .unwrap();
}
