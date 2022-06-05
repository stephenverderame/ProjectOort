#version 430 core
#extension GL_ARB_bindless_texture : require
const int MAX_TEXTURES = 10;

in vec2 f_tex_coords;

uniform uint tex_count;
uniform sampler2D textures[MAX_TEXTURES];

out vec4 frag_color;

const float gamma = 2.2;
const float exposure = 0.5;

vec3 toneMap(vec3 color) {
    color = vec3(1.0) - exp(-color * exposure);
    color = pow(color, vec3(1.0 / gamma));
    return color;
}

float textureLinearize(sampler2D tex, vec2 tex_coords) {
    const float near = 0.1;
    const float far = 200.0;
    float z = texture(tex, tex_coords).r;
    return (2.0 * near) / (far + near - z * (far - near));
}

void main() {
    frag_color = texture(textures[0], f_tex_coords);
    for (uint i = 1; i < tex_count; ++i) {
        frag_color += texture(textures[i], f_tex_coords);
    }
    // tone mapping done automatically by sRGB framebuffer
}