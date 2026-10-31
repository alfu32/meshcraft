"""Two-storey courtyard residence: native architecture, finishes and site design."""
def build(c):
    c.begin()
    c.material('house-brick', 'Terracotta masonry', [175, 101, 73, 255],
               'meshcraft.brick.v1', dict(brickWidth=2.4, brickHeight=.75, mortar=.075))
    c.material('house-deck', 'Oiled oak decking', [158, 117, 76, 255], 'meshcraft.wood.v1', dict(axis='x', grain=28))
    c.material('house-water', 'Reflecting pool', [39, 119, 142, 220], 'meshcraft.water.v1', dict(speed=0, amplitude=.06, opacity=.85, resolution=8))
    c.material('house-grass', 'Meadow planting', [72, 105, 52, 255], 'meshcraft.grass.v1', dict(density=.6, height=.5, width=.05, wind=0))

    def slab(name, a, b, material, thickness=2):
        c.cmd('architecture.create_rectangular_slab', name=name, start=a, end=b,
              slab_thickness=thickness, slab_top_material=material, slab_side_material=material)

    def wall(name, a, b, height, exterior='ivory', openings=(), normal=(0,0,1)):
        c.cmd('architecture.create_wall', name=name, start=a, end=b, wall_height=height,
              wall_thickness=2.4, wall_offset=.5, wall_exterior_material=exterior,
              wall_interior_material='ivory')
        owner = c.rpc.status()['selection']
        for i,(p,q) in enumerate(openings):
            c.rpc.select(owner)
            c.cmd('architecture.add_hole', start=p, end=q)
            # Windows remain independent frame elements; wall holes are explicitly owned.
            c.cmd('architecture.create_frame', name=f'{name} | glazing {i+1}', start=p, end=q,
                  normal=list(normal), frame_width=.55, frame_depth=1.2,
                  frame_material='graphite', glazing_material='glass')

    slab('Foundation and ground floor', [0,.5,0], [200,.5,65], 'stone', 2.4)
    slab('East wing floor', [150,.5,65], [200,.5,125], 'stone', 2.4)
    slab('West wing floor', [0,.5,65], [45,.5,115], 'stone', 2.4)
    wall('Rear enclosure', [0,.5,0], [200,.5,0], 31)
    wall('West enclosure', [0,.5,0], [0,.5,115], 31, openings=[([0,6,18],[0,26,49])], normal=(-1,0,0))
    wall('East masonry', [200,.5,0], [200,.5,125], 31, 'house-brick',
         [([200,5,18],[200,26,54]),([200,5,74],[200,26,113])], (1,0,0))
    wall('Living frontage', [45,.5,65], [150,.5,65], 31, openings=[
         ([49,2,65],[72,29,65]),([74,2,65],[97,29,65]),([99,2,65],[122,29,65]),([124,2,65],[147,29,65])])
    wall('West wing courtyard', [45,.5,65], [45,.5,115], 31, openings=[([45,2,72],[45,29,108])], normal=(1,0,0))
    wall('West wing front', [0,.5,115], [45,.5,115], 31, openings=[([4,3,115],[41,28,115])])
    wall('East wing courtyard', [150,.5,65], [150,.5,125], 31, openings=[([150,2,72],[150,29,117])], normal=(-1,0,0))
    wall('East wing front', [150,.5,125], [200,.5,125], 31, 'house-brick', [([154,2,125],[196,29,125])])
    slab('Upper floor and sheltered terrace', [-1,33,0], [130,33,86], 'concrete', 2.4)
    wall('Upper rear', [0,33,0], [112,33,0], 27)
    wall('Upper west', [0,33,0], [0,33,65], 27, openings=[([0,38,12],[0,55,49])], normal=(-1,0,0))
    wall('Upper east', [112,33,0], [112,33,65], 27, 'house-brick')
    wall('Upper courtyard frontage', [0,33,65], [112,33,65], 27, openings=[
         ([5,35,65],[35,58,65]),([39,35,65],[69,58,65]),([73,35,65],[107,58,65])])
    slab('Upper roof', [-4,63,-4], [116,63,70], 'ivory', 2.2)
    slab('East wing roof', [146,33,-3], [204,33,129], 'ivory', 2.2)
    slab('West wing roof', [-4,33,64], [47,33,119], 'ivory', 2.2)
    slab('Courtyard deck', [45,.35,65], [150,.35,143], 'house-deck', .35)
    slab('Pool surround', [52,.45,101], [141,.45,144], 'stone', .5)
    slab('Reflecting pool water', [56,.6,105], [137,.6,140], 'house-water', .25)
    slab('Paved approach', [-12,.15,125], [208,.15,155], 'stone', .5)
    for x1,x2 in [(-30,-13),(211,241)]:
        slab('Meadow border', [x1,.1,-12], [x2,.1,155], 'house-grass', .2)
    c.part('residence', 'Residence | terrace railing and mullions', part='railing')
    c.part('residence', 'Residence | timber terrace pergola', part='pergola')
    for row in range(3):
        c.part('residence', f'Residence | photovoltaic row {row+1}', part='solar', row=row)
    c.part('residence', 'Residence | outdoor table', part='table')
    for x in [65,101]:
        c.part('residence', f'Residence | outdoor chair {x}', part='chair', x=x)
    c.part('residence', 'Residence | entry paving and facade ribs', part='site')
    for name,pos,form,h,r,seed in [('West garden canopy',[-22,.3,96],0,65,18,13),
                                  ('East garden canopy',[228,.3,139],0,58,17,41),
                                  ('Screening conifer',[226,.3,22],1,72,14,8)]:
        c.tree(name,pos,form,h,r,seed,3500)
    c.rpc.select([])
    c.pose([343,230,365],[105,27,76],42)
    c.group('Civil')
    c.cmd('view.architecture_settings')
    c.save('projects/residence')
