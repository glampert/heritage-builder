#version 330 core

uniform vec2 viewport_size;

layout(location = 0) in vec2 in_position;
layout(location = 1) in vec2 in_tex_coords;
layout(location = 2) in vec4 in_vert_color;

out vec4 vert_color;
out vec2 tex_coords;

void main() {
    vert_color = in_vert_color;
    tex_coords = vec2(in_tex_coords.x, in_tex_coords.y);

    // Map to normalized clip coordinates:
    // 'in_position' comes in as screen space.
    vec2 ndc = vec2(
        (in_position.x / viewport_size.x) * 2.0 - 1.0,
        1.0 - (in_position.y / viewport_size.y) * 2.0); // Origin: top-left corner.

    gl_Position = vec4(ndc, 0.0, 1.0);
}
