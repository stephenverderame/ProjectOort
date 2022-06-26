#version 430 core

in vec2 f_tex_coords;

uniform sampler2D tex;

out vec4 frag_color;

void main() {
    frag_color = texture(tex, f_tex_coords);
}