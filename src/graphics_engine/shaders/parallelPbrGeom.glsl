#version 430 core

layout (std430, binding = 5) readonly buffer ViewMatrices {
    mat4 viewproj_mats[];
};

layout (triangles, invocations = 6) in;
layout (triangle_strip, max_vertices = 18) out;

in FragData {
    vec2 tex_coords;
    vec3 frag_pos;
    mat3 tbn;
} g_in[];

out FragData {
    vec2 tex_coords;
    vec3 frag_pos;
    mat3 tbn;
} g_out;

void main() {
    for (int j = 0; j < 3; ++j) {
        g_out.tbn = g_in[j].tbn;
        g_out.tex_coords = g_in[j].tex_coords;
        g_out.frag_pos = gl_in[j].gl_Position.xyz;
        gl_Position = viewproj_mats[gl_InvocationID] * gl_in[j].gl_Position;
        EmitVertex();
    }
    EndPrimitive();
}