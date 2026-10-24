#version 330
in vec3 vertexPosition;
in vec4 vertexColor;
uniform mat4 mvp;
uniform float depthOffset;
out vec3 worldPosition;
out vec4 color;
flat out float faceOrientation;
void main() {
    worldPosition = vertexPosition;
    color = vertexColor;
    faceOrientation = 1.0;
    gl_Position = mvp * vec4(vertexPosition, 1.0);
    // Small eye-depth-aware fill bias keeps coplanar black lines continuous.
    gl_Position.z += depthOffset / max(abs(gl_Position.w), 0.01);
}
