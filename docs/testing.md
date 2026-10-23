good work, Summary of latest work done:
- theres no window chrome (close, minimize,maximise/restore)
- i want more functions available for the scripted objects scope. take the functions that are available in openscad. transforms over a set/list of points ( rotate(origin Point,up Point,amount float), translate(delta Point), scale(sizes Point). also construction helpers like rectangle(p0,p1), polyline(array of Point), cuboid(p0, p1), circle(center,radius[,resolution=12])
- lets implement cut/copy and  paste, lets add copy as script (copies as object script)
- lets finalise the tools: Dim, Text and Leader
- Arrange Toolbars doesnt need to exist.
- Implement the orthographic camera
- Implement walkthrough camera
- ground plane shouldnt be opaque when looked at from underneath ( so basically no double face for the ground plane, only the upper face .... easy )
- the ribbon is too tall, id make it roughly 50-60% of the current height.
- we need a flip functionality that flips both face normals and segments direction.
- selected segments should display an arrow indicating their diirection ( which is important for path-sweep and maybe other tools too ( like cuttiing holes or extrude ).
- midpoint snap doesnt work anymore
- the ribbon was shortened too much , add 15 px to the current height
- paste should have 2 variants: paste in place which does exactly what paste does now and normal paste will create a floating bundle of what is pasted, anchored at the bottom-left front corner of its bounding box and the user will be required to pick an insertion point. the wireframe of the floating bundle will be shown after entering the command and before picking the insertion point

Completed through 0.12.0:

- Restored window controls; removed Arrange Toolbars.
- Expanded scripting with transforms and geometry helpers.
- Added Cut, Copy, Copy as Script, floating Paste, and Paste in Place.
- Completed Dim, Text, and Leader.
- Added orthographic and walkthrough cameras.
- Made the ground visible only from above.
- Adjusted the ribbon height, including the requested extra 15 px.
- Added Flip for face normals and segment direction, plus selected-segment arrows.
- Restored midpoint snapping.

Committed and pushed. 528 tests pass, alongside strict Clippy checks and native UI verification.