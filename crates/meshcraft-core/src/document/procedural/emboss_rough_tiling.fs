// Rough tiling: raised, bevelled tiles whose faces and grooves are both rough.
vec3 proceduralShadingNormal(vec3 n,vec3 p,vec2 uv) {
    return patternBump(n,embossTiles().x*u_tileDepth+embossRough()*u_roughDepth);
}
vec4 proceduralColor(vec4 c,vec3 p,vec3 n,vec2 uv) {
    vec2 t=embossTiles();
    vec3 tile=c.rgb*(1.0+u_variation*(t.y-0.5));
    vec3 base=mix(u_grooveColor.rgb,tile,t.x);
    return vec4(base*mix(1.0-u_hollowShade,1.0,embossRough()),c.a);
}
