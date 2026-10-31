const part=this.part;
if(part==='railing'){
  for(let x=2;x<129;x+=12){B(x,33.2,84,.45,11,.6,'graphite');B(x,34,84.15,11.4,9.6,.16,'glass');}
  B(2,44,84,126,.4,.7,'graphite');
  for(const x of [49,74,99,124])B(x+11.2,2,65,.4,27,.6,'graphite');
  for(const x of [154,168,182])B(x,2,125,.4,27,.6,'graphite');
  for(const x of [5,39,73])B(x+15,35,65,.4,23,.5,'graphite');
}
if(part==='pergola'){
  for(const x of [1,44])B(x,33.2,67,.9,28,.9,'graphite');
  B(0,59.5,64,46,1.5,1.5,'oak');B(0,59.5,85,46,1.5,1.5,'oak');
  for(let x=0;x<46;x+=3.2)B(x,61,64,1,1.5,23,'oak');
}
if(part==='solar'){
  for(let i=0;i<6;i++){
    const x=8+i*16,z=7+this.row*17;
    B(x,63.3,z,14,.65,15,'graphite');B(x+.25,64,z+.25,13.5,.12,14.5,'navy');
    for(let k=1;k<3;k++)B(x+.3+k*4.5,64.15,z+.3,.06,.03,14.4,'steel');
    B(x+.3,64.15,z+7.5,13.4,.03,.06,'steel');
  }
}
if(part==='table'){
  B(71,8,84,25,.7,11,'oak');
  for(const x of [73,94])for(const z of [86,93])B(x,0,z,.7,8,.7,'graphite');
}
if(part==='chair'){
  const x=this.x;
  B(x,4.4,85,4.8,.5,5,'linen');B(x,4.9,85,.45,5.5,5,'oak');
  for(const dx of [.5,4.1])for(const z of [85.6,89.4])B(x+dx,0,z,.35,4.4,.35,'graphite');
}
if(part==='site'){
  for(let x=-10;x<210;x+=11)E([x,.7,125],[x,.7,155],'concrete');
  for(let z=125;z<=155;z+=10)E([-10,.7,z],[208,.7,z],'concrete');
  for(let z=8;z<61;z+=3)B(112.1,35,z,.6,23,.45,'oak');
  B(48,.7,100,3,1,45,'concrete');B(140,.7,100,3,1,45,'concrete');
}
return out;
