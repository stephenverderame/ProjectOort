#version 430 core
#extension GL_ARB_bindless_texture : require
in vec2 f_tex_coords;
in vec3 frag_pos;
in mat3 tbn;

out vec4 frag_color;

uniform vec3 cam_pos;
uniform sampler2D albedo_map;
uniform sampler2D normal_map;
uniform sampler2D metallic_map;
uniform sampler2D roughness_map;
uniform sampler2D emission_map;
uniform sampler2D ao_map;
uniform bool use_ao;

uniform vec3 dir_light_dir;
const float dir_light_near = 0.3;
const float light_size_uv = 0.2;

uniform samplerCube irradiance_map;
uniform samplerCube prefilter_map;
uniform sampler2D brdf_lut;

uniform int tile_num_x;
#define MAX_LIGHTS_PER_TILE 1024

const float max_reflection_mips = 4.0; // we use 5 mip maps (0 to 4)

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
    // vec3 always takes up the size of vec4 
    // (buffer-backed blocks padded to 16 bytes)
};

layout(std430, binding = 1) readonly buffer VisibleLightIndices {
    // flattened 2D array of work_groups x visible_lights
    int indices[];
} visibleLightBuffer;

layout(std140, binding = 2) uniform CascadeUniform {
    vec4 far_planes;
    mat4 viewproj_mats[5];
};

uniform sampler2D cascadeDepthMaps[3];
uniform mat4 view;

const float PI = 3.14159265359;

// Gets the normalized light space (uv) pcss search width
// `lightSize` - size of light in uv coordinates
// `recvDist` - depth of reciever in normalized light space
float pcssSearchWidth(float lightSize, float recvDist) {
    return lightSize * (recvDist - dir_light_near) / recvDist;
} 
// poisson disk samples to randomly sample
// without samples being too close to eachother
// low discrepancy sequence
const vec2 poissonDisk[64] = {
    vec2(-0.613392, 0.617481),
    vec2(0.170019, -0.040254),
    vec2(-0.299417, 0.791925),
    vec2(0.645680, 0.493210),
    vec2(-0.651784, 0.717887),
    vec2(0.421003, 0.027070),
    vec2(-0.817194, -0.271096),
    vec2(-0.705374, -0.668203),
    vec2(0.977050, -0.108615),
    vec2(0.063326, 0.142369),
	vec2(0.203528, 0.214331),
	vec2(-0.667531, 0.326090),
	vec2(-0.098422, -0.295755),
	vec2(-0.885922, 0.215369),
	vec2(0.566637, 0.605213),
	vec2(0.039766, -0.396100),
	vec2(0.751946, 0.453352),
	vec2(0.078707, -0.715323),
	vec2(-0.075838, -0.529344),
	vec2(0.724479, -0.580798),
	vec2(0.222999, -0.215125),
	vec2(-0.467574, -0.405438),
	vec2(-0.248268, -0.814753),
	vec2(0.354411, -0.887570),
	vec2(0.175817, 0.382366),
	vec2(0.487472, -0.063082),
	vec2(-0.084078, 0.898312),
	vec2(0.488876, -0.783441),
	vec2(0.470016, 0.217933),
	vec2(-0.696890, -0.549791),
	vec2(-0.149693, 0.605762),
	vec2(0.034211, 0.979980),
	vec2(0.503098, -0.308878),
	vec2(-0.016205, -0.872921),
	vec2(0.385784, -0.393902),
	vec2(-0.146886, -0.859249),
	vec2(0.643361, 0.164098),
	vec2(0.634388, -0.049471),
	vec2(-0.688894, 0.007843),
	vec2(0.464034, -0.188818),
	vec2(-0.440840, 0.137486),
	vec2(0.364483, 0.511704),
	vec2(0.034028, 0.325968),
	vec2(0.099094, -0.308023),
	vec2(0.693960, -0.366253),
	vec2(0.678884, -0.204688),
	vec2(0.001801, 0.780328),
	vec2(0.145177, -0.898984),
	vec2(0.062655, -0.611866),
	vec2(0.315226, -0.604297),
	vec2(-0.780145, 0.486251),
	vec2(-0.371868, 0.882138),
	vec2(0.200476, 0.494430),
	vec2(-0.494552, -0.711051),
	vec2(0.612476, 0.705252),
	vec2(-0.578845, -0.768792),
	vec2(-0.772454, -0.090976),
	vec2(0.504440, 0.372295),
	vec2(0.155736, 0.065157),
	vec2(0.391522, 0.849605),
	vec2(-0.620106, -0.328104),
	vec2(0.789239, -0.419965),
	vec2(-0.545396, 0.538133),
	vec2(-0.178564, -0.596057),
};

// Gets the average blocker distance
// `shadowCoords` - fragment position in light space uv coordinates
// `searchWidth` - the blocker search radius in UV coordinates
// `depth_map` - depth map to use
// returns (avgBlockerDist, # blockers)
vec2 findBlockerDist(vec3 shadowCoords, float searchWidth, sampler2D depth_map, float bias) {
    int blockers = 0;
    float avgBlockerDistance = 0;
    const int blocker_search_samples = 64;

    for(int i = 0; i < blocker_search_samples; ++i) {
        vec2 rand_offset = poissonDisk[i] * searchWidth;
        vec2 pos = shadowCoords.xy + rand_offset;
        float sample_depth = texture(depth_map, pos).r;
        if (sample_depth < shadowCoords.z - bias) {
            ++blockers;
            avgBlockerDistance += sample_depth;
        }
    }

    return vec2(avgBlockerDistance / float(blockers), blockers);
}
// Performs pcf on the depth map around `shadowCoords`
// does so by randomly sampling positions within `filter_size`
// `shadowCoords` - fragment position in light space uv coordinates
// `filter_size` - size of the pcf kernel radius
// returns the average shadow "boolean". 1 is fully in shadow, 0 is not in shadow
float pcf(vec3 shadowCoords, sampler2D depth_map, float filter_size_uv, float bias) {
    const int pcf_samples = 64;

    float sum = 0;
    for(int i = 0; i < pcf_samples; ++i) {
        vec2 offset = poissonDisk[i] * filter_size_uv;
        vec2 pos = shadowCoords.xy + offset;
        float depth = texture(depth_map, pos).r;
        sum += shadowCoords.z - bias > depth ? 1.0 : 0.0;
    }

    return sum / pcf_samples;
}

int getCascadeIndex() {
    vec4 frag_pos_view = view * vec4(frag_pos, 1.0);
    float depth = abs(frag_pos_view.z);
    for(int i = 0; i < 3; ++i) {
        if (depth < far_planes[i]) return i;
    }
    return -1;
}

sampler2D getCascadeTex(int cIdx) {
    return cascadeDepthMaps[cIdx];
}

vec3 getProjCoords(int casIdx) {
    vec4 light_space = viewproj_mats[casIdx] * vec4(frag_pos, 1.0);
    return (light_space.xyz / light_space.w) * 0.5 + 0.5;
}
// calculates the shadow for frag_pos
// 1 represents fully in shadow and 0 represents fully out of shadow
float calcShadow(vec3 norm) {
    int cascIdx = getCascadeIndex();
    if (cascIdx == -1) return 0;
    vec3 projCoords = getProjCoords(cascIdx);
    //if (projCoords.z > 1) return 0;
    sampler2D depth_map = getCascadeTex(cascIdx);
    vec3 lightDir = normalize(dir_light_dir);
    float bias = max(0.05 * (1.0 - dot(norm, lightDir)), 0.005) * (1.0 / (far_planes[cascIdx] * 0.1));

    float adj_light_size = light_size_uv;// / far_planes[0] * far_planes[cascIdx];
    float searchWidth = pcssSearchWidth(adj_light_size, projCoords.z) / far_planes[cascIdx] * far_planes[0];
    vec2 blockers = findBlockerDist(projCoords, searchWidth, depth_map, bias);
    if (blockers.y < 1.0) return 0;
    float avgBlockerDist = blockers.x; //uv coordinates
    //if (projCoords.z > 1.0) return 0;

    float penumbraWidth = (projCoords.z - avgBlockerDist) * adj_light_size / avgBlockerDist;
    float pcfRadius = penumbraWidth * dir_light_near / projCoords.z;

    return pcf(projCoords, depth_map, pcfRadius, bias);
    
    //return projCoords.z - bias > texture(depth_map, projCoords.xy).r ? 1.0 : 0.0;


}

/// Approximates amount of surface microfacets are aligned to the halfway vector
float normalDistribGGX(vec3 norm, vec3 halfway, float roughness, float alphaPrime) {
    // Trowbridge-Reitz
    float a = roughness * roughness;
    float a2 = a * alphaPrime; //a * a for point lights
    float n_dot_h = max(dot(norm, halfway), 0.0);
    float n_dot_h_2 = n_dot_h * n_dot_h;

    float num = a2;
    float denom = (n_dot_h_2 * (num - 1.0) + 1.0);

    return num / (PI * denom * denom);
}

float geometrySchlickGGX(float n_dot_v, float roughness) {
    float r = roughness + 1.0;
    float k = (r*r) / 8.0;

    return n_dot_v / (n_dot_v * (1.0 - k) + k);
}

/// Approximates self-shadowing property of microfacets
float geometrySmith(vec3 norm, vec3 view_dir, vec3 light_dir, float roughness) {
    float n_dot_v = max(dot(norm, view_dir), 0.0);
    float n_dot_l = max(dot(norm, light_dir), 0.0);
    float ggx2 = geometrySchlickGGX(n_dot_v, roughness);
    float ggx1 = geometrySchlickGGX(n_dot_l, roughness);

    return ggx2 * ggx1;
}

vec3 getF0(vec3 albedo, float metallic) {
    // non-metallic surfaces have f0 of 0.04
    // metallic surfaces take this from albedo color
    return mix(vec3(0.04), albedo, metallic);
}

/// Mixes light reflection and refraction
/// `view_dir` - normalized view direction
/// `norm` - normalized normal vector
vec3 fresnelSchlick(vec3 f0, vec3 light_dir, vec3 halfway) {
    float cos_theta = max(dot(light_dir, halfway), 0.0);
    // f0 is surface reflectance at zero incidence
    // (how much the surface reflects when looking directly at it)
    return f0 + (1.0 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

/// `view_dir` - normalized view direction
/// `norm` - normalized normal vector
vec3 fresnelSchlickRoughness(vec3 f0, vec3 view_dir, vec3 norm, float roughness) {
    float cos_theta = max(dot(view_dir, norm), 0.0);
    // fresnel but with added roughness parameter for irradiance
    return f0 + (max(vec3(1.0 - roughness), f0) - f0) 
        * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

vec3 getNormal() {
    vec3 tangentNormal = texture(normal_map, f_tex_coords).xyz * 2.0 - 1.0;
    return normalize(tbn * tangentNormal);
}

/// `R` - reflection vector
vec3 lightDirSphere(vec3 R, vec3 light_pos, float radius) {
    vec3 oldLightDir = light_pos - frag_pos;
    vec3 centerToRay = dot(R, oldLightDir) * R - oldLightDir;

    vec3 closestPoint = oldLightDir + centerToRay * clamp(radius / length(centerToRay), 0.0, 1.0);
    
    return closestPoint;
}


/// Gets the unnormalized view direction from the closest point on the tube to the frag position
vec3 lightDirTube(vec3 tubeStart, vec3 tubeEnd, vec3 norm, vec3 R, float radius) {
    vec3 L0 = tubeStart - frag_pos;
    vec3 L1 = tubeEnd - frag_pos;

    float distL0 = length(L0);
    float distL1 = length(L1);
    float nL0 = dot(L0, norm) / (2.0 * distL0);
    float nL1 = dot(L1, norm) / (2.0 * distL1);
    float nL = (2.0 * clamp(nL0 + nL1, 0.0, 1.0)) /
        (distL0 * distL1 + dot(L0, L1) + 2.0);
    
    vec3 line = tubeEnd - tubeStart;

    float rLd = dot(R, line);
    float distLd = length(line);

    float t = (dot(R, L0) * rLd - dot(L0, line)) / (distLd * distLd - rLd * rLd);

    vec3 closestPoint = L0 + line * clamp(t, 0.0, 1.0);
    vec3 centerToRay = dot(closestPoint, R) * R - closestPoint;
    closestPoint = closestPoint + centerToRay * clamp(radius / length(centerToRay), 0.0, 1.0);
    return closestPoint;
}
/// Returns the unnormalized light direction based on the type of light
vec3 lightDir(LightData light, vec3 R, vec3 norm) {
    switch (light.light_mode) {
        case 0:
            return lightDirSphere(R, light.start, light.radius);
        case 1:
            return lightDirTube(light.start, light.end, norm, R, light.radius);
        case 2: //point light
            return light.start - frag_pos;
        default:
            return vec3(0);
    }
}

float lightAPrime(LightData light, float roughness, float dist) {
    switch (light.light_mode) {
        case 0:
        case 1:
            return clamp(light.radius / (dist * 2.0) + roughness * roughness, 0.0, 1.0);
        default:
            return roughness * roughness;
    }
}

/// Computes the direct radiance from an array of light sources
/// `R` - reflection vector
vec3 directRadiance(vec3 norm, vec3 view_dir, vec3 f0, float roughness, 
    float metallic, vec3 albedo, vec3 R) 
{
    ivec2 location = ivec2(gl_FragCoord.xy);
    ivec2 tileId = location / ivec2(16, 16);
    uint workGroupIndex = tileId.y * tile_num_x + tileId.x;
    uint offset = workGroupIndex * MAX_LIGHTS_PER_TILE;

    vec3 radiance_out = vec3(0);

    for(int i = 0; i < MAX_LIGHTS_PER_TILE && visibleLightBuffer.indices[offset + i] != -1; ++i) {
        // using point lights, so we know where the light is coming from 
        // so not exactly integrating over total area

        // Area Light Intuition: keep the PBR specular light calculation, but change the light direction
        // we want our light direction vector to be from the closest point on the light mesh to our frag_position
        // so we find the closest point on the mesh to our reflection ray

        LightData light = lights[visibleLightBuffer.indices[offset + i]];

        float light_radius = light.radius;
        float luminance = light.luminance;
        
        vec3 light_dir = lightDir(light, R, norm);
        float dist = max(length(light_dir), 0.00001);

        float attenuation = 1.0 / (dist * dist + 0.3);
        vec3 light_radiance = light.color * attenuation;

        light_dir = normalize(light_dir);
        vec3 halfway = normalize(light_dir + view_dir);

        float aPrime = lightAPrime(light, roughness, dist);

        vec3 fresnel = fresnelSchlick(f0, view_dir, halfway);
        float ndf = normalDistribGGX(norm, halfway, roughness, aPrime);
        float g = geometrySmith(norm, view_dir, light_dir, roughness);

        // cook-torrence brdf
        // approximates how much each individual light ray contributes
        // to final reflected light of an opaque surface
        float n_dot_l = max(dot(norm, light_dir), 0.0);
        float brdfDenom = 4.0 * max(dot(norm, view_dir), 0.0) * 
            n_dot_l + 0.0001;
        // add small factor to prevent divide by 0
        vec3 specular = (ndf * g * fresnel) / brdfDenom;

        vec3 ks = fresnel; // specular factor
        vec3 kd = vec3(1.0) - ks; // 1 - ks to conserve energy
        kd *= 1.0 - metallic; //metallic surfaces don't have diffuse reflections

        radiance_out += (kd * albedo / PI + specular) * light_radiance * n_dot_l * luminance;

    }

    return radiance_out;
}
/// Applies a fog effect to frag_color by mixing it with a constant fog color 
/// based on distance of fragment to viewer
vec3 applyFog(vec3 frag_color) {
    float depth = abs((view * vec4(frag_pos, 1.0)).z);
    const vec3 fog_color = vec3(0.4);
    const float fog_density = 0.001;
    float fog_factor = 1.0 / exp(depth * fog_density * depth * fog_density);
    fog_factor = clamp(fog_factor, 0.0, 1.0);
    return mix(fog_color, frag_color, fog_factor);

}

void main() {
    vec3 albedo = texture(albedo_map, f_tex_coords).rgb; // load textures using SRGB so no need to gamma correct
    vec3 emission = texture(emission_map, f_tex_coords).rgb;
    float metallic = texture(metallic_map, f_tex_coords).r;
    float roughness = texture(roughness_map, f_tex_coords).r;
    vec3 ao = use_ao ? texture(ao_map, f_tex_coords).rgb : vec3(0.7);

    vec3 norm = normalize(getNormal());
    vec3 view_dir = normalize(cam_pos - frag_pos);
    vec3 ref = reflect(-view_dir, norm);

    vec3 prefilter_color = textureLod(prefilter_map, ref, roughness * max_reflection_mips).rgb;

    vec3 f0 = getF0(albedo, metallic);

    vec3 direct_radiance = directRadiance(norm, view_dir, f0, roughness, 
        metallic, albedo, ref);

    vec3 ks = fresnelSchlickRoughness(f0, view_dir, norm, roughness);
    vec2 env_brdf = texture(brdf_lut, vec2(max(dot(norm, view_dir), 0.0), roughness)).rg;
    vec3 kd = 1.0 - ks;
    kd *= 1.0 - metallic;

    vec3 irradiance = texture(irradiance_map, norm).rgb;
    // irradiance map is precomputed integral of light intensity over hemisphere
    vec3 diffuse = irradiance * albedo;
    vec3 specular = prefilter_color * (ks * env_brdf.x + env_brdf.y);
    vec3 ambient = (kd * diffuse + specular) * ao * (1.0 - calcShadow(norm) * 0.7);
    vec3 color = ambient + direct_radiance + emission * 4;

    frag_color = vec4(applyFog(color), 1.0);
}