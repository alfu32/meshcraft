"""A complete fitted workspace and an ergonomic furniture assembly."""
def build(c):
    c.begin()
    c.material('oak','Worktop oak',[179,144,100,255],'meshcraft.wood.v1',dict(axis='x',ringScale=.5,grain=14,relief=.01,lightColor=[.72,.60,.42,1],darkColor=[.61,.49,.33,1]))
    for part in ['room','desk','monitors','accessories','chair','shelf','boxes','art']:
        c.part('studio','Studio | '+part,part=part)
    for row in range(2):c.part('studio',f'Studio | keyboard keys {row}',part='keyboard',row=row)
    for column,shelf in [(0,0),(1,0),(2,0),(0,3),(1,4),(2,3)]:
        c.part('studio',f'Studio | library {column} {shelf}',part='books',column=column,shelf=shelf)
    c.rpc.select([]);c.cmd('tool.builtin.linear_dimension')
    for p in [[3,7.7,13.5],[24,7.7,13.5],[13.5,7.7,16.2]]:c.rpc.pointer('click',p)
    c.rpc.pointer('cancel');c.cmd('annotation.update',size=.42)
    c.rpc.select([]);c.pose([49,37,53],[13,10,11],38)
    c.cmd('view.camera.orthographic');c.group('Construction');c.cmd('view.selection')
    c.save('projects/studio')
