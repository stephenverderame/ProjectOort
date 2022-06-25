#version 430 core
layout (location = 0) in vec3 pos;

uniform mat4 viewproj;
uniform mat4 model;

void main() {
    gl_Position = viewproj * model * vec4(pos, 1.0);
}