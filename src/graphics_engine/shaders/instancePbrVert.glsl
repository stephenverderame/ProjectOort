#version 430 core
layout (location = 0) in vec3 pos;
layout (location = 1) in vec3 normal;
layout (location = 2) in vec2 tex_coords;
layout (location = 3) in vec3 tangent;
layout (location = 4) in vec4 instance_model_col0;
layout (location = 5) in vec4 instance_model_col1;
layout (location = 6) in vec4 instance_model_col2;
layout (location = 7) in vec4 instance_model_col3;

out mat3 tbn;
out vec3 frag_pos;
out vec2 f_tex_coords;

uniform mat4 viewproj;

mat3 calcTbn(mat4 model) {
    vec3 T = normalize(mat3(model) * tangent);
    vec3 N = normalize(mat3(model) * normal);
    T = normalize(T - dot(T, N) * N);
    vec3 B = cross(N, T);
    // hit with Grahm Schmidt to re-orthogonalize
    return mat3(T, B, N);
}

void main() {
    mat4 model = mat4(instance_model_col0, instance_model_col1, 
        instance_model_col2, instance_model_col3);
    f_tex_coords = tex_coords;
    tbn = calcTbn(model);
    frag_pos = vec3(model * vec4(pos, 1.0));

    gl_Position = viewproj * vec4(frag_pos, 1.0);
}