#![allow(dead_code)]
// This Metal value layer is consumed by later renderer slices.

pub(crate) mod api;
pub(crate) mod buffer;
pub(crate) mod frame;
pub(crate) mod iosurface_layer;
pub(crate) mod pipeline;
pub(crate) mod render_pass;
pub(crate) mod sampler;
pub(crate) mod shaders;
pub(crate) mod target;
pub(crate) mod texture;

use crate::renderer::shadertoy::Target;

/// The custom-shader target for the Metal renderer (upstream `Metal.zig`'s
/// `custom_shader_target`): Metal cross-compiles custom shaders to MSL.
pub(crate) const CUSTOM_SHADER_TARGET: Target = Target::Msl;

#[cfg(test)]
mod tests {
    use super::CUSTOM_SHADER_TARGET;
    use crate::renderer::shadertoy::Target;

    #[test]
    fn custom_shader_target_is_msl() {
        assert_eq!(CUSTOM_SHADER_TARGET, Target::Msl);
    }
}
