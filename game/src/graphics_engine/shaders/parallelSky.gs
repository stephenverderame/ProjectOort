#version 430 core

layout (std430, binding = 5) readonly buffer ViewMatrices {
    mat4 viewproj_mats[6];
};

layout (triangles, invocations = 6) in;
layout (triangle_strip, max_vertices = 3) out;

in FragData {
    vec3 tex_coords;
} g_in[];

out FragData {
    vec3 tex_coords;
} g_out;

uniform mat4 proj;

void main() {
    gl_Layer = gl_InvocationID;
    mat4 view = mat4(mat3(inverse(proj) * viewproj_mats[gl_InvocationID]));
    mat4 viewproj = proj * view;
    for (int j = 0; j < 3; ++j) {
        g_out.tex_coords = g_in[j].tex_coords;
        gl_Position = viewproj * vec4(g_in[j].tex_coords, 1.0);
        EmitVertex();
    }
    EndPrimitive();
}