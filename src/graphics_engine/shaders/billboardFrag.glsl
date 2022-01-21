#version 430 core
in vec2 tcoords;
in vec4 color;

uniform sampler2D tex;

out vec4 frag_color;
void main() {
    frag_color = vec4(1.0, 1.0, 1.0, texture(tex, tcoords).r) * color;
}