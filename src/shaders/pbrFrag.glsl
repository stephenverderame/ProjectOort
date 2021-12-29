#version 430 core
in vec2 f_tex_coords;
in vec3 frag_pos;
in vec3 f_normal;

out vec4 frag_color;

uniform vec3 cam_pos;
uniform sampler2D albedo_map;
uniform sampler2D normal_map;
uniform sampler2D metallic_map;
uniform sampler2D roughness_map;
uniform sampler2D emission_map;
uniform sampler2D ao_map;
uniform bool use_ao;

uniform samplerCube irradiance_map;
uniform samplerCube prefilter_map;
uniform sampler2D brdf_lut;

const float max_reflection_mips = 4.0; // we use 5 mip maps (0 to 4)

struct LightData {
    vec4 start;
    vec4 end;
};

layout(std430, binding = 0) buffer LightBuffer {
    uint light_num;
    LightData lights[];
    // vec3 always takes up the size of vec4 
    // (buffer-backed blocks padded to 16 bytes)
};

const vec3 light_color = vec3(0.5451, 0, 0.5451);

const float PI = 3.14159265359;

/// Approximates amount of surface microfacets are aligned to the halfway vector
float normalDistribGGX(vec3 norm, vec3 halfway, float roughness, float alphaPrime) {
    // Trowbridge-Reitz
    float a = roughness * roughness;
    float a2 = a * alphaPrime; //a2 * a2 for point lights
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

    vec3 Q1  = dFdx(frag_pos);
    vec3 Q2  = dFdy(frag_pos);
    vec2 st1 = dFdx(f_tex_coords);
    vec2 st2 = dFdy(f_tex_coords);

    vec3 N   = normalize(f_normal);
    vec3 T  = normalize(Q1*st2.t - Q2*st1.t);
    vec3 B  = -normalize(cross(N, T));
    mat3 TBN = mat3(T, B, N);

    return normalize(TBN * tangentNormal);
}

float pointLightAttenutation(vec3 light_pos, vec3 frag_pos) {
    float dist = length(light_pos - frag_pos);
    float attenuation = 1.0 / (dist * dist);
    return attenuation;
}

/// closest point to `pos` on line defined by `lineStart` and `lineEnd`
vec3 closestOnLine(vec3 lineStart, vec3 lineEnd, vec3 pos) {
    vec3 v = lineEnd - lineStart;
    float t = clamp(dot(pos - lineStart, v) / dot(v, v), 0.0, 1.0);
    return v * t + lineStart;
}

/// `R` - reflection vector
vec3 lightDirSphere(vec3 R, vec3 light_pos, float radius, vec3 frag_pos) {
    vec3 oldLightDir = light_pos - frag_pos;
    vec3 centerToRay = dot(R, oldLightDir) * R - oldLightDir;

    vec3 closestPoint = oldLightDir + centerToRay * clamp(radius / length(centerToRay), 0.0, 1.0);
    
    return closestPoint;
}

float sphereAreaLightAttenuation(float dist, float radius, vec3 frag_pos) {
    float f = clamp(1.0 - pow(dist/radius, 4), 0.0, 1.0);
    return (f * f) / (dist * dist + 1.0);

}
vec3 lightDirTube2(vec3 tubeStart, vec3 tubeEnd, vec3 norm, vec3 R, float radius) {
    vec3 center = closestOnLine(tubeStart, tubeEnd, frag_pos);
    return lightDirSphere(R, center, radius, frag_pos);
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

/// Computes the direct radiance from an array of light sources
/// `R` - reflection vector
vec3 directRadiance(vec3 norm, vec3 view_dir, vec3 f0, float roughness, 
    float metallic, vec3 albedo, vec3 R) 
{
    vec3 radiance_out = vec3(0);

    for(int i = 0; i < light_num; ++i) {
        // using point lights, so we know where the light is coming from 
        // so not exactly integrating over total area

        // Area Light Intuition: keep the PBR specular light calculation, but change the light direction
        // we want our light direction vector to be from the closest point on the light mesh to our frag_position
        // so we find the closest point on the mesh to our reflection ray

        LightData light = lights[i];

        const float light_radius = 1.5;
        const float luminance = 10;
        
        vec3 light_dir = lightDirTube(light.start.xyz, light.end.xyz, norm, R, light_radius);
        float dist = max(length(light_dir), 0.00001);

        float attenuation = 1.0 / (dist * dist + 0.3);
        vec3 light_radiance = light_color * attenuation;

        light_dir = normalize(light_dir);
        vec3 halfway = normalize(light_dir + view_dir);

        float areaLightAPrime = clamp(light_radius / (dist * 2.0) + roughness * roughness, 0.0, 1.0);

        vec3 fresnel = fresnelSchlick(f0, view_dir, halfway);
        float ndf = normalDistribGGX(norm, halfway, roughness, areaLightAPrime);
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
    vec3 ambient = (kd * diffuse + specular) * ao;
    vec3 color = ambient + direct_radiance + emission * 4;

    frag_color = vec4(color, 1.0);
}