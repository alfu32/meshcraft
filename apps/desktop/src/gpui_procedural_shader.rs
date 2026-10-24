//! Compile on the graphics owner and retain real GLSL compile/link logs for the inspector.
use raylib::{ffi, prelude::Shader};
use std::ffi::{CStr, CString};
unsafe fn function<T: Copy>(name: &CStr) -> Result<T, String> {
    let address = unsafe { ffi::rlGetProcAddress(name.as_ptr()) };
    if address.is_null() {
        return Err(format!(
            "OpenGL 3.3 function unavailable: {}",
            name.to_string_lossy()
        ));
    }
    Ok(unsafe { std::mem::transmute_copy(&address) })
}
pub(super) fn compile(vertex: &str, fragment: &str) -> Result<Shader, String> {
    unsafe {
        let create: function_types::CreateShader = function(c"glCreateShader")?;
        let source: function_types::ShaderSource = function(c"glShaderSource")?;
        let compile: function_types::CompileShader = function(c"glCompileShader")?;
        let status: function_types::GetShaderiv = function(c"glGetShaderiv")?;
        let log: function_types::GetShaderInfoLog = function(c"glGetShaderInfoLog")?;
        let delete: function_types::DeleteShader = function(c"glDeleteShader")?;
        let create_program: function_types::CreateProgram = function(c"glCreateProgram")?;
        let attach: function_types::AttachShader = function(c"glAttachShader")?;
        let link: function_types::LinkProgram = function(c"glLinkProgram")?;
        let program_status: function_types::GetProgramiv = function(c"glGetProgramiv")?;
        let program_log: function_types::GetProgramInfoLog = function(c"glGetProgramInfoLog")?;
        let delete_program: function_types::DeleteProgram = function(c"glDeleteProgram")?;
        let mut shaders = vec![];
        for (kind, text, label) in [(0x8b31, vertex, "Vertex"), (0x8b30, fragment, "Fragment")] {
            let text = match CString::new(text) {
                Ok(t) => t,
                Err(_) => {
                    for id in shaders {
                        delete(id);
                    }
                    return Err("Shader source contains a NUL byte".into());
                }
            };
            let id = create(kind);
            source(id, 1, &text.as_ptr(), std::ptr::null());
            compile(id);
            let mut ok = 0;
            status(id, 0x8b81, &mut ok);
            if ok == 0 {
                let mut bytes = vec![0u8; 8192];
                let mut n = 0;
                log(id, bytes.len() as i32, &mut n, bytes.as_mut_ptr().cast());
                delete(id);
                for id in shaders {
                    delete(id);
                }
                return Err(format!(
                    "{label}: {}",
                    String::from_utf8_lossy(&bytes[..(n as usize).min(bytes.len())])
                ));
            }
            shaders.push(id);
        }
        let id = create_program();
        for shader in &shaders {
            attach(id, *shader);
        }
        link(id);
        let mut ok = 0;
        program_status(id, 0x8b82, &mut ok);
        for shader in shaders {
            delete(shader);
        }
        if ok == 0 {
            let mut bytes = vec![0u8; 8192];
            let mut n = 0;
            program_log(id, bytes.len() as i32, &mut n, bytes.as_mut_ptr().cast());
            delete_program(id);
            return Err(format!(
                "Link: {}",
                String::from_utf8_lossy(&bytes[..(n as usize).min(bytes.len())])
            ));
        }
        Ok(Shader::from_raw(ffi::Shader {
            id,
            locs: std::ptr::null_mut(),
        }))
    }
}
mod function_types {
    // C char is unsigned on Linux ARM64 and signed on other supported targets.
    use std::ffi::c_char;
    pub type CreateShader = unsafe extern "system" fn(u32) -> u32;
    pub type ShaderSource = unsafe extern "system" fn(u32, i32, *const *const c_char, *const i32);
    pub type CompileShader = unsafe extern "system" fn(u32);
    pub type GetShaderiv = unsafe extern "system" fn(u32, u32, *mut i32);
    pub type GetShaderInfoLog = unsafe extern "system" fn(u32, i32, *mut i32, *mut c_char);
    pub type DeleteShader = unsafe extern "system" fn(u32);
    pub type CreateProgram = unsafe extern "system" fn() -> u32;
    pub type AttachShader = unsafe extern "system" fn(u32, u32);
    pub type LinkProgram = unsafe extern "system" fn(u32);
    pub type GetProgramiv = unsafe extern "system" fn(u32, u32, *mut i32);
    pub type GetProgramInfoLog = unsafe extern "system" fn(u32, i32, *mut i32, *mut c_char);
    pub type DeleteProgram = unsafe extern "system" fn(u32);
}
