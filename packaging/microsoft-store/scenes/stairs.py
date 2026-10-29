new()
material('cedar','Cedar treads',[160,100,56,255],'meshcraft.wood.v1',dict(axis='x',ringScale=2,grain=30))
material('steel','Graphite supports',[52,65,75,255])
p=lambda x,z:dict(x=x,y=0,z=z)
cmd('architecture.create_stair',name='Courtyard turning stair',contour=[p(0,0),p(4,0),p(4,8),p(12,8),p(12,12),p(0,12)],walking_path=[p(2,0),p(2,10),p(12,10)],stair_height=6,stair_steps=22,stair_support=.25,stair_left=True,stair_right=True,stair_spacing=.6,stair_levels=3,stair_tread_material='cedar',stair_support_material='steel')
turning=scene()['status']['selection'][0]
cmd('architecture.create_stair',name='Straight companion',contour=[p(16,0),p(20,0),p(20,12),p(16,12)],walking_path=[p(18,0),p(18,12)],stair_height=6,stair_steps=18,stair_support=.25,stair_left=True,stair_right=False,stair_spacing=.6,stair_levels=2,stair_tread_material='smoke-white',stair_support_material='steel')
cmd('architecture.create_rectangular_slab',start=[-1,-.15,-1],end=[22,-.15,14],slab_thickness=.2,name='Study platform')
rpc.select([turning]);pose([28,22,33],[9,2.8,6],36);cmd('view.panel.architecture_stairs.toggle');save('08-architectural-stairs')
