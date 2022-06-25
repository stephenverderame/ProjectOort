#version 430 core
layout (location = 0) in vec3 pos;
layout (location = 1) in vec2 tex_coords;
layout (location = 2) in vec4 instance_pos_rot;
layout (location = 3) in vec2 instance_scale;
layout (location = 4) in vec4 instance_color;

uniform mat4 view;
uniform mat4 proj;

out vec4 color;
out vec2 tcoords;

out vec4 particle_pos_cam;
out vec4 frag_pos_cam;
out vec2 screen_coords;
out float radius;

mat3 rotateAxisAngle(vec3 axis, float angle)
{
    axis = normalize(axis);
    float s = sin(angle);
    float c = cos(angle);
    float oc = 1.0 - c;
    
    return mat3(oc * axis.x * axis.x + c,           oc * axis.x * axis.y - axis.z * s,  oc * axis.z * axis.x + axis.y * s,
                oc * axis.x * axis.y + axis.z * s,  oc * axis.y * axis.y + c,           oc * axis.y * axis.z - axis.x * s,
                oc * axis.z * axis.x - axis.y * s,  oc * axis.y * axis.z + axis.x * s,  oc * axis.z * axis.z + c         );
}

void main() {
    color = instance_color;
    tcoords = tex_coords;
    float theta = instance_pos_rot.w;
    mat4 vi = inverse(view);
    vec3 cam_z = vi[2].xyz;
    mat3 rot = rotateAxisAngle(cam_z, instance_pos_rot.w);
    vec3 cam_right = rot * vi[0].xyz;
    vec3 cam_up = rot * vi[1].xyz;
    vec3 pos_worldspace = instance_pos_rot.xyz + cam_right * pos.x * instance_scale.x
        + cam_up * pos.y * instance_scale.y;

    particle_pos_cam = view * vec4(instance_pos_rot.xyz, 1.0);
    frag_pos_cam = view * vec4(pos_worldspace, 1.0);
    gl_Position = proj * frag_pos_cam;
    vec3 ndc = gl_Position.xyz / gl_Position.w; //perspective division (-1 to 1 range)
    screen_coords = ndc.xy * 0.5 + 0.5; //convert to 0 to 1 range
    radius = max(length(cam_right), length(cam_up)) * max(instance_scale.x, instance_scale.y);
}