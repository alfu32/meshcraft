#version 330
layout(location=0) in vec3 vertexPosition;
layout(location=3) in vec4 vertexColor;
layout(location=4) in vec4 instanceRow0;
layout(location=5) in vec4 instanceRow1;
layout(location=6) in vec4 instanceRow2;
layout(location=7) in vec4 instanceFlags;
uniform mat4 viewProjection;
uniform float depthOffset;
out vec3 worldPosition;
out vec4 color;
flat out float faceOrientation;
void main() {
    vec4 p=vec4(vertexPosition,1.0);
    worldPosition=vec3(dot(instanceRow0,p),dot(instanceRow1,p),dot(instanceRow2,p));
    color=instanceFlags.y>0.5 ? vec4(188.0/255.0,201.0/255.0,212.0/255.0,1.0) : vertexColor;
    faceOrientation=instanceFlags.x;
    gl_Position=viewProjection*vec4(worldPosition,1.0);
    gl_Position.z+=depthOffset/max(abs(gl_Position.w),0.01);
}
