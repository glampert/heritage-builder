// Sprites shader: textured + tinted quads.
// Vertex color carries the per-sprite tint.

struct Uniforms {
    viewport_size: vec2f,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(1) @binding(0) var sprite_texture: texture_2d<f32>;
@group(1) @binding(1) var sprite_sampler: sampler;

struct VertexInput {
    @location(0) position:   vec2f,
    @location(1) tex_coords: vec2f,
    @location(2) color:      vec4f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0)       tex_coords:    vec2f,
    @location(1)       color:         vec4f,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Flip V coordinate (image origin is top-left, texture origin is bottom-left).
    out.tex_coords = vec2f(in.tex_coords.x, 1.0 - in.tex_coords.y);
    out.color = in.color;

    // Screen-space to NDC. Origin: top-left corner.
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
