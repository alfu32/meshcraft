// Inverse rotation maps source-local coordinates into the oriented pattern frame.
// Pattern orientation is Rv * Ru; defaults preserve existing material appearance.
vec3 patternCoordinates(vec3 p) {
    vec2 a=radians(vec2(u_rotationU,u_rotationV));
    float cu=cos(a.x),su=sin(a.x),cv=cos(a.y),sv=sin(a.y);
    p=vec3(cv*p.x-sv*p.z,p.y,sv*p.x+cv*p.z);
    return vec3(p.x,cu*p.y+su*p.z,-su*p.y+cu*p.z);
}
