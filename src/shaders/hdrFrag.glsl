#version 330 core

in vec2 f_tex_coords;

uniform sampler2D diffuse;

out vec4 frag_color;

void main() {
    frag_color = texture(diffuse, f_tex_coords);

}