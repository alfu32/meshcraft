vec2 brickUv() {
    vec3 n=abs(sourceNormal);
    if(n.y>n.x&&n.y>n.z)return localPosition.xz;
    return n.x>n.z?localPosition.zy:localPosition.xy;
}
vec3 brickPattern() {
    vec2 size=vec2(u_brickWidth,u_brickHeight),p=brickUv()/size;
    float row=floor(p.y);p.x+=mod(row,2.0)*u_stagger;
    vec2 cell=floor(p),f=fract(p),edge=min(f,1.0-f)*size;
    float gap=u_mortar*min(size.x,size.y)*0.5;
    float aa=max(length(fwidth(brickUv())),0.00001);
    float inside=smoothstep(gap-aa,gap+aa,min(edge.x,edge.y));
    float variation=patternHash(vec3(cell,float(u_seed)));
    return vec3(inside,variation,patternNoise(localPosition*85.));
}
vec3 proceduralShadingNormal(vec3 n,vec3 p,vec2 uv) {
    vec3 b=brickPattern();return patternBump(n,u_relief*(b.x*0.035+b.z*0.002));
}
vec4 proceduralColor(vec4 c,vec3 p,vec3 n,vec2 uv) {
    vec3 b=brickPattern();vec3 brick=u_brickColor.rgb*(1.0-u_variation*b.y)*(0.92+0.08*b.z);
    return vec4(mix(u_mortarColor.rgb,brick,b.x),c.a*mix(u_mortarColor.a,u_brickColor.a,b.x));
}
