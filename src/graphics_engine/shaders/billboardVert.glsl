#version 430 core
layout (location = 0) in vec3 pos;
layout (location = 1) in vec2 tex_coords;
layout (location = 2) in vec4 instance_model_col0;
layout (location = 3) in vec4 instance_model_col1;
layout (location = 4) in vec4 instance_model_col2;
layout (location = 5) in vec4 instance_model_col3;
layout (location = 6) in vec4 instance_color;

uniform mat4 view;
uniform mat4 proj;

out vec4 color;
out vec2 tcoords;

void main() {
    mat4 model = mat4(instance_model_col0, instance_model_col1, 
        instance_model_col2, instance_model_col3);
    color = instance_color;
    tcoords = tex_coords;
    gl_Position = proj * view * model * vec4(pos, 1.0);
}