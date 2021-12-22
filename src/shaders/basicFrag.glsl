#version 330 core

in vec2 tcoords;
uniform sampler2D diffuse_tex;

out vec4 color;
void main() {
    color = texture(diffuse_tex, tcoords);
}