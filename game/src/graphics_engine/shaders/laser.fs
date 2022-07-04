#version 430 core

in FragData {
    vec2 tex_coords;
    vec3 color;
} f_in;

out vec4 frag_color;
void main() {
    frag_color = vec4(f_in.color * 8, 1.0);
}