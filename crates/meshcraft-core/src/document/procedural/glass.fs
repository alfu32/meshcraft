// Clear glass: mostly see-through when viewed head-on, reflective at grazing
// angles. The material colour tints the glass.
float glassFresnel(vec3 position,vec3 normal) {
    vec3 v,n,r;
    environmentFrame(position,normal,v,n,r);
    return 0.04+0.96*pow(1.0-max(dot(n,v),0.0),5.0);
}
vec4 proceduralColor(vec4 c,vec3 p,vec3 n,vec2 uv) {
    float f=glassFresnel(p,n)*u_reflectivity;
    return vec4(c.rgb,clamp(mix(u_opacity,1.0,f),0.0,1.0));
}
vec3 proceduralLighting(vec3 lit,vec4 color,vec3 position,vec3 normal) {
    vec3 v,n,r;
    environmentFrame(position,normal,v,n,r);
    float f=glassFresnel(position,normal)*u_reflectivity;
    vec3 env=environmentColor(r,u_polish);
    float sun=pow(max(dot(r,normalize(towardLight)),0.0),mix(64.0,3000.0,u_polish));
    return mix(lit,env,f)+vec3(sun*u_reflectivity*2.0);
}
