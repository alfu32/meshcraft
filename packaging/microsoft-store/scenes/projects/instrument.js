// A serviceable benchtop process controller, in millimetres. Separate jobs keep
// component generation inside the app's normal script execution budget.
const part=this.part;
if(part==='chassis'){
  B(0,0,0,240,5,160,'steel');B(0,5,0,5,45,160,'navy');B(235,5,0,5,45,160,'navy');
  B(5,5,0,230,45,4,'navy');B(5,5,156,230,45,4,'steel');
}
if(part==='feet')for(const x of [this.side?230:10])for(const z of [12,148]){R([x,-6,z],[x,0,z],6,'rubber',20);R([x,5,z],[x,45,z],3.2,'steel',16);}
if(part==='vents')for(const x of [0,235])for(let z=20;z<145;z+=9)B(x-1,20,z,7,2.6,5.3,'graphite');
if(part==='fascia'){
  B(28,14,160.3,118,30,1.5,'graphite');B(31,17,162,112,24,.6,'screen');
  for(let i=0;i<7;i++)B(37+i*13,20,162.8,8,2,.2,i<5?'cyan':'navy');
  B(37,35,162.8,74,1,.2,'cyan');B(37,31,162.8,50,1,.2,'cyan');
  for(const x of [12,228])for(const y of [12,42])Bolt([x,y,160],[x,y,163],1.7);
}
if(part==='controls'){
  Ring([188,28,160],[188,28,165],13,9,'steel',32);
  R([188,28,163],[188,28,173],9.5,'graphite',32);B(187.4,33,173,1.2,3,.6,'ivory');
  for(const x of [157,217]){R([x,19,160],[x,19,165],4.1,'steel',20);R([x,19,165],[x,19,166],3.1,x===157?'amber':'cyan',20);}
  for(const x of [155,217])for(const y of [37,45])R([x,y,160],[x,y,162],1.8,'rubber',12);
}
if(part==='electronics'){
  B(15,8,16,210,1.8,123,'pcb');
  B(25,10,28,45,20,36,'graphite');B(27,30,30,41,1,32,'steel');
  for(let i=0;i<10;i++)B(84+i*4.5,11,26,2.1,22,46,'steel');
  B(82,10,24,51,3,50,'graphite');B(150,10,24,57,17,32,'ivory');
  for(let i=0;i<8;i++)B(151+i*7,28,28,4,4,22,'graphite');
}
if(part==='capacitors')for(let i=this.side*3;i<this.side*3+3;i++){R([28+i*16,10,105],[28+i*16,24,105],5.3,'navy',20);B(26+i*16,24.1,103,4,.2,4,'steel');}
if(part==='terminals'){
  for(const x of [22,218])for(const z of [22,130])Bolt([x,9,z],[x,12,z],2);
  for(let i=0;i<6;i++)B(143+i*10,10,111,6,7,13,'amber');
}
if(part==='lid'){
  const y=103;
  B(-3,y,-2,246,3,40,'navy');B(-3,y,129,246,3,35,'navy');
  B(-3,y,38,148,3,91,'navy');B(222,y,38,21,3,91,'navy');
  for(let z=39;z<129;z+=6)B(145,y,z,77,3,2.1,'navy');
  B(48,y+3.1,9,55,.3,17,'steel');B(52,y+3.5,12,46,.1,3,'graphite');B(52,y+3.5,18,29,.1,2,'graphite');
}
if(part==='lid-bolts')for(const x of [this.side?231:9])for(const z of [10,152]){Ring([x,106,z],[x,106.8,z],4,2.2,'steel',16);Bolt([x,107,z],[x,109,z],2.1);}
if(part==='connector'){
  const x=35+this.side*33;R([x,24,0],[x,24,-5],8.4,'steel',24);Ring([x,24,-5],[x,24,-8],7,4.8,'graphite',20);
}
if(part==='rear'){
  B(164,14,-3,42,20,3,'graphite');R([219,36,-2],[219,36,0],2,'amber',12);
}
return out;
