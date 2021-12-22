#version 330 core

layout (location = 0) in vec3 pos;

uniform mat4 proj;
uniform mat4 view;

out vec3 tex_coords;

void main() {
    gl_Position = proj * mat4(mat3(view)) * vec4(pos, 1.0);
    tex_coords = pos;
}