new()
material('obsidian','Obsidian',[43,55,64,255])
material('sand','Warm plaster',[225,214,190,255])
material('cedar','Cedar',[168,102,48,255],'meshcraft.wood.v1',{})
material('pool','Pool water',[30,144,175,220],'meshcraft.water.v1',dict(speed=0,amplitude=.055,opacity=.88,resolution=10))
material('lawn','Meadow',[78,119,45,255],'meshcraft.grass.v1',dict(density=18,height=.24,width=.025,wind=0))
cmd('architecture.create_rectangular_slab',start=[-2,.4,-2],end=[22,.4,17],slab_thickness=.4,slab_top_material='side-grey',name='Landscape terrace')
cmd('architecture.create_rectangular_slab',start=[0,.7,0],end=[18,.7,10],slab_thickness=.3,slab_top_material='cedar',name='Living floor')
for a,b,n in [([0,.7,0],[18,.7,0],'Garden wall'),([0,.7,0],[0,.7,10],'West wall'),([18,.7,0],[18,.7,10],'East wall')]:
 cmd('architecture.create_wall',start=a,end=b,wall_height=4.8,wall_thickness=.35,wall_offset=.5,wall_exterior_material='sand',wall_interior_material='smoke-white',name=n)
cmd('architecture.create_wall',start=[0,.7,10],end=[18,.7,10],wall_height=4.8,wall_thickness=.35,wall_exterior_material='sand',name='Glazed frontage')
cmd('architecture.add_hole',start=[1,1.2,10],end=[7.4,4.9,10])
cmd('architecture.add_hole',start=[9,.7,10],end=[17,5,10])
for i,(a,b) in enumerate([(1,3.08),(3.16,5.24),(5.32,7.4),(9,11.6),(11.7,14.3),(14.4,17)]):
 cmd('architecture.create_frame',start=[a,1.2 if a<8 else .7,10.2],end=[b,4.9 if a<8 else 5,10.2],normal=[0,0,1],frame_width=.075,frame_depth=.16,frame_material='obsidian',glazing_material='glass',name='Courtyard glazing '+str(i))
cmd('architecture.create_rectangular_slab',start=[-.7,5.6,-.7],end=[18.7,5.6,10.7],slab_thickness=.3,slab_top_material='smoke-white',name='Cantilever roof')
cmd('architecture.create_rectangular_slab',start=[-1,.65,10.7],end=[19,.65,14],slab_thickness=.25,slab_top_material='cedar',name='Cedar deck')
cmd('architecture.create_rectangular_slab',start=[21,.42,0],end=[29,.42,12],slab_thickness=.2,slab_top_material='pool',name='Reflecting pool')
cmd('architecture.create_rectangular_slab',start=[-8,.42,-2],end=[-3,.42,13],slab_thickness=.2,slab_top_material='lawn',name='Meadow strip')
cmd('architecture.create_stair',contour=[dict(x=x,y=y,z=z) for x,y,z in [(7,0,14),(12,0,14),(12,0,17),(7,0,17)]],walking_path=[dict(x=9.5,y=0,z=17),dict(x=9.5,y=0,z=14)],stair_height=.65,stair_steps=4,stair_left=False,stair_right=False,stair_levels=0,stair_tread_material='side-grey',name='Entrance steps')
# Mesh furniture and pergola: generated through the same MCP one-shot SDK.
run('''const p=(x,y,z)=>new Point(x,y,z),out=[];const box=(a,b,id,c)=>out.push(cuboid(p(...a),p(...b),id,phong(c)));
for(let i=0;i<17;i++)box([1+i*.43,5.72,7],[1.18+i*.43,6.12,10.6],'slat'+i,'#b98751');
box([2,1,11.2],[5.8,1.35,12.6],'bench','#414c54');box([2.2,.7,11.4],[2.4,1,12.4],'leg0','#414c54');box([5.4,.7,11.4],[5.6,1,12.4],'leg1','#414c54');
box([10,1.2,11],[13,1.35,12.5],'table','#dfc28c');
for(let x of [10.1,12.6])for(let z of [11.1,12.1])box([x,.7,z],[x+.2,1.2,z+.2],'tableleg'+x+z,'#414c54');return out;''','Courtyard furnishings')
for name,pos,form,h,r,seed in [('Garden tree',[-5,.45,3],0,8,3.2,13),('Terrace tree',[22,.45,-2],1,10,2.5,31)]:
 tree(name,pos,form,h,r,seed)
rpc.select([])
pose([43,30,47],[10,2.6,6],36)
cmd('view.panel.materials.toggle')
save('03-architecture')
