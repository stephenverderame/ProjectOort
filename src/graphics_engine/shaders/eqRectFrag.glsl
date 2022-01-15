#version 430 core
in vec3 tex_coords;
out vec4 frag_color;

uniform sampler2D equirectangular_map;

const vec2 invAtan = vec2(0.1591, 0.3183);

vec2 sampleSphericalMap(vec3 v) {
    return vec2(atan(v.z, v.x), asin(v.y)) * invAtan + 0.5;
}

void main() {
    vec2 uv = sampleSphericalMap(normalize(tex_coords));
    vec3 envColor = texture(equirectangular_map, uv).rgb;
  
    frag_color = vec4(envColor, 1.0);
}