#version 330
in vec3 vertexPosition;
in vec4 vertexColor;
uniform mat4 mvp;
uniform float depthOffset;
out vec3 worldPosition;
out vec3 localPosition;
out vec4 color;
flat out float faceOrientation;
void main() {
    worldPosition = vertexPosition;
    localPosition = vertexPosition;
    color = vertexColor;
    faceOrientation = 1.0;
    gl_Position = mvp * vec4(vertexPosition, 1.0);
    // This shader draws the synthetic ground receiver. Keep it a few depth
    // samples behind authored faces, even when a distant camera makes its
    // 0.01 model-unit separation from a floor too small for the depth buffer.
    gl_Position.z += depthOffset * gl_Position.w;
}
