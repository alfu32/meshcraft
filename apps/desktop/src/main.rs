use eframe::egui;
use meshcraft_core::{document::Document, legacy::import_current_json};
use meshcraft_desktop::MeshcraftApp;
use std::{env, fs, path::PathBuf};

fn main() -> eframe::Result {
    let startup_document = load_startup_document();
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_maximized(true)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Meshcraft",
        options,
        Box::new(move |creation_context| {
            let app = match startup_document {
                Ok(Some((document, status))) => {
                    MeshcraftApp::new_with_document(creation_context, document, status)
                }
                Ok(None) => MeshcraftApp::new(creation_context),
                Err(error) => MeshcraftApp::new_with_error(creation_context, error),
            };
            Ok(Box::new(app))
        }),
    )
}

fn load_startup_document() -> Result<Option<(Document, String)>, String> {
    let mut args = env::args_os().skip(1);
    let Some(first) = args.next() else {
        return Ok(None);
    };
    let path = if first == "--open" {
        args.next()
            .map(PathBuf::from)
            .ok_or_else(|| "--open requires a document path".to_owned())?
    } else {
        PathBuf::from(first)
    };
    let source = fs::read_to_string(&path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    let document = import_current_json(&source)
        .map_err(|error| format!("Could not import {}: {error}", path.display()))?;
    let occurrences = document
        .occurrence_refs()
        .map_err(|error| format!("Could not count imported document occurrences: {error}"))?;
    let is_root_wrapper = occurrences
        .iter()
        .find(|item| item.path.len() == 1)
        .and_then(|root| document.resolve_occurrence(root))
        .is_some_and(|resolved| resolved.definition.name == "Root");
    let top_level_depth = if is_root_wrapper { 2 } else { 1 };
    let top_level_count = occurrences
        .iter()
        .filter(|item| item.path.len() == top_level_depth)
        .count();
    Ok(Some((
        document,
        format!(
            "Opened {} · {top_level_count} top-level objects imported read-only.",
            path.display()
        ),
    )))
}
