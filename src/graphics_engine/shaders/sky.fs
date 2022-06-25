#version 430 core

in FragData {
    vec3 tex_coords;
} f_in;

uniform samplerCube skybox;

out vec4 color;

void main() {
    color = texture(skybox, f_in.tex_coords);
}