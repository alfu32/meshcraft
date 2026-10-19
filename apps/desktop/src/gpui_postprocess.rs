//! GPU-only FXAA: three quality levels, with an optional unfiltered path.
use super::*;
use raylib::prelude::{RaylibShader, RaylibShaderModeExt, Rectangle, Shader, Vector2};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Antialiasing {
    Off,
    Low,
    #[default]
    Medium,
    High,
}
impl Antialiasing {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }
    pub fn from_label(s: &str) -> Self {
        match s {
            "Off" => Self::Off,
            "Low" => Self::Low,
            "High" => Self::High,
            _ => Self::Medium,
        }
    }
    pub fn level(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
        }
    }
    pub fn from_level(level: u32) -> Self {
        match level {
            0 => Self::Off,
            1 => Self::Low,
            3 => Self::High,
            _ => Self::Medium,
        }
    }
    fn parameters(self) -> (f32, f32) {
        match self {
            Self::Off => (0., 1.),
            Self::Low => (2., 0.20),
            Self::Medium => (4., 0.125),
            Self::High => (8., 0.063),
        }
    }
}
pub(super) struct Resolve {
    pub(super) shader: Shader,
}
impl Resolve {
    pub fn new(rl: &mut raylib::RaylibHandle, thread: &raylib::RaylibThread) -> Self {
        Self {
            shader: rl.load_shader_from_memory(
                thread,
                None,
                Some(include_str!("../../../assets/shaders/antialias.fs")),
            ),
        }
    }
    pub fn draw(
        &mut self,
        rl: &mut raylib::RaylibHandle,
        thread: &raylib::RaylibThread,
        source: &RenderTexture2D,
        destination: &mut RenderTexture2D,
        quality: Antialiasing,
    ) {
        let (span, threshold) = quality.parameters();
        self.shader.set_shader_value(
            self.shader.get_shader_location("inverseSize"),
            Vector2::new(1. / source.width() as f32, 1. / source.height() as f32),
        );
        self.shader
            .set_shader_value(self.shader.get_shader_location("span"), span);
        self.shader
            .set_shader_value(self.shader.get_shader_location("threshold"), threshold);
        let mut output = rl.begin_texture_mode(thread, destination);
        output.clear_background(Color::BLANK);
        let mut pass = output.begin_shader_mode(&mut self.shader);
        pass.draw_texture_rec(
            source.texture(),
            Rectangle::new(0., 0., source.width() as f32, -(source.height() as f32)),
            Vector2::zero(),
            Color::WHITE,
        );
    }
}
