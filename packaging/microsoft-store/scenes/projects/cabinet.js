// One shared construction rule; each placed cabinet keeps independent dimensions.
$NUMBER('plate', 'Plate thickness', 0.2);
const BASE = {width:6,height:8.8,depth:5.6};
function onCreate() {
  $NUMBER('width', 'Width', BASE.width);
  $NUMBER('height', 'Height', BASE.height);
  $NUMBER('depth', 'Depth', BASE.depth);
  $NUMBER('opening', 'Opening angle', 0);
  $NUMBER('shelves', 'Shelves', 1);
  $CHECKBOX('feet', 'Adjustable feet', true);
  $CHECKBOX('top', 'Top panel', true);
  $CHECKBOX('display', 'Shelf contents', false);
  $LIST('layout', {doors:'Hinged doors',drawers:'Drawer stack',open:'Open shelving'}, 'Front', 'doors');
  $MATERIAL('front', 'Front finish', 'sage');
  $MATERIAL('carcass', 'Carcass finish', 'ivory');
  $HOTSPOT(new Point(BASE.width,BASE.height,BASE.depth), 'size', 'Size', $HOTSPOT_SHAPE_DIAMOND, '#18c7e6ff');
}
function onAnimationFrame(params) {
  const {plate:T,opening,layout,front,carcass,shelves,feet}=params;
  const W=Math.max(2,params.width+params.size.position.x-BASE.width);
  const H=Math.max(2,params.height+params.size.position.y-BASE.height);
  const D=Math.max(2,params.depth+params.size.position.z-BASE.depth);
  const base=feet?1:0;
  B(0,base,0,T,H-base,D,carcass);B(W-T,base,0,T,H-base,D,carcass);
  B(T,base,0,W-2*T,T,D,carcass);if(params.top)B(T,H-T,0,W-2*T,T,D,carcass);
  B(T,base+T,0,W-2*T,H-base-2*T,T*.45,carcass);
  if(feet)for(const x of [.4,W-.4])for(const z of [.5,D-.5]){
    B(x-.1,0,z-.1,.2,base,.2,'graphite');B(x-.2,0,z-.2,.4,.08,.4,'graphite');
  }
  if(layout==='drawers'){
    for(let i=0;i<3;i++){
      const y=base+(H-base)*i/3+.04,h=(H-base)/3-.08,z=D+.06+(i===2?opening*.035:0);
      B(.04,y,z,W-.08,h,T,front);
      B(.35,y+.15,z-D+.6,W-.7,T,D-.8,carcass);
      B(.3,y+.15,z-D+.6,T,h-.3,D-.8,carcass);B(W-.3-T,y+.15,z-D+.6,T,h-.3,D-.8,carcass);
      B(.35,y+.15,z-D+.6,W-.7,h-.3,T,carcass);
      B(W*.26,y+h-.35,z+T+.12,W*.48,.08,.13,'brass');
    }
  }else{
    for(let s=1;s<=Math.min(5,shelves);s++)B(T,base+(H-base)*s/(shelves+1),T,W-2*T,T,D-T-.1,carcass);
    if(layout==='doors')for(let side=0;side<2;side++){
      const start=out.length,x=side*W/2+.03;
      B(x,base+.025,D+.06,W/2-.06,H-base-.05,T,front);
      B(side?W/2+.22:W/2-.31,H-1.05,D+T+.15,.09,.75,.13,'brass');
      const door=out.splice(start);
      out.push(rotate(P(side?W:0,0,D+.06),P(0,1,0),side?opening:-opening,door));
    }
  }
  if(params.display && layout==='open'){
    for(const x of [W/3,2*W/3])B(x,base+T,T,T,H-base-2*T,D-T,carcass);
    const y=base+T;
    for(let i=0;i<7;i++)B(T+.2+i*.55,y,D-2.6,.36,2.1+(i%3)*.25,2.2,['ivory','sage','clay'][i%3]);
    B(W/3+.45,y,T+.5,W/3-1.2,2.5,D-T-.9,'linen');B(W/2-.6,y+1.1,D-.35,1.2,.4,.04,'graphite');
    B(2*W/3+.4,base+(H-base)*.52,T+.3,W/3-1,1.6,D-.7,'sage');
    B(T+.4,base+(H-base)*.72,D-2.4,W/3-.8,.3,2.1,'ivory');
    B(T+.7,base+(H-base)*.72+.3,D-2.2,W/3-1.1,.22,1.9,'clay');
  }
  return [this.geometry,out];
}
