use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{MTLDevice, MTLLibrary};

use crate::renderer::metal::api::MetalPixelFormat;
use crate::renderer::metal::buffer::MetalBufferElement;
use crate::renderer::metal::pipeline::{
    standard_pipeline_build_values, MetalPipeline, MetalPipelineError, MetalPipelineOptions,
    MetalStandardPipelineDescription, STANDARD_PIPELINE_DESCRIPTIONS,
};
use crate::renderer::size::{GridSize, Size};

pub(crate) const STANDARD_METAL_SHADER_SOURCE: &str = include_str!("shaders.metal");

#[derive(Debug)]
pub(crate) struct MetalShaderLibrary {
    library: Retained<ProtocolObject<dyn MTLLibrary>>,
}

impl MetalShaderLibrary {
    pub(crate) fn compile(
        device: &ProtocolObject<dyn MTLDevice>,
    ) -> Result<Self, MetalShaderLibraryError> {
        Ok(Self {
            library: compile_source(device, STANDARD_METAL_SHADER_SOURCE)?,
        })
    }

    pub(crate) fn library(&self) -> &ProtocolObject<dyn MTLLibrary> {
        &self.library
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MetalShaderLibraryError {
    CompileFailed(String),
}

fn compile_source(
    device: &ProtocolObject<dyn MTLDevice>,
    source: &str,
) -> Result<Retained<ProtocolObject<dyn MTLLibrary>>, MetalShaderLibraryError> {
    let source = NSString::from_str(source);
    device
        .newLibraryWithSource_options_error(&source, None)
        .map_err(|error| MetalShaderLibraryError::CompileFailed(error.to_string()))
}

#[derive(Debug)]
pub(crate) struct MetalStandardPipelines {
    pub(crate) bg_color: MetalPipeline,
    pub(crate) cell_bg: MetalPipeline,
    pub(crate) cell_text: MetalPipeline,
    pub(crate) image: MetalPipeline,
    pub(crate) bg_image: MetalPipeline,
}

impl MetalStandardPipelines {
    pub(crate) fn new(
        device: &ProtocolObject<dyn MTLDevice>,
        pixel_format: MetalPixelFormat,
    ) -> Result<Self, MetalStandardPipelinesError> {
        let library = MetalShaderLibrary::compile(device)
            .map_err(MetalStandardPipelinesError::ShaderLibrary)?;
        build_from_library(device, library.library(), pixel_format)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MetalStandardPipelinesError {
    ShaderLibrary(MetalShaderLibraryError),
    MissingStandardPipeline(&'static str),
    Pipeline {
        name: &'static str,
        error: MetalPipelineError,
    },
}

fn build_from_library(
    device: &ProtocolObject<dyn MTLDevice>,
    library: &ProtocolObject<dyn MTLLibrary>,
    pixel_format: MetalPixelFormat,
) -> Result<MetalStandardPipelines, MetalStandardPipelinesError> {
    Ok(MetalStandardPipelines {
        bg_color: build_standard_pipeline(device, library, "bg_color", pixel_format)?,
        cell_bg: build_standard_pipeline(device, library, "cell_bg", pixel_format)?,
        cell_text: build_standard_pipeline(device, library, "cell_text", pixel_format)?,
        image: build_standard_pipeline(device, library, "image", pixel_format)?,
        bg_image: build_standard_pipeline(device, library, "bg_image", pixel_format)?,
    })
}

fn build_standard_pipeline(
    device: &ProtocolObject<dyn MTLDevice>,
    library: &ProtocolObject<dyn MTLLibrary>,
    name: &'static str,
    pixel_format: MetalPixelFormat,
) -> Result<MetalPipeline, MetalStandardPipelinesError> {
    let description = standard_pipeline_description(name)
        .ok_or(MetalStandardPipelinesError::MissingStandardPipeline(name))?;
    let values = standard_pipeline_build_values(description, pixel_format);
    MetalPipeline::new(MetalPipelineOptions {
        device,
        vertex_library: library,
        fragment_library: library,
        values,
    })
    .map_err(|error| MetalStandardPipelinesError::Pipeline { name, error })
}

fn standard_pipeline_description(name: &'static str) -> Option<MetalStandardPipelineDescription> {
    STANDARD_PIPELINE_DESCRIPTIONS
        .iter()
        .copied()
        .find(|description| description.name == name)
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C, align(16))]
pub(crate) struct MetalUniforms {
    pub(crate) projection_matrix: [[f32; 4]; 4],
    pub(crate) screen_size: [f32; 2],
    pub(crate) cell_size: [f32; 2],
    pub(crate) grid_size: [u16; 2],
    pub(crate) _padding0: [u8; 12],
    pub(crate) grid_padding: [f32; 4],
    pub(crate) padding_extend: u8,
    pub(crate) _padding1: [u8; 3],
    pub(crate) min_contrast: f32,
    pub(crate) cursor_pos: [u16; 2],
    pub(crate) cursor_color: [u8; 4],
    pub(crate) bg_color: [u8; 4],
    pub(crate) bools: MetalUniformBools,
    pub(crate) _padding2: [u8; 8],
}

/// The 2D orthographic projection matrix (upstream `math.ortho2d`). Maps the
/// `[left, right] × [bottom, top]` rectangle to clip space; the `bottom`/`top`
/// convention yields the negative-Y scale used for terminal coordinates.
pub(crate) fn ortho2d(left: f32, right: f32, bottom: f32, top: f32) -> [[f32; 4]; 4] {
    let w = right - left;
    let h = top - bottom;
    [
        [2.0 / w, 0.0, 0.0, 0.0],
        [0.0, 2.0 / h, 0.0, 0.0],
        [0.0, 0.0, -1.0, 0.0],
        [-(right + left) / w, -(top + bottom) / h, 0.0, 1.0],
    ]
}

impl MetalUniforms {
    /// Update the screen-size-derived uniform fields (upstream
    /// `updateScreenSizeUniforms`): the orthographic `projection_matrix`, the
    /// `grid_padding` (the blank space around the grid), and the `screen_size`.
    /// The other uniform groups (cell/grid size, min-contrast, colors, cursor,
    /// bools) are set by their own updates.
    pub(crate) fn update_screen_size(&mut self, size: Size, grid: GridSize) {
        let terminal = size.terminal();
        let blank = size
            .screen
            .blank_padding(size.padding, grid, size.cell)
            .add(size.padding);
        self.projection_matrix = ortho2d(
            -(size.padding.left as f32),
            (terminal.width + size.padding.right) as f32,
            (terminal.height + size.padding.bottom) as f32,
            -(size.padding.top as f32),
        );
        self.grid_padding = [
            blank.top as f32,
            blank.right as f32,
            blank.bottom as f32,
            blank.left as f32,
        ];
        self.screen_size = [size.screen.width as f32, size.screen.height as f32];
    }

    #[cfg(test)]
    pub(crate) fn test_bg_color(width: u16, height: u16, bg_color: [u8; 4]) -> Self {
        Self::test_with_grid(
            [width, height],
            [width, height],
            [1.0, 1.0],
            [0.0; 4],
            0,
            bg_color,
        )
    }

    #[cfg(test)]
    pub(crate) fn test_with_grid(
        screen_size: [u16; 2],
        grid_size: [u16; 2],
        cell_size: [f32; 2],
        grid_padding: [f32; 4],
        padding_extend: u8,
        bg_color: [u8; 4],
    ) -> Self {
        Self {
            projection_matrix: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            screen_size: [screen_size[0] as f32, screen_size[1] as f32],
            cell_size,
            grid_size,
            _padding0: [0; 12],
            grid_padding,
            padding_extend,
            _padding1: [0; 3],
            min_contrast: 0.0,
            cursor_pos: [0, 0],
            cursor_color: [0, 0, 0, 0],
            bg_color,
            bools: MetalUniformBools {
                cursor_wide: false,
                use_display_p3: true,
                use_linear_blending: false,
                use_linear_correction: false,
            },
            _padding2: [0; 8],
        }
    }
}

unsafe impl MetalBufferElement for MetalUniforms {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct MetalUniformBools {
    pub(crate) cursor_wide: bool,
    pub(crate) use_display_p3: bool,
    pub(crate) use_linear_blending: bool,
    pub(crate) use_linear_correction: bool,
}

#[cfg(test)]
mod tests {
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_foundation::NSString;
    use objc2_metal::{MTLCreateSystemDefaultDevice, MTLDevice, MTLLibrary};

    use super::{
        build_from_library, compile_source, ortho2d, MetalShaderLibrary, MetalShaderLibraryError,
        MetalStandardPipelines, MetalStandardPipelinesError, MetalUniformBools, MetalUniforms,
        STANDARD_METAL_SHADER_SOURCE,
    };
    use crate::renderer::metal::api::MetalPixelFormat;
    use crate::renderer::metal::pipeline::{MetalPipelineError, STANDARD_PIPELINE_DESCRIPTIONS};
    use crate::renderer::size::GridSize;

    const INCOMPATIBLE_STANDARD_SHADER_SOURCE: &str = r#"
#include <metal_stdlib>
using namespace metal;

struct PositionIn {
    float2 position [[attribute(0)]];
};

struct VertexOut {
    float4 position [[position]];
};

vertex VertexOut full_screen_vertex(PositionIn in [[stage_in]]) {
    VertexOut out;
    out.position = float4(in.position, 0.0, 1.0);
    return out;
}

fragment float4 bg_color_fragment() {
    return float4(0.0, 0.0, 0.0, 1.0);
}

fragment float4 cell_bg_fragment() {
    return float4(0.0, 0.0, 0.0, 1.0);
}

struct CellTextIn {
    uint2 glyph_pos [[attribute(0)]];
    uint2 glyph_size [[attribute(1)]];
    short2 bearings [[attribute(2)]];
    ushort2 grid_pos [[attribute(3)]];
    uchar4 color [[attribute(4)]];
    uchar atlas [[attribute(5)]];
    uchar flags [[attribute(6)]];
};

vertex VertexOut cell_text_vertex(CellTextIn in [[stage_in]]) {
    VertexOut out;
    out.position = float4(float2(in.grid_pos), 0.0, 1.0);
    return out;
}

fragment float4 cell_text_fragment() {
    return float4(1.0, 1.0, 1.0, 1.0);
}

struct ImageIn {
    float2 grid_pos [[attribute(0)]];
    float2 cell_offset [[attribute(1)]];
    float4 source_rect [[attribute(2)]];
    float2 dest_size [[attribute(3)]];
};

vertex VertexOut image_vertex(ImageIn in [[stage_in]]) {
    VertexOut out;
    out.position = float4(in.grid_pos + in.cell_offset, 0.0, 1.0);
    return out;
}

fragment float4 image_fragment() {
    return float4(1.0, 1.0, 1.0, 1.0);
}

struct BgImageIn {
    float opacity [[attribute(0)]];
    uchar info [[attribute(1)]];
};

vertex VertexOut bg_image_vertex(BgImageIn in [[stage_in]]) {
    VertexOut out;
    out.position = float4(in.opacity, float(in.info), 0.0, 1.0);
    return out;
}

fragment float4 bg_image_fragment() {
    return float4(1.0, 1.0, 1.0, 1.0);
}
"#;

    fn metal_device() -> Retained<ProtocolObject<dyn MTLDevice>> {
        MTLCreateSystemDefaultDevice().expect("Roastty requires a Metal device")
    }

    fn compile_test_source(
        device: &ProtocolObject<dyn MTLDevice>,
        source: &str,
    ) -> Retained<ProtocolObject<dyn MTLLibrary>> {
        let source = NSString::from_str(source);
        device
            .newLibraryWithSource_options_error(&source, None)
            .expect("test shader source should compile")
    }

    #[test]
    fn standard_shader_source_contains_every_standard_function_name() {
        for description in STANDARD_PIPELINE_DESCRIPTIONS {
            assert!(
                STANDARD_METAL_SHADER_SOURCE.contains(description.vertex_function),
                "missing vertex function {}",
                description.vertex_function
            );
            assert!(
                STANDARD_METAL_SHADER_SOURCE.contains(description.fragment_function),
                "missing fragment function {}",
                description.fragment_function
            );
        }
    }

    #[test]
    fn standard_shader_library_compiles() {
        let device = metal_device();
        let library =
            MetalShaderLibrary::compile(&device).expect("standard shader source should compile");

        let _ = library.library();
    }

    #[test]
    fn standard_shader_library_resolves_every_pipeline_function() {
        let device = metal_device();
        let library =
            MetalShaderLibrary::compile(&device).expect("standard shader source should compile");

        for description in STANDARD_PIPELINE_DESCRIPTIONS {
            let vertex_name = NSString::from_str(description.vertex_function);
            assert!(
                library
                    .library()
                    .newFunctionWithName(&vertex_name)
                    .is_some(),
                "missing vertex function {}",
                description.vertex_function
            );

            let fragment_name = NSString::from_str(description.fragment_function);
            assert!(
                library
                    .library()
                    .newFunctionWithName(&fragment_name)
                    .is_some(),
                "missing fragment function {}",
                description.fragment_function
            );
        }
    }

    #[test]
    fn standard_pipelines_create_all_pipeline_states() {
        let device = metal_device();
        let pipelines = MetalStandardPipelines::new(&device, MetalPixelFormat::Bgra8UnormSrgb)
            .expect("standard pipelines should compile");

        let _ = (
            &pipelines.bg_color,
            &pipelines.cell_bg,
            &pipelines.cell_text,
            &pipelines.image,
            &pipelines.bg_image,
        );
    }

    #[test]
    fn invalid_shader_source_returns_compile_error() {
        let device = metal_device();
        let error = compile_source(&device, "this is not metal source")
            .expect_err("invalid source should fail");

        let MetalShaderLibraryError::CompileFailed(message) = error;
        assert!(!message.trim().is_empty());
    }

    #[test]
    fn metal_uniform_layout_matches_standard_shader_struct() {
        assert_eq!(std::mem::size_of::<MetalUniforms>(), 144);
        assert_eq!(std::mem::align_of::<MetalUniforms>(), 16);
        assert_eq!(std::mem::offset_of!(MetalUniforms, projection_matrix), 0);
        assert_eq!(std::mem::offset_of!(MetalUniforms, screen_size), 64);
        assert_eq!(std::mem::offset_of!(MetalUniforms, cell_size), 72);
        assert_eq!(std::mem::offset_of!(MetalUniforms, grid_size), 80);
        assert_eq!(std::mem::offset_of!(MetalUniforms, _padding0), 84);
        assert_eq!(std::mem::offset_of!(MetalUniforms, grid_padding), 96);
        assert_eq!(std::mem::offset_of!(MetalUniforms, padding_extend), 112);
        assert_eq!(std::mem::offset_of!(MetalUniforms, _padding1), 113);
        assert_eq!(std::mem::offset_of!(MetalUniforms, min_contrast), 116);
        assert_eq!(std::mem::offset_of!(MetalUniforms, cursor_pos), 120);
        assert_eq!(std::mem::offset_of!(MetalUniforms, cursor_color), 124);
        assert_eq!(std::mem::offset_of!(MetalUniforms, bg_color), 128);
        assert_eq!(std::mem::offset_of!(MetalUniforms, bools), 132);
        assert_eq!(std::mem::offset_of!(MetalUniforms, _padding2), 136);

        assert_eq!(std::mem::size_of::<MetalUniformBools>(), 4);
        assert_eq!(std::mem::align_of::<MetalUniformBools>(), 1);
        assert_eq!(std::mem::offset_of!(MetalUniformBools, cursor_wide), 0);
        assert_eq!(std::mem::offset_of!(MetalUniformBools, use_display_p3), 1);
        assert_eq!(
            std::mem::offset_of!(MetalUniformBools, use_linear_blending),
            2
        );
        assert_eq!(
            std::mem::offset_of!(MetalUniformBools, use_linear_correction),
            3
        );
    }

    #[test]
    fn metal_uniform_constructor_initializes_padding_bytes() {
        let uniforms = MetalUniforms::test_bg_color(4, 4, [32, 64, 128, 255]);
        let bytes = unsafe {
            std::slice::from_raw_parts(
                (&uniforms as *const MetalUniforms).cast::<u8>(),
                std::mem::size_of::<MetalUniforms>(),
            )
        };

        assert_eq!(&bytes[84..96], &[0; 12]);
        assert_eq!(&bytes[113..116], &[0; 3]);
        assert_eq!(&bytes[136..144], &[0; 8]);
        assert_eq!(uniforms.bools.cursor_wide as u8, 0);
        assert_eq!(uniforms.bools.use_display_p3 as u8, 1);
        assert_eq!(uniforms.bools.use_linear_blending as u8, 0);
        assert_eq!(uniforms.bools.use_linear_correction as u8, 0);
    }

    #[test]
    fn named_pipeline_failure_preserves_pipeline_name() {
        let device = metal_device();
        let library = compile_test_source(&device, INCOMPATIBLE_STANDARD_SHADER_SOURCE);
        let error = build_from_library(&device, &library, MetalPixelFormat::Bgra8UnormSrgb)
            .expect_err("incompatible full screen vertex input should fail bg_color");

        let MetalStandardPipelinesError::Pipeline { name, error } = error else {
            panic!("expected named pipeline error");
        };
        assert_eq!(name, "bg_color");
        let MetalPipelineError::PipelineCreationFailed(message) = error else {
            panic!("expected pipeline creation error");
        };
        assert!(!message.trim().is_empty());
    }

    #[test]
    fn ortho2d_matches_upstream_matrix() {
        // ortho2d(0, 4, 2, 0): w = 4, h = -2.
        assert_eq!(
            ortho2d(0.0, 4.0, 2.0, 0.0),
            [
                [0.5, 0.0, 0.0, 0.0],
                [0.0, -1.0, 0.0, 0.0],
                [0.0, 0.0, -1.0, 0.0],
                [-1.0, 1.0, 0.0, 1.0],
            ]
        );
    }

    #[test]
    fn update_screen_size_sets_screen_derived_fields_only() {
        use crate::renderer::size::{CellSize, Padding, ScreenSize, Size};

        // Start from a uniforms with distinctive cell/grid/bg values to prove they
        // are untouched.
        let mut uniforms =
            MetalUniforms::test_with_grid([1, 1], [7, 9], [11.0, 13.0], [0.0; 4], 0, [1, 2, 3, 4]);

        let size = Size {
            screen: ScreenSize {
                width: 100,
                height: 80,
            },
            cell: CellSize {
                width: 10,
                height: 20,
            },
            padding: Padding {
                top: 2,
                bottom: 3,
                right: 5,
                left: 4,
            },
        };
        let grid = GridSize {
            columns: 8,
            rows: 3,
        };

        uniforms.update_screen_size(size, grid);

        // terminal = screen - padding = {91, 75}; projection bounds
        // (-left, terminal.width + right, terminal.height + bottom, -top)
        // = (-4, 96, 78, -2).
        assert_eq!(uniforms.projection_matrix, ortho2d(-4.0, 96.0, 78.0, -2.0));

        // blank_padding: grid 80×60, padded 89×65, leftover 11×15 →
        // {top: 0, bottom: 15, right: 11, left: 0}; `.add(padding)` →
        // {top: 2, bottom: 18, right: 16, left: 4}; grid_padding is
        // [top, right, bottom, left].
        assert_eq!(uniforms.grid_padding, [2.0, 16.0, 18.0, 4.0]);

        assert_eq!(uniforms.screen_size, [100.0, 80.0]);

        // The non-screen-size fields are untouched.
        assert_eq!(uniforms.cell_size, [11.0, 13.0]);
        assert_eq!(uniforms.grid_size, [7, 9]);
        assert_eq!(uniforms.bg_color, [1, 2, 3, 4]);
    }
}
