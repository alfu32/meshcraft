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
out vec4 color;
vec4 project(vec3 point) {
    vec4 p=vec4(point,1.0);
    return viewProjection*vec4(dot(instanceRow0,p),dot(instanceRow1,p),dot(instanceRow2,p),1.0);
}
void main() {
    vec4 a=project(endpointA), b=project(endpointB);
    float da=a.z+a.w, db=b.z+b.w;
    if (da<0.0 && db<0.0) {gl_Position=vec4(2.0,2.0,2.0,1.0);color=vec4(0.0);return;}
    if (da<0.0) a=mix(a,b,da/(da-db));
    else if (db<0.0) b=mix(b,a,db/(db-da));
    vec2 delta=(b.xy/b.w-a.xy/a.w)*viewportSize;
    float lengthPixels=length(delta);
    vec2 side=lengthPixels>0.00001 ? vec2(-delta.y,delta.x)/lengthPixels : vec2(0.0);
    int corner=gl_VertexID%6;
    float along=(corner==1 || corner==2 || corner==4) ? 1.0:0.0;
    float signSide=(corner==2 || corner==4 || corner==5) ? 1.0:-1.0;
    bool selected=instanceFlags.z>0.5;
    float width=selected ? lineWidths.y:lineWidths.x;
    vec4 p=mix(a,b,along);
    p.xy+=side*signSide*width/viewportSize*p.w;
    // Keep physical depth: a constant NDC bias exposes hidden edges in distant models.
    // The face pass already supplies a small eye-depth-aware fill offset.
    gl_Position=p;
    color=selected ? vec4(245.0/255.0,128.0/255.0,46.0/255.0,1.0)
        : instanceFlags.y>0.5 ? vec4(130.0/255.0,147.0/255.0,161.0/255.0,1.0)
        : blackLines!=0 ? vec4(0.0,0.0,0.0,1.0) : vertexColor;
}
