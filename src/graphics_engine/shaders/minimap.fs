#version 430 core

in FragData {
    vec2 tex_coords;
    flat vec4 color;
    flat uint tex_idx;
} fragData;

uniform sampler2D textures[3];

out vec4 frag_color;

void main() {
    frag_color = texture(textures[fragData.tex_idx], fragData.tex_coords).r * 
        fragData.color;
}