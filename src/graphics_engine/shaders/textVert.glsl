#version 430 core
layout (location = 0) in vec3 pos;
layout (location = 1) in vec2 tex_coords;
layout (location = 2) in vec4 instance_model_col0;
layout (location = 3) in vec4 instance_model_col1;
layout (location = 4) in vec4 instance_model_col2;
layout (location = 5) in vec4 instance_model_col3;
layout (location = 6) in ivec4 x_y_width_height;
layout (location = 7) in vec4 color;

uniform mat4 viewproj;

flat out Glyph {
    ivec4 x_y_width_height;
    vec4 color;
} glyph;

out vec2 tcoords;

void main() {
    mat4 model = mat4(instance_model_col0, instance_model_col1, 
        instance_model_col2, instance_model_col3);
    gl_Position = viewproj * model * vec4(pos, 1.0);
    tcoords = tex_coords;
    glyph.color = color;
    glyph.x_y_width_height = x_y_width_height;
}