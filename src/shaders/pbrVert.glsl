#version 430 core
layout (location = 0) in vec3 pos;
layout (location = 1) in vec3 normal;
layout (location = 2) in vec2 tex_coords;
layout (location = 3) in vec3 tangent;

out mat3 tbn;
out vec3 frag_pos;
out vec2 f_tex_coords;

uniform mat4 viewproj;
uniform mat4 model;

mat3 calcTbn() {
    vec3 T = normalize(vec3(model * vec4(tangent, 0.0)));
    vec3 N = normalize(vec3(model * vec4(normal, 0.0)));
    T = normalize(T - dot(T, N) * N);
    vec3 B = cross(N, T);
    // hit with Grahm Schmidt to re-orthogonalize
    return mat3(T, B, N);
}

void main() {
    f_tex_coords = tex_coords;
    tbn = calcTbn();
    frag_pos = vec3(model * vec4(pos, 1.0));

    gl_Position = viewproj * vec4(frag_pos, 1.0);
}