#version 430 core
#extension GL_ARB_bindless_texture : require

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

float textureLinearize(sampler2D tex, vec2 tex_coords) {
    const float near = 0.1;
    const float far = 200.0;
    float z = texture(tex, tex_coords).r;
    return (2.0 * near) / (far + near - z * (far - near));
}

void main() {
    vec3 color = texture(diffuse, f_tex_coords).rgb;
    if (do_blend) 
        color += texture(bloom_tex, f_tex_coords).rgb;
    //color = vec3(textureLinearize(cascade1, f_tex_coords));
    //color = vec3(texture(cascade2, f_tex_coords).r);
    //color = vec3(far_planes[2], 0, 0);
    frag_color = vec4(color, 1.0);
    // tone mapping done automatically by sRGB framebuffer
}