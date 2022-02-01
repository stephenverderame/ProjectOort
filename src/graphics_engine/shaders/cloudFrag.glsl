#version 430 core

// ray specified in local space
in Ray {
    vec3 origin;
    vec3 dir;
} ray;

const uint march_steps = 100;
uniform sampler3D volume;
uniform vec3 light_dir;
const float EPS = 0.00001;
const vec3 scattering_coeff = 25 * vec3(0.25, 0.5, 1.0);
const vec3 absorb_coeff = 0 * vec3(0.75, 0.5, 0.0);
const vec3 extinction_coeff = scattering_coeff + absorb_coeff;
const float mie_approx_g = 0.10;
const float mie_approx_k = 1.55 * mie_approx_g - 0.55 
    * mie_approx_g * mie_approx_g * mie_approx_g;
const float PI = 3.14159265358979323846264338327950288;
const vec3 light_lum = vec3(4.0);

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

float miePhaseFunction(vec3 lightDir, vec3 rayDir) {
    float cos_theta = dot(lightDir, normalize(rayDir));
    // Schlick Approximation to Henyey-Greenstein model of Mie Light Scattering
    return (1 - mie_approx_k * mie_approx_k) /
        (4 * PI * (1 + mie_approx_k * cos_theta) * (1 + mie_approx_k * cos_theta));
}

vec3 transmittance(float xa, float xb) {
    // possible TODO: take density into account
    return exp((xa - xb) * extinction_coeff);
}
/*
vec3 inScattering(vec3 pt, float stepDelta) {
    vec3 lightDir = normalize(light_dir);
    vec3 rayDir = ray.dir;
    const float luminance = 10;
    float phase = miePhaseFunction(lightDir, rayDir);

    // TODO take into account shadow maps

    vec3 acc_extinction = vec3(0);
    float acc_alpha = 0.0;
    vec3 volumetricShadow = vec3(0);
    for (uint i = 0; i < 32; ++i) {
        vec3 samplePt = pt - lightDir * float(i) * stepDelta;
        float d = densityAt(samplePt);
        acc_extinction += stepDelta * float(i) * extinction_coeff * d;
        acc_alpha += (1.0 - acc_alpha) * d * 0.5;
    }

    return phase * exp(-acc_extinction);
}*/

vec3 shadowTransmittance(vec3 pt, uint stepNum) {
    vec3 lightDir = normalize(light_dir);
    bool miss;
    vec2 near_far = getNearFarT(-lightDir, pt, miss);
    // near should be < 0, far should be > 0
    float stepSize = abs(near_far.y) / float(stepNum);
    vec3 shadow = vec3(1.0);
    for (float t = 0; t < near_far.y; t += stepSize) {
        vec3 samplePt = pt - lightDir * t;
        float d = densityAt(samplePt);
        shadow *= exp(-d * extinction_coeff * stepSize);
    }
    return shadow;
}

vec4 rayMarch2(vec3 rayOrigin, vec2 near_far) {
    float delta = (near_far.y - max(near_far.x, 0.0)) / float(march_steps);
    vec3 extinction = vec3(1.0);
    vec3 scatter = vec3(0.0);
    vec3 lightDir = normalize(light_dir);
    float acc_alpha = 0.0;
    for (uint i = 0; i < march_steps; ++i) {
        vec3 samplePt = rayOrigin + ray.dir * delta * float(i);
        float d = densityAt(samplePt);

        vec3 scatter_val = scattering_coeff * d;
        vec3 extinction_val = extinction_coeff * d;

        extinction *= exp(-extinction_val * delta);
        vec3 lightColor = shadowTransmittance(samplePt, 32);// * light_lum;
        vec3 ambient = vec3(0.0); // ambient * phase ambient
        vec3 stepScattering = scatter_val * delta * 
            (miePhaseFunction(lightDir, ray.dir) * lightColor + ambient);
        scatter += extinction * stepScattering;
        /*scattered += shadow * transmittance * d 
            * scattering_coeff * delta * light_lum;
        transmittance *= exp(-d * extinction_coeff * delta);*/
        /*vec3 S = light_lum * shadow * d * scattering_coeff;
        vec3 extinction = max(vec3(0.0000000001), d * extinction_coeff);
        vec3 exp_delta = exp(-extinction * delta);
        vec3 int_S = (S - S * exp_delta) / extinction;
        scattered += transmittance * int_S;
        transmittance *= exp_delta;*/

        acc_alpha += (1.0 - acc_alpha) * d * 0.1;
        
    }

    return vec4(scatter, acc_alpha);
}


out vec4 frag_color;

void main() {
    bool miss;
    vec2 near_far = getNearFarT(ray.dir, ray.origin, miss);
    if (miss) discard;
    vec3 origin = ray.origin + ray.dir * max(near_far.x, 0.0);

    frag_color = rayMarch2(origin, near_far);
}