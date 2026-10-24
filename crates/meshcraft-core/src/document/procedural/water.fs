vec4 proceduralColor(vec4 c,vec3 p,vec3 n,vec2 uv) {
    vec3 v=normalize(eye-p);float fresnel=0.02+0.98*pow(1.0-abs(dot(n,v)),5.0);
    vec3 reflected=reflect(-v,n);
    vec3 sky=mix(vec3(0.30,0.48,0.62),vec3(0.76,0.88,0.98),clamp(reflected.y*0.5+0.5,0.0,1.0));
    float ripple=0.5+0.5*sin(localPosition.x*2.3+localPosition.z*1.7+time*u_speed);
    vec3 water=mix(u_deepColor.rgb,u_shallowColor.rgb,ripple*0.45+0.2);
    float foam=u_foam*smoothstep(0.70,0.98,1.0-abs(n.y));
    return vec4(mix(mix(water,sky,fresnel),vec3(0.92,0.98,1),foam),u_opacity);
}
