mod app;
mod assets;
mod viewport;

pub use app::MeshcraftApp;

#[cfg(not(target_arch = "wasm32"))]
pub mod bevy_app;

#[cfg(target_arch = "wasm32")]
mod web;
