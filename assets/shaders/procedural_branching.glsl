// Stateless conditional IFS: each instance reconstructs only its own ancestry.
// No branch positions, connectivity, triangles or leaf positions are uploaded.
uniform int treePass;
uniform float branchPixelScale; // pixels per local unit; 0 = full-detail shadow/bake
#ifdef BRANCHING_CACHED
uniform samplerBuffer branchStates;
#endif

vec3 branchUnit(vec3 p, vec3 fallback) {
    return dot(p,p)>1e-12 ? normalize(p) : fallback;
}
vec3 branchEnvelope(vec3 p) {
    float r=length(p.xz);
    if(r>u_radius)p.xz*=u_radius/r;
    p.y=clamp(p.y,0.0,u_height);
    return p;
}
vec3 branchCurve(vec3 a,vec3 c,vec3 b,float t) {
    return mix(mix(a,c,t),mix(c,b,t),t);
}
vec3 branchTangent(vec3 a,vec3 c,vec3 b,float t) {
    return branchUnit(mix(c-a,b-c,t),vec3(0,1,0));
}
// Breadth-first ordinal -> child slots. Surviving slot paths don't change when
// sibling counts, camera/LOD or budgets change. Six levels bound the GPU loop.
int branchPath(int ordinal,out int slots[6]) {
    if(ordinal==0)return 0;
    int first=1,width=clamp(u_branches,1,12),depth=1;
    for(int k=1;k<6;k++) {
        if(ordinal<first+width || depth>=u_levels)break;
        first+=width;width*=clamp(u_forks,1,8);depth++;
    }
    int local=ordinal-first;
    for(int k=5;k>=0;k--) {
        if(k>=depth)continue;
        slots[k]=local%clamp(u_forks,1,8);local/=clamp(u_forks,1,8);
    }
    // The first level has its own arity, not u_forks.
    slots[0]+=local*clamp(u_forks,1,8);
    return depth;
}
void branchEvaluate(int ordinal,out vec3 a,out vec3 c,out vec3 b,
                    out vec3 frame,out float radius,out float presence,out uint key) {
    int slots[6];int depth=branchPath(ordinal,slots);
    uvec2 path=uvec2(1u,0u);key=hash(u_seed^path.x);
    float stage=1.0/float(clamp(u_levels,0,6)+1);
    presence=smoothstep(0.0,stage,u_growth);
    float trunk=u_form==1?0.82:(u_form==4?0.25:(u_form==2 || u_form==3?0.43:0.60));
    a=vec3(0);b=vec3(0,u_height*trunk*presence,0);
    frame=vec3(1,0,0);c=(a+b)*0.5;
    radius=u_trunkRadius*presence;
    for(int level=1;level<=6;level++) {
        if(level>depth)break;
        if(presence<1e-6)break;
        int slot=slots[level-1];
        path=uvec2((path.x<<4u)|uint(slot+1),(path.y<<4u)|(path.x>>28u));
        key=hash(path.x^hash(path.y)^u_seed*137u);
        float jitter=(random(key+1u)-0.5)*u_variation;
        BranchRule rule;
        rule.parentStart=a;rule.parentControl=c;rule.parentEnd=b;rule.parentFrame=frame;rule.parentRadius=radius;
        rule.attach=level==1?mix(0.25,u_attachment,fract(float(slot)*0.618034+random(key+2u)*0.16)):u_attachment;
        rule.angle=radians(u_angle)*(1.0+jitter*0.7);
        rule.azimuth=radians(u_twist)*float(slot)+float(level)*2.399963+random(key+3u)*u_variation*2.0;
        rule.lengthScale=u_lengthRatio*(1.0+jitter*0.5);
        rule.radiusScale=u_radiusRatio;
        rule.bend=u_bend;rule.tropism=u_gravity;
        float score=mix(0.04,0.96,random(key+4u));
        rule.activation=smoothstep(score-0.04,score+0.04,u_branchDensity)
            *smoothstep(float(level)*stage,float(level+1)*stage,u_growth);
        // Families specialize rules, not the evaluator or its topology storage.
        if(u_form==1) { // conifer: whorls, a tall leader and a tapered crown
            if(level==1) {
                rule.attach=mix(0.12,0.96,float(slot)/float(max(u_branches-1,1)));
                rule.angle=radians(77.0)+jitter*0.4;
                rule.lengthScale*=1.15*(1.0-rule.attach)+0.08;
            }
            rule.tropism*=0.25;rule.bend*=0.4;
        } else if(u_form==2) { // coral: repeated upright bifurcations, no leaf cards
            rule.attach=mix(rule.attach,1.0,0.85);rule.angle*=0.65;
            rule.lengthScale*=0.82;rule.tropism+=0.35;
        } else if(u_form==3) { // roots: inverted after the common evaluation
            rule.angle=level==1?radians(65.0)+jitter*0.4:rule.angle;
            rule.tropism*=0.2;rule.bend+=0.15;
        } else if(u_form==4) { // shrub: basal crown, broad primary branches
            if(level==1) {rule.angle=radians(58.0)+jitter*0.5;rule.lengthScale*=2.4;}
        } else if(u_form==5 && level>1) { // weeping: pendant descendants
            rule.tropism=-0.75+u_gravity*0.3;rule.bend=-0.5+u_bend*0.3;
            rule.angle+=radians(22.0);rule.lengthScale*=1.1;
        }
        branchingRule(rule,level,slot,path,key);
        presence*=clamp(rule.activation,0.0,1.0);
        float t=clamp(rule.attach,0.0,1.0);
        vec3 origin=branchCurve(a,c,b,t);
        vec3 axis=branchTangent(a,c,b,t);
        frame=branchUnit(frame-axis*dot(frame,axis),
                         branchUnit(cross(axis,vec3(0,0,1)),vec3(1,0,0)));
        vec3 side=frame*cos(rule.azimuth)+cross(axis,frame)*sin(rule.azimuth);
        vec3 direction=branchUnit(axis*cos(clamp(rule.angle,0.0,3.1415927))+side*sin(clamp(rule.angle,0.0,3.1415927)),axis);
        direction=branchUnit(direction+vec3(0,clamp(rule.tropism,-1.0,1.0),0),axis);
        float extent=length(b-a)*clamp(rule.lengthScale,0.0,2.5)*presence;
        radius*=mix(1.0,0.45,t)*clamp(rule.radiusScale,0.0,0.95)*presence;
        a=origin;b=branchEnvelope(a+direction*extent);
        c=branchEnvelope(mix(a,b,0.5)+vec3(0,clamp(rule.bend,-1.0,1.0)*extent*0.5,0));
        // Descendants inherit this fade. The zero-scale bake path bypasses LOD.
        if(branchPixelScale>0.0) {
            float pixels=length(b-a)*branchPixelScale;
            float visible=smoothstep(u_minPixels*0.4,max(u_minPixels,0.001),pixels);
            presence*=visible;radius*=visible;b=mix(a,b,visible);c=mix(a,c,visible);
        }
    }
    if(u_form==3) {a.y=-a.y;c.y=-c.y;b.y=-b.y;}
}
vec3 branchingSway(vec3 p) {
    float f=clamp(abs(p.y)/u_height,0.0,1.0);
    p.x+=sin(time*1.1+float(u_seed)*0.7+p.y*0.3)*u_height*u_wind*0.6*f*f;
    p.z+=cos(time*0.9+p.x*0.5)*u_height*u_wind*0.25*f*f;
    return p;
}
void treeVertex(out vec3 p,out vec3 normal,out vec2 uv) {
    bool leaf=treePass==1;int ordinal=gl_InstanceID;
    uint leafKey=hash(uint(ordinal)^u_seed*137u);
    if(leaf) {
        // Last actually submitted level, including a budget-truncated final level.
        int first=0,next=1,width=clamp(u_branches,1,12);
        for(int k=0;k<6;k++) {
            if(next>=treeBranches || k>=u_levels)break;
            first=next;next+=width;width*=clamp(u_forks,1,8);
        }
        ordinal=first+int(random(leafKey+1u)*float(max(treeBranches-first,1)));
    }
    vec3 a,c,b,frame;float radius,presence;uint key;
#ifdef BRANCHING_CACHED
    vec4 s0=texelFetch(branchStates,ordinal*4);
    vec4 s1=texelFetch(branchStates,ordinal*4+1);
    vec4 s2=texelFetch(branchStates,ordinal*4+2);
    vec4 s3=texelFetch(branchStates,ordinal*4+3);
    a=s0.xyz;radius=s0.w;c=s1.xyz;presence=s1.w;b=s2.xyz;key=uint(s2.w);frame=s3.xyz;
#else
    branchEvaluate(ordinal,a,c,b,frame,radius,presence,key);
#endif
    treePart=leaf?1.0:0.0;treeKey=float(key&0x00ffffffu);
    vec2 pattern[6]=vec2[](vec2(0,0),vec2(1,0),vec2(1,1),vec2(0,0),vec2(1,1),vec2(0,1));
    uv=pattern[gl_VertexID%6];
    if(presence<1e-6) {p=a;normal=vec3(0,1,0);return;}
    if(leaf) {
        float t=0.25+0.75*random(leafKey+2u);
        vec3 root=branchCurve(a,c,b,t);
        root+=(vec3(random(leafKey+6u),random(leafKey+7u),random(leafKey+8u))-0.5)
            *u_leafSize*2.0*presence;
        float angle=random(leafKey+3u)*6.2831853;
        vec3 side=vec3(cos(angle),0,sin(angle));
        vec3 up=branchUnit(vec3(-sin(angle)*0.6,0.6+random(leafKey+4u),cos(angle)*0.6),vec3(0,1,0));
        if(u_form==5)up.y=-up.y;
        float size=u_leafSize*(0.7+random(leafKey+5u)*0.6)*presence;
        float width=u_form==1?0.22:(u_form==5?0.4:1.0);
        p=branchingSway(root+(uv.x-0.5)*size*width*side+uv.y*size*up);
        normal=branchUnit(cross(side,up),vec3(0,1,0));
    } else {
        int cell=gl_VertexID/6,sector=cell%BRANCH_SECTORS,segment=cell/BRANCH_SECTORS;
        float t=(float(segment)+uv.y)/float(BRANCH_SEGMENTS);
        vec3 tangent=branchTangent(a,c,b,t);
        vec3 u=branchUnit(frame-tangent*dot(frame,tangent),
                          branchUnit(cross(tangent,vec3(0,0,1)),vec3(1,0,0)));
        vec3 v=cross(tangent,u);
        float angle=(float(sector)+uv.x)*6.2831853/float(BRANCH_SECTORS);
        normal=u*cos(angle)+v*sin(angle);
        p=branchingSway(branchCurve(a,c,b,t)+normal*radius*mix(1.0,0.45,t));
        uv=vec2((float(sector)+uv.x)/float(BRANCH_SECTORS),t*length(b-a));
    }
}
