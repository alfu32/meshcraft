//! Home screen data and session behaviour: document presets and embedded samples.
use super::*;
#[cfg(not(target_arch = "wasm32"))]
use serde_json::Value;

/// One choice in the Home screen's New section. Units are presentation only:
/// `unit_size` of `unit_name` is displayed for each internal model unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DocumentPreset {
    pub id: &'static str,
    pub title: &'static str,
    pub detail: &'static str,
    pub icon: &'static str,
    pub unit_name: &'static str,
    pub unit_size: f64,
}

pub(crate) const DOCUMENT_PRESETS: [DocumentPreset; 5] = [
    DocumentPreset {
        id: "urban_planning",
        title: "Urban Planning",
        detail: "1 unit = 1 m",
        icon: "surface_rect",
        unit_name: "m",
        unit_size: 1.0,
    },
    DocumentPreset {
        id: "architecture_metric",
        title: "Architecture, Metric",
        detail: "1 unit = 30 cm",
        icon: "arch_wall",
        unit_name: "cm",
        unit_size: 30.0,
    },
    DocumentPreset {
        id: "architecture_imperial",
        title: "Architecture, Imperial",
        detail: "1 unit = 1 ft",
        icon: "arch_wall",
        unit_name: "ft",
        unit_size: 1.0,
    },
    DocumentPreset {
        id: "product_metric",
        title: "Product Design, Metric",
        detail: "1 unit = 1 cm",
        icon: "cuboid",
        unit_name: "cm",
        unit_size: 1.0,
    },
    DocumentPreset {
        id: "product_imperial",
        title: "Product Design, Imperial",
        detail: "1 unit = 1 in",
        icon: "cuboid",
        unit_name: "in",
        unit_size: 1.0,
    },
];

pub(crate) fn document_preset(id: &str) -> Option<&'static DocumentPreset> {
    DOCUMENT_PRESETS.iter().find(|preset| preset.id == id)
}

/// A demonstration project embedded in the executable. Opening one never binds
/// the document to a file, so Save asks for a location and the sample stays intact.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy)]
pub(crate) struct SampleProject {
    pub id: &'static str,
    pub title: &'static str,
    pub detail: &'static str,
    pub project: &'static [u8],
    pub thumbnail: &'static [u8],
}

#[cfg(not(target_arch = "wasm32"))]
macro_rules! sample {
    ($id:literal, $title:literal, $detail:literal) => {
        SampleProject {
            id: $id,
            title: $title,
            detail: $detail,
            project: include_bytes!(concat!("../../../assets/samples/", $id, ".octd.gz")),
            thumbnail: include_bytes!(concat!("../../../assets/samples/", $id, ".png")),
        }
    };
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) const SAMPLE_PROJECTS: [SampleProject; 9] = [
    sample!(
        "instrument",
        "Instrument Controller",
        "Exploded assembly with cover, controls, PCB and fasteners"
    ),
    sample!(
        "kitchen",
        "Fitted Kitchen",
        "Independent configurable cabinet instances and an island"
    ),
    sample!(
        "residence",
        "Courtyard Residence",
        "Two storeys of native walls, slabs, openings and glazing frames"
    ),
    sample!(
        "casework",
        "Scripted Casework",
        "JavaScript cabinets with prototype and instance inputs"
    ),
    sample!(
        "rooftop",
        "Roof Terrace",
        "Procedural brick, wood, water, grass and foliage"
    ),
    sample!(
        "park",
        "Landscape Canopy",
        "GPU-generated trees with editable branching and wind"
    ),
    sample!(
        "loft",
        "Double-Height Loft",
        "Native turning stair, mezzanine and guardrail guides"
    ),
    sample!(
        "plantroom",
        "Twin-Pump Skid",
        "Piping, control cabinet and a spatially swept service rail"
    ),
    sample!(
        "studio",
        "Home Office",
        "Furnished room with dimension annotation"
    ),
];

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn sample_project(id: &str) -> Option<&'static SampleProject> {
    SAMPLE_PROJECTS.iter().find(|sample| sample.id == id)
}

impl MeshcraftApp {
    pub fn native_home_open(&self) -> bool {
        self.home_open
    }

    pub fn native_set_home_open(&mut self, open: bool) {
        self.home_open = open;
    }

    /// The authored viewpoint of a sample that was just opened.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_take_sample_camera(&mut self) -> Option<Value> {
        self.sample_camera.take()
    }

    /// True once, when an import into a new document has finished.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_take_import_fit(&mut self) -> bool {
        if !self.import_fit_pending || self.pending_interchange.is_some() {
            return false;
        }
        self.import_fit_pending = false;
        true
    }

    pub(super) fn set_home_command(&mut self, payload: &CommandPayload) -> CommandResult {
        let open = match payload.values.get("open") {
            Some(value) => match value.as_bool() {
                Some(open) => open,
                None => return CommandResult::failed("open must be a boolean"),
            },
            None => !self.home_open,
        };
        self.home_open = open;
        let mut result = CommandResult::completed();
        result.outputs.insert("open".to_owned(), open.into());
        result
    }

    /// `file.new`: an optional `preset` selects the unit scale of the new document.
    pub(super) fn new_document_command(&mut self, payload: &CommandPayload) -> CommandResult {
        let preset = match payload.values.get("preset") {
            None => None,
            Some(value) => {
                let Some(preset) = value.as_str().and_then(document_preset) else {
                    let known = DOCUMENT_PRESETS.map(|preset| preset.id).join(", ");
                    return CommandResult::failed(format!("preset must be one of: {known}"));
                };
                Some(preset)
            }
        };
        if self.document_is_dirty() {
            return self.request_document_replacement(ReplacementTarget::NewDocument(preset));
        }
        self.start_new_document(preset)
    }

    pub(super) fn apply_document_preset(&mut self, preset: &DocumentPreset) -> Result<(), String> {
        let settings = ModelSettings {
            unit_name: preset.unit_name.to_owned(),
            unit_size: preset.unit_size,
            ..self.document.model_settings().clone()
        };
        self.commit_document_edit("Project units", |document| {
            document.update_model_settings(settings)?;
            Ok(())
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn open_sample_command(&mut self, payload: &CommandPayload) -> CommandResult {
        let Some(id) = payload
            .values
            .get("sample")
            .and_then(|value| value.as_str())
        else {
            return CommandResult::failed("sample must be a string");
        };
        let Some(sample) = sample_project(id) else {
            let known = SAMPLE_PROJECTS.map(|sample| sample.id).join(", ");
            return CommandResult::failed(format!("sample must be one of: {known}"));
        };
        if self.pending_project_load.is_some() {
            return CommandResult::failed("A project is already being opened.");
        }
        if self.pending_project_save.is_some() {
            return CommandResult::failed("Wait for the current project save to finish first.");
        }
        if self.pending_replacement.is_some() {
            return CommandResult::failed("A document replacement is already awaiting a decision.");
        }
        if self.document_is_dirty() {
            return self.request_document_replacement(ReplacementTarget::OpenSample(sample.id));
        }
        self.start_sample_load(sample, None)
    }

    /// Decode on a worker like a file load; the document stays unbound from any path.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn start_sample_load(
        &mut self,
        sample: &'static SampleProject,
        job_id: Option<String>,
    ) -> CommandResult {
        if self.pending_project_load.is_some() {
            return CommandResult::failed("A project is already being opened.");
        }
        if self.pending_project_save.is_some() {
            return CommandResult::failed("Wait for the current project save to finish first.");
        }
        let job_id = job_id.unwrap_or_else(|| {
            let job_id = format!("document-load-{}", self.next_job_id);
            self.next_job_id = self.next_job_id.saturating_add(1);
            job_id
        });
        let input_revision = self.document.revision();
        let cancellation = Arc::new(AtomicBool::new(false));
        let worker_cancellation = Arc::clone(&cancellation);
        let (sender, receiver) = mpsc::channel();
        let spawned = thread::Builder::new().name(job_id.clone()).spawn(move || {
            let loaded = read_project_with(
                sample.project,
                MAX_PROJECT_BYTES as usize,
                |progress| {
                    let _ = sender.send(ProjectLoadEvent::Progress(progress));
                },
                || worker_cancellation.load(Ordering::Acquire),
            )
            .map_err(|error| error.to_string());
            let _ = sender.send(ProjectLoadEvent::Finished(Box::new(loaded)));
        });
        if let Err(error) = spawned {
            return CommandResult::failed(format!("could not start project loader: {error}"));
        }
        self.status = format!("Opening sample {}…", sample.title);
        self.pending_project_load = Some(PendingProjectLoad {
            job_id: job_id.clone(),
            path: PathBuf::from(sample.title),
            sample: true,
            input_revision,
            cancellation,
            receiver,
        });
        let result = CommandResult::pending(
            job_id.clone(),
            HostEffect::StartJob {
                job_id: job_id.clone(),
                kind: "document.load".to_owned(),
                input_revision: Some(input_revision),
            },
        );
        self.job_results.insert(job_id, result.clone());
        result
    }

    /// `import.mesh` with `new_document`: replace the document, then import into it.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn import_into_new_document(&mut self, path: PathBuf) -> CommandResult {
        if self.document_is_dirty() {
            return self.request_document_replacement(ReplacementTarget::ImportMesh(path));
        }
        self.start_import_document(path)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn start_import_document(&mut self, path: PathBuf) -> CommandResult {
        let result = self.start_new_document(None);
        if result.completion == CommandCompletion::Failed {
            return result;
        }
        self.import_fit_pending = true;
        let payload = CommandPayload {
            values: BTreeMap::from([(
                "path".to_owned(),
                Value::String(path.display().to_string()),
            )]),
            ..CommandPayload::default()
        };
        self.dispatch_command("import.mesh", &payload).result
    }
}

/// Where a Recent project's preview is kept, beside the native preferences.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn recent_thumbnail_path(project: &str) -> Option<PathBuf> {
    // FNV-1a: stable across releases, unlike the standard library's hasher.
    let hash = project
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    Some(
        native_preferences_path()?
            .parent()?
            .join("recent-thumbnails")
            .join(format!("{hash:016x}.png")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_define_the_requested_unit_scales() {
        let expected = [
            ("urban_planning", "1 m"),
            ("architecture_metric", "30 cm"),
            ("architecture_imperial", "1 ft"),
            ("product_metric", "1 cm"),
            ("product_imperial", "1 in"),
        ];
        for (id, one_unit) in expected {
            let mut app = super::super::tests::test_app();
            let payload = CommandPayload {
                values: BTreeMap::from([("preset".to_owned(), Value::from(id))]),
                ..CommandPayload::default()
            };
            let dispatch = app.dispatch_command("file.new", &payload);
            assert!(dispatch.accepted, "{id}: {:?}", dispatch.result.diagnostics);
            let settings = app.document.model_settings();
            assert_eq!(one_unit, settings.format_length(1.0), "{id}");
            settings.validate().unwrap();
            assert!(!app.document_is_dirty(), "{id}: a new project starts clean");
        }
    }

    #[test]
    fn plain_new_document_keeps_generic_units_and_unknown_presets_fail() {
        let mut app = super::super::tests::test_app();
        app.dispatch_command("file.new", &CommandPayload::default());
        assert_eq!("1 unit", app.document.model_settings().format_length(1.0));
        let payload = CommandPayload {
            values: BTreeMap::from([("preset".to_owned(), Value::from("nautical"))]),
            ..CommandPayload::default()
        };
        let dispatch = app.dispatch_command("file.new", &payload);
        assert_eq!(CommandCompletion::Failed, dispatch.result.completion);
    }

    #[test]
    fn creating_or_opening_a_document_closes_home_and_the_command_toggles_it() {
        let mut app = super::super::tests::test_app();
        assert!(!app.native_home_open());
        app.dispatch_command("file.home", &CommandPayload::default());
        assert!(app.native_home_open());
        assert_eq!(Some(true), app.native_command_state("file.home").checked);
        app.dispatch_command("file.new", &CommandPayload::default());
        assert!(!app.native_home_open());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn every_sample_decodes_with_geometry_and_a_thumbnail() {
        for sample in &SAMPLE_PROJECTS {
            let project =
                read_project_with(sample.project, MAX_PROJECT_BYTES as usize, |_| {}, || false)
                    .unwrap_or_else(|error| panic!("{}: {error}", sample.id));
            let mut app = super::super::tests::test_app();
            app.document = project.document;
            assert!(
                !app.top_level_occurrences().is_empty(),
                "{} has no content",
                sample.id
            );
            assert!(
                image::load_from_memory(sample.thumbnail).is_ok(),
                "{}",
                sample.id
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn opened_sample_is_unbound_and_stays_out_of_recents() {
        let mut app = super::super::tests::test_app();
        let payload = CommandPayload {
            values: BTreeMap::from([("sample".to_owned(), Value::from("casework"))]),
            ..CommandPayload::default()
        };
        app.native_set_home_open(true);
        let dispatch = app.dispatch_command("file.open_sample", &payload);
        assert!(matches!(
            dispatch.result.completion,
            CommandCompletion::Pending { .. }
        ));
        let deadline = Instant::now() + Duration::from_secs(30);
        while app.pending_project_load.is_some() && Instant::now() < deadline {
            app.poll_project_load();
            thread::sleep(Duration::from_millis(5));
        }
        assert!(app.pending_project_load.is_none());
        assert!(app.current_project_path().is_none());
        assert!(app.native_recent_projects().is_empty());
        assert!(!app.document_is_dirty());
        assert!(!app.native_home_open());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn dirty_documents_ask_before_a_preset_or_sample_replaces_them() {
        let mut app = super::super::tests::test_app();
        app.document
            .add_cuboid("edit", Vec3::new(1.0, 1.0, 1.0), Vec3::ZERO)
            .unwrap();
        assert!(app.document_is_dirty());
        for (command, key, value) in [
            ("file.new", "preset", "product_metric"),
            ("file.open_sample", "sample", "casework"),
        ] {
            let payload = CommandPayload {
                values: BTreeMap::from([(key.to_owned(), Value::from(value))]),
                ..CommandPayload::default()
            };
            app.dispatch_command(command, &payload);
            assert!(
                app.native_document_replacement_prompt().is_some(),
                "{command}"
            );
            app.native_resolve_document_replacement(ReplacementDecision::Cancel);
            assert!(app.document_is_dirty());
        }
        let payload = CommandPayload {
            values: BTreeMap::from([("preset".to_owned(), Value::from("product_metric"))]),
            ..CommandPayload::default()
        };
        app.dispatch_command("file.new", &payload);
        app.native_resolve_document_replacement(ReplacementDecision::Discard);
        assert_eq!("1 cm", app.document.model_settings().format_length(1.0));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn saving_to_a_new_location_joins_recent_once() {
        let directory = std::env::temp_dir().join(format!("meshcraft-home-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("saved.octd");
        let mut app = super::super::tests::test_app();
        let payload = CommandPayload {
            values: BTreeMap::from([("path".to_owned(), Value::from(path.display().to_string()))]),
            ..CommandPayload::default()
        };
        for _ in 0..2 {
            app.dispatch_command("file.save_as", &payload);
            let deadline = Instant::now() + Duration::from_secs(30);
            while app.pending_project_save.is_some() && Instant::now() < deadline {
                app.poll_project_save();
                thread::sleep(Duration::from_millis(5));
            }
        }
        assert_eq!(1, app.native_recent_projects().len());
        assert!(app.native_recent_projects()[0].ends_with("saved.octd"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recent_thumbnail_names_are_stable() {
        let one = recent_thumbnail_path("/projects/a.octd");
        assert_eq!(one, recent_thumbnail_path("/projects/a.octd"));
        assert_ne!(one, recent_thumbnail_path("/projects/b.octd"));
    }
}
