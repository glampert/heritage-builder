#version 330 core

uniform vec4 sprite_tint;
uniform sampler2D sprite_texture; // @ tmu:0

in vec2 tex_coords;

out vec4 frag_color;

void main() {
    frag_color = texture(sprite_texture, tex_coords) * sprite_tint;
}
