#version 430

struct Triangle {
    vec4 v[4];
};

layout(std430, binding = 5) readonly buffer ATriangles {
    Triangle a_triangles[];
};

layout(std430, binding = 6) readonly buffer BTriangles {
    Triangle b_triangles[];
};

layout(std430, binding = 7) writeonly buffer Collisions {
    vec4 out_buffer[];
};

uniform mat4 model_a;
uniform mat4 model_b;

shared int collisions;

const float eps = 0.000001;

layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;

// Returns true if all vertices of b are on the same side of the plane of a
// Therefore, if this returns true, no collision can be possible
// coplanar - output parameter if triangles are coplaner
// b_sd - ('b_signed_dist') output parameter storing signed distances of vertices of b
// from plane of a
bool planeTest(Triangle a, Triangle b, vec3 a_norm, 
    inout bool coplanar, out vec3 b_sd) 
{
    float d = dot(-a_norm, a.v[0].xyz);
    // norm dot x + d = 0 -> plane equation
    // plug in any vertex to get d
    b_sd.x = dot(a_norm, b.v[0].xyz) + d;
    b_sd.y = dot(a_norm, b.v[1].xyz) + d;
    b_sd.z = dot(a_norm, b.v[2].xyz) + d;
    coplanar = coplanar || (abs(b_sd.x) < eps && abs(b_sd.y) < eps && abs(b_sd.z) < eps);
    return (b_sd.x < 0 && b_sd.y < 0 && b_sd.z < 0) ||
        (b_sd.x > 0 && b_sd.y > 0 && b_sd.z > 0);
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
float getT(vec3 verts_on_l, vec3 dist_to_plane, uint oppositeIdx, uint vertIdx) {
    return verts_on_l[vertIdx] + (verts_on_l[oppositeIdx] - verts_on_l[vertIdx]) *
    dist_to_plane[vertIdx] / (dist_to_plane[vertIdx] - dist_to_plane[oppositeIdx]);
}

// orders v so that v.x <= v.y
vec2 putInOrder(vec2 v) {
    if (v.x > v.y) {
        float y = v.y;
        v.y = v.x;
        v.x = y;
    }
    return v;
}
// Tests wether the intervals defined by t_a and t_b overlap
// Intervals do not need to be in ascending order (ie. x, does not need to be the min)
bool intervalOverlap(vec2 a_t, vec2 b_t) {
    a_t = putInOrder(a_t);
    b_t = putInOrder(b_t);
    return a_t.x - eps <= b_t.x && a_t.y + eps >= b_t.x || 
        a_t.x - eps <= b_t.y && a_t.y + eps >= b_t.y ||
        b_t.x - eps <= a_t.x && b_t.y + eps >= a_t.x || 
        b_t.x - eps <= a_t.y && b_t.y + eps >= a_t.y;
}


// gets the overlap interval of a triangle
// project_on_l - the values of vertex 0, 1, 2 projected onto the line
// signed_dists - the signed distance of vertex 0, 1, 2 to the other triangle's plane
// vertices - the index of the vertex on the opposisite side of the other triangle's plane
//  followed by the indices of the vertices on the same side of the plane
//  so signed_dists[vertices.x] should have opposite sign as signed_dists[vertices.y] and
//  signed_dists[vertices.z]
vec2 getInterval(vec3 project_on_l, vec3 signed_dists, uvec3 vertices) {
    return vec2(getT(project_on_l, signed_dists, vertices.x, vertices.y),
        getT(project_on_l, signed_dists, vertices.x, vertices.z));
}

float cross2D(vec2 a, vec2 b) {
    return a.x * b.y - a.y * b.x;
}

bool lineIntersection2D(vec2 start_a, vec2 end_a, vec2 start_b, vec2 end_b) {
    vec2 a = end_a - start_a;
    vec2 b = end_b - start_b;
    
    float rs = cross2D(a, b);
    float qpr = cross2D(start_b - start_a, a);

    if (abs(rs) < eps && abs(qpr) < eps) {
        // colinear, project onto line and test overlap
        vec2 l = normalize(a);
        vec2 t_a = vec2(dot(start_a, l), dot(end_a, l));
        vec2 t_b = vec2(dot(start_b, l), dot(end_b, l));
        return intervalOverlap(t_a, t_b);
    }
    else if (abs(rs) < eps) return false; // parallel

    float t = cross2D(start_b - start_a, b) / rs;
    float u = qpr / rs;

    return t >= -eps && t <= 1 + eps && u >= -eps && u <= 1 + eps;
}

/// Gets a vector containing the vertex index of the vertex that is on the opposite
/// side of the plane as the other two vertices, followed by the other two vertices
// on the same side of the plane
uvec3 oppositeVertex(vec3 triangleSignedDistances) {
    if (triangleSignedDistances[0] * triangleSignedDistances[1] > 0)
        return uvec3(2, 0, 1);
    else if (triangleSignedDistances[0] * triangleSignedDistances[2] > 0)
        return uvec3(1, 0, 2);
    else
        return uvec3(0, 1, 2);
}

// Moller's Triangle-Triangle interval overlap method
void mollerTriangleTest(uvec2 location) {
    Triangle a = a_triangles[location.x];
    Triangle b = b_triangles[location.y];
    // do world computation on CPU
    vec3 a_norm = normalize(cross(a.v[2].xyz - a.v[0].xyz, a.v[1].xyz - a.v[0].xyz));
    vec3 b_norm = normalize(cross(b.v[2].xyz - b.v[0].xyz, b.v[1].xyz - b.v[0].xyz));
    bool coplanar = false;
    vec3 b_dist_from_a; // signed distances of vertices of b onto plane of a
    vec3 a_dist_from_b;
    bool test_a = planeTest(a, b, a_norm, coplanar, b_dist_from_a);
    bool test_b = planeTest(b, a, b_norm, coplanar, a_dist_from_b);
    if (!(test_a || test_b) && !coplanar) {
        // a line must pass through both triangles
        // this line's direction is the cross product of the normals
        vec3 v = normalize(cross(a_norm, b_norm));
        // overlap test doesn't change if we project not onto v, but onto
        // the coordinate axis for which v is most closely aligned
        uint idx = absMaxDim(v);
        vec3 a_pts = vec3(a.v[0][idx], a.v[1][idx], a.v[2][idx]);
        vec3 b_pts = vec3(b.v[0][idx], b.v[1][idx], b.v[2][idx]);
        // we find intersections between the two edges connecting the vertex that
        // is on the other side of the triangle plane as the other two vertices
        // and v projected onto its world axis
        uvec3 a_opp = oppositeVertex(a_dist_from_b);
        uvec3 b_opp = oppositeVertex(b_dist_from_a);
        vec2 a_t = getInterval(a_pts, a_dist_from_b, a_opp);
        vec2 b_t = getInterval(b_pts, b_dist_from_a, b_opp);
        
        if (intervalOverlap(a_t, b_t)) {
            // intervals overlap, so collision detected
            atomicAdd(collisions, 1);
        }
    } else if (coplanar) {
        // project onto axis-aligned plane that is closest to the plane
        // both triangles are on and perform 2d triangle collision detection
        uint normalAxis = absMaxDim(a_norm);
        uint x = (normalAxis + 1) % 3;
        uint y = (normalAxis + 2) % 3;


        vec2 a1 = vec2(a.v[0][x], a.v[0][y]);
        vec2 a2 = vec2(a.v[1][x], a.v[1][y]);
        vec2 a3 = vec2(a.v[2][x], a.v[2][y]);

        vec2 b1 = vec2(b.v[0][x], b.v[0][y]);
        vec2 b2 = vec2(b.v[1][x], b.v[1][y]);
        vec2 b3 = vec2(b.v[2][x], b.v[2][y]);

        if (lineIntersection2D(a1, a2, b1, b2) || lineIntersection2D(a1, a2, b2, b3)
            || lineIntersection2D(a1, a2, b1, b3) || lineIntersection2D(a2, a3, b1, b2)
            || lineIntersection2D(a2, a3, b2, b3) || lineIntersection2D(a2, a3, b1, b3)
            || lineIntersection2D(a1, a3, b1, b2) || lineIntersection2D(a1, a3, b2, b3)
            || lineIntersection2D(a1, a3, b1, b3)) 
        {
            atomicAdd(collisions, 1);
        }
    }
}

void main() {
    uvec2 location = gl_GlobalInvocationID.xy;

    if (gl_LocalInvocationIndex == 0) {
        collisions = 0;
    }

    barrier();

    if (location.x < a_triangles.length() && location.y < b_triangles.length()) {
        mollerTriangleTest(location);
    }

    barrier();

    if (gl_LocalInvocationIndex == 0) {
        uint workGroupId = gl_WorkGroupID.y * gl_NumWorkGroups.x + gl_WorkGroupID.x;
        out_buffer[workGroupId].x = float(collisions);
    }

}