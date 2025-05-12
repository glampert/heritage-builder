#version 330 core

uniform vec2 viewport_size;

layout(location = 0) in vec2 in_position;
layout(location = 1) in vec4 in_color;
layout(location = 2) in float in_point_size;

out vec4 color;

void main() {
    color = in_color;

    // Map to normalized clip coordinates:
    // 'in_position' comes in as screen space.
    vec2 ndc = (in_position / viewport_size) * 2.0 - 1.0;

    gl_Position = vec4(ndc, 0.0, 1.0);
    gl_PointSize = in_point_size;
}
