#version 330
layout(location=0) in vec3 domainA;
layout(location=1) in vec3 domainB;
layout(location=2) in vec3 domainC;
layout(location=3) in vec2 domainSeed;
uniform mat4 viewProjection;
uniform mat4 model;
uniform int bladesPerTriangle;
uniform int generation;
uniform float time;
uniform float density;
uniform vec4 baseColor;
out vec3 worldPosition;
out vec4 color;
out vec2 proceduralUv;
out vec2 materialUv;
out vec3 proceduralNormal;
flat out float faceOrientation;
/*PARAMETERS*/
/*VERTEX*/
uint hash(uint x) { x ^= x >> 16u; x *= 0x7feb352du; x ^= x >> 15u; x *= 0x846ca68bu; return x ^ (x >> 16u); }
float random(uint x) { return float(hash(x) & 0x00ffffffu) / 16777216.0; }
void main() {
    vec3 normal = normalize(cross(domainB-domainA,domainC-domainA));
    vec3 position;
    vec2 uv = vec2(0.0);
    color = baseColor;
    if (generation == 1) {
        uint candidate = uint(gl_InstanceID % bladesPerTriangle);
        uint key = hash(uint(domainSeed.x) | (uint(domainSeed.y) << 16u)) ^ hash(candidate) ^ uint(u_seed);
        vec3 wa=(model*vec4(domainA,1.0)).xyz, wb=(model*vec4(domainB,1.0)).xyz, wc=(model*vec4(domainC,1.0)).xyz;
        float area=length(cross(wb-wa,wc-wa))*0.5;
        // Fractional last blade uses a stable sample, so density varies smoothly.
        if (float(candidate)+random(key+3u) >= area*density) {
            gl_Position=vec4(2.0,2.0,2.0,1.0);worldPosition=vec3(0.0);proceduralUv=uv;materialUv=uv;proceduralNormal=normal;faceOrientation=1.0;return;
        }
        float r=sqrt(random(key+1u)), s=random(key+2u);
        vec3 root=domainA*(1.0-r)+domainB*(r*(1.0-s))+domainC*(r*s);
        vec3 tangent=normalize(domainB-domainA), bitangent=normalize(cross(normal,tangent));
        float angle=random(key+4u)*6.28318530718;
        vec3 side=tangent*cos(angle)+bitangent*sin(angle), bend=cross(normal,side);
        int segment=gl_VertexID/6;
        int corner=gl_VertexID%6;
        vec2 pattern[6]=vec2[](vec2(-1,0),vec2(1,0),vec2(1,1),vec2(-1,0),vec2(1,1),vec2(-1,1));
        uv=vec2(pattern[corner].x,(float(segment)+pattern[corner].y)/3.0);
        float h=u_height*(0.7+0.6*random(key+5u));
        float wind=u_wind*sin(time*1.7+root.x*0.5+root.z*0.7+angle);
        position=root+normal*(uv.y*h)+side*(uv.x*u_width*0.5*(1.0-uv.y))+bend*(uv.y*uv.y*h*(u_curvature+wind));
        materialUv=vec2(uv.x*u_width*0.5,uv.y*h);
        color.rgb*=1.0-u_variation*random(key+6u);
        color.rgb*=0.65+0.35*uv.y;
    } else {
        position=gl_VertexID==0?domainA:(gl_VertexID==1?domainB:domainC);
        uv=gl_VertexID==0?vec2(0,0):(gl_VertexID==1?vec2(1,0):vec2(0,1));
        vec3 tangent=normalize(domainB-domainA);materialUv=vec2(dot(position-domainA,tangent),dot(position-domainA,cross(normal,tangent)));
    }
    proceduralVertex(position,normal,uv);
    worldPosition=(model*vec4(position,1.0)).xyz;
    proceduralUv=uv;
    proceduralNormal=normalize(transpose(inverse(mat3(model)))*normal);
    faceOrientation=determinant(mat3(model))<0.0?-1.0:1.0;
    gl_Position=viewProjection*vec4(worldPosition,1.0);
}
