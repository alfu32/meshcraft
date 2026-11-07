//! The material library that ships with Meshcraft (T84).
//!
//! Library materials are ready-made [`Material`]s grouped in sections. They are
//! not part of a document until they are added to it; once added they are
//! ordinary model materials that can be edited freely. Every library material
//! has a stable ID starting with `library.`, so adding one twice selects the
//! copy already in the model instead of overwriting the user's edits.
use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::Rgba8;
use super::materials::{Material, Pbr};
use super::procedural::{self, ProceduralMaterial};

pub const LIBRARY_PREFIX: &str = "library.";

#[derive(Clone, Debug, PartialEq)]
pub struct LibrarySection {
    pub id: &'static str,
    pub title: &'static str,
    pub materials: Vec<Material>,
}

/// Hues of the plain colour section, in degrees. Magenta and fuchsia are often
/// the same colour; here fuchsia is the pinker hue between magenta and red.
const HUES: [(&str, &str, f64); 8] = [
    ("red", "Red", 0.),
    ("orange", "Orange", 30.),
    ("yellow", "Yellow", 60.),
    ("green", "Green", 120.),
    ("cyan", "Cyan", 180.),
    ("blue", "Blue", 240.),
    ("magenta", "Magenta", 300.),
    ("fuchsia", "Fuchsia", 330.),
];
const LIGHTNESS: [u8; 3] = [25, 50, 75];

/// Every library section, in the order the panel lists them.
pub fn material_library() -> Vec<LibrarySection> {
    vec![
        LibrarySection {
            id: "colors",
            title: "Plain colors",
            materials: plain_colors(),
        },
        LibrarySection {
            id: "vegetation",
            title: "Vegetation",
            materials: vegetation(),
        },
        LibrarySection {
            id: "construction",
            title: "Construction",
            materials: construction(),
        },
    ]
}

/// A library material by ID.
pub fn library_material(id: &str) -> Option<Material> {
    material_library()
        .into_iter()
        .flat_map(|s| s.materials)
        .find(|m| m.id == id)
}

/// HSL to 8-bit RGB; `hue` in degrees, saturation and lightness in 0..=1.
pub fn hsl(hue: f64, saturation: f64, lightness: f64) -> Rgba8 {
    let c = (1. - (2. * lightness - 1.).abs()) * saturation;
    let h = hue.rem_euclid(360.) / 60.;
    let x = c * (1. - (h % 2. - 1.).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    let m = lightness - c / 2.;
    let byte = |v: f64| ((v + m) * 255.).round().clamp(0., 255.) as u8;
    Rgba8::new(byte(r), byte(g), byte(b), 255)
}

fn plain(id: String, name: String, color: Rgba8) -> Material {
    Material {
        id,
        name,
        color,
        ..Default::default()
    }
}

fn plain_colors() -> Vec<Material> {
    let mut materials = Vec::new();
    for (id, name, hue) in HUES {
        for l in LIGHTNESS {
            materials.push(plain(
                format!("{LIBRARY_PREFIX}color.{id}-{l}"),
                format!("{name} {l}%"),
                hsl(hue, 1., f64::from(l) / 100.),
            ));
        }
    }
    for step in 0..=10u8 {
        let l = step * 10;
        let v = (f64::from(l) / 100. * 255.).round() as u8;
        materials.push(plain(
            format!("{LIBRARY_PREFIX}color.grey-{l}"),
            format!("Grey {l}%"),
            Rgba8::new(v, v, v, 255),
        ));
    }
    materials
}

fn shaded(
    id: &str,
    name: &str,
    program: &str,
    color: Rgba8,
    parameters: Value,
    (roughness, metallic): (f64, f64),
) -> Material {
    let parameters: BTreeMap<String, Value> =
        serde_json::from_value(parameters).unwrap_or_default();
    Material {
        id: format!("{LIBRARY_PREFIX}{id}"),
        name: name.into(),
        color,
        pbr: Some(Pbr {
            roughness,
            metallic,
        }),
        procedural: Some(ProceduralMaterial {
            program: program.into(),
            parameters,
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn vegetation() -> Vec<Material> {
    let grass = Rgba8::new(65, 130, 35, 255);
    let leaves = Rgba8::new(44, 115, 20, 255);
    let matte = (0.72, 0.);
    vec![
        shaded(
            "grass.normal",
            "Grass",
            procedural::GRASS,
            grass,
            json!({}),
            matte,
        ),
        shaded(
            "grass.thick",
            "Thick grass",
            procedural::GRASS,
            grass,
            json!({"density": 75., "width": 0.08}),
            matte,
        ),
        shaded(
            "grass.tall",
            "Tall grass",
            procedural::GRASS,
            grass,
            json!({"height": 1.6}),
            matte,
        ),
        shaded(
            "grass.thick_tall",
            "Thick tall grass",
            procedural::GRASS,
            grass,
            json!({"density": 75., "width": 0.08, "height": 1.6}),
            matte,
        ),
        shaded(
            "foliage.normal",
            "Foliage",
            procedural::FOLIAGE,
            leaves,
            json!({}),
            matte,
        ),
        shaded(
            "foliage.thick",
            "Thick foliage",
            procedural::FOLIAGE,
            leaves,
            json!({"density": 45.}),
            matte,
        ),
        shaded(
            "foliage.tall",
            "Tall foliage",
            procedural::FOLIAGE,
            leaves,
            json!({"height": 0.4, "size": 0.8}),
            matte,
        ),
        shaded(
            "foliage.thick_tall",
            "Thick tall foliage",
            procedural::FOLIAGE,
            leaves,
            json!({"density": 45., "height": 0.4, "size": 0.8}),
            matte,
        ),
    ]
}

fn construction() -> Vec<Material> {
    let rgba = |c: Rgba8| {
        json!([
            f64::from(c.red) / 255.,
            f64::from(c.green) / 255.,
            f64::from(c.blue) / 255.,
            1.
        ])
    };
    let mut materials = vec![
        shaded(
            "glass.blue",
            "Blue glass",
            procedural::GLASS,
            Rgba8::new(170, 200, 235, 255),
            json!({}),
            (0.05, 0.),
        ),
        shaded(
            "glass.green",
            "Green glass",
            procedural::GLASS,
            Rgba8::new(175, 225, 195, 255),
            json!({}),
            (0.05, 0.),
        ),
    ];
    for (id, name, brick) in [
        ("red", "Red brick", Rgba8::new(148, 48, 23, 255)),
        ("yellow", "Yellow brick", Rgba8::new(209, 168, 92, 255)),
        ("white", "White brick", Rgba8::new(226, 222, 210, 255)),
    ] {
        materials.push(shaded(
            &format!("brick.{id}"),
            name,
            procedural::BRICK,
            brick,
            json!({"brickColor": rgba(brick)}),
            (0.72, 0.),
        ));
    }
    let ceramic = Rgba8::new(214, 208, 196, 255);
    for (id, name, size) in [
        ("0_5", "Tiles ½ unit", 0.5),
        ("1", "Tiles 1 unit", 1.),
        ("1_5", "Tiles 1½ units", 1.5),
        ("2", "Tiles 2 units", 2.),
    ] {
        materials.push(shaded(
            &format!("tiles.{id}"),
            name,
            procedural::EMBOSS_TILING,
            ceramic,
            tile_parameters(size),
            (0.45, 0.),
        ));
    }
    let stone = Rgba8::new(158, 152, 142, 255);
    let fine = json!({"grainSize": 0.06, "roughDepth": 0.03, "hollowShade": 0.25});
    let rough = json!({"grainSize": 0.3, "roughDepth": 0.12, "hollowShade": 0.4});
    materials.push(shaded(
        "stone.fine",
        "Fine stone",
        procedural::EMBOSS_ROUGH,
        stone,
        fine.clone(),
        (0.85, 0.),
    ));
    materials.push(shaded(
        "stone.rough",
        "Rough stone",
        procedural::EMBOSS_ROUGH,
        stone,
        rough.clone(),
        (0.85, 0.),
    ));
    for (id, name, grain) in [
        ("fine", "Fine stone tiles", fine),
        ("rough", "Rough stone tiles", rough),
    ] {
        let mut parameters = tile_parameters(1.);
        parameters
            .as_object_mut()
            .unwrap()
            .extend(grain.as_object().unwrap().clone());
        materials.push(shaded(
            &format!("stone_tiles.{id}"),
            name,
            procedural::EMBOSS_ROUGH_TILING,
            stone,
            parameters,
            (0.85, 0.),
        ));
    }
    materials.push(shaded(
        "metal.zinc",
        "Zinc",
        procedural::METAL,
        Rgba8::new(186, 192, 198, 255),
        json!({"reflectivity": 0.6, "polish": 0.15}),
        (0.45, 1.),
    ));
    materials.push(shaded(
        "metal.chrome",
        "Chrome",
        procedural::METAL,
        Rgba8::new(235, 238, 242, 255),
        json!({"reflectivity": 0.97, "polish": 0.98}),
        (0.04, 1.),
    ));
    materials
}

/// Square tiles of `size` model units; joints scale with the tile.
fn tile_parameters(size: f64) -> Value {
    json!({
        "tileWidth": size,
        "tileHeight": size,
        "groove": 0.03 * size,
        "bevel": 0.03 * size,
        "tileDepth": 0.03 * size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;

    #[test]
    fn library_has_the_requested_sections_and_every_material_is_valid() {
        let library = material_library();
        let count = |id: &str| library.iter().find(|s| s.id == id).unwrap().materials.len();
        // 8 hues × 3 lightness steps + 11 greys.
        assert_eq!(count("colors"), 35);
        assert_eq!(count("vegetation"), 8);
        // 2 glass, 3 brick, 4 tiles, 2 stone, 2 stone tiles, 2 metal.
        assert_eq!(count("construction"), 15);
        let mut document = Document::new();
        let mut ids = std::collections::BTreeSet::new();
        for material in library.iter().flat_map(|s| &s.materials) {
            assert!(material.id.starts_with(LIBRARY_PREFIX));
            assert!(ids.insert(material.id.clone()), "{} repeats", material.id);
            document
                .put_material(material.clone())
                .unwrap_or_else(|e| panic!("{}: {e}", material.id));
            assert_eq!(library_material(&material.id).as_ref(), Some(material));
        }
    }

    #[test]
    fn plain_colors_sample_hsl_at_full_saturation() {
        assert_eq!(hsl(0., 1., 0.5), Rgba8::new(255, 0, 0, 255));
        assert_eq!(hsl(120., 1., 0.25), Rgba8::new(0, 128, 0, 255));
        assert_eq!(hsl(240., 1., 0.75), Rgba8::new(128, 128, 255, 255));
        assert_eq!(hsl(300., 1., 0.5), Rgba8::new(255, 0, 255, 255));
        assert_eq!(
            library_material("library.color.grey-50").unwrap().color,
            Rgba8::new(128, 128, 128, 255)
        );
        assert_eq!(
            library_material("library.color.grey-0").unwrap().color,
            Rgba8::new(0, 0, 0, 255)
        );
    }
}
