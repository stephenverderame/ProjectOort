#version 430 core
in vec2 tcoords;
in vec4 color;

uniform sampler2D tex;

out vec4 frag_color;
void main() {
    frag_color = texture(tex, tcoords) * color;
}