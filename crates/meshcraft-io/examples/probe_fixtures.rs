use meshcraft_core::legacy::export_current_value;
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
                    let expected = match export_current_value(&project.document) {
                        Ok(snapshot) => snapshot,
                        Err(error) => {
                            eprintln!("FAIL {path}: canonical export error: {error}");
                            failed += 1;
                            continue;
                        }
                    };
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
                        Ok(reopened) => {
                            let actual = match export_current_value(&reopened.document) {
                                Ok(snapshot) => snapshot,
                                Err(error) => {
                                    eprintln!(
                                        "FAIL {path}: reopened canonical export error: {error}"
                                    );
                                    failed += 1;
                                    continue;
                                }
                            };
                            let reopened_definition_count = reopened.document.definitions().count();
                            let reopened_occurrence_count = reopened
                                .document
                                .occurrence_refs()
                                .map_or(0, |refs| refs.len());
                            let difference = first_difference(&expected, &actual, "");
                            if difference.is_some()
                                || reopened_definition_count != definition_count
                                || reopened_occurrence_count != occurrence_count
                            {
                                let difference = difference
                                    .map(|(path, expected, actual)| {
                                        format!(
                                            "{path} (expected {}, actual {})",
                                            expected.map_or_else(
                                                || "<missing>".to_owned(),
                                                |value| value.to_string()
                                            ),
                                            actual.map_or_else(
                                                || "<missing>".to_owned(),
                                                |value| value.to_string()
                                            )
                                        )
                                    })
                                    .unwrap_or_else(|| "entity counts".to_owned());
                                eprintln!(
                                    "FAIL {path}: export/reopen differs at {difference} ({definition_count}/{reopened_definition_count} definitions, {occurrence_count}/{reopened_occurrence_count} occurrences)"
                                );
                                failed += 1;
                                continue;
                            }
                            println!(
                                "PASS {path}: canonical schema-17 round trip, {definition_count} definitions, {occurrence_count} occurrences"
                            );
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

fn first_difference(
    left: &Value,
    right: &Value,
    path: &str,
) -> Option<(String, Option<Value>, Option<Value>)> {
    if left == right || canonical_float_equal(left, right) {
        return None;
    }
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => {
            let fields = left
                .keys()
                .chain(right.keys())
                .collect::<std::collections::BTreeSet<_>>();
            for field in fields {
                let escaped = field.replace('~', "~0").replace('/', "~1");
                let field_path = format!("{path}/{escaped}");
                match (left.get(field), right.get(field)) {
                    (Some(left), Some(right)) => {
                        if let Some(difference) = first_difference(left, right, &field_path) {
                            return Some(difference);
                        }
                    }
                    (left, right) => {
                        return Some((field_path, left.cloned(), right.cloned()));
                    }
                }
            }
            None
        }
        (Value::Array(left), Value::Array(right)) => {
            if left.len() != right.len() {
                return Some((
                    format!("{path}/length"),
                    Some(Value::from(left.len())),
                    Some(Value::from(right.len())),
                ));
            }
            left.iter()
                .zip(right)
                .enumerate()
                .find_map(|(index, (left, right))| {
                    first_difference(left, right, &format!("{path}/{index}"))
                })
        }
        _ => Some((
            if path.is_empty() {
                "/".to_owned()
            } else {
                path.to_owned()
            },
            Some(left.clone()),
            Some(right.clone()),
        )),
    }
}

fn canonical_float_equal(left: &Value, right: &Value) -> bool {
    let (Value::Number(left), Value::Number(right)) = (left, right) else {
        return false;
    };
    if !left.is_f64() || !right.is_f64() {
        return false;
    }
    let (Some(left), Some(right)) = (left.as_f64(), right.as_f64()) else {
        return false;
    };
    if !left.is_finite() || !right.is_finite() {
        return left == right;
    }
    let ordered_bits = |value: f64| {
        let bits = value.to_bits();
        if bits & (1 << 63) == 0 {
            bits | (1 << 63)
        } else {
            !bits
        }
    };
    // Affine decomposition/recomposition can shift a component by a few representable steps.
    ordered_bits(left).abs_diff(ordered_bits(right)) <= 8
}
