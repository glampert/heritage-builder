#version 330 core

uniform vec2 viewport_size;

layout(location = 0) in vec2 in_position;
layout(location = 1) in vec2 in_tex_coords;

out vec2 tex_coords;

void main() {
    // Flip UVs here:
    tex_coords = vec2(in_tex_coords.x, 1.0 - in_tex_coords.y);

    // Map to normalized clip coordinates:
    // 'in_position' comes in as screen space.
    vec2 ndc = (in_position / viewport_size) * 2.0 - 1.0;

    gl_Position = vec4(ndc, 0.0, 1.0);
}
