// Read-only: the complete code Meshcraft compiles for the viewport.
// Your Vertex and Fragment tab code is inserted into this host template,
// which adds the camera, lighting, shadows and parameter uniforms.

// ===== Vertex shader =====
#version 330

layout(location=0) in vec3 domainA;
layout(location=1) in vec3 domainB;
layout(location=2) in vec3 domainC;
layout(location=3) in vec2 domainSeed;
uniform mat4 viewProjection;
uniform mat4 model;
uniform int bladesPerTriangle;
uniform int generation;
uniform float time;
uniform float density;
uniform vec4 baseColor;
out vec3 worldPosition;
out vec3 localPosition;
out vec3 sourceNormal;
uniform int customVertexCount; uniform int customInstanceCount;
out vec4 color;
out vec2 proceduralUv;
out vec2 materialUv;
out vec3 proceduralNormal;
flat out float faceOrientation;
flat out float variationKey;
uniform vec3 u_size;
const float u_height = 0.0;
const float u_width = 0.0;
const float u_wind = 0.0;
const float u_curvature = 0.0;
const float u_variation = 0.0;
const uint u_seed = 0u;

// Called once per emitted vertex. Every three consecutive vertices form a triangle.
//   vertexId:   0 .. vertexCount-1 within one instance (vertexCount is 36 here)
//   instanceId: 0 .. instances-1
// Write the local position, normal and uv. Parameters are uniforms named u_<id>;
// customVertexCount, customInstanceCount and time (seconds) are also available.
void customVertex(int vertexId, int instanceId, out vec3 position, out vec3 normal, out vec2 uv) {
    const vec3 faces[6] = vec3[](vec3(1, 0, 0), vec3(-1, 0, 0), vec3(0, 1, 0),
                                 vec3(0, -1, 0), vec3(0, 0, 1), vec3(0, 0, -1));
    const vec2 corners[6] = vec2[](vec2(0, 0), vec2(1, 0), vec2(1, 1),
                                   vec2(0, 0), vec2(1, 1), vec2(0, 1));
    normal = faces[vertexId / 6];
    uv = corners[vertexId % 6];
    vec3 u = abs(normal.y) > 0.5 ? vec3(1, 0, 0) : vec3(0, 1, 0);
    vec3 v = cross(normal, u);
    vec2 s = uv * 2.0 - 1.0;
    position = (normal + u * s.x + v * s.y) * 0.5 + vec3(0, 0.5, 0);
    position *= u_size;
}

void proceduralVertex(inout vec3 position,inout vec3 normal,vec2 uv){}
uint hash(uint x) { x ^= x >> 16u; x *= 0x7feb352du; x ^= x >> 15u; x *= 0x846ca68bu; return x ^ (x >> 16u); }
float random(uint x) { return float(hash(x) & 0x00ffffffu) / 16777216.0; }
void main() {
    vec3 normal = normalize(cross(domainB-domainA,domainC-domainA));
    vec3 position;
    vec2 uv = vec2(0.0);
    color = baseColor;variationKey=0.0;
    sourceNormal=normal;localPosition=domainA;
    if(generation==5){customVertex(gl_VertexID,gl_InstanceID,position,normal,uv);materialUv=uv;}else
    if (generation == 1 || generation == 4) {
        uint candidate = uint(gl_InstanceID % bladesPerTriangle);
        uint key = hash(uint(domainSeed.x) | (uint(domainSeed.y) << 16u)) ^ hash(candidate) ^ uint(u_seed);
        vec3 wa=(model*vec4(domainA,1.0)).xyz, wb=(model*vec4(domainB,1.0)).xyz, wc=(model*vec4(domainC,1.0)).xyz;
        float area=length(cross(wb-wa,wc-wa))*0.5;
        // Fractional last blade uses a stable sample, so density varies smoothly.
        if (float(candidate)+random(key+3u) >= area*density) {
            gl_Position=vec4(2.0,2.0,2.0,1.0);worldPosition=vec3(0.0);proceduralUv=uv;materialUv=uv;proceduralNormal=normal;faceOrientation=1.0;return;
        }
        float r=sqrt(random(key+1u)), s=random(key+2u);
        vec3 root=domainA*(1.0-r)+domainB*(r*(1.0-s))+domainC*(r*s);
        vec3 tangent=normalize(domainB-domainA), bitangent=normalize(cross(normal,tangent));
        float angle=random(key+4u)*6.28318530718;
        vec3 side=tangent*cos(angle)+bitangent*sin(angle), bend=cross(normal,side);
        int segment=gl_VertexID/6;
        int corner=gl_VertexID%6;
        vec2 pattern[6]=vec2[](vec2(-1,0),vec2(1,0),vec2(1,1),vec2(-1,0),vec2(1,1),vec2(-1,1));
        uv=vec2(pattern[corner].x,(float(segment)+pattern[corner].y)/3.0);
        variationKey=random(key+6u);
#ifdef MESHCRAFT_FOLIAGE
        {
            float offsetAngle=random(key+7u)*6.28318530718;
            root+=(tangent*cos(offsetAngle)+bitangent*sin(offsetAngle))*(u_spread*sqrt(random(key+8u)));
            root+=normal*(u_height*(0.35+0.65*random(key+9u)));
            float length=u_size*(0.7+0.6*random(key+5u));
            float tilt=radians(u_tilt)*(0.7+0.6*random(key+10u));
            vec3 forward=bend*cos(tilt)+normal*sin(tilt);
            float wind=u_wind*sin(time*1.7+root.x*0.5+root.z*0.7+angle);
            position=root+forward*(uv.y*length)+side*(uv.x*u_width*0.5)
                +normal*(uv.y*uv.y*length*(u_curl+wind)+abs(uv.x)*u_width*0.10);
            normal=normalize(cross(side,forward));
            materialUv=vec2(uv.x*u_width*0.5,uv.y*length);
            color.rgb=vec3(1.0-u_variation*variationKey);
        }
#else
        {
        float h=u_height*(0.7+0.6*random(key+5u));
        float wind=u_wind*sin(time*1.7+root.x*0.5+root.z*0.7+angle);
        position=root+normal*(uv.y*h)+side*(uv.x*u_width*0.5*(1.0-uv.y))+bend*(uv.y*uv.y*h*(u_curvature+wind));
        materialUv=vec2(uv.x*u_width*0.5,uv.y*h);
        color.rgb*=1.0-u_variation*random(key+6u);
        color.rgb*=0.65+0.35*uv.y;
        }
#endif
    } else {
        position=gl_VertexID==0?domainA:(gl_VertexID==1?domainB:domainC);
        if(generation==3) {
            int n=int(round(sqrt(float(bladesPerTriangle)))),k=gl_InstanceID%bladesPerTriangle,row=0;
            for(int i=0;i<64;i++){int cells=2*(n-i)-1;if(k<cells){row=i;break;}k-=cells;}
            int col=k/2;bool flip=(k%2)==1;vec2 q=vec2(row,col);
            if(!flip)q+=gl_VertexID==0?vec2(0):gl_VertexID==1?vec2(1,0):vec2(0,1);
            else q+=gl_VertexID==0?vec2(1,0):gl_VertexID==1?vec2(1,1):vec2(0,1);
            q/=float(n);position=domainA+(domainB-domainA)*q.x+(domainC-domainA)*q.y;
        }
        uv=gl_VertexID==0?vec2(0,0):(gl_VertexID==1?vec2(1,0):vec2(0,1));
        vec3 tangent=normalize(domainB-domainA);materialUv=vec2(dot(position-domainA,tangent),dot(position-domainA,cross(normal,tangent)));
    }
    localPosition=position;
    proceduralVertex(position,normal,uv);
    worldPosition=(model*vec4(position,1.0)).xyz;
    proceduralUv=uv;
    proceduralNormal=normalize(transpose(inverse(mat3(model)))*normal);
    faceOrientation=determinant(mat3(model))<0.0?-1.0:1.0;
    gl_Position=viewProjection*vec4(worldPosition,1.0);
}


// ===== Fragment shader =====
#version 330
in vec3 worldPosition;
in vec3 localPosition;
in vec3 sourceNormal;

in vec4 color;
in vec2 proceduralUv;
in vec2 materialUv;
in vec3 proceduralNormal;
uniform int generation;
flat in float faceOrientation;
flat in float variationKey;
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
uniform vec3 u_size;
const float u_height = 0.0;
const float u_width = 0.0;
const float u_wind = 0.0;
const float u_curvature = 0.0;
const float u_variation = 0.0;
const uint u_seed = 0u;

vec3 proceduralShadingNormal(vec3 n,vec3 p,vec2 uv){return n;}
// Optional. Returns the surface color; color is the object's material color and
// position/normal are in world space. Remove this function to keep the color.
vec4 proceduralColor(vec4 color, vec3 position, vec3 normal, vec2 uv) {
    return color;
}

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
    vec3 normal=generation!=1&&generation!=4?normalize(proceduralNormal):normalize(cross(dFdx(worldPosition),dFdy(worldPosition)));
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

