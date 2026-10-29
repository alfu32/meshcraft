$MATERIAL('prototypeFinish','Shared default material','smoke-white');
function onCreate(){
  $MATERIAL('finish','Instance material','ansi-grey');
}
function onAnimationFrame(params){
  return cuboid(new Point(),new Point(3,2,2),'body',params.finish);
}
