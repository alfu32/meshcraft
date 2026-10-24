vec4 proceduralColor(vec4 c,vec3 p,vec3 n,vec2 uv) {
    if(treePart>0.5) {
        vec2 q=vec2((uv.x-0.5)*2.0,uv.y*2.0-1.0);
        if(dot(q,q)>1.0)discard;
        float vein=1.0-0.15*exp(-abs(q.x)*35.0);
        return vec4(u_leafColor.rgb*vein*(0.75+0.25*fract(sin(treeKey*73.1)*43758.5)),u_leafColor.a);
    }
    float grooves=0.75+0.25*sin(uv.x*68.0+sin(uv.y*12.0)*2.0);
    return vec4(u_barkColor.rgb*grooves,u_barkColor.a);
}
