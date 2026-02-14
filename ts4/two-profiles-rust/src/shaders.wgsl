// Copyright 2025 TermSurf
// WGSL shaders for the IOSurface receiver.
// Part of Issue 416 Experiment 1: single-pane Rust receiver.

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) texcoord: vec2f,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VertexOutput {
    // Fullscreen quad as triangle strip (4 vertices, no vertex buffer needed).
    var positions = array<vec2f, 4>(
        vec2f(-1.0, -1.0), vec2f(1.0, -1.0),
        vec2f(-1.0,  1.0), vec2f(1.0,  1.0),
    );
    var texcoords = array<vec2f, 4>(
        vec2f(0.0, 1.0), vec2f(1.0, 1.0),
        vec2f(0.0, 0.0), vec2f(1.0, 0.0),
    );
    var out: VertexOutput;
    out.position = vec4f(positions[vid], 0.0, 1.0);
    out.texcoord = texcoords[vid];
    return out;
}

@group(0) @binding(0)
var tex: texture_2d<f32>;
@group(0) @binding(1)
var samp: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    return textureSample(tex, samp, in.texcoord);
}
