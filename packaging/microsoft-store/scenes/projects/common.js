// Shared modeling helpers, evaluated inside Meshcraft's documented one-shot SDK.
// Every result becomes editable native mesh geometry, not an external render.
const P=(x=0,y=0,z=0)=>new Point(x,y,z), out=[];
let serial=0;
const ID=k=>k+':'+serial++;
const materials={};
function M(key){
  if(typeof key==='object')return key;
  if(key.startsWith('#'))return phong(key);
  return materials[key]||(materials[key]=model.getMaterial(key));
}
const va=(a,b)=>a.map((v,i)=>v+b[i]),vs=(a,b)=>a.map((v,i)=>v-b[i]);
const vm=(a,s)=>a.map(v=>v*s),vd=(a,b)=>a.reduce((s,v,i)=>s+v*b[i],0);
const vc=(a,b)=>[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];
const vn=a=>vm(a,1/Math.hypot(...a));
function B(x,y,z,w,h,d,mat='ivory'){
  out.push(cuboid(P(x,y,z),P(x+w,y+h,z+d),ID('panel'),M(mat)));
}
function Q(a,b,c,d,mat){out.push(face(P(...a),P(...b),P(...c),ID('face'),M(mat)),face(P(...a),P(...c),P(...d),ID('face'),M(mat)));}
function E(a,b,mat='graphite'){out.push(line(P(...a),P(...b),ID('edge'),M(mat)));}
function basis(a,b){const t=vn(vs(b,a)),u=vn(vc(t,Math.abs(t[1])<.8?[0,1,0]:[1,0,0]));return [u,vc(t,u)];}
function round(a,r,u,v,i,n){const t=i*2*Math.PI/n;return va(a,va(vm(u,r*Math.cos(t)),vm(v,r*Math.sin(t))));}
function R(a,b,r,mat='steel',n=20){
  const [u,v]=basis(a,b);
  for(let i=0;i<n;i++){
    const a0=round(a,r,u,v,i,n),a1=round(a,r,u,v,i+1,n),b0=round(b,r,u,v,i,n),b1=round(b,r,u,v,i+1,n);
    Q(a0,a1,b1,b0,mat);
    out.push(face(P(...a),P(...a1),P(...a0),ID('cap'),M(mat)),face(P(...b),P(...b0),P(...b1),ID('cap'),M(mat)));
    E(a0,a1,mat);E(b0,b1,mat);
  }
}
function Ring(a,b,ro,ri,mat='steel',n=24){
  const [u,v]=basis(a,b);
  for(let i=0;i<n;i++){
    const a0=round(a,ro,u,v,i,n),a1=round(a,ro,u,v,i+1,n),b0=round(b,ro,u,v,i,n),b1=round(b,ro,u,v,i+1,n);
    const c0=round(a,ri,u,v,i,n),c1=round(a,ri,u,v,i+1,n),d0=round(b,ri,u,v,i,n),d1=round(b,ri,u,v,i+1,n);
    Q(a0,a1,b1,b0,mat);Q(c1,c0,d0,d1,mat);Q(a1,a0,c0,c1,mat);Q(b0,b1,d1,d0,mat);
    E(a0,a1,mat);E(b0,b1,mat);E(c0,c1,mat);E(d0,d1,mat);
  }
}
function Pipe(points,r,mat='steel',n=16){
  const rings=points.map((p,i)=>{
    const [u,v]=basis(i?points[i-1]:p,i===points.length-1?p:points[i+1]);
    return Array.from({length:n},(_,j)=>round(p,r,u,v,j,n));
  });
  for(let k=0;k<rings.length-1;k++)for(let i=0;i<n;i++)Q(rings[k][i],rings[k][(i+1)%n],rings[k+1][(i+1)%n],rings[k+1][i],mat);
  for(const k of [0,rings.length-1])for(let i=0;i<n;i++)E(rings[k][i],rings[k][(i+1)%n],mat);
}
function Ball(c,r,mat='steel',n=16){out.push(sphere(P(...c),r,ID('sphere'),M(mat),n));}
function Bolt(a,b,r=1.5,mat='steel'){R(a,b,r,mat,6);const d=vm(vn(vs(b,a)),r*.4);R(b,va(b,d),r*1.5,mat,6);}
function Rim(x,y,z,w,h,d,t,mat='graphite'){
  B(x,y,z,t,h,d,mat);B(x+w-t,y,z,t,h,d,mat);B(x+t,y,z,w-2*t,t,d,mat);B(x+t,y+h-t,z,w-2*t,t,d,mat);
}
function Slats(x,y,z,w,h,d,count,mat='oak'){
  const gap=w/count;for(let i=0;i<count;i++)B(x+i*gap,y,z,gap*.42,h,d,mat);
}
function Foot(x,y,z,h,mat='graphite'){R([x,y,z],[x,y+h,z],.18,mat,12);R([x,y,z],[x,y+.06,z],.32,mat,16);}
