#version 430 core
layout (location = 0) in vec3 pos;
layout (location = 1) in vec3 normal;
layout (location = 2) in vec2 tex_coords;
layout (location = 3) in vec4 instance_model_col0;
layout (location = 4) in vec4 instance_model_col1;
layout (location = 5) in vec4 instance_model_col2;
layout (location = 6) in vec4 instance_model_col3;

uniform mat4 viewproj;

out FragData {
    vec2 tex_coords;
    vec3 color;
} v_out;

void main() {
    mat4 model = mat4(instance_model_col0, instance_model_col1, 
        instance_model_col2, instance_model_col3);
    gl_Position = viewproj * model * vec4(pos, 1.0);
    v_out.tex_coords = tex_coords;
    v_out.color = vec3(0.5451, 0.0, 0.5451);
}