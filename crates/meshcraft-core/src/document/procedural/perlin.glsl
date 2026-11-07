// Classic 3D Perlin noise, after Stefan Gustavson's texture-free GLSL version
// (MIT licence). Returns roughly -1..1.
vec4 perlinMod289(vec4 x) { return x-floor(x*(1.0/289.0))*289.0; }
vec3 perlinMod289(vec3 x) { return x-floor(x*(1.0/289.0))*289.0; }
vec4 perlinPermute(vec4 x) { return perlinMod289(((x*34.0)+10.0)*x); }
vec4 perlinInvSqrt(vec4 r) { return 1.79284291400159-0.85373472095314*r; }
vec3 perlinFade(vec3 t) { return t*t*t*(t*(t*6.0-15.0)+10.0); }
float perlinNoise(vec3 P) {
    vec3 Pi0=perlinMod289(floor(P)),Pi1=perlinMod289(floor(P)+vec3(1.0));
    vec3 Pf0=fract(P),Pf1=Pf0-vec3(1.0);
    vec4 ix=vec4(Pi0.x,Pi1.x,Pi0.x,Pi1.x),iy=vec4(Pi0.yy,Pi1.yy);
    vec4 iz0=Pi0.zzzz,iz1=Pi1.zzzz;
    vec4 ixy=perlinPermute(perlinPermute(ix)+iy);
    vec4 ixy0=perlinPermute(ixy+iz0),ixy1=perlinPermute(ixy+iz1);
    vec4 gx0=ixy0*(1.0/7.0),gy0=fract(floor(gx0)*(1.0/7.0))-0.5;
    gx0=fract(gx0);
    vec4 gz0=vec4(0.5)-abs(gx0)-abs(gy0),sz0=step(gz0,vec4(0.0));
    gx0-=sz0*(step(0.0,gx0)-0.5);gy0-=sz0*(step(0.0,gy0)-0.5);
    vec4 gx1=ixy1*(1.0/7.0),gy1=fract(floor(gx1)*(1.0/7.0))-0.5;
    gx1=fract(gx1);
    vec4 gz1=vec4(0.5)-abs(gx1)-abs(gy1),sz1=step(gz1,vec4(0.0));
    gx1-=sz1*(step(0.0,gx1)-0.5);gy1-=sz1*(step(0.0,gy1)-0.5);
    vec3 g000=vec3(gx0.x,gy0.x,gz0.x),g100=vec3(gx0.y,gy0.y,gz0.y);
    vec3 g010=vec3(gx0.z,gy0.z,gz0.z),g110=vec3(gx0.w,gy0.w,gz0.w);
    vec3 g001=vec3(gx1.x,gy1.x,gz1.x),g101=vec3(gx1.y,gy1.y,gz1.y);
    vec3 g011=vec3(gx1.z,gy1.z,gz1.z),g111=vec3(gx1.w,gy1.w,gz1.w);
    vec4 norm0=perlinInvSqrt(vec4(dot(g000,g000),dot(g010,g010),dot(g100,g100),dot(g110,g110)));
    g000*=norm0.x;g010*=norm0.y;g100*=norm0.z;g110*=norm0.w;
    vec4 norm1=perlinInvSqrt(vec4(dot(g001,g001),dot(g011,g011),dot(g101,g101),dot(g111,g111)));
    g001*=norm1.x;g011*=norm1.y;g101*=norm1.z;g111*=norm1.w;
    float n000=dot(g000,Pf0),n100=dot(g100,vec3(Pf1.x,Pf0.yz));
    float n010=dot(g010,vec3(Pf0.x,Pf1.y,Pf0.z)),n110=dot(g110,vec3(Pf1.xy,Pf0.z));
    float n001=dot(g001,vec3(Pf0.xy,Pf1.z)),n101=dot(g101,vec3(Pf1.x,Pf0.y,Pf1.z));
    float n011=dot(g011,vec3(Pf0.x,Pf1.yz)),n111=dot(g111,Pf1);
    vec3 f=perlinFade(Pf0);
    vec4 nz=mix(vec4(n000,n100,n010,n110),vec4(n001,n101,n011,n111),f.z);
    vec2 nyz=mix(nz.xy,nz.zw,f.y);
    return 2.2*mix(nyz.x,nyz.y,f.x);
}
