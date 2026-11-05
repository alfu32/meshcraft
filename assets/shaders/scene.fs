#version 330
in vec3 worldPosition;
in vec3 localPosition;
in vec4 color;
flat in float faceOrientation;
uniform mat4 lightVP;
uniform sampler2D shadowMap;
uniform vec3 towardLight;
uniform vec3 eye;
uniform float ambient;
uniform float directional;
uniform float shadowStrength;
uniform float specular;
uniform float depthBias;
uniform float normalBias;
uniform int shadows;
// Material image: offset (xy) and tile size (zw) in model units, as texture_mapping.rs defines.
uniform sampler2D surfaceTexture;
uniform int textured;
uniform vec4 textureFrame;
uniform float textureTint;
out vec4 finalColor;
vec4 surfaceColor() {
    if (textured == 0) return color;
    vec3 n = normalize(cross(dFdx(localPosition), dFdy(localPosition)));
    vec3 u = abs(n.y) > 0.999 ? vec3(1.0, 0.0, 0.0) : normalize(cross(vec3(0.0, 1.0, 0.0), n));
    vec3 v = normalize(cross(n, u));
    vec2 uv = (vec2(dot(localPosition, u), dot(localPosition, v)) - textureFrame.xy) / textureFrame.zw;
    // Image rows run top-down; V runs up the face.
    vec4 image = texture(surfaceTexture, vec2(uv.x, -uv.y));
    vec3 rgb = mix(image.rgb, image.rgb * color.rgb, textureTint);
    // Opaque materials cut out transparent pixels; translucent ones blend them.
    if (color.a > 0.999) {
        if (image.a < 0.5) discard;
        return vec4(rgb, 1.0);
    }
    return vec4(rgb, image.a * color.a);
}
float visibility(vec3 normal) {
    if (shadows == 0) return 1.0;
    vec4 projected = lightVP * vec4(worldPosition + normal * normalBias, 1.0);
    vec3 p = projected.xyz / projected.w * 0.5 + 0.5;
    if (any(lessThan(p, vec3(0.0))) || any(greaterThan(p, vec3(1.0)))) return 1.0;
    vec2 texel = 1.0 / vec2(textureSize(shadowMap, 0));
    float bias = depthBias * (1.0 + 3.0 * (1.0 - max(dot(normal, towardLight), 0.0)));
    float occlusion = 0.0;
    for (int x = -1; x <= 1; ++x) for (int y = -1; y <= 1; ++y) {
        vec3 depthColor = texture(shadowMap, p.xy + vec2(x, y) * texel).rgb;
        float depth = dot(depthColor, vec3(65536.0, 256.0, 1.0)) / 65793.0;
        occlusion += p.z - bias > depth ? 1.0 : 0.0;
    }
    return 1.0 - shadowStrength * occlusion / 9.0;
}
void main() {
    // Flat normals from world-position derivatives also support shared indexed meshes.
    vec3 normal = normalize(cross(dFdx(worldPosition), dFdy(worldPosition)));
    float diffuse = max(dot(normal, towardLight), 0.0);
    float visible = visibility(normal);
    vec4 base = surfaceColor();
    vec3 tint = (gl_FrontFacing == (faceOrientation > 0.0)) ? base.rgb : base.rgb * vec3(0.76, 0.79, 0.87);
    float shine = diffuse > 0.0 ? pow(max(dot(normalize(eye-worldPosition), reflect(-towardLight, normal)), 0.0), 32.0) : 0.0;
    vec3 lit = tint * (ambient + directional * diffuse * visible) + vec3(specular * shine * visible * 0.2);
    finalColor = vec4(clamp(lit, 0.0, 1.0), base.a);
}
