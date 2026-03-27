// Blit shader: fullscreen triangle that samples the offscreen render target.
// No vertex buffer needed; vertices are generated from vertex_index.

@group(0) @binding(0) var src_texture: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0)       tex_coords:    vec2f,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Fullscreen triangle covering [-1,1] clip space:
    //   vertex 0: (-1, -1)  uv (0, 1)
    //   vertex 1: ( 3, -1)  uv (2, 1)
    //   vertex 2: (-1,  3)  uv (0,-1)
    var out: VertexOutput;
    let x = f32(i32(vertex_index  & 1u)) * 4.0 - 1.0;
    let y = f32(i32(vertex_index >> 1u)) * 4.0 - 1.0;
    out.clip_position = vec4f(x, y, 0.0, 1.0);

    // Map clip coords to UV: x [-1,3] -> [0,2], y [-1,3] -> [1,-1]
    out.tex_coords = vec2f((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    return textureSample(src_texture, src_sampler, in.tex_coords);
}
