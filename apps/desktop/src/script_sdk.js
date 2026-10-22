// Fresh globals per invocation; an immutable scene JSON string is shared by the VM.
(function(request) {
"use strict";
const point = p => {if(Array.isArray(p))p={x:p[0],y:p[1],z:p[2]===undefined?0:p[2]};return {x:Number(p.x),y:Number(p.y),z:Number(p.z),id:p.id===undefined?"":String(p.id)};};
let nextId=0;
const automaticId=name=>name+':'+nextId++;
class Point { constructor(x=0,y=0,z=0,id="") {Object.assign(this,{x,y,z,id});} }
class Color { constructor(red=255,green=255,blue=255,alpha=255) {Object.assign(this,{red,green,blue,alpha});} }
const toColor = c => {
 if(Array.isArray(c)) {const v=[...c];if(v.length===3)v.push(1);return new Color(...v.map(x=>Math.round(x*255)));}
 if(typeof c==="string") {const named={red:"ff0000",green:"008000",blue:"0000ff",white:"ffffff",black:"000000",yellow:"ffff00",cyan:"00ffff",magenta:"ff00ff",orange:"ffa500",gray:"808080",grey:"808080",transparent:"00000000"};let h=(named[c.toLowerCase()]||c).replace(/^#/,"");if(h.length===6)h+="ff";if(!/^[0-9a-f]{8}$/i.test(h))throw Error("Color must be #RRGGBB or #RRGGBBAA");return new Color(...h.match(/../g).map(x=>parseInt(x,16)));}
 if(c&&c.color)return toColor(c.color);
 if(!c)return new Color();
 const v=[c.red,c.green,c.blue,c.alpha===undefined?255:c.alpha];
 if(v.some(x=>!Number.isFinite(x)||x<0||x>255))throw Error("Color channels must be 0..255");
 return new Color(...v.map(Math.round));
};
class Material {constructor(c="#ffffffff",texturePath=null){this.id="script";this.name="Script material";this.color=toColor(c);this.texturePath=texturePath;this.textureScaleU=1;this.textureScaleV=1;this.shadingType="phong";}}
const phong=c=>new Material(c),gouraud=c=>Object.assign(new Material(c),{shadingType:"gouraud"});
class Segment {constructor(a,b,id,material="#20252aff"){Object.assign(this,{kind:"segment",id:String(id),points:[point(a),point(b)],color:toColor(material),material:material instanceof Material?material:undefined});}}
class Face {constructor(a,b,c,id,material){Object.assign(this,{kind:"face",id:String(id),points:[point(a),point(b),point(c)],color:toColor(material),material:material instanceof Material?material:undefined});}}
const line=(...args)=>new Segment(...args),face=(...args)=>new Face(...args);
const state=request.state||{initializer:"",properties:{}};
const instance=Object.assign({},state.properties,{id:request.id,geometry:request.geometry,hotspots:request.hotspots||{},time:request.time,deltaTime:request.deltaTime});
let phase="prototype";
const declarations=[];
const initialized=state.parameters||{},parameterValues=Object.create(null);
const prototypeValues=request.prototypeParameters||{},instanceValues=request.instanceParameters||{};
const apiVersion=request.source.apiVersion||1;
const reserved=new Set(["__proto__","prototype","constructor"]);
const validValue=(d,v)=>d.kind==='text'?typeof v==='string'&&v.length<=16384:d.kind==='number'?Number.isFinite(v):d.kind==='checkbox'?typeof v==='boolean':d.kind==='list'?typeof v==='string'&&Object.prototype.hasOwnProperty.call(d.options,v):v&&[v.x,v.y,v.z].every(Number.isFinite);
function declare(kind,id,label,defaultValue,options={}) {
 if(apiVersion!==2)throw Error('Model declarations require script API v2; switch the editor API selector.');
 if(phase==='frame')throw Error('Declare model inputs outside hooks or inside onCreate, not onAnimationFrame.');
 if(typeof id!=='string'||!id||id.length>128||reserved.has(id))throw Error('Parameter IDs must be nonempty, safe strings of at most 128 characters.');
 if(declarations.some(d=>d.id===id))throw Error('Duplicate parameter ID: '+id);
 if(declarations.length>=256)throw Error('At most 256 model parameters are supported.');
 if(label===undefined)label=id;
 if(typeof label!=='string'||label.length>4096)throw Error('Parameter label must be a string of at most 4096 characters.');
 const d={kind,id,label,scope:phase,defaultValue,options};
 if(!validValue(d,defaultValue))throw Error('Invalid default for parameter '+id);
 declarations.push(d);
 const values=phase==='prototype'?prototypeValues:instanceValues;
 return Object.prototype.hasOwnProperty.call(values,id)&&validValue(d,values[id])?values[id]:phase==='instance'&&Object.prototype.hasOwnProperty.call(initialized,id)&&validValue(d,initialized[id])?initialized[id]:defaultValue;
}
const $TEXT=(id,label=id,value="")=>declare('text',id,label,value);
const $NUMBER=(id,label=id,value=0)=>declare('number',id,label,value);
const $CHECKBOX=(id,label=id,value=false)=>declare('checkbox',id,label,value);
function $LIST(id,values,label=id,value=Object.keys(values||{})[0]) {
 if(!values||Array.isArray(values)||typeof values!=='object'||Object.keys(values).length===0||Object.keys(values).length>256||Object.entries(values).some(([k,v])=>reserved.has(k)||typeof v!=='string'))throw Error('List choices must map nonempty keys to labels (at most 256).');
 return declare('list',id,label,value,values);
}
function $HOTSPOT(position,id,label=id) {
 if(phase!=='frame'&&apiVersion===2)declare('hotspot',id,label,point(position));
 return {kind:"hotspot",id:String(id),position:point(instance.hotspots[id]||position)};
}
const flatten=items=> {const out=[]; const walk=(v,depth)=> {if(depth>32)throw Error("Geometry nesting exceeds 32 levels");if(Array.isArray(v))for(const child of v)walk(child,depth+1);else if(v!=null){if(out.length>=100000)throw Error("Geometry exceeds 100000 drawables");out.push(v);}};walk(items,0);return out;};
const polyline=(points,id=automaticId("polyline"),material,closed=false)=>points.slice(1).map((p,i)=>line(points[i],p,id+":"+i,material)).concat(closed&&points.length>2?[line(points[points.length-1],points[0],id+":close",material)]:[]);
function circle(center,radius,id,material,segments=12){if(typeof id==='number'){segments=id;id=undefined;}if(id===undefined)id=automaticId("circle");center=point(center);segments=Math.max(3,Math.min(512,Math.floor(segments)));const pts=Array.from({length:segments},(_,i)=>new Point(center.x+radius*Math.cos(i*2*Math.PI/segments),center.y,center.z+radius*Math.sin(i*2*Math.PI/segments)));return polyline(pts,id,material,true);}
function cube(origin,size,id,material){if(typeof size==="number")size=new Point(size,size,size);const p=Array.from({length:8},(_,i)=>new Point(origin.x+(i&1?size.x:0),origin.y+(i&2?size.y:0),origin.z+(i&4?size.z:0)));
 const quads=[[0,4,6,2],[1,3,7,5],[0,1,5,4],[2,6,7,3],[0,2,3,1],[4,5,7,6]];
 const out=[];for(let i=0;i<quads.length;i++){const[a,b,c,d]=quads[i];out.push(face(p[a],p[b],p[c],id+":f"+i+"a",material),face(p[a],p[c],p[d],id+":f"+i+"b",material));}
 for(const[a,b]of[[0,1],[0,2],[0,4],[1,3],[1,5],[2,3],[2,6],[3,7],[4,5],[4,6],[5,7],[6,7]])out.push(line(p[a],p[b],id+":e"+a+":"+b));return out;}
function sphere(center,radius,id,material,segments=24){segments=Math.max(4,Math.min(128,Math.floor(segments)));const rings=Math.ceil(segments/2),out=[];
 const p=(a,b)=>new Point(center.x+radius*Math.sin(a)*Math.cos(b),center.y+radius*Math.cos(a),center.z+radius*Math.sin(a)*Math.sin(b));
 for(let j=0;j<rings;j++)for(let i=0;i<segments;i++){const a=j*Math.PI/rings,b=(j+1)*Math.PI/rings,c=i*2*Math.PI/segments,d=(i+1)*2*Math.PI/segments;out.push(face(p(a,c),p(b,c),p(b,d),id+":"+j+":"+i+"a",material),face(p(a,c),p(b,d),p(a,d),id+":"+j+":"+i+"b",material));}return out;}
function cone(origin,radius,height,id,material,segments=32){segments=Math.max(3,Math.min(512,Math.floor(segments)));const top=new Point(origin.x,origin.y+height,origin.z),out=[];for(let i=0;i<segments;i++){const p=a=>new Point(origin.x+radius*Math.cos(a),origin.y,origin.z+radius*Math.sin(a));const a=p(i*2*Math.PI/segments),b=p((i+1)*2*Math.PI/segments);out.push(face(a,top,b,id+":side"+i,material),face(origin,a,b,id+":base"+i,material));}return out;}
// Geometry transforms accept a Point, drawable, or any nested list of them.
const vector=v=>{const p=typeof v==='number'?new Point(v,v,v):Array.isArray(v)?new Point(...v):point(v);if(![p.x,p.y,p.z].every(Number.isFinite))throw Error('Expected a finite Point or [x,y,z]');return p;};
const dot=(a,b)=>{a=vector(a);b=vector(b);return a.x*b.x+a.y*b.y+a.z*b.z;};
const cross=(a,b)=>{a=vector(a);b=vector(b);return new Point(a.y*b.z-a.z*b.y,a.z*b.x-a.x*b.z,a.x*b.y-a.y*b.x);};
const norm=a=>Math.sqrt(dot(a,a));
const normalize=a=>{a=vector(a);const n=norm(a);if(n<1e-12)throw Error('Direction must be nonzero');return new Point(a.x/n,a.y/n,a.z/n);};
const add=(a,b)=>{a=vector(a);b=vector(b);return new Point(a.x+b.x,a.y+b.y,a.z+b.z);};
const subtract=(a,b)=>{a=vector(a);b=vector(b);return new Point(a.x-b.x,a.y-b.y,a.z-b.z);};
const distance=(a,b)=>norm(subtract(a,b));
function mapPoints(items, transform, flip=false){
 if(Array.isArray(items)&&!(items.length===3&&items.every(Number.isFinite)))return items.map(v=>mapPoints(v,transform,flip));
 if(items==null)return items;
 if(items.kind==='segment'||items.kind==='face'){
  const points=items.points.map(p=>mapPoints(p,transform));if(flip&&items.kind==='face')points.reverse();return {...items,points};
 }
 if(items.kind==='hotspot')return {...items,position:instance.hotspots[items.id]?point(instance.hotspots[items.id]):mapPoints(items.position,transform)};
 const result=transform(vector(items));return {...(Array.isArray(items)?{}:items),x:result.x,y:result.y,z:result.z};
}
function translate(delta,items){const d=vector(delta),f=items=>mapPoints(items,p=>add(p,d));return items===undefined?f:f(items);}
function scale(sizes,items){const s=vector(sizes),f=items=>mapPoints(items,p=>new Point(p.x*s.x,p.y*s.y,p.z*s.z),s.x*s.y*s.z<0);return items===undefined?f:f(items);}
function rotate(origin,up,amount,items){
 if(typeof origin==='number'){items=amount;amount=origin;origin=new Point();}
 if(amount===undefined){const e=vector(origin),f=items=>rotate(new Point(),new Point(0,0,1),e.z)(rotate(new Point(),new Point(0,1,0),e.y)(rotate(new Point(),new Point(1,0,0),e.x)(items)));return up===undefined?f:f(up);}
 const o=vector(origin),axis=normalize(up),a=Number(amount)*Math.PI/180,c=Math.cos(a),s=Math.sin(a);
 if(!Number.isFinite(a))throw Error('Rotation angle must be finite degrees');
 const f=items=>mapPoints(items,p=>{const v=subtract(p,o),k=cross(axis,v),d=dot(axis,v)*(1-c);return add(o,new Point(v.x*c+k.x*s+axis.x*d,v.y*c+k.y*s+axis.y*d,v.z*c+k.z*s+axis.z*d));});
 return items===undefined?f:f(items);
}
function mirror(normal,items){const n=normalize(normal),f=items=>mapPoints(items,p=>{const d=2*dot(n,p);return new Point(p.x-d*n.x,p.y-d*n.y,p.z-d*n.z);},true);return items===undefined?f:f(items);}
function multmatrix(matrix,items){
 if(!Array.isArray(matrix)||matrix.length!==4||matrix.some(r=>!Array.isArray(r)||r.length!==4||r.some(v=>!Number.isFinite(v))))throw Error('multmatrix needs a finite 4x4 matrix');
 const m=matrix,det=m[0][0]*(m[1][1]*m[2][2]-m[1][2]*m[2][1])-m[0][1]*(m[1][0]*m[2][2]-m[1][2]*m[2][0])+m[0][2]*(m[1][0]*m[2][1]-m[1][1]*m[2][0]);
 const f=items=>mapPoints(items,p=>{const v=[p.x,p.y,p.z,1],r=m.map(row=>row.reduce((s,x,i)=>s+x*v[i],0));if(Math.abs(r[3])<1e-12)throw Error('Matrix projects to infinity');return new Point(r[0]/r[3],r[1]/r[3],r[2]/r[3]);},det<0);return items===undefined?f:f(items);
}
function withIds(items,prefix){
 if(Array.isArray(items))return items.map(v=>withIds(v,prefix));if(items==null)return items;
 const out={...items};if(out.id)out.id=String(prefix)+':'+out.id;if(out.points)out.points=out.points.map(p=>withIds(p,prefix));if(out.position)out.position=withIds(out.position,prefix);return out;
}
function color(tint,items){const material=tint instanceof Material?tint:phong(tint);const f=items=>{if(Array.isArray(items))return items.map(f);if(items==null)return items;return {...items,color:material.color,material};};return items===undefined?f:f(items);}
function cylinder(origin,radius,height,id,material,segments=32,topRadius=radius){
 origin=vector(origin);segments=Math.max(3,Math.min(512,Math.floor(segments)));if(![radius,height,topRadius].every(Number.isFinite)||radius<0||topRadius<0||height===0)throw Error('Cylinder needs nonnegative radii and nonzero height');
 const out=[],p=(r,y,i)=>new Point(origin.x+r*Math.cos(i*2*Math.PI/segments),origin.y+y,origin.z+r*Math.sin(i*2*Math.PI/segments)),center=new Point(origin.x,origin.y+height,origin.z);
 for(let i=0;i<segments;i++){const a=p(radius,0,i),b=p(radius,0,i+1),c=p(topRadius,height,i),d=p(topRadius,height,i+1);
  if(radius>0){out.push(face(origin,a,b,id+':base'+i,material),face(a,c,b,id+':side'+i+'a',material));}
  if(topRadius>0){out.push(face(center,d,c,id+':top'+i,material),face(b,c,d,id+':side'+i+'b',material));}
  out.push(line(a,b,id+':edge'+i),line(c,d,id+':rim'+i),line(a,c,id+':vertical'+i));
 }if(height<0)for(const item of out)if(item.kind==='face')item.points.reverse();return out;
}
// Ear clipping handles simple, planar concave polygons; holes are separate paths.
function triangulate(points){
 if(points.length<3)throw Error('Polygon needs at least three points');
 let normal=new Point();for(let i=0;i<points.length;i++){const a=points[i],b=points[(i+1)%points.length];normal=add(normal,cross(a,b));}
 normal=normalize(normal);const u=normalize(subtract(points[1],points[0])),v=cross(normal,u),p=points.map(a=>[dot(a,u),dot(a,v)]);
 if(points.some(a=>Math.abs(dot(subtract(a,points[0]),normal))>1e-6))throw Error('Polygon must be planar');
 const cross2=(a,b,c)=>(b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]);
 const remaining=points.map((_,i)=>i),out=[];
 while(remaining.length>3){let found=false;for(let i=0;i<remaining.length;i++){
  const a=remaining[(i+remaining.length-1)%remaining.length],b=remaining[i],c=remaining[(i+1)%remaining.length];if(cross2(p[a],p[b],p[c])<=1e-12)continue;
  if(remaining.some(j=>j!==a&&j!==b&&j!==c&&cross2(p[a],p[b],p[j])>=-1e-12&&cross2(p[b],p[c],p[j])>=-1e-12&&cross2(p[c],p[a],p[j])>=-1e-12))continue;
  out.push([a,b,c]);remaining.splice(i,1);found=true;break;
 }if(!found)throw Error('Polygon is degenerate or self-intersecting');}
 out.push(remaining);return out;
}
function polygon(points,id,material){const p=points.map(vector);return [...triangulate(p).map((t,i)=>face(...t.map(j=>p[j]),id+':f'+i,material)),...polyline(p,id+':e',undefined,true)];}
function rectangle(p0,p1,id=automaticId('rectangle'),material){
 const a=vector(p0),b=vector(p1),d=subtract(b,a);let u;
 if(Math.abs(d.y)<1e-12)u=new Point(d.x,0,0);
 else if(Math.abs(d.z)<1e-12)u=new Point(d.x,0,0);
 else if(Math.abs(d.x)<1e-12)u=new Point(0,d.y,0);
 else throw Error('Rectangle corners must share X, Y or Z; rotate a planar rectangle for arbitrary orientation');
 return polygon([a,add(a,u),b,subtract(b,u)],id,material);
}
function cuboid(p0,p1,id=automaticId('cuboid'),material){const a=vector(p0),b=vector(p1),o=new Point(Math.min(a.x,b.x),Math.min(a.y,b.y),Math.min(a.z,b.z));return cube(o,new Point(Math.abs(b.x-a.x),Math.abs(b.y-a.y),Math.abs(b.z-a.z)),id,material);}
function square(origin,size,id,material){origin=vector(origin);size=vector(size);return polygon([origin,add(origin,new Point(size.x,0,0)),add(origin,new Point(size.x,0,size.z)),add(origin,new Point(0,0,size.z))],id,material);}
function polyhedron(points,faces,id,material){const p=points.map(vector),out=[];for(let i=0;i<faces.length;i++){const indices=faces[i];if(indices.some(j=>!Number.isInteger(j)||!p[j]))throw Error('Invalid polyhedron point index');out.push(...polygon(indices.map(j=>p[j]),id+':'+i,material));}return out;}
function linear_extrude(height,points,id,material){
 if(!Number.isFinite(height)||height===0)throw Error('Extrusion height must be nonzero');const p=points.map(vector);if(p.some(a=>Math.abs(a.y-p[0].y)>1e-6))throw Error('Extrusion profile must be on the XZ plane');let triangles=triangulate(p);const t=triangles[0];if(cross(subtract(p[t[1]],p[t[0]]),subtract(p[t[2]],p[t[0]])).y*height<0){p.reverse();triangles=triangulate(p);}const q=p.map(a=>add(a,new Point(0,height,0))),out=[];
 for(let i=0;i<triangles.length;i++){const[a,b,c]=triangles[i];out.push(face(p[c],p[b],p[a],id+':base'+i,material),face(q[a],q[b],q[c],id+':top'+i,material));}
 for(let i=0;i<p.length;i++){const j=(i+1)%p.length;out.push(face(p[i],p[j],q[j],id+':side'+i+'a',material),face(p[i],q[j],q[i],id+':side'+i+'b',material),line(p[i],p[j],id+':baseEdge'+i),line(q[i],q[j],id+':topEdge'+i),line(p[i],q[i],id+':vertical'+i));}return out;
}
const PI=Math.PI,abs=Math.abs,sign=Math.sign,floor=Math.floor,ceil=Math.ceil,round=Math.round,sqrt=Math.sqrt,pow=Math.pow,exp=Math.exp,ln=Math.log,log=Math.log10;
const sin=a=>Math.sin(a*PI/180),cos=a=>Math.cos(a*PI/180),tan=a=>Math.tan(a*PI/180),asin=a=>Math.asin(a)*180/PI,acos=a=>Math.acos(a)*180/PI,atan=a=>Math.atan(a)*180/PI,atan2=(y,x)=>Math.atan2(y,x)*180/PI;
const len=a=>a.length,concat=(...lists)=>lists.flat(),str=(...values)=>values.join(''),min=(...v)=>Math.min(...v.flat()),max=(...v)=>Math.max(...v.flat());
const is_undef=v=>v===undefined,is_bool=v=>typeof v==='boolean',is_num=v=>typeof v==='number'&&Number.isFinite(v),is_string=v=>typeof v==='string',is_list=Array.isArray,is_function=v=>typeof v==='function';
function lookup(x,table){const rows=[...table].sort((a,b)=>a[0]-b[0]);if(!rows.length)throw Error('lookup needs a nonempty table');if(x<=rows[0][0])return rows[0][1];for(let i=1;i<rows.length;i++)if(x<=rows[i][0]){const a=rows[i-1],b=rows[i];return a[1]+(b[1]-a[1])*(x-a[0])/(b[0]-a[0]);}return rows[rows.length-1][1];}
function rands(low,high,count,seed){count=Math.floor(count);if(count<0||count>100000)throw Error('Random list count must be 0..100000');let state=seed===undefined?null:Number(seed)>>>0;return Array.from({length:count},()=>{if(state===null)return low+(high-low)*Math.random();state=(1664525*state+1013904223)>>>0;return low+(high-low)*state/4294967296;});}

const freeze=o=>{if(o&&typeof o==="object"){Object.freeze(o);for(const v of Object.values(o))freeze(v);}return o;};
// Most generators only need their own geometry/state. Parse and freeze the scene
// snapshot only when a script actually queries it, never once per ordinary instance.
let data;
const snapshot=()=>data||(data=freeze(request.model||JSON.parse(__meshcraftModelJson)));
const model=Object.freeze({get objects(){return snapshot().objects;},get materials(){return snapshot().materials;},get settings(){return snapshot().settings;},time:request.time,deltaTime:request.deltaTime,getObject:id=>snapshot().objects.find(o=>o.id===id),findObjects:name=>snapshot().objects.filter(o=>o.name===name),getMaterial:id=>snapshot().materials.find(m=>m.id===id),queryGeometry:id=>snapshot().objects.find(o=>o.id===id)?.geometry||[]});
// Compile in a lexical scope containing only the SDK and immutable query facade.
const compile=(source,name)=> {
 if(/^\s*(?:async\s+)?function\b/.test(source))return eval("("+source+"\n)");
 return eval("(function "+name+"("+(name==='onAnimationFrame'&&apiVersion===2?'params':'instance')+",model){\n"+source+"\n})");
};
let hooks;
if(request.source.source && request.source.source.trim()) {
 hooks=eval("(function(){\n"+request.source.source+"\n;return {onCreate:typeof onCreate==='function'?onCreate:function(){},onAnimationFrame:typeof onAnimationFrame==='function'?onAnimationFrame:null};})()");
 if(!hooks.onAnimationFrame)throw Error("Define function onAnimationFrame(instance, model) in the script.");
} else hooks={onCreate:compile(request.source.onCreate,"onCreate"),onAnimationFrame:compile(request.source.onAnimationFrame,"onAnimationFrame")};
const initializer=hooks.onCreate.toString()+(apiVersion===2?"\n@api2":"");
phase='instance';
if(state.initializer!==initializer){hooks.onCreate.call(instance,instance,model);}
else {
 for(const d of state.declarations||[])if(d.scope==='instance') {
  declare(d.kind,d.id,d.label,d.defaultValue,d.options);
 }
}
const parms=Object.create(null),declaredHotspots=[];
for(const d of declarations) {
 const values=d.scope==='prototype'?prototypeValues:instanceValues;
 const value=Object.prototype.hasOwnProperty.call(values,d.id)&&validValue(d,values[d.id])?values[d.id]:d.scope==='instance'&&Object.prototype.hasOwnProperty.call(initialized,d.id)&&validValue(d,initialized[d.id])?initialized[d.id]:d.defaultValue;
 if(d.scope==='instance')parameterValues[d.id]=value;
 if(d.kind==='hotspot') {
  const h={kind:'hotspot',id:d.id,position:point(instance.hotspots[d.id]||value)};
  parms[d.id]=h;declaredHotspots.push(h);
 } else parms[d.id]=value;
}
freeze(parms);
phase='frame';
const output=flatten(hooks.onAnimationFrame.call(instance,apiVersion===2?parms:instance,model));
for(const h of declaredHotspots)if(!output.some(v=>v.kind==='hotspot'&&v.id===h.id))output.push(h);
const properties={};for(const[k,v]of Object.entries(instance))if(!["id","geometry","hotspots","time","deltaTime"].includes(k))properties[k]=v;
const result=JSON.stringify({items:output,state:{initializer,properties,declarations,parameters:parameterValues}});
if(result.length>16000000)throw Error("Script output exceeds 16 MB");
return result;
})
