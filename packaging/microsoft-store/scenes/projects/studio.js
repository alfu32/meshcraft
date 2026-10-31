const part=this.part;
if(part==='casework-room'){
  B(-4,-.25,-2,55,.25,20,'stone');B(-4,0,-2,55,24,.25,'ivory');
  B(-4,.15,-1.6,55,.5,.12,'graphite');
}
if(part==='room'){
  B(-2,-.3,-2,34,.3,30,'stone');B(-2,0,-2,34,26,.25,'ivory');B(-2,0,-2,.25,26,30,'ivory');
  for(let x=-2;x<33;x+=8)E([x,.02,-2],[x,.02,28],'concrete');
}
if(part==='desk'){
  B(3,7.3,6,21,.4,7.5,'oak');
  for(const x of [3.5,23]){B(x,0,6.7,.45,7.3,.5,'graphite');B(x,0,12.5,.45,7.3,.5,'graphite');B(x,0,6.5,.5,.4,6.8,'graphite');}
  B(4,5.9,6.7,19,.65,.5,'graphite');
  B(4.3,4.8,8.6,4,1.5,4.4,'ivory');B(4.6,5.6,13.05,3.4,.1,.2,'brass');
}
if(part==='monitors')for(const x of [6,14]){
  B(x+1.5,7.7,7.3,3,.18,2,'graphite');B(x+2.8,7.9,7.8,.3,2.6,.3,'steel');
  B(x,10,7.5,6,3.8,.3,'graphite');B(x+.16,10.16,7.85,5.68,3.45,.05,'screen');
  B(x+.5,12.7,7.92,2.7,.08,.02,'cyan');B(x+.5,11.9,7.92,4.1,.05,.02,'steel');
  B(x+.5,11.4,7.92,3.3,.05,.02,'steel');
}
if(part==='keyboard'){
  B(8,7.72,10.5,9,.18,2.4,'graphite');
  for(let row=this.row*2;row<this.row*2+2;row++)for(let col=0;col<12;col++)B(8.2+col*.7,7.93,10.7+row*.48,.56,.08,.36,'ivory');
}
if(part==='accessories'){
  out.push(translate(P(19,7.95,11.8),scale(P(.65,.24,.95),sphere(P(),1,ID('mouse'),M('ivory'),12))));
  B(20.7,7.75,8.5,2.4,.2,3.5,'clay');B(20.85,7.96,8.7,2.1,.04,3.1,'ivory');
  B(3.8,7.8,10,1.6,.2,1.4,'brass');R([4.6,8,10.7],[4.6,12.2,10.7],.045,'graphite',10);
  R([4.6,12.2,10.7],[6.1,13.2,10.7],.05,'graphite',10);R([6.1,13.1,10.7],[6.1,13.3,10.7],.7,'brass',20);
}
if(part==='chair'){
  const x=14,z=18;
  R([x,.8,z],[x,4.5,z],.3,'graphite',16);B(x-2.4,4.3,z-2.3,4.8,.6,4.6,'sage');
  for(let i=0;i<5;i++){
    const a=i*2*Math.PI/5,b=[x+3.1*Math.cos(a),.6,z+3.1*Math.sin(a)];
    R([x,1.3,z],b,.12,'steel',10);R([b[0]-.3,.5,b[2]],[b[0]+.3,.5,b[2]],.42,'graphite',12);
  }
  B(x-2.35,4.9,z+1.8,4.7,5.5,.32,'graphite');B(x-2.1,5.15,z+1.7,4.2,5,.15,'sage');
  for(const dx of [-2.7,2.4]){B(x+dx,5,z-.8,.25,2,.3,'graphite');B(x+dx,6.8,z-1.8,.4,.25,3.2,'graphite');}
}
if(part==='shelf'){
  for(const x of [2,9.7,17.4,25.2])B(x,0,0,.25,23,3.5,'graphite');
  for(const y of [0,4.6,9.2,13.8,18.4,23])B(2,y,0,23.5,.25,3.5,'oak');
}
if(part==='books'){
  const x=2.5+this.column*7.75,y=.25+this.shelf*4.6;
  for(let i=0;i<7;i++)B(x+i*.7,y,.5,.45,2.6+(i%3)*.35,2.6,['ivory','clay','sage','navy'][i%4]);
  B(x+5.3,y,.6,1.1,2.3,2.5,'linen');
}
if(part==='boxes'){
  for(const x of [3,11,19]){B(x,9.45,.4,5.6,3.4,2.8,'linen');B(x+2.4,10.7,3.25,.7,.35,.03,'graphite');}
}
if(part==='art'){
  B(28,12,-1.6,3.5,8,.2,'graphite');B(28.15,12.15,-1.35,3.2,7.7,.05,'ivory');
  B(28.5,13,-1.2,1.5,4,.03,'clay');B(29.4,16,-1.15,1.4,2.6,.03,'sage');
}
return out;
