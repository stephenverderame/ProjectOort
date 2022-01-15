#version 430

struct LightData {
    vec4 start;
    vec4 end;
};

layout(std430, binding = 0) readonly buffer LightBuffer {
    uint light_num;
    LightData lights[];
};

layout(std430, binding = 1) writeonly buffer VisibleLightIndices {
    // flattened 2D array of work_groups x visible_lights
    int indices[];
} visibleLightBuffer;

#define MAX_LIGHTS_PER_TILE 1024

uniform sampler2D depth_tex;
uniform mat4 view;
uniform mat4 proj;
uniform mat4 viewproj;
uniform ivec2 screen_size;

// shared values between threads in a working group
shared uint minDepth; //integral to allow atomic access
shared uint maxDepth;
// min and max depth values of a tile

shared uint visibleLightCount; // num lights visible from a tile (group)
shared int visibleLightIndices[MAX_LIGHTS_PER_TILE]; // indices of lights visible from a tile (working group)

shared vec4 frustumPlanes[6]; // the view frustum for a tile


// read-write synchronization done with barrier() function
// no matter how different the input data is, all invocations
// must hit the same set of barrier calls in the exact same order

// atomic functions can also be performed on integral types and structs/vectors/arrays
// of integral types

// compute shaders must manually read/write all user-defined inputs and outputs

#define TILE_SIZE 16 // split image into 16 x 16 tiles. Each tile is handled
// by a working group with a thread per pixel

layout(local_size_x = TILE_SIZE, local_size_y = TILE_SIZE, local_size_z = 1) in;
// local size defines the number of invocations of a shader that occurs in a work group

void updateMinMaxDepth(ivec2 location) {
    vec2 tex_coords = vec2(location) / screen_size;
    float depth = texture(depth_tex, tex_coords).r;

    // linearize depth values due to a perspective matrix
    depth = (0.5 * proj[3][2]) / (depth + proj[2][2] * 0.5 - 0.5);

    uint depthInt = floatBitsToUint(depth);
    atomicMin(minDepth, depthInt);
    atomicMax(maxDepth, depthInt);
}

void calcFrustrumPlanes(ivec2 tileId, ivec2 tileNum) {
    float minDepth_f = uintBitsToFloat(minDepth);
    float maxDepth_f = uintBitsToFloat(maxDepth);

    // Steps based on tile sale
    vec2 negativeStep = (2.0 * vec2(tileId)) / vec2(tileNum);
    vec2 positiveStep = (2.0 * vec2(tileId + ivec2(1, 1))) / vec2(tileNum);

    // Set up starting values for planes using steps and min and max z values
    frustumPlanes[0] = vec4(1.0, 0.0, 0.0, 1.0 - negativeStep.x); // Left
    frustumPlanes[1] = vec4(-1.0, 0.0, 0.0, -1.0 + positiveStep.x); // Right
    frustumPlanes[2] = vec4(0.0, 1.0, 0.0, 1.0 - negativeStep.y); // Bottom
    frustumPlanes[3] = vec4(0.0, -1.0, 0.0, -1.0 + positiveStep.y); // Top
    frustumPlanes[4] = vec4(0.0, 0.0, -1.0, -minDepth_f); // Near
    frustumPlanes[5] = vec4(0.0, 0.0, 1.0, maxDepth_f); // Far

    // Transform the first four planes
    for (uint i = 0; i < 4; i++) {
        frustumPlanes[i] *= viewproj;
        frustumPlanes[i] /= length(frustumPlanes[i].xyz);
    }

    // Transform the depth planes
    frustumPlanes[4] *= view;
    frustumPlanes[4] /= length(frustumPlanes[4].xyz);
    frustumPlanes[5] *= view;
    frustumPlanes[5] /= length(frustumPlanes[5].xyz);
}

bool isLightInFrustrum(uint lightIndex) {
    vec4 position = vec4((lights[lightIndex].start + lights[lightIndex].end).xyz / 2.0, 1.0);
    // simply take the midpoint
    float radius = length(lights[lightIndex].start.xyz - position.xyz) + 8.0;

    // We check if the light exists in our frustum
    float dist = 0.0;
    for (uint j = 0; j < 6; j++) {
        dist = dot(position, frustumPlanes[j]) + radius;

        // If one of the tests fails, then there is no intersection
        if (dist <= 0.0) {
            break;
        }
    }
    return dist > 0.0;
}

void cullLights() {
    // Step 3: Cull lights.
	// Parallelize the threads against the lights now.
	// Can handle 256 simultaniously. Anymore lights than that and additional passes are performed
	uint threadCount = TILE_SIZE * TILE_SIZE;
	uint lightsPerThread = (light_num + threadCount - 1) / threadCount;
	for (uint i = 0; i < lightsPerThread; i++) {
		// Get the lightIndex to test for this thread / pass. If the index is >= light count, then this thread can stop testing lights
		uint lightIndex = i * threadCount + gl_LocalInvocationIndex;
		if (lightIndex >= light_num) {
			break;
		}
		if (isLightInFrustrum(lightIndex)) {
			// Add index to the shared array of visible indices
			uint offset = atomicAdd(visibleLightCount, 1);
            if (offset < MAX_LIGHTS_PER_TILE)
			    visibleLightIndices[offset] = int(lightIndex);
                // lightIndex is visible from the tile
		}
	}
}

void flushLocalToGlobalVisibleLights(uint groupIndex) {
    uint globalOffset = groupIndex * MAX_LIGHTS_PER_TILE;
    // offset to area of memory for this work group

    for (uint i = 0; i < visibleLightCount; ++i) {
        visibleLightBuffer.indices[globalOffset + i] = visibleLightIndices[i];
    }
    if (visibleLightCount < MAX_LIGHTS_PER_TILE) {
        // terminal index unless whole buffer was filled
        visibleLightBuffer.indices[globalOffset + visibleLightCount] = -1;
    }
}

void main() {
    ivec2 location = ivec2(gl_GlobalInvocationID.xy); //gl_WorkGroupID * gl_WorkGroupSize + gl_LocalInvocationID
    // value uniques identifies the shader invocation
    ivec2 itemId = ivec2(gl_LocalInvocationID.xy); //current invocation of shader within the work group
    // each component between 0 and gl_WorkGroupSize.xyz
    ivec2 tileId = ivec2(gl_WorkGroupID.xy); //current work group for this shader invocation
    ivec2 tileNum = ivec2(gl_NumWorkGroups.xy); // total number of work groups passed to dispatch function\

    uint workGroupId = tileId.y * tileNum.x + tileId.x; // "flattened" gl_WorkGroupID

    if (gl_LocalInvocationIndex == 0) {
        minDepth = 0xFFFFFFFF;
        maxDepth = 0;
        visibleLightCount = 0;
        // one thread initializes all shared variables
    }

    barrier();
    updateMinMaxDepth(location);
    barrier();

    if (gl_LocalInvocationIndex == 0) {
        calcFrustrumPlanes(tileId, tileNum);
    }

    barrier();
    cullLights();
    barrier();

    if (gl_LocalInvocationIndex == 0) {
        flushLocalToGlobalVisibleLights(workGroupId);
    }
}