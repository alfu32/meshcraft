use meshcraft_io::project::read_project;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn main() {
    let inventory: Value = serde_json::from_slice(
        &fs::read("docs/rewrite/source-inventory.json").expect("read source inventory"),
    )
    .expect("parse source inventory");
    let source = PathBuf::from(
        inventory["source"]
            .as_str()
            .expect("source checkout path in inventory"),
    );
    let small_only = std::env::args().any(|argument| argument == "--small");
    let fixtures = inventory["fixtures"].as_array().expect("fixture array");
    let mut passed = 0;
    let mut failed = 0;
    let mut warning_count = 0;

    for fixture in fixtures {
        let path = fixture["path"].as_str().expect("fixture path");
        let bytes = fixture["bytes"].as_u64().expect("fixture size");
        if small_only && bytes > 200_000 {
            continue;
        }
        let data = match fs::read(source.join(path)) {
            Ok(data) => data,
            Err(error) => {
                eprintln!("FAIL {path}: read error: {error}");
                failed += 1;
                continue;
            }
        };
        match read_project(&data) {
            Ok(project) => {
                warning_count += project.warnings.len();
                for warning in &project.warnings {
                    println!("WARN {path}: {warning}");
                }
                println!(
                    "PASS {path}: {} definitions, {} occurrences",
                    project.document.definitions().count(),
                    project
                        .document
                        .occurrence_refs()
                        .map_or(0, |refs| refs.len())
                );
                passed += 1;
            }
            Err(error) => {
                eprintln!("FAIL {path}: {error}");
                failed += 1;
            }
        }
    }
    println!("SUMMARY passed={passed} failed={failed} warnings={warning_count}");
    if failed > 0 {
        std::process::exit(1);
    }
}
