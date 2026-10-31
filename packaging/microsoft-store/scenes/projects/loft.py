"""A furnished live/work loft with a native contour/path stair and mezzanine."""
def build(c):
    c.begin()
    c.material('loft-brick', 'Soft red masonry', [165,112,86,255], 'meshcraft.brick.v1',
               dict(brickWidth=2.4,brickHeight=.75,brickColor=[.55,.34,.25,1],mortarColor=[.59,.55,.48,1],variation=.12))
    c.material('oak', 'Natural oak', [182,141,94,255], 'meshcraft.wood.v1',
               dict(axis='x',ringScale=1.1,grain=25,relief=.015,lightColor=[.72,.59,.42,1],darkColor=[.57,.44,.29,1]))
    c.cmd('architecture.create_rectangular_slab', name='Loft | ground floor',start=[0,0,0],end=[70,0,55],slab_thickness=1.5,slab_top_material='concrete')
    c.cmd('architecture.create_wall', name='Loft | brick back wall',start=[0,0,0],end=[70,0,0],wall_height=58,wall_thickness=2,
          wall_exterior_material='loft-brick',wall_interior_material='loft-brick')
    c.cmd('architecture.add_hole',start=[8,34,0],end=[62,54,0])
    c.cmd('architecture.create_frame',name='Loft | factory clerestory',start=[8,34,0],end=[62,54,0],normal=[0,0,1],frame_width=.5,frame_depth=.9,frame_material='graphite',glazing_material='glass')
    c.cmd('architecture.create_wall', name='Loft | west wall',start=[0,0,0],end=[0,0,55],wall_height=58,wall_thickness=2,
          wall_exterior_material='ivory',wall_interior_material='ivory')
    c.cmd('architecture.add_hole',start=[0,8,8],end=[0,44,45])
    c.cmd('architecture.create_frame',name='Loft | tall west glazing',start=[0,8,8],end=[0,44,45],normal=[1,0,0],frame_width=.5,frame_depth=.9,frame_material='graphite',glazing_material='glass')
    c.cmd('architecture.create_rectangular_slab',name='Loft | oak mezzanine',start=[0,27,0],end=[35,27,25],slab_thickness=1.5,slab_top_material='oak')
    p=lambda x,z:dict(x=x,y=0,z=z)
    c.cmd('architecture.create_stair',name='Loft | turning access stair',
          contour=[p(x,z) for x,z in [(50,55),(60,55),(60,15),(35,15),(35,25),(50,25)]],
          walking_path=[p(55,55),p(55,20),p(35,20)],stair_height=27,stair_steps=20,stair_support=.6,
          stair_left=True,stair_right=True,stair_spacing=3.5,stair_levels=3,stair_tread_material='oak',stair_support_material='graphite')
    stair=c.rpc.status()['selection'][0]
    for part in ['structure','railing','duct','mullions','sofa','table','desk','shelf']:
        c.part('loft','Loft | '+part,part=part)
    for i,(x,z) in enumerate([(8,32),(26,32),(8,48),(26,48)]):
        c.part('loft',f'Loft | dining chair {i+1}',part='chair',x=x,z=z)
    for x in [12,27,45]:
        c.part('loft',f'Loft | pendant {x}',part='lamp',x=x)
    c.rpc.select([stair])
    c.pose([108,87,126],[29,23,21],39)
    c.group('Civil')
    c.cmd('view.panel.architecture_stairs.toggle')
    c.save('projects/loft')
