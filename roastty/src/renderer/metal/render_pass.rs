use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLCommandBuffer, MTLCommandBufferStatus, MTLCommandEncoder, MTLCommandQueue,
    MTLRenderCommandEncoder, MTLRenderPassDescriptor,
};

use crate::renderer::metal::api::{
    MetalClearColor, MetalCommandBufferStatus, MetalLoadAction, MetalPrimitiveType,
    MetalStoreAction,
};
use crate::renderer::metal::pipeline::MetalPipeline;
use crate::renderer::metal::texture::MetalTexture;

pub(crate) struct MetalCommandFrame {
    command_buffer: Retained<ProtocolObject<dyn MTLCommandBuffer>>,
}

impl MetalCommandFrame {
    pub(crate) fn begin(
        queue: &ProtocolObject<dyn MTLCommandQueue>,
    ) -> Result<Self, MetalCommandFrameError> {
        let command_buffer = queue
            .commandBuffer()
            .ok_or(MetalCommandFrameError::CommandBufferCreationFailed)?;
        Ok(Self { command_buffer })
    }

    pub(crate) fn render_pass(
        &self,
        attachments: &[MetalRenderPassAttachment<'_>],
    ) -> Result<MetalRenderPass, MetalRenderPassError> {
        MetalRenderPass::begin(&self.command_buffer, attachments)
    }

    pub(crate) fn commit_and_wait(self) -> Result<(), MetalCommandFrameError> {
        self.command_buffer.commit();
        self.command_buffer.waitUntilCompleted();
        command_buffer_status_result(self.command_buffer.status())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MetalCommandFrameError {
    CommandBufferCreationFailed,
    CommandBufferFailed,
    CommandBufferNotCompleted(u64),
}

fn command_buffer_status_result(
    status: MTLCommandBufferStatus,
) -> Result<(), MetalCommandFrameError> {
    match MetalCommandBufferStatus::from_objc(status) {
        Some(MetalCommandBufferStatus::Completed) => Ok(()),
        Some(MetalCommandBufferStatus::Error) => Err(MetalCommandFrameError::CommandBufferFailed),
        None => Err(MetalCommandFrameError::CommandBufferNotCompleted(
            status.0 as u64,
        )),
    }
}

pub(crate) struct MetalRenderPass {
    encoder: Retained<ProtocolObject<dyn MTLRenderCommandEncoder>>,
}

impl MetalRenderPass {
    fn begin(
        command_buffer: &ProtocolObject<dyn MTLCommandBuffer>,
        attachments: &[MetalRenderPassAttachment<'_>],
    ) -> Result<Self, MetalRenderPassError> {
        let descriptor = MTLRenderPassDescriptor::renderPassDescriptor();
        let color_attachments = descriptor.colorAttachments();

        for (index, attachment) in attachments.iter().enumerate() {
            let color_attachment = unsafe { color_attachments.objectAtIndexedSubscript(index) };
            color_attachment.setLoadAction(if attachment.clear_color.is_some() {
                MetalLoadAction::Clear.to_objc()
            } else {
                MetalLoadAction::Load.to_objc()
            });
            color_attachment.setStoreAction(MetalStoreAction::Store.to_objc());
            color_attachment.setTexture(Some(attachment.texture.texture()));
            if let Some(clear_color) = attachment.clear_color {
                color_attachment.setClearColor(clear_color.to_objc());
            }
        }

        let encoder = command_buffer
            .renderCommandEncoderWithDescriptor(&descriptor)
            .ok_or(MetalRenderPassError::EncoderCreationFailed)?;

        Ok(Self { encoder })
    }

    pub(crate) fn step(&self, step: MetalRenderPassStep<'_>) {
        if step.draw.instance_count == 0 {
            return;
        }

        self.encoder.setRenderPipelineState(step.pipeline.state());
        bind_step_buffers(&self.encoder, step.buffers);
        bind_step_textures(&self.encoder, step.textures);
        if let Some(uniforms) = step.uniforms {
            unsafe {
                self.encoder
                    .setVertexBuffer_offset_atIndex(Some(uniforms), 0, 1);
                self.encoder
                    .setFragmentBuffer_offset_atIndex(Some(uniforms), 0, 1);
            }
        }
        unsafe {
            self.encoder
                .drawPrimitives_vertexStart_vertexCount_instanceCount(
                    step.draw.primitive_type.to_objc(),
                    0,
                    step.draw.vertex_count,
                    step.draw.instance_count,
                );
        }
    }

    pub(crate) fn complete(self) {
        self.encoder.endEncoding();
    }
}

fn bind_step_buffers(
    encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    buffers: &[Option<&ProtocolObject<dyn objc2_metal::MTLBuffer>>],
) {
    if let Some(buffer) = buffers.first().copied().flatten() {
        bind_step_buffer(encoder, buffer, 0);
    }

    for (offset, buffer) in buffers.iter().skip(1).enumerate() {
        if let Some(buffer) = buffer {
            bind_step_buffer(encoder, buffer, offset + 2);
        }
    }
}

fn bind_step_buffer(
    encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    buffer: &ProtocolObject<dyn objc2_metal::MTLBuffer>,
    index: usize,
) {
    unsafe {
        encoder.setVertexBuffer_offset_atIndex(Some(buffer), 0, index);
        encoder.setFragmentBuffer_offset_atIndex(Some(buffer), 0, index);
    }
}

fn bind_step_textures(
    encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    textures: &[Option<&MetalTexture>],
) {
    for (index, texture) in textures.iter().enumerate() {
        if let Some(texture) = texture {
            unsafe {
                encoder.setVertexTexture_atIndex(Some(texture.texture()), index);
                encoder.setFragmentTexture_atIndex(Some(texture.texture()), index);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MetalRenderPassError {
    EncoderCreationFailed,
}

pub(crate) struct MetalRenderPassAttachment<'a> {
    pub(crate) texture: &'a MetalTexture,
    pub(crate) clear_color: Option<MetalClearColor>,
}

pub(crate) struct MetalRenderPassStep<'a> {
    pub(crate) pipeline: &'a MetalPipeline,
    pub(crate) buffers: &'a [Option<&'a ProtocolObject<dyn objc2_metal::MTLBuffer>>],
    pub(crate) textures: &'a [Option<&'a MetalTexture>],
    pub(crate) uniforms: Option<&'a ProtocolObject<dyn objc2_metal::MTLBuffer>>,
    pub(crate) draw: MetalDraw,
}

pub(crate) struct MetalDraw {
    pub(crate) primitive_type: MetalPrimitiveType,
    pub(crate) vertex_count: usize,
    pub(crate) instance_count: usize,
}

#[cfg(test)]
mod tests {
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_metal::{MTLCommandBufferStatus, MTLCreateSystemDefaultDevice, MTLDevice};

    use super::*;
    use crate::renderer::metal::api::{MetalPixelFormat, MetalResourceOptions, MetalStorageMode};
    use crate::renderer::metal::buffer::{MetalBuffer, MetalBufferOptions};
    use crate::renderer::metal::shaders::{MetalStandardPipelines, MetalUniforms};
    use crate::renderer::metal::texture::{
        image_texture_options, render_target_texture_options, ImageTextureFormat,
    };
    use crate::renderer::shader::{
        BgImageFit, BgImageInfo, BgImagePosition, BgImageVertex, CellBg, CellTextAtlas,
        CellTextFlags, CellTextVertex, ImageVertex,
    };

    fn metal_device() -> Retained<ProtocolObject<dyn MTLDevice>> {
        MTLCreateSystemDefaultDevice().expect("Roastty requires a Metal device")
    }

    fn render_target(
        device: &ProtocolObject<dyn MTLDevice>,
        width: usize,
        height: usize,
    ) -> MetalTexture {
        MetalTexture::new(
            device,
            render_target_texture_options(MetalPixelFormat::Bgra8Unorm, MetalStorageMode::Shared),
            width,
            height,
            None,
        )
        .expect("render target texture should be created")
    }

    fn command_queue_is_created_from_default_device(
    ) -> Retained<ProtocolObject<dyn objc2_metal::MTLCommandQueue>> {
        let device = metal_device();
        device
            .newCommandQueue()
            .expect("command queue should be created")
    }

    fn assert_pixels(bytes: &[u8], expected: [u8; 4]) {
        for pixel in bytes.chunks_exact(4) {
            assert_eq!(pixel, expected);
        }
    }

    fn assert_pixel_grid(bytes: &[u8], width: usize, expected: &[[u8; 4]]) {
        let pixels = bytes
            .chunks_exact(4)
            .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
            .collect::<Vec<_>>();
        assert_eq!(pixels.len(), expected.len());

        for (index, (actual, expected)) in pixels.iter().zip(expected).enumerate() {
            assert_eq!(
                actual,
                expected,
                "pixel mismatch at x={}, y={}",
                index % width,
                index / width
            );
        }
    }

    fn uniform_buffer(
        device: &ProtocolObject<dyn MTLDevice>,
        uniforms: MetalUniforms,
    ) -> MetalBuffer<MetalUniforms> {
        MetalBuffer::init_fill(
            MetalBufferOptions {
                device,
                resource_options: MetalResourceOptions::image(MetalStorageMode::Shared),
            },
            &[uniforms],
        )
        .expect("uniform buffer should be created")
    }

    fn image_uniforms(width: u16, height: u16) -> MetalUniforms {
        screen_uniforms(width, height, [0, 0, 0, 0])
    }

    fn screen_uniforms(width: u16, height: u16, bg_color: [u8; 4]) -> MetalUniforms {
        let mut uniforms = MetalUniforms::test_with_grid(
            [width, height],
            [width, height],
            [1.0, 1.0],
            [0.0; 4],
            0,
            bg_color,
        );
        uniforms.projection_matrix = ortho2d(0.0, width as f32, height as f32, 0.0);
        uniforms
    }

    fn cell_text_uniforms(
        screen_size: [u16; 2],
        grid_size: [u16; 2],
        cell_size: [f32; 2],
        bg_color: [u8; 4],
    ) -> MetalUniforms {
        let mut uniforms =
            MetalUniforms::test_with_grid(screen_size, grid_size, cell_size, [0.0; 4], 0, bg_color);
        uniforms.projection_matrix =
            ortho2d(0.0, screen_size[0] as f32, screen_size[1] as f32, 0.0);
        uniforms.cursor_pos = [u16::MAX, u16::MAX];
        uniforms
    }

    fn ortho2d(left: f32, right: f32, bottom: f32, top: f32) -> [[f32; 4]; 4] {
        let width = right - left;
        let height = top - bottom;
        [
            [2.0 / width, 0.0, 0.0, 0.0],
            [0.0, 2.0 / height, 0.0, 0.0],
            [0.0, 0.0, -1.0, 0.0],
            [-(right + left) / width, -(top + bottom) / height, 0.0, 1.0],
        ]
    }

    fn cell_bg_buffer(
        device: &ProtocolObject<dyn MTLDevice>,
        cells: &[CellBg],
    ) -> MetalBuffer<CellBg> {
        MetalBuffer::init_fill(
            MetalBufferOptions {
                device,
                resource_options: MetalResourceOptions::image(MetalStorageMode::Shared),
            },
            cells,
        )
        .expect("cell background buffer should be created")
    }

    fn image_vertex_buffer(
        device: &ProtocolObject<dyn MTLDevice>,
        vertex: ImageVertex,
    ) -> MetalBuffer<ImageVertex> {
        MetalBuffer::init_fill(
            MetalBufferOptions {
                device,
                resource_options: MetalResourceOptions::image(MetalStorageMode::Shared),
            },
            &[vertex],
        )
        .expect("image vertex buffer should be created")
    }

    fn bg_image_vertex_buffer(
        device: &ProtocolObject<dyn MTLDevice>,
        vertex: BgImageVertex,
    ) -> MetalBuffer<BgImageVertex> {
        MetalBuffer::init_fill(
            MetalBufferOptions {
                device,
                resource_options: MetalResourceOptions::image(MetalStorageMode::Shared),
            },
            &[vertex],
        )
        .expect("background image vertex buffer should be created")
    }

    fn cell_text_vertex_buffer(
        device: &ProtocolObject<dyn MTLDevice>,
        vertex: CellTextVertex,
    ) -> MetalBuffer<CellTextVertex> {
        MetalBuffer::init_fill(
            MetalBufferOptions {
                device,
                resource_options: MetalResourceOptions::image(MetalStorageMode::Shared),
            },
            &[vertex],
        )
        .expect("cell text vertex buffer should be created")
    }

    fn image_texture(
        device: &ProtocolObject<dyn MTLDevice>,
        width: usize,
        height: usize,
        rgba: &[u8],
    ) -> MetalTexture {
        MetalTexture::new(
            device,
            image_texture_options(ImageTextureFormat::Rgba, false, MetalStorageMode::Shared),
            width,
            height,
            Some(rgba),
        )
        .expect("image texture should be created")
    }

    fn grayscale_atlas_texture(
        device: &ProtocolObject<dyn MTLDevice>,
        width: usize,
        height: usize,
        gray: &[u8],
    ) -> MetalTexture {
        MetalTexture::new(
            device,
            image_texture_options(ImageTextureFormat::Gray, false, MetalStorageMode::Shared),
            width,
            height,
            Some(gray),
        )
        .expect("grayscale atlas texture should be created")
    }

    fn dummy_color_atlas_texture(device: &ProtocolObject<dyn MTLDevice>) -> MetalTexture {
        image_texture(device, 1, 1, &[0, 0, 0, 0])
    }

    fn image_vertex(
        grid_pos: [f32; 2],
        cell_offset: [f32; 2],
        source_rect: [f32; 4],
        dest_size: [f32; 2],
    ) -> ImageVertex {
        ImageVertex {
            grid_pos,
            cell_offset,
            source_rect,
            dest_size,
        }
    }

    fn bg_image_vertex(opacity: f32, position: BgImagePosition, fit: BgImageFit) -> BgImageVertex {
        BgImageVertex {
            opacity,
            info: BgImageInfo::new(position, fit, false),
            _padding: [0; 3],
        }
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
            _padding: [0; 2],
        }
    }

    #[test]
    fn command_queue_creation_succeeds() {
        let queue = command_queue_is_created_from_default_device();
        let _ = queue;
    }

    #[test]
    fn command_buffer_status_mapping_is_deterministic() {
        assert_eq!(
            command_buffer_status_result(MTLCommandBufferStatus::Completed),
            Ok(())
        );
        assert_eq!(
            command_buffer_status_result(MTLCommandBufferStatus::Error),
            Err(MetalCommandFrameError::CommandBufferFailed)
        );
        assert_eq!(
            command_buffer_status_result(MTLCommandBufferStatus::Scheduled),
            Err(MetalCommandFrameError::CommandBufferNotCompleted(3))
        );
    }

    #[test]
    fn clear_only_render_pass_stores_bgra_bytes() {
        let device = metal_device();
        let queue = device
            .newCommandQueue()
            .expect("command queue should be created");
        let target = render_target(&device, 4, 4);
        let frame = MetalCommandFrame::begin(&queue).expect("command frame should begin");
        let pass = frame
            .render_pass(&[MetalRenderPassAttachment {
                texture: &target,
                clear_color: Some(MetalClearColor {
                    red: 32.0 / 255.0,
                    green: 64.0 / 255.0,
                    blue: 128.0 / 255.0,
                    alpha: 1.0,
                }),
            }])
            .expect("render pass should begin");

        pass.complete();
        frame
            .commit_and_wait()
            .expect("command frame should complete");

        assert_pixels(&target.read_bytes(), [128, 64, 32, 255]);
    }

    #[test]
    fn bg_color_render_pass_draws_production_shader_pixels() {
        let device = metal_device();
        let queue = device
            .newCommandQueue()
            .expect("command queue should be created");
        let pipelines = MetalStandardPipelines::new(&device, MetalPixelFormat::Bgra8Unorm)
            .expect("standard pipelines should compile");
        let uniforms = MetalUniforms::test_bg_color(4, 4, [32, 64, 128, 255]);
        let uniforms = MetalBuffer::init_fill(
            MetalBufferOptions {
                device: &device,
                resource_options: MetalResourceOptions::image(MetalStorageMode::Shared),
            },
            &[uniforms],
        )
        .expect("uniform buffer should be created");
        let target = render_target(&device, 4, 4);
        let frame = MetalCommandFrame::begin(&queue).expect("command frame should begin");
        let pass = frame
            .render_pass(&[MetalRenderPassAttachment {
                texture: &target,
                clear_color: Some(MetalClearColor {
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                    alpha: 0.0,
                }),
            }])
            .expect("render pass should begin");

        pass.step(MetalRenderPassStep {
            pipeline: &pipelines.bg_color,
            buffers: &[],
            textures: &[],
            uniforms: Some(uniforms.buffer()),
            draw: MetalDraw {
                primitive_type: MetalPrimitiveType::Triangle,
                vertex_count: 3,
                instance_count: 1,
            },
        });
        pass.complete();
        frame
            .commit_and_wait()
            .expect("command frame should complete");

        assert_pixels(&target.read_bytes(), [128, 64, 32, 255]);
    }

    #[test]
    fn cell_bg_render_pass_draws_per_cell_colors() {
        let device = metal_device();
        let queue = device
            .newCommandQueue()
            .expect("command queue should be created");
        let pipelines = MetalStandardPipelines::new(&device, MetalPixelFormat::Bgra8Unorm)
            .expect("standard pipelines should compile");
        let uniforms = uniform_buffer(&device, MetalUniforms::test_bg_color(4, 4, [0, 0, 0, 0]));
        let cells = (0..16u8)
            .map(|index| CellBg([16 + index * 7, 32 + index * 5, 48 + index * 3, 255]))
            .collect::<Vec<_>>();
        let cells = cell_bg_buffer(&device, &cells);
        let target = render_target(&device, 4, 4);
        let frame = MetalCommandFrame::begin(&queue).expect("command frame should begin");
        let pass = frame
            .render_pass(&[MetalRenderPassAttachment {
                texture: &target,
                clear_color: Some(MetalClearColor {
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                    alpha: 0.0,
                }),
            }])
            .expect("render pass should begin");

        pass.step(MetalRenderPassStep {
            pipeline: &pipelines.cell_bg,
            buffers: &[None, Some(cells.buffer())],
            textures: &[],
            uniforms: Some(uniforms.buffer()),
            draw: MetalDraw {
                primitive_type: MetalPrimitiveType::Triangle,
                vertex_count: 3,
                instance_count: 1,
            },
        });
        pass.complete();
        frame
            .commit_and_wait()
            .expect("command frame should complete");

        let expected = (0..16u8)
            .map(|index| [48 + index * 3, 32 + index * 5, 16 + index * 7, 255])
            .collect::<Vec<_>>();
        assert_pixel_grid(&target.read_bytes(), 4, &expected);
    }

    #[test]
    fn cell_bg_padding_without_extend_outputs_transparent() {
        let device = metal_device();
        let queue = device
            .newCommandQueue()
            .expect("command queue should be created");
        let pipelines = MetalStandardPipelines::new(&device, MetalPixelFormat::Bgra8Unorm)
            .expect("standard pipelines should compile");
        let uniforms = uniform_buffer(
            &device,
            MetalUniforms::test_with_grid(
                [5, 5],
                [2, 2],
                [1.0, 1.0],
                [1.0, 0.0, 0.0, 2.0],
                0,
                [0, 0, 0, 0],
            ),
        );
        let cells = cell_bg_buffer(
            &device,
            &[
                CellBg([32, 64, 96, 255]),
                CellBg([48, 80, 112, 255]),
                CellBg([64, 96, 128, 255]),
                CellBg([80, 112, 144, 255]),
            ],
        );
        let target = render_target(&device, 5, 5);
        let frame = MetalCommandFrame::begin(&queue).expect("command frame should begin");
        let pass = frame
            .render_pass(&[MetalRenderPassAttachment {
                texture: &target,
                clear_color: Some(MetalClearColor {
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                    alpha: 0.0,
                }),
            }])
            .expect("render pass should begin");

        pass.step(MetalRenderPassStep {
            pipeline: &pipelines.cell_bg,
            buffers: &[None, Some(cells.buffer())],
            textures: &[],
            uniforms: Some(uniforms.buffer()),
            draw: MetalDraw {
                primitive_type: MetalPrimitiveType::Triangle,
                vertex_count: 3,
                instance_count: 1,
            },
        });
        pass.complete();
        frame
            .commit_and_wait()
            .expect("command frame should complete");

        let transparent = [0, 0, 0, 0];
        assert_pixel_grid(
            &target.read_bytes(),
            5,
            &[
                transparent,
                transparent,
                transparent,
                transparent,
                transparent,
                transparent,
                transparent,
                [96, 64, 32, 255],
                [112, 80, 48, 255],
                transparent,
                transparent,
                transparent,
                [128, 96, 64, 255],
                [144, 112, 80, 255],
                transparent,
                transparent,
                transparent,
                transparent,
                transparent,
                transparent,
                transparent,
                transparent,
                transparent,
                transparent,
                transparent,
            ],
        );
    }

    #[test]
    fn cell_bg_zero_instance_step_does_not_bind_or_draw() {
        let device = metal_device();
        let queue = device
            .newCommandQueue()
            .expect("command queue should be created");
        let pipelines = MetalStandardPipelines::new(&device, MetalPixelFormat::Bgra8Unorm)
            .expect("standard pipelines should compile");
        let uniforms = uniform_buffer(&device, MetalUniforms::test_bg_color(4, 4, [0, 0, 0, 0]));
        let cells = cell_bg_buffer(&device, &[CellBg([255, 0, 0, 255]); 16]);
        let target = render_target(&device, 4, 4);
        let frame = MetalCommandFrame::begin(&queue).expect("command frame should begin");
        let pass = frame
            .render_pass(&[MetalRenderPassAttachment {
                texture: &target,
                clear_color: Some(MetalClearColor {
                    red: 0.0,
                    green: 1.0,
                    blue: 0.0,
                    alpha: 1.0,
                }),
            }])
            .expect("render pass should begin");

        pass.step(MetalRenderPassStep {
            pipeline: &pipelines.cell_bg,
            buffers: &[None, Some(cells.buffer())],
            textures: &[],
            uniforms: Some(uniforms.buffer()),
            draw: MetalDraw {
                primitive_type: MetalPrimitiveType::Triangle,
                vertex_count: 3,
                instance_count: 0,
            },
        });
        pass.complete();
        frame
            .commit_and_wait()
            .expect("command frame should complete");

        assert_pixels(&target.read_bytes(), [0, 255, 0, 255]);
    }

    #[test]
    fn cell_text_grayscale_render_pass_draws_atlas_mask() {
        let device = metal_device();
        let queue = device
            .newCommandQueue()
            .expect("command queue should be created");
        let pipelines = MetalStandardPipelines::new(&device, MetalPixelFormat::Bgra8Unorm)
            .expect("standard pipelines should compile");
        let uniforms = uniform_buffer(
            &device,
            cell_text_uniforms([2, 2], [1, 1], [2.0, 2.0], [0, 0, 0, 0]),
        );
        let cells = cell_bg_buffer(&device, &[CellBg([0, 0, 0, 0])]);
        let vertices = cell_text_vertex_buffer(
            &device,
            cell_text_vertex([0, 0], [2, 2], [0, 2], [0, 0], [255, 0, 0, 255]),
        );
        let grayscale_atlas = grayscale_atlas_texture(&device, 2, 2, &[255, 0, 0, 255]);
        let color_atlas = dummy_color_atlas_texture(&device);
        let target = render_target(&device, 2, 2);
        let frame = MetalCommandFrame::begin(&queue).expect("command frame should begin");
        let pass = frame
            .render_pass(&[MetalRenderPassAttachment {
                texture: &target,
                clear_color: Some(MetalClearColor {
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                    alpha: 0.0,
                }),
            }])
            .expect("render pass should begin");

        pass.step(MetalRenderPassStep {
            pipeline: &pipelines.cell_text,
            buffers: &[Some(vertices.buffer()), Some(cells.buffer())],
            textures: &[Some(&grayscale_atlas), Some(&color_atlas)],
            uniforms: Some(uniforms.buffer()),
            draw: MetalDraw {
                primitive_type: MetalPrimitiveType::TriangleStrip,
                vertex_count: 4,
                instance_count: 1,
            },
        });
        pass.complete();
        frame
            .commit_and_wait()
            .expect("command frame should complete");

        let transparent = [0, 0, 0, 0];
        let red = [0, 0, 255, 255];
        assert_pixel_grid(
            &target.read_bytes(),
            2,
            &[red, transparent, transparent, red],
        );
    }

    #[test]
    fn cell_text_grayscale_respects_bearings_and_glyph_size() {
        let device = metal_device();
        let queue = device
            .newCommandQueue()
            .expect("command queue should be created");
        let pipelines = MetalStandardPipelines::new(&device, MetalPixelFormat::Bgra8Unorm)
            .expect("standard pipelines should compile");
        let uniforms = uniform_buffer(
            &device,
            cell_text_uniforms([3, 3], [1, 1], [3.0, 3.0], [0, 0, 0, 0]),
        );
        let cells = cell_bg_buffer(&device, &[CellBg([0, 0, 0, 0])]);
        let vertices = cell_text_vertex_buffer(
            &device,
            cell_text_vertex([0, 0], [1, 1], [1, 2], [0, 0], [255, 0, 0, 255]),
        );
        let grayscale_atlas = grayscale_atlas_texture(&device, 1, 1, &[255]);
        let color_atlas = dummy_color_atlas_texture(&device);
        let target = render_target(&device, 3, 3);
        let frame = MetalCommandFrame::begin(&queue).expect("command frame should begin");
        let pass = frame
            .render_pass(&[MetalRenderPassAttachment {
                texture: &target,
                clear_color: Some(MetalClearColor {
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                    alpha: 0.0,
                }),
            }])
            .expect("render pass should begin");

        pass.step(MetalRenderPassStep {
            pipeline: &pipelines.cell_text,
            buffers: &[Some(vertices.buffer()), Some(cells.buffer())],
            textures: &[Some(&grayscale_atlas), Some(&color_atlas)],
            uniforms: Some(uniforms.buffer()),
            draw: MetalDraw {
                primitive_type: MetalPrimitiveType::TriangleStrip,
                vertex_count: 4,
                instance_count: 1,
            },
        });
        pass.complete();
        frame
            .commit_and_wait()
            .expect("command frame should complete");

        let transparent = [0, 0, 0, 0];
        let red = [0, 0, 255, 255];
        assert_pixel_grid(
            &target.read_bytes(),
            3,
            &[
                transparent,
                transparent,
                transparent,
                transparent,
                red,
                transparent,
                transparent,
                transparent,
                transparent,
            ],
        );
    }

    #[test]
    fn cell_text_zero_instance_step_does_not_bind_or_draw() {
        let device = metal_device();
        let queue = device
            .newCommandQueue()
            .expect("command queue should be created");
        let pipelines = MetalStandardPipelines::new(&device, MetalPixelFormat::Bgra8Unorm)
            .expect("standard pipelines should compile");
        let uniforms = uniform_buffer(
            &device,
            cell_text_uniforms([2, 2], [1, 1], [2.0, 2.0], [0, 0, 0, 0]),
        );
        let cells = cell_bg_buffer(&device, &[CellBg([0, 0, 0, 0])]);
        let vertices = cell_text_vertex_buffer(
            &device,
            cell_text_vertex([0, 0], [1, 1], [0, 1], [0, 0], [255, 0, 0, 255]),
        );
        let grayscale_atlas = grayscale_atlas_texture(&device, 1, 1, &[255]);
        let color_atlas = dummy_color_atlas_texture(&device);
        let target = render_target(&device, 2, 2);
        let frame = MetalCommandFrame::begin(&queue).expect("command frame should begin");
        let pass = frame
            .render_pass(&[MetalRenderPassAttachment {
                texture: &target,
                clear_color: Some(MetalClearColor {
                    red: 0.0,
                    green: 1.0,
                    blue: 0.0,
                    alpha: 1.0,
                }),
            }])
            .expect("render pass should begin");

        pass.step(MetalRenderPassStep {
            pipeline: &pipelines.cell_text,
            buffers: &[Some(vertices.buffer()), Some(cells.buffer())],
            textures: &[Some(&grayscale_atlas), Some(&color_atlas)],
            uniforms: Some(uniforms.buffer()),
            draw: MetalDraw {
                primitive_type: MetalPrimitiveType::TriangleStrip,
                vertex_count: 4,
                instance_count: 0,
            },
        });
        pass.complete();
        frame
            .commit_and_wait()
            .expect("command frame should complete");

        assert_pixels(&target.read_bytes(), [0, 255, 0, 255]);
    }

    #[test]
    fn image_render_pass_draws_uploaded_texture_pixels() {
        let device = metal_device();
        let queue = device
            .newCommandQueue()
            .expect("command queue should be created");
        let pipelines = MetalStandardPipelines::new(&device, MetalPixelFormat::Bgra8Unorm)
            .expect("standard pipelines should compile");
        let uniforms = uniform_buffer(&device, image_uniforms(2, 2));
        let vertices = image_vertex_buffer(
            &device,
            image_vertex([0.0, 0.0], [0.0, 0.0], [0.0, 0.0, 2.0, 2.0], [2.0, 2.0]),
        );
        let image = image_texture(
            &device,
            2,
            2,
            &[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        );
        let target = render_target(&device, 2, 2);
        let frame = MetalCommandFrame::begin(&queue).expect("command frame should begin");
        let pass = frame
            .render_pass(&[MetalRenderPassAttachment {
                texture: &target,
                clear_color: Some(MetalClearColor {
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                    alpha: 0.0,
                }),
            }])
            .expect("render pass should begin");

        pass.step(MetalRenderPassStep {
            pipeline: &pipelines.image,
            buffers: &[Some(vertices.buffer())],
            textures: &[Some(&image)],
            uniforms: Some(uniforms.buffer()),
            draw: MetalDraw {
                primitive_type: MetalPrimitiveType::TriangleStrip,
                vertex_count: 4,
                instance_count: 1,
            },
        });
        pass.complete();
        frame
            .commit_and_wait()
            .expect("command frame should complete");

        assert_pixel_grid(
            &target.read_bytes(),
            2,
            &[
                [0, 0, 255, 255],
                [0, 255, 0, 255],
                [255, 0, 0, 255],
                [255, 255, 255, 255],
            ],
        );
    }

    #[test]
    fn image_render_pass_respects_cell_offset_and_dest_size() {
        let device = metal_device();
        let queue = device
            .newCommandQueue()
            .expect("command queue should be created");
        let pipelines = MetalStandardPipelines::new(&device, MetalPixelFormat::Bgra8Unorm)
            .expect("standard pipelines should compile");
        let uniforms = uniform_buffer(&device, image_uniforms(4, 4));
        let vertices = image_vertex_buffer(
            &device,
            image_vertex([0.0, 0.0], [1.0, 1.0], [0.0, 0.0, 1.0, 1.0], [2.0, 2.0]),
        );
        let image = image_texture(&device, 1, 1, &[255, 0, 0, 255]);
        let target = render_target(&device, 4, 4);
        let frame = MetalCommandFrame::begin(&queue).expect("command frame should begin");
        let pass = frame
            .render_pass(&[MetalRenderPassAttachment {
                texture: &target,
                clear_color: Some(MetalClearColor {
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                    alpha: 0.0,
                }),
            }])
            .expect("render pass should begin");

        pass.step(MetalRenderPassStep {
            pipeline: &pipelines.image,
            buffers: &[Some(vertices.buffer())],
            textures: &[Some(&image)],
            uniforms: Some(uniforms.buffer()),
            draw: MetalDraw {
                primitive_type: MetalPrimitiveType::TriangleStrip,
                vertex_count: 4,
                instance_count: 1,
            },
        });
        pass.complete();
        frame
            .commit_and_wait()
            .expect("command frame should complete");

        let transparent = [0, 0, 0, 0];
        let red = [0, 0, 255, 255];
        assert_pixel_grid(
            &target.read_bytes(),
            4,
            &[
                transparent,
                transparent,
                transparent,
                transparent,
                transparent,
                red,
                red,
                transparent,
                transparent,
                red,
                red,
                transparent,
                transparent,
                transparent,
                transparent,
                transparent,
            ],
        );
    }

    #[test]
    fn image_zero_instance_step_does_not_bind_or_draw() {
        let device = metal_device();
        let queue = device
            .newCommandQueue()
            .expect("command queue should be created");
        let pipelines = MetalStandardPipelines::new(&device, MetalPixelFormat::Bgra8Unorm)
            .expect("standard pipelines should compile");
        let uniforms = uniform_buffer(&device, image_uniforms(2, 2));
        let vertices = image_vertex_buffer(
            &device,
            image_vertex([0.0, 0.0], [0.0, 0.0], [0.0, 0.0, 1.0, 1.0], [2.0, 2.0]),
        );
        let image = image_texture(&device, 1, 1, &[255, 0, 0, 255]);
        let target = render_target(&device, 2, 2);
        let frame = MetalCommandFrame::begin(&queue).expect("command frame should begin");
        let pass = frame
            .render_pass(&[MetalRenderPassAttachment {
                texture: &target,
                clear_color: Some(MetalClearColor {
                    red: 0.0,
                    green: 1.0,
                    blue: 0.0,
                    alpha: 1.0,
                }),
            }])
            .expect("render pass should begin");

        pass.step(MetalRenderPassStep {
            pipeline: &pipelines.image,
            buffers: &[Some(vertices.buffer())],
            textures: &[Some(&image)],
            uniforms: Some(uniforms.buffer()),
            draw: MetalDraw {
                primitive_type: MetalPrimitiveType::TriangleStrip,
                vertex_count: 4,
                instance_count: 0,
            },
        });
        pass.complete();
        frame
            .commit_and_wait()
            .expect("command frame should complete");

        assert_pixels(&target.read_bytes(), [0, 255, 0, 255]);
    }

    #[test]
    fn bg_image_render_pass_draws_texture_over_background() {
        let device = metal_device();
        let queue = device
            .newCommandQueue()
            .expect("command queue should be created");
        let pipelines = MetalStandardPipelines::new(&device, MetalPixelFormat::Bgra8Unorm)
            .expect("standard pipelines should compile");
        let uniforms = uniform_buffer(&device, screen_uniforms(2, 2, [0, 0, 0, 255]));
        let vertices = bg_image_vertex_buffer(
            &device,
            bg_image_vertex(1.0, BgImagePosition::TopLeft, BgImageFit::Stretch),
        );
        let image = image_texture(
            &device,
            2,
            2,
            &[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        );
        let target = render_target(&device, 2, 2);
        let frame = MetalCommandFrame::begin(&queue).expect("command frame should begin");
        let pass = frame
            .render_pass(&[MetalRenderPassAttachment {
                texture: &target,
                clear_color: Some(MetalClearColor {
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                    alpha: 0.0,
                }),
            }])
            .expect("render pass should begin");

        pass.step(MetalRenderPassStep {
            pipeline: &pipelines.bg_image,
            buffers: &[Some(vertices.buffer())],
            textures: &[Some(&image)],
            uniforms: Some(uniforms.buffer()),
            draw: MetalDraw {
                primitive_type: MetalPrimitiveType::Triangle,
                vertex_count: 3,
                instance_count: 1,
            },
        });
        pass.complete();
        frame
            .commit_and_wait()
            .expect("command frame should complete");

        assert_pixel_grid(
            &target.read_bytes(),
            2,
            &[
                [0, 0, 255, 255],
                [0, 255, 0, 255],
                [255, 0, 0, 255],
                [255, 255, 255, 255],
            ],
        );
    }

    #[test]
    fn bg_image_none_fit_uses_vertex_texture_size_for_placement() {
        let device = metal_device();
        let queue = device
            .newCommandQueue()
            .expect("command queue should be created");
        let pipelines = MetalStandardPipelines::new(&device, MetalPixelFormat::Bgra8Unorm)
            .expect("standard pipelines should compile");
        let uniforms = uniform_buffer(&device, screen_uniforms(4, 4, [0, 0, 0, 255]));
        let vertices = bg_image_vertex_buffer(
            &device,
            bg_image_vertex(1.0, BgImagePosition::MiddleCenter, BgImageFit::None),
        );
        let image = image_texture(
            &device,
            2,
            2,
            &[
                255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
            ],
        );
        let target = render_target(&device, 4, 4);
        let frame = MetalCommandFrame::begin(&queue).expect("command frame should begin");
        let pass = frame
            .render_pass(&[MetalRenderPassAttachment {
                texture: &target,
                clear_color: Some(MetalClearColor {
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                    alpha: 0.0,
                }),
            }])
            .expect("render pass should begin");

        pass.step(MetalRenderPassStep {
            pipeline: &pipelines.bg_image,
            buffers: &[Some(vertices.buffer())],
            textures: &[Some(&image)],
            uniforms: Some(uniforms.buffer()),
            draw: MetalDraw {
                primitive_type: MetalPrimitiveType::Triangle,
                vertex_count: 3,
                instance_count: 1,
            },
        });
        pass.complete();
        frame
            .commit_and_wait()
            .expect("command frame should complete");

        let black = [0, 0, 0, 255];
        let red = [0, 0, 255, 255];
        assert_pixel_grid(
            &target.read_bytes(),
            4,
            &[
                black, black, black, black, black, red, red, black, black, red, red, black, black,
                black, black, black,
            ],
        );
    }

    #[test]
    fn bg_image_zero_instance_step_does_not_bind_or_draw() {
        let device = metal_device();
        let queue = device
            .newCommandQueue()
            .expect("command queue should be created");
        let pipelines = MetalStandardPipelines::new(&device, MetalPixelFormat::Bgra8Unorm)
            .expect("standard pipelines should compile");
        let uniforms = uniform_buffer(&device, screen_uniforms(2, 2, [0, 0, 0, 255]));
        let vertices = bg_image_vertex_buffer(
            &device,
            bg_image_vertex(1.0, BgImagePosition::TopLeft, BgImageFit::Stretch),
        );
        let image = image_texture(&device, 1, 1, &[255, 0, 0, 255]);
        let target = render_target(&device, 2, 2);
        let frame = MetalCommandFrame::begin(&queue).expect("command frame should begin");
        let pass = frame
            .render_pass(&[MetalRenderPassAttachment {
                texture: &target,
                clear_color: Some(MetalClearColor {
                    red: 0.0,
                    green: 1.0,
                    blue: 0.0,
                    alpha: 1.0,
                }),
            }])
            .expect("render pass should begin");

        pass.step(MetalRenderPassStep {
            pipeline: &pipelines.bg_image,
            buffers: &[Some(vertices.buffer())],
            textures: &[Some(&image)],
            uniforms: Some(uniforms.buffer()),
            draw: MetalDraw {
                primitive_type: MetalPrimitiveType::Triangle,
                vertex_count: 3,
                instance_count: 0,
            },
        });
        pass.complete();
        frame
            .commit_and_wait()
            .expect("command frame should complete");

        assert_pixels(&target.read_bytes(), [0, 255, 0, 255]);
    }

    #[test]
    fn zero_instance_render_pass_step_does_not_draw() {
        let device = metal_device();
        let queue = device
            .newCommandQueue()
            .expect("command queue should be created");
        let pipelines = MetalStandardPipelines::new(&device, MetalPixelFormat::Bgra8Unorm)
            .expect("standard pipelines should compile");
        let uniforms = MetalUniforms::test_bg_color(4, 4, [255, 0, 0, 255]);
        let uniforms = MetalBuffer::init_fill(
            MetalBufferOptions {
                device: &device,
                resource_options: MetalResourceOptions::image(MetalStorageMode::Shared),
            },
            &[uniforms],
        )
        .expect("uniform buffer should be created");
        let target = render_target(&device, 4, 4);
        let frame = MetalCommandFrame::begin(&queue).expect("command frame should begin");
        let pass = frame
            .render_pass(&[MetalRenderPassAttachment {
                texture: &target,
                clear_color: Some(MetalClearColor {
                    red: 0.0,
                    green: 1.0,
                    blue: 0.0,
                    alpha: 1.0,
                }),
            }])
            .expect("render pass should begin");

        pass.step(MetalRenderPassStep {
            pipeline: &pipelines.bg_color,
            buffers: &[],
            textures: &[],
            uniforms: Some(uniforms.buffer()),
            draw: MetalDraw {
                primitive_type: MetalPrimitiveType::Triangle,
                vertex_count: 3,
                instance_count: 0,
            },
        });
        pass.complete();
        frame
            .commit_and_wait()
            .expect("command frame should complete");

        assert_pixels(&target.read_bytes(), [0, 255, 0, 255]);
    }
}
