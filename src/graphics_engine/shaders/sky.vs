#version 430 core

layout (location = 0) in vec3 pos;

uniform mat4 proj;
uniform mat4 view;

out FragData {
    vec3 tex_coords;
} v_out;

void main() {
    gl_Position = proj * mat4(mat3(view)) * vec4(pos, 1.0);
    v_out.tex_coords = pos;
}