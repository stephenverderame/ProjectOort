#version 430 core

in vec2 tcoords;
flat in vec3 color;

out vec4 frag_color;
void main() {
    frag_color = vec4(color * 4, 1.0);
}