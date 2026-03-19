// Lines shader: per-vertex colored lines (debug drawing).

struct Uniforms {
    viewport_size: vec2f,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec2f,
    @location(1) color:    vec4f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0)       color:         vec4f,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
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
    return in.color;
}
