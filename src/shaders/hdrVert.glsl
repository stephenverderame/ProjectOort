#version 330 core
layout (location = 0) in vec2 pos;
layout (location = 1) in vec2 tex_coords;

uniform mat4 model;
out vec2 f_tex_coords;
void main() {
    gl_Position = model * vec4(pos.x, pos.y, 0.0, 1.0);
    f_tex_coords = tex_coords;
}