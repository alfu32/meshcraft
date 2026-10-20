// Fresh globals per invocation; an immutable scene JSON string is shared by the VM.
(function(request) {
"use strict";
const point = p => ({x:Number(p.x),y:Number(p.y),z:Number(p.z),id:p.id===undefined?"":String(p.id)});
class Point { constructor(x=0,y=0,z=0,id="") {Object.assign(this,{x,y,z,id});} }
class Color { constructor(red=255,green=255,blue=255,alpha=255) {Object.assign(this,{red,green,blue,alpha});} }
const color = c => {
 if(typeof c==="string") {let h=c.replace(/^#/,"");if(h.length===6)h+="ff";if(!/^[0-9a-f]{8}$/i.test(h))throw Error("Color must be #RRGGBB or #RRGGBBAA");return new Color(...h.match(/../g).map(x=>parseInt(x,16)));}
 if(c&&c.color)return color(c.color);
 if(!c)return new Color();
 const v=[c.red,c.green,c.blue,c.alpha===undefined?255:c.alpha];
 if(v.some(x=>!Number.isFinite(x)||x<0||x>255))throw Error("Color channels must be 0..255");
 return new Color(...v.map(Math.round));
};
class Material {constructor(c="#ffffffff",texturePath=null){this.id="script";this.name="Script material";this.color=color(c);this.texturePath=texturePath;this.textureScaleU=1;this.textureScaleV=1;this.shadingType="phong";}}
const phong=c=>new Material(c),gouraud=c=>Object.assign(new Material(c),{shadingType:"gouraud"});
class Segment {constructor(a,b,id,material="#20252aff"){Object.assign(this,{kind:"segment",id:String(id),points:[point(a),point(b)],color:color(material),material:material instanceof Material?material:undefined});}}
class Face {constructor(a,b,c,id,material){Object.assign(this,{kind:"face",id:String(id),points:[point(a),point(b),point(c)],color:color(material),material:material instanceof Material?material:undefined});}}
const line=(...args)=>new Segment(...args),face=(...args)=>new Face(...args);
const state=request.state||{initializer:"",properties:{}};
const instance=Object.assign({},state.properties,{id:request.id,geometry:request.geometry,hotspots:request.hotspots||{},time:request.time,deltaTime:request.deltaTime});
const hotspot=(position,id)=>({kind:"hotspot",id:String(id),position:point(instance.hotspots[id]||position)});
const flatten=items=> {const out=[]; const walk=(v,depth)=> {if(depth>32)throw Error("Geometry nesting exceeds 32 levels");if(Array.isArray(v))for(const child of v)walk(child,depth+1);else if(v!=null){if(out.length>=100000)throw Error("Geometry exceeds 100000 drawables");out.push(v);}};walk(items,0);return out;};
const polyline=(points,id,material,closed=false)=>points.slice(1).map((p,i)=>line(points[i],p,id+":"+i,material)).concat(closed&&points.length>2?[line(points[points.length-1],points[0],id+":close",material)]:[]);
function circle(center,radius,id,material,segments=32){segments=Math.max(3,Math.min(512,Math.floor(segments)));const pts=Array.from({length:segments},(_,i)=>new Point(center.x+radius*Math.cos(i*2*Math.PI/segments),center.y,center.z+radius*Math.sin(i*2*Math.PI/segments)));return polyline(pts,id,material,true);}
function cube(origin,size,id,material){if(typeof size==="number")size=new Point(size,size,size);const p=Array.from({length:8},(_,i)=>new Point(origin.x+(i&1?size.x:0),origin.y+(i&2?size.y:0),origin.z+(i&4?size.z:0)));
 const quads=[[0,4,6,2],[1,3,7,5],[0,1,5,4],[2,6,7,3],[0,2,3,1],[4,5,7,6]];
 const out=[];for(let i=0;i<quads.length;i++){const[a,b,c,d]=quads[i];out.push(face(p[a],p[b],p[c],id+":f"+i+"a",material),face(p[a],p[c],p[d],id+":f"+i+"b",material));}
 for(const[a,b]of[[0,1],[0,2],[0,4],[1,3],[1,5],[2,3],[2,6],[3,7],[4,5],[4,6],[5,7],[6,7]])out.push(line(p[a],p[b],id+":e"+a+":"+b));return out;}
function sphere(center,radius,id,material,segments=24){segments=Math.max(4,Math.min(128,Math.floor(segments)));const rings=Math.ceil(segments/2),out=[];
 const p=(a,b)=>new Point(center.x+radius*Math.sin(a)*Math.cos(b),center.y+radius*Math.cos(a),center.z+radius*Math.sin(a)*Math.sin(b));
 for(let j=0;j<rings;j++)for(let i=0;i<segments;i++){const a=j*Math.PI/rings,b=(j+1)*Math.PI/rings,c=i*2*Math.PI/segments,d=(i+1)*2*Math.PI/segments;out.push(face(p(a,c),p(b,c),p(b,d),id+":"+j+":"+i+"a",material),face(p(a,c),p(b,d),p(a,d),id+":"+j+":"+i+"b",material));}return out;}
function cone(origin,radius,height,id,material,segments=32){segments=Math.max(3,Math.min(512,Math.floor(segments)));const top=new Point(origin.x,origin.y+height,origin.z),out=[];for(let i=0;i<segments;i++){const p=a=>new Point(origin.x+radius*Math.cos(a),origin.y,origin.z+radius*Math.sin(a));const a=p(i*2*Math.PI/segments),b=p((i+1)*2*Math.PI/segments);out.push(face(a,top,b,id+":side"+i,material),face(origin,a,b,id+":base"+i,material));}return out;}
const freeze=o=>{if(o&&typeof o==="object"){Object.freeze(o);for(const v of Object.values(o))freeze(v);}return o;};
// Most generators only need their own geometry/state. Parse and freeze the scene
// snapshot only when a script actually queries it, never once per ordinary instance.
let data;
const snapshot=()=>data||(data=freeze(request.model||JSON.parse(__meshcraftModelJson)));
const model=Object.freeze({get objects(){return snapshot().objects;},get materials(){return snapshot().materials;},get settings(){return snapshot().settings;},time:request.time,deltaTime:request.deltaTime,getObject:id=>snapshot().objects.find(o=>o.id===id),findObjects:name=>snapshot().objects.filter(o=>o.name===name),getMaterial:id=>snapshot().materials.find(m=>m.id===id),queryGeometry:id=>snapshot().objects.find(o=>o.id===id)?.geometry||[]});
// Compile in a lexical scope containing only the SDK and immutable query facade.
const compile=(source,name)=> {
 if(/^\s*(?:async\s+)?function\b/.test(source))return eval("("+source+"\n)");
 return eval("(function "+name+"(instance,model){\n"+source+"\n})");
};
let hooks;
if(request.source.source && request.source.source.trim()) {
 hooks=eval("(function(){\n"+request.source.source+"\n;return {onCreate:typeof onCreate==='function'?onCreate:function(){},onAnimationFrame:typeof onAnimationFrame==='function'?onAnimationFrame:null};})()");
 if(!hooks.onAnimationFrame)throw Error("Define function onAnimationFrame(instance, model) in the script.");
} else hooks={onCreate:compile(request.source.onCreate,"onCreate"),onAnimationFrame:compile(request.source.onAnimationFrame,"onAnimationFrame")};
const initializer=hooks.onCreate.toString();
if(state.initializer!==initializer){hooks.onCreate.call(instance,instance,model);}
const output=flatten(hooks.onAnimationFrame.call(instance,instance,model));
const properties={};for(const[k,v]of Object.entries(instance))if(!["id","geometry","hotspots","time","deltaTime"].includes(k))properties[k]=v;
const result=JSON.stringify({items:output,state:{initializer,properties}});
if(result.length>16000000)throw Error("Script output exceeds 16 MB");
return result;
})
