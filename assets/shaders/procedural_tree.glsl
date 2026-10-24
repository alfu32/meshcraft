// The CPU supplies one object domain. All cylinders and leaf cards are GPU vertices.
uniform int treeBranches;
uniform int treeLeafCount;
uniform int treePass;
flat out float treePart;
flat out float treeKey;
int levelCount(int depth) {int n=u_branches;for(int i=1;i<depth;i++)n*=3;return n;}
void branch(int id,out vec3 start,out vec3 end,out float radius) {
    start=vec3(0);end=vec3(0,u_height*0.65,0);radius=u_trunkRadius;
    if(id==0)return;
    int path[4];int count=0,index=id;
    for(int i=0;i<4;i++) {path[count++]=index;if(index<=u_branches)break;index=(index-1-u_branches)/3+1;}
    for(int i=3;i>=0;i--) {
        if(i>=count)continue;
        uint k=hash(uint(path[i])+uint(u_seed)*137u);
        float a=random(k+1u)*6.2831853;
        vec3 parentStart=start,parentEnd=end;
        if(i==count-1) {
            start=mix(parentStart,parentEnd,0.45+0.5*random(k+2u));
            end=vec3(cos(a)*u_radius*0.58,u_height*(0.72+0.13*random(k+3u)),sin(a)*u_radius*0.58);
            radius=u_trunkRadius*0.45;
        } else {
            start=mix(parentStart,parentEnd,0.6+0.3*random(k+2u));
            float reach=u_radius*pow(0.5,float(count-i));
            end=parentEnd+vec3(cos(a)*reach,u_height*(0.045+0.03*random(k+3u)),sin(a)*reach);
            float radial=length(end.xz);if(radial>u_radius)end.xz*=u_radius/radial;
            end.y=min(end.y,u_height);radius*=0.48;
        }
    }
}
vec3 sway(vec3 p) {float f=clamp(p.y/u_height,0.0,1.0);p.x+=sin(time*1.1+float(u_seed)*0.7+p.y*0.3)*u_height*u_wind*0.6*f*f;p.z+=cos(time*0.9+p.x*0.5)*u_height*u_wind*0.25*f*f;return p;}
void treeVertex(out vec3 p,out vec3 normal,out vec2 uv) {
    int id=gl_InstanceID+(treePass==1?treeBranches:0);bool leaf=treePass==1;
    treePart=leaf?1.0:0.0;treeKey=float(id)+float(u_seed);
    int branchId=id;
    uint key=hash(uint(id)^uint(u_seed)*137u);
    if(leaf) {
        int terminal=levelCount(u_levels);
        int first=treeBranches-terminal;
        branchId=first+int(random(key+1u)*float(terminal));
    }
    vec3 a,b;float radius;branch(branchId,a,b,radius);
    vec3 axis=normalize(b-a),u=normalize(cross(axis,abs(axis.y)>0.9?vec3(1,0,0):vec3(0,1,0))),v=cross(axis,u);
    vec2 corners[6]=vec2[](vec2(0,0),vec2(1,0),vec2(1,1),vec2(0,0),vec2(1,1),vec2(0,1));
    if(leaf) {
        uv=corners[gl_VertexID%6];
        float angle=random(key+2u)*6.2831853;
        vec3 root=mix(a,b,0.2+0.8*random(key+3u));
        root+=vec3(random(key+4u)-0.5,random(key+5u)-0.5,random(key+6u)-0.5)*u_leafSize*3.0;
        vec3 side=vec3(cos(angle),0,sin(angle));
        vec3 up=normalize(vec3(-sin(angle)*0.6,0.6+random(key+7u),cos(angle)*0.6));
        float size=u_leafSize*(0.7+random(key+8u)*0.6);
        p=sway(root+(uv.x-0.5)*size*side+uv.y*size*up);
        normal=normalize(cross(side,up));
    } else {
        int sector=gl_VertexID/6;uv=corners[gl_VertexID%6];
        float angle=(float(sector)+uv.x)*6.2831853/8.0;
        vec3 radial=u*cos(angle)+v*sin(angle);
        p=sway(mix(a,b,uv.y)+radial*radius*mix(1.0,0.45,uv.y));
        normal=radial;uv.x=(float(sector)+uv.x)/8.0;uv.y*=length(b-a);
    }
}
