mod app;
mod assets;
#[cfg(not(target_arch = "wasm32"))]
mod native_spatial;
mod viewport;

pub use app::MeshcraftApp;

#[cfg(not(target_arch = "wasm32"))]
pub mod gpui_app;

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub mod script_engine;

#[cfg(not(target_arch = "wasm32"))]
mod script_runtime;
