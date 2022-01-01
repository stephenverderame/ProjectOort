#version 430 core
layout (location = 0) in vec3 pos;
layout (location = 1) in vec3 normal;
layout (location = 2) in vec2 tex_coords;
layout (location = 3) in vec4 instance_model_col0;
layout (location = 4) in vec4 instance_model_col1;
layout (location = 5) in vec4 instance_model_col2;
layout (location = 6) in vec4 instance_model_col3;
layout (location = 7) in vec3 instance_color;

out vec3 f_normal;
out vec3 frag_pos;
out vec2 f_tex_coords;
out vec4 frag_pos_light;

uniform mat4 viewproj;
uniform mat4 light_viewproj;

void main() {
    mat4 model = mat4(instance_model_col0, instance_model_col1, 
        instance_model_col2, instance_model_col3);
    f_tex_coords = tex_coords;
    f_normal = mat3(model) * normal;
    frag_pos = vec3(model * vec4(pos, 1.0));
    frag_pos_light = light_viewproj * vec4(frag_pos, 1.0);

    gl_Position = viewproj * vec4(frag_pos, 1.0);
}