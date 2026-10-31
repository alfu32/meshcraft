"""A reusable furniture system, shared script and independent configurations."""
from pathlib import Path

def build(c):
    c.begin()
    c.material('oak','Pale European oak',[182,148,103,255],'meshcraft.wood.v1',dict(axis='y',ringScale=.45,grain=15,relief=.01,lightColor=[.73,.62,.45,1],darkColor=[.64,.52,.36,1]))
    source=Path(__file__).with_name('cabinet.js').read_text()
    source=source.replace('const BASE = {width:6,height:8.8,depth:5.6};','const BASE = {width:18,height:12,depth:4.5};')
    source=source.replace("'Front', 'doors'","'Front', 'open'").replace("'Shelf contents', false","'Shelf contents', true")
    definition,first=c.prototype('Atelier | configurable storage',source)
    c.parameters(definition,first,carcass='oak',shelves=2)
    second=c.place(definition,[23,0,0]);c.parameters(definition,second,width=12,height=8.8,layout='doors',front='sage',carcass='ivory',opening=28,display=False)
    third=c.place(definition,[39,0,0]);c.parameters(definition,third,width=8,height=20,layout='open',carcass='oak',shelves=4,display=False)
    c.part('studio','Atelier | presentation floor and wall',part='casework-room')
    c.rpc.select([first]);c.pose([65,36,65],[22,8,3],38)
    c.group('Objects');c.cmd('view.object_info');c.save('projects/casework')
