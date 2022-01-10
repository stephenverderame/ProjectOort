#version 430

struct Triangle {
    vec4 v1, v2, v3;
};

layout(std430, binding = 5) readonly buffer ATriangles {
    Triangle a_triangles[];
};

layout(std430, binding = 6) readonly buffer BTriangles {
    Triangle b_triangles[];
};

layout(std430, binding = 7) writeonly buffer Collisions {
    uvec4 out_buffer[];
};

uniform mat4 model_a;
uniform mat4 model_b;

shared uint collisions;

const float eps = 0.00001;

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

// Returns true if all vertices of b are on the same side of the plane of a
// Therefore, if this returns true, no collision can be possible
// output parameter to psuedo pass by reference
bool planeTest(out Triangle a, out Triangle b, vec3 a_norm, 
    out bool coplanar, out vec3 b_signed_dist) 
{
    float d = dot(-a_norm, a.v1.xyz);
    // norm dot x + d = 0 -> plane equation
    // plug in any vertex to get d
    float b_d1 = dot(a_norm, b.v1.xyz) + d;
    float b_d2 = dot(a_norm, b.v2.xyz) + d;
    float b_d3 = dot(a_norm, b.v3.xyz) + d;
    coplanar = coplanar || (abs(b_d1) < eps && abs(b_d2) < eps && abs(b_d3) < eps);
    b_signed_dist = vec3(b_d1, b_d2, b_d3);
    return (b_d1 < -eps && b_d2 < -eps && b_d3 < -eps) ||
        (b_d1 > eps && b_d2 > eps && b_d3 > eps);
}

// gets the index of the largest dimension of v
uint absMaxDim(vec3 v) {
    uint axis = 0;
    float max_component = 0;
    for(int i = 0; i < 3; ++i) {
        float e = abs(v[i]);
        if (e > max_component) {
            max_component = e;
            axis = i;
        }
    }
    return axis;
}

// gets the t value of the intersection point of the parameterized line
// vert[index] to vert[0] and the triangle intersection line
// requires index is 1 or 2
float getT(vec3 verts_on_l, vec3 dist_to_plane, uint index) {
    return verts_on_l[0] + (verts_on_l[index] - verts_on_l[0]) *
    dist_to_plane[0] / (dist_to_plane[0] - dist_to_plane[1]);
}

bool lineIntersection(vec2 start_a, vec2 end_a, vec2 start_b, vec2 end_b) {
    vec2 a = end_a - start_a;
    vec2 b = end_b - start_b;
    vec2 y = start_b - start_a;
    mat2 A = mat2(a, b);

    vec2 x = y * inverse(A);

    return x.x >= -eps && x.x <= 1 + eps &&
        x.y >= -eps && x.y <= 1 + eps;
}

bool pointInTriangle(vec2 point, vec2 a, vec2 b, vec2 c) {
    vec2 p = point - a;
    mat2 A = mat2(b - a, c - a);

    vec2 o = p * inverse(A);
    // o is the barycentric coordinates for point with respect to the triangle
    return o.x > -eps && o.y > -eps && o.x + o.y < 1 + eps;
}

void main() {
    uvec2 location = gl_GlobalInvocationID.xy;

    if (gl_LocalInvocationIndex == 0) {
        collisions = 0;
    }

    barrier();

    // Moller's Triangle-Triangle interval overlap method
    if (location.x < a_triangles.length() && location.y < b_triangles.length()) {
        Triangle a = a_triangles[location.x];
        Triangle b = b_triangles[location.y];
        // do world computation on CPU
        vec3 a_norm = normalize(cross(a.v3.xyz - a.v1.xyz, a.v2.xyz - a.v1.xyz));
        vec3 b_norm = normalize(cross(b.v3.xyz - b.v1.xyz, b.v2.xyz - b.v1.xyz));
        bool coplanar = false;
        vec3 b_onto_a; // signed distances of vertices of b onto plane of a
        vec3 a_onto_b;
        if (!planeTest(a, b, a_norm, coplanar, b_onto_a) && !coplanar
            && !planeTest(b, a, b_norm, coplanar, a_onto_b) && !coplanar) {
            // a line must pass through both triangles
            // this line is the cross product of the normals
            vec3 v = cross(a_norm, b_norm);
            // overlap test doesn't change if we project not onto v, but onto
            // the coordinate axis for which v is most closely aligned
            uint idx = absMaxDim(v);
            vec3 a_pts = vec3(a.v1[idx], a.v2[idx], a.v3[idx]);
            vec3 b_pts = vec3(b.v1[idx], b.v2[idx], b.v3[idx]);

            float a_t1 = getT(a_pts, a_onto_b, 1);
            float a_t2 = getT(a_pts, a_onto_b, 2);

            float b_t1 = getT(b_pts, b_onto_a, 1);
            float b_t2 = getT(b_pts, b_onto_a, 2);

            if (a_t1 <= b_t1 && a_t2 >= b_t1 || a_t1 <= b_t2 && a_t2 >= b_t2) {
                // intervals overlap, so collision detected
                atomicAdd(collisions, 1);
            }
        } else if (coplanar) {
            // project onto axis-aligned plane that is closest to the plane
            // both triangles are on and perform 2d triangle collision detection
            uint normalAxis = absMaxDim(a_norm);
            uint x = (normalAxis + 1) % 3;
            uint y = (normalAxis + 2) % 3;

            vec2 a1 = vec2(a.v1[x], a.v1[y]);
            vec2 a2 = vec2(a.v2[x], a.v2[y]);
            vec2 a3 = vec2(a.v3[x], a.v3[y]);

            vec2 b1 = vec2(b.v1[x], b.v1[y]);
            vec2 b2 = vec2(b.v2[x], b.v2[y]);
            vec2 b3 = vec2(b.v3[x], b.v3[y]);

            if (lineIntersection(a1, a2, b1, b2) || lineIntersection(a1, a2, b2, b3)
                || lineIntersection(a1, a2, b1, b3) || lineIntersection(a2, a3, b1, b2)
                || lineIntersection(a2, a3, b2, b3) || lineIntersection(a2, a3, b1, b3)
                || lineIntersection(a1, a3, b1, b2) || lineIntersection(a1, a3, b2, b3)
                || lineIntersection(a1, a3, b1, b3)) 
            {
                atomicAdd(collisions, 1);
            } else if (pointInTriangle(a1, b1, b2, b3) || pointInTriangle(a2, b1, b2, b3)
                || pointInTriangle(a3, b1, b2, b3) || pointInTriangle(b1, a1, a2, a3)
                || pointInTriangle(b2, a1, a2, a3) || pointInTriangle(b3, a1, a2, a3)) 
            {
                atomicAdd(collisions, 1);
            }
        }  
    }

    barrier();

    if (gl_LocalInvocationIndex == 0) {
        uint workGroupId = gl_WorkGroupID.y * gl_NumWorkGroups.x + gl_WorkGroupID.x;
        out_buffer[workGroupId].x = collisions;
    }

}