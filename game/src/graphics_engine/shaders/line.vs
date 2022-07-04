#version 430 core
layout (location = 0) in vec3 pos;
layout (location = 1) in vec4 start_pos;
layout (location = 2) in vec4 end_pos;
layout (location = 3) in vec4 color;

uniform mat4 viewproj;

out vec4 line_color;

void main() {
    // pos.x is 0 for start pos, 1 for end pos
    vec3 world_pos = pos.x * end_pos.xyz + (1 - pos.x) * start_pos.xyz;

    gl_Position = viewproj * vec4(world_pos, 1.0);
    line_color = color;
}