#version 330
in vec4 color;
noperspective in vec2 glyphUv;
noperspective in float distancePixels;
flat in int feedbackStyle;
out vec4 finalColor;
void main() {
    if(feedbackStyle==1 && mod(distancePixels,10.0)>6.0) discard;
    if(feedbackStyle==2 && dot(glyphUv,glyphUv)>1.0) discard;
    // A filled arrowhead pointing along endpointA -> endpointB.
    if(feedbackStyle==3 && abs(glyphUv.y)>(1.0-glyphUv.x)*0.5) discard;
    finalColor=color;
}
