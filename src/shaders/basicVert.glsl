#version 430 core
layout (location = 0) in vec3 pos;
layout (location = 1) in vec3 normal;
layout (location = 2) in vec2 tex_coords;

uniform mat4 model;
uniform mat4 viewproj;

out vec2 tcoords;
void main() {
    gl_Position = viewproj * model * vec4(pos, 1.0);
    tcoords = tex_coords;
}