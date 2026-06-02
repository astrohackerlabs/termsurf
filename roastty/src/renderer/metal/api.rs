#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub(crate) enum MetalPixelFormat {
    Invalid = 0,
    R8Unorm = 10,
    R8UnormSrgb = 11,
    Rgba8Unorm = 70,
    Rgba8UnormSrgb = 71,
    Bgra8Unorm = 80,
    Bgra8UnormSrgb = 81,
}

impl MetalPixelFormat {
    pub(crate) fn raw(self) -> u64 {
        self as u64
    }

    pub(crate) fn to_objc(self) -> objc2_metal::MTLPixelFormat {
        match self {
            MetalPixelFormat::Invalid => objc2_metal::MTLPixelFormat::Invalid,
            MetalPixelFormat::R8Unorm => objc2_metal::MTLPixelFormat::R8Unorm,
            MetalPixelFormat::R8UnormSrgb => objc2_metal::MTLPixelFormat::R8Unorm_sRGB,
            MetalPixelFormat::Rgba8Unorm => objc2_metal::MTLPixelFormat::RGBA8Unorm,
            MetalPixelFormat::Rgba8UnormSrgb => objc2_metal::MTLPixelFormat::RGBA8Unorm_sRGB,
            MetalPixelFormat::Bgra8Unorm => objc2_metal::MTLPixelFormat::BGRA8Unorm,
            MetalPixelFormat::Bgra8UnormSrgb => objc2_metal::MTLPixelFormat::BGRA8Unorm_sRGB,
        }
    }

    pub(crate) fn bytes_per_pixel(self) -> Option<usize> {
        match self {
            MetalPixelFormat::Invalid => None,
            MetalPixelFormat::R8Unorm | MetalPixelFormat::R8UnormSrgb => Some(1),
            MetalPixelFormat::Rgba8Unorm
            | MetalPixelFormat::Rgba8UnormSrgb
            | MetalPixelFormat::Bgra8Unorm
            | MetalPixelFormat::Bgra8UnormSrgb => Some(4),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub(crate) enum MetalCommandBufferStatus {
    Completed = 4,
    Error = 5,
}

impl MetalCommandBufferStatus {
    pub(crate) fn raw(self) -> u64 {
        self as u64
    }

    pub(crate) fn from_objc(status: objc2_metal::MTLCommandBufferStatus) -> Option<Self> {
        match status.0 as u64 {
            4 => Some(Self::Completed),
            5 => Some(Self::Error),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub(crate) enum MetalLoadAction {
    Load = 1,
    Clear = 2,
}

impl MetalLoadAction {
    pub(crate) fn raw(self) -> u64 {
        self as u64
    }

    pub(crate) fn to_objc(self) -> objc2_metal::MTLLoadAction {
        match self {
            MetalLoadAction::Load => objc2_metal::MTLLoadAction::Load,
            MetalLoadAction::Clear => objc2_metal::MTLLoadAction::Clear,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub(crate) enum MetalStoreAction {
    Store = 1,
}

impl MetalStoreAction {
    pub(crate) fn raw(self) -> u64 {
        self as u64
    }

    pub(crate) fn to_objc(self) -> objc2_metal::MTLStoreAction {
        match self {
            MetalStoreAction::Store => objc2_metal::MTLStoreAction::Store,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub(crate) enum MetalPrimitiveType {
    Triangle = 3,
}

impl MetalPrimitiveType {
    pub(crate) fn raw(self) -> u64 {
        self as u64
    }

    pub(crate) fn to_objc(self) -> objc2_metal::MTLPrimitiveType {
        match self {
            MetalPrimitiveType::Triangle => objc2_metal::MTLPrimitiveType::Triangle,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MetalClearColor {
    pub(crate) red: f64,
    pub(crate) green: f64,
    pub(crate) blue: f64,
    pub(crate) alpha: f64,
}

impl MetalClearColor {
    pub(crate) fn to_objc(self) -> objc2_metal::MTLClearColor {
        objc2_metal::MTLClearColor {
            red: self.red,
            green: self.green,
            blue: self.blue,
            alpha: self.alpha,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum MetalCpuCacheMode {
    Default = 0,
    WriteCombined = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum MetalStorageMode {
    Shared = 0,
    Managed = 1,
    Private = 2,
    Memoryless = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum MetalHazardTrackingMode {
    Default = 0,
    Untracked = 1,
    Tracked = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MetalResourceOptions {
    pub(crate) cpu_cache_mode: MetalCpuCacheMode,
    pub(crate) storage_mode: MetalStorageMode,
    pub(crate) hazard_tracking_mode: MetalHazardTrackingMode,
}

impl MetalResourceOptions {
    pub(crate) fn image(storage_mode: MetalStorageMode) -> Self {
        Self {
            cpu_cache_mode: MetalCpuCacheMode::WriteCombined,
            storage_mode,
            hazard_tracking_mode: MetalHazardTrackingMode::Default,
        }
    }

    pub(crate) fn raw(self) -> u64 {
        self.cpu_cache_mode as u64
            | ((self.storage_mode as u64) << 4)
            | ((self.hazard_tracking_mode as u64) << 8)
    }

    pub(crate) fn to_objc(self) -> objc2_metal::MTLResourceOptions {
        objc2_metal::MTLResourceOptions(self.raw() as usize)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MetalTextureUsage {
    pub(crate) shader_read: bool,
    pub(crate) render_target: bool,
}

impl MetalTextureUsage {
    pub(crate) fn shader_read() -> Self {
        Self {
            shader_read: true,
            render_target: false,
        }
    }

    pub(crate) fn raw(self) -> u64 {
        (self.shader_read as u64) | ((self.render_target as u64) << 2)
    }

    pub(crate) fn to_objc(self) -> objc2_metal::MTLTextureUsage {
        objc2_metal::MTLTextureUsage(self.raw() as usize)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub(crate) enum MetalVertexFormat {
    UChar4 = 3,
    Short2 = 16,
    UShort2 = 13,
    Float = 28,
    Float2 = 29,
    Float4 = 31,
    Int = 32,
    Int2 = 33,
    UInt = 36,
    UInt2 = 37,
    UInt4 = 39,
    UChar = 45,
    Char = 46,
}

impl MetalVertexFormat {
    pub(crate) fn raw(self) -> u64 {
        self as u64
    }

    pub(crate) fn to_objc(self) -> objc2_metal::MTLVertexFormat {
        match self {
            MetalVertexFormat::UChar4 => objc2_metal::MTLVertexFormat::UChar4,
            MetalVertexFormat::Short2 => objc2_metal::MTLVertexFormat::Short2,
            MetalVertexFormat::UShort2 => objc2_metal::MTLVertexFormat::UShort2,
            MetalVertexFormat::Float => objc2_metal::MTLVertexFormat::Float,
            MetalVertexFormat::Float2 => objc2_metal::MTLVertexFormat::Float2,
            MetalVertexFormat::Float4 => objc2_metal::MTLVertexFormat::Float4,
            MetalVertexFormat::Int => objc2_metal::MTLVertexFormat::Int,
            MetalVertexFormat::Int2 => objc2_metal::MTLVertexFormat::Int2,
            MetalVertexFormat::UInt => objc2_metal::MTLVertexFormat::UInt,
            MetalVertexFormat::UInt2 => objc2_metal::MTLVertexFormat::UInt2,
            MetalVertexFormat::UInt4 => objc2_metal::MTLVertexFormat::UInt4,
            MetalVertexFormat::UChar => objc2_metal::MTLVertexFormat::UChar,
            MetalVertexFormat::Char => objc2_metal::MTLVertexFormat::Char,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub(crate) enum MetalVertexStepFunction {
    PerVertex = 1,
    PerInstance = 2,
}

impl MetalVertexStepFunction {
    pub(crate) fn raw(self) -> u64 {
        self as u64
    }

    pub(crate) fn to_objc(self) -> objc2_metal::MTLVertexStepFunction {
        match self {
            MetalVertexStepFunction::PerVertex => objc2_metal::MTLVertexStepFunction::PerVertex,
            MetalVertexStepFunction::PerInstance => objc2_metal::MTLVertexStepFunction::PerInstance,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub(crate) enum MetalBlendFactor {
    One = 1,
    OneMinusSourceAlpha = 5,
}

impl MetalBlendFactor {
    pub(crate) fn raw(self) -> u64 {
        self as u64
    }

    pub(crate) fn to_objc(self) -> objc2_metal::MTLBlendFactor {
        match self {
            MetalBlendFactor::One => objc2_metal::MTLBlendFactor::One,
            MetalBlendFactor::OneMinusSourceAlpha => {
                objc2_metal::MTLBlendFactor::OneMinusSourceAlpha
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub(crate) enum MetalBlendOperation {
    Add = 0,
}

impl MetalBlendOperation {
    pub(crate) fn raw(self) -> u64 {
        self as u64
    }

    pub(crate) fn to_objc(self) -> objc2_metal::MTLBlendOperation {
        match self {
            MetalBlendOperation::Add => objc2_metal::MTLBlendOperation::Add,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metal_pixel_format_raw_values_match_upstream() {
        assert_eq!(MetalPixelFormat::Invalid.raw(), 0);
        assert_eq!(MetalPixelFormat::R8Unorm.raw(), 10);
        assert_eq!(MetalPixelFormat::R8UnormSrgb.raw(), 11);
        assert_eq!(MetalPixelFormat::Rgba8Unorm.raw(), 70);
        assert_eq!(MetalPixelFormat::Rgba8UnormSrgb.raw(), 71);
        assert_eq!(MetalPixelFormat::Bgra8Unorm.raw(), 80);
        assert_eq!(MetalPixelFormat::Bgra8UnormSrgb.raw(), 81);
    }

    #[test]
    fn metal_pixel_format_objc_values_match_internal_raw_values() {
        assert_eq!(
            MetalPixelFormat::Invalid.to_objc().0 as u64,
            MetalPixelFormat::Invalid.raw()
        );
        assert_eq!(
            MetalPixelFormat::R8Unorm.to_objc().0 as u64,
            MetalPixelFormat::R8Unorm.raw()
        );
        assert_eq!(
            MetalPixelFormat::R8UnormSrgb.to_objc().0 as u64,
            MetalPixelFormat::R8UnormSrgb.raw()
        );
        assert_eq!(
            MetalPixelFormat::Rgba8Unorm.to_objc().0 as u64,
            MetalPixelFormat::Rgba8Unorm.raw()
        );
        assert_eq!(
            MetalPixelFormat::Rgba8UnormSrgb.to_objc().0 as u64,
            MetalPixelFormat::Rgba8UnormSrgb.raw()
        );
        assert_eq!(
            MetalPixelFormat::Bgra8Unorm.to_objc().0 as u64,
            MetalPixelFormat::Bgra8Unorm.raw()
        );
        assert_eq!(
            MetalPixelFormat::Bgra8UnormSrgb.to_objc().0 as u64,
            MetalPixelFormat::Bgra8UnormSrgb.raw()
        );
    }

    #[test]
    fn metal_resource_options_raw_values_match_packed_layout() {
        assert_eq!(
            MetalResourceOptions {
                cpu_cache_mode: MetalCpuCacheMode::WriteCombined,
                storage_mode: MetalStorageMode::Shared,
                hazard_tracking_mode: MetalHazardTrackingMode::Default,
            }
            .raw(),
            0x1
        );
        assert_eq!(
            MetalResourceOptions {
                cpu_cache_mode: MetalCpuCacheMode::Default,
                storage_mode: MetalStorageMode::Managed,
                hazard_tracking_mode: MetalHazardTrackingMode::Default,
            }
            .raw(),
            0x10
        );
        assert_eq!(
            MetalResourceOptions {
                cpu_cache_mode: MetalCpuCacheMode::Default,
                storage_mode: MetalStorageMode::Private,
                hazard_tracking_mode: MetalHazardTrackingMode::Default,
            }
            .raw(),
            0x20
        );
        assert_eq!(
            MetalResourceOptions {
                cpu_cache_mode: MetalCpuCacheMode::Default,
                storage_mode: MetalStorageMode::Memoryless,
                hazard_tracking_mode: MetalHazardTrackingMode::Default,
            }
            .raw(),
            0x30
        );
        assert_eq!(
            MetalResourceOptions {
                cpu_cache_mode: MetalCpuCacheMode::Default,
                storage_mode: MetalStorageMode::Shared,
                hazard_tracking_mode: MetalHazardTrackingMode::Untracked,
            }
            .raw(),
            0x100
        );
        assert_eq!(
            MetalResourceOptions {
                cpu_cache_mode: MetalCpuCacheMode::Default,
                storage_mode: MetalStorageMode::Shared,
                hazard_tracking_mode: MetalHazardTrackingMode::Tracked,
            }
            .raw(),
            0x200
        );
    }

    #[test]
    fn metal_resource_options_objc_values_match_internal_raw_values() {
        let options = MetalResourceOptions::image(MetalStorageMode::Managed);
        assert_eq!(options.to_objc().0 as u64, options.raw());
    }

    #[test]
    fn metal_texture_usage_raw_values_match_packed_layout() {
        assert_eq!(MetalTextureUsage::shader_read().raw(), 0x1);
        assert_eq!(
            MetalTextureUsage {
                shader_read: false,
                render_target: true,
            }
            .raw(),
            0x4
        );
    }

    #[test]
    fn metal_texture_usage_objc_values_match_internal_raw_values() {
        let usage = MetalTextureUsage::shader_read();
        assert_eq!(usage.to_objc().0 as u64, usage.raw());
    }

    #[test]
    fn metal_pixel_format_bytes_per_pixel_covers_supported_formats() {
        assert_eq!(MetalPixelFormat::Invalid.bytes_per_pixel(), None);
        assert_eq!(MetalPixelFormat::R8Unorm.bytes_per_pixel(), Some(1));
        assert_eq!(MetalPixelFormat::R8UnormSrgb.bytes_per_pixel(), Some(1));
        assert_eq!(MetalPixelFormat::Rgba8Unorm.bytes_per_pixel(), Some(4));
        assert_eq!(MetalPixelFormat::Rgba8UnormSrgb.bytes_per_pixel(), Some(4));
        assert_eq!(MetalPixelFormat::Bgra8Unorm.bytes_per_pixel(), Some(4));
        assert_eq!(MetalPixelFormat::Bgra8UnormSrgb.bytes_per_pixel(), Some(4));
    }

    #[test]
    fn metal_command_buffer_status_values_match_upstream_subset() {
        assert_eq!(MetalCommandBufferStatus::Completed.raw(), 4);
        assert_eq!(MetalCommandBufferStatus::Error.raw(), 5);
        assert_eq!(
            MetalCommandBufferStatus::from_objc(objc2_metal::MTLCommandBufferStatus::Completed),
            Some(MetalCommandBufferStatus::Completed)
        );
        assert_eq!(
            MetalCommandBufferStatus::from_objc(objc2_metal::MTLCommandBufferStatus::Error),
            Some(MetalCommandBufferStatus::Error)
        );
        assert_eq!(
            MetalCommandBufferStatus::from_objc(objc2_metal::MTLCommandBufferStatus::Scheduled),
            None
        );
    }

    #[test]
    fn metal_load_action_values_match_upstream_subset() {
        assert_eq!(MetalLoadAction::Load.raw(), 1);
        assert_eq!(MetalLoadAction::Clear.raw(), 2);
        assert_eq!(
            MetalLoadAction::Load.to_objc().0 as u64,
            MetalLoadAction::Load.raw()
        );
        assert_eq!(
            MetalLoadAction::Clear.to_objc().0 as u64,
            MetalLoadAction::Clear.raw()
        );
    }

    #[test]
    fn metal_store_action_values_match_upstream_subset() {
        assert_eq!(MetalStoreAction::Store.raw(), 1);
        assert_eq!(
            MetalStoreAction::Store.to_objc().0 as u64,
            MetalStoreAction::Store.raw()
        );
    }

    #[test]
    fn metal_primitive_type_values_match_upstream_subset() {
        assert_eq!(MetalPrimitiveType::Triangle.raw(), 3);
        assert_eq!(
            MetalPrimitiveType::Triangle.to_objc().0 as u64,
            MetalPrimitiveType::Triangle.raw()
        );
    }

    #[test]
    fn metal_clear_color_converts_to_objc_values() {
        let color = MetalClearColor {
            red: 0.25,
            green: 0.5,
            blue: 0.75,
            alpha: 1.0,
        };
        let objc = color.to_objc();

        assert_eq!(objc.red, 0.25);
        assert_eq!(objc.green, 0.5);
        assert_eq!(objc.blue, 0.75);
        assert_eq!(objc.alpha, 1.0);
    }

    #[test]
    fn metal_vertex_format_raw_values_match_upstream_subset() {
        assert_eq!(MetalVertexFormat::UChar4.raw(), 3);
        assert_eq!(MetalVertexFormat::UShort2.raw(), 13);
        assert_eq!(MetalVertexFormat::Short2.raw(), 16);
        assert_eq!(MetalVertexFormat::Float.raw(), 28);
        assert_eq!(MetalVertexFormat::Float2.raw(), 29);
        assert_eq!(MetalVertexFormat::Float4.raw(), 31);
        assert_eq!(MetalVertexFormat::Int.raw(), 32);
        assert_eq!(MetalVertexFormat::Int2.raw(), 33);
        assert_eq!(MetalVertexFormat::UInt.raw(), 36);
        assert_eq!(MetalVertexFormat::UInt2.raw(), 37);
        assert_eq!(MetalVertexFormat::UInt4.raw(), 39);
        assert_eq!(MetalVertexFormat::UChar.raw(), 45);
        assert_eq!(MetalVertexFormat::Char.raw(), 46);
    }

    #[test]
    fn metal_vertex_format_objc_values_match_internal_raw_values() {
        let formats = [
            MetalVertexFormat::UChar4,
            MetalVertexFormat::Short2,
            MetalVertexFormat::UShort2,
            MetalVertexFormat::Float,
            MetalVertexFormat::Float2,
            MetalVertexFormat::Float4,
            MetalVertexFormat::Int,
            MetalVertexFormat::Int2,
            MetalVertexFormat::UInt,
            MetalVertexFormat::UInt2,
            MetalVertexFormat::UInt4,
            MetalVertexFormat::UChar,
            MetalVertexFormat::Char,
        ];

        for format in formats {
            assert_eq!(format.to_objc().0 as u64, format.raw());
        }
    }

    #[test]
    fn metal_vertex_step_function_raw_values_match_upstream_subset() {
        assert_eq!(MetalVertexStepFunction::PerVertex.raw(), 1);
        assert_eq!(MetalVertexStepFunction::PerInstance.raw(), 2);
    }

    #[test]
    fn metal_vertex_step_function_objc_values_match_internal_raw_values() {
        assert_eq!(
            MetalVertexStepFunction::PerVertex.to_objc().0 as u64,
            MetalVertexStepFunction::PerVertex.raw()
        );
        assert_eq!(
            MetalVertexStepFunction::PerInstance.to_objc().0 as u64,
            MetalVertexStepFunction::PerInstance.raw()
        );
    }

    #[test]
    fn metal_blend_factor_raw_values_match_upstream_subset() {
        assert_eq!(MetalBlendFactor::One.raw(), 1);
        assert_eq!(MetalBlendFactor::OneMinusSourceAlpha.raw(), 5);
    }

    #[test]
    fn metal_blend_factor_objc_values_match_internal_raw_values() {
        assert_eq!(
            MetalBlendFactor::One.to_objc().0 as u64,
            MetalBlendFactor::One.raw()
        );
        assert_eq!(
            MetalBlendFactor::OneMinusSourceAlpha.to_objc().0 as u64,
            MetalBlendFactor::OneMinusSourceAlpha.raw()
        );
    }

    #[test]
    fn metal_blend_operation_raw_values_match_upstream_subset() {
        assert_eq!(MetalBlendOperation::Add.raw(), 0);
    }

    #[test]
    fn metal_blend_operation_objc_values_match_internal_raw_values() {
        assert_eq!(
            MetalBlendOperation::Add.to_objc().0 as u64,
            MetalBlendOperation::Add.raw()
        );
    }
}
