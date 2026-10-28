mod app;
mod assets;
#[cfg(not(target_arch = "wasm32"))]
mod native_snap;
#[cfg(not(target_arch = "wasm32"))]
mod native_spatial;
#[cfg(not(target_arch = "wasm32"))]
mod resource_stats;
mod viewport;

pub use app::MeshcraftApp;

#[cfg(not(target_arch = "wasm32"))]
pub mod gpui_app;

#[cfg(target_os = "linux")]
pub mod linux_graphics;

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub mod script_engine;

#[cfg(not(target_arch = "wasm32"))]
mod script_runtime;

#[cfg(not(target_arch = "wasm32"))]
mod mcp;
#[cfg(not(target_arch = "wasm32"))]
mod script_api;
