#version 330
// Each line is a static six-vertex quad; endpoints stay on the GPU between pointer moves.
layout(location = 0) in vec3 vertexPosition;
layout(location = 1) in vec2 vertexTexCoord;
layout(location = 2) in vec3 vertexNormal; // Second endpoint, not a lighting normal.
uniform mat4 mvp;
uniform vec2 viewportSize;
uniform float lineWidth;
void main() {
    vec4 a = mvp * vec4(vertexPosition, 1.0);
    vec4 b = mvp * vec4(vertexNormal, 1.0);
    float da = a.z + a.w;
    float db = b.z + b.w;
    if (da < 0.0 && db < 0.0) {
        gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
        return;
    }
    // Clip before dividing by w, including lines crossing the eye/near plane.
    if (da < 0.0) a = mix(a, b, da / (da - db));
    if (db < 0.0) b = mix(b, a, db / (db - max(da, 0.0)));
    vec2 delta = (b.xy / b.w - a.xy / a.w) * viewportSize;
    float lengthPixels = length(delta);
    vec2 side = lengthPixels > 0.00001 ? vec2(-delta.y, delta.x) / lengthPixels : vec2(0.0);
    vec4 point = mix(a, b, vertexTexCoord.x);
    point.xy += side * vertexTexCoord.y * lineWidth / viewportSize * point.w;
    point.z -= 0.00001 * point.w;
    gl_Position = point;
}
