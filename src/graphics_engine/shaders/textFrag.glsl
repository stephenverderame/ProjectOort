#version 430 core

uniform sampler2D tex;
uniform vec2 tex_width_height;

in Glyph {
    flat ivec4 x_y_width_height;
    flat vec4 color;
} glyph;

in vec2 tcoords;

out vec4 frag_color;

const float smoothing = 1.0/16.0;

void main() {
    vec2 coords;
	coords.x = float(glyph.x_y_width_height.x) + 
        tcoords.x * float(glyph.x_y_width_height.z);
	coords.y = float(tex_width_height.y) - 
        (float(glyph.x_y_width_height.y) + float(glyph.x_y_width_height.w)) + 
        tcoords.y * float(glyph.x_y_width_height.w);
	coords /= tex_width_height;
    float dist = texture(tex, coords).a;
    float a = smoothstep(0.5 - smoothing, 0.5 + smoothing, dist);
    frag_color = vec4(glyph.color.rgb, glyph.color.a * a);
}