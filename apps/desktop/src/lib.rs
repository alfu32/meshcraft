mod app;
mod assets;
mod viewport;

pub use app::MeshcraftApp;

#[cfg(target_arch = "wasm32")]
mod web;
