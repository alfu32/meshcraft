const part=this.part;
if(part==='park-paving'){
  for(let z=-40;z<65;z+=7)E([-4,.05,z],[8,.05,z],'concrete');
  for(let x=-49;x<100;x+=7)E([x,.05,14],[x,.05,26],'concrete');
  B(-5,0,-40,.4,.3,105,'concrete');B(8,0,-40,.4,.3,105,'concrete');
}
if(part==='bench'){
  const x=this.x,z=this.z;
  for(let i=0;i<7;i++)B(x,4.6,z+i*.52,18,.28,.35,'oak');
  for(const dx of [1,16]){B(x+dx,0,z,.4,4.6,3.8,'graphite');B(x+dx,4.6,z,.4,3,.4,'graphite');}
  B(x,7.2,z,18,.6,.3,'oak');B(x,6.2,z,18,.6,.3,'oak');
}
if(part==='light'){
  const x=this.x,z=this.z;
  R([x,0,z],[x,27,z],.17,'graphite',12);B(x-.5,26,z-.5,1,1,4,'graphite');B(x-.36,25.96,z-.35,.72,.04,3.7,'ivory');
}
if(part==='roof-pergola'){
  for(const x of [34,84])for(const z of [4,27])B(x,0,z,.7,28,.7,'graphite');
  B(33,27,3,53,1,1.2,'graphite');B(33,27,27,53,1,1.2,'graphite');
  for(let x=33;x<87;x+=3)B(x,28,3,1,1,26,'oak');
}
if(part==='roof-sofa'){
  const x=this.x,z=this.z;
  B(x,.4,z,22,1,8,'graphite');B(x,1.4,z,22,2.3,7.7,'linen');B(x,3.7,z,22,4,1.4,'linen');
  B(x-.3,1.6,z,1.1,5,8.2,'linen');B(x+21.2,1.6,z,1.1,5,8.2,'linen');
  B(x+3,3.7,z+1.5,4,2.3,.5,'sage');B(x+14,3.7,z+1.5,4,2.3,.5,'clay');
}
if(part==='roof-table'){
  B(47,.2,36,13,3.8,8,'stone');B(48,4,37,11,.15,6,'graphite');
  B(51,4.2,38,3,.16,2.7,'clay');B(54.4,4.2,38,3,.16,2.7,'oak');
}
if(part==='roof-bar'){
  B(3,0,3,25,24,1,'house-brick');B(3,0,3,1,24,15,'house-brick');
  B(2,24,2,27,.8,18,'ivory');B(4,9.5,13,22,.5,5,'stone');B(4,0,13,22,9.5,4.5,'oak');
  for(let i=0;i<18;i++)B(4.3+i*1.2,.2,17.5,.25,9,.2,'oak');
  B(7,10,14,4,.2,2.5,'graphite');R([9,10.2,15.2],[9,11,15.2],.4,'steel',16);
}
if(part==='roof-paving'){
  for(let x=0;x<=90;x+=9)E([x,.02,0],[x,.02,70],'concrete');
  for(let z=0;z<=70;z+=10)E([0,.02,z],[90,.02,z],'concrete');
  B(0,0,0,90,5.5,.7,'stone');B(0,0,0,.7,5.5,70,'stone');B(89.3,0,0,.7,5.5,70,'stone');
  for(let x=3;x<87;x+=12){B(x,5.5,0,.3,5.5,.3,'graphite');B(x+.3,6,.15,11.3,4.8,.1,'glass');}
}
return out;
