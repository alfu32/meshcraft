const part=this.part;
if(part==='structure'){
  for(const x of [1,34])for(const z of [1,24])B(x,0,z,.85,27,.85,'graphite');
  B(0,24.5,23.8,35,1.8,1.2,'graphite');B(33.7,24.5,0,1.3,1.8,25,'graphite');
  B(0,54.5,0,70,1.5,1.5,'graphite');
}
if(part==='railing'){
  for(let x=1;x<34;x+=6){B(x,27,24,.35,10.5,.35,'graphite');B(x+.4,28,24.1,5.1,8.5,.12,'glass');}
  B(1,37.5,24,33,.35,.5,'graphite');
  for(let z=1;z<14;z+=6){B(34,27,z,.35,10.5,.35,'graphite');B(34.1,28,z+.4,.12,8.5,5.1,'glass');}
  B(34,37.5,1,.35,.35,13,'graphite');
}
if(part==='mullions'){
  for(let x=14;x<62;x+=9)B(x,34,0,.35,20,.6,'graphite');
  B(8,44,0,54,.35,.6,'graphite');
  for(let z=14;z<45;z+=9)B(0,8,z,.6,36,.35,'graphite');
  B(0,26,8,.6,.35,37,'graphite');
}
if(part==='duct'){
  Pipe([[7,51,5],[58,51,5],[61,50,5],[62,48,5],[62,43,5]],1.2,'steel',16);
  Pipe([[24,51,5],[24,51,17],[24,50,19],[24,47,20]],.85,'steel',16);
  for(const x of [12,36,54]){R([x,51,5],[x,57,5],.05,'graphite',8);Ring([x-.18,51,5],[x+.18,51,5],1.3,1.2,'graphite',12);}
}
if(part==='sofa'){
  B(5,28,3,23,1.3,8,'graphite');B(5,29.3,3,23,2.7,7.6,'linen');B(5,32,3,23,5,1.5,'linen');
  B(4.6,30,3,1.5,5,8.3,'linen');B(27.5,30,3,1.5,5,8.3,'linen');
  B(7,32,4.7,5,2.5,.5,'sage');B(21,32,4.7,5,2.5,.5,'clay');
  R([16,27,17],[16,31.2,17],.2,'graphite',12);R([16,31.2,17],[16,31.6,17],4,'oak',32);
  B(4,27.1,13,26,.06,10,'sage');
}
if(part==='table'){
  B(6,7.4,36,27,.5,10,'oak');
  for(const x of [7,31])for(const z of [37,44])B(x,0,z,.65,7.4,.65,'graphite');
}
if(part==='desk'){
  B(4,7.3,5,24,.4,7,'oak');
  for(const x of [5,26])for(const z of [6,10.5])B(x,0,z,.5,7.3,.5,'graphite');
  B(13,7.7,6.5,4,.2,2,'graphite');B(14.8,7.9,6.7,.35,2.8,.4,'steel');
  B(10.5,10.3,6,9,5,.35,'graphite');B(10.8,10.6,6.4,8.4,4.4,.05,'screen');
  B(10,7.75,9.5,8,.15,2,'graphite');B(22,7.8,9,1.2,.15,1.7,'ivory');
}
if(part==='shelf'){
  B(39,0,1,1,17,4,'graphite');B(48,0,1,1,17,4,'graphite');
  for(let y=0;y<17;y+=4)B(39,y,1,10,.25,4,'oak');
  for(let i=0;i<7;i++)B(40+i*.9,.3,2,.6,2.8+(i%3)*.3,2.6,['ivory','sage','clay'][i%3]);
  B(40,4.3,1.5,6,2.8,3,'linen');B(42.5,5.2,4.55,1,.5,.05,'graphite');
}
if(part==='chair'){
  const x=this.x,z=this.z;
  B(x,4.2,z,4.5,.4,4.3,'linen');B(x,4.6,z,4.5,4.4,.4,'oak');
  for(const dx of [.4,3.7])for(const dz of [.4,3.5])B(x+dx,0,z+dz,.35,4.2,.35,'graphite');
}
if(part==='lamp'){
  const x=this.x;
  R([x,39,36],[x,56,36],.045,'graphite',8);R([x,38.7,36],[x,39.3,36],2.4,'brass',24);
  R([x,38.5,36],[x,38.7,36],2.15,'ivory',24);
}
return out;
