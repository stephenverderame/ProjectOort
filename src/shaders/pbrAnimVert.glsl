#version 430 core
layout (location = 0) in vec3 pos;
layout (location = 1) in vec3 normal;
layout (location = 2) in vec2 tex_coords;
layout (location = 3) in vec3 tangent;
layout (location = 4) in ivec4 bone_ids;
layout (location = 5) in vec4 bone_weights;

#define MAX_BONES_PER_VERTEX 4
#define MAX_BONES 100

out mat3 tbn;
out vec3 frag_pos;
out vec2 f_tex_coords;

uniform mat4 viewproj;
uniform mat4 model;

struct BoneTransformedData {
    vec4 b_position;
    vec3 b_normal;
    vec3 b_tangent;
};

layout(std140) uniform BoneUniform {
    mat4 boneMatrices[MAX_BONES];
};

mat3 calcTbn(vec3 normal, vec3 tangent) {
    vec3 T = normalize(mat3(model) * tangent);
    // truncate out translation or multiply model by vec4 with w = 0
    // so we don't translate the ntangents or normal
    vec3 N = normalize(mat3(model) * normal);
    T = normalize(T - dot(T, N) * N);
    vec3 B = cross(N, T);
    return mat3(T, B, N);
}

BoneTransformedData getPositionNormalTangent() {
    BoneTransformedData res;
    res.b_position = vec4(0.0);
    res.b_normal = vec3(0.0);
    res.b_tangent = vec3(0.0);
    for(int i = 0; i < MAX_BONES_PER_VERTEX; ++i) {
        if (bones_ids[i] == -1 || bone_ids[i] >= MAX_BONES)
            break;
        vec4 local_pos = boneMatrices[bone_ids[i]] * vec4(pos, 1.0);
        vec3 local_norm = mat3(boneMatrices[bone_ids[i]]) * normal;
        vec3 local_tan = mat3(boneMatrices[bone_ids[i]]) * tangent;
        res.b_position += local_pos * bone_weights[i];
        res.b_normal += local_norm * bone_weights[i];
        res.b_tangent += local_tan * bone_weights[i];   
    }
    return res;
}

void main() {
    BoneTransformedData data = getPositionNormalTangent();
    f_tex_coords = tex_coords;
    tbn = calcTbn(data.b_normal, data.b_tangent);
    frag_pos = vec3(model * vec4(data.b_position.xyz, 1.0));
    // bone weights should sum to 1, so b_position.w should be 1 anyway
    // manually do this for sanity

    gl_Position = viewproj * vec4(frag_pos, 1.0);
}