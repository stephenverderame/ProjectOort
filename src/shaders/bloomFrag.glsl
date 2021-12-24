#version 330 core

in vec2 f_tex_coords;

uniform sampler2D diffuse;

out vec4 frag_color;

void main() {
    vec3 color = texture(diffuse, f_tex_coords).rgb;
    float brightness = dot(color, vec3(0.216, 0.7152, 0.0722));
    if (brightness > 1) {
        frag_color = vec4(color, 1.0);
    }
    else {
        frag_color = vec4(0.0, 0.0, 0.0, 1.0);
    }
}