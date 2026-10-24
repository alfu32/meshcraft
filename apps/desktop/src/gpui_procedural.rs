//! GPU-only surface/blade generation. CPU buffers contain only distribution triangles.
#[path = "gpui_procedural_shader.rs"]
mod shader_compiler;
use super::*;
use meshcraft_core::{
    document::{
        materials::{Material, material_id},
        procedural::{GRASS, OVERRIDES_KEY, Program},
    },
    geometry::{Aabb3, Affine3},
};
use raylib::prelude::{RaylibShader, Shader};
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct Domain {
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    seed: [f32; 2],
}
struct DomainGpu {
    vao: u32,
    vbo: u32,
    data: Vec<Domain>,
    bounds: Aabb3,
    max_area: f64,
}
impl Drop for DomainGpu {
    fn drop(&mut self) {
        unsafe {
            raylib::ffi::rlUnloadVertexArray(self.vao);
            raylib::ffi::rlUnloadVertexBuffer(self.vbo);
        }
    }
}
impl DomainGpu {
    fn new(data: Vec<Domain>) -> Self {
        let bounds = Aabb3::from_points(
            data.iter()
                .flat_map(|d| [d.a, d.b, d.c])
                .map(|p| Vec3::new(p[0] as f64, p[1] as f64, p[2] as f64)),
        )
        .unwrap();
        unsafe {
            let vao = raylib::ffi::rlLoadVertexArray();
            let vbo = raylib::ffi::rlLoadVertexBuffer(
                data.as_ptr().cast(),
                std::mem::size_of_val(data.as_slice()) as i32,
                false,
            );
            raylib::ffi::rlEnableVertexArray(vao);
            raylib::ffi::rlEnableVertexBuffer(vbo);
            for (i, n, offset) in [(0, 3, 0), (1, 3, 12), (2, 3, 24), (3, 2, 36)] {
                raylib::ffi::rlSetVertexAttribute(
                    i,
                    n,
                    raylib::ffi::RL_FLOAT as i32,
                    false,
                    44,
                    offset,
                );
                raylib::ffi::rlEnableVertexAttribute(i);
            }
            raylib::ffi::rlDisableVertexArray();
            raylib::ffi::rlDisableVertexBuffer();
            Self {
                vao,
                vbo,
                bounds,
                max_area: data
                    .iter()
                    .map(|d| {
                        let p = |a: [f32; 3]| Vec3::new(a[0] as f64, a[1] as f64, a[2] as f64);
                        (p(d.b) - p(d.a)).cross(p(d.c) - p(d.a)).length() * 0.5
                    })
                    .fold(0., f64::max),
                data,
            }
        }
    }
}
struct Compiled {
    source: (String, String),
    shader: Option<Shader>,
    error: Option<String>,
}
struct Batch {
    domain: usize,
    program: usize,
    material: Material,
    values: BTreeMap<String, Value>,
    placements: Vec<(SelectionTarget, Affine3)>,
    expansion: f64,
}
#[derive(Default)]
pub(super) struct Renderer {
    domains: Vec<DomainGpu>,
    programs: Vec<Compiled>,
    batches: Vec<Batch>,
    pub surface_materials: BTreeSet<String>,
    pub diagnostics: Vec<String>,
    pub animated: bool,
    pub candidates: usize,
    pub draw_calls: usize,
    pub domain_uploads: usize,
    light: Option<raylib::ffi::Matrix>,
    epoch: Option<Instant>,
    time: f32,
    textures: HashMap<String, Option<raylib::prelude::Texture2D>>,
}
fn uniform_type(kind: &str) -> &str {
    match kind {
        "boolean" | "integer" | "enum" => "int",
        "unsigned" | "seed" => "uint",
        "vec2" => "vec2",
        "vec3" => "vec3",
        "vec4" | "color" => "vec4",
        "matrix" => "mat4",
        "texture" => "sampler2D",
        _ => "float",
    }
}
fn shader_sources(p: &Program) -> (String, String) {
    let mut declarations = String::new();
    let mut ids = BTreeSet::new();
    for param in &p.parameters {
        declarations.push_str(&format!(
            "uniform {} u_{};\n",
            uniform_type(&param.kind),
            param.id
        ));
        ids.insert(param.id.as_str());
    }
    // The common blade generator also compiles for surface-only programs.
    for name in ["height", "width", "wind", "curvature", "variation", "seed"] {
        if !ids.contains(name) {
            declarations.push_str(&format!(
                "const {} u_{} = {};\n",
                if name == "seed" { "uint" } else { "float" },
                name,
                if name == "seed" { "0u" } else { "0.0" }
            ));
        }
    }
    (
        include_str!("../../../assets/shaders/procedural.vs")
            .replace("/*PARAMETERS*/", &declarations)
            .replace("/*VERTEX*/", &p.vertex),
        include_str!("../../../assets/shaders/procedural.fs")
            .replace("/*PARAMETERS*/", &declarations)
            .replace("/*FRAGMENT*/", &p.fragment),
    )
}
fn matrix(m: Affine3) -> raylib::ffi::Matrix {
    let a = m.linear;
    let t = m.translation;
    raylib::ffi::Matrix {
        m0: a[0][0] as f32,
        m1: a[1][0] as f32,
        m2: a[2][0] as f32,
        m3: 0.,
        m4: a[0][1] as f32,
        m5: a[1][1] as f32,
        m6: a[2][1] as f32,
        m7: 0.,
        m8: a[0][2] as f32,
        m9: a[1][2] as f32,
        m10: a[2][2] as f32,
        m11: 0.,
        m12: t.x as f32,
        m13: t.y as f32,
        m14: t.z as f32,
        m15: 1.,
    }
}
fn num(values: &BTreeMap<String, Value>, key: &str, default: f64) -> f64 {
    values.get(key).and_then(Value::as_f64).unwrap_or(default)
}
impl Renderer {
    pub fn prepare(
        &mut self,
        rl: &mut raylib::RaylibHandle,
        thread: &raylib::RaylibThread,
        scene: &RenderScene,
    ) {
        self.surface_materials.clear();
        self.diagnostics.clear();
        self.batches.clear();
        self.animated = false;
        let programs = scene.document.procedural_programs();
        let materials = scene.document.materials();
        let mut compiled = HashMap::new();
        let mut active_programs = BTreeSet::new();
        for m in &materials {
            let Some(config) = &m.procedural else {
                continue;
            };
            let Some(program) = programs.iter().find(|p| p.id == config.program) else {
                self.diagnostics.push(format!(
                    "{}: unknown program {}; showing base material",
                    m.name, config.program
                ));
                continue;
            };
            if !program.supported() {
                self.diagnostics.push(format!(
                    "{}: unsupported {} program {}; showing base material",
                    m.name, program.backend, program.id
                ));
                continue;
            }
            let source = shader_sources(program);
            let index = self
                .programs
                .iter()
                .position(|p| p.source == source)
                .unwrap_or_else(|| {
                    let result = shader_compiler::compile(&source.0, &source.1);
                    let (shader, error) = match result {
                        Ok(shader) => (Some(shader), None),
                        Err(e) => (None, Some(format!("{}: {e}", program.name))),
                    };
                    self.programs.push(Compiled {
                        source,
                        shader,
                        error,
                    });
                    self.programs.len() - 1
                });
            active_programs.insert(index);
            if let Some(e) = &self.programs[index].error {
                self.diagnostics.push(e.clone());
                continue;
            }

            compiled.insert(m.id.clone(), index);
        }
        let mut old = std::mem::take(&mut self.domains);
        // One source buffer per actual domain; repeated occurrences share it.
        let mut source_groups = BTreeMap::<
            (usize, String, String),
            (
                Vec<Domain>,
                BTreeMap<String, Value>,
                Vec<(SelectionTarget, Affine3)>,
            ),
        >::new();
        let mut local_cache = HashMap::<usize, Vec<(String, String)>>::new();
        for occurrence in scene.document.scene_geometry_refs().unwrap_or_default() {
            if !scene
                .display_policy
                .occurrence_visible(&scene.document, &occurrence)
            {
                continue;
            }
            if scene.document.is_object_occurrence(&occurrence)
                && scene
                    .display_policy
                    .state(meshcraft_core::selection::SelectionKind::Object)
                    .wireframe
            {
                continue;
            }
            let (Some(r), Some(g)) = (
                scene.document.resolve_occurrence(&occurrence),
                scene.document.effective_geometry(&occurrence),
            ) else {
                continue;
            };
            let source_key = g as *const _ as usize;
            if let Some(keys) = local_cache.get(&source_key) {
                for (id, key) in keys {
                    source_groups
                        .get_mut(&(source_key, id.clone(), key.clone()))
                        .unwrap()
                        .2
                        .push((
                            SelectionTarget {
                                occurrence: occurrence.clone(),
                                entity: SelectedEntity::Occurrence,
                            },
                            r.world_transform,
                        ));
                }
                continue;
            }
            let mut local =
                BTreeMap::<(String, String), (Vec<Domain>, BTreeMap<String, Value>)>::new();
            for face in g.triangles.values() {
                let entity =
                    meshcraft_core::document::architecture::semantic_entity(&face.extensions)
                        .unwrap_or(SelectedEntity::Triangle(face.id));
                let target = SelectionTarget {
                    occurrence: occurrence.clone(),
                    entity,
                };
                if !scene
                    .display_policy
                    .can_draw_target(&scene.document, &target)
                    || scene.display_policy.state(target.entity.kind()).wireframe
                {
                    continue;
                }
                let Some(id) =
                    material_id(&face.extensions).filter(|id| compiled.contains_key(*id))
                else {
                    continue;
                };
                let m = materials.iter().find(|m| m.id == id).unwrap();
                let config = m.procedural.as_ref().unwrap();
                let program = programs.iter().find(|p| p.id == config.program).unwrap();
                let overrides = face
                    .extensions
                    .get(OVERRIDES_KEY)
                    .cloned()
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_default();
                let Ok(values) = program.resolve(&config.parameters, &overrides) else {
                    self.diagnostics.push(format!(
                        "{}: invalid assignment parameters; showing base surface",
                        m.name
                    ));
                    continue;
                };
                if program.generation == "surface" {
                    self.surface_materials
                        .insert(surface_key(id, &face.extensions));
                }
                let key = (id.to_owned(), serde_json::to_string(&values).unwrap());
                let entry = local.entry(key).or_insert_with(|| (vec![], values));
                let [Some(a), Some(b), Some(c)] = face.vertices.map(|v| g.vertex_position(v))
                else {
                    continue;
                };
                if (b - a).cross(c - a).length() < 1e-12 {
                    continue;
                }
                let xyz = |p: Vec3| [p.x as f32, p.y as f32, p.z as f32];
                let seed = domain_seed([a, b, c]);
                entry.0.push(Domain {
                    a: xyz(a),
                    b: xyz(b),
                    c: xyz(c),
                    seed: [(seed & 65535) as f32, (seed >> 16) as f32],
                });
            }
            local_cache.insert(source_key, local.keys().cloned().collect());
            for ((id, key), (domain, values)) in local {
                let group = source_groups
                    .entry((g as *const _ as usize, id, key))
                    .or_insert_with(|| (domain, values, vec![]));
                group.2.push((
                    SelectionTarget {
                        occurrence: occurrence.clone(),
                        entity: SelectedEntity::Occurrence,
                    },
                    r.world_transform,
                ));
            }
        }
        for ((_, id, _), (data, values, placements)) in source_groups {
            if data.is_empty() {
                continue;
            }
            let domain = self
                .domains
                .iter()
                .position(|d| d.data == data)
                .unwrap_or_else(|| {
                    let gpu = old
                        .iter()
                        .position(|d| d.data == data)
                        .map(|i| old.swap_remove(i))
                        .unwrap_or_else(|| {
                            self.domain_uploads += 1;
                            DomainGpu::new(data)
                        });
                    self.domains.push(gpu);
                    self.domains.len() - 1
                });
            let material = materials.iter().find(|m| m.id == id).unwrap().clone();
            let config = material.procedural.as_ref().unwrap();
            let p = programs.iter().find(|p| p.id == config.program).unwrap();
            self.animated |= p.animated && (p.id != GRASS || num(&values, "wind", 0.) > 0.);
            for parameter in p.parameters.iter().filter(|p| p.kind == "texture") {
                let path = values
                    .get(&parameter.id)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                if !self.textures.contains_key(&path) {
                    let texture = if path.is_empty() {
                        None
                    } else {
                        rl.load_texture(thread, &path).ok()
                    };
                    self.textures.insert(path.clone(), texture);
                }
                if !path.is_empty() && self.textures[&path].is_none() {
                    self.diagnostics
                        .push(format!("Texture unavailable: {path}"));
                }
            }
            let base = config
                .base_material
                .as_ref()
                .and_then(|id| materials.iter().find(|m| &m.id == id))
                .unwrap_or(&material);
            if let Some(path) = &base.texture_path {
                if !self.textures.contains_key(path) {
                    self.textures
                        .insert(path.clone(), rl.load_texture(thread, path).ok());
                }
                if self.textures[path].is_none() {
                    self.diagnostics
                        .push(format!("Base texture unavailable: {path}"));
                }
            }
            if p.generation == "blades"
                && self.domains[domain].data.len() > config.max_instances as usize
            {
                self.diagnostics.push(format!("{}: {} source triangles exceed the {} candidate budget; increase the budget or simplify the domain",material.name,self.domains[domain].data.len(),config.max_instances));
            }
            let extra = p.bounds_expansion
                + if p.generation == "blades" {
                    num(&values, "height", 0.)
                        * 1.3
                        * (1. + num(&values, "curvature", 0.).abs() + num(&values, "wind", 0.))
                        + num(&values, "width", 0.)
                } else {
                    0.
                };
            if let Some(batch) = self.batches.iter_mut().find(|b| {
                b.domain == domain
                    && b.program == compiled[&id]
                    && b.material == material
                    && b.values == values
            }) {
                batch.placements.extend(placements);
                continue;
            }
            self.batches.push(Batch {
                domain,
                program: compiled[&id],
                material,
                values,
                placements,
                expansion: extra,
            });
        }
        let mut remap = HashMap::new();
        self.programs = std::mem::take(&mut self.programs)
            .into_iter()
            .enumerate()
            .filter_map(|(old, p)| {
                if active_programs.contains(&old) {
                    let next = remap.len();
                    remap.insert(old, next);
                    Some(p)
                } else {
                    None
                }
            })
            .collect();
        for b in &mut self.batches {
            b.program = remap[&b.program];
        }
        let used_textures = self
            .batches
            .iter()
            .flat_map(|b| b.values.values().filter_map(Value::as_str))
            .collect::<BTreeSet<_>>();
        let base_paths = materials
            .iter()
            .filter_map(|m| m.texture_path.as_deref())
            .collect::<BTreeSet<_>>();
        self.textures
            .retain(|p, _| used_textures.contains(p.as_str()) || base_paths.contains(p.as_str()));
        self.diagnostics.sort();
        self.diagnostics.dedup();
    }
    pub fn tick(&mut self) {
        self.time = self
            .epoch
            .get_or_insert_with(Instant::now)
            .elapsed()
            .as_secs_f32();
    }
    pub fn set_light(&mut self, m: raylib::ffi::Matrix) {
        self.light = Some(m);
    }
    pub fn bounds(&self) -> Option<Aabb3> {
        Aabb3::from_points(self.batches.iter().flat_map(|b| {
            b.placements.iter().flat_map(move |(_, m)| {
                corners(self.domains[b.domain].bounds, b.expansion).map(|p| m.transform_point(p))
            })
        }))
    }
    pub fn draw(
        &mut self,
        scene: &RenderScene,
        camera: Option<OrbitCamera>,
        depth: bool,
        shadow: u32,
        translucent: bool,
    ) {
        unsafe {
            raylib::ffi::rlDrawRenderBatchActive();
            raylib::ffi::rlDisableBackfaceCulling();
        }
        if !depth && !translucent {
            self.animated = false;
            self.candidates = 0;
            self.draw_calls = 0;
        }
        let vp = unsafe {
            raylib::ffi::MatrixMultiply(
                raylib::ffi::rlGetMatrixModelview(),
                raylib::ffi::rlGetMatrixProjection(),
            )
        };
        let catalog = scene.document.materials();
        let programs = scene.document.procedural_programs();
        for batch in &self.batches {
            let config = batch.material.procedural.as_ref().unwrap();
            let program = programs.iter().find(|p| p.id == config.program).unwrap();
            let Some(shader) = self.programs[batch.program].shader.as_mut() else {
                continue;
            };
            shader.set_shader_value_matrix(shader.get_shader_location("viewProjection"), vp);
            if let Some(light) = self.light {
                shader.set_shader_value_matrix(shader.get_shader_location("lightVP"), light);
            }
            shader.set_shader_value(shader.get_shader_location("time"), self.time);
            shader.set_shader_value(shader.get_shader_location("depthPass"), i32::from(depth));
            shader.set_shader_value(
                shader.get_shader_location("generation"),
                i32::from(program.generation == "blades"),
            );
            shader.set_shader_value(
                shader.get_shader_location("towardLight"),
                ray_vec3(scene.lighting.direction()),
            );
            shader.set_shader_value(
                shader.get_shader_location("eye"),
                ray_vec3(camera.map_or(Vec3::ZERO, |c| c.position())),
            );
            for (name, value) in [
                ("ambient", scene.lighting.ambient),
                ("directional", scene.lighting.directional),
                ("specular", scene.lighting.specular),
                ("shadowStrength", scene.lighting.shadow_strength),
                ("depthBias", scene.lighting.depth_bias),
                ("normalBias", scene.lighting.normal_bias),
            ] {
                shader.set_shader_value(shader.get_shader_location(name), value);
            }
            shader.set_shader_value(
                shader.get_shader_location("shadows"),
                i32::from(scene.lighting.shadows && !depth),
            );
            shader.set_shader_value(shader.get_shader_location("shadowMap"), 1i32);
            let base = config
                .base_material
                .as_ref()
                .and_then(|id| catalog.iter().find(|m| &m.id == id))
                .unwrap_or(&batch.material);
            let color = base.color;
            shader.set_shader_value(
                shader.get_shader_location("pbrMode"),
                i32::from(base.pbr.is_some()),
            );
            shader.set_shader_value(
                shader.get_shader_location("roughness"),
                base.pbr.as_ref().map_or(0.6, |p| p.roughness as f32),
            );
            shader.set_shader_value(
                shader.get_shader_location("metallic"),
                base.pbr.as_ref().map_or(0., |p| p.metallic as f32),
            );
            shader.set_shader_value(shader.get_shader_location("baseTexture"), 10i32);
            shader.set_shader_value(
                shader.get_shader_location("hasBaseTexture"),
                i32::from(
                    base.texture_path
                        .as_ref()
                        .is_some_and(|p| self.textures.get(p).is_some_and(Option::is_some)),
                ),
            );
            shader.set_shader_value(
                shader.get_shader_location("textureSizes"),
                raylib::prelude::Vector2::new(
                    base.texture_scale_u as f32,
                    base.texture_scale_v as f32,
                ),
            );
            unsafe {
                raylib::ffi::rlActiveTextureSlot(10);
                raylib::ffi::rlEnableTexture(
                    base.texture_path
                        .as_ref()
                        .and_then(|p| self.textures.get(p))
                        .and_then(Option::as_ref)
                        .map_or(raylib::ffi::rlGetTextureIdDefault(), |t| t.id),
                );
                raylib::ffi::rlActiveTextureSlot(0);
            }
            let transparent = color.alpha < 255 || program.transparent;
            if !depth && transparent != translucent {
                continue;
            }
            shader.set_shader_value(
                shader.get_shader_location("baseColor"),
                raylib::prelude::Vector4::new(
                    color.red as f32 / 255.,
                    color.green as f32 / 255.,
                    color.blue as f32 / 255.,
                    color.alpha as f32 / 255.,
                ),
            );
            bind_parameters(shader, program, &batch.values);
            let mut texture_slot = 2i32;
            for p in program.parameters.iter().filter(|p| p.kind == "texture") {
                if let Some(path) = batch.values.get(&p.id).and_then(Value::as_str) {
                    shader.set_shader_value(
                        shader.get_shader_location(&format!("u_{}", p.id)),
                        texture_slot,
                    );
                    unsafe {
                        raylib::ffi::rlActiveTextureSlot(texture_slot);
                        if let Some(Some(texture)) = self.textures.get(path) {
                            raylib::ffi::rlEnableTexture(texture.id);
                        } else {
                            raylib::ffi::rlEnableTexture(raylib::ffi::rlGetTextureIdDefault());
                        }
                    }
                    texture_slot += 1;
                }
            }
            unsafe {
                raylib::ffi::rlActiveTextureSlot(0);
            }

            let domain = &self.domains[batch.domain];
            for (target, model) in &batch.placements {
                let center = model.transform_point((domain.bounds.min + domain.bounds.max) * 0.5);
                let distance = camera.map_or(0., |c| c.position().distance(center));
                let lod = 1. / (1. + (distance / config.lod_distance).powi(2));
                let density = num(&batch.values, "density", 1.) * lod;
                if !visible(domain.bounds, batch.expansion, *model, vp) {
                    continue;
                }
                let cols = [
                    Vec3::new(model.linear[0][0], model.linear[1][0], model.linear[2][0]),
                    Vec3::new(model.linear[0][1], model.linear[1][1], model.linear[2][1]),
                    Vec3::new(model.linear[0][2], model.linear[1][2], model.linear[2][2]),
                ];
                let max_area = domain.max_area
                    * (cols[0].cross(cols[1]).length_squared()
                        + cols[1].cross(cols[2]).length_squared()
                        + cols[2].cross(cols[0]).length_squared())
                    .sqrt();
                if program.generation == "blades"
                    && (density <= 0. || domain.data.len() > config.max_instances as usize)
                {
                    continue;
                }
                let per = if program.generation == "blades" {
                    ((max_area * density).ceil() as usize)
                        .min(config.max_instances as usize / domain.data.len())
                        .max(1)
                } else {
                    1
                };
                shader.set_shader_value_matrix(shader.get_shader_location("model"), matrix(*model));
                shader
                    .set_shader_value(shader.get_shader_location("bladesPerTriangle"), per as i32);
                shader.set_shader_value(shader.get_shader_location("density"), density as f32);
                shader.set_shader_value(
                    shader.get_shader_location("externalGeometry"),
                    i32::from(lighting::external(
                        &scene.edit_context,
                        &scene.document,
                        target,
                    )),
                );
                unsafe {
                    raylib::ffi::rlEnableShader(shader.id);
                    raylib::ffi::rlActiveTextureSlot(1);
                    raylib::ffi::rlEnableTexture(shadow);
                    raylib::ffi::rlActiveTextureSlot(0);
                    if transparent && !depth {
                        raylib::ffi::rlDisableDepthMask();
                    }
                    raylib::ffi::rlEnableVertexArray(domain.vao);
                    for i in 0..4 {
                        raylib::ffi::rlSetVertexAttributeDivisor(i, per as i32);
                    }
                    raylib::ffi::rlDrawVertexArrayInstanced(
                        0,
                        if program.generation == "blades" {
                            18
                        } else {
                            3
                        },
                        (domain.data.len() * per) as i32,
                    );
                    raylib::ffi::rlDisableVertexArray();
                    raylib::ffi::rlEnableDepthMask();
                }
                if !depth && config.show_bounds {
                    let points =
                        corners(domain.bounds, batch.expansion).map(|p| model.transform_point(p));
                    for i in 0..8 {
                        for bit in [1, 2, 4] {
                            if i & bit == 0 {
                                unsafe {
                                    raylib::ffi::rlDisableShader();
                                    raylib::ffi::DrawLine3D(
                                        ray_vec3(points[i]),
                                        ray_vec3(points[i | bit]),
                                        Color::new(80, 180, 255, 255),
                                    );
                                }
                            }
                        }
                    }
                }
                if !depth {
                    self.animated |= program.animated
                        && (program.id != GRASS || num(&batch.values, "wind", 0.) > 0.);
                    self.candidates += domain.data.len() * per;
                    self.draw_calls += 1;
                }
            }
        }
        unsafe {
            for slot in 1..11 {
                raylib::ffi::rlActiveTextureSlot(slot);
                raylib::ffi::rlDisableTexture();
            }
            raylib::ffi::rlActiveTextureSlot(0);
            raylib::ffi::rlDisableShader();
            raylib::ffi::rlEnableBackfaceCulling();
        }
    }
}
fn bind_parameters(shader: &mut Shader, program: &Program, values: &BTreeMap<String, Value>) {
    for p in &program.parameters {
        let Some(v) = values.get(&p.id) else { continue };
        let loc = shader.get_shader_location(&format!("u_{}", p.id));
        if loc < 0 {
            continue;
        }
        match p.kind.as_str() {
            "boolean" => shader.set_shader_value(loc, i32::from(v.as_bool().unwrap_or(false))),
            "integer" => shader.set_shader_value(loc, v.as_i64().unwrap_or(0) as i32),
            "seed" | "unsigned" => {
                let n = v.as_u64().unwrap_or(0) as u32;
                unsafe {
                    raylib::ffi::rlSetUniform(loc, (&n as *const u32).cast(), 8, 1);
                }
            }
            "enum" => shader.set_shader_value(
                loc,
                p.options
                    .keys()
                    .position(|s| Some(s.as_str()) == v.as_str())
                    .unwrap_or(0) as i32,
            ),
            "vec2" | "vec3" | "vec4" | "color" | "matrix" => {
                let values = v
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap() as f32)
                    .collect::<Vec<_>>();
                unsafe {
                    if values.len() == 16 {
                        let a = &values;
                        raylib::ffi::rlSetUniformMatrix(
                            loc,
                            raylib::ffi::Matrix {
                                m0: a[0],
                                m1: a[1],
                                m2: a[2],
                                m3: a[3],
                                m4: a[4],
                                m5: a[5],
                                m6: a[6],
                                m7: a[7],
                                m8: a[8],
                                m9: a[9],
                                m10: a[10],
                                m11: a[11],
                                m12: a[12],
                                m13: a[13],
                                m14: a[14],
                                m15: a[15],
                            },
                        );
                    } else {
                        raylib::ffi::rlSetUniform(
                            loc,
                            values.as_ptr().cast(),
                            (values.len() - 1) as i32,
                            1,
                        );
                    }
                }
            }
            "texture" => {} // Bound by the texture resource cache added below.
            _ => shader.set_shader_value(loc, v.as_f64().unwrap_or(0.) as f32),
        }
    }
}

fn corners(bounds: Aabb3, extra: f64) -> [Vec3; 8] {
    let lo = bounds.min - Vec3::new(extra, extra, extra);
    let hi = bounds.max + Vec3::new(extra, extra, extra);
    std::array::from_fn(|i| {
        Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        )
    })
}
fn visible(bounds: Aabb3, extra: f64, model: Affine3, vp: raylib::ffi::Matrix) -> bool {
    let clip = corners(bounds, extra).map(|p| {
        let p = model.transform_point(p);
        let (x, y, z) = (p.x as f32, p.y as f32, p.z as f32);
        [
            vp.m0 * x + vp.m4 * y + vp.m8 * z + vp.m12,
            vp.m1 * x + vp.m5 * y + vp.m9 * z + vp.m13,
            vp.m2 * x + vp.m6 * y + vp.m10 * z + vp.m14,
            vp.m3 * x + vp.m7 * y + vp.m11 * z + vp.m15,
        ]
    });
    !(0..3).any(|axis| clip.iter().all(|p| p[axis] < -p[3]) || clip.iter().all(|p| p[axis] > p[3]))
}

pub(super) fn surface_key(id: &str, extensions: &BTreeMap<String, Value>) -> String {
    format!(
        "{id}:{}",
        extensions.get(OVERRIDES_KEY).unwrap_or(&Value::Null)
    )
}

fn domain_seed(points: [Vec3; 3]) -> u32 {
    // Geometry, not regenerated entity IDs, defines the distribution across save/load.
    let mut hash = 2166136261u32;
    for point in points {
        for value in [point.x, point.y, point.z] {
            for byte in ((value * 1e6).round() as i64).to_le_bytes() {
                hash = (hash ^ u32::from(byte)).wrapping_mul(16777619);
            }
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use meshcraft_core::{
        document::{materials::Material, procedural::ProceduralMaterial},
        operations::transforms::assign_material,
        selection::{SelectionMode, SelectionSession},
    };
    fn drawing() -> Document {
        let mut d = Document::new();
        let (_, faces) = d
            .append_planar_mesh_to_scene_root(
                vec![
                    Vec3::new(-4., 0., -4.),
                    Vec3::new(4., 0., -4.),
                    Vec3::new(4., 0., 4.),
                    Vec3::new(-4., 0., 4.),
                ],
                vec![[0, 1], [1, 2], [2, 3], [3, 0]],
                vec![[0, 2, 1], [0, 3, 2]],
            )
            .unwrap();
        let m = Material {
            id: "grass".into(),
            name: "Grass".into(),
            color: Rgba8::new(60, 140, 30, 255),
            procedural: Some(ProceduralMaterial {
                parameters: BTreeMap::from([
                    ("wind".into(), json!(0.)),
                    ("density".into(), json!(15.)),
                ]),
                ..Default::default()
            }),
            ..Default::default()
        };
        d.put_material(m).unwrap();
        let mut selection = SelectionSession::default();
        selection.apply(
            faces.into_iter().map(|id| SelectionTarget {
                occurrence: meshcraft_core::document::OccurrenceRef::scene_root(),
                entity: SelectedEntity::Triangle(id),
            }),
            SelectionMode::Replace,
        );
        assign_material(&mut d, &selection, "grass").unwrap();
        d
    }
    #[test]
    fn source_contract_and_conservative_frustum_bounds() {
        let source = shader_sources(&meshcraft_core::document::procedural::grass_program());
        assert!(source.0.contains("gl_InstanceID"));
        assert!(source.0.contains("uniform uint u_seed"));
        let identity = matrix(Affine3::IDENTITY);
        let b = Aabb3 {
            min: Vec3::new(-0.1, -0.1, -0.1),
            max: Vec3::new(0.1, 0.1, 0.1),
        };
        assert!(visible(b, 0., Affine3::IDENTITY, identity));
        assert!(!visible(
            b,
            0.,
            Affine3::from_translation(Vec3::X * 5.),
            identity
        ));
        assert!(visible(
            b,
            6.,
            Affine3::from_translation(Vec3::X * 5.),
            identity
        ));
    }
    #[test]
    #[ignore = "requires private X11/OpenGL 3.3; exercises actual GPU generation and fallback"]
    fn procedural_material_gpu_generation_and_fallback() {
        let (mut rl, thread) = raylib::init()
            .size(420, 320)
            .hidden()
            .title("procedural material test")
            .log_level(TraceLogLevel::LOG_WARNING)
            .build();
        let mut target = rl.load_render_texture(&thread, 420, 320).unwrap();
        let mut renderer = lighting::Renderer::new(&mut rl, &thread);
        let d = drawing();
        let topology = d.scene_root_definition().unwrap().geometry.clone();
        let mut scene = RenderScene {
            document: Arc::new(d.clone()),
            grid_visible: false,
            guides_visible: false,
            ..Default::default()
        };
        let camera = OrbitCamera {
            target: Vec3::ZERO,
            distance: 14.,
            yaw: 0.7,
            pitch: 0.65,
            ..Default::default()
        };
        renderer.prepare(&mut rl, &thread, &scene);
        assert!(
            renderer.gpu.procedural.diagnostics.is_empty(),
            "{:?}",
            renderer.gpu.procedural.diagnostics
        );
        assert_eq!(1, renderer.gpu.procedural.domains.len());
        let domain = renderer.gpu.procedural.domains[0].vao;
        let uploads = renderer.gpu.procedural.domain_uploads;
        draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            0,
            &mut renderer,
        );
        let first = copy_target_to_gpui_image(&target).unwrap();
        assert!(
            first
                .pixels()
                .filter(|p| i32::from(p[1]) > i32::from(p[0]) + 10
                    && i32::from(p[1]) > i32::from(p[2]) + 10)
                .count()
                > 200,
            "grass must render green blade pixels"
        );
        assert!(renderer.gpu.procedural.candidates > 100);
        assert!(renderer.gpu.procedural.candidates <= 50_000);
        draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            1,
            &mut renderer,
        );
        assert_eq!(
            first,
            copy_target_to_gpui_image(&target).unwrap(),
            "fixed time and seed are deterministic"
        );
        let mut shadow = rl.load_render_texture(&thread, 160, 160).unwrap();
        let light = Camera3D::orthographic(
            RayVector3::new(6., 8., 6.),
            RayVector3::zero(),
            RayVector3::new(0., 1., 0.),
            14.,
        );
        {
            let mut pass = rl.begin_texture_mode(&thread, &mut shadow);
            pass.clear_background(Color::WHITE);
            let _view = pass.begin_mode3D(light);
            renderer.gpu.procedural.draw(&scene, None, true, 0, false);
        }
        let shadow_pixels = copy_target_to_gpui_image(&shadow).unwrap();
        assert!(
            shadow_pixels
                .pixels()
                .filter(|p| p[0] != 255 || p[1] != 255 || p[2] != 255)
                .count()
                > 20,
            "generated blades alone must write the packed shadow depth"
        );
        let mut translucent = d.clone();
        let mut faded = translucent
            .materials()
            .into_iter()
            .find(|m| m.id == "grass")
            .unwrap();
        faded.color.alpha = 120;
        translucent.put_material(faded).unwrap();
        scene.document = Arc::new(translucent);
        renderer.prepare(&mut rl, &thread, &scene);
        {
            let mut pass = rl.begin_texture_mode(&thread, &mut shadow);
            pass.clear_background(Color::WHITE);
            let _view = pass.begin_mode3D(light);
            renderer.gpu.procedural.draw(&scene, None, true, 0, false);
        }
        assert!(
            copy_target_to_gpui_image(&shadow)
                .unwrap()
                .pixels()
                .all(|p| p[0] == 255 && p[1] == 255 && p[2] == 255),
            "blended blades do not cast opaque shadows"
        );
        scene.document = Arc::new(d.clone());
        renderer.prepare(&mut rl, &thread, &scene);
        let mut changed = d.clone();
        let mut m = changed
            .materials()
            .into_iter()
            .find(|m| m.id == "grass")
            .unwrap();
        m.procedural
            .as_mut()
            .unwrap()
            .parameters
            .insert("seed".into(), json!(42));
        changed.put_material(m).unwrap();
        scene.document = Arc::new(changed.clone());
        renderer.prepare(&mut rl, &thread, &scene);
        assert_eq!(domain, renderer.gpu.procedural.domains[0].vao);
        assert_eq!(
            uploads, renderer.gpu.procedural.domain_uploads,
            "parameter edits reuse distribution buffers"
        );
        draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            2,
            &mut renderer,
        );
        assert_ne!(
            first,
            copy_target_to_gpui_image(&target).unwrap(),
            "seed changes the distribution"
        );
        assert_eq!(
            topology.vertices,
            scene
                .document
                .scene_root_definition()
                .unwrap()
                .geometry
                .vertices
        );
        assert_eq!(
            topology.triangles.len(),
            scene
                .document
                .scene_root_definition()
                .unwrap()
                .geometry
                .triangles
                .len()
        );
        let mut m = changed
            .materials()
            .into_iter()
            .find(|m| m.id == "grass")
            .unwrap();
        m.procedural
            .as_mut()
            .unwrap()
            .parameters
            .insert("wind".into(), json!(0.8));
        changed.put_material(m).unwrap();
        scene.document = Arc::new(changed.clone());
        renderer.prepare(&mut rl, &thread, &scene);
        assert!(renderer.gpu.procedural.animated);
        renderer.gpu.procedural.time = 1.;
        draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            3,
            &mut renderer,
        );
        let windy = copy_target_to_gpui_image(&target).unwrap();
        renderer.gpu.procedural.time = 2.;
        draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            4,
            &mut renderer,
        );
        assert_ne!(windy, copy_target_to_gpui_image(&target).unwrap());
        assert_eq!(uploads, renderer.gpu.procedural.domain_uploads);
        let program = Program {
            id: "bad".into(),
            name: "Invalid shader".into(),
            vertex: "this is not GLSL".into(),
            ..Default::default()
        };
        changed.put_procedural_program(program).unwrap();
        let mut m = changed
            .materials()
            .into_iter()
            .find(|m| m.id == "grass")
            .unwrap();
        m.procedural.as_mut().unwrap().program = "bad".into();
        m.procedural.as_mut().unwrap().parameters.clear();
        changed.put_material(m).unwrap();
        scene.document = Arc::new(changed);
        renderer.prepare(&mut rl, &thread, &scene);
        assert!(!renderer.gpu.procedural.diagnostics.is_empty());
        assert!(renderer.gpu.procedural.batches.is_empty());
        assert!(renderer.gpu.procedural.surface_materials.is_empty());
        draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            5,
            &mut renderer,
        );
        assert!(
            copy_target_to_gpui_image(&target)
                .unwrap()
                .pixels()
                .filter(|p| i32::from(p[1]) > i32::from(p[0]) + 10
                    && i32::from(p[1]) > i32::from(p[2]) + 10)
                .count()
                > 200,
            "shader errors keep the base surface visible"
        );
        if let Ok(path) = std::env::var("MESHCRAFT_PROCEDURAL_SCREENSHOT") {
            first.save(path).unwrap();
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use meshcraft_core::{
        document::{
            materials::{Material, Pbr},
            procedural::ProceduralMaterial,
        },
        operations::transforms::{assign_material, assign_procedural_material},
        selection::{SelectionMode, SelectionSession},
    };
    #[test]
    #[ignore = "requires private X11/OpenGL; texture uniforms, PBR, sharing and scoped overrides"]
    fn procedural_surface_texture_pbr_and_instance_sharing() {
        let (mut rl, thread) = raylib::init()
            .size(420, 320)
            .hidden()
            .title("procedural surface test")
            .log_level(TraceLogLevel::LOG_WARNING)
            .build();
        let mut target = rl.load_render_texture(&thread, 420, 320).unwrap();
        let mut renderer = lighting::Renderer::new(&mut rl, &thread);
        let path = std::env::temp_dir().join(format!(
            "meshcraft-procedural-test-{}.png",
            std::process::id()
        ));
        image::RgbaImage::from_pixel(4, 4, image::Rgba([240, 20, 10, 255]))
            .save(&path)
            .unwrap();
        let mut d = Document::new();
        let first = d
            .add_cuboid("Source", Vec3::new(3., 3., 3.), Vec3::ZERO)
            .unwrap();
        let second = d.copy_root_occurrence(&first, Vec3::X * 5.).unwrap();
        let mut parameter = meshcraft_core::document::procedural::grass_program()
            .parameters
            .pop()
            .unwrap();
        parameter.id = "albedo".into();
        parameter.kind = "texture".into();
        parameter.default = json!(path.to_string_lossy());
        d.put_procedural_program(Program {id:"texture-surface".into(),name:"Textured surface".into(),parameters:vec![parameter],fragment:"vec4 proceduralColor(vec4 color,vec3 position,vec3 normal,vec2 uv){return texture(u_albedo,uv);}".into(),..Default::default()}).unwrap();
        let mut material = Material {
            id: "custom".into(),
            name: "Custom".into(),
            procedural: Some(ProceduralMaterial {
                program: "texture-surface".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        d.put_material(material.clone()).unwrap();
        let mut selection = SelectionSession::default();
        for p in [&first, &second] {
            let ids = d
                .effective_geometry(p)
                .unwrap()
                .triangles
                .keys()
                .copied()
                .collect::<Vec<_>>();
            for id in ids {
                selection.select(SelectionTarget {
                    occurrence: p.clone(),
                    entity: SelectedEntity::Triangle(id),
                });
            }
        }
        assign_material(&mut d, &selection, "custom").unwrap();
        let mut scene = RenderScene {
            document: Arc::new(d.clone()),
            grid_visible: false,
            guides_visible: false,
            ..Default::default()
        };
        let camera = OrbitCamera {
            target: Vec3::new(3., 1., 0.),
            distance: 18.,
            ..Default::default()
        };
        renderer.prepare(&mut rl, &thread, &scene);
        assert!(
            renderer.gpu.procedural.diagnostics.is_empty(),
            "{:?}",
            renderer.gpu.procedural.diagnostics
        );
        assert_eq!(1, renderer.gpu.procedural.domains.len());
        assert_eq!(2, renderer.gpu.procedural.batches[0].placements.len());
        draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            0,
            &mut renderer,
        );
        let phong = copy_target_to_gpui_image(&target).unwrap();
        assert!(
            phong.pixels().filter(|p| p[2] > 100 && p[1] < 80).count() > 100,
            "texture input must reach the shader (BGRA readback)"
        );
        let uploads = renderer.gpu.procedural.domain_uploads;
        d.put_material(Material {
            id: "pbr".into(),
            name: "PBR".into(),
            pbr: Some(Pbr {
                roughness: 0.15,
                metallic: 0.8,
            }),
            ..Default::default()
        })
        .unwrap();
        material.procedural.as_mut().unwrap().base_material = Some("pbr".into());
        d.put_material(material.clone()).unwrap();
        scene.document = Arc::new(d.clone());
        renderer.prepare(&mut rl, &thread, &scene);
        draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            1,
            &mut renderer,
        );
        assert_ne!(phong, copy_target_to_gpui_image(&target).unwrap());
        assert_eq!(uploads, renderer.gpu.procedural.domain_uploads);
        // Switch to grass and override just one face of one shared instance.
        material.procedural = Some(ProceduralMaterial {
            parameters: BTreeMap::from([("wind".into(), json!(0.)), ("density".into(), json!(2.))]),
            ..Default::default()
        });
        d.put_material(material).unwrap();
        let face = *d
            .effective_geometry(&first)
            .unwrap()
            .triangles
            .keys()
            .next()
            .unwrap();
        selection.apply(
            [SelectionTarget {
                occurrence: first.clone(),
                entity: SelectedEntity::Triangle(face),
            }],
            SelectionMode::Replace,
        );
        assign_procedural_material(
            &mut d,
            &selection,
            "custom",
            &BTreeMap::from([("height".into(), json!(2.))]),
        )
        .unwrap();
        scene.document = Arc::new(d.clone());
        renderer.prepare(&mut rl, &thread, &scene);
        assert!(
            renderer
                .gpu
                .procedural
                .batches
                .iter()
                .any(|b| num(&b.values, "height", 0.) == 2.)
        );
        assert!(
            renderer
                .gpu
                .procedural
                .batches
                .iter()
                .any(|b| num(&b.values, "height", 0.) == 0.8)
        );
        let count = d.effective_geometry(&first).unwrap().triangles.len();
        draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            camera,
            &scene,
            2,
            &mut renderer,
        );
        assert_eq!(12, count);
        assert_eq!(12, d.effective_geometry(&second).unwrap().triangles.len());
        assert!(
            !renderer.gpu.procedural.animated,
            "zero wind does not run a continuous refresh loop"
        );
        let outside = OrbitCamera {
            target: Vec3::new(10000., 10000., 10000.),
            distance: 5.,
            ..camera
        };
        draw_offscreen_scene(
            &mut rl,
            &thread,
            &mut target,
            outside,
            &scene,
            3,
            &mut renderer,
        );
        assert_eq!(
            0, renderer.gpu.procedural.draw_calls,
            "expanded bounds cull offscreen domains"
        );
        std::fs::remove_file(path).unwrap();
    }
}
