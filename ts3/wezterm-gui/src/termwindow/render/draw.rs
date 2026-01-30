use crate::colorease::ColorEaseUniform;
use crate::termwindow::webgpu::ShaderUniform;
use crate::termwindow::RenderFrame;
use crate::uniforms::UniformBuilder;
use ::window::glium;
use ::window::glium::uniforms::{
    MagnifySamplerFilter, MinifySamplerFilter, Sampler, SamplerWrapFunction,
};
use ::window::glium::{BlendingFunction, LinearBlendingFactor, Surface};
#[cfg(target_os = "macos")]
use ::window::WindowOps;
use config::{DimensionContext, FreeTypeLoadTarget};

impl crate::TermWindow {
    pub fn call_draw(&mut self, frame: &mut RenderFrame) -> anyhow::Result<()> {
        // Log once which backend is being used
        #[cfg(target_os = "macos")]
        {
            use std::sync::Once;
            static LOGGED: Once = Once::new();
            LOGGED.call_once(|| {
                let backend = match frame {
                    RenderFrame::Glium(_) => "Glium (OpenGL)",
                    RenderFrame::WebGpu => "WebGpu",
                };
                log::info!("[Render] Using {} backend for rendering", backend);
            });
        }

        match frame {
            RenderFrame::Glium(ref mut frame) => self.call_draw_glium(frame),
            RenderFrame::WebGpu => self.call_draw_webgpu(),
        }
    }

    fn call_draw_webgpu(&mut self) -> anyhow::Result<()> {
        use crate::termwindow::webgpu::WebGpuTexture;

        let webgpu = self.webgpu.as_ref().unwrap();
        let render_state = self.render_state.as_ref().unwrap();

        let output = webgpu.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = webgpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });
        let tex = render_state.glyph_cache.borrow().atlas.texture();
        let tex = tex.downcast_ref::<WebGpuTexture>().unwrap();
        let texture_view = tex.create_view(&wgpu::TextureViewDescriptor::default());

        let texture_linear_bind_group =
            webgpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &webgpu.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&webgpu.texture_linear_sampler),
                    },
                ],
                label: Some("linear bind group"),
            });

        let texture_nearest_bind_group =
            webgpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &webgpu.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&webgpu.texture_nearest_sampler),
                    },
                ],
                label: Some("nearest bind group"),
            });

        let mut cleared = false;
        let foreground_text_hsb = self.config.foreground_text_hsb;
        let foreground_text_hsb = [
            foreground_text_hsb.hue,
            foreground_text_hsb.saturation,
            foreground_text_hsb.brightness,
        ];

        let milliseconds = self.created.elapsed().as_millis() as u32;
        let projection = euclid::Transform3D::<f32, f32, f32>::ortho(
            -(self.dimensions.pixel_width as f32) / 2.0,
            self.dimensions.pixel_width as f32 / 2.0,
            self.dimensions.pixel_height as f32 / 2.0,
            -(self.dimensions.pixel_height as f32) / 2.0,
            -1.0,
            1.0,
        )
        .to_arrays_transposed();

        for layer in render_state.layers.borrow().iter() {
            for idx in 0..3 {
                let vb = &layer.vb.borrow()[idx];
                let (vertex_count, index_count) = vb.vertex_index_count();
                let vertex_buffer;
                let uniforms;
                if vertex_count > 0 {
                    let mut vertices = vb.current_vb_mut();
                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: if cleared {
                                    wgpu::LoadOp::Load
                                } else {
                                    wgpu::LoadOp::Clear(wgpu::Color {
                                        r: 0.,
                                        g: 0.,
                                        b: 0.,
                                        a: 0.,
                                    })
                                },
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        occlusion_query_set: None,
                        timestamp_writes: None,
                        multiview_mask: None,
                    });
                    cleared = true;

                    uniforms = webgpu.create_uniform(ShaderUniform {
                        foreground_text_hsb,
                        milliseconds,
                        projection,
                    });

                    render_pass.set_pipeline(&webgpu.render_pipeline);
                    render_pass.set_bind_group(0, &uniforms, &[]);
                    render_pass.set_bind_group(1, &texture_linear_bind_group, &[]);
                    render_pass.set_bind_group(2, &texture_nearest_bind_group, &[]);
                    vertex_buffer = vertices.webgpu_mut().recreate();
                    vertex_buffer.unmap();
                    render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    render_pass
                        .set_index_buffer(vb.indices.webgpu().slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..index_count as _, 0, 0..1);
                }

                vb.next_index();
            }
        }

        // submit will accept anything that implements IntoIter
        webgpu.queue.submit(std::iter::once(encoder.finish()));

        // Render webview overlays after main content (Experiment 2: pink test texture via XPC)
        // This uses its own encoder and submits separately
        #[cfg(target_os = "macos")]
        {
            self.render_webview_overlays_webgpu(webgpu, &output.texture)?;
        }

        output.present();

        Ok(())
    }

    /// Render webview overlays using wgpu (macOS only)
    /// Uses the same approach as ts2: render pipeline with sRGB texture view
    #[cfg(target_os = "macos")]
    fn render_webview_overlays_webgpu(
        &self,
        webgpu: &crate::termwindow::webgpu::WebGpuState,
        output_texture: &wgpu::Texture,
    ) -> anyhow::Result<()> {
        log::info!(
            "[RENDER-LOOP] render_webview_overlays_webgpu called at {:?}",
            std::time::Instant::now()
        );

        use crate::termwindow::webview_socket::get_server;
        use cef::osr_texture_import::iosurface::IOSurfaceImporter;
        use cef::osr_texture_import::TextureImporter;
        use cef::sys::cef_color_type_t;

        // Get webview registry (which panes have webviews)
        let server = match get_server() {
            Some(s) => s,
            None => return Ok(()),
        };

        let state = server.state();
        let webview_panes = state.read().unwrap();

        if webview_panes.overlays.is_empty() {
            return Ok(());
        }

        // Get XPC manager (single source of truth for texture data)
        let xpc_manager = match crate::termwindow::webview_xpc::get_xpc_manager() {
            Some(m) => m,
            None => return Ok(()),
        };

        // Get positioned panes for viewport calculation
        let positioned_panes = self.get_panes_to_render();

        // Get active tab_id to filter overlays (only render overlays from active tab)
        let active_tab_id = match mux::Mux::try_get() {
            Some(mux) => match mux.get_active_tab_for_window(self.mux_window_id) {
                Some(tab) => tab.tab_id(),
                None => {
                    log::debug!("[Render] No active tab, skipping webview overlays");
                    return Ok(());
                }
            },
            None => return Ok(()),
        };

        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // For each webview pane, get CURRENT texture from XpcManager
        for (pane_id, overlay) in webview_panes.overlays.iter() {
            // Skip overlays from other tabs (fixes tab leak bug)
            if overlay.tab_id != active_tab_id {
                log::debug!(
                    "[Render] Skipping overlay for pane {} (tab {} != active {})",
                    pane_id, overlay.tab_id, active_tab_id
                );
                continue;
            }
            // Register invalidate callback if not already registered.
            // This allows XPC to trigger a window redraw when new textures arrive.
            if !xpc_manager.has_invalidate_callback(*pane_id) {
                let window = self.window.clone();
                let pane_id_for_log = *pane_id;
                let callback = std::sync::Arc::new(move || {
                    if let Some(ref w) = window {
                        log::info!(
                            "[XPC] Invalidate callback fired for pane {}",
                            pane_id_for_log
                        );
                        w.invalidate();
                    }
                });
                xpc_manager.register_invalidate_callback(*pane_id, callback);
            }

            // Get texture from XpcManager (may have been updated by resize)
            let surface = match xpc_manager.get_received_surface(*pane_id) {
                Some(s) => s,
                None => {
                    log::warn!("[Render] Webview pane {} has no surface yet", pane_id);
                    continue;
                }
            };

            if surface.mach_port == 0 {
                continue;
            }

            log::info!(
                "[Render] Importing IOSurface from mach_port={}, size={}x{}",
                surface.mach_port,
                surface.width,
                surface.height
            );

            // Note: viewport_w/h calculated later, so we log texture size here
            // and will log comparison after viewport calculation

            // Import IOSurface from Mach port
            let importer = match IOSurfaceImporter::from_mach_port(
                surface.mach_port,
                cef_color_type_t::CEF_COLOR_TYPE_BGRA_8888,
                surface.width,
                surface.height,
            ) {
                Some(imp) => imp,
                None => {
                    log::warn!(
                        "[Render] Failed to import IOSurface from mach_port={}",
                        surface.mach_port
                    );
                    continue;
                }
            };

            // Import to wgpu texture
            let texture = match importer.import_to_wgpu(&webgpu.device) {
                Ok(tex) => tex,
                Err(e) => {
                    log::warn!("[Render] Failed to import IOSurface to wgpu: {}", e);
                    continue;
                }
            };

            log::info!(
                "[Render] Successfully imported IOSurface texture, rendering with pipeline..."
            );

            // Create sRGB texture view - tells GPU the data is already sRGB-encoded
            // This prevents double gamma correction when rendering to WezTerm's sRGB surface
            let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("Webview Texture View"),
                format: Some(wgpu::TextureFormat::Bgra8UnormSrgb),
                ..Default::default()
            });

            // Create sampler
            let sampler = webgpu.device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            });

            // Create bind group
            let bind_group = webgpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Webview Texture Bind Group"),
                layout: &webgpu.webview_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });

            // Create encoder and render pass
            let mut encoder =
                webgpu
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Webview Overlay Encoder"),
                    });

            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Webview Overlay Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &output_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load, // Preserve existing terminal content
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });

                render_pass.set_pipeline(&webgpu.webview_render_pipeline);
                render_pass.set_bind_group(0, &bind_group, &[]);

                // Find this pane's position in the current layout
                let positioned_pane = positioned_panes.iter().find(|p| p.pane.pane_id() == *pane_id);

                // Calculate pane pixel bounds using ts2's half-cell boundary logic.
                // Pane dividers occupy one full cell, but each adjacent pane claims half.
                // This ensures webviews fill panes precisely.
                let (viewport_x, viewport_y, viewport_w, viewport_h) = match positioned_pane {
                    Some(pos) => {
                        let cell_width = self.render_metrics.cell_size.width as f32;
                        let cell_height = self.render_metrics.cell_size.height as f32;

                        // Get tab bar height
                        let tab_bar_height = if self.show_tab_bar {
                            self.tab_bar_pixel_height().unwrap_or(0.)
                        } else {
                            0.0
                        };

                        // Get border dimensions
                        let border = self.get_os_border();
                        let border_left = border.left.get() as f32;
                        let border_top = border.top.get() as f32;

                        // Get padding from config
                        let h_context = DimensionContext {
                            dpi: self.dimensions.dpi as f32,
                            pixel_max: self.dimensions.pixel_width as f32,
                            pixel_cell: cell_width,
                        };
                        let v_context = DimensionContext {
                            dpi: self.dimensions.dpi as f32,
                            pixel_max: self.dimensions.pixel_height as f32,
                            pixel_cell: cell_height,
                        };
                        let padding_left = self.config.window_padding.left.evaluate_as_pixels(h_context);
                        let padding_top = self.config.window_padding.top.evaluate_as_pixels(v_context);

                        // Terminal grid dimensions
                        let terminal_cols = self.terminal_size.cols as usize;
                        let terminal_rows = self.terminal_size.rows as usize;

                        // Window pixel dimensions
                        let pixel_width = self.dimensions.pixel_width as f32;
                        let pixel_height = self.dimensions.pixel_height as f32;

                        // Top of content area (after tab bar, padding, border)
                        let top_pixel_y = tab_bar_height + padding_top + border_top;

                        // X coordinate: extend to window edge or half-cell into divider
                        let (x, width_delta) = if pos.left == 0 {
                            // Left edge pane: start at window border
                            (0.0, padding_left + border_left + (cell_width / 2.0))
                        } else {
                            // Interior pane: extend half-cell into left divider
                            let x = padding_left + border_left + (pos.left as f32 * cell_width) - (cell_width / 2.0);
                            (x, cell_width) // half-cell on each side
                        };

                        // Y coordinate: extend to content top or half-cell into divider
                        let (y, height_delta) = if pos.top == 0 {
                            // Top edge pane: start at content area top
                            (top_pixel_y - padding_top, padding_top + (cell_height / 2.0))
                        } else {
                            // Interior pane: extend half-cell into top divider
                            let y = top_pixel_y + (pos.top as f32 * cell_height) - (cell_height / 2.0);
                            (y, cell_height)
                        };

                        // Width: extend to window edge or use grid width with delta
                        let w = if pos.left + pos.width >= terminal_cols {
                            // Rightmost pane: extend to window edge
                            pixel_width - x
                        } else {
                            // Interior: use grid width plus delta
                            (pos.width as f32 * cell_width) + width_delta
                        };

                        // Height: extend to window edge or use grid height with delta
                        let h = if pos.top + pos.height >= terminal_rows {
                            // Bottom pane: extend to window edge
                            pixel_height - y
                        } else {
                            // Interior: use grid height plus delta
                            (pos.height as f32 * cell_height) + height_delta
                        };

                        log::info!(
                            "[BOUNDS] pane={} pos=({},{}) size={}x{} -> viewport=({:.0},{:.0}) {:.0}x{:.0}",
                            pane_id, pos.left, pos.top, pos.width, pos.height,
                            x, y, w, h
                        );

                        (x, y, w, h)
                    }
                    None => {
                        // Pane not found in current layout - fall back to full window
                        log::warn!(
                            "[Render] Pane {} not found in layout, using full window",
                            pane_id
                        );
                        (
                            0.0,
                            0.0,
                            self.dimensions.pixel_width as f32,
                            self.dimensions.pixel_height as f32,
                        )
                    }
                };

                render_pass.set_viewport(viewport_x, viewport_y, viewport_w, viewport_h, 0.0, 1.0);

                // Debounce resize commands (ts2 pattern)
                {
                    use std::time::{Duration, Instant, SystemTime};
                    const SETTLE_DELAY: Duration = Duration::from_millis(30);

                    let scale = self.dimensions.dpi as f32 / 72.0;
                    let scale = if scale <= 0.0 { 2.0 } else { scale };

                    // Log viewport dimensions
                    let logical_w = (viewport_w / scale) as u32;
                    let logical_h = (viewport_h / scale) as u32;
                    log::info!(
                        "[VIEWPORT-SIZE] pane={} viewport={}x{} logical={}x{} scale={:.2}",
                        pane_id, viewport_w as u32, viewport_h as u32, logical_w, logical_h, scale
                    );

                    // Check for size mismatch between texture and viewport
                    // Texture size from IOSurface is already in physical pixels
                    if surface.width != viewport_w as u32 || surface.height != viewport_h as u32 {
                        log::warn!(
                            "[SIZE-MISMATCH] pane={} texture={}x{} viewport={}x{} diff=({}, {})",
                            pane_id,
                            surface.width, surface.height,
                            viewport_w as u32, viewport_h as u32,
                            surface.width as i32 - viewport_w as i32,
                            surface.height as i32 - viewport_h as i32
                        );
                    }

                    // Detect when borders would be visible (texture smaller than viewport)
                    if surface.width < viewport_w as u32 || surface.height < viewport_h as u32 {
                        log::warn!(
                            "[BORDER-VISIBLE] pane={} texture={}x{} < viewport={}x{} gap=({}, {})",
                            pane_id,
                            surface.width, surface.height,
                            viewport_w as u32, viewport_h as u32,
                            viewport_w as i32 - surface.width as i32,
                            viewport_h as i32 - surface.height as i32
                        );
                    }

                    // Use physical pixels for debounce tracking (avoids truncation issues)
                    let physical_w = viewport_w as u32;
                    let physical_h = viewport_h as u32;
                    let target_size = (physical_w, physical_h);

                    let mut resize_state = self.webview_resize_state.borrow_mut();
                    let state = resize_state.entry(*pane_id).or_insert(crate::termwindow::WebviewResizeState {
                        pending_size: None,
                        pending_since: None,
                        last_sent_size: None,
                    });

                    // Fast path: size unchanged from last sent
                    if state.last_sent_size == Some(target_size) {
                        log::info!(
                            "[DEBOUNCE] pane={} FAST_PATH size={}x{} (already sent)",
                            pane_id, physical_w, physical_h
                        );
                        state.pending_size = None;
                        state.pending_since = None;
                        drop(resize_state);
                    } else {
                        // Check if target size changed
                        if state.pending_size != Some(target_size) {
                            state.pending_size = Some(target_size);
                            state.pending_since = Some(Instant::now());
                            log::info!(
                                "[DEBOUNCE] pane={} TARGET_CHANGED to {}x{} (timer reset)",
                                pane_id, physical_w, physical_h
                            );
                        }

                        // Settle-and-send logic
                        if let Some(since) = state.pending_since {
                            let elapsed = since.elapsed();
                            if elapsed >= SETTLE_DELAY {
                                log::info!(
                                    "[RESIZE-SEND] pane={} physical={}x{} scale={:.2} timestamp={:?}",
                                    pane_id, physical_w, physical_h, scale,
                                    SystemTime::now()
                                );
                                state.last_sent_size = Some(target_size);
                                state.pending_size = None;
                                state.pending_since = None;
                                drop(resize_state);

                                if let Some(xpc_manager) = crate::termwindow::webview_xpc::get_xpc_manager() {
                                    xpc_manager.send_resize_physical(*pane_id, physical_w, physical_h, scale);
                                }
                            } else {
                                let remaining = SETTLE_DELAY.saturating_sub(elapsed);
                                log::info!(
                                    "[DEBOUNCE] pane={} WAITING elapsed={:?} remaining={:?}",
                                    pane_id, elapsed, remaining
                                );
                                drop(resize_state);
                                log::info!("[DEBOUNCE] pane={} calling window.invalidate()", pane_id);
                                if let Some(ref w) = self.window {
                                    w.invalidate();
                                    log::info!("[DEBOUNCE] pane={} invalidate() completed", pane_id);
                                } else {
                                    log::warn!("[DEBOUNCE] pane={} self.window is None!", pane_id);
                                }
                            }
                        }
                    }
                }

                // Draw a single triangle that covers the viewport
                render_pass.draw(0..3, 0..1);
            }

            webgpu.queue.submit(std::iter::once(encoder.finish()));

            log::info!(
                "[Render] Rendered {}x{} webview texture to screen",
                surface.width,
                surface.height
            );
        }

        Ok(())
    }

    fn call_draw_glium(&mut self, frame: &mut glium::Frame) -> anyhow::Result<()> {
        use window::glium::texture::SrgbTexture2d;

        let gl_state = self.render_state.as_ref().unwrap();
        let tex = gl_state.glyph_cache.borrow().atlas.texture();
        let tex = tex.downcast_ref::<SrgbTexture2d>().unwrap();

        frame.clear_color(0., 0., 0., 0.);

        let projection = euclid::Transform3D::<f32, f32, f32>::ortho(
            -(self.dimensions.pixel_width as f32) / 2.0,
            self.dimensions.pixel_width as f32 / 2.0,
            self.dimensions.pixel_height as f32 / 2.0,
            -(self.dimensions.pixel_height as f32) / 2.0,
            -1.0,
            1.0,
        )
        .to_arrays_transposed();

        let use_subpixel = match self
            .config
            .freetype_render_target
            .unwrap_or(self.config.freetype_load_target)
        {
            FreeTypeLoadTarget::HorizontalLcd | FreeTypeLoadTarget::VerticalLcd => true,
            _ => false,
        };

        let dual_source_blending = glium::DrawParameters {
            blend: glium::Blend {
                color: BlendingFunction::Addition {
                    source: LinearBlendingFactor::SourceOneColor,
                    destination: LinearBlendingFactor::OneMinusSourceOneColor,
                },
                alpha: BlendingFunction::Addition {
                    source: LinearBlendingFactor::SourceOneColor,
                    destination: LinearBlendingFactor::OneMinusSourceOneColor,
                },
                constant_value: (0.0, 0.0, 0.0, 0.0),
            },

            ..Default::default()
        };

        let alpha_blending = glium::DrawParameters {
            blend: glium::Blend {
                color: BlendingFunction::Addition {
                    source: LinearBlendingFactor::SourceAlpha,
                    destination: LinearBlendingFactor::OneMinusSourceAlpha,
                },
                alpha: BlendingFunction::Addition {
                    source: LinearBlendingFactor::One,
                    destination: LinearBlendingFactor::OneMinusSourceAlpha,
                },
                constant_value: (0.0, 0.0, 0.0, 0.0),
            },
            ..Default::default()
        };

        // Clamp and use the nearest texel rather than interpolate.
        // This prevents things like the box cursor outlines from
        // being randomly doubled in width or height
        let atlas_nearest_sampler = Sampler::new(&*tex)
            .wrap_function(SamplerWrapFunction::Clamp)
            .magnify_filter(MagnifySamplerFilter::Nearest)
            .minify_filter(MinifySamplerFilter::Nearest);

        let atlas_linear_sampler = Sampler::new(&*tex)
            .wrap_function(SamplerWrapFunction::Clamp)
            .magnify_filter(MagnifySamplerFilter::Linear)
            .minify_filter(MinifySamplerFilter::Linear);

        let foreground_text_hsb = self.config.foreground_text_hsb;
        let foreground_text_hsb = (
            foreground_text_hsb.hue,
            foreground_text_hsb.saturation,
            foreground_text_hsb.brightness,
        );

        let milliseconds = self.created.elapsed().as_millis() as u32;

        let cursor_blink: ColorEaseUniform = (*self.cursor_blink_state.borrow()).into();
        let blink: ColorEaseUniform = (*self.blink_state.borrow()).into();
        let rapid_blink: ColorEaseUniform = (*self.rapid_blink_state.borrow()).into();

        for layer in gl_state.layers.borrow().iter() {
            for idx in 0..3 {
                let vb = &layer.vb.borrow()[idx];
                let (vertex_count, index_count) = vb.vertex_index_count();
                if vertex_count > 0 {
                    let vertices = vb.current_vb_mut();
                    let subpixel_aa = use_subpixel && idx == 1;

                    let mut uniforms = UniformBuilder::default();

                    uniforms.add("projection", &projection);
                    uniforms.add("atlas_nearest_sampler", &atlas_nearest_sampler);
                    uniforms.add("atlas_linear_sampler", &atlas_linear_sampler);
                    uniforms.add("foreground_text_hsb", &foreground_text_hsb);
                    uniforms.add("subpixel_aa", &subpixel_aa);
                    uniforms.add("milliseconds", &milliseconds);
                    uniforms.add_struct("cursor_blink", &cursor_blink);
                    uniforms.add_struct("blink", &blink);
                    uniforms.add_struct("rapid_blink", &rapid_blink);

                    frame.draw(
                        vertices.glium().slice(0..vertex_count).unwrap(),
                        vb.indices.glium().slice(0..index_count).unwrap(),
                        gl_state.glyph_prog.as_ref().unwrap(),
                        &uniforms,
                        if subpixel_aa {
                            &dual_source_blending
                        } else {
                            &alpha_blending
                        },
                    )?;
                }

                vb.next_index();
            }
        }

        Ok(())
    }
}
