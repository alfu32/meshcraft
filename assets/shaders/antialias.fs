#version 330
in vec2 fragTexCoord;
in vec4 fragColor;
uniform sampler2D texture0;
uniform vec2 inverseSize;
uniform float span;
uniform float threshold;
out vec4 finalColor;
float luminance(vec3 c) { return dot(c, vec3(0.299, 0.587, 0.114)); }
void main() {
    vec2 uv = fragTexCoord;
    vec4 center = texture(texture0, uv);
    float m = luminance(center.rgb);
    float nw = luminance(texture(texture0, uv + vec2(-1,-1)*inverseSize).rgb);
    float ne = luminance(texture(texture0, uv + vec2(1,-1)*inverseSize).rgb);
    float sw = luminance(texture(texture0, uv + vec2(-1,1)*inverseSize).rgb);
    float se = luminance(texture(texture0, uv + vec2(1,1)*inverseSize).rgb);
    float lo = min(m, min(min(nw, ne), min(sw, se)));
    float hi = max(m, max(max(nw, ne), max(sw, se)));
    if (hi-lo < max(0.025, hi*threshold)) { finalColor = center; return; }
    vec2 direction = vec2(-((nw+ne)-(sw+se)), (nw+sw)-(ne+se));
    float reduce = max((nw+ne+sw+se)*0.03125, 0.0078125);
    direction = clamp(direction/(min(abs(direction.x),abs(direction.y))+reduce), -span, span)*inverseSize;
    vec3 inner = 0.5*(texture(texture0,uv-direction/6.0).rgb + texture(texture0,uv+direction/6.0).rgb);
    vec3 outer = inner*0.5 + 0.25*(texture(texture0,uv-direction*0.5).rgb + texture(texture0,uv+direction*0.5).rgb);
    float y = luminance(outer);
    finalColor = vec4(y < lo || y > hi ? inner : outer, center.a);
}
