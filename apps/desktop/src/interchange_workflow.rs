//! Worker-backed interchange. Geometry is immutable in workers; only the UI commits imports
//! or publishes a completed temporary export. Cancel/failure never replaces a good output.
use super::*;
use meshcraft_io::mesh;
#[cfg(not(target_arch = "wasm32"))]
use meshcraft_io::mesh::{Format, Mesh, Options, Unit};

impl MeshcraftApp {
    pub(super) fn execute_interchange(
        &mut self,
        action: &str,
        payload: &CommandPayload,
    ) -> CommandResult {
        if action == "formats" {
            self.status="Import/export: OBJ, STL, ASCII FBX, glTF/GLB, Collada, ASCII DXF, 3MF, AMF and IFC4 proxy geometry. See docs/interchange.md for subsets and units.".into();
            let mut r = CommandResult::completed();
            r.outputs
                .insert("extensions".into(), json!(mesh::EXTENSIONS));
            r.outputs.insert(
                "guide".into(),
                json!(include_str!("../../../docs/interchange.md")),
            );
            return r;
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = payload;
            CommandResult::failed("mesh file dialogs and workers require the desktop host")
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.start_interchange(action, payload)
                .unwrap_or_else(CommandResult::failed)
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct StagedExport {
    path: PathBuf,
    triangles: usize,
    segments: usize,
    warnings: Vec<String>,
}
#[cfg(not(target_arch = "wasm32"))]
impl Drop for StagedExport {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
#[cfg(not(target_arch = "wasm32"))]
enum Outcome {
    Imported {
        source: Box<Document>,
        source_origin: Vec3,
        triangles: usize,
        segments: usize,
        warnings: Vec<String>,
    },
    Exported(StagedExport),
}
#[cfg(not(target_arch = "wasm32"))]
pub(super) struct InterchangeJob {
    id: String,
    path: PathBuf,
    generation: u64,
    revision: u64,
    context: EditContext,
    position: Option<Vec3>,
    cancel: Arc<AtomicBool>,
    receiver: Receiver<Result<Outcome, String>>,
}
#[cfg(not(target_arch = "wasm32"))]
impl Drop for InterchangeJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn stage_export(
    path: &Path,
    bytes: &[u8],
    id: &str,
    triangles: usize,
    segments: usize,
    warnings: Vec<String>,
    cancel: &AtomicBool,
) -> Result<StagedExport, String> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let filename = path
        .file_name()
        .ok_or("export needs a filename")?
        .to_string_lossy();
    let temp = parent.join(format!(
        ".{filename}.meshcraft-{}-{id}.tmp",
        std::process::id()
    ));
    let result;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| format!("cannot create export: {e}"))?;
    result = StagedExport {
        path: temp,
        triangles,
        segments,
        warnings,
    };
    for chunk in bytes.chunks(1024 * 1024) {
        mesh_check(cancel)?;
        file.write_all(chunk)
            .map_err(|e| format!("cannot write export: {e}"))?;
    }
    file.sync_all()
        .map_err(|e| format!("cannot flush export: {e}"))?;
    mesh_check(cancel)?;
    Ok(result)
}
#[cfg(not(target_arch = "wasm32"))]
fn mesh_check(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("interchange canceled".into())
    } else {
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl MeshcraftApp {
    fn start_interchange(
        &mut self,
        action: &str,
        payload: &CommandPayload,
    ) -> Result<CommandResult, String> {
        if action == "cancel" {
            let job = self
                .pending_interchange
                .as_ref()
                .ok_or("no import/export is running")?;
            job.cancel.store(true, Ordering::Release);
            self.status = "Cancelling import/export…".into();
            let mut r = CommandResult::completed();
            r.host_effects.push(HostEffect::CancelJob {
                job_id: job.id.clone(),
            });
            return Ok(r);
        }
        if self.pending_interchange.is_some() {
            return Err(
                "an import/export is already running; wait or use interchange.cancel".into(),
            );
        }
        if self.pending_replacement.is_some() || self.pending_project_load.is_some() {
            return Err("finish opening/replacing the current document first".into());
        }
        let import = action == "import";
        let ifc = action == "ifc";
        let path = if let Some(v) = payload.values.get("path") {
            PathBuf::from(v.as_str().ok_or("path must be a string")?)
        } else {
            let mut dialog = rfd::FileDialog::new();
            if ifc {
                dialog = dialog.add_filter("IFC4 geometry", &["ifc"]);
            } else {
                for (label, extensions) in [
                    ("Wavefront OBJ", vec!["obj"]),
                    ("STL binary", vec!["stl", "stlb"]),
                    ("STL ASCII", vec!["stla"]),
                    ("ASCII FBX", vec!["fbx"]),
                    ("glTF / GLB", vec!["gltf", "glb"]),
                    ("Collada", vec!["dae"]),
                    ("ASCII DXF", vec!["dxf"]),
                    ("3MF", vec!["3mf"]),
                    ("AMF", vec!["amf"]),
                    ("IFC geometry", vec!["ifc"]),
                ] {
                    dialog = dialog.add_filter(label, &extensions);
                }
                if import {
                    dialog = dialog.add_filter("All supported mesh formats", mesh::EXTENSIONS);
                }
            }
            let path = if import {
                dialog.pick_file()
            } else {
                dialog
                    .set_file_name(if ifc { "model.ifc" } else { "model.obj" })
                    .save_file()
            };
            let Some(path) = path else {
                return Ok(CommandResult::completed());
            };
            path
        };
        if path.as_os_str().is_empty() {
            return Err("path cannot be empty".into());
        }
        let path = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(path)
        };
        let format = Format::from_path(&path)?;
        if ifc && format != Format::Ifc {
            return Err("IFC export requires a .ifc filename".into());
        }
        let unit=payload.values.get("unit").map(|v|Unit::parse(v.as_str().unwrap_or("")).ok_or("unknown physical unit; use micron, millimeter, centimeter, meter, inch or foot")).transpose()?;
        let scale = payload
            .values
            .get("scale")
            .map(|v| {
                v.as_f64()
                    .filter(|n| n.is_finite() && *n > 0.)
                    .ok_or("scale must be positive and finite")
            })
            .transpose()?
            .unwrap_or(1.);
        let position = payload
            .values
            .get("position")
            .map(|v| {
                let p = v
                    .as_array()
                    .filter(|p| p.len() == 3)
                    .ok_or("position needs [x,y,z]")?;
                let mut n = [0.; 3];
                for i in 0..3 {
                    n[i] = p[i]
                        .as_f64()
                        .filter(|v| v.is_finite())
                        .ok_or("position must be finite")?;
                }
                Ok::<_, String>(Vec3::new(n[0], n[1], n[2]))
            })
            .transpose()?;
        let options = Options {
            meters_per_model_unit: self.document.model_settings().unit_size,
            unit: unit.or_else(|| {
                if import {
                    None
                } else {
                    Unit::from_filename(&path)
                }
            }),
            scale,
        };
        let selection_only = payload
            .values
            .get("selection_only")
            .map(|v| v.as_bool().ok_or("selection_only must be boolean"))
            .transpose()?
            .unwrap_or(!self.selection.selected().is_empty());
        if !import && selection_only && self.selection.selected().is_empty() {
            return Err("selection is empty; use selection_only:false to export the model".into());
        }
        let document = (!import).then(|| self.document.clone());
        let selection = selection_only.then(|| self.selection.clone());
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let worker_path = path.clone();
        let id = format!("interchange-{}", self.next_job_id);
        self.next_job_id += 1;
        let worker_id = id.clone();
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name(id.clone())
            .spawn(move || {
                let result = (|| {
                    if import {
                        let mut mesh = mesh::read_file(&worker_path, &options, &worker_cancel)?;
                        let source_origin = meshcraft_core::geometry::Aabb3::from_points(
                            mesh.triangles
                                .iter()
                                .flat_map(|(p, _)| p)
                                .chain(mesh.segments.iter().flat_map(|(p, _)| p))
                                .copied(),
                        )
                        .ok_or("import has no bounds")?
                        .min;
                        mesh.transform(Affine3::from_translation(-source_origin));
                        let triangles = mesh.triangles.len();
                        let segments = mesh.segments.len();
                        let warnings = mesh.warnings.clone();
                        let source = Box::new(mesh.into_document()?);
                        mesh_check(&worker_cancel)?;
                        Ok(Outcome::Imported {
                            source,
                            source_origin,
                            triangles,
                            segments,
                            warnings,
                        })
                    } else {
                        let mesh = Mesh::from_document(
                            document.as_ref().unwrap(),
                            selection.as_ref(),
                            &worker_cancel,
                        )?;
                        let triangles = mesh.triangles.len();
                        let segments = mesh.segments.len();
                        let (bytes, warnings) =
                            mesh::encode(&mesh, format, &options, &worker_cancel)?;
                        let staged = stage_export(
                            &worker_path,
                            &bytes,
                            &worker_id,
                            triangles,
                            if matches!(
                                format,
                                Format::Stl
                                    | Format::StlAscii
                                    | Format::StlBinary
                                    | Format::Fbx
                                    | Format::ThreeMf
                                    | Format::Amf
                            ) {
                                0
                            } else {
                                segments
                            },
                            warnings,
                            &worker_cancel,
                        )?;
                        Ok(Outcome::Exported(staged))
                    }
                })();
                let _ = sender.send(result);
            })
            .map_err(|e| format!("cannot start import/export worker: {e}"))?;
        let revision = self.document.revision();
        self.pending_interchange = Some(InterchangeJob {
            id: id.clone(),
            path: path.clone(),
            generation: self.document_generation,
            revision,
            context: self.selection.context.clone(),
            position,
            cancel,
            receiver,
        });
        self.status = format!(
            "{} {}… Use File > Cancel Import / Export to stop.",
            if import {
                "Importing"
            } else if selection_only {
                "Exporting selection to"
            } else {
                "Exporting model to"
            },
            path.display()
        );
        let result = CommandResult::pending(
            id.clone(),
            HostEffect::StartJob {
                job_id: id.clone(),
                kind: if import { "import.mesh" } else { "export.mesh" }.into(),
                input_revision: Some(revision),
            },
        );
        self.job_results.insert(id, result.clone());
        Ok(result)
    }
    pub(super) fn poll_interchange(&mut self) {
        let outcome = match self
            .pending_interchange
            .as_ref()
            .map(|j| j.receiver.try_recv())
        {
            Some(Ok(r)) => r,
            Some(Err(TryRecvError::Disconnected)) => {
                Err("import/export worker stopped unexpectedly".into())
            }
            _ => return,
        };
        let job = self.pending_interchange.take().unwrap();
        let result = (|| -> Result<CommandResult, String> {
            mesh_check(&job.cancel)?;
            let mut result = CommandResult::completed();
            let (triangles, segments, warnings) = match outcome? {
                Outcome::Imported { source, source_origin, triangles, segments, warnings } => {
                    if job.generation != self.document_generation
                        || job.revision != self.document.revision()
                        || job.context != self.selection.context
                    {
                        return Err("import canceled because the document or editing context changed; retry the import".into());
                    }
                    let name = job.path.file_stem().unwrap_or_default().to_string_lossy();
                    let context = job.context.clone();
                    let (definition, placed) = self.commit_document_edit("Import mesh prototype", |d| {
                        let id = d.import_subscene(&source, &name, None, None)?;
                        let placed = job.position.map(|p| modeling_objects::place_definition(d, &context, id, p, None, false)).transpose()?;
                        Ok((id, placed))
                    })?;
                    self.object_browser_selection = Some(definition);
                    self.selection.clear();
                    self.panel_focus = Some("objects".into());
                    if let Some(path) = placed {
                        let target = SelectionTarget { occurrence: path, entity: SelectedEntity::Occurrence };
                        self.selection.select(target.clone());
                        result.outputs.insert("target".into(), mcp_workflow::target_json(&target));
                    } else {
                        self.begin_placing_definition(definition);
                    }
                    result.document_revision = Some(self.document.revision());
                    result.outputs.insert("definition_id".into(), json!(definition.0));
                    result.outputs.insert("awaiting_placement".into(), json!(job.position.is_none()));
                    result.outputs.insert("source_origin".into(), json!(source_origin));
                    (triangles, segments, warnings)
                }
                Outcome::Exported(staged) => {
                    mesh_check(&job.cancel)?;
                    fs::rename(&staged.path, &job.path).map_err(|e| format!("cannot publish export; existing file was retained: {e}"))?;
                    (staged.triangles, staged.segments, staged.warnings.clone())
                }
            };
            let imported = result.outputs.contains_key("definition_id");
            let placement_hint = if imported && job.position.is_none() { "Click to place the object. " } else { "" };
            self.status = format!("{} {}: {triangles} faces, {segments} lines. {placement_hint}{}", if imported { "Imported" } else { "Exported" }, job.path.display(), warnings.join(" "));
            result.outputs.insert("path".into(), json!(job.path));
            result.outputs.insert("triangles".into(), json!(triangles));
            result.outputs.insert("segments".into(), json!(segments));
            result.outputs.insert("warnings".into(), json!(warnings));
            Ok(result)
        })().unwrap_or_else(|e| {
            self.status = format!("Import/export failed: {e}");
            CommandResult::failed(e)
        });
        self.job_results.insert(job.id.clone(), result);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    fn path(label: &str, extension: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "meshcraft-interchange-{}-{label}.{extension}",
            std::process::id()
        ))
    }
    fn payload(path: &Path) -> CommandPayload {
        CommandPayload {
            values: BTreeMap::from([("path".into(), json!(path))]),
            ..Default::default()
        }
    }
    fn assert_same_authored(a: &Document, b: &Document) {
        let mut a = serde_json::to_value(a).unwrap();
        let mut b = serde_json::to_value(b).unwrap();
        a.as_object_mut().unwrap().remove("revision");
        b.as_object_mut().unwrap().remove("revision");
        assert_eq!(a, b);
    }
    fn wait(app: &mut MeshcraftApp, id: &str) -> CommandResult {
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.pending_interchange.is_some() {
            app.poll_interchange();
            assert!(Instant::now() < deadline, "interchange timed out");
            thread::sleep(Duration::from_millis(1));
        }
        app.job_results[id].clone()
    }
    fn start(app: &mut MeshcraftApp, action: &str, payload: &CommandPayload) -> String {
        let r = app.execute_interchange(action, payload);
        let CommandCompletion::Pending { job_id } = r.completion else {
            panic!("{r:?}")
        };
        job_id
    }
    #[test]
    fn imported_mesh_starts_wireframe_placement_and_prototype_creation_is_undoable() {
        let mut app = super::super::tests::test_app();
        let file = path("placement", "obj");
        fs::write(&file, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let before = app.document.clone();
        let job = start(&mut app, "import", &payload(&file));
        let result = wait(&mut app, &job);
        assert_eq!(result.completion, CommandCompletion::Completed);
        assert_eq!(result.outputs["triangles"], 1);
        assert_eq!(result.outputs["awaiting_placement"], true);
        assert_eq!(app.active_tool, "Object Place");
        assert!(
            app.document
                .definition(DefinitionId(
                    result.outputs["definition_id"].as_u64().unwrap()
                ))
                .is_some()
        );
        assert_eq!(app.history.entries().len(), 1);
        app.history.undo(&mut app.document).unwrap();
        assert_same_authored(&app.document, &before);
        fs::remove_file(file).unwrap();
    }
    #[test]
    fn import_with_position_inserts_one_instance_in_a_single_transaction() {
        let mut app = super::super::tests::test_app();
        let file = path("position", "obj");
        fs::write(&file, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let before = app.document.clone();
        let mut args = payload(&file);
        args.values.insert("position".into(), json!([5, 6, 7]));
        let job = start(&mut app, "import", &args);
        let r = wait(&mut app, &job);
        assert_eq!(r.completion, CommandCompletion::Completed, "{r:?}");
        assert_eq!(r.outputs["awaiting_placement"], false);
        assert_eq!(app.history.entries().len(), 1);
        let m = Mesh::from_document(&app.document, Some(&app.selection), &AtomicBool::new(false))
            .unwrap();
        assert_eq!(m.triangles.len(), 1);
        assert_eq!(m.triangles[0].0[0], Vec3::new(5., 6., 7.));
        app.history.undo(&mut app.document).unwrap();
        assert_same_authored(&app.document, &before);
        fs::remove_file(file).unwrap();
    }
    #[test]
    fn stale_and_canceled_imports_leave_document_and_history_untouched() {
        for stale in [false, true] {
            let mut app = super::super::tests::test_app();
            let file = path(if stale { "stale" } else { "cancel" }, "obj");
            fs::write(&file, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3").unwrap();
            let job = start(&mut app, "import", &payload(&file));
            if stale {
                app.document
                    .add_cuboid("other", Vec3::new(1., 1., 1.), Vec3::ZERO)
                    .unwrap();
            } else {
                app.execute_interchange("cancel", &CommandPayload::default());
            }
            let before = app.document.clone();
            let r = wait(&mut app, &job);
            assert_eq!(r.completion, CommandCompletion::Failed);
            assert_same_authored(&app.document, &before);
            assert!(app.history.entries().is_empty());
            fs::remove_file(file).unwrap();
        }
    }
    #[test]
    fn canceled_and_failed_exports_preserve_existing_destination_and_remove_temps() {
        for canceled in [false, true] {
            let mut app = super::super::tests::test_app();
            app.document
                .add_cuboid("box", Vec3::new(1., 1., 1.), Vec3::ZERO)
                .unwrap();
            let file = path(
                if canceled {
                    "cancel-output"
                } else {
                    "failure-output"
                },
                "amf",
            );
            fs::write(&file, b"previous good output").unwrap();
            let mut args = payload(&file);
            if !canceled {
                args.values.insert("unit".into(), json!("centimeter"));
            }
            let job = start(&mut app, "export", &args);
            if canceled {
                app.execute_interchange("cancel", &CommandPayload::default());
            }
            assert_eq!(wait(&mut app, &job).completion, CommandCompletion::Failed);
            assert_eq!(fs::read(&file).unwrap(), b"previous good output");
            let prefix = format!(
                ".{}.meshcraft-",
                file.file_name().unwrap().to_string_lossy()
            );
            assert!(!fs::read_dir(file.parent().unwrap()).unwrap().any(|e| {
                e.unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&prefix)
            }));
            fs::remove_file(file).unwrap();
        }
    }
    #[test]
    fn export_uses_frozen_dispatch_snapshot_and_replaces_output_without_editing_history() {
        let mut app = super::super::tests::test_app();
        app.document
            .add_cuboid("box", Vec3::new(1., 1., 1.), Vec3::ZERO)
            .unwrap();
        let file = path("snapshot", "glb");
        fs::write(&file, b"old").unwrap();
        let job = start(&mut app, "export", &payload(&file));
        app.document
            .add_cuboid("later", Vec3::new(1., 1., 1.), Vec3::X * 10.)
            .unwrap();
        let current = app.document.clone();
        let result = wait(&mut app, &job);
        assert_eq!(
            result.completion,
            CommandCompletion::Completed,
            "{result:?}"
        );
        assert_eq!(result.outputs["triangles"], 12);
        let m = mesh::read_file(&file, &Options::default(), &AtomicBool::new(false)).unwrap();
        assert_eq!(m.triangles.len(), 12);
        assert_eq!(app.document, current);
        assert!(app.history.entries().is_empty());
        fs::remove_file(file).unwrap();
    }
    #[test]
    fn command_catalog_exposes_interchange_schemas_and_canonical_files_menu() {
        let commands = MeshcraftApp::command_catalog();
        for id in ["import.mesh", "export.mesh", "export.ifc_model"] {
            let c = commands.iter().find(|c| c.id.as_str() == id).unwrap();
            assert_eq!(c.capability, CommandCapability::Desktop);
            assert!(c.parameters.iter().any(|p| p.name == "path"));
            assert_eq!(c.presentation.menu_path, vec!["Files", "Import & Export"]);
            assert!(c.presentation.menu_aliases.is_empty());
        }
        let mut app = super::super::tests::test_app();
        let r = app.execute_interchange("formats", &CommandPayload::default());
        assert!(
            r.outputs["guide"]
                .as_str()
                .unwrap()
                .contains("Supported subsets")
        );
    }
}
