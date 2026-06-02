#![allow(dead_code)]
// Pipeline descriptor values are consumed by later renderer slices.

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
}
