new()
material('water','Lagoon water',[31,137,157,255],'meshcraft.water.v1',dict(speed=0,amplitude=.09,resolution=12,opacity=.95))
material('brick','Terracotta brick',[176,84,53,255],'meshcraft.brick.v1',dict(brickWidth=.9,brickHeight=.34))
material('wood','Cedar wood',[160,107,63,255],'meshcraft.wood.v1',dict(ringScale=2.8,grain=35))
material('grass','Meadow grass',[58,113,37,255],'meshcraft.grass.v1',dict(density=35,height=.65,width=.055,wind=0,seed=19))
material('leaves','Ground foliage',[62,121,40,255],'meshcraft.foliage.v1',dict(density=18,size=.55,height=.5,wind=0,seed=7))
for x,z,name in [(-8,-4,'Brick'),(0,-4,'Wood'),(8,-4,'Water'),(-4,4,'Grass'),(4,4,'Foliage')]:
 run('return cuboid(new Point(this.x-3,-.3,this.z-2.5),new Point(this.x+3,0,this.z+2.5),"plinth",phong("#e5e1d7"));',name+' plinth',dict(x=x,z=z))
# Independent mesh samples and canonical slab domains face upward.
for x,z,name,source in [(-8,-4,'brick','return cuboid(new Point(-2,0,-1.5),new Point(2,3.5,1.5),"brick");'),(0,-4,'wood','return cylinder(new Point(0,0,0),1.7,3.8,"wood",undefined,40);')]:
    obj=run('return translate(new Point('+str(x)+',0,'+str(z)+'),(()=>{'+source+'})());',name.title()+' sample')
    rpc.select(obj['targets']);cmd('material.apply',id=name)
for x,z,name in [(8,-4,'water'),(-4,4,'grass'),(4,4,'leaves')]:
    cmd('architecture.create_rectangular_slab',start=[x-2.8,.08,z-2.3],end=[x+2.8,.08,z+2.3],slab_thickness=.08,slab_top_material=name,name=name.title()+' sample')
rpc.select([]);pose([18,16,24],[0,1,0],36);cmd('view.panel.materials.toggle');save('06-procedural-materials')
