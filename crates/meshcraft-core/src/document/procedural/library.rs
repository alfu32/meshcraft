//! Standard surface shaders: emboss, metal, glass and Perlin noise (T83).
//!
//! All of them shade existing faces; none moves vertices, so silhouettes, picking,
//! snapping and export are unchanged. Relief is shading only.
use super::*;

pub const EMBOSS_ROUGH: &str = "emboss.rough_texture";
pub const EMBOSS_TILING: &str = "emboss.tiling";
pub const EMBOSS_ROUGH_TILING: &str = "emboss.rough_tiling";
pub const METAL: &str = "metal.polished";
pub const GLASS: &str = "glass.clear";
pub const PERLIN: &str = "noise.perlin";
/// IDs of the standard surface shaders, in the order they are listed.
pub const LIBRARY: [&str; 6] = [
    EMBOSS_ROUGH,
    EMBOSS_TILING,
    EMBOSS_ROUGH_TILING,
    METAL,
    GLASS,
    PERLIN,
];

pub fn library_programs() -> Vec<Program> {
    vec![
        emboss_rough_program(),
        emboss_tiling_program(),
        emboss_rough_tiling_program(),
        metal_program(),
        glass_program(),
        perlin_program(),
    ]
}

fn rough_parameters() -> Vec<Parameter> {
    vec![
        scalar(
            "grainSize",
            "Grain size",
            0.16,
            0.001,
            1000.,
            "Size of the largest lumps, in local model units.",
        ),
        scalar(
            "roughDepth",
            "Roughness depth",
            0.06,
            0.,
            10.,
            "Relief of the grain. Shading only; faces stay flat.",
        ),
        scalar(
            "hollowShade",
            "Darken hollows",
            0.3,
            0.,
            1.,
            "How much the low parts of the grain are darkened.",
        ),
    ]
}

fn tile_parameters() -> Vec<Parameter> {
    vec![
        scalar(
            "tileWidth",
            "Tile width",
            0.3,
            0.001,
            1000.,
            "Local model units, including the groove.",
        ),
        scalar(
            "tileHeight",
            "Tile height",
            0.3,
            0.001,
            1000.,
            "Local model units, including the groove.",
        ),
        scalar(
            "groove",
            "Groove width",
            0.012,
            0.,
            100.,
            "Width of the flat joint between tiles, in local model units.",
        ),
        scalar(
            "bevel",
            "Bevel width",
            0.012,
            0.,
            100.,
            "Width of the slope from the groove up to the tile face.",
        ),
        scalar(
            "tileDepth",
            "Tile depth",
            0.012,
            0.,
            10.,
            "Height of the tiles above the grooves. Shading only.",
        ),
        scalar(
            "stagger",
            "Row offset",
            0.,
            0.,
            1.,
            "Shifts every other row; 0.5 gives a running bond.",
        ),
        scalar(
            "variation",
            "Tile variation",
            0.12,
            0.,
            1.,
            "Random brightness difference between tiles.",
        ),
        color("grooveColor", "Groove color", [0.42, 0.4, 0.37, 1.]),
        scalar(
            "rotationU",
            "Rotation U (degrees)",
            0.,
            -360.,
            360.,
            "Rotate the pattern around its local U (X) axis, then around V (Y).",
        ),
        scalar(
            "rotationV",
            "Rotation V (degrees)",
            0.,
            -360.,
            360.,
            "Rotate the pattern around its local V (Y) axis after the U rotation.",
        ),
    ]
}

fn environment_parameters(reflectivity: f64, polish: f64) -> Vec<Parameter> {
    vec![
        scalar(
            "reflectivity",
            "Reflectivity",
            reflectivity,
            0.,
            1.,
            "How much of the environment the surface mirrors.",
        ),
        scalar(
            "polish",
            "Polish",
            polish,
            0.,
            1.,
            "0 gives soft satin reflections, 1 a sharp mirror.",
        ),
        color("skyColor", "Sky color", [0.62, 0.74, 0.88, 1.]),
        color("groundColor", "Ground color", [0.46, 0.43, 0.4, 1.]),
    ]
}

fn surface(id: &str, name: &str, parameters: Vec<Parameter>, fragment: String) -> Program {
    Program {
        id: id.into(),
        name: name.into(),
        parameters,
        fragment,
        ..Default::default()
    }
}

pub fn emboss_rough_program() -> Program {
    let mut parameters = rough_parameters();
    parameters.push(seed());
    surface(
        EMBOSS_ROUGH,
        "Emboss · rough texture",
        parameters,
        [
            include_str!("patterns.glsl"),
            include_str!("emboss_rough.glsl"),
            include_str!("emboss_rough.fs"),
        ]
        .join("\n"),
    )
}

pub fn emboss_tiling_program() -> Program {
    let mut parameters = tile_parameters();
    parameters.push(seed());
    surface(
        EMBOSS_TILING,
        "Emboss · tiling",
        parameters,
        [
            include_str!("patterns.glsl"),
            include_str!("orientation.glsl"),
            include_str!("emboss_tiles.glsl"),
            include_str!("emboss_tiling.fs"),
        ]
        .join("\n"),
    )
}

pub fn emboss_rough_tiling_program() -> Program {
    let mut parameters = tile_parameters();
    parameters.extend(rough_parameters());
    parameters.push(seed());
    surface(
        EMBOSS_ROUGH_TILING,
        "Emboss · rough tiling",
        parameters,
        [
            include_str!("patterns.glsl"),
            include_str!("orientation.glsl"),
            include_str!("emboss_rough.glsl"),
            include_str!("emboss_tiles.glsl"),
            include_str!("emboss_rough_tiling.fs"),
        ]
        .join("\n"),
    )
}

pub fn metal_program() -> Program {
    surface(
        METAL,
        "Polished metal",
        environment_parameters(0.9, 0.85),
        [include_str!("environment.glsl"), include_str!("metal.fs")].join("\n"),
    )
}

pub fn glass_program() -> Program {
    let mut parameters = environment_parameters(1., 0.95);
    parameters.insert(
        0,
        scalar(
            "opacity",
            "Opacity",
            0.18,
            0.,
            1.,
            "Opacity seen head-on; edges become more reflective and opaque.",
        ),
    );
    Program {
        transparent: true,
        ..surface(
            GLASS,
            "Glass",
            parameters,
            [include_str!("environment.glsl"), include_str!("glass.fs")].join("\n"),
        )
    }
}

pub fn perlin_program() -> Program {
    surface(
        PERLIN,
        "Perlin noise",
        vec![
            scalar(
                "featureSize",
                "Feature size",
                0.5,
                0.001,
                1000.,
                "Size of the largest blobs, in local model units.",
            ),
            Parameter {
                description: "Layers of finer detail. Each adds one noise evaluation.".into(),
                ..integer("octaves", "Octaves", 4, 1., 8.)
            },
            scalar(
                "contrast",
                "Contrast",
                1.4,
                0.,
                4.,
                "Spreads the noise towards the two colors.",
            ),
            color("lowColor", "Low color", [0.24, 0.2, 0.16, 1.]),
            color("highColor", "High color", [0.92, 0.86, 0.74, 1.]),
            scalar(
                "relief",
                "Relief",
                0.,
                0.,
                10.,
                "Optional bump following the noise. Shading only; 0 turns it off.",
            ),
            seed(),
        ],
        [
            include_str!("patterns.glsl"),
            include_str!("perlin.glsl"),
            include_str!("perlin.fs"),
        ]
        .join("\n"),
    )
}
