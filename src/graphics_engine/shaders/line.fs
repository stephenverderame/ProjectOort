#version 430 core

in vec4 line_color;
out vec4 frag_color;

void main() {
    frag_color = line_color;
}