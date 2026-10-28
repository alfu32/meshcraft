//! GL 3.3 transform-feedback branch states, reused by tube/leaf vertex instances.
//! There is no readback or CPU branch/curve data on the rendering path.
use super::*;
use shader_compiler::function;

pub(super) struct StatePass {
    shader: Shader,
    buffer: u32,
    texture: u32,
    capacity: usize,
}
impl StatePass {
    #[cfg(test)]
    pub(super) fn storage(&self) -> (u32, usize) {
        (self.buffer, self.capacity)
    }
    pub(super) fn new(program: &Program) -> Result<Self, String> {
        let (vertex, _) = shader_sources(program);
        let vertex = vertex.replace("void main() {", "void unusedMain() {")
            + r#"
out vec4 state0;out vec4 state1;out vec4 state2;out vec4 state3;
void main(){vec3 a,c,b,frame;float radius,presence;uint key;
branchEvaluate(gl_VertexID,a,c,b,frame,radius,presence,key);
state0=vec4(a,radius);state1=vec4(c,presence);
state2=vec4(b,float(key&0x00ffffffu));state3=vec4(frame,0);
gl_Position=vec4(0);}
"#;
        let shader = shader_compiler::compile_feedback(
            &vertex,
            "#version 330\nout vec4 finalColor;void main(){finalColor=vec4(1);}",
            &[c"state0", c"state1", c"state2", c"state3"],
        )?;
        unsafe {
            let gen_buffers: unsafe extern "system" fn(i32, *mut u32) = function(c"glGenBuffers")?;
            let gen_textures: unsafe extern "system" fn(i32, *mut u32) =
                function(c"glGenTextures")?;
            let mut buffer = 0;
            let mut texture = 0;
            gen_buffers(1, &mut buffer);
            gen_textures(1, &mut texture);
            Ok(Self {
                shader,
                buffer,
                texture,
                capacity: 0,
            })
        }
    }
    pub(super) fn run(
        &mut self,
        program: &Program,
        values: &BTreeMap<String, Value>,
        time: f32,
        pixels: f32,
        vao: u32,
        count: usize,
    ) -> Result<(), String> {
        if count == 0 || count > 16_384 {
            return Err("Invalid branching state count".into());
        }
        bind_parameters(&mut self.shader, program, values);
        self.shader
            .set_shader_value(self.shader.get_shader_location("time"), time);
        self.shader
            .set_shader_value(self.shader.get_shader_location("branchPixelScale"), pixels);
        unsafe {
            let bind: unsafe extern "system" fn(u32, u32) = function(c"glBindBuffer")?;
            let allocate: unsafe extern "system" fn(u32, isize, *const std::ffi::c_void, u32) =
                function(c"glBufferData")?;
            let bind_base: unsafe extern "system" fn(u32, u32, u32) =
                function(c"glBindBufferBase")?;
            let bind_texture: unsafe extern "system" fn(u32, u32) = function(c"glBindTexture")?;
            let tex_buffer: unsafe extern "system" fn(u32, u32, u32) = function(c"glTexBuffer")?;
            let begin: unsafe extern "system" fn(u32) = function(c"glBeginTransformFeedback")?;
            let end: unsafe extern "system" fn() = function(c"glEndTransformFeedback")?;
            let enable: unsafe extern "system" fn(u32) = function(c"glEnable")?;
            let disable: unsafe extern "system" fn(u32) = function(c"glDisable")?;
            let draw: unsafe extern "system" fn(u32, i32, i32) = function(c"glDrawArrays")?;
            raylib::ffi::rlActiveTextureSlot(11);
            bind_texture(0x8c2a, 0); // GL_TEXTURE_BUFFER: not sampled while being written
            bind(0x8c8e, self.buffer);
            if count > self.capacity {
                self.capacity = count.next_power_of_two();
                allocate(
                    0x8c8e,
                    (self.capacity * 64) as isize,
                    std::ptr::null(),
                    0x88e8,
                );
            }
            bind_base(0x8c8e, 0, self.buffer);
            raylib::ffi::rlEnableShader(self.shader.id);
            raylib::ffi::rlEnableVertexArray(vao);
            enable(0x8c89); // GL_RASTERIZER_DISCARD
            begin(0x0000); // GL_POINTS, one shader invocation per branch candidate
            draw(0x0000, 0, count as i32);
            end();
            disable(0x8c89);
            bind_base(0x8c8e, 0, 0);
            bind(0x8c8e, 0);
            bind_texture(0x8c2a, self.texture);
            tex_buffer(0x8c2a, 0x8814, self.buffer); // RGBA32F: four texels per branch
            raylib::ffi::rlDisableVertexArray();
            raylib::ffi::rlDisableShader();
            raylib::ffi::rlActiveTextureSlot(0);
        }
        Ok(())
    }
}
impl Drop for StatePass {
    fn drop(&mut self) {
        unsafe {
            if let Ok(delete) =
                function::<unsafe extern "system" fn(i32, *const u32)>(c"glDeleteBuffers")
            {
                delete(1, &self.buffer);
            }
            if let Ok(delete) =
                function::<unsafe extern "system" fn(i32, *const u32)>(c"glDeleteTextures")
            {
                delete(1, &self.texture);
            }
        }
    }
}
