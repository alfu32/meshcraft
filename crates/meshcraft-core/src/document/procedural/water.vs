float waterWave(vec2 p, float t) {
    float a=radians(u_direction),k=6.2831853/u_wavelength;
    vec2 d=vec2(cos(a),sin(a)),e=vec2(-sin(a+0.7),cos(a+0.7));
    return sin(dot(p,d)*k-t)+0.35*sin(dot(p,e)*k*1.73+t*1.21);
}
void proceduralVertex(inout vec3 p,inout vec3 n,vec2 uv) {
    vec3 ref=abs(n.y)<0.9?vec3(0,1,0):vec3(1,0,0);
    vec3 u=normalize(cross(ref,n)),v=cross(n,u);
    vec2 q=vec2(dot(p,u),dot(p,v));float t=time*u_speed;
    float a=radians(u_direction),k=6.2831853/u_wavelength;
    vec2 d=vec2(cos(a),sin(a)),e=vec2(-sin(a+0.7),cos(a+0.7));
    vec2 grad=u_amplitude*k*(cos(dot(q,d)*k-t)*d+0.35*1.73*cos(dot(q,e)*k*1.73+t*1.21)*e);
    p+=n*(u_offset+u_amplitude*waterWave(q,t));n=normalize(n-u*grad.x-v*grad.y);
}
