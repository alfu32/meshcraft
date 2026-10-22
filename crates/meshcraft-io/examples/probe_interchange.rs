//! Read-only importer probe: cargo run -p meshcraft-io --example probe_interchange -- file.obj ...
use meshcraft_io::mesh::{Options, read_file};
use std::{path::Path, sync::atomic::AtomicBool};
fn main() {
    let mut failed = false;
    for name in std::env::args().skip(1) {
        let started = std::time::Instant::now();
        match read_file(
            Path::new(&name),
            &Options::default(),
            &AtomicBool::new(false),
        ) {
            Ok(mesh) => println!(
                "{}",
                serde_json::json!({"path":name,"triangles":mesh.triangles.len(),"segments":mesh.segments.len(),"elapsed_ms":started.elapsed().as_millis(),"warnings":mesh.warnings})
            ),
            Err(error) => {
                failed = true;
                println!("{}", serde_json::json!({"path":name,"error":error}));
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}
