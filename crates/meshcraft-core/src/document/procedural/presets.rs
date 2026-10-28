//! Stable built-in shader presets. Dimensions use local model units.
use super::*;
pub const WATER: &str = "meshcraft.water.v1";
pub const BRICK: &str = "meshcraft.brick.v1";
pub const WOOD: &str = "meshcraft.wood.v1";
pub const FOLIAGE: &str = "meshcraft.foliage.v1";
pub const TREE: &str = "meshcraft.tree.v1";
pub const BRANCHING: &str = "meshcraft.branching.v1";
/// Canonical injected GLSL type, also published with the program schema.
pub const BRANCH_RULE_GLSL: &str = "struct BranchRule { float attach; float angle; float azimuth; float lengthScale; float radiusScale; float bend; float tropism; float activation; vec3 parentStart; vec3 parentControl; vec3 parentEnd; vec3 parentFrame; float parentRadius; };";
pub fn is_builtin(id: &str) -> bool {
    matches!(
        id,
        GRASS | WATER | BRICK | WOOD | TREE | FOLIAGE | BRANCHING
    )
}
pub fn builtin_programs() -> Vec<Program> {
    vec![
        grass_program(),
        foliage_program(),
        water_program(),
        brick_program(),
        wood_program(),
        tree_program(),
        branching_program(),
    ]
}
fn scalar(id: &str, label: &str, value: f64, min: f64, max: f64, description: &str) -> Parameter {
    Parameter {
        id: id.into(),
        label: label.into(),
        kind: "float".into(),
        default: value.into(),
        description: description.into(),
        min: Some(min),
        max: Some(max),
        unit: String::new(),
        step: None,
        options: BTreeMap::new(),
        extensions: BTreeMap::new(),
    }
}
fn integer(id: &str, label: &str, value: u32, min: f64, max: f64) -> Parameter {
    Parameter {
        kind: "integer".into(),
        default: value.into(),
        ..scalar(
            id,
            label,
            value as f64,
            min,
            max,
            "Rendering detail; bounded by the instance budget.",
        )
    }
}
fn color(id: &str, label: &str, rgba: [f64; 4]) -> Parameter {
    Parameter {
        kind: "color".into(),
        default: serde_json::json!(rgba),
        ..scalar(
            id,
            label,
            0.,
            0.,
            1.,
            "RGBA channels in the range zero to one.",
        )
    }
}
fn seed() -> Parameter {
    Parameter {
        kind: "seed".into(),
        default: 1u32.into(),
        min: None,
        max: None,
        ..scalar(
            "seed",
            "Seed",
            1.,
            0.,
            u32::MAX as f64,
            "Deterministic variation. Changing this reshuffles the result.",
        )
    }
}
pub fn foliage_program() -> Program {
    Program {
        id: FOLIAGE.into(),
        name: "Foliage".into(),
        generation: "foliage".into(),
        animated: true,
        parameters: vec![
            scalar(
                "density",
                "Density",
                18.,
                0.,
                10000.,
                "Leaf candidates per square world model unit; LOD and budget cap the work.",
            ),
            scalar(
                "size",
                "Leaf length",
                0.55,
                0.001,
                100.,
                "Length in local model units, with seeded size variation.",
            ),
            scalar(
                "width",
                "Leaf width",
                0.32,
                0.001,
                100.,
                "Maximum width in local model units.",
            ),
            scalar(
                "height",
                "Height above surface",
                0.12,
                0.,
                1000.,
                "Root height along the source normal; zero places leaves directly on the surface.",
            ),
            scalar(
                "spread",
                "Edge spread",
                0.45,
                0.,
                1000.,
                "Random tangent-plane offset in local units, allowing leaves beyond the source boundary.",
            ),
            scalar(
                "tilt",
                "Tilt (degrees)",
                30.,
                0.,
                90.,
                "Leaf angle above the source plane, with seeded variation.",
            ),
            scalar(
                "curl",
                "Curl",
                0.2,
                -1.,
                1.,
                "Tip curl relative to leaf length.",
            ),
            scalar(
                "wind",
                "Wind strength",
                0.1,
                0.,
                1.,
                "Animated tip motion relative to leaf length; zero pauses animation.",
            ),
            scalar(
                "variation",
                "Color variation",
                0.25,
                0.,
                1.,
                "Seeded variation in leaf brightness.",
            ),
            color("leafColor", "Leaf color", [0.12, 0.36, 0.035, 1.]),
            color("alternateColor", "Alternate color", [0.36, 0.60, 0.075, 1.]),
            seed(),
        ],
        fragment: include_str!("foliage.fs").into(),
        ..Default::default()
    }
}
pub fn water_program() -> Program {
    Program {
        id: WATER.into(),
        name: "Water".into(),
        generation: "surface_grid".into(),
        animated: true,
        transparent: true,
        parameters: vec![
            scalar(
                "amplitude",
                "Wave height",
                0.12,
                0.,
                100.,
                "Maximum primary wave displacement along the source normal.",
            ),
            scalar(
                "offset",
                "Surface offset",
                0.18,
                -1000.,
                1000.,
                "Mean water height above the source surface, along its normal.",
            ),
            scalar(
                "wavelength",
                "Wavelength",
                3.,
                0.01,
                10000.,
                "Primary wavelength in local model units.",
            ),
            scalar(
                "speed",
                "Wave speed",
                0.7,
                0.,
                20.,
                "Animation rate; zero freezes the water.",
            ),
            scalar(
                "direction",
                "Direction (degrees)",
                25.,
                -360.,
                360.,
                "Wave travel direction in the local surface plane.",
            ),
            integer("resolution", "Surface subdivisions", 16, 1., 64.),
            scalar(
                "opacity",
                "Opacity",
                0.72,
                0.,
                1.,
                "Water transparency. No screen-space refraction or scene reflection.",
            ),
            color("deepColor", "Deep water", [0.015, 0.16, 0.23, 1.]),
            color("shallowColor", "Shallow water", [0.04, 0.48, 0.50, 1.]),
            scalar(
                "foam",
                "Crest foam",
                0.12,
                0.,
                1.,
                "White foam near wave crests.",
            ),
        ],
        vertex: include_str!("water.vs").into(),
        fragment: include_str!("water.fs").into(),
        ..Default::default()
    }
}
pub fn brick_program() -> Program {
    Program {
        id: BRICK.into(),
        name: "Brick".into(),
        parameters: vec![
            scalar(
                "brickWidth",
                "Brick width",
                0.60,
                0.001,
                1000.,
                "Local model units.",
            ),
            scalar(
                "brickHeight",
                "Course height",
                0.25,
                0.001,
                1000.,
                "Local model units, including mortar.",
            ),
            scalar(
                "mortar",
                "Mortar fraction",
                0.08,
                0.001,
                0.45,
                "Fraction of the shorter brick dimension.",
            ),
            scalar(
                "stagger",
                "Course stagger",
                0.5,
                0.,
                1.,
                "Running-bond offset on alternating rows.",
            ),
            scalar(
                "variation",
                "Brick variation",
                0.22,
                0.,
                1.,
                "Seeded variation between bricks.",
            ),
            scalar(
                "relief",
                "Surface relief",
                0.12,
                0.,
                1.,
                "Shading relief only; does not alter topology or silhouettes.",
            ),
            color("brickColor", "Brick color", [0.58, 0.19, 0.09, 1.]),
            color("mortarColor", "Mortar color", [0.65, 0.62, 0.54, 1.]),
            seed(),
        ],
        fragment: format!(
            "{}\n{}",
            include_str!("patterns.glsl"),
            include_str!("brick.fs")
        ),
        ..Default::default()
    }
}
pub fn wood_program() -> Program {
    Program {
        id: WOOD.into(),
        name: "Wood".into(),
        parameters: vec![
            scalar(
                "ringScale",
                "Growth rings",
                5.,
                0.01,
                1000.,
                "Ring frequency per local model unit.",
            ),
            scalar(
                "grain",
                "Fine grain",
                45.,
                0.1,
                2000.,
                "Fine fiber frequency.",
            ),
            scalar(
                "turbulence",
                "Ring distortion",
                0.18,
                0.,
                2.,
                "Noise distortion of growth rings.",
            ),
            scalar(
                "relief",
                "Grain relief",
                0.08,
                0.,
                1.,
                "Shading relief only.",
            ),
            Parameter {
                kind: "enum".into(),
                default: "z".into(),
                options: BTreeMap::from([
                    ("x".into(), "X".into()),
                    ("y".into(), "Y".into()),
                    ("z".into(), "Z".into()),
                ]),
                ..scalar(
                    "axis",
                    "Grain axis",
                    0.,
                    0.,
                    2.,
                    "Local longitudinal direction of the wood.",
                )
            },
            color("lightColor", "Light wood", [0.67, 0.39, 0.16, 1.]),
            color("darkColor", "Dark wood", [0.22, 0.075, 0.025, 1.]),
            seed(),
        ],
        fragment: format!(
            "{}\n{}",
            include_str!("patterns.glsl"),
            include_str!("wood.fs")
        ),
        ..Default::default()
    }
}
pub fn tree_program() -> Program {
    Program {
        id: TREE.into(),
        name: "Tree".into(),
        generation: "tree".into(),
        animated: true,
        parameters: vec![
            scalar(
                "height",
                "Height",
                6.,
                0.1,
                1000.,
                "Tree height in local model units.",
            ),
            scalar(
                "radius",
                "Crown radius",
                2.3,
                0.01,
                1000.,
                "Maximum crown radius.",
            ),
            scalar(
                "trunkRadius",
                "Trunk radius",
                0.18,
                0.001,
                100.,
                "Trunk base radius.",
            ),
            integer("branches", "Primary branches", 8, 2., 12.),
            integer("levels", "Branch levels", 3, 1., 4.),
            integer("leaves", "Leaf budget", 3500, 0., 100000.),
            scalar(
                "leafSize",
                "Leaf size",
                0.18,
                0.001,
                10.,
                "Leaf card length in local units.",
            ),
            scalar(
                "wind",
                "Wind strength",
                0.08,
                0.,
                1.,
                "Maximum sway as a fraction of height; zero pauses animation.",
            ),
            color("barkColor", "Bark", [0.24, 0.12, 0.055, 1.]),
            color("leafColor", "Leaves", [0.19, 0.43, 0.055, 1.]),
            seed(),
        ],
        fragment: include_str!("tree.fs").into(),
        ..Default::default()
    }
}
/// Bounded conditional branching; all branch frames, curves and surfaces are GLSL.
pub fn branching_program() -> Program {
    let mut program = tree_program();
    program.id = BRANCHING.into();
    program.name = "Branching fractal".into();
    program.generation = "branching".into();
    program.extensions.insert(
        "branchingLimits".into(),
        serde_json::json!({
            "maxDepth":6,"maxPrimaryBranches":12,"maxChildren":8,
            "maxBranchCandidates":16384,"curveSegments":3,"radialSectors":6,
            "hook":"branchingRule(inout BranchRule rule,int level,int child,uvec2 path,uint key)",
            "ruleType":BRANCH_RULE_GLSL,
            "pathEncoding":"root=1; append four-bit child+1 slots; x=low32, y=high32",
            "ruleFields":{
                "attach":"Parent curve parameter, clamped 0..1",
                "angle":"Deflection from parent tangent in radians, clamped 0..pi",
                "azimuth":"Rotation around parent tangent in radians",
                "lengthScale":"Child/parent chord length ratio, clamped 0..2.5",
                "radiusScale":"Child/parent attachment radius ratio, clamped 0..0.95",
                "bend":"Quadratic curvature, clamped -1..1",
                "tropism":"Local vertical attraction, clamped -1..1",
                "activation":"Continuous child survival, clamped 0..1; inherited by descendants",
                "parentStart":"vec3; read-only local parent curve start; writes ignored",
                "parentControl":"vec3; read-only local quadratic control point; writes ignored",
                "parentEnd":"vec3; read-only local parent curve end; writes ignored",
                "parentFrame":"vec3; read-only transported frame reference; writes ignored",
                "parentRadius":"float; read-only parent base radius; writes ignored"
            }
        }),
    );
    program
        .parameters
        .retain(|p| !matches!(p.id.as_str(), "branches" | "levels"));
    program.parameters.splice(0..0, [
        Parameter {
            id: "form".into(), label: "Form".into(), kind: "enum".into(),
            default: "broadleaf".into(),
            description: "Rule family for the GPU generator.".into(),
            options: [
                ("broadleaf", "Broadleaf tree"), ("conifer", "Conifer"),
                ("coral", "Coral"), ("roots", "Roots"),
                ("shrub", "Shrub"), ("weeping", "Weeping tree"),
            ].into_iter().map(|(k,v)| (k.into(),v.into())).collect(),
            min: None, max: None, step: None, unit: String::new(), extensions: BTreeMap::new(),
        },
        integer("branches", "Primary branches", 7, 1., 12.),
        integer("forks", "Children per branch", 3, 1., 8.),
        integer("levels", "Branching depth", 3, 0., 6.),
        scalar("angle", "Branch angle", 42., 0., 160., "Angle from the parent's tangent, in degrees."),
        scalar("twist", "Phyllotactic angle", 137.5, -180., 180., "Rotation between child slots, in degrees. Slot identity stays stable when counts change."),
        scalar("attachment", "Attachment", 0.72, 0.05, 1., "Attachment position along the parent curve; primary branches are distributed along the trunk."),
        scalar("lengthRatio", "Length ratio", 0.62, 0.05, 0.9, "Child length relative to its parent, before family and random variation."),
        scalar("radiusRatio", "Radius ratio", 0.58, 0.05, 0.9, "Child radius relative to the parent radius at its attachment."),
        scalar("bend", "Curvature", 0.2, -1., 1., "Quadratic branch curvature; child frames follow the curved parent tangent."),
        scalar("gravity", "Tropism", 0.15, -1., 1., "Positive bends toward local up; negative bends down. Roots invert their local up."),
        scalar("variation", "Variation", 0.3, 0., 1., "Deterministic variation of lengths, angles and attachments."),
        scalar("branchDensity", "Branch survival", 1., 0., 1., "Continuous seeded pruning; zero retains only the trunk. Descendants inherit parent survival."),
        scalar("growth", "Growth", 1., 0., 1., "Continuous growth from an empty object to the configured branching depth."),
        scalar("minPixels", "Branch LOD", 1., 0., 8., "Fade subpixel branch hierarchies in the viewport. Zero disables branch LOD; bake captures full detail."),
    ]);
    program.vertex = include_str!("branching.vs").into();
    let order = [
        "form",
        "height",
        "radius",
        "trunkRadius",
        "branches",
        "forks",
        "levels",
        "leaves",
        "leafSize",
        "wind",
        "growth",
        "branchDensity",
        "angle",
        "twist",
        "attachment",
        "lengthRatio",
        "radiusRatio",
        "bend",
        "gravity",
        "variation",
        "minPixels",
        "barkColor",
        "leafColor",
        "seed",
    ];
    program.parameters.sort_by_key(|p| {
        order
            .iter()
            .position(|id| *id == p.id)
            .unwrap_or(usize::MAX)
    });
    program
}
/// Set sensible shading defaults while preserving the caller's unique material ID.
pub fn apply_preset(material: &mut super::super::materials::Material, id: &str) {
    use super::super::materials::Pbr;
    material.name = match id {
        WATER => "Water",
        BRICK => "Brick",
        WOOD => "Wood",
        TREE => "Tree",
        BRANCHING => "Tree",
        FOLIAGE => "Foliage",
        _ => "Grass",
    }
    .into();
    material.color = match id {
        WATER => Rgba8::new(12, 95, 130, 185),
        BRICK => Rgba8::new(148, 48, 23, 255),
        WOOD => Rgba8::new(171, 99, 41, 255),
        FOLIAGE => Rgba8::new(44, 115, 20, 255),
        _ => Rgba8::new(65, 130, 35, 255),
    };
    material.pbr = Some(Pbr {
        roughness: if id == WATER { 0.16 } else { 0.72 },
        metallic: 0.,
    });
    material.procedural = Some(ProceduralMaterial {
        program: id.into(),
        ..Default::default()
    });
}
