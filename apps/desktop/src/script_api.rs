//! Shared, offline JavaScript API documentation used by editor completion and MCP.
#[derive(Clone, serde::Serialize)]
pub(crate) struct Entry {
    pub name: String,
    pub signature: String,
    #[serde(rename = "example")]
    pub insert: String,
    #[serde(rename = "description")]
    pub help: String,
}
pub(crate) fn entry(name: &str, signature: &str, insert: &str, help: &str) -> Entry {
    Entry {
        name: name.into(),
        signature: signature.into(),
        insert: insert.into(),
        help: help.into(),
    }
}
pub(crate) fn is_declaration(name: &str) -> bool {
    matches!(
        name,
        "$TEXT" | "$NUMBER" | "$CHECKBOX" | "$LIST" | "$MATERIAL" | "$HOTSPOT" | "$IFC"
    )
}
pub(crate) fn prioritize(entries: &mut [Entry]) {
    entries.sort_by_key(|e| !is_declaration(&e.name));
}
pub(crate) fn catalog() -> Vec<Entry> {
    let mut entries = vec![
        entry(
            "$TEXT",
            "$TEXT(id, label=id, defaultValue='')",
            "$TEXT('caption', 'Caption', 'Panel');",
            "Declare a text model parameter. Top level: shared prototype value; onCreate: independent instance value. API v2 onAnimationFrame(params,model) reads params.caption. IDs are unique across both scopes; labels/default changes preserve authored compatible values.",
        ),
        entry(
            "$NUMBER",
            "$NUMBER(id, label=id, defaultValue=0)",
            "$NUMBER('width', 'Width', 3);",
            "Declare a finite numeric model parameter in internal model units. Outside hooks registers a prototype parameter; onCreate registers an instance parameter. Read its value through the API v2 frame parameters.",
        ),
        entry(
            "$CHECKBOX",
            "$CHECKBOX(id, label=id, defaultValue=false)",
            "$CHECKBOX('enabled', 'Enabled', true);",
            "Declare a boolean model parameter. The Object Properties panel renders its checkbox; values are model data and support save/copy/Undo.",
        ),
        entry(
            "$LIST",
            "$LIST(id, values, label=id, defaultValue=firstKey)",
            "$LIST('style', {solid: 'Solid', open: 'Open'}, 'Style', 'solid');",
            "Declare a single-choice model parameter. values maps stable stored string keys to display labels; the frame receives the chosen key. Omitted default selects the first key. Declare outside hooks or in onCreate, never in onAnimationFrame.",
        ),
        entry(
            "$MATERIAL",
            "$MATERIAL(id, label=id, defaultValue='ansi-grey')",
            "$MATERIAL('finish', 'Material', 'ansi-grey');",
            "Declare a material picker backed by the current model catalog, including procedural materials. Outside hooks: shared prototype choice; inside onCreate: independent instance choice. The stored value is a material ID; onAnimationFrame receives the resolved Material as params[id], ready for geometry helpers. Choices update when catalog materials are added or renamed. Unknown saved IDs keep their authored value and render the ANSI grey fallback until reassigned. Declare outside hooks or in onCreate, never in onAnimationFrame.",
        ),
        entry(
            "Point",
            "new Point(x=0, y=0, z=0, id='')",
            "new Point(0, 0, 0)",
            "Local model-unit coordinates. An optional ID keeps the vertex identity stable as it moves.",
        ),
        entry(
            "Color",
            "new Color(red=255, green=255, blue=255, alpha=255)",
            "new Color(255, 255, 255, 255)",
            "RGBA channels from 0 to 255. Colors also accept #RRGGBB and #RRGGBBAA strings.",
        ),
        entry(
            "Material",
            "new Material(color, texturePath=null)",
            "new Material('#83bdeaff')",
            "Fields: id, name, color, shadingType, texturePath, textureScaleU, textureScaleV. Initial rendering uses color/alpha.",
        ),
        entry(
            "phong",
            "phong(color) → Material",
            "phong('#83bdeaff')",
            "Create a material with Phong shading metadata. Color and alpha are rendered in this phase.",
        ),
        entry(
            "gouraud",
            "gouraud(color) → Material",
            "gouraud('#83bdeaff')",
            "Create a material with Gouraud shading metadata. Optional texture settings remain metadata.",
        ),
        entry(
            "Segment",
            "new Segment(p0, p1, id, material?)",
            "new Segment(new Point(), new Point(1, 0, 0), this.id + ':edge')",
            "A line segment with a nonempty stable ID. An absent material uses opaque black.",
        ),
        entry(
            "line",
            "line(p0, p1, id, material?) → Segment",
            "line(new Point(), new Point(1, 0, 0), this.id + ':edge')",
            "Same as new Segment; the default line color is opaque black. Point arguments have x/y/z coordinates. Each segment ID must be unique in the emitted array.",
        ),
        entry(
            "Face",
            "new Face(a, b, c, id, material?)",
            "new Face(new Point(), new Point(1, 0, 0), new Point(0, 1, 0), this.id + ':face')",
            "A triangle in local coordinates; absent material uses ANSI grey (#c0c0c0). Vertex winding controls its normal. Its ID must be unique.",
        ),
        entry(
            "face",
            "face(a, b, c, id, material?) → Face",
            "face(new Point(), new Point(1, 0, 0), new Point(0, 1, 0), this.id + ':face')",
            "Same as new Face. Return this drawable or a nested array containing it from onAnimationFrame.",
        ),
        entry(
            "$HOTSPOT",
            "$HOTSPOT(position, id, label=id, shape=$HOTSPOT_SHAPE_CIRCLE, color='#00ffffff') → Hotspot",
            "$HOTSPOT(new Point(3, 0, 0), 'width')",
            "Declare a model hotspot outside hooks or in onCreate (API v2). It is emitted automatically; params[id].position is the current per-instance dragged position. In onAnimationFrame it can also return an explicit hotspot drawable, which must be included in returned geometry. Shape uses a $HOTSPOT_SHAPE_ constant (CIRCLE, RECTANGLE, TRIANGLE, DIAMOND or TRIANGLE_DOWN); color is #RRGGBB or #RRGGBBAA, default cyan. All IDs must be stable and declared IDs are unique across inputs and hotspots.",
        ),
        entry(
            "polyline",
            "polyline(points, id?, material?, closed=false) → Segment[]",
            "polyline([new Point(), new Point(1, 0, 0)], 'path')",
            "Connect consecutive points; closed=true adds the final edge. Child IDs are prefixed by id.",
        ),
        entry(
            "circle",
            "circle(center, radius, resolution=12) / circle(center, radius, id, material?, segments=12) → Segment[]",
            "circle(new Point(), 1, 12)",
            "A closed polygonal circle on the local XZ plane. Radius is in model units; segments are clamped to 3–512.",
        ),
        entry(
            "cube",
            "cube(origin, size, id, material?) → Drawable[]",
            "cube(new Point(), new Point(3, 2, 1), 'body', phong('#83bdeaff'))",
            "Create triangular faces and boundary edges. Size accepts a Point or one scalar for all axes.",
        ),
        entry(
            "sphere",
            "sphere(center, radius, id, material?, segments=24) → Face[]",
            "sphere(new Point(), 1, 'sphere')",
            "A triangulated sphere. Radius is in model units; segments are clamped to 4–128.",
        ),
        entry(
            "cone",
            "cone(origin, radius, height, id, material?, segments=32) → Face[]",
            "cone(new Point(), 1, 2, 'cone')",
            "A closed triangulated cone along local +Y. Radius/height are model-unit distances.",
        ),
        entry(
            "rectangle",
            "rectangle(p0, p1, id?, material?) → Drawable[]",
            "rectangle(new Point(), new Point(3, 0, 2))",
            "Filled axis-aligned rectangle with boundary edges. Corners share X, Y or Z.",
        ),
        entry(
            "cuboid",
            "cuboid(p0, p1, id?, material?) → Drawable[]",
            "cuboid(new Point(), new Point(3, 2, 1))",
            "Solid box between opposite corners. Optional helper IDs are generated in call order.",
        ),
        entry(
            "translate",
            "translate(delta, items?) / translate(delta)(items)",
            "translate(new Point(1, 0, 0))(this.geometry)",
            "Transform points, drawables or nested lists without mutating inputs. IDs are retained; use withIds when emitting originals and copies together.",
        ),
        entry(
            "scale",
            "scale(sizes, items?) / scale(sizes)(items)",
            "scale(new Point(2, 1, 1))(this.geometry)",
            "Component-wise model-unit scaling about the origin. A scalar scales all axes. Reflections correct triangle winding.",
        ),
        entry(
            "rotate",
            "rotate(origin, up, degrees, items?) / rotate(eulerDegrees)(items)",
            "rotate(new Point(), new Point(0, 1, 0), 90)(this.geometry)",
            "Rotate about an origin and axis direction, in degrees. Also supports OpenSCAD-style Euler XYZ angles or rotate(degrees, axis).",
        ),
        entry(
            "mirror",
            "mirror(normal, items?) → transform / geometry",
            "mirror(new Point(1, 0, 0))(this.geometry)",
            "Reflect across the plane through the origin with this normal.",
        ),
        entry(
            "multmatrix",
            "multmatrix(matrix4x4, items?) → transform / geometry",
            "multmatrix([[1,0,0,0],[0,1,0,0],[0,0,1,0],[0,0,0,1]])(this.geometry)",
            "Apply a 4×4 matrix to Point values, drawables or nested lists.",
        ),
        entry(
            "withIds",
            "withIds(items, prefix) → geometry",
            "withIds(translate(new Point(3,0,0))(this.geometry), 'copy')",
            "Prefix drawable and shared point IDs to emit independent copies alongside their source.",
        ),
        entry(
            "cylinder",
            "cylinder(origin, radius, height, id, material?, segments=32, topRadius=radius)",
            "cylinder(new Point(), 1, 3, 'cylinder')",
            "Closed Y-axis cylinder or truncated cone, including boundary edges.",
        ),
        entry(
            "polygon",
            "polygon(points, id, material?) → Drawable[]",
            "polygon([new Point(),new Point(2,0,0),new Point(0,0,2)], 'polygon')",
            "Triangulate a simple planar polygon, including concave outlines, and emit its boundary.",
        ),
        entry(
            "square",
            "square(origin, sizes, id, material?) → Drawable[]",
            "square(new Point(), new Point(2, 0, 3), 'square')",
            "Filled rectangle on the XZ plane.",
        ),
        entry(
            "polyhedron",
            "polyhedron(points, faces, id, material?) → Drawable[]",
            "polyhedron([new Point(),new Point(1,0,0),new Point(0,1,0)], [[0,1,2]], 'mesh')",
            "Build a mesh from indexed planar polygon faces.",
        ),
        entry(
            "linear_extrude",
            "linear_extrude(height, profilePoints, id, material?) → Drawable[]",
            "linear_extrude(2, [new Point(),new Point(2,0,0),new Point(0,0,2)], 'solid')",
            "Extrude an XZ-plane polygon along Y, with caps and side faces.",
        ),
        entry(
            "color",
            "color(tint, items?) / color(tint)(items)",
            "color('#ffaa4488')(this.geometry)",
            "Apply a material/color without changing coordinates; accepts hex, basic color names, Color, or normalized RGBA arrays.",
        ),
        entry(
            "dot",
            "dot(a, b) → number",
            "dot(new Point(1,0,0), new Point(0,1,0))",
            "Vector dot product.",
        ),
        entry(
            "cross",
            "cross(a, b) → Point",
            "cross(new Point(1,0,0), new Point(0,1,0))",
            "Vector cross product.",
        ),
        entry(
            "norm",
            "norm(vector) → number",
            "norm(new Point(3,4,0))",
            "Euclidean vector length.",
        ),
        entry(
            "normalize",
            "normalize(vector) → Point",
            "normalize(new Point(1,1,0))",
            "Unit direction; rejects a zero vector.",
        ),
        entry(
            "distance",
            "distance(a, b) → number",
            "distance(new Point(), new Point(3,4,0))",
            "Distance in model units.",
        ),
        entry(
            "lookup",
            "lookup(x, [[x0,y0],...]) → number",
            "lookup(0.5, [[0,0],[1,10]])",
            "Piecewise linear interpolation, clamped to table endpoints.",
        ),
        entry(
            "rands",
            "rands(low, high, count, seed?) → number[]",
            "rands(0, 1, 10, 42)",
            "Generate bounded random lists. A seed makes results repeatable.",
        ),
        entry(
            "sin",
            "sin(value: number) → number",
            "sin(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "cos",
            "cos(value: number) → number",
            "cos(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "tan",
            "tan(value: number) → number",
            "tan(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "asin",
            "asin(value: number) → number",
            "asin(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "acos",
            "acos(value: number) → number",
            "acos(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "atan",
            "atan(value: number) → number",
            "atan(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "atan2",
            "atan2(y, x) → number",
            "atan2(1, 1)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "abs",
            "abs(value: number) → number",
            "abs(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "sign",
            "sign(value: number) → number",
            "sign(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "floor",
            "floor(value: number) → number",
            "floor(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "ceil",
            "ceil(value: number) → number",
            "ceil(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "round",
            "round(value: number) → number",
            "round(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "sqrt",
            "sqrt(value: number) → number",
            "sqrt(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "pow",
            "pow(base, exponent) → number",
            "pow(2, 3)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "exp",
            "exp(value: number) → number",
            "exp(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "ln",
            "ln(value: number) → number",
            "ln(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "log",
            "log(value: number) → number",
            "log(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "min",
            "min(...numbers | numbers[]) → number",
            "min(1, 2, 3)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "max",
            "max(...numbers | numbers[]) → number",
            "max(1, 2, 3)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "len",
            "len(value: string | Array) → number",
            "len([1, 2, 3])",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "concat",
            "concat(...lists) → Array",
            "concat([1, 2], [3, 4])",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "str",
            "str(...values) → string",
            "str('height: ', this.height)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "is_undef",
            "is_undef(value) → boolean",
            "is_undef(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "is_bool",
            "is_bool(value) → boolean",
            "is_bool(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "is_num",
            "is_num(value) → boolean",
            "is_num(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "is_string",
            "is_string(value) → boolean",
            "is_string(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "is_list",
            "is_list(value) → boolean",
            "is_list(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry(
            "is_function",
            "is_function(value) → boolean",
            "is_function(0)",
            "OpenSCAD-style helper. Trigonometry uses degrees; ordinary Math functions remain available.",
        ),
        entry("PI", "PI: number", "PI", "The mathematical constant π."),
        entry(
            "add",
            "add(a, b) → Point",
            "add(new Point(), new Point(1, 2, 3))",
            "Add two vectors.",
        ),
        entry(
            "subtract",
            "subtract(a, b) → Point",
            "subtract(new Point(1, 2, 3), new Point())",
            "Subtract two vectors.",
        ),
        entry(
            "this",
            "this / instance → current object instance",
            "this.",
            "Both names refer to the same object. Assign your own JSON properties in onCreate; they persist per instance. No host mutation methods are exposed.",
        ),
        entry(
            "this.id",
            "this.id: string",
            "this.id",
            "The current occurrence ID, supplied by the host. Useful as a prefix for emitted entity IDs.",
        ),
        entry(
            "this.geometry",
            "this.geometry: Drawable[]",
            "this.geometry",
            "Graphically authored local geometry. Return it, spread it, or combine it with generated output. This does not overwrite the authored prototype.",
        ),
        entry(
            "this.hotspots",
            "this.hotspots: Record<string, Point>",
            "this.hotspots",
            "Per-instance dragged position overrides. $HOTSPOT(position, id) supplies defaults when the map has no value for id.",
        ),
        entry(
            "this.time",
            "this.time: number (seconds)",
            "this.time",
            "Elapsed script runtime time, also available as model.time. Use for animation.",
        ),
        entry(
            "this.deltaTime",
            "this.deltaTime: number (seconds)",
            "this.deltaTime",
            "Elapsed time since the previous frame request, also available as model.deltaTime.",
        ),
        entry(
            "instance",
            "instance: same object as this",
            "instance.",
            "onCreate receives the instance as its first argument. Legacy API v1 onAnimationFrame also receives it; API v2 receives params instead, with the instance still bound as this. Custom JSON properties are preserved by copy and save.",
        ),
        entry(
            "model",
            "model: immutable query facade",
            "model.",
            "Fields: objects, materials, settings, time, deltaTime. Methods: getObject, findObjects, getMaterial, queryGeometry. No document writes, filesystem, networking, Node or DOM.",
        ),
        entry(
            "model.objects",
            "model.objects: readonly ObjectRecord[]",
            "model.objects",
            "Each record has id, name, definitionId, transform, geometry and ifc (instance-owned IFC property sets). Geometry is local; transform places it in the scene.",
        ),
        entry(
            "model.materials",
            "model.materials: readonly Material[]",
            "model.materials",
            "Named project materials: id, name, color (red/green/blue/alpha), shadingType, texturePath, textureScaleU/V.",
        ),
        entry(
            "model.settings",
            "model.settings: readonly ModelSettings",
            "model.settings",
            "Fields use snake_case: unit_name, unit_size, grid_size, circle_segments, snap_radius, arc_max_length, double_line_size and double_line_offset. Coordinates and numeric parameters are internal units. Displayed lengths multiply by unit_size and use unit_name; a grid step displays grid_size * unit_size. snap_radius is in screen pixels.",
        ),
        entry(
            "model.time",
            "model.time: number (seconds)",
            "model.time",
            "Elapsed runtime clock for this frame. It is not a model-unit distance.",
        ),
        entry(
            "model.deltaTime",
            "model.deltaTime: number (seconds)",
            "model.deltaTime",
            "Time since the previous frame request. Suitable for updating this instance's own animation properties.",
        ),
        entry(
            "model.getObject",
            "model.getObject(id) → ObjectRecord | undefined",
            "model.getObject(this.id)",
            "Read an occurrence by its ID. Record fields: id, name, definitionId, transform and geometry.",
        ),
        entry(
            "model.findObjects",
            "model.findObjects(name) → ObjectRecord[]",
            "model.findObjects('Object')",
            "Find occurrences by exact prototype name. Results are frozen model snapshots.",
        ),
        entry(
            "model.getMaterial",
            "model.getMaterial(id) → Material | undefined",
            "model.getMaterial('ansi-grey')",
            "Read a catalog material by stable ID. Use its color or pass the value to a geometry helper.",
        ),
        entry(
            "model.queryGeometry",
            "model.queryGeometry(id) → readonly Drawable[]",
            "model.queryGeometry(this.id)",
            "Read an occurrence's current local drawable array. Apply its transform for world coordinates. Mutating query results is prohibited.",
        ),
        entry(
            "params",
            "params: readonly model parameter values (API v2)",
            "params.",
            "First argument of onAnimationFrame in API v2. Contains all $TEXT/$NUMBER/$CHECKBOX/$LIST values and resolved $MATERIAL choices plus declared $HOTSPOT objects, with their current position. Prototype values are shared; onCreate declarations are per instance. The argument can be named params, parms, or any valid JS identifier. The instance remains this and model is the second argument.",
        ),
        entry(
            "onCreate",
            "function onCreate(instance, model) → void",
            "function onCreate(instance, model) {\n  this.width = 3;\n}",
            "Initialize additional instance properties. It runs once for the current initializer, not on every animation frame.",
        ),
        entry(
            "onAnimationFrame",
            "function onAnimationFrame(params, model) → Drawable[] (API v2)",
            "function onAnimationFrame(params, model) {\n  return this.geometry;\n}",
            "Return arrays or nested arrays of drawables. This hook is required. Geometry uses local coordinates; placement happens afterward.",
        ),
        entry(
            "Math.sin",
            "Math.sin(angleRadians) → number",
            "Math.sin(this.time)",
            "Standard JavaScript Math is available, together with ordinary ECMAScript arrays, objects, JSON and other built-ins.",
        ),
        entry(
            "Math.cos",
            "Math.cos(angleRadians) → number",
            "Math.cos(this.time)",
            "Standard JavaScript cosine; angles are in radians.",
        ),
        entry(
            "Math.PI",
            "Math.PI: number",
            "Math.PI",
            "Standard JavaScript constant π. Other ECMAScript Math functions are available.",
        ),
    ];
    for (suffix, shape, description) in [
        (
            "CIRCLE",
            "CIRCLE",
            "Circular marker; the default hotspot shape.",
        ),
        (
            "RECTANGLE",
            "SQUARE",
            "Rectangular marker, rendered as a screen-sized square.",
        ),
        ("TRIANGLE", "TRIANGLE_UP", "Upward triangular marker."),
        ("DIAMOND", "DIAMOND", "Diamond marker."),
        (
            "TRIANGLE_DOWN",
            "TRIANGLE_DOWN",
            "Downward triangular marker.",
        ),
    ] {
        let name = format!("$HOTSPOT_SHAPE_{suffix}");
        entries.push(entry(
            &name,
            &format!("{name}: '{shape}'"),
            &name,
            description,
        ));
    }
    entries.push(entry("$IFC", "$IFC(set, name, value, options? / dataType='IfcLabel', kind='single')", "$IFC('Pset_WallCommon', 'FireRating', 'EI60');", "Declare an IFC4X3_ADD2 property. Outside hooks sets prototype creation defaults; onCreate sets instance creation values. New instances receive independent copies. Existing instance edits are retained. Kinds: single, enumerated, list, bounded, table, reference, complex, quantity. The fourth argument can be {dataType,kind,unit,secondaryDataType,secondaryUnit,expression,interpolation,usageName,enumeration}. Read instance metadata through this.ifc[set][name].value; model query objects include ifc. Use meshcraft_ifc for the full offline standard catalog and editable properties."));
    entries.push(entry("this.ifc", "this.ifc: readonly IFC property sets", "this.ifc", "Instance-owned IFC properties. Prototype defaults are copied at creation and never live-inherited. Each property has kind, dataType, value, unit and description."));
    prioritize(&mut entries);
    entries
}

#[cfg(test)]
mod declaration_tests {
    #[test]
    fn declarations_lead_the_catalog_and_removed_hotspot_alias_is_absent() {
        let entries = super::catalog();
        assert!(entries[..5].iter().all(|e| super::is_declaration(&e.name)));
        assert!(entries.iter().any(|e| e.name == "$HOTSPOT"));
        assert!(!entries.iter().any(|e| e.name == "hotspot"));
        assert!(!entries.iter().any(|e| e.name.contains("_HAPE_")));
    }
}
