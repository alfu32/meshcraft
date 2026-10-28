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
    compile_feedback(vertex, fragment, &[])
}
pub(super) fn compile_feedback(
    vertex: &str,
    fragment: &str,
    varyings: &[&CStr],
) -> Result<Shader, String> {
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
        let feedback: unsafe extern "system" fn(u32, i32, *const *const std::ffi::c_char, u32) =
            function(c"glTransformFeedbackVaryings")?;
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
        if !varyings.is_empty() {
            let names: Vec<_> = varyings.iter().map(|v| v.as_ptr()).collect();
            feedback(id, names.len() as i32, names.as_ptr(), 0x8c8c);
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

/// Synchronous readback is confined to explicit bake commands on the graphics owner.
/// All functions are resolved before touching GL state; the buffer is size-checked.
pub(super) unsafe fn capture(
    vao: u32,
    vertices: i32,
    instances: i32,
) -> Result<Vec<[f32; 7]>, String> {
    unsafe {
        let count = (vertices as usize)
            .checked_mul(instances as usize)
            .ok_or("Bake size overflow")?;
        if count > 6_000_000 {
            return Err("Bake exceeds the two million triangle safety limit".into());
        }
        let gen_buffer: unsafe extern "system" fn(i32, *mut u32) = function(c"glGenBuffers")?;
        let bind: unsafe extern "system" fn(u32, u32) = function(c"glBindBuffer")?;
        let allocate: unsafe extern "system" fn(u32, isize, *const std::ffi::c_void, u32) =
            function(c"glBufferData")?;
        let bind_base: unsafe extern "system" fn(u32, u32, u32) = function(c"glBindBufferBase")?;
        let begin: unsafe extern "system" fn(u32) = function(c"glBeginTransformFeedback")?;
        let end: unsafe extern "system" fn() = function(c"glEndTransformFeedback")?;
        let enable: unsafe extern "system" fn(u32) = function(c"glEnable")?;
        let disable: unsafe extern "system" fn(u32) = function(c"glDisable")?;
        let draw: unsafe extern "system" fn(u32, i32, i32, i32) =
            function(c"glDrawArraysInstanced")?;
        let read: unsafe extern "system" fn(u32, isize, isize, *mut std::ffi::c_void) =
            function(c"glGetBufferSubData")?;
        let delete: unsafe extern "system" fn(i32, *const u32) = function(c"glDeleteBuffers")?;
        let error: unsafe extern "system" fn() -> u32 = function(c"glGetError")?;
        // Previous passes may leave unrelated driver errors; isolate this operation.
        for _ in 0..16 {
            if error() == 0 {
                break;
            }
        }
        let mut buffer = 0;
        let mut output = vec![[0f32; 7]; count];
        let size = std::mem::size_of_val(output.as_slice()) as isize;
        gen_buffer(1, &mut buffer);
        bind(0x8c8e, buffer);
        allocate(0x8c8e, size, std::ptr::null(), 0x88e1);
        if error() != 0 {
            bind(0x8c8e, 0);
            delete(1, &buffer);
            return Err("GPU could not allocate bake capture buffer".into());
        }
        bind_base(0x8c8e, 0, buffer);
        raylib::ffi::rlEnableVertexArray(vao);
        for i in 0..4 {
            raylib::ffi::rlSetVertexAttributeDivisor(i, instances);
        }
        enable(0x8c89);
        begin(0x0004);
        draw(0x0004, 0, vertices, instances);
        end();
        disable(0x8c89);
        read(0x8c8e, 0, size, output.as_mut_ptr().cast());
        let code = error();
        bind_base(0x8c8e, 0, 0);
        bind(0x8c8e, 0);
        delete(1, &buffer);
        raylib::ffi::rlDisableVertexArray();
        if code != 0 {
            return Err(format!("GPU bake capture failed (GL {code:#x})"));
        }
        Ok(output)
    }
}
