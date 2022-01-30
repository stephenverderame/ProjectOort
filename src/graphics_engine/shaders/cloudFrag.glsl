#version 430 core

// ray specified in local space
in Ray {
    vec3 origin;
    vec3 dir;
} ray;

const uint march_steps = 100;
uniform sampler3D volume;
uniform vec3 light_dir;

// in local space, so we know our ray will intersect with -1 to 1 box
// compute ray intersections to get the near and far t values
vec2 getNearFarT() {
    vec3 invDir = 1.0 / ray.dir;

    vec3 t_btm = invDir * (vec3(-1) - ray.origin);
    vec3 t_top = invDir * (vec3(1) - ray.origin);

    vec3 t_min = vec3(min(t_btm.x, t_top.x),
        min(t_btm.y, t_top.y), min(t_btm.z, t_top.z));
    vec3 t_max = vec3(max(t_btm.x, t_top.x),
        max(t_btm.y, t_top.y), max(t_btm.z, t_top.z));

    float largestMin = max(t_min.x, max(t_min.y, t_min.z));
    float smallestMax = min(t_max.x, min(t_max.y, t_max.z));

    return vec2(largestMin, smallestMax);
}

float sampleVolume(vec3 v) {
    return texture(volume, v).r;
}

// computes the gradient at `pos` with step sizes in the x, y, and z direction defined by `dir`
vec3 gradient(vec3 pos, vec3 dir) {
    return normalize(vec3(
        sampleVolume(pos + vec3(dir.x, 0, 0)) - sampleVolume(pos - vec3(dir.x, 0, 0)),
        sampleVolume(pos + vec3(0, dir.y, 0)) - sampleVolume(pos - vec3(0, dir.y, 0)),
        sampleVolume(pos + vec3(0, 0, dir.z)) - sampleVolume(pos - vec3(0, 0, dir.z))
    ));
}

vec4 rayMarch(vec3 rayOrigin, vec3 stepDir, float stepDelta, float far) {
    vec3 baseColor = vec3(0.4);
    vec4 acc = vec4(0);
    vec3 lightDir = normalize(light_dir);
    for (uint i = 0; i < march_steps; ++i) {
        vec3 origin = rayOrigin + stepDir * float(i);
        float s = texture(volume, origin).r;
        s = smoothstep(0.12, 0.35, s);

        vec3 grad = gradient(origin, vec3(stepDelta));
        float nDotL = max(0, dot(grad, lightDir));

        acc.rgb += (1.0 - acc.a) * s * baseColor * nDotL;
        acc.a += (1.0 - acc.a) * s * 0.5;

        if (acc.a > 0.95 || stepDelta * float(i) >= far) break;
    }
    return acc;
}

out vec4 frag_color;

void main() {
    //bool throw;
    vec2 near_far = getNearFarT();
    //if (throw) discard;
    vec3 d = 1.0 / (ray.dir * (near_far.y - near_far.x));
    float step_size = min(abs(d.x), min(abs(d.y), abs(d.z))) 
        / float(march_steps);
    vec3 origin = ray.origin + ray.dir * near_far.x;
    vec3 dir = ray.dir * step_size;

    frag_color = rayMarch(origin, dir, step_size, near_far.y);
}