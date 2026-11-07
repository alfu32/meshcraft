// Rough texture: the surface lighting follows a noise height field; faces stay flat.
vec3 proceduralShadingNormal(vec3 n,vec3 p,vec2 uv) {
    return patternBump(n,embossRough()*u_roughDepth);
}
vec4 proceduralColor(vec4 c,vec3 p,vec3 n,vec2 uv) {
    return vec4(c.rgb*mix(1.0-u_hollowShade,1.0,embossRough()),c.a);
}
