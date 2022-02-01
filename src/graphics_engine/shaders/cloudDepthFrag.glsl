#version 430 core

// ray specified in local space
in Ray {
    vec3 origin;
    vec3 dir;
} ray;

const uint march_steps = 16;
uniform sampler3D volume;
uniform mat4 viewproj;
uniform mat4 model;

const float EPS = 0.00001;

// in local space, so we know our ray will intersect with -1 to 1 box
// compute ray intersections to get the near and far t values
vec2 getNearFarT(vec3 dir, vec3 origin, out bool miss) {
    vec3 invDir = 1.0 / dir;

    // <0, 0, 0> is aabb min extents, <1, 1, 1> is max extents
    vec3 t1 = invDir * (vec3(0) - origin);
    vec3 t2 = invDir * (vec3(1) - origin);

    vec3 t_mins = min(t1, t2);
    vec3 t_maxs = max(t1, t2);

    float near = max(t_mins.x, max(t_mins.y, t_mins.z));
    float far = min(t_maxs.x, min(t_maxs.y, t_maxs.z));

    miss = near - EPS >= far || far < -EPS;
    return vec2(near, far);
}

float densityAt(vec3 pt) {
    float density = texture(volume, pt).r;
    return smoothstep(0.12, 0.35, density);
}

void main() {
    bool miss;
    vec2 near_far = getNearFarT(ray.dir, ray.origin, miss);
    if (miss) discard;
    vec3 origin = ray.origin + max(near_far.x, 0.0) * ray.dir;
    float delta = (near_far.y - max(near_far.x, 0.0)) / float(march_steps);
    float alpha = 0.0;
    const float alpha_threshold = 0.7;
    for (int i = 0; i < march_steps; ++i) {
        vec3 pt = origin + ray.dir * delta * float(i);
        float d = densityAt(pt);

        alpha += (1.0 - alpha) * d * 0.5;
        if (alpha > alpha_threshold) break;
    }
    if (alpha > alpha_threshold) {
        vec3 pt = ray.origin + ray.dir * (max(near_far.x, 0.0) + near_far.y) / 2.0;
        vec4 clipSpacePt = viewproj * model * vec4(pt, 1.0);
        clipSpacePt /= clipSpacePt.w;
        float near = gl_DepthRange.near;
        float far = gl_DepthRange.far;
        gl_FragDepth = (((far - near) * clipSpacePt.z) + near + far) / 2.0;
    } else {
        discard;
    }
}