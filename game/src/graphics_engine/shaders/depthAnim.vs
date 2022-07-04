#version 430 core
layout (location = 0) in vec3 pos;
layout (location = 1) in ivec4 bone_ids;
layout (location = 2) in vec4 bone_weights;

#define MAX_BONES_PER_VERTEX 4

uniform mat4 viewproj;
uniform mat4 model;

layout(std430, binding = 4) readonly buffer BoneBuffer {
    mat4 boneMatrices[];
};

vec3 calcPos() {
    vec4 res = vec4(0.0);
    for(int i = 0; i < MAX_BONES_PER_VERTEX; ++i) {
        if (bone_ids[i] == -1)
            break;
        res += bone_weights[i] * 
            boneMatrices[bone_ids[i]] * vec4(pos, 1.0);
    }
    return res.xyz;
}

void main() {
    gl_Position = viewproj * model * vec4(calcPos(), 1.0);
}