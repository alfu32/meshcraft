#version 330
in vec4 color;
noperspective in vec2 glyphUv;
noperspective in float distancePixels;
flat in int feedbackStyle;
flat in vec2 glyphExtent;
flat in float glyphScale;
out vec4 finalColor;
float segmentDistance(vec2 p,vec2 a,vec2 b) {
    vec2 ab=b-a;
    return length(p-a-ab*clamp(dot(p-a,ab)/dot(ab,ab),0.0,1.0));
}
void main() {
    if(feedbackStyle==1 && mod(distancePixels,10.0)>6.0) discard;
    if(feedbackStyle>=2) {
        vec2 p=glyphUv*glyphExtent;
        float d;
        if(feedbackStyle==2) {
            d=length(p)-2.8;
        } else {
            float scale=glyphScale;
            d=min(segmentDistance(p,vec2(-9,0)*scale,vec2(7,0)*scale),
                min(segmentDistance(p,vec2(-1,6)*scale,vec2(7,0)*scale),
                    segmentDistance(p,vec2(-1,-6)*scale,vec2(7,0)*scale)))-1.2;
        }
        // A dark keyline and white halo separate glyphs from red linework,
        // bright faces and dark materials. Distances are physical screen pixels.
        float aa=max(fwidth(d)*0.5,0.35);
        float coverage=1.0-smoothstep(1.45-aa,1.45+aa,d);
        if(coverage<=0.0) discard;
        vec3 ink=mix(color.rgb,vec3(0.07,0.10,0.12),smoothstep(-aa,aa,d));
        ink=mix(ink,vec3(1.0),smoothstep(0.65-aa,0.65+aa,d));
        finalColor=vec4(ink,color.a*coverage);
        return;
    }
    finalColor=color;
}
