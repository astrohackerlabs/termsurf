use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLCommandQueue, MTLDevice};

use crate::font::atlas::Atlas;
use crate::renderer::cell::Contents;
use crate::renderer::image::{BackgroundImageState, DrawPlacements, ImageState};
use crate::renderer::metal::api::{
    MetalClearColor, MetalPixelFormat, MetalResourceOptions, MetalStorageMode,
};
use crate::renderer::metal::buffer::{MetalBuffer, MetalBufferOptions};
use crate::renderer::metal::frame::{FrameState, FrameStateError};
use crate::renderer::metal::iosurface_layer::{MetalIOSurfaceLayer, MetalSurfacePresentationMode};
use crate::renderer::metal::render_pass::{
    MetalCommandFrame, MetalCommandFrameError, MetalImageDrawPass, MetalRenderPassAttachment,
    MetalRenderPassError,
};
use crate::renderer::metal::sampler::{
    MetalSampler, MetalSamplerDescriptorOptions, MetalSamplerError, MetalSamplerOptions,
};
use crate::renderer::metal::shaders::{
    MetalStandardPipelines, MetalStandardPipelinesError, MetalUniforms,
};
use crate::renderer::metal::target::{MetalTarget, MetalTargetError, MetalTargetOptions};
use crate::renderer::metal::texture::{MetalImageUploadBackend, MetalTexture};

pub(crate) struct MetalFrameCompositor {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    pipelines: MetalStandardPipelines,
    frame: FrameState,
    image_sampler: MetalSampler,
    layer: MetalIOSurfaceLayer,
    target: Option<MetalTarget>,
    pixel_format: MetalPixelFormat,
    storage_mode: MetalStorageMode,
    resource_options: MetalResourceOptions,
}

#[derive(Clone)]
pub(crate) struct MetalFrameCompositorOptions<'a> {
    pub(crate) device: Retained<ProtocolObject<dyn MTLDevice>>,
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) pixel_format: MetalPixelFormat,
    pub(crate) storage_mode: MetalStorageMode,
    pub(crate) resource_options: MetalResourceOptions,
    pub(crate) grayscale_atlas: &'a Atlas,
    pub(crate) color_atlas: &'a Atlas,
}

#[derive(Clone, Copy)]
pub(crate) struct MetalFrameInput<'a> {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) contents_scale: f64,
    pub(crate) uniforms: &'a MetalUniforms,
    pub(crate) contents: &'a Contents,
    pub(crate) grayscale_atlas: &'a Atlas,
    pub(crate) color_atlas: &'a Atlas,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MetalFramePresentation {
    pub(crate) fg_count: usize,
    pub(crate) mode: MetalSurfacePresentationMode,
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) target_reallocated: bool,
}

#[derive(Debug)]
pub(crate) enum MetalFrameCompositorError {
    InvalidContentsScale,
    CommandQueueCreationFailed,
    Pipelines(MetalStandardPipelinesError),
    Frame(FrameStateError),
    Buffer(crate::renderer::metal::buffer::MetalBufferError),
    ImageSampler(MetalSamplerError),
    Target(MetalTargetError),
    CommandFrame(MetalCommandFrameError),
    RenderPass(MetalRenderPassError),
}

impl From<FrameStateError> for MetalFrameCompositorError {
    fn from(error: FrameStateError) -> Self {
        Self::Frame(error)
    }
}

impl From<crate::renderer::metal::buffer::MetalBufferError> for MetalFrameCompositorError {
    fn from(error: crate::renderer::metal::buffer::MetalBufferError) -> Self {
        Self::Buffer(error)
    }
}

impl From<MetalTargetError> for MetalFrameCompositorError {
    fn from(error: MetalTargetError) -> Self {
        Self::Target(error)
    }
}

impl From<MetalCommandFrameError> for MetalFrameCompositorError {
    fn from(error: MetalCommandFrameError) -> Self {
        Self::CommandFrame(error)
    }
}

impl From<MetalRenderPassError> for MetalFrameCompositorError {
    fn from(error: MetalRenderPassError) -> Self {
        Self::RenderPass(error)
    }
}

impl MetalFrameCompositor {
    pub(crate) fn new(
        options: MetalFrameCompositorOptions<'_>,
    ) -> Result<Self, MetalFrameCompositorError> {
        let queue = options
            .device
            .newCommandQueue()
            .ok_or(MetalFrameCompositorError::CommandQueueCreationFailed)?;
        let pipelines = MetalStandardPipelines::new(&options.device, options.pixel_format)
            .map_err(MetalFrameCompositorError::Pipelines)?;
        let frame = FrameState::new(
            MetalBufferOptions {
                device: &options.device,
                resource_options: options.resource_options,
            },
            options.grayscale_atlas,
            options.color_atlas,
        )?;
        let image_sampler = MetalSampler::new(MetalSamplerOptions {
            device: &options.device,
            descriptor: MetalSamplerDescriptorOptions::default(),
        })
        .map_err(MetalFrameCompositorError::ImageSampler)?;
        let mut compositor = Self {
            device: options.device,
            queue,
            pipelines,
            frame,
            image_sampler,
            layer: MetalIOSurfaceLayer::new(),
            target: None,
            pixel_format: options.pixel_format,
            storage_mode: options.storage_mode,
            resource_options: options.resource_options,
        };
        compositor.ensure_target(options.width, options.height)?;
        Ok(compositor)
    }

    /// The IOSurface-backed `CALayer` this compositor presents into (Issue 802 / Exp 15) —
    /// for attaching to the app's NSView.
    pub(crate) fn layer(&self) -> &MetalIOSurfaceLayer {
        &self.layer
    }

    pub(crate) fn draw_frame(
        &mut self,
        input: MetalFrameInput<'_>,
    ) -> Result<MetalFramePresentation, MetalFrameCompositorError> {
        self.draw_frame_with_presenter(input, None, |layer, target| {
            layer.set_surface(target.surface())
        })
    }

    pub(crate) fn draw_frame_with_images(
        &mut self,
        input: MetalFrameInput<'_>,
        images: &mut ImageState<MetalTexture>,
        background: &mut BackgroundImageState<MetalTexture>,
    ) -> Result<MetalFramePresentation, MetalFrameCompositorError> {
        self.draw_frame_with_presenter(input, Some((images, background)), |layer, target| {
            layer.set_surface(target.surface())
        })
    }

    fn draw_frame_with_presenter(
        &mut self,
        input: MetalFrameInput<'_>,
        mut images: Option<(
            &mut ImageState<MetalTexture>,
            &mut BackgroundImageState<MetalTexture>,
        )>,
        presenter: impl FnOnce(&MetalIOSurfaceLayer, &MetalTarget) -> MetalSurfacePresentationMode,
    ) -> Result<MetalFramePresentation, MetalFrameCompositorError> {
        if !input.contents_scale.is_finite() || input.contents_scale <= 0.0 {
            return Err(MetalFrameCompositorError::InvalidContentsScale);
        }

        let target_reallocated = self.ensure_target(input.width, input.height)?;
        let device = self.device.clone();
        let fg_count = self.frame.sync(
            MetalBufferOptions {
                device: &device,
                resource_options: self.resource_options,
            },
            input.uniforms,
            input.contents,
            input.grayscale_atlas,
            input.color_atlas,
        )?;

        let target = self
            .target
            .as_ref()
            .expect("target should exist after ensure_target");
        if let Some((images, background)) = images.as_mut() {
            let mut upload_backend =
                MetalImageUploadBackend::new(&device, self.storage_mode, false);
            images.upload(&mut upload_backend);
            background.upload(&mut upload_backend);
        }
        let command_frame = MetalCommandFrame::begin(&self.queue)?;
        let pass = command_frame.render_pass(&[MetalRenderPassAttachment {
            texture: target.texture(),
            clear_color: Some(MetalClearColor {
                red: 0.0,
                green: 0.0,
                blue: 0.0,
                alpha: 0.0,
            }),
        }])?;
        if let Some((images, background)) = images {
            let background_vertex = if let Some(texture) = background.ready_texture() {
                let vertex = MetalBuffer::init_fill(
                    MetalBufferOptions {
                        device: &device,
                        resource_options: self.resource_options,
                    },
                    &[background.vertex()],
                )?;
                pass.draw_background_image(
                    &self.pipelines,
                    self.frame.uniforms_buffer(),
                    &vertex,
                    texture,
                );
                Some(vertex)
            } else {
                pass.draw_background_color(
                    &self.pipelines,
                    self.frame.uniforms_buffer(),
                    self.frame.cells(),
                );
                None
            };
            let mut image_pass = MetalImageDrawPass::new(
                &pass,
                &self.pipelines,
                self.frame.uniforms_buffer(),
                &self.image_sampler,
                MetalBufferOptions {
                    device: &device,
                    resource_options: self.resource_options,
                },
            );
            images.draw(DrawPlacements::KittyBelowBackground, &mut image_pass);
            pass.draw_cell_backgrounds(
                &self.pipelines,
                self.frame.uniforms_buffer(),
                self.frame.cells(),
            );
            images.draw(DrawPlacements::KittyBelowText, &mut image_pass);
            pass.draw_cell_text(
                &self.pipelines,
                self.frame.uniforms_buffer(),
                self.frame.cells(),
                self.frame.grayscale_texture(),
                self.frame.color_texture(),
                fg_count,
            );
            images.draw(DrawPlacements::KittyAboveText, &mut image_pass);
            drop(image_pass);
            drop(background_vertex);
        } else {
            pass.draw_frame(&self.pipelines, &self.frame, fg_count);
        }
        pass.complete();
        command_frame.commit_and_wait()?;

        self.layer.set_bounds_pixels(
            input.width as f64 / input.contents_scale,
            input.height as f64 / input.contents_scale,
            input.contents_scale,
        );
        let mode = presenter(&self.layer, target);

        Ok(MetalFramePresentation {
            fg_count,
            mode,
            width: input.width,
            height: input.height,
            target_reallocated,
        })
    }

    fn ensure_target(
        &mut self,
        width: usize,
        height: usize,
    ) -> Result<bool, MetalFrameCompositorError> {
        let needs_target = self.target.as_ref().map_or(true, |target| {
            target.width() != width || target.height() != height
        });
        if !needs_target {
            return Ok(false);
        }

        self.target = Some(MetalTarget::new(MetalTargetOptions {
            device: &self.device,
            width,
            height,
            pixel_format: self.pixel_format,
            storage_mode: self.storage_mode,
        })?);
        Ok(true)
    }
}

#[cfg(test)]
impl MetalFrameCompositor {
    fn draw_frame_immediate(
        &mut self,
        input: MetalFrameInput<'_>,
    ) -> Result<MetalFramePresentation, MetalFrameCompositorError> {
        self.draw_frame_with_presenter(input, None, |layer, target| {
            assert!(layer.set_surface_if_size_matches(target.surface()));
            MetalSurfacePresentationMode::Immediate
        })
    }

    fn draw_frame_with_images_immediate(
        &mut self,
        input: MetalFrameInput<'_>,
        images: &mut ImageState<MetalTexture>,
        background: &mut BackgroundImageState<MetalTexture>,
    ) -> Result<MetalFramePresentation, MetalFrameCompositorError> {
        self.draw_frame_with_presenter(input, Some((images, background)), |layer, target| {
            assert!(layer.set_surface_if_size_matches(target.surface()));
            MetalSurfacePresentationMode::Immediate
        })
    }

    pub(crate) fn target_bytes(&self) -> Vec<u8> {
        self.target
            .as_ref()
            .expect("compositor should have a target")
            .read_bytes()
    }

    fn layer_expected_pixel_size(&self) -> (usize, usize) {
        self.layer.expected_pixel_size()
    }
}

#[cfg(test)]
mod tests {
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_metal::{MTLCreateSystemDefaultDevice, MTLDevice};

    use super::*;
    use crate::config::Config;
    use crate::font::atlas::{Atlas, Format};
    use crate::renderer::cell::{Contents, Key};
    use crate::renderer::image::{
        BackgroundImageState, ImageId, PendingImage, PixelFormat, Placement, RendererImage,
    };
    use crate::renderer::metal::api::MetalStorageMode;
    use crate::renderer::shader::{CellBg, CellTextAtlas, CellTextFlags, CellTextVertex};
    use crate::renderer::size::GridSize;

    fn metal_device() -> Option<Retained<ProtocolObject<dyn MTLDevice>>> {
        MTLCreateSystemDefaultDevice()
    }

    fn compositor(
        device: Retained<ProtocolObject<dyn MTLDevice>>,
        width: usize,
        height: usize,
        grayscale: &Atlas,
        color: &Atlas,
    ) -> MetalFrameCompositor {
        MetalFrameCompositor::new(MetalFrameCompositorOptions {
            device,
            width,
            height,
            pixel_format: MetalPixelFormat::Bgra8Unorm,
            storage_mode: MetalStorageMode::Shared,
            resource_options: MetalResourceOptions::image(MetalStorageMode::Shared),
            grayscale_atlas: grayscale,
            color_atlas: color,
        })
        .expect("compositor should be created")
    }

    fn frame_input<'a>(
        width: usize,
        height: usize,
        contents_scale: f64,
        uniforms: &'a MetalUniforms,
        contents: &'a Contents,
        grayscale: &'a Atlas,
        color: &'a Atlas,
    ) -> MetalFrameInput<'a> {
        MetalFrameInput {
            width,
            height,
            contents_scale,
            uniforms,
            contents,
            grayscale_atlas: grayscale,
            color_atlas: color,
        }
    }

    fn assert_pixels(bytes: &[u8], expected: [u8; 4]) {
        for pixel in bytes.chunks_exact(4) {
            assert_eq!(pixel, expected);
        }
    }

    fn assert_pixel_grid(bytes: &[u8], expected: &[[u8; 4]]) {
        let pixels = bytes
            .chunks_exact(4)
            .map(|chunk| [chunk[0], chunk[1], chunk[2], chunk[3]])
            .collect::<Vec<_>>();
        assert_eq!(pixels, expected);
    }

    fn pending_rgba(rgba: [u8; 4]) -> PendingImage {
        PendingImage {
            width: 1,
            height: 1,
            pixel_format: PixelFormat::Rgba,
            data: rgba.to_vec(),
        }
    }

    fn temp_image_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "roastty-metal-compositor-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).expect("create temp image dir");
        dir
    }

    fn write_png(path: &std::path::Path, rgba: [u8; 4]) {
        image::save_buffer_with_format(
            path,
            &rgba,
            1,
            1,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .expect("write png");
    }

    fn background_state(path: &std::path::Path) -> BackgroundImageState<MetalTexture> {
        let mut config = Config::default();
        config
            .set("background-image", Some(&path.to_string_lossy()))
            .expect("set background image");
        let mut background = BackgroundImageState::default();
        background.update_from_config(&config);
        background
    }

    fn placement(image_id: u32, x: i32, z: i32) -> Placement {
        Placement {
            image_id: ImageId::Kitty(image_id),
            x,
            y: 0,
            z,
            width: 1,
            height: 1,
            cell_offset_x: 0,
            cell_offset_y: 0,
            source_x: 0,
            source_y: 0,
            source_width: 1,
            source_height: 1,
        }
    }

    fn cell_text_uniforms(
        screen_size: [u16; 2],
        grid_size: [u16; 2],
        cell_size: [f32; 2],
        bg_color: [u8; 4],
    ) -> MetalUniforms {
        let mut uniforms =
            MetalUniforms::test_with_grid(screen_size, grid_size, cell_size, [0.0; 4], 0, bg_color);
        uniforms.projection_matrix = crate::renderer::metal::shaders::ortho2d(
            0.0,
            screen_size[0] as f32,
            screen_size[1] as f32,
            0.0,
        );
        uniforms.cursor_pos = [u16::MAX, u16::MAX];
        uniforms
    }

    fn cell_text_vertex(
        glyph_pos: [u32; 2],
        glyph_size: [u32; 2],
        bearings: [i16; 2],
        grid_pos: [u16; 2],
        color: [u8; 4],
    ) -> CellTextVertex {
        CellTextVertex {
            glyph_pos,
            glyph_size,
            bearings,
            grid_pos,
            color,
            atlas: CellTextAtlas::Grayscale,
            flags: CellTextFlags::new(false, false),
            _padding: [0, 0],
        }
    }

    #[test]
    fn compositor_draws_background_frame_and_reuses_target() {
        let Some(device) = metal_device() else {
            return;
        };
        let grayscale = Atlas::new(8, Format::Grayscale);
        let color = Atlas::new(8, Format::Bgra);
        let mut compositor = compositor(device, 2, 2, &grayscale, &color);

        let mut contents = Contents::default();
        contents.resize(GridSize {
            columns: 1,
            rows: 1,
        });
        let uniforms = cell_text_uniforms([2, 2], [1, 1], [2.0, 2.0], [32, 64, 128, 255]);

        let presentation = compositor
            .draw_frame_immediate(frame_input(
                2, 2, 2.0, &uniforms, &contents, &grayscale, &color,
            ))
            .expect("frame should draw");
        assert_eq!(presentation.fg_count, 0);
        assert_eq!(presentation.mode, MetalSurfacePresentationMode::Immediate);
        assert_eq!(presentation.width, 2);
        assert_eq!(presentation.height, 2);
        assert!(!presentation.target_reallocated);
        assert_eq!(compositor.layer_expected_pixel_size(), (2, 2));
        assert_pixels(&compositor.target_bytes(), [128, 64, 32, 255]);

        let presentation = compositor
            .draw_frame_immediate(frame_input(
                2, 2, 2.0, &uniforms, &contents, &grayscale, &color,
            ))
            .expect("second frame should draw");
        assert!(!presentation.target_reallocated);
    }

    #[test]
    fn compositor_resizes_target_and_draws_cell_background() {
        let Some(device) = metal_device() else {
            return;
        };
        let grayscale = Atlas::new(8, Format::Grayscale);
        let color = Atlas::new(8, Format::Bgra);
        let mut compositor = compositor(device, 2, 2, &grayscale, &color);

        let mut contents = Contents::default();
        contents.resize(GridSize {
            columns: 1,
            rows: 1,
        });
        *contents.bg_cell_mut(0, 0) = CellBg([0, 255, 0, 255]);
        let uniforms = cell_text_uniforms([4, 4], [1, 1], [4.0, 4.0], [0, 0, 0, 0]);

        let presentation = compositor
            .draw_frame_immediate(frame_input(
                4, 4, 2.0, &uniforms, &contents, &grayscale, &color,
            ))
            .expect("resized frame should draw");

        assert!(presentation.target_reallocated);
        assert_eq!(presentation.width, 4);
        assert_eq!(presentation.height, 4);
        assert_eq!(compositor.layer_expected_pixel_size(), (4, 4));
        assert_pixels(&compositor.target_bytes(), [0, 255, 0, 255]);
    }

    #[test]
    fn compositor_draws_foreground_glyph() {
        let Some(device) = metal_device() else {
            return;
        };
        let mut grayscale = Atlas::new(8, Format::Grayscale);
        let region = grayscale.reserve(2, 2).expect("reserve glyph region");
        grayscale.set(region, &[255, 255, 255, 255]);
        let color = Atlas::new(8, Format::Bgra);
        let mut compositor = compositor(device, 2, 2, &grayscale, &color);

        let mut contents = Contents::default();
        contents.resize(GridSize {
            columns: 1,
            rows: 1,
        });
        *contents.bg_cell_mut(0, 0) = CellBg([0, 0, 0, 0]);
        contents.add(
            Key::Text,
            cell_text_vertex(
                [region.x, region.y],
                [2, 2],
                [0, 2],
                [0, 0],
                [255, 0, 0, 255],
            ),
        );
        let uniforms = cell_text_uniforms([2, 2], [1, 1], [2.0, 2.0], [0, 0, 0, 0]);

        let presentation = compositor
            .draw_frame_immediate(frame_input(
                2, 2, 1.0, &uniforms, &contents, &grayscale, &color,
            ))
            .expect("glyph frame should draw");

        assert_eq!(presentation.fg_count, 1);
        assert_eq!(compositor.layer_expected_pixel_size(), (2, 2));
        assert_pixels(&compositor.target_bytes(), [0, 0, 255, 255]);
    }

    #[test]
    fn compositor_live_kitty_image_buckets_interleave_with_cells_and_text() {
        let Some(device) = metal_device() else {
            return;
        };
        let mut grayscale = Atlas::new(8, Format::Grayscale);
        let region = grayscale.reserve(1, 1).expect("reserve glyph region");
        grayscale.set(region, &[255]);
        let color = Atlas::new(8, Format::Bgra);
        let mut compositor = compositor(device, 4, 1, &grayscale, &color);

        let mut contents = Contents::default();
        contents.resize(GridSize {
            columns: 4,
            rows: 1,
        });
        for x in 0..4 {
            *contents.bg_cell_mut(0, x) = CellBg([5, 6, 7, 255]);
        }
        for x in 1..=2 {
            contents.add(
                Key::Text,
                cell_text_vertex(
                    [region.x, region.y],
                    [1, 1],
                    [0, 1],
                    [x, 0],
                    [255, 0, 0, 255],
                ),
            );
        }

        let mut images = ImageState::<MetalTexture>::default();
        images.images.insert(
            ImageId::Kitty(1),
            RendererImage::Pending(pending_rgba([255, 0, 0, 255])),
        );
        images.images.insert(
            ImageId::Kitty(2),
            RendererImage::Pending(pending_rgba([0, 255, 0, 255])),
        );
        images.images.insert(
            ImageId::Kitty(3),
            RendererImage::Pending(pending_rgba([255, 255, 0, 255])),
        );
        images.images.insert(
            ImageId::Kitty(4),
            RendererImage::Pending(pending_rgba([0, 255, 255, 255])),
        );
        images.kitty_placements = vec![
            placement(1, 0, i32::MIN / 2 - 1),
            placement(2, 1, -1),
            placement(4, 3, -1),
            placement(3, 2, 0),
        ];
        images.kitty_bg_end = 1;
        images.kitty_text_end = 3;

        let uniforms = cell_text_uniforms([4, 1], [4, 1], [1.0, 1.0], [0, 0, 0, 0]);
        let mut background = BackgroundImageState::<MetalTexture>::default();
        let presentation = compositor
            .draw_frame_with_images_immediate(
                frame_input(4, 1, 1.0, &uniforms, &contents, &grayscale, &color),
                &mut images,
                &mut background,
            )
            .expect("image frame should draw");

        assert_eq!(presentation.fg_count, 2);
        assert_pixel_grid(
            &compositor.target_bytes(),
            &[
                [7, 6, 5, 255],
                [0, 0, 255, 255],
                [0, 255, 255, 255],
                [255, 255, 0, 255],
            ],
        );
    }

    #[test]
    fn compositor_draws_background_image_instead_of_bg_color() {
        let Some(device) = metal_device() else {
            return;
        };
        let grayscale = Atlas::new(8, Format::Grayscale);
        let color = Atlas::new(8, Format::Bgra);
        let mut compositor = compositor(device, 1, 1, &grayscale, &color);
        let contents = Contents::default();
        let uniforms = cell_text_uniforms([1, 1], [1, 1], [1.0, 1.0], [10, 20, 30, 255]);
        let mut images = ImageState::<MetalTexture>::default();
        let dir = temp_image_dir("background-image");
        let path = dir.join("bg.png");
        write_png(&path, [255, 0, 0, 255]);
        let mut background = background_state(&path);

        compositor
            .draw_frame_with_images_immediate(
                frame_input(1, 1, 1.0, &uniforms, &contents, &grayscale, &color),
                &mut images,
                &mut background,
            )
            .expect("background image frame should draw");

        assert_eq!(compositor.target_bytes(), vec![0, 0, 255, 255]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn compositor_background_image_does_not_double_compose_bg_color() {
        let Some(device) = metal_device() else {
            return;
        };
        let grayscale = Atlas::new(8, Format::Grayscale);
        let color = Atlas::new(8, Format::Bgra);
        let mut compositor = compositor(device, 1, 1, &grayscale, &color);
        let contents = Contents::default();
        let uniforms = cell_text_uniforms([1, 1], [1, 1], [1.0, 1.0], [10, 20, 30, 128]);
        let mut images = ImageState::<MetalTexture>::default();
        let dir = temp_image_dir("background-image-alpha");
        let path = dir.join("bg.png");
        write_png(&path, [255, 0, 0, 255]);
        let mut background = background_state(&path);

        compositor
            .draw_frame_with_images_immediate(
                frame_input(1, 1, 1.0, &uniforms, &contents, &grayscale, &color),
                &mut images,
                &mut background,
            )
            .expect("background image alpha frame should draw");

        assert_eq!(compositor.target_bytes(), vec![0, 0, 128, 128]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
