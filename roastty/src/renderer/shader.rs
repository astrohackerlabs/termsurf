#![allow(dead_code)]
// Shader input value types are consumed by later renderer slices.

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub(crate) struct ImageVertex {
    pub(crate) grid_pos: [f32; 2],
    pub(crate) cell_offset: [f32; 2],
    pub(crate) source_rect: [f32; 4],
    pub(crate) dest_size: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrimitiveType {
    TriangleStrip,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ImageDrawCall {
    pub(crate) vertex: ImageVertex,
    pub(crate) primitive: PrimitiveType,
    pub(crate) vertex_count: u32,
    pub(crate) instance_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_vertex_layout_matches_upstream_shader_parameter() {
        assert_eq!(std::mem::size_of::<ImageVertex>(), 40);
        assert_eq!(std::mem::align_of::<ImageVertex>(), 4);
    }
}
