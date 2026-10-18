use meshcraft_io::project::{read_project, write_project};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let inventory: Value = serde_json::from_slice(
        &fs::read("docs/rewrite/source-inventory.json").expect("read source inventory"),
    )
    .expect("parse source inventory");
    let source = PathBuf::from(
        inventory["source"]
            .as_str()
            .expect("source checkout path in inventory"),
    );
    let small_only = arguments.iter().any(|argument| argument == "--small");
    let roundtrip = arguments.iter().any(|argument| argument == "--roundtrip");
    let only = arguments
        .windows(2)
        .find(|pair| pair[0] == "--only")
        .map(|pair| pair[1].as_str());
    let fixtures = inventory["fixtures"].as_array().expect("fixture array");
    let mut passed = 0;
    let mut failed = 0;
    let mut warning_count = 0;

    for fixture in fixtures {
        let path = fixture["path"].as_str().expect("fixture path");
        let bytes = fixture["bytes"].as_u64().expect("fixture size");
        if only.is_some_and(|only| only != path) {
            continue;
        }
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
                let definition_count = project.document.definitions().count();
                let occurrence_count = project
                    .document
                    .occurrence_refs()
                    .map_or(0, |refs| refs.len());
                if roundtrip {
                    drop(project.history);
                    let encoded = match write_project(&project.document, None) {
                        Ok(encoded) => encoded,
                        Err(error) => {
                            eprintln!("FAIL {path}: export error: {error}");
                            failed += 1;
                            continue;
                        }
                    };
                    match read_project(&encoded) {
                        Ok(reopened)
                            if reopened.document.definitions().count() == definition_count
                                && reopened
                                    .document
                                    .occurrence_refs()
                                    .is_ok_and(|refs| refs.len() == occurrence_count) =>
                        {
                            println!(
                                "PASS {path}: schema-17 round trip, {definition_count} definitions, {occurrence_count} occurrences"
                            );
                        }
                        Ok(_) => {
                            eprintln!(
                                "FAIL {path}: export/reopen changed definition or occurrence counts"
                            );
                            failed += 1;
                            continue;
                        }
                        Err(error) => {
                            eprintln!("FAIL {path}: reopened export is invalid: {error}");
                            failed += 1;
                            continue;
                        }
                    }
                } else {
                    println!(
                        "PASS {path}: {definition_count} definitions, {occurrence_count} occurrences"
                    );
                }
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
