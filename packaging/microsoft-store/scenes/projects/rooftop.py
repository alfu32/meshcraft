"""A furnished rooftop garden with five procedural surface families."""
def build(c):
    c.begin()
    c.material('oak','Terrace oak',[164,126,86,255],'meshcraft.wood.v1',dict(axis='x',ringScale=1.2,grain=30,relief=.02,lightColor=[.67,.53,.36,1],darkColor=[.49,.37,.23,1]))
    c.material('house-brick','Terrace brick',[165,98,70,255],'meshcraft.brick.v1',dict(brickWidth=2.4,brickHeight=.75,brickColor=[.55,.32,.23,1],mortarColor=[.62,.58,.5,1]))
    c.material('roof-water','Reflecting water',[40,126,143,230],'meshcraft.water.v1',dict(speed=0,amplitude=.05,resolution=8,opacity=.9))
    c.material('roof-foliage','Lush ground foliage',[69,114,46,255],'meshcraft.foliage.v1',dict(density=.5,size=2.2,height=2.6,wind=0,seed=7))
    c.material('roof-grass','Ornamental grasses',[75,112,45,255],'meshcraft.grass.v1',dict(density=2.2,height=3,width=.18,wind=0,seed=19))
    def slab(name,a,b,mat,thickness=.5):
        c.cmd('architecture.create_rectangular_slab',name=name,start=a,end=b,slab_thickness=thickness,slab_top_material=mat)
    slab('Roof | terrace paving',[0,0,0],[90,0,70],'stone',1.5)
    slab('Roof | left planting bed',[3,1.5,41],[28,1.5,66],'concrete',1.5)
    slab('Roof | foliage planting',[3.8,1.65,41.8],[27.2,1.65,65.2],'roof-foliage',.2)
    slab('Roof | meadow bed',[59,1.5,52],[86,1.5,66],'concrete',1.5)
    slab('Roof | ornamental meadow',[59.8,1.65,52.8],[85.2,1.65,65.2],'roof-grass',.2)
    slab('Roof | water surround',[30,1.3,36],[42,1.3,66],'graphite',1.3)
    slab('Roof | reflection rill',[30.5,1.5,36.5],[41.5,1.5,65.5],'roof-water',.2)
    slab('Roof | timber boardwalk',[44,.25,48],[56,.25,70],'oak',.25)
    for part in ['roof-paving','roof-pergola','roof-bar','roof-table']:
        c.part('landscape','Roof | '+part,part=part)
    for x,z in [(41,25),(63,25)]:c.part('landscape',f'Roof | lounge {x}',part='roof-sofa',x=x,z=z)
    c.tree('Roof | ornamental canopy',[15,1.7,55],0,33,10,23,1800)
    c.rpc.select([]);c.pose([134,108,143],[44,10,34],40)
    c.group('Materials');c.cmd('view.panel.materials.toggle');c.save('projects/rooftop')
