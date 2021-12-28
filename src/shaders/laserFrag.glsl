#version 330 core

in vec2 tcoords;
uniform vec3 color;

out vec4 frag_color;
void main() {
    frag_color = vec4(color * 10, 1.0);
}