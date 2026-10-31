"""Twin-pump hydronic skid with separately editable service components."""
def build(c):
    c.begin()
    c.cmd('architecture.create_rectangular_slab',name='Plant | concrete equipment pad',start=[-3,-.5,-4],end=[96,-.5,56],slab_thickness=1.5,slab_top_material='concrete')
    for part in ['base','grating','manifold','valve','control','tank','tank-top','tank-gauge']:
        c.part('plantroom','Plant | '+part,part=part)
    for x in [18,53]:
        for part in ['motor','pump','outlet','suction','mounts']:
            c.part('plantroom',f'Pump {x} | {part}',part=part,x=x)
        for half in [0,1]:
            c.part('plantroom',f'Pump {x} | volute fasteners {half}',part='fasteners',x=x,half=half)
    # The service rail is generated with the actual spatial Sweep tool. Its
    # twelve-edge I-profile follows two right-angle changes of direction.
    profile=c.run('''const p=[[-.9,-.7],[-.9,.7],[-.6,.7],[-.6,.15],[.6,.15],[.6,.7],[.9,.7],[.9,-.7],[.6,-.7],[.6,-.15],[-.6,-.15],[-.6,-.7]];
return polyline(p.map(([y,z])=>new Point(4,20+y,46+z)),'Service I-profile',phong('#aebbc3'),true);''','Plant | spatial service rail')
    c.rpc.select(profile['targets']);c.cmd('selection.edit_object')
    geometry=c.rpc.call('meshcraft_geometry',dict(path=profile['targets'][0]['path'],limit=1000))
    c.rpc.select([p['target'] for p in geometry['primitives'] if p['target']['kind']=='segment'])
    c.cmd('tool.builtin.extrude_swipe_3d')
    for point in [[4,20,46],[84,20,46],[84,20,4],[84,38,4]]:c.rpc.pointer('click',point)
    c.cmd('tool.finish');c.cmd('material.apply',id='steel');c.cmd('selection.exit_context')
    c.rpc.select([]);c.pose([145,102,155],[44,16,25],40)
    c.group('Meshing');c.cmd('view.objects');c.save('projects/plantroom')
