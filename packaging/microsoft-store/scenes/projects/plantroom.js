const part=this.part,x=this.x||0;
if(part==='frame-posts')for(const [x,z] of [[4,46],[84,46],[84,4]]){B(x-.5,1.8,z-.5,1,18.2,1,'steel');B(x-1,1.8,z-1,2,.3,2,'graphite');}
if(part==='base'){
  for(const z of [0,47])B(0,0,z,92,1.4,1.7,'graphite');
  for(const a of [0,45,90])B(a,0,0,1.5,1.4,48,'graphite');
  for(const a of [3,44,87])for(const z of [2,44])B(a,-.4,z,2,.4,2,'rubber');
}
if(part==='grating')for(let a=1;a<90;a+=3)B(a,1.4,1,1.1,.35,46,'steel');
if(part==='motor'){
  R([x,7,5],[x,7,19],3.1,'navy',28);
  for(let i=0;i<12;i++){
    const start=out.length;B(x-.12,10.1,5.4,.24,.7,13,'navy');
    out.push(rotate(P(x,7,0),P(0,0,1),i*30,out.splice(start)));
  }
  R([x,7,18.8],[x,7,21.5],1.4,'amber',20);
  Ring([x,7,4],[x,7,5],3.4,2.4,'graphite',20);
}
if(part==='pump'){
  R([x,7,22],[x,7,27],4.7,'navy',32);Ring([x,7,27],[x,7,28.2],4.1,1.65,'steel',24);
  R([x+2.9,9.5,24.5],[x+2.9,14,24.5],1.5,'navy',20);
}
if(part==='mounts'){
  B(x-4,1.8,3,8,1.1,25,'navy');
  for(const a of [x-3.3,x+3.3])for(const z of [5,24])Bolt([a,2.9,z],[a,3.6,z],.35);
  B(x-2.5,2.9,7,1,1.2,10,'navy');B(x+1.5,2.9,7,1,1.2,10,'navy');
}
if(part==='fasteners')for(let i=this.half*4;i<this.half*4+4;i++){
  const a=i*Math.PI/4,xx=x+3.5*Math.cos(a),y=7+3.5*Math.sin(a);
  Bolt([xx,y,28.2],[xx,y,28.9],.32);
}
if(part==='outlet'){
  Pipe([[x+2.9,14,24.5],[x+2.9,18,24.5],[x+2.9,20,22.5],[x+2.9,20,8],[x+2.9,22,6]],1.05,'steel',16);
  Ring([x+2.9,14.1,24.5],[x+2.9,14.8,24.5],2.1,1.05,'graphite',20);
}
if(part==='suction'){
  Pipe([[x,7,28],[x,7,37],[x,7,40],[x+2,7,42]],1.4,'steel',16);
  Ring([x,7,30],[x,7,30.7],2.25,1.4,'graphite',20);
}
if(part==='manifold'){
  R([2,22,6],[76,22,6],1.25,'steel',24);R([2,7,42],[76,7,42],1.45,'steel',24);
  for(const a of [4,74]){B(a,1.8,5,1,19,1,'graphite');B(a,1.8,41,1,3.7,1,'graphite');}
}
if(part==='valve'){
  R([35,7,42],[39,7,42],2.1,'navy',24);R([37,9,42],[37,15,42],.3,'brass',16);
  Ring([37,15,42],[37,15.35,42],2.5,2.15,'amber',24);
  for(let i=0;i<3;i++){const a=i*2*Math.PI/3;R([37,15.2,42],[37+2.3*Math.cos(a),15.2,42+2.3*Math.sin(a)],.1,'amber',8);}
}
if(part==='control'){
  B(69,1.8,27,1,10,1,'graphite');B(80,1.8,27,1,10,1,'graphite');B(67,11,25,16,16,4,'ivory');
  B(69,19,29.1,12,5,.1,'graphite');B(69.5,19.5,29.3,11,4,.05,'screen');B(70,20,29.4,8,.35,.03,'cyan');
  for(const a of [71,76,80])R([a,15,29],[a,15,29.8],.6,a===80?'ansi-red':'cyan',16);
}
if(part==='tank'){
  R([86,5,12],[86,35,12],5.1,'steel',32);
  for(const a of [82,90])B(a,1.8,10,.6,3.2,5,'graphite');
  Ring([86,4.5,12],[86,5,12],5.4,4.8,'graphite',24);
}
if(part==='tank-top'){
  out.push(translate(P(86,35,12),scale(P(5.1,2.7,5.1),sphere(P(),1,ID('tank-head'),M('steel'),20))));
  R([86,37,12],[86,39,12],.8,'steel',16);B(82,22,17.2,8,3,.1,'navy');
}
if(part==='tank-gauge'){
  R([86,9,17],[86,30,17],.2,'cyan',12);R([85.5,9,16.6],[86.5,9,17],.3,'brass',12);
  Pipe([[76,22,6],[83,22,6],[86,19,7],[86,14,7]],1,'steel',16);
  R([10,22,6],[10,26,6],.2,'brass',12);R([10,26,6],[10,26,7.2],1.2,'graphite',24);R([10,26,7.2],[10,26,7.35],1,'ivory',24);
  E([10,26,7.4],[10.6,26.4,7.4],'graphite');
}
return out;
