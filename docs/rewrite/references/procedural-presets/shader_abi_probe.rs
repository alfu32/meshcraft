//! Compile-only ABI probe for the real shader compiler, without native GUI dependencies.
//! Install the desired rust-std target, then run rustc --edition=2024 --emit=metadata
//! --target aarch64-unknown-linux-gnu <this file> -o /tmp/shader-abi.rmeta.
//! The shim replaces raylib linkage only; it does not validate OpenGL execution.
extern crate self as raylib;

pub mod ffi {
    pub struct Shader {
        pub id: u32,
        pub locs: *mut i32,
    }
    #[allow(non_snake_case)]
    pub unsafe fn rlGetProcAddress(_: *const std::ffi::c_char) -> *mut std::ffi::c_void {
        std::ptr::null_mut()
    }
}

pub mod prelude {
    pub struct Shader;
    impl Shader {
        pub unsafe fn from_raw(_: crate::ffi::Shader) -> Self {
            Self
        }
    }
}

#[path = "../../../../apps/desktop/src/gpui_procedural_shader.rs"]
mod compiler;

fn main() {
    let _ = compiler::compile("", "");
}
