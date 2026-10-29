# Actual Extrude Swipe 3D tool: a 12-edge I-section, with two 90-degree bends.
# This recipe operates the saved direct-modeling project; it does not approximate
# the result with a scripted tube mesh.
obsolete=[o['target'] for o in scene()['objects'] if o['name'].startswith('Swept elbow')]
if obsolete:
    rpc.select(obsolete);cmd('edit.delete_selection')
source='''const points=[[-1,-.75],[-1,.75],[-.7,.75],[-.7,.15],[.7,.15],[.7,.75],[1,.75],[1,-.75],[.7,-.75],[.7,-.15],[-.7,-.15],[-.7,-.75]];
return polyline(points.map(([y,z])=>new Point(2,1.4+y,-5+z)),"I-section",phong("#ef983f"),true);'''
profile=run(source,'I-section · spatial sweep')
rpc.select(profile['targets']);cmd('selection.edit_object')
g=rpc.call('meshcraft_geometry',dict(path=profile['targets'][0]['path'],limit=1000))
rpc.select([p['target'] for p in g['primitives'] if p['target']['kind']=='segment'])
cmd('tool.builtin.extrude_swipe_3d')
for point in [[2,1.4,-5],[8,1.4,-5],[8,1.4,3],[8,6.4,3]]:
    rpc.pointer('click',point)
cmd('tool.finish')
material('sweep-amber','Sweep · amber',[239,152,63,255])
cmd('material.apply',id='sweep-amber')
cmd('selection.exit_context')
rpc.select([])
pose([18,16,24],[.5,2,-1.5],36)
save('01-direct-modeling')
