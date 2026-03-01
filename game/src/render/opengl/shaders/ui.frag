#version 330 core

uniform sampler2D sprite_texture; // @ tmu:0

in vec4 vert_color; // UI uses per-vertex color for tint.
in vec2 tex_coords;

out vec4 frag_color;

void main() {
    frag_color = texture(sprite_texture, tex_coords) * vert_color;
}
