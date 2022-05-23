#version 430 core

// ray specified in local space
in Ray {
    vec3 origin;
    vec3 dir;
} ray;

const uint march_steps = 32;
uniform sampler3D volume;
uniform sampler2D cam_depth;
uniform vec3 light_dir;
uniform mat4 model;
uniform mat4 viewproj;
uniform mat4 view;
uniform mat4 proj;
uniform int tile_num_x;

const float EPS = 0.00001;
const vec3 scattering_coeff = 15 * vec3(0.25, 0.5, 1.0);
const vec3 absorb_coeff = 0 * vec3(0.75, 0.5, 0.0);
const vec3 extinction_coeff = scattering_coeff + absorb_coeff;
const float mie_approx_g = 0.50;
const float mie_approx_k = 1.55 * mie_approx_g - 0.55 
    * mie_approx_g * mie_approx_g * mie_approx_g;
const float PI = 3.14159265358979323846264338327950288;
const vec3 light_lum = vec3(1.0);
const uint MAX_LIGHTS_PER_TILE = 1024;
// TODO: Probably should be a uniform
const float cam_near_plane = 0.1;
const float cam_far_plane = 1000.0;

struct LightData {
    vec3 start;
    float radius;
    vec3 end;
    float luminance;
    vec3 color; 
    uint light_mode;
};

layout(std430, binding = 0) readonly buffer LightBuffer {
    uint light_num;
    LightData lights[];
};

layout(std430, binding = 1) readonly buffer VisibleLightIndices {
    // flattened 2D array of work_groups x visible_lights
    int indices[];
} visibleLightBuffer;

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
    return smoothstep(0.05, 0.85, density);
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

vec3 shadowTransmittance(vec3 pt, uint stepNum, vec3 lightDir) {
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

vec3 inScattering(vec3 pt, uint volShadowSteps, float near) {
    // should do this foreach light
    vec3 ld = normalize(light_dir);
    vec3 lightSum = shadowTransmittance(pt, volShadowSteps, ld) 
        * miePhaseFunction(ld, ray.dir) * light_lum;// * visibilityFromLight(via shadow mapping)
    
    // NEEDS Work
    // Artifacts with Forward+ tiles and volumetrics
    // also this causes an SO when we get too close to volumetric
    vec4 clipSpacePt = viewproj * model * vec4(ray.origin + ray.dir * near, 1.0);
    clipSpacePt /= clipSpacePt.w;
    ivec2 location = ivec2(gl_FragCoord.xy);
    ivec2 tileId = location / ivec2(16, 16);
    uint workGroupIndex = tileId.y * tile_num_x + tileId.x;
    uint offset = workGroupIndex * MAX_LIGHTS_PER_TILE;


    for(int i = 0; i < light_num/*MAX_LIGHTS_PER_TILE && visibleLightBuffer.indices[offset + i] != -1*/; ++i) {

        LightData light = lights[i/*visibleLightBuffer.indices[offset + i]*/];

        vec3 avg_light_pos = (light.start + light.end) / 2.0;
        
        vec3 light_dir = (inverse(model) * vec4(avg_light_pos, 1.0)).xyz - pt;
        float dist = max(length(light_dir), 0.00001);

        if (dist < 2) {
            float attenuation = 1.0 / (dist * dist + 0.3);
            vec3 light_radiance = light.color * attenuation;

            light_dir = normalize(light_dir);
            lightSum += light_radiance * shadowTransmittance(pt, volShadowSteps, light_dir) 
                * light_radiance * miePhaseFunction(light_dir, ray.dir);
        }
        
    }
    return lightSum;
}

float linearize_depth(float depth) {
    float near = cam_near_plane;
    float far = cam_far_plane;
    float z = depth * 2.0 - 1.0; // to [-1, 1]
    return (2.0 * near * far) / (far + near - z * (far - near));
}

/// Returns `true` if `sample_pt` is occluded by an opaque object from the perspective
/// of the player. `false` otherwise
bool is_occluded(vec3 sample_pt) {
    vec4 sample_view_space = view * model * vec4(sample_pt, 1.0);
    vec4 frag_pos = proj * sample_view_space;

    vec2 ndc = frag_pos.xy / frag_pos.w;
    vec2 screen_coords = ndc * 0.5 + 0.5; //convert to 0 to 1 range

    float depth = texture(cam_depth, screen_coords).r;

    return depth < sample_view_space.z;

}

vec4 rayMarch2(vec3 rayOrigin, vec2 near_far) {
    #define FROSTBITE_METHOD
    float delta = (near_far.y - max(near_far.x, 0.0)) / float(march_steps);
    vec3 int_transmittance = vec3(1.0);
    vec3 int_scatter = vec3(0.0);
    vec3 lightDir = normalize(light_dir);
    float acc_alpha = 0.0;
    for (uint i = 0; i < march_steps; ++i) {
        vec3 samplePt = rayOrigin + ray.dir * delta * float(i);
        if (is_occluded(samplePt)) break;
        float d = densityAt(samplePt);
        #ifdef FROSTBITE_METHOD
            vec3 scattering = d * scattering_coeff;
            vec3 extinction = d * extinction_coeff;
            vec3 clampedExtinction = max(extinction, vec3(0.0000001));
            // original paper used extinction below
            vec3 transmittance = exp(-clampedExtinction * delta);

            vec3 luminance = inScattering(samplePt, 32, near_far.x) * scattering;
            vec3 int_scatt = (luminance - luminance * transmittance) / clampedExtinction;

            int_scatter += int_transmittance * int_scatt;
            int_transmittance *= transmittance;
        #endif

        #ifdef SIGGRAPH_COURSE_METHOD
            vec3 scatter_val = scattering_coeff * d;
            vec3 extinction_val = extinction_coeff * d;

            extinction *= exp(-extinction_val * delta);
            vec3 lightColor = shadowTransmittance(samplePt, 32);// * light_lum;
            vec3 ambient = vec3(0.0); // ambient * phase ambient
            vec3 stepScattering = scatter_val * delta * 
                (miePhaseFunction(lightDir, ray.dir) * lightColor + ambient);
            scatter += extinction * stepScattering;
        #endif

        /*scattered += shadow * transmittance * d 
            * scattering_coeff * delta * light_lum;
        transmittance *= exp(-d * extinction_coeff * delta);*/
        /*vec3 S = light_lum * shadow * d * scattering_coeff;
        vec3 extinction = max(vec3(0.0000000001), d * extinction_coeff);
        vec3 exp_delta = exp(-extinction * delta);
        vec3 int_S = (S - S * exp_delta) / extinction;
        scattered += transmittance * int_S;
        transmittance *= exp_delta;*/

        #ifdef SHADERTOY_METHOD
            // Should be same implementation as Frostbite, except they don't use a phase function?
            vec3 luminance = inScattering(samplePt, 32);
            vec3 S = /*L * Lattenuation */ luminance * d * scattering_coeff;
            vec3 sampleExtinction = max(vec3(0.0000000001), d * extinction_coeff);
            vec3 Sint = (S - S * exp(-sampleExtinction * delta)) / sampleExtinction;
            int_scatter += int_transmittance * Sint;

            // Evaluate transmittance to view independentely
            int_transmittance *= exp(-sampleExtinction * delta);
        #endif

        acc_alpha += (1.0 - acc_alpha) * d * 0.1;
        
    }

    return vec4(int_scatter, acc_alpha);
}


out vec4 frag_color;

void main() {
    bool miss;
    vec2 near_far = getNearFarT(ray.dir, ray.origin, miss);
    if (miss) discard;
    vec3 origin = ray.origin + ray.dir * max(near_far.x, 0.0);

    frag_color = rayMarch2(origin, near_far);
}