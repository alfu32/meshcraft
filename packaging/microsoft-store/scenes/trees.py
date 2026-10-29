new()
targets=[]
for name,position,form,h,r,seed,leaves in [('Broadleaf grove',[-7,0,0],0,9,4,17,6500),('Conifer',[4,0,-1],1,11,3,9,4500),('Weeping willow',[13,0,2],2,8,4,21,5000)]:
 targets.append(tree(name,position,form,h,r,seed,leaves))
rpc.select([]);pose([27,15,30],[2,3,0],36);save('07-branching-preview')
