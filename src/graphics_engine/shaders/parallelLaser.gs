#version 430 core

layout (std430, binding = 5) readonly buffer ViewMatrices {
    mat4 viewproj_mats[];
};

layout (triangles, invocations = 6) in;
layout (triangle_strip, max_vertices = 3) out;

in FragData {
    vec2 tex_coords;
    vec3 color;
} g_in[];

out FragData {
    vec2 tex_coords;
    vec3 color;
} g_out;

void main() {
    gl_Layer = gl_InvocationID;
    for (int j = 0; j < 3; ++j) {
        g_out.color = g_in[j].color;
        g_out.tex_coords = g_in[j].tex_coords;
        gl_Position = viewproj_mats[gl_InvocationID] * gl_in[j].gl_Position;
        EmitVertex();
    }
    EndPrimitive();
}