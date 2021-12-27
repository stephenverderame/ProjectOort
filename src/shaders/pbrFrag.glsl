#version 330 core
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

vec3 light_positions[4] = vec3[4](
    vec3(10, 10, 0),
    vec3(0, 5, 0),
    vec3(-10, 10, 0),
    vec3(1, 5, 15)
);

vec3 light_color = vec3(0.5451, 0, 0.5451);

const float PI = 3.14159265359;

/// Approximates amount of surface microfacets are aligned to the halfway vector
float normalDistribGGX(vec3 norm, vec3 halfway, float roughness) {
    // Trowbridge-Reitz
    float a = roughness * roughness;
    float a2 = a * a;
    float n_dot_h = max(dot(norm, halfway), 0.0);
    float n_dot_h_2 = n_dot_h * n_dot_h;

    float denom = (n_dot_h_2 * (a2 - 1.0) + 1.0);

    return a2 / (PI * denom * denom);
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
vec3 fresnelSchlick(vec3 f0, vec3 light_dir, vec3 halfway) {
    float cos_theta = max(dot(light_dir, halfway), 0.0);
    // f0 is surface reflectance at zero incidence
    // (how much the surface reflects when looking directly at it)
    return f0 + (1.0 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

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

/// Computes the direct radiance from an array of light sources
vec3 directRadiance(vec3 norm, vec3 view_dir, vec3 f0, float roughness, 
    float metallic, vec3 albedo) 
{
    vec3 radiance_out = vec3(0);

    for(int i = 0; i < 4; ++i) {
        // using point lights, so we know where the light is coming from 
        // so not exactly integrating over total area

        vec3 light_dir = normalize(light_positions[i] - frag_pos);
        vec3 halfway = normalize(light_dir + view_dir);

        float dist = length(light_positions[i] - frag_pos);
        float attenuation = 1.0 / (dist * dist);
        vec3 light_radiance = light_color * attenuation;

        vec3 fresnel = fresnelSchlick(f0, view_dir, halfway);
        float ndf = normalDistribGGX(norm, halfway, roughness);
        float g = geometrySmith(norm, view_dir, light_dir, roughness);

        // cook-torrence brdf
        // approximates how much each individual light ray contributes
        // to final reflected light of an opaque surface
        float n_dot_l = max(dot(norm, light_dir), 0.0);
        float brdfDenom = 4.0 * max(dot(norm, view_dir), 0.0) * 
            n_dot_l + 0.0001;
        // add small factor to prevent divide by 0
        vec3 specular = ndf * g * fresnel / brdfDenom;

        vec3 ks = fresnel; // specular factor
        vec3 kd = vec3(1.0) - ks; // 1 - ks to conserve energy
        kd *= 1.0 - metallic; //metallic surfaces don't have diffuse reflections

        radiance_out += (kd * albedo / PI + specular) * light_radiance * n_dot_l;

    }

    return radiance_out;
}

void main() {
    vec3 albedo = pow(texture(albedo_map, f_tex_coords).rgb, vec3(2.2));
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
        metallic, albedo);

    vec3 ks = fresnelSchlickRoughness(f0, view_dir, norm, roughness);
    vec2 env_brdf = texture(brdf_lut, vec2(max(dot(norm, view_dir), 0.0), roughness)).rg;
    vec3 kd = 1.0 - ks;
    kd *= 1.0 - metallic;

    vec3 irradiance = texture(irradiance_map, norm).rgb;
    // irradiance map is precomputed integral of light intensity over hemisphere
    vec3 diffuse = irradiance * albedo;
    vec3 specular = prefilter_color * (ks * env_brdf.x + env_brdf.y);
    vec3 ambient = (kd * diffuse + specular) * ao;
    vec3 color = ambient + direct_radiance + emission;

    frag_color = vec4(color, 1.0);
}