// Tiles on the face's dominant plane. x: height, 0 in a groove and 1 on a tile
// face, with a bevel between; y: a per-tile random value in 0..1.
vec2 embossTileUv() {
    vec3 n=abs(patternCoordinates(sourceNormal));
    vec3 p=patternCoordinates(localPosition);
    if(n.y>n.x&&n.y>n.z)return p.xz;
    return n.x>n.z?p.zy:p.xy;
}
vec2 embossTiles() {
    vec2 size=max(vec2(u_tileWidth,u_tileHeight),vec2(0.0001)),p=embossTileUv()/size;
    p.x+=mod(floor(p.y),2.0)*u_stagger;
    vec2 cell=floor(p),edge=min(fract(p),1.0-fract(p))*size;
    float aa=max(length(fwidth(embossTileUv())),0.00001);
    float groove=u_groove*0.5;
    float h=smoothstep(groove,groove+max(u_bevel,aa),min(edge.x,edge.y));
    return vec2(h,patternHash(vec3(cell,7.0)));
}
