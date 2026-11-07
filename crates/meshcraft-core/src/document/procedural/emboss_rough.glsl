// Rough emboss height in 0..1: four octaves of value noise in the surface's own
// space, so the grain moves with the object.
float embossRough() {
    vec3 p=localPosition/max(u_grainSize,0.0001);
    float h=0.0,a=0.5;
    for(int i=0;i<4;i++){h+=a*patternNoise(p);p=p*2.03+1.7;a*=0.5;}
    return h/0.9375;
}
