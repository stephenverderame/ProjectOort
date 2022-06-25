#version 430 core

uniform float inv_fac;
out float o_inv_fac;

void main() {
    o_inv_fac = inv_fac;
}