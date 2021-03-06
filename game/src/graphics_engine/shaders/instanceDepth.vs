#version 430 core
layout (location = 0) in vec3 pos;
layout (location = 1) in vec4 instance_model_col0;
layout (location = 2) in vec4 instance_model_col1;
layout (location = 3) in vec4 instance_model_col2;
layout (location = 4) in vec4 instance_model_col3;

uniform mat4 viewproj;

void main() {
    mat4 model = mat4(instance_model_col0, instance_model_col1, 
        instance_model_col2, instance_model_col3);
    gl_Position = viewproj * model * vec4(pos, 1.0);
}