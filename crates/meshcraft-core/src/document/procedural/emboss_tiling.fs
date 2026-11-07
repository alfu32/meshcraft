// Tiling: raised tiles with bevelled grooves. The tiles take the material colour.
vec3 proceduralShadingNormal(vec3 n,vec3 p,vec2 uv) {
    return patternBump(n,embossTiles().x*u_tileDepth);
}
vec4 proceduralColor(vec4 c,vec3 p,vec3 n,vec2 uv) {
    vec2 t=embossTiles();
    vec3 tile=c.rgb*(1.0+u_variation*(t.y-0.5));
    return vec4(mix(u_grooveColor.rgb,tile,t.x),c.a);
}
