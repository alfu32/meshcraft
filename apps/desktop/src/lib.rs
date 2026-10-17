mod app;
mod assets;

pub use app::MeshcraftApp;

#[cfg(target_arch = "wasm32")]
mod web;
