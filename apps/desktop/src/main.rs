#[cfg(not(target_arch = "wasm32"))]
use meshcraft_core::document::Document;
#[cfg(not(target_arch = "wasm32"))]
use meshcraft_desktop::{MeshcraftApp, bevy_app::run_native};
use meshcraft_io::project::{ImportedProject, read_project};
use std::{env, ffi::OsString, fs, path::PathBuf};

const DEFAULT_WINDOW_SIZE: [f32; 2] = [1280.0, 800.0];
const MIN_WINDOW_SIZE: [u32; 2] = [900, 600];
const MAX_WINDOW_SIZE: u32 = 32_768;
const USAGE: &str = "Usage: meshcraft [--open] <project.octd|project.k3d> [options]
       meshcraft [options]

Options:
  --open <path>        Open a project at startup (a positional path also works)
  --size <WIDTHxHEIGHT> Open a window at this logical size (implies --windowed)
  --windowed           Start in a window using the default size or --size
  --maximized          Start maximized (the default)
  --version            Print the application version
  --help               Print this help

The app starts maximized by default. Window sizes are logical points; the minimum is 900x600.";

#[derive(Clone, Debug, PartialEq)]
struct StartupOptions {
    document: Option<PathBuf>,
    window_size: [f32; 2],
    maximized: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum StartupCommand {
    Run(StartupOptions),
    Help,
    Version,
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let startup = match parse_startup_args(env::args_os().skip(1)) {
        Ok(StartupCommand::Run(options)) => options,
        Ok(StartupCommand::Help) => {
            println!("{USAGE}");
            return;
        }
        Ok(StartupCommand::Version) => {
            println!("Meshcraft {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Err(error) => {
            eprintln!("{error}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    let startup_path = startup.document.clone();
    let startup_document = load_startup_document(startup.document);
    let (document, status, legacy_history, loaded_path) = match startup_document {
        Ok(Some((project, status))) => (project.document, status, project.history, startup_path),
        Ok(None) => (
            Document::new(),
            "Ready · empty document".to_owned(),
            None,
            None,
        ),
        Err(error) => (Document::new(), error, None, None),
    };
    let mut session = MeshcraftApp::new_native_session(document, status, legacy_history);
    if let Some(path) = loaded_path {
        session.set_native_project_path(path);
    }
    run_native(startup.window_size, startup.maximized, session);
}

#[cfg(target_arch = "wasm32")]
fn main() {}

fn parse_startup_args(args: impl IntoIterator<Item = OsString>) -> Result<StartupCommand, String> {
    let mut args = args.into_iter().peekable();
    let mut document = None;
    let mut window_size = None;
    let mut requested_maximized = None;

    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--help" | "-h") => return Ok(StartupCommand::Help),
            Some("--version" | "-V") => return Ok(StartupCommand::Version),
            Some("--open" | "--size") => {
                let option = argument.to_string_lossy().into_owned();
                let value = args
                    .next()
                    .ok_or_else(|| format!("{option} requires a value"))?;
                if value.to_string_lossy().starts_with('-') {
                    return Err(format!("{option} requires a value before the next option"));
                }
                if argument == "--open" {
                    if document.is_some() {
                        return Err("provide only one project path".to_owned());
                    }
                    document = Some(PathBuf::from(value));
                } else {
                    if window_size.is_some() {
                        return Err("provide --size only once".to_owned());
                    }
                    window_size = Some(parse_window_size(&value)?);
                }
            }
            Some("--windowed") => {
                if requested_maximized == Some(true) {
                    return Err("--windowed and --maximized cannot be combined".to_owned());
                }
                requested_maximized = Some(false);
            }
            Some("--maximized") => {
                if requested_maximized == Some(false) {
                    return Err("--windowed and --maximized cannot be combined".to_owned());
                }
                requested_maximized = Some(true);
            }
            Some("--") => {
                for path in args {
                    if document.replace(PathBuf::from(path)).is_some() {
                        return Err("provide only one project path".to_owned());
                    }
                }
                break;
            }
            Some(value) if value.starts_with('-') => {
                return Err(format!("unknown option `{value}`"));
            }
            _ => {
                if document.replace(PathBuf::from(argument)).is_some() {
                    return Err("provide only one project path".to_owned());
                }
            }
        }
    }

    if window_size.is_some() && requested_maximized == Some(true) {
        return Err("--size cannot be combined with --maximized".to_owned());
    }
    let has_custom_window_size = window_size.is_some();
    let window_size = window_size.unwrap_or(DEFAULT_WINDOW_SIZE);
    let maximized = requested_maximized.unwrap_or(!has_custom_window_size);
    Ok(StartupCommand::Run(StartupOptions {
        document,
        window_size,
        maximized,
    }))
}

fn parse_window_size(value: &OsString) -> Result<[f32; 2], String> {
    let value = value
        .to_str()
        .ok_or_else(|| "--size must use WIDTHxHEIGHT digits".to_owned())?;
    let (width, height) = value
        .split_once(['x', 'X'])
        .ok_or_else(|| "--size must use WIDTHxHEIGHT (for example 1600x900)".to_owned())?;
    if width.is_empty() || height.is_empty() || height.contains(['x', 'X']) {
        return Err("--size must use WIDTHxHEIGHT (for example 1600x900)".to_owned());
    }
    let width = width
        .parse::<u32>()
        .map_err(|_| "--size width must be an unsigned integer".to_owned())?;
    let height = height
        .parse::<u32>()
        .map_err(|_| "--size height must be an unsigned integer".to_owned())?;
    if width < MIN_WINDOW_SIZE[0] || height < MIN_WINDOW_SIZE[1] {
        return Err(format!(
            "--size must be at least {}x{} logical points",
            MIN_WINDOW_SIZE[0], MIN_WINDOW_SIZE[1]
        ));
    }
    if width > MAX_WINDOW_SIZE || height > MAX_WINDOW_SIZE {
        return Err(format!(
            "--size dimensions cannot exceed {MAX_WINDOW_SIZE} logical points"
        ));
    }
    Ok([width as f32, height as f32])
}

fn load_startup_document(
    path: Option<PathBuf>,
) -> Result<Option<(ImportedProject, String)>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let source =
        fs::read(&path).map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    let imported = read_project(&source)
        .map_err(|error| format!("Could not import {}: {error}", path.display()))?;
    let document = &imported.document;
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
    let history_status = imported
        .history
        .as_ref()
        .map_or_else(String::new, |history| {
            format!(
                " · {} legacy history entries retained",
                history.entries.len()
            )
        });
    let repair_status = if imported.warnings.is_empty() {
        String::new()
    } else {
        format!(" · Import repair: {}", imported.warnings.join("; "))
    };
    Ok(Some((
        imported,
        format!(
            "Opened {} · {top_level_count} top-level objects imported read-only{history_status}{repair_status}.",
            path.display(),
        ),
    )))
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_WINDOW_SIZE, StartupCommand, StartupOptions, parse_startup_args};
    use std::{ffi::OsString, path::PathBuf};

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn startup_defaults_to_maximized_with_a_valid_empty_document() {
        assert_eq!(
            StartupCommand::Run(StartupOptions {
                document: None,
                window_size: DEFAULT_WINDOW_SIZE,
                maximized: true,
            }),
            parse_startup_args([]).unwrap()
        );
    }

    #[test]
    fn startup_accepts_a_positional_or_named_project_path() {
        let positional = parse_startup_args(args(&["design.octd"])).unwrap();
        let named = parse_startup_args(args(&["--open", "design.octd"])).unwrap();
        assert_eq!(positional, named);
        assert_eq!(
            Some(PathBuf::from("design.octd")),
            match positional {
                StartupCommand::Run(options) => {
                    assert!(options.maximized);
                    options.document
                }
                other => panic!("expected run options, got {other:?}"),
            }
        );
    }

    #[test]
    fn explicit_size_uses_windowed_mode_and_accepts_either_separator_case() {
        for size in ["1600x900", "1600X900"] {
            assert_eq!(
                StartupCommand::Run(StartupOptions {
                    document: None,
                    window_size: [1600.0, 900.0],
                    maximized: false,
                }),
                parse_startup_args(args(&["--size", size])).unwrap()
            );
        }
    }

    #[test]
    fn startup_options_reject_conflicts_and_invalid_dimensions() {
        for invalid in [
            vec!["--open"],
            vec!["--size"],
            vec!["--size", "800x600"],
            vec!["--size", "1600xnope"],
            vec!["--size", "1600x900", "--maximized"],
            vec!["--windowed", "--maximized"],
            vec!["a.octd", "b.octd"],
            vec!["--unknown"],
        ] {
            assert!(
                parse_startup_args(args(&invalid)).is_err(),
                "accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn help_and_version_are_standalone_commands() {
        assert_eq!(
            StartupCommand::Help,
            parse_startup_args(args(&["--help"])).unwrap()
        );
        assert_eq!(
            StartupCommand::Version,
            parse_startup_args(args(&["--version"])).unwrap()
        );
    }
}
