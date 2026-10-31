"""Pocket-park landscape and a shared, genuinely editable GPU branching program."""
def build(c):
    c.begin()
    program=next(p for p in c.scene()['proceduralPrograms'] if p['id']=='meshcraft.branching.v1')
    program['id']='project.canopy.v1';program['name']='Landscape canopy'
    defaults=dict(height=60,radius=19,trunkRadius=1.5,leafSize=2.4,leaves=2200,wind=0,
                  branches=6,forks=3,levels=3,minPixels=.25,leafColor=[.22,.38,.11,1],barkColor=[.27,.20,.14,1])
    for parameter in program['parameters']:
        if parameter['id'] in defaults:parameter['default']=defaults[parameter['id']]
    program['vertex']=(__import__('pathlib').Path(__file__).with_name('canopy.glsl').read_text())
    c.cmd('material.program',program=program)
    c.tree_program=program['id']
    c.material('park-grass','Park meadow',[69,108,47,255],'meshcraft.grass.v1',dict(density=.35,height=.7,width=.07,wind=0))
    c.cmd('architecture.create_rectangular_slab',name='Park | paving base',start=[-50,0,-40],end=[100,0,65],slab_thickness=1.5,slab_top_material='stone')
    for i,(a,b) in enumerate([([-45,.15,-32],[-8,.15,8]),([13,.15,-32],[92,.15,8]),([-45,.15,30],[-8,.15,60]),([20,.15,30],[92,.15,60])]):
        c.cmd('architecture.create_rectangular_slab',name=f'Park | planted island {i+1}',start=a,end=b,slab_thickness=.3,slab_top_material='park-grass')
    c.part('landscape','Park | path joints and kerbs',part='park-paving')
    for x,z in [(-33,17),(19,17),(42,17)]:
        c.part('landscape',f'Park | timber bench {x}',part='bench',x=x,z=z)
    for x,z in [(6,-25),(6,48),(87,17)]:
        c.part('landscape',f'Park | lighting {x} {z}',part='light',x=x,z=z)
    for name,pos,form,h,r,seed,leaves in [('Canopy oak',[-23,.2,-9],0,60,19,17,2200),
                                         ('Columnar pine',[39,.2,-14],1,73,16,8,2400),
                                         ('Weeping canopy',[64,.2,46],2,54,20,31,2200),
                                         ('Compact planting',[-27,.2,46],3,17,10,27,1200)]:
        c.tree(name,pos,form,h,r,seed,leaves)
    c.rpc.select([]);c.pose([151,105,174],[23,25,12],40)
    c.group('Objects');c.save('projects/park')
