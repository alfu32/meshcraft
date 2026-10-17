use std::collections::BTreeMap;

use eframe::egui;

const ICON_ATLAS_PNG: &[u8] = include_bytes!("../../../assets/tool-icons/icons.png");
const ICON_MAPPING_CSV: &str = include_str!("../../../assets/tool-icons/icons.mapping.csv");
const ICON_ATLAS_SIZE: u32 = 320;

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
    if width != ICON_ATLAS_SIZE || height != ICON_ATLAS_SIZE {
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
                || x1 < x0
                || y1 < y0
                || x1 >= ICON_ATLAS_SIZE
                || y1 >= ICON_ATLAS_SIZE
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
    use super::{ICON_ATLAS_SIZE, icon_regions};

    #[test]
    fn current_tool_icons_are_mapped_inside_the_atlas() {
        let regions = icon_regions();
        for name in [
            "select",
            "construction_line",
            "rectangle",
            "camera_rotating_operation",
        ] {
            let region = regions
                .get(name)
                .unwrap_or_else(|| panic!("missing icon {name}"));
            assert_eq!((region.width, region.height), (32, 32));
            assert!(region.x + region.width <= ICON_ATLAS_SIZE);
            assert!(region.y + region.height <= ICON_ATLAS_SIZE);
        }
        assert!(regions.len() >= 80);
    }
}
