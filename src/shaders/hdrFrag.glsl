#version 430 core

in vec2 f_tex_coords;

uniform sampler2D diffuse;
uniform sampler2D bloom_tex;

uniform bool do_blend;

out vec4 frag_color;

const float gamma = 2.2;
const float exposure = 0.5;

vec3 toneMap(vec3 color) {
    color = vec3(1.0) - exp(-color * exposure);
    color = pow(color, vec3(1.0 / gamma));
    return color;
}

void main() {
    vec3 color = texture(diffuse, f_tex_coords).rgb;
    if (do_blend) 
        color += texture(bloom_tex, f_tex_coords).rgb;
    frag_color = vec4(color, 1.0);
    // tone mapping done automatically by sRGB framebuffer
}