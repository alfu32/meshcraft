// Shared declarations become prototype parameters. onCreate declares instance inputs.
$NUMBER('depth', 'Depth · shared', 4);
$NUMBER('spacing', 'Louver spacing · shared', 0.55);
function onCreate() {
  $NUMBER('width', 'Span', 5);
  $NUMBER('height', 'Height', 3.5);
  $LIST('finish', {teal:'Lagoon',amber:'Amber',ivory:'Ivory'}, 'Finish', 'teal');
  $HOTSPOT(new Point(5,3.5,0), 'size', 'Corner', $HOTSPOT_SHAPE_DIAMOND, '#00d9ffff');
}
function onAnimationFrame(params) {
  const {depth,spacing,finish}=params;
  const width=params.width+params.size.position.x-5;
  const height=params.height+params.size.position.y-3.5;
  const P=(x,y,z)=>new Point(x,y,z);
  const colors={teal:'#238b9c',amber:'#d99548',ivory:'#e4dfd1'};
  const frame=phong(colors[finish]),slat=phong('#e4c68e');
  const out=[];const box=(a,b,id,m=frame)=>out.push(cuboid(P(...a),P(...b),id,m));
  for(const x of [0,width-.2])for(const z of [0,depth-.2])box([x,0,z],[x+.2,height,z+.2],'post'+x+':'+z);
  box([-.15,height-.25,-.15],[width+.15,height+.1,.15],'front');
  box([-.15,height-.25,depth-.15],[width+.15,height+.1,depth+.15],'back');
  for(let x=0,i=0;x<=width;x+=spacing,i++)box([x,height,-.2],[x+.13,height+.24,depth+.2],'louver'+i,slat);
  for(let i=0;i<7;i++)box([.22,1.5+i*.2,depth-.22],[width-.22,1.59+i*.2,depth-.13],'screen'+i);
  box([-.35,-.18,-.35],[width+.35,0,depth+.35],'deck',phong('#b7b9ae'));
  return [this.geometry,out];
}
