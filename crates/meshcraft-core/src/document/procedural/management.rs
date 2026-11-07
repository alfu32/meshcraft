//! Managing stored shader programs: usage and deletion (T83).
use super::*;

/// What refers to one program.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProgramUsage {
    /// Names of materials whose procedural program is this one.
    pub materials: Vec<String>,
    /// Shader-object settings (prototypes, instance edits) that generate geometry with it.
    pub objects: usize,
}

impl Document {
    pub fn procedural_program_usage(&self, id: &str) -> ProgramUsage {
        ProgramUsage {
            materials: self
                .materials()
                .into_iter()
                .filter(|m| m.procedural.as_ref().is_some_and(|p| p.program == id))
                .map(|m| m.name)
                .collect(),
            objects: self
                .procedural_object_settings()
                .iter()
                .filter(|m| m.procedural.as_ref().is_some_and(|p| p.program == id))
                .count(),
        }
    }

    /// Removes a stored program. Materials that use it become conventional
    /// materials, keeping their colour, texture and PBR settings. Built-ins can't be
    /// removed, and neither can a program that shader objects still generate from,
    /// since those objects would have no geometry. Returns the materials reset.
    pub fn remove_procedural_program(&mut self, id: &str) -> Result<Vec<String>, String> {
        if is_builtin(id) {
            return Err("Built-in shader programs can't be deleted".into());
        }
        let programs = self
            .procedural_programs()
            .into_iter()
            .filter(|p| !is_builtin(&p.id))
            .collect::<Vec<_>>();
        if !programs.iter().any(|p| p.id == id) {
            return Err(format!("Shader program {id} doesn't exist"));
        }
        let usage = self.procedural_program_usage(id);
        if usage.objects > 0 {
            return Err(format!(
                "{} shader object{} still use{} {id}; delete or bake {} first",
                usage.objects,
                if usage.objects == 1 { "" } else { "s" },
                if usage.objects == 1 { "s" } else { "" },
                if usage.objects == 1 { "it" } else { "them" },
            ));
        }
        let mut candidate = self.clone();
        for mut material in candidate.materials() {
            if material
                .procedural
                .as_ref()
                .is_some_and(|p| p.program == id)
            {
                material.procedural = None;
                candidate.put_material(material)?;
            }
        }
        candidate.extensions.insert(
            PROGRAMS_KEY.into(),
            serde_json::to_value(
                programs
                    .into_iter()
                    .filter(|p| p.id != id)
                    .collect::<Vec<_>>(),
            )
            .map_err(|e| e.to_string())?,
        );
        candidate.validate_procedural_materials()?;
        candidate.bump_revision();
        *self = candidate;
        Ok(usage.materials)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::materials::Material;

    fn custom(id: &str) -> Program {
        Program {
            id: id.into(),
            name: id.into(),
            fragment: "vec4 proceduralColor(vec4 c,vec3 p,vec3 n,vec2 uv){return c;}".into(),
            ..Default::default()
        }
    }

    #[test]
    fn deleting_a_program_resets_its_materials_and_keeps_the_rest() {
        let mut d = Document::new();
        d.put_procedural_program(custom("custom.a")).unwrap();
        d.put_procedural_program(custom("custom.b")).unwrap();
        let mut a = d.next_material();
        a.name = "Uses A".into();
        a.color = Rgba8::new(10, 20, 30, 255);
        a.procedural = Some(ProceduralMaterial {
            program: "custom.a".into(),
            ..Default::default()
        });
        d.put_material(a.clone()).unwrap();
        let mut b = d.next_material();
        b.name = "Uses B".into();
        b.procedural = Some(ProceduralMaterial {
            program: "custom.b".into(),
            ..Default::default()
        });
        d.put_material(b).unwrap();
        assert_eq!(
            d.procedural_program_usage("custom.a").materials,
            vec!["Uses A".to_owned()]
        );

        assert_eq!(
            d.remove_procedural_program("custom.a").unwrap(),
            vec!["Uses A".to_owned()]
        );
        let programs = d.procedural_programs();
        assert!(!programs.iter().any(|p| p.id == "custom.a"));
        assert!(programs.iter().any(|p| p.id == "custom.b"));
        let materials = d.materials();
        let reset = materials.iter().find(|m| m.id == a.id).unwrap();
        assert_eq!(reset.procedural, None);
        assert_eq!(reset.color, a.color);
        let kept = materials.iter().find(|m| m.name == "Uses B").unwrap();
        assert_eq!(kept.procedural.as_ref().unwrap().program, "custom.b");

        assert!(d.remove_procedural_program("custom.a").is_err());
        assert!(d.remove_procedural_program(GRASS).is_err());
        assert!(d.remove_procedural_program(METAL).is_err());
    }

    #[test]
    fn programs_that_shader_objects_use_cannot_be_deleted() {
        let mut d = Document::new();
        let mut program = blank_object_program();
        program.id = "custom.object.1".into();
        d.put_procedural_program(program.clone()).unwrap();
        let id = d.create_procedural_tree().unwrap();
        let settings = Material {
            procedural: Some(ProceduralMaterial {
                program: program.id.clone(),
                ..Default::default()
            }),
            ..Default::default()
        };
        d.set_procedural_object(id, None, settings).unwrap();
        assert_eq!(d.procedural_program_usage(&program.id).objects, 1);
        let before = d.clone();
        let error = d.remove_procedural_program(&program.id).unwrap_err();
        assert!(error.contains("1 shader object still uses"), "{error}");
        assert_eq!(d.procedural_programs(), before.procedural_programs());
    }

    #[test]
    fn standard_library_programs_are_valid_read_only_surfaces() {
        let ids = library_programs()
            .into_iter()
            .map(|p| {
                p.validate().unwrap();
                assert_eq!(p.generation, "surface", "{}", p.id);
                assert!(is_builtin(&p.id));
                assert!(preset_look(&p.id).is_some(), "{}", p.id);
                p.id
            })
            .collect::<Vec<_>>();
        assert_eq!(ids, LIBRARY);
        assert!(glass_program().transparent);
        let all = builtin_programs();
        for id in LIBRARY {
            assert_eq!(all.iter().filter(|p| p.id == id).count(), 1);
        }
        let mut d = Document::new();
        let mut copy = metal_program();
        assert!(d.put_procedural_program(copy.clone()).is_err());
        copy.id = "custom.metal".into();
        d.put_procedural_program(copy).unwrap();
    }
}
