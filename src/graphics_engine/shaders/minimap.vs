#version 430 core
layout (location = 0) in vec3 pos;
layout (location = 1) in vec2 tex_coords;
layout (location = 2) in vec4 color;
layout (location = 3) in uint tex_idx;
layout (location = 4) in vec4 instance_model_col0;
layout (location = 5) in vec4 instance_model_col1;
layout (location = 6) in vec4 instance_model_col2;
layout (location = 7) in vec4 instance_model_col3;

out FragData {
    vec2 tex_coords;
    flat vec4 color;
    flat uint tex_idx;
} fragData;

void main() {
    mat4 model = mat4(instance_model_col0, instance_model_col1, 
        instance_model_col2, instance_model_col3);

    fragData.tex_coords = tex_coords;
    fragData.color = color;
    fragData.tex_idx = tex_idx;
    gl_Position = model * vec4(pos, 1.0);
}