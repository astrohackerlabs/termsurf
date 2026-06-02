#![allow(dead_code)]
// Pipeline descriptor values are consumed by later renderer slices.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLDevice, MTLLibrary, MTLRenderPipelineColorAttachmentDescriptor, MTLRenderPipelineDescriptor,
    MTLRenderPipelineState, MTLVertexDescriptor,
};

use crate::renderer::metal::api::{
    MetalBlendFactor, MetalBlendOperation, MetalPixelFormat, MetalVertexFormat,
    MetalVertexStepFunction,
};
use crate::renderer::shader::{BgImageVertex, CellTextVertex, ImageVertex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MetalVertexAttribute {
    pub(crate) format: MetalVertexFormat,
    pub(crate) offset: usize,
    pub(crate) buffer_index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MetalVertexLayout {
    pub(crate) stride: usize,
    pub(crate) step_function: MetalVertexStepFunction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MetalVertexDescriptor {
    pub(crate) attributes: Vec<MetalVertexAttribute>,
    pub(crate) layout: MetalVertexLayout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MetalPipelineAttachmentOptions {
    pub(crate) pixel_format: MetalPixelFormat,
    pub(crate) blending_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MetalPipelineAttachmentDescriptor {
    pub(crate) pixel_format: MetalPixelFormat,
    pub(crate) blending_enabled: bool,
    pub(crate) blend: Option<MetalBlendDescriptor>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MetalBlendDescriptor {
    pub(crate) rgb_operation: MetalBlendOperation,
    pub(crate) alpha_operation: MetalBlendOperation,
    pub(crate) source_rgb_factor: MetalBlendFactor,
    pub(crate) source_alpha_factor: MetalBlendFactor,
    pub(crate) destination_rgb_factor: MetalBlendFactor,
    pub(crate) destination_alpha_factor: MetalBlendFactor,
}

pub(crate) fn pipeline_attachment_descriptor(
    options: MetalPipelineAttachmentOptions,
) -> MetalPipelineAttachmentDescriptor {
    MetalPipelineAttachmentDescriptor {
        pixel_format: options.pixel_format,
        blending_enabled: options.blending_enabled,
        blend: options
            .blending_enabled
            .then_some(premultiplied_alpha_blend()),
    }
}

fn premultiplied_alpha_blend() -> MetalBlendDescriptor {
    MetalBlendDescriptor {
        rgb_operation: MetalBlendOperation::Add,
        alpha_operation: MetalBlendOperation::Add,
        source_rgb_factor: MetalBlendFactor::One,
        source_alpha_factor: MetalBlendFactor::One,
        destination_rgb_factor: MetalBlendFactor::OneMinusSourceAlpha,
        destination_alpha_factor: MetalBlendFactor::OneMinusSourceAlpha,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MetalPipelineVertexInputKind {
    None,
    CellText,
    Image,
    BgImage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MetalStandardPipelineDescription {
    pub(crate) name: &'static str,
    pub(crate) vertex_function: &'static str,
    pub(crate) fragment_function: &'static str,
    pub(crate) vertex_input: MetalPipelineVertexInputKind,
    pub(crate) step_function: MetalVertexStepFunction,
    pub(crate) blending_enabled: bool,
}

pub(crate) const STANDARD_PIPELINE_DESCRIPTIONS: &[MetalStandardPipelineDescription] = &[
    MetalStandardPipelineDescription {
        name: "bg_color",
        vertex_function: "full_screen_vertex",
        fragment_function: "bg_color_fragment",
        vertex_input: MetalPipelineVertexInputKind::None,
        step_function: MetalVertexStepFunction::PerVertex,
        blending_enabled: false,
    },
    MetalStandardPipelineDescription {
        name: "cell_bg",
        vertex_function: "full_screen_vertex",
        fragment_function: "cell_bg_fragment",
        vertex_input: MetalPipelineVertexInputKind::None,
        step_function: MetalVertexStepFunction::PerVertex,
        blending_enabled: true,
    },
    MetalStandardPipelineDescription {
        name: "cell_text",
        vertex_function: "cell_text_vertex",
        fragment_function: "cell_text_fragment",
        vertex_input: MetalPipelineVertexInputKind::CellText,
        step_function: MetalVertexStepFunction::PerInstance,
        blending_enabled: true,
    },
    MetalStandardPipelineDescription {
        name: "image",
        vertex_function: "image_vertex",
        fragment_function: "image_fragment",
        vertex_input: MetalPipelineVertexInputKind::Image,
        step_function: MetalVertexStepFunction::PerInstance,
        blending_enabled: true,
    },
    MetalStandardPipelineDescription {
        name: "bg_image",
        vertex_function: "bg_image_vertex",
        fragment_function: "bg_image_fragment",
        vertex_input: MetalPipelineVertexInputKind::BgImage,
        step_function: MetalVertexStepFunction::PerInstance,
        blending_enabled: true,
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MetalPipelineBuildValues {
    pub(crate) name: &'static str,
    pub(crate) vertex_function: &'static str,
    pub(crate) fragment_function: &'static str,
    pub(crate) vertex_input: MetalPipelineVertexInputKind,
    pub(crate) vertex_descriptor: Option<MetalVertexDescriptor>,
    pub(crate) attachment: MetalPipelineAttachmentDescriptor,
}

pub(crate) fn standard_pipeline_build_values(
    description: MetalStandardPipelineDescription,
    pixel_format: MetalPixelFormat,
) -> MetalPipelineBuildValues {
    MetalPipelineBuildValues {
        name: description.name,
        vertex_function: description.vertex_function,
        fragment_function: description.fragment_function,
        vertex_input: description.vertex_input,
        vertex_descriptor: match description.vertex_input {
            MetalPipelineVertexInputKind::None => None,
            MetalPipelineVertexInputKind::CellText => {
                Some(CellTextVertex::vertex_descriptor(description.step_function))
            }
            MetalPipelineVertexInputKind::Image => {
                Some(ImageVertex::vertex_descriptor(description.step_function))
            }
            MetalPipelineVertexInputKind::BgImage => {
                Some(BgImageVertex::vertex_descriptor(description.step_function))
            }
        },
        attachment: pipeline_attachment_descriptor(MetalPipelineAttachmentOptions {
            pixel_format,
            blending_enabled: description.blending_enabled,
        }),
    }
}

#[derive(Debug)]
pub(crate) struct MetalPipeline {
    state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
}

impl MetalPipeline {
    pub(crate) fn new(options: MetalPipelineOptions<'_>) -> Result<Self, MetalPipelineError> {
        let descriptor = MTLRenderPipelineDescriptor::new();

        let vertex_name = NSString::from_str(options.values.vertex_function);
        let vertex_function = options
            .vertex_library
            .newFunctionWithName(&vertex_name)
            .ok_or(MetalPipelineError::MissingVertexFunction(
                options.values.vertex_function,
            ))?;
        descriptor.setVertexFunction(Some(&vertex_function));

        let fragment_name = NSString::from_str(options.values.fragment_function);
        let fragment_function = options
            .fragment_library
            .newFunctionWithName(&fragment_name)
            .ok_or(MetalPipelineError::MissingFragmentFunction(
                options.values.fragment_function,
            ))?;
        descriptor.setFragmentFunction(Some(&fragment_function));

        if let Some(vertex_descriptor) = &options.values.vertex_descriptor {
            let descriptor_objc = build_metal_vertex_descriptor(vertex_descriptor);
            descriptor.setVertexDescriptor(Some(&descriptor_objc));
        }

        let attachments = descriptor.colorAttachments();
        let attachment = unsafe { attachments.objectAtIndexedSubscript(0) };
        apply_pipeline_attachment_descriptor(&attachment, options.values.attachment);

        let state = options
            .device
            .newRenderPipelineStateWithDescriptor_error(&descriptor)
            .map_err(|error| MetalPipelineError::PipelineCreationFailed(error.to_string()))?;

        Ok(Self { state })
    }

    pub(crate) fn state(&self) -> &ProtocolObject<dyn MTLRenderPipelineState> {
        &self.state
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MetalPipelineOptions<'a> {
    pub(crate) device: &'a ProtocolObject<dyn MTLDevice>,
    pub(crate) vertex_library: &'a ProtocolObject<dyn MTLLibrary>,
    pub(crate) fragment_library: &'a ProtocolObject<dyn MTLLibrary>,
    pub(crate) values: MetalPipelineBuildValues,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MetalPipelineError {
    MissingVertexFunction(&'static str),
    MissingFragmentFunction(&'static str),
    PipelineCreationFailed(String),
}

pub(crate) fn build_metal_vertex_descriptor(
    descriptor: &MetalVertexDescriptor,
) -> Retained<MTLVertexDescriptor> {
    let objc_descriptor = MTLVertexDescriptor::vertexDescriptor();
    let attributes = objc_descriptor.attributes();
    for (index, attribute) in descriptor.attributes.iter().enumerate() {
        let target = unsafe { attributes.objectAtIndexedSubscript(index) };
        target.setFormat(attribute.format.to_objc());
        unsafe {
            target.setOffset(attribute.offset);
            target.setBufferIndex(attribute.buffer_index);
        }
    }

    let layouts = objc_descriptor.layouts();
    let layout = unsafe { layouts.objectAtIndexedSubscript(0) };
    unsafe {
        layout.setStride(descriptor.layout.stride);
    }
    layout.setStepFunction(descriptor.layout.step_function.to_objc());

    objc_descriptor
}

pub(crate) fn apply_pipeline_attachment_descriptor(
    target: &MTLRenderPipelineColorAttachmentDescriptor,
    descriptor: MetalPipelineAttachmentDescriptor,
) {
    target.setPixelFormat(descriptor.pixel_format.to_objc());
    target.setBlendingEnabled(descriptor.blending_enabled);

    if let Some(blend) = descriptor.blend {
        target.setRgbBlendOperation(blend.rgb_operation.to_objc());
        target.setAlphaBlendOperation(blend.alpha_operation.to_objc());
        target.setSourceRGBBlendFactor(blend.source_rgb_factor.to_objc());
        target.setSourceAlphaBlendFactor(blend.source_alpha_factor.to_objc());
        target.setDestinationRGBBlendFactor(blend.destination_rgb_factor.to_objc());
        target.setDestinationAlphaBlendFactor(blend.destination_alpha_factor.to_objc());
    }
}

pub(crate) trait MetalVertexInput {
    fn vertex_descriptor(step_function: MetalVertexStepFunction) -> MetalVertexDescriptor;
}

impl MetalVertexInput for CellTextVertex {
    fn vertex_descriptor(step_function: MetalVertexStepFunction) -> MetalVertexDescriptor {
        MetalVertexDescriptor {
            attributes: vec![
                MetalVertexAttribute {
                    format: MetalVertexFormat::UInt2,
                    offset: std::mem::offset_of!(CellTextVertex, glyph_pos),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::UInt2,
                    offset: std::mem::offset_of!(CellTextVertex, glyph_size),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::Short2,
                    offset: std::mem::offset_of!(CellTextVertex, bearings),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::UShort2,
                    offset: std::mem::offset_of!(CellTextVertex, grid_pos),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::UChar4,
                    offset: std::mem::offset_of!(CellTextVertex, color),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::UChar,
                    offset: std::mem::offset_of!(CellTextVertex, atlas),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::UChar,
                    offset: std::mem::offset_of!(CellTextVertex, flags),
                    buffer_index: 0,
                },
            ],
            layout: MetalVertexLayout {
                stride: std::mem::size_of::<CellTextVertex>(),
                step_function,
            },
        }
    }
}

impl MetalVertexInput for ImageVertex {
    fn vertex_descriptor(step_function: MetalVertexStepFunction) -> MetalVertexDescriptor {
        MetalVertexDescriptor {
            attributes: vec![
                MetalVertexAttribute {
                    format: MetalVertexFormat::Float2,
                    offset: std::mem::offset_of!(ImageVertex, grid_pos),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::Float2,
                    offset: std::mem::offset_of!(ImageVertex, cell_offset),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::Float4,
                    offset: std::mem::offset_of!(ImageVertex, source_rect),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::Float2,
                    offset: std::mem::offset_of!(ImageVertex, dest_size),
                    buffer_index: 0,
                },
            ],
            layout: MetalVertexLayout {
                stride: std::mem::size_of::<ImageVertex>(),
                step_function,
            },
        }
    }
}

impl MetalVertexInput for BgImageVertex {
    fn vertex_descriptor(step_function: MetalVertexStepFunction) -> MetalVertexDescriptor {
        MetalVertexDescriptor {
            attributes: vec![
                MetalVertexAttribute {
                    format: MetalVertexFormat::Float,
                    offset: std::mem::offset_of!(BgImageVertex, opacity),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::UChar,
                    offset: std::mem::offset_of!(BgImageVertex, info),
                    buffer_index: 0,
                },
            ],
            layout: MetalVertexLayout {
                stride: std::mem::size_of::<BgImageVertex>(),
                step_function,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_metal::MTLCreateSystemDefaultDevice;

    const TEST_SHADER_SOURCE: &str = r#"
#include <metal_stdlib>
using namespace metal;

struct VertexOut {
    float4 position [[position]];
};

vertex VertexOut full_screen_vertex(uint vertex_id [[vertex_id]]) {
    float2 positions[4] = {
        float2(-1.0, -1.0),
        float2( 1.0, -1.0),
        float2(-1.0,  1.0),
        float2( 1.0,  1.0),
    };
    VertexOut out;
    out.position = float4(positions[vertex_id], 0.0, 1.0);
    return out;
}

fragment float4 bg_color_fragment() {
    return float4(0.0, 0.0, 0.0, 1.0);
}

fragment float4 cell_bg_fragment() {
    return float4(0.0, 0.0, 0.0, 1.0);
}

struct ImageIn {
    float2 grid_pos [[attribute(0)]];
    float2 cell_offset [[attribute(1)]];
    float4 source_rect [[attribute(2)]];
    float2 dest_size [[attribute(3)]];
};

vertex VertexOut image_vertex(ImageIn in [[stage_in]]) {
    VertexOut out;
    out.position = float4(in.grid_pos, 0.0, 1.0);
    return out;
}

fragment float4 image_fragment() {
    return float4(1.0, 1.0, 1.0, 1.0);
}
"#;

    fn metal_device() -> Retained<ProtocolObject<dyn MTLDevice>> {
        MTLCreateSystemDefaultDevice().expect("Roastty requires a Metal device")
    }

    fn test_library(
        device: &ProtocolObject<dyn MTLDevice>,
    ) -> Retained<ProtocolObject<dyn MTLLibrary>> {
        let source = NSString::from_str(TEST_SHADER_SOURCE);
        device
            .newLibraryWithSource_options_error(&source, None)
            .expect("test shader source should compile")
    }

    fn standard_values(name: &str, pixel_format: MetalPixelFormat) -> MetalPipelineBuildValues {
        let description = STANDARD_PIPELINE_DESCRIPTIONS
            .iter()
            .copied()
            .find(|description| description.name == name)
            .expect("standard pipeline exists");
        standard_pipeline_build_values(description, pixel_format)
    }

    #[test]
    fn cell_text_vertex_descriptor_maps_fields_to_upstream_attributes() {
        let descriptor = CellTextVertex::vertex_descriptor(MetalVertexStepFunction::PerInstance);

        assert_eq!(
            descriptor.attributes,
            vec![
                MetalVertexAttribute {
                    format: MetalVertexFormat::UInt2,
                    offset: std::mem::offset_of!(CellTextVertex, glyph_pos),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::UInt2,
                    offset: std::mem::offset_of!(CellTextVertex, glyph_size),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::Short2,
                    offset: std::mem::offset_of!(CellTextVertex, bearings),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::UShort2,
                    offset: std::mem::offset_of!(CellTextVertex, grid_pos),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::UChar4,
                    offset: std::mem::offset_of!(CellTextVertex, color),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::UChar,
                    offset: std::mem::offset_of!(CellTextVertex, atlas),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::UChar,
                    offset: std::mem::offset_of!(CellTextVertex, flags),
                    buffer_index: 0,
                },
            ]
        );
        assert_eq!(
            descriptor.layout,
            MetalVertexLayout {
                stride: std::mem::size_of::<CellTextVertex>(),
                step_function: MetalVertexStepFunction::PerInstance,
            }
        );
    }

    #[test]
    fn image_vertex_descriptor_maps_fields_to_upstream_attributes() {
        let descriptor = ImageVertex::vertex_descriptor(MetalVertexStepFunction::PerVertex);

        assert_eq!(
            descriptor.attributes,
            vec![
                MetalVertexAttribute {
                    format: MetalVertexFormat::Float2,
                    offset: std::mem::offset_of!(ImageVertex, grid_pos),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::Float2,
                    offset: std::mem::offset_of!(ImageVertex, cell_offset),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::Float4,
                    offset: std::mem::offset_of!(ImageVertex, source_rect),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::Float2,
                    offset: std::mem::offset_of!(ImageVertex, dest_size),
                    buffer_index: 0,
                },
            ]
        );
        assert_eq!(
            descriptor.layout,
            MetalVertexLayout {
                stride: std::mem::size_of::<ImageVertex>(),
                step_function: MetalVertexStepFunction::PerVertex,
            }
        );
    }

    #[test]
    fn image_vertex_descriptor_preserves_attributes_for_per_instance_step() {
        let per_vertex = ImageVertex::vertex_descriptor(MetalVertexStepFunction::PerVertex);
        let per_instance = ImageVertex::vertex_descriptor(MetalVertexStepFunction::PerInstance);

        assert_eq!(per_instance.attributes, per_vertex.attributes);
        assert_eq!(
            per_instance.layout.stride,
            std::mem::size_of::<ImageVertex>()
        );
        assert_eq!(
            per_instance.layout.step_function,
            MetalVertexStepFunction::PerInstance
        );
    }

    #[test]
    fn bg_image_vertex_descriptor_maps_fields_to_upstream_attributes() {
        let descriptor = BgImageVertex::vertex_descriptor(MetalVertexStepFunction::PerInstance);

        assert_eq!(
            descriptor.attributes,
            vec![
                MetalVertexAttribute {
                    format: MetalVertexFormat::Float,
                    offset: std::mem::offset_of!(BgImageVertex, opacity),
                    buffer_index: 0,
                },
                MetalVertexAttribute {
                    format: MetalVertexFormat::UChar,
                    offset: std::mem::offset_of!(BgImageVertex, info),
                    buffer_index: 0,
                },
            ]
        );
        assert_eq!(
            descriptor.layout,
            MetalVertexLayout {
                stride: std::mem::size_of::<BgImageVertex>(),
                step_function: MetalVertexStepFunction::PerInstance,
            }
        );
    }

    #[test]
    fn metal_vertex_descriptor_readback_matches_cell_text_values() {
        let descriptor = CellTextVertex::vertex_descriptor(MetalVertexStepFunction::PerInstance);
        let objc_descriptor = build_metal_vertex_descriptor(&descriptor);
        let attributes = objc_descriptor.attributes();

        for (index, expected) in descriptor.attributes.iter().enumerate() {
            let actual = unsafe { attributes.objectAtIndexedSubscript(index) };
            assert_eq!(actual.format().0 as u64, expected.format.raw());
            assert_eq!(actual.offset(), expected.offset);
            assert_eq!(actual.bufferIndex(), expected.buffer_index);
        }

        let layouts = objc_descriptor.layouts();
        let layout = unsafe { layouts.objectAtIndexedSubscript(0) };
        assert_eq!(layout.stride(), descriptor.layout.stride);
        assert_eq!(
            layout.stepFunction().0 as u64,
            descriptor.layout.step_function.raw()
        );
    }

    #[test]
    fn enabled_attachment_uses_upstream_premultiplied_alpha_blend() {
        let descriptor = pipeline_attachment_descriptor(MetalPipelineAttachmentOptions {
            pixel_format: MetalPixelFormat::Rgba8Unorm,
            blending_enabled: true,
        });

        assert_eq!(
            descriptor,
            MetalPipelineAttachmentDescriptor {
                pixel_format: MetalPixelFormat::Rgba8Unorm,
                blending_enabled: true,
                blend: Some(MetalBlendDescriptor {
                    rgb_operation: MetalBlendOperation::Add,
                    alpha_operation: MetalBlendOperation::Add,
                    source_rgb_factor: MetalBlendFactor::One,
                    source_alpha_factor: MetalBlendFactor::One,
                    destination_rgb_factor: MetalBlendFactor::OneMinusSourceAlpha,
                    destination_alpha_factor: MetalBlendFactor::OneMinusSourceAlpha,
                }),
            }
        );
    }

    #[test]
    fn disabled_attachment_has_no_blend_descriptor() {
        let descriptor = pipeline_attachment_descriptor(MetalPipelineAttachmentOptions {
            pixel_format: MetalPixelFormat::Bgra8Unorm,
            blending_enabled: false,
        });

        assert_eq!(
            descriptor,
            MetalPipelineAttachmentDescriptor {
                pixel_format: MetalPixelFormat::Bgra8Unorm,
                blending_enabled: false,
                blend: None,
            }
        );
    }

    #[test]
    fn attachment_pixel_formats_pass_through_unchanged() {
        assert_eq!(
            pipeline_attachment_descriptor(MetalPipelineAttachmentOptions {
                pixel_format: MetalPixelFormat::Rgba8Unorm,
                blending_enabled: true,
            })
            .pixel_format,
            MetalPixelFormat::Rgba8Unorm
        );
        assert_eq!(
            pipeline_attachment_descriptor(MetalPipelineAttachmentOptions {
                pixel_format: MetalPixelFormat::Bgra8Unorm,
                blending_enabled: true,
            })
            .pixel_format,
            MetalPixelFormat::Bgra8Unorm
        );
    }

    #[test]
    fn enabled_attachment_descriptor_readback_matches_values() {
        let render_descriptor = MTLRenderPipelineDescriptor::new();
        let attachments = render_descriptor.colorAttachments();
        let attachment = unsafe { attachments.objectAtIndexedSubscript(0) };
        let descriptor = pipeline_attachment_descriptor(MetalPipelineAttachmentOptions {
            pixel_format: MetalPixelFormat::Bgra8Unorm,
            blending_enabled: true,
        });

        apply_pipeline_attachment_descriptor(&attachment, descriptor);

        assert_eq!(
            attachment.pixelFormat().0 as u64,
            MetalPixelFormat::Bgra8Unorm.raw()
        );
        assert!(attachment.isBlendingEnabled());
        assert_eq!(
            attachment.rgbBlendOperation().0 as u64,
            MetalBlendOperation::Add.raw()
        );
        assert_eq!(
            attachment.alphaBlendOperation().0 as u64,
            MetalBlendOperation::Add.raw()
        );
        assert_eq!(
            attachment.sourceRGBBlendFactor().0 as u64,
            MetalBlendFactor::One.raw()
        );
        assert_eq!(
            attachment.sourceAlphaBlendFactor().0 as u64,
            MetalBlendFactor::One.raw()
        );
        assert_eq!(
            attachment.destinationRGBBlendFactor().0 as u64,
            MetalBlendFactor::OneMinusSourceAlpha.raw()
        );
        assert_eq!(
            attachment.destinationAlphaBlendFactor().0 as u64,
            MetalBlendFactor::OneMinusSourceAlpha.raw()
        );
    }

    #[test]
    fn disabled_attachment_descriptor_readback_matches_values() {
        let render_descriptor = MTLRenderPipelineDescriptor::new();
        let attachments = render_descriptor.colorAttachments();
        let attachment = unsafe { attachments.objectAtIndexedSubscript(0) };
        let descriptor = pipeline_attachment_descriptor(MetalPipelineAttachmentOptions {
            pixel_format: MetalPixelFormat::Bgra8Unorm,
            blending_enabled: false,
        });

        apply_pipeline_attachment_descriptor(&attachment, descriptor);

        assert_eq!(
            attachment.pixelFormat().0 as u64,
            MetalPixelFormat::Bgra8Unorm.raw()
        );
        assert!(!attachment.isBlendingEnabled());
    }

    #[test]
    fn standard_pipeline_descriptions_match_upstream_table() {
        assert_eq!(
            STANDARD_PIPELINE_DESCRIPTIONS,
            &[
                MetalStandardPipelineDescription {
                    name: "bg_color",
                    vertex_function: "full_screen_vertex",
                    fragment_function: "bg_color_fragment",
                    vertex_input: MetalPipelineVertexInputKind::None,
                    step_function: MetalVertexStepFunction::PerVertex,
                    blending_enabled: false,
                },
                MetalStandardPipelineDescription {
                    name: "cell_bg",
                    vertex_function: "full_screen_vertex",
                    fragment_function: "cell_bg_fragment",
                    vertex_input: MetalPipelineVertexInputKind::None,
                    step_function: MetalVertexStepFunction::PerVertex,
                    blending_enabled: true,
                },
                MetalStandardPipelineDescription {
                    name: "cell_text",
                    vertex_function: "cell_text_vertex",
                    fragment_function: "cell_text_fragment",
                    vertex_input: MetalPipelineVertexInputKind::CellText,
                    step_function: MetalVertexStepFunction::PerInstance,
                    blending_enabled: true,
                },
                MetalStandardPipelineDescription {
                    name: "image",
                    vertex_function: "image_vertex",
                    fragment_function: "image_fragment",
                    vertex_input: MetalPipelineVertexInputKind::Image,
                    step_function: MetalVertexStepFunction::PerInstance,
                    blending_enabled: true,
                },
                MetalStandardPipelineDescription {
                    name: "bg_image",
                    vertex_function: "bg_image_vertex",
                    fragment_function: "bg_image_fragment",
                    vertex_input: MetalPipelineVertexInputKind::BgImage,
                    step_function: MetalVertexStepFunction::PerInstance,
                    blending_enabled: true,
                },
            ]
        );
    }

    #[test]
    fn standard_pipeline_build_values_compose_descriptors_and_attachments() {
        let values: Vec<_> = STANDARD_PIPELINE_DESCRIPTIONS
            .iter()
            .copied()
            .map(|description| {
                standard_pipeline_build_values(description, MetalPixelFormat::Bgra8Unorm)
            })
            .collect();

        assert_eq!(values.len(), 5);
        assert_eq!(values[0].name, "bg_color");
        assert_eq!(values[0].vertex_descriptor, None);
        assert_eq!(
            values[0].attachment.pixel_format,
            MetalPixelFormat::Bgra8Unorm
        );
        assert!(!values[0].attachment.blending_enabled);
        assert_eq!(values[0].attachment.blend, None);

        assert_eq!(values[1].name, "cell_bg");
        assert_eq!(values[1].vertex_descriptor, None);
        assert!(values[1].attachment.blending_enabled);
        assert!(values[1].attachment.blend.is_some());

        assert_eq!(values[2].name, "cell_text");
        assert_eq!(
            values[2].vertex_input,
            MetalPipelineVertexInputKind::CellText
        );
        assert_eq!(
            values[2].vertex_descriptor,
            Some(CellTextVertex::vertex_descriptor(
                MetalVertexStepFunction::PerInstance
            ))
        );
        assert!(values[2].attachment.blending_enabled);

        assert_eq!(values[3].name, "image");
        assert_eq!(values[3].vertex_input, MetalPipelineVertexInputKind::Image);
        assert_eq!(
            values[3].vertex_descriptor,
            Some(ImageVertex::vertex_descriptor(
                MetalVertexStepFunction::PerInstance
            ))
        );

        assert_eq!(values[4].name, "bg_image");
        assert_eq!(
            values[4].vertex_input,
            MetalPipelineVertexInputKind::BgImage
        );
        assert_eq!(
            values[4].vertex_descriptor,
            Some(BgImageVertex::vertex_descriptor(
                MetalVertexStepFunction::PerInstance
            ))
        );
    }

    #[test]
    fn live_pipeline_creation_accepts_standard_bg_color_values() {
        let device = metal_device();
        let library = test_library(&device);
        let values = standard_values("bg_color", MetalPixelFormat::Bgra8Unorm);

        let pipeline = MetalPipeline::new(MetalPipelineOptions {
            device: &device,
            vertex_library: &library,
            fragment_library: &library,
            values,
        })
        .expect("bg_color pipeline should compile");

        let _ = pipeline.state();
    }

    #[test]
    fn live_pipeline_creation_accepts_standard_image_values() {
        let device = metal_device();
        let library = test_library(&device);
        let values = standard_values("image", MetalPixelFormat::Bgra8Unorm);

        let pipeline = MetalPipeline::new(MetalPipelineOptions {
            device: &device,
            vertex_library: &library,
            fragment_library: &library,
            values,
        })
        .expect("image pipeline should compile");

        let _ = pipeline.state();
    }

    #[test]
    fn live_pipeline_missing_vertex_function_returns_explicit_error() {
        let device = metal_device();
        let library = test_library(&device);
        let mut values = standard_values("bg_color", MetalPixelFormat::Bgra8Unorm);
        values.vertex_function = "missing_vertex_function";

        let error = MetalPipeline::new(MetalPipelineOptions {
            device: &device,
            vertex_library: &library,
            fragment_library: &library,
            values,
        })
        .expect_err("missing vertex function should fail before pipeline creation");

        assert_eq!(
            error,
            MetalPipelineError::MissingVertexFunction("missing_vertex_function")
        );
    }

    #[test]
    fn live_pipeline_missing_fragment_function_returns_explicit_error() {
        let device = metal_device();
        let library = test_library(&device);
        let mut values = standard_values("bg_color", MetalPixelFormat::Bgra8Unorm);
        values.fragment_function = "missing_fragment_function";

        let error = MetalPipeline::new(MetalPipelineOptions {
            device: &device,
            vertex_library: &library,
            fragment_library: &library,
            values,
        })
        .expect_err("missing fragment function should fail before pipeline creation");

        assert_eq!(
            error,
            MetalPipelineError::MissingFragmentFunction("missing_fragment_function")
        );
    }

    #[test]
    fn live_pipeline_creation_failure_returns_error_message() {
        let device = metal_device();
        let library = test_library(&device);
        let mut values = standard_values("bg_color", MetalPixelFormat::Bgra8Unorm);
        values.vertex_function = "image_vertex";

        let error = MetalPipeline::new(MetalPipelineOptions {
            device: &device,
            vertex_library: &library,
            fragment_library: &library,
            values,
        })
        .expect_err("incompatible vertex interface should fail pipeline creation");

        let MetalPipelineError::PipelineCreationFailed(message) = error else {
            panic!("expected pipeline creation error");
        };
        assert!(!message.trim().is_empty());
    }
}
