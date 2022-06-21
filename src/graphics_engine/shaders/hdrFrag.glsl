#version 430 core
//#extension GL_ARB_bindless_texture : require
#extension GL_ARB_shader_subroutine : require
const int MAX_TEXTURES = 10;

in vec2 f_tex_coords;

uniform uint tex_count;
uniform sampler2D textures[MAX_TEXTURES];
uniform mat3 models[MAX_TEXTURES];

out vec4 frag_color;

const float gamma = 2.2;
const float exposure = 0.5;

subroutine vec4 fn_blend_t(vec4, vec4);
subroutine uniform fn_blend_t blend_function; 


subroutine(fn_blend_t) vec4 blendAdd(vec4 new, vec4 old) {
    return new + old;
}

subroutine(fn_blend_t) vec4 blendOverlay(vec4 new, vec4 old) {
    return vec4(new.rgb * new.a + old.rgb * (1.0 - new.a), max(new.a, old.a));
}

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
    frag_color = texture(textures[0], (models[0] * vec3(f_tex_coords, 1.0)).xy);
    for (uint i = 1; i < tex_count; ++i) {
        vec2 tex_coords = (models[i] * vec3(f_tex_coords, 1.0)).xy;
        if (tex_coords.x >= 0 && tex_coords.x <= 1.0 
            && tex_coords.y >= 0 && tex_coords.y <= 1)
        {
            frag_color = 
                blend_function(texture(textures[i], tex_coords), frag_color);
        }
    }
    // tone mapping done automatically by sRGB framebuffer
}