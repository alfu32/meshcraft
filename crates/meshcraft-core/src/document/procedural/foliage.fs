vec4 proceduralColor(vec4 c, vec3 p, vec3 n, vec2 uv) {
    // Broad pointed leaf, cut from a curled strip. Outline narrows to both tips.
    float outline=pow(max(sin(uv.y*3.14159265),0.0),0.70);
    float edge=abs(uv.x)-outline;
    float aa=max(fwidth(edge),0.001);
    if(edge>0.0)discard;
    float center=1.0-smoothstep(0.015,0.045,abs(uv.x));
    float vein=1.0-smoothstep(0.025,0.075,abs(fract(uv.y*8.0-abs(uv.x)*1.8)-0.5));
    vec4 leaf=mix(u_leafColor,u_alternateColor,variationKey);
    leaf.rgb*=c.rgb*(0.87+0.08*vein+0.12*center)*(0.85+0.15*uv.y);
    leaf.a*=c.a*smoothstep(0.0,aa,-edge);
    return leaf;
}
