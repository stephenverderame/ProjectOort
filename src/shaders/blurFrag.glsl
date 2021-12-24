#version 330 core

in vec2 f_tex_coords;

uniform sampler2D diffuse;

uniform bool horizontal_pass;
const float kernel[5] = float[5](
    0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);

out vec4 frag_color;

vec4 blurPass(vec2 stepSize) {
    // blurs entire image in one direction with 9 x 9 guassian kernel
    // implements a separable convolution
    vec2 tex_offset = 1.0 / textureSize(diffuse, 0);
    vec3 result = texture(diffuse, f_tex_coords).rgb * kernel[0];
    for(int i = 1; i < 5; ++i) {
        result += texture(diffuse, f_tex_coords + tex_offset * (stepSize * i)).rgb * kernel[i];
        result += texture(diffuse, f_tex_coords - tex_offset * (stepSize * i)).rgb * kernel[i];
    }
    return vec4(result, 1.0);
}

void main() {
    if (horizontal_pass) {
        frag_color = blurPass(vec2(1.0, 0.0));
    } else {
        frag_color = blurPass(vec2(0.0, 1.0));
    }
}