use std::{env, fs, path::PathBuf};

fn main() {
    let source = PathBuf::from("../../assets/tool-icons");
    println!(
        "cargo:rerun-if-changed={}",
        source.join("icons.svg").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        source.join("icons.mapping.csv").display()
    );
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_data(&fs::read(source.join("icons.svg")).unwrap(), &options)
        .expect("parse k3d icon sheet");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let icon = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("../../packaging/windows/meshcraft.ico");
        println!("cargo:rerun-if-changed={}", icon.display());
        let resource = output.join("meshcraft.rc");
        fs::write(
            &resource,
            format!("1 ICON \"{}\"\n", icon.to_string_lossy().replace('\\', "/")),
        )
        .expect("write application icon resource");
        embed_resource::compile_for(&resource, ["meshcraft"], embed_resource::NONE)
            .manifest_required()
            .expect("compile Windows application icon");
    }
    let width = tree.size().width().round() as u32;
    let height = tree.size().height().round() as u32;
    assert_eq!(width % 32, 0);
    assert_eq!(height % 32, 0);
    fs::write(output.join("icon_dimensions.rs"), format!("pub const ICON_ATLAS_WIDTH: u32 = {width};\npub const ICON_ATLAS_HEIGHT: u32 = {height};\n")).unwrap();
    for tile in [32, 48, 64] {
        let mut pixmap =
            resvg::tiny_skia::Pixmap::new(width * tile / 32, height * tile / 32).unwrap();
        let scale = tile as f32 / 32.0;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        pixmap
            .save_png(output.join(format!("icons.{tile}.png")))
            .expect("write generated icon atlas");
    }
}
