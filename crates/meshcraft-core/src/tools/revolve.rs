//! Staged axis and numeric-angle input for Revolve.

use crate::geometry::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RevolveInput {
    pub axis_origin: Vec3,
    pub axis_direction: Vec3,
    pub angle_radians: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevolvePick {
    AxisOrigin,
    AxisDirection,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RevolveToolState {
    active: bool,
    axis_points: Vec<Vec3>,
    pointer: Option<Vec3>,
    numeric_input: String,
}

impl RevolveToolState {
    pub fn active(&self) -> bool {
        self.active
    }

    pub fn axis_points(&self) -> &[Vec3] {
        &self.axis_points
    }

    pub fn pointer(&self) -> Option<Vec3> {
        self.pointer
    }

    pub fn numeric_input(&self) -> &str {
        &self.numeric_input
    }

    pub fn activate(&mut self, active: bool) {
        self.active = active;
        self.axis_points.clear();
        self.pointer = None;
        self.numeric_input.clear();
    }

    pub fn set_pointer(&mut self, point: Option<Vec3>) {
        self.pointer = point.filter(|point| point.is_finite());
    }

    pub fn pick(&mut self, point: Vec3) -> Result<RevolvePick, &'static str> {
        if !self.active {
            return Err("Revolve tool is not active.");
        }
        if !point.is_finite() {
            return Err("Revolve axis points must be finite.");
        }
        if self.axis_points.len() >= 2 {
            return Err("Revolve axis is already defined; enter an angle or finish.");
        }
        self.axis_points.push(point);
        self.pointer = Some(point);
        self.numeric_input.clear();
        Ok(if self.axis_points.len() == 1 {
            RevolvePick::AxisOrigin
        } else {
            RevolvePick::AxisDirection
        })
    }

    pub fn add_numeric_text(&mut self, text: &str) {
        if self.axis_points.len() < 2 {
            return;
        }
        self.numeric_input.extend(text.chars().filter(|character| {
            character.is_ascii_digit() || matches!(character, '-' | '+' | '.' | 'e' | 'E')
        }));
    }

    pub fn clear_numeric_input(&mut self) {
        self.numeric_input.clear();
    }

    pub fn set_angle_degrees(&mut self, degrees: f64) -> Result<(), &'static str> {
        if !degrees.is_finite() || degrees == 0.0 || degrees.abs() > 360.0 {
            return Err("Revolve angle must be non-zero and no greater than 360 degrees.");
        }
        self.numeric_input = degrees.to_string();
        Ok(())
    }

    pub fn backspace(&mut self) -> bool {
        if self.numeric_input.pop().is_some() {
            return true;
        }
        if self.axis_points.pop().is_some() {
            self.pointer = self.axis_points.last().copied();
            return true;
        }
        false
    }

    pub fn angle_radians(&self) -> Result<f64, &'static str> {
        if self.numeric_input.is_empty() {
            return Ok(std::f64::consts::TAU);
        }
        let degrees = self
            .numeric_input
            .parse::<f64>()
            .map_err(|_| "Revolve angle input is not a valid number.")?;
        if !degrees.is_finite() || degrees == 0.0 || degrees.abs() > 360.0 {
            return Err("Revolve angle must be non-zero and no greater than 360 degrees.");
        }
        Ok(degrees.to_radians())
    }

    pub fn input(&self) -> Result<RevolveInput, &'static str> {
        if self.axis_points.len() != 2 {
            return Err("Pick the revolve axis origin and direction points first.");
        }
        Ok(RevolveInput {
            axis_origin: self.axis_points[0],
            axis_direction: self.axis_points[1] - self.axis_points[0],
            angle_radians: self.angle_radians()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revolve_state_collects_snapped_axis_points_and_typed_angles() {
        let mut state = RevolveToolState::default();
        state.activate(true);
        assert_eq!(Ok(RevolvePick::AxisOrigin), state.pick(Vec3::ZERO));
        assert_eq!(Ok(RevolvePick::AxisDirection), state.pick(Vec3::Y));
        state.add_numeric_text("-9a0.5");
        let input = state.input().unwrap();
        assert_eq!(Vec3::ZERO, input.axis_origin);
        assert_eq!(Vec3::Y, input.axis_direction);
        assert!((input.angle_radians.to_degrees() + 90.5).abs() < 1.0e-10);
        assert!(state.backspace());
        assert_eq!("-90.", state.numeric_input());
    }

    #[test]
    fn revolve_state_defaults_to_full_turn_and_rejects_bad_angles() {
        let mut state = RevolveToolState::default();
        state.activate(true);
        state.pick(Vec3::ZERO).unwrap();
        state.pick(Vec3::Y).unwrap();
        assert_eq!(std::f64::consts::TAU, state.input().unwrap().angle_radians);
        state.set_angle_degrees(360.0).unwrap();
        assert_eq!(std::f64::consts::TAU, state.input().unwrap().angle_radians);
        assert!(state.set_angle_degrees(0.0).is_err());
        assert!(state.set_angle_degrees(361.0).is_err());
        state.clear_numeric_input();
        state.add_numeric_text(".");
        assert!(state.input().is_err());
    }
}
