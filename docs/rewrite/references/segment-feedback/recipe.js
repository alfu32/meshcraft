// One-shot Meshcraft SDK model; select its segments in prototype edit mode.
const p=(x,z)=>new Point(x,.05,z);
const points=[p(-6,-4),p(3,-4)];
for(let i=1;i<=12;i++){const a=-90+i*90/12;points.push(p(3+3*cos(a),-1+3*sin(a)));}
points.push(p(6,4),p(-6,4));
const material=phong('#c8c6cc');
return [polyline(points,'boundary',phong('#000000'),true),
    points.map((point,i)=>face(p(0,0),points[(i+1)%points.length],point,'fill'+i,material))];
