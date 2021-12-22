#version 330 core
layout (location = 0) in vec3 pos;
layout (location = 1) in vec3 normal;
layout (location = 2) in vec2 tex_coords;

out vec3 f_normal;
out vec3 frag_pos;
out vec2 f_tex_coords;

uniform mat4 viewproj;
uniform mat4 model;

void main() {
    f_tex_coords = tex_coords;
    f_normal = mat3(model) * normal;
    frag_pos = vec3(model * vec4(pos, 1.0));

    gl_Position = viewproj * vec4(frag_pos, 1.0);
}