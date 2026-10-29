new()
result=rpc.call('meshcraft_object_script',dict(action='create',api_version=2,
    source=(root/'packaging/microsoft-store/scenes/dynamic-pavilion.js').read_text()))
time.sleep(1)
definition=result['definition_id'];first=scene()['objects'][-1]['target']
move(first,[-9,0,0]);second=place(definition,[0,0,0]);third=place(definition,[9,0,0])
for target,id,value in [(first,'width',6),(second,'height',5),(second,'finish','amber'),
                         (third,'width',7),(third,'finish','ivory')]:
    rpc.call('meshcraft_parameters',dict(action='set',definition_id=definition,
        paths=[target['path']],id=id,value=value))
rpc.select([second]);pose([25,15,27],[3,2,2],36);save('02-dynamic-instances')
