use std::collections::BTreeMap;

use eframe::egui;

const ICON_ATLAS_PNG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icons.32.png"));
const ICON_MAPPING_CSV: &str = include_str!("../../../assets/tool-icons/icons.mapping.csv");
include!(concat!(env!("OUT_DIR"), "/icon_dimensions.rs"));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IconRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub fn icon_regions() -> BTreeMap<&'static str, IconRegion> {
    parse_icon_regions(ICON_MAPPING_CSV)
}

pub fn load_icon_atlas(context: &egui::Context) -> Option<egui::TextureHandle> {
    let rgba = image::load_from_memory(ICON_ATLAS_PNG).ok()?.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width != ICON_ATLAS_WIDTH || height != ICON_ATLAS_HEIGHT {
        return None;
    }

    let pixels =
        egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], rgba.as_raw());
    Some(context.load_texture(
        "legacy-tool-icon-atlas",
        pixels,
        egui::TextureOptions::NEAREST,
    ))
}

/// Decode and crop the generated high-DPI atlas once, never on a UI redraw.
#[cfg(not(target_arch = "wasm32"))]
pub fn gpui_icon(name: &str) -> Option<std::sync::Arc<gpui::RenderImage>> {
    use std::sync::{Arc, OnceLock};
    static ICONS: OnceLock<BTreeMap<&'static str, Arc<gpui::RenderImage>>> = OnceLock::new();
    ICONS
        .get_or_init(|| {
            let atlas =
                image::load_from_memory(include_bytes!(concat!(env!("OUT_DIR"), "/icons.64.png")))
                    .expect("generated icon atlas")
                    .to_rgba8();
            icon_regions()
                .into_iter()
                .map(|(name, r)| {
                    let mut tile = image::imageops::crop_imm(
                        &atlas,
                        r.x * 2,
                        r.y * 2,
                        r.width * 2,
                        r.height * 2,
                    )
                    .to_image();
                    // GPUI RenderImage expects premultiplied BGRA. PNG decodes to straight RGBA.
                    for pixel in tile.pixels_mut() {
                        let [r, g, b, a] = pixel.0;
                        pixel.0 = [
                            ((u16::from(b) * u16::from(a)) / 255) as u8,
                            ((u16::from(g) * u16::from(a)) / 255) as u8,
                            ((u16::from(r) * u16::from(a)) / 255) as u8,
                            a,
                        ];
                    }
                    (
                        name,
                        Arc::new(gpui::RenderImage::new(vec![image::Frame::new(tile)])),
                    )
                })
                .collect()
        })
        .get(name)
        .cloned()
}

fn parse_icon_regions(mapping: &'static str) -> BTreeMap<&'static str, IconRegion> {
    mapping
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields = line.split('|').collect::<Vec<_>>();
            if fields.len() < 8 {
                return None;
            }

            let name = fields[1].trim();
            let x0 = fields[4].trim().parse::<u32>().ok()?;
            let x1 = fields[5].trim().parse::<u32>().ok()?;
            let y0 = fields[6].trim().parse::<u32>().ok()?;
            let y1 = fields[7].trim().parse::<u32>().ok()?;
            if name.is_empty()
                || x0 >= ICON_ATLAS_WIDTH
                || y0 >= ICON_ATLAS_HEIGHT
                || x1 < x0
                || y1 < y0
                || x1 >= ICON_ATLAS_WIDTH
                || y1 >= ICON_ATLAS_HEIGHT
            {
                return None;
            }

            Some((
                name,
                IconRegion {
                    x: x0,
                    y: y0,
                    width: x1 - x0 + 1,
                    height: y1 - y0 + 1,
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ICON_ATLAS_HEIGHT, ICON_ATLAS_WIDTH, icon_regions};

    #[test]
    fn every_catalog_command_has_a_named_vector_icon() {
        let icons = icon_regions();
        let missing = crate::MeshcraftApp::command_catalog()
            .into_iter()
            .filter(|command| {
                !icons.contains_key(command.id.as_str())
                    && !command
                        .presentation
                        .icon_key
                        .as_deref()
                        .is_some_and(|name| icons.contains_key(name))
            })
            .map(|command| command.id.as_str().to_owned())
            .collect::<Vec<_>>();
        assert!(missing.is_empty(), "missing command icons: {missing:?}");
    }

    #[test]
    fn current_tool_icons_are_mapped_inside_the_atlas() {
        let regions = icon_regions();
        for name in [
            "select",
            "construction_line",
            "rectangle",
            "cylinder",
            "revolve",
            "camera_rotating_operation",
        ] {
            let region = regions
                .get(name)
                .unwrap_or_else(|| panic!("missing icon {name}"));
            assert_eq!((region.width, region.height), (32, 32));
            assert!(region.x + region.width <= ICON_ATLAS_WIDTH);
            assert!(region.y + region.height <= ICON_ATLAS_HEIGHT);
        }
        assert!(regions.len() >= 80);
    }
}
