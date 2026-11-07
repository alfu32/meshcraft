// A simple studio environment for reflections: a sky gradient with two bright
// strips, a horizon and a darker ground, plus the scene light as a sun. Sharpness
// runs from 0 (soft, satin) to 1 (mirror).
vec3 environmentColor(vec3 r,float sharpness) {
    float edge=mix(0.25,0.004,sharpness);
    vec3 sky=mix(u_skyColor.rgb*1.2,u_skyColor.rgb*0.6,clamp(r.y,0.0,1.0));
    float strips=smoothstep(edge,-edge,abs(r.y-0.42)-0.05)+smoothstep(edge,-edge,abs(r.y-0.16)-0.025)*0.6;
    sky+=vec3(strips);
    vec3 ground=u_groundColor.rgb*(0.7+0.3*exp(r.y*6.0));
    vec3 env=mix(ground,sky,smoothstep(-edge,edge,r.y));
    float sun=pow(max(dot(r,normalize(towardLight)),0.0),mix(24.0,3000.0,sharpness));
    return env+vec3(sun*mix(1.5,8.0,sharpness));
}
// View direction, a normal facing the viewer, and the mirrored view direction.
void environmentFrame(vec3 position,vec3 normal,out vec3 v,out vec3 n,out vec3 r) {
    v=normalize(eye-position);
    n=dot(normal,v)<0.0?-normal:normal;
    r=reflect(-v,n);
}
