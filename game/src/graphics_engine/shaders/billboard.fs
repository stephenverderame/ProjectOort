#version 430 core
in vec2 tcoords;
in vec4 color;
in vec4 particle_pos_cam;
in vec4 frag_pos_cam;
in vec2 screen_coords;
in float radius;

uniform sampler2D tex;
uniform sampler2D cam_depth;
uniform float particle_density;
// TODO: Probably should be a uniform
const float cam_front_plane = 0.1;
const float cam_far_plane = 1000.0;

out vec4 frag_color;

float linearize_depth(float depth) {
    float near = cam_front_plane;
    float far = cam_far_plane;
    float z = depth * 2.0 - 1.0; // to [-1, 1]
    return (2.0 * near * far) / (far + near - z * (far - near));
}

/**
 * Computes the fragment opacity by determining the distance the ray travells through the particle
 * Assumes the particle is spherical and that camera rays are aligned with camera z axis
 * Spherical billboards
 */
float calc_opacity() {
    float d = length(particle_pos_cam.xy - frag_pos_cam.xy);
    if (d < radius) {
        float w = sqrt(radius * radius - d * d); //pythagorean theorem, w^2 + d^2 = radius^2
        // negate z coordinate bc right-hand coordinate system positive is towards camera
        float entry = -particle_pos_cam.z - w; //assume ray is aligned with camera's z axis for simplicity
        float exit = -particle_pos_cam.z + w;
        float opaque_depth = linearize_depth(texture(cam_depth, screen_coords).r);
        float travelled_length = min(exit, opaque_depth) - max(entry, cam_front_plane); 
        return 1.0 - clamp(exp(-particle_density * (1 - d/radius) * travelled_length), 0.0, 1.0);
        // clamp so negative values (corresponding to occluded) become 0 opacity
    } else {
        return 0.0;
    }
}

void main() {
    //float depth = linearize_depth(texture(cam_depth, screen_coords).r) / cam_far_plane;
    //frag_color = vec4(depth, depth, depth, 1.0);
    frag_color = vec4(color.rgb, texture(tex, tcoords).r * calc_opacity());
    //frag_color = vec4(particle_pos_cam.xyz, 1.0);
}