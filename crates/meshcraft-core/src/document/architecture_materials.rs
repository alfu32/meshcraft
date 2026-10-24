//! Stable material references for architecture surfaces; legacy RGBA remains an IO fallback.
use super::*;
impl Element {
    pub fn surface_materials(&self) -> &BTreeMap<String, String> {
        match self {
            Self::Wall(v) => &v.materials,
            Self::Slab(v) => &v.materials,
            Self::Frame(v) => &v.materials,
        }
    }
    pub fn procedural_parameters_mut(&mut self) -> &mut BTreeMap<String, BTreeMap<String, Value>> {
        match self {
            Self::Wall(w) => &mut w.procedural_parameters,
            Self::Slab(s) => &mut s.procedural_parameters,
            Self::Frame(f) => &mut f.procedural_parameters,
        }
    }
    pub fn surface_materials_mut(&mut self) -> &mut BTreeMap<String, String> {
        match self {
            Self::Wall(v) => &mut v.materials,
            Self::Slab(v) => &mut v.materials,
            Self::Frame(v) => &mut v.materials,
        }
    }
}
impl Document {
    pub fn resolve_architecture_materials(&mut self, element: &mut Element) -> Result<(), String> {
        let resolve = |doc: &mut Document,
                       ids: &mut BTreeMap<String, String>,
                       key: &str,
                       color: &mut Color|
         -> Result<(), String> {
            let id = if let Some(id) = ids.get(key) {
                id.clone()
            } else {
                let id = doc.material_for_color(color.rgba())?;
                ids.insert(key.into(), id.clone());
                id
            };
            let c = doc
                .material_colors()
                .get(&id)
                .copied()
                .ok_or_else(|| format!("Unknown material {id}"))?;
            *color = Color {
                r: c.red as f64 / 255.,
                g: c.green as f64 / 255.,
                b: c.blue as f64 / 255.,
                a: c.alpha as f64 / 255.,
            };
            Ok(())
        };
        match element {
            Element::Wall(w) => {
                resolve(self, &mut w.materials, "exterior", &mut w.exterior_color)?;
                resolve(self, &mut w.materials, "interior", &mut w.interior_color)?;
            }
            Element::Slab(s) => {
                resolve(self, &mut s.materials, "top", &mut s.top_color)?;
                resolve(self, &mut s.materials, "bottom", &mut s.bottom_color)?;
                resolve(self, &mut s.materials, "side", &mut s.side_color)?;
            }
            Element::Frame(f) => {
                resolve(self, &mut f.materials, "frame", &mut f.color)?;
                resolve(self, &mut f.materials, "glazing", &mut f.glazing_color)?;
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn surface_material_ids_survive_equal_colors_edits_and_regeneration() {
        let mut d = Document::new();
        d.ensure_scene_root();
        let mut a = d.next_material();
        a.color = Rgba8::new(190, 80, 25, 180);
        d.put_material(a.clone()).unwrap();
        let mut b = d.next_material();
        b.color = a.color;
        b.shading_type = materials::ShadingType::Flat;
        d.put_material(b.clone()).unwrap();
        let mut wall = Wall::default();
        wall.materials.insert("exterior".into(), a.id.clone());
        wall.materials.insert("interior".into(), b.id.clone());
        let id = d
            .put_architecture(
                Document::SCENE_ROOT_DEFINITION_ID,
                None,
                Element::Wall(wall),
            )
            .unwrap();
        let g = &d
            .definition(Document::SCENE_ROOT_DEFINITION_ID)
            .unwrap()
            .geometry;
        for material in [&a.id, &b.id] {
            assert!(
                g.triangles
                    .values()
                    .any(|f| materials::material_id(&f.extensions) == Some(material))
            );
        }
        a.color = Rgba8::new(22, 88, 190, 200);
        d.put_material(a.clone()).unwrap();
        assert!(d.regenerate_architecture().is_empty());
        let w = d
            .wall_records(Document::SCENE_ROOT_DEFINITION_ID)
            .into_iter()
            .find(|(i, _)| i == &id)
            .unwrap()
            .1;
        assert_eq!(w.exterior_color.rgba(), a.color);
        assert_eq!(w.interior_color.rgba(), b.color);
        let g = &d
            .definition(Document::SCENE_ROOT_DEFINITION_ID)
            .unwrap()
            .geometry;
        assert!(
            g.triangles
                .values()
                .filter(|f| materials::material_id(&f.extensions) == Some(a.id.as_str()))
                .all(|f| f.color == a.color)
        );
    }
}
