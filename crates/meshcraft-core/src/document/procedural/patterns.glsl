float patternHash(vec3 p) { return fract(sin(dot(p,vec3(127.1,311.7,74.7))+float(u_seed)*0.137)*43758.5453); }
float patternNoise(vec3 p) {
    vec3 i=floor(p),f=fract(p);f=f*f*(3.0-2.0*f);
    return mix(mix(mix(patternHash(i),patternHash(i+vec3(1,0,0)),f.x),mix(patternHash(i+vec3(0,1,0)),patternHash(i+vec3(1,1,0)),f.x),f.y),mix(mix(patternHash(i+vec3(0,0,1)),patternHash(i+vec3(1,0,1)),f.x),mix(patternHash(i+vec3(0,1,1)),patternHash(i+vec3(1,1,1)),f.x),f.y),f.z);
}
// Derivative bump mapping works on arbitrary source triangles and transformed instances.
vec3 patternBump(vec3 n,float h) {
    vec3 dpdx=dFdx(worldPosition),dpdy=dFdy(worldPosition);
    vec3 r1=cross(dpdy,n),r2=cross(n,dpdx);
    float det=dot(dpdx,r1);
    if(abs(det)<1e-10)return n;
    return normalize(abs(det)*n-sign(det)*(dFdx(h)*r1+dFdy(h)*r2));
}
