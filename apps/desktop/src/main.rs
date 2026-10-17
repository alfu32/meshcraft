use eframe::egui;
use meshcraft_desktop::MeshcraftApp;

fn main() -> eframe::Result {
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
        Box::new(|creation_context| Ok(Box::new(MeshcraftApp::new(creation_context)))),
    )
}
