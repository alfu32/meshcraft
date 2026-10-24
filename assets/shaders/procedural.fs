#version 330
in vec3 worldPosition;
in vec3 localPosition;
in vec3 sourceNormal;
/*TREE_INPUTS*/
in vec4 color;
in vec2 proceduralUv;
in vec2 materialUv;
in vec3 proceduralNormal;
uniform int generation;
flat in float faceOrientation;
uniform float time;
uniform vec3 towardLight;
uniform vec3 eye;
uniform float ambient;
uniform float directional;
uniform float specular;
uniform int depthPass;
uniform int externalGeometry;
uniform mat4 lightVP;
uniform sampler2D shadowMap;
uniform int shadows;
uniform float shadowStrength;
uniform float depthBias;
uniform float normalBias;
uniform int pbrMode;
uniform float roughness;
uniform float metallic;
uniform sampler2D baseTexture;
uniform int hasBaseTexture;
uniform vec2 textureSizes;
out vec4 finalColor;
/*PARAMETERS*/
/*FRAGMENT*/
float visibility(vec3 normal) {
    if (shadows==0) return 1.0;
    vec4 p4=lightVP*vec4(worldPosition+normal*normalBias,1.0);
    vec3 p=p4.xyz/p4.w*0.5+0.5;
    if(any(lessThan(p,vec3(0.0)))||any(greaterThan(p,vec3(1.0)))) return 1.0;
    vec2 texel=1.0/vec2(textureSize(shadowMap,0));float occlusion=0.0;
    for(int x=-1;x<=1;x++) for(int y=-1;y<=1;y++) {
        float depth=dot(texture(shadowMap,p.xy+vec2(x,y)*texel).rgb,vec3(65536,256,1))/65793.0;
        occlusion+=p.z-depthBias>depth?1.0:0.0;
    }
    return 1.0-shadowStrength*occlusion/9.0;
}
void main() {
    vec3 normal=generation!=1?normalize(proceduralNormal):normalize(cross(dFdx(worldPosition),dFdy(worldPosition)));
    normal=proceduralShadingNormal(normal,worldPosition,proceduralUv);
    vec4 c=proceduralColor(color,worldPosition,normal,proceduralUv);
    if(hasBaseTexture!=0) c*=texture(baseTexture,materialUv/max(textureSizes,vec2(0.0001)));
    if(c.a<0.01) discard;
    if(depthPass!=0) {
        if(c.a<0.99) discard;
        float depth=floor(min(gl_FragCoord.z,0.99999994)*16777215.0);
        finalColor=vec4(floor(depth/65536.0),mod(floor(depth/256.0),256.0),mod(depth,256.0),255.0)/255.0;return;
    }
    float diffuse=abs(dot(normal,towardLight));
    float visible=visibility(normal);
    float shine=pow(max(dot(normalize(eye-worldPosition),reflect(-towardLight,normal)),0.0),32.0);
    vec3 lit=c.rgb*(ambient+directional*diffuse*visible)+vec3(specular*shine*visible*0.2);
    if(pbrMode!=0) {
        const float PI=3.14159265359;
        vec3 n=gl_FrontFacing?normal:-normal;
        vec3 v=normalize(eye-worldPosition),l=normalize(towardLight),h=normalize(v+l);
        float nv=max(dot(n,v),0.001),nl=max(dot(n,l),0.0),nh=max(dot(n,h),0.0),vh=max(dot(v,h),0.0);
        float a=max(roughness,0.04);a=a*a;float a2=a*a;
        float denom=nh*nh*(a2-1.0)+1.0;float distribution=a2/(PI*denom*denom);
        float k=(roughness+1.0)*(roughness+1.0)/8.0;
        float geometry=(nv/(nv*(1.0-k)+k))*(nl/(nl*(1.0-k)+k));
        vec3 f0=mix(vec3(0.04),c.rgb,metallic);vec3 fresnel=f0+(1.0-f0)*pow(1.0-vh,5.0);
        vec3 reflected=distribution*geometry*fresnel/(4.0*nv*max(nl,0.001));
        vec3 diffuseColor=(1.0-fresnel)*(1.0-metallic)*c.rgb/PI;
        lit=c.rgb*ambient+(diffuseColor+reflected)*PI*directional*nl*visible;
    }
    if(externalGeometry!=0) lit=mix(lit,vec3(0.65,0.76,0.86),0.6);
    finalColor=vec4(clamp(lit,0.0,1.0),c.a);
}
