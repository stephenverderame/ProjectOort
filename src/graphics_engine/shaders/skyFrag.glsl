#version 430 core

in vec3 tex_coords;

uniform samplerCube skybox;

out vec4 color;

void main() {
    color = texture(skybox, tex_coords);
}