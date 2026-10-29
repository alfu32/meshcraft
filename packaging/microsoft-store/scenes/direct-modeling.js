// One-shot modeling study, evaluated by Meshcraft's own scripting SDK through MCP.
const P=(x,y,z)=>new Point(x,y,z);
const blue=phong('#289db9'),orange=phong('#ef983f'),ivory=phong('#e6e3d8');
const out=[];
function quad(a,b,c,d,id,mat){out.push(face(a,b,c,id+'a',mat),face(a,c,d,id+'b',mat));}
function ring(cx,cz,y,outer,inner,h,id,mat){
  const n=64,pt=(r,Y,i)=>P(cx+r*cos(i*360/n),Y,cz+r*sin(i*360/n));
  for(let i=0;i<n;i++){
    const a=pt(outer,y,i),b=pt(outer,y,i+1),c=pt(outer,y+h,i+1),d=pt(outer,y+h,i);
    const e=pt(inner,y,i),f=pt(inner,y,i+1),g=pt(inner,y+h,i+1),j=pt(inner,y+h,i);
    quad(a,d,c,b,id+'outer'+i,mat);quad(e,f,g,j,id+'inner'+i,mat);
    quad(d,j,g,c,id+'top'+i,mat);quad(a,b,f,e,id+'base'+i,mat);
    for(const [v,w,k] of [[d,c,'o'],[j,g,'i'],[a,b,'b']])out.push(line(v,w,id+k+i));
  }
}
const part=this.part || 'housing';
if(['housing','flange','body','fasteners'].includes(part)) {
// Machined annular housing, flange and radial fasteners.
if(part==='housing'||part==='flange')ring(-3,1,0,3.2,1.3,.55,'flange',ivory);
if(part==='housing'||part==='body')ring(-3,1,.55,2.5,1.3,2.4,'housing',blue);
if(part==='housing'||part==='fasteners')for(let i=0;i<8;i++){let a=i*45;out.push(cylinder(P(-3+2.84*cos(a),.55,1+2.84*sin(a)),.18,.24,'bolt'+i,orange,6));}
}
if(part==='profile') {
// A stepped concave profile and its extrusion.
const profile=[P(-6,0,-5),P(-1,0,-5),P(-1,0,-3.8),P(-2.5,0,-3.8),P(-2.5,0,-3),P(-4,0,-3),P(-4,0,-2.2),P(-6,0,-2.2)];
out.push(linear_extrude(2.4,profile,'profile',ivory));
out.push(polyline(profile.map(p=>P(p.x,0.02,p.z-2.8)),'outline',phong('#1697ad'),true));
}
return out;
