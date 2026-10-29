#version 330
layout(location=0) in vec3 endpointA;
layout(location=1) in vec3 endpointB;
layout(location=3) in vec4 vertexColor;
layout(location=4) in vec4 instanceRow0;
layout(location=5) in vec4 instanceRow1;
layout(location=6) in vec4 instanceRow2;
layout(location=7) in vec4 instanceFlags;
uniform mat4 viewProjection;
uniform vec2 viewportSize;
uniform vec2 lineWidths;
uniform int blackLines;
// 0: ribbon; 1/2: endpoint; 3: midpoint; 4: direction arrow.
uniform int glyphMode;
out vec4 color;
noperspective out vec2 glyphUv;
noperspective out float distancePixels;
flat out int feedbackStyle;
flat out vec2 glyphExtent;
flat out float glyphScale;
vec4 project(vec3 point) {
    vec4 p=vec4(point,1.0);
    return viewProjection*vec4(dot(instanceRow0,p),dot(instanceRow1,p),dot(instanceRow2,p),1.0);
}
void main() {
    vec4 originalA=project(endpointA), originalB=project(endpointB);
    vec4 a=originalA,b=originalB;
    float da=a.z+a.w, db=b.z+b.w;
    color=vec4(0.0);glyphUv=vec2(0.0);distancePixels=0.0;feedbackStyle=0;glyphExtent=vec2(1.0);glyphScale=1.0;
    if (da<0.0 && db<0.0) {gl_Position=vec4(2.0,2.0,2.0,1.0);return;}
    if (da<0.0) a=mix(a,b,da/(da-db));
    else if (db<0.0) b=mix(b,a,db/(db-da));
    vec2 delta=(b.xy/b.w-a.xy/a.w)*viewportSize*0.5;
    float lengthPixels=length(delta);
    vec2 tangent=lengthPixels>0.00001 ? delta/lengthPixels:vec2(1.0,0.0);
    vec2 side=vec2(-tangent.y,tangent.x);
    int corner=gl_VertexID%6;
    float along=(corner==1 || corner==2 || corner==4) ? 1.0:0.0;
    float signSide=(corner==2 || corner==4 || corner==5) ? 1.0:-1.0;
    bool selected=instanceFlags.z>0.5;
    if(glyphMode>0) {
        // Use the real endpoints/midpoint, never a clipped replacement point.
        vec4 center=glyphMode==1?originalA:glyphMode==2?originalB:(originalA+originalB)*0.5;
        if(center.z+center.w<=0.0 || (glyphMode==4 && lengthPixels<32.0)) {
            gl_Position=vec4(2.0,2.0,2.0,1.0);return;
        }
        glyphUv=vec2(along*2.0-1.0,signSide);
        vec2 offset;
        if(glyphMode==4) {
            // A stroked shaft/head stays recognizable at ordinary viewport size.
            // Keep it next to the midpoint disk and inside shorter segments.
            glyphScale=clamp(lengthPixels/64.0,0.55,1.0);
            glyphExtent=vec2(9.0,6.0)*glyphScale+vec2(3.0);
            offset=tangent*(glyphUv.x*glyphExtent.x+12.0*glyphScale)+side*glyphUv.y*glyphExtent.y;
            feedbackStyle=3;
        } else {
            glyphExtent=vec2(4.5);
            offset=glyphUv*glyphExtent;
            feedbackStyle=2;
        }
        center.xy+=offset*2.0/viewportSize*center.w;
        gl_Position=center;
        color=glyphMode<=2?vec4(1.0,0.15,0.15,1.0):vec4(0.12,0.85,0.25,1.0);
        return;
    }
    float width=selected ? lineWidths.y:lineWidths.x;
    vec4 p=mix(a,b,along);
    p.xy+=side*signSide*width/viewportSize*p.w;
    gl_Position=p;
    distancePixels=along*lengthPixels;
    feedbackStyle=selected?1:0;
    color=selected ? vec4(1.0,0.15,0.15,1.0)
        : instanceFlags.y>0.5 ? vec4(130.0/255.0,147.0/255.0,161.0/255.0,1.0)
        : blackLines!=0 ? vec4(0.0,0.0,0.0,1.0) : vertexColor;
}
