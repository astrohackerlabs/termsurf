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
    use crate::renderer::metal::texture::render_target_texture_options;

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
