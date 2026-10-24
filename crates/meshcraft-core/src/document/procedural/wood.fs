float woodPattern() {
    vec3 p=u_axis==0?localPosition.yxz:(u_axis==2?localPosition.xzy:localPosition);
    float warp=patternNoise(p*vec3(1.5,0.18,1.5))*u_turbulence;
    float rings=length(p.xz+vec2(warp,-warp*0.7))*u_ringScale;
    float bands=0.5+0.5*sin(rings*6.2831853);
    float fibers=patternNoise(p*vec3(u_grain,0.6,u_grain));
    return clamp(pow(bands,4.0)*0.72+fibers*0.28,0.0,1.0);
}
vec3 proceduralShadingNormal(vec3 n,vec3 p,vec2 uv) { return patternBump(n,woodPattern()*u_relief*0.012); }
vec4 proceduralColor(vec4 c,vec3 p,vec3 n,vec2 uv) { return vec4(mix(u_lightColor.rgb,u_darkColor.rgb,woodPattern()),c.a*mix(u_lightColor.a,u_darkColor.a,woodPattern())); }
