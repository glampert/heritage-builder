// UI (ImGui) shader: textured with per-vertex color tint.

struct Uniforms {
    viewport_size: vec2f,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(1) @binding(0) var sprite_texture: texture_2d<f32>;
@group(1) @binding(1) var sprite_sampler: sampler;

struct VertexInput {
    @location(0) position:   vec2f,
    @location(1) tex_coords: vec2f,
    @location(2) color:      vec4f, // RGBA normalized from u8x4.
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0)       tex_coords:    vec2f,
    @location(1)       color:         vec4f,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // ImGui UVs are already correct (no V-flip).
    out.tex_coords = in.tex_coords;
    out.color = in.color;

    let ndc = vec2f(
        (in.position.x / uniforms.viewport_size.x) * 2.0 - 1.0,
        1.0 - (in.position.y / uniforms.viewport_size.y) * 2.0,
    );
    out.clip_position = vec4f(ndc, 0.0, 1.0);

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    return textureSample(sprite_texture, sprite_sampler, in.tex_coords) * in.color;
}
