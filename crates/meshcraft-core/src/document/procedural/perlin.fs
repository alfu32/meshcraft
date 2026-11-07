// Perlin noise: fractal Perlin noise in 0..1, sampled in the surface's own space.
float perlinPattern() {
    vec3 p=localPosition/max(u_featureSize,0.0001)+vec3(13.1,7.7,3.3)*mod(float(u_seed),97.0);
    float h=0.0,a=0.5,total=0.0;
    for(int i=0;i<8;i++){
        if(i>=u_octaves)break;
        h+=a*perlinNoise(p);total+=a;p=p*2.0+vec3(1.7,9.2,3.1);a*=0.5;
    }
    return clamp(0.5+0.5*u_contrast*h/max(total,0.0001),0.0,1.0);
}
vec3 proceduralShadingNormal(vec3 n,vec3 p,vec2 uv) {
    if(u_relief<=0.0)return n;
    return patternBump(n,perlinPattern()*u_relief);
}
vec4 proceduralColor(vec4 c,vec3 p,vec3 n,vec2 uv) {
    vec4 shade=mix(u_lowColor,u_highColor,perlinPattern());
    return vec4(shade.rgb*c.rgb,shade.a*c.a);
}
