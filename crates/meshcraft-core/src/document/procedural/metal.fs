// Polished metal: the surface mirrors the environment, tinted by the material
// colour as real metals are, with Fresnel brightening at grazing angles.
vec3 proceduralLighting(vec3 lit,vec4 color,vec3 position,vec3 normal) {
    vec3 v,n,r;
    environmentFrame(position,normal,v,n,r);
    vec3 f0=color.rgb;
    vec3 fresnel=f0+(1.0-f0)*pow(1.0-max(dot(n,v),0.0),5.0);
    return mix(lit,environmentColor(r,u_polish)*fresnel,u_reflectivity);
}
