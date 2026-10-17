#version 330 core

uniform sampler2D sprite_texture; // @ tmu:0

in vec4 vert_color; // Premultiplied alpha. UI uses per-vertex color for tint.
in vec2 tex_coords;

out vec4 frag_color;

void main() {
    // Textures have straight alpha; premultiply the texel to match the vertex color.
    vec4 texel = texture(sprite_texture, tex_coords);
    frag_color = vec4(texel.rgb * texel.a, texel.a) * vert_color;
}
