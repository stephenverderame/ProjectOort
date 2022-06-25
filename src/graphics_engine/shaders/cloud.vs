#version 430 core
layout (location = 0) in vec3 pos;


out Ray {
    vec3 origin;
    vec3 dir;
} ray;

uniform mat4 model;
uniform vec3 cam_pos;
uniform mat4 viewproj;

void main() {
    vec4 camPosLocal = inverse(model) * vec4(cam_pos, 1.0);
    vec3 pos = (pos + vec3(1.0)) * 0.5;
    ray.dir = normalize(pos - camPosLocal.xyz);
    ray.origin = camPosLocal.xyz;// + vec3(0.5);
    gl_Position = viewproj * model * vec4(pos, 1.0);

}