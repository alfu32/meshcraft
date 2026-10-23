//! Small append/backspace numeric entry shared by interactive tools.
use crate::{document::ModelSettings, geometry::Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointInput {
    Scalar {
        value: f64,
        explicit_length: bool,
        angle: bool,
    },
    Offset {
        value: Vec3,
        dimensions: usize,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatMode {
    Multiply,
    Divide,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Repetition {
    pub mode: RepeatMode,
    pub count: usize,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entry {
    pub point: Option<PointInput>,
    pub repetition: Option<Repetition>,
}

pub fn starts_entry(c: char) -> bool {
    c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'x' | 'X' | '/')
}
pub fn accepted(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '-' | '+' | ',' | '/' | ' ' | '°' | '\'')
}

fn meters(unit: &str) -> Option<f64> {
    Some(match unit {
        "nm" => 1e-9,
        "um" | "µm" | "μm" | "micron" => 1e-6,
        "mm" | "millimeter" | "millimeters" => 0.001,
        "cm" | "centimeter" | "centimeters" => 0.01,
        "m" | "meter" | "meters" | "metre" | "metres" => 1.,
        "km" | "kilometer" | "kilometers" => 1000.,
        "in" | "inch" | "inches" => 0.0254,
        "ft" | "foot" | "feet" | "'" => 0.3048,
        "yd" | "yard" | "yards" => 0.9144,
        _ => return None,
    })
}
fn number_and_unit(text: &str) -> Result<(f64, String), String> {
    let text = text.trim();
    // Longest finite numeric prefix also handles scientific notation without treating 'e' as a unit.
    for end in text
        .char_indices()
        .map(|(i, _)| i)
        .chain([text.len()])
        .rev()
    {
        if let Ok(value) = text[..end].trim().parse::<f64>() {
            if !value.is_finite() {
                return Err("Enter a finite number".into());
            }
            return Ok((value, text[end..].trim().to_lowercase()));
        }
    }
    Err("Enter a number, relative coordinates, xN or /N".into())
}
fn scalar(
    text: &str,
    inherited_unit: &str,
    settings: &ModelSettings,
) -> Result<PointInput, String> {
    let (mut value, own_unit) = number_and_unit(text)?;
    let unit = if own_unit.is_empty() {
        inherited_unit
    } else {
        &own_unit
    };
    let angle = matches!(
        unit,
        "deg" | "degree" | "degrees" | "°" | "rad" | "radian" | "radians"
    );
    if angle {
        if unit.starts_with("rad") {
            value = value.to_degrees();
        }
    } else if !unit.is_empty() {
        let configured = settings.unit_name.trim().to_lowercase();
        let ratio = if unit == configured {
            1.
        } else {
            meters(unit)
                .zip(meters(&configured))
                .map(|(a, b)| a / b)
                .ok_or_else(|| {
                    format!(
                        "Cannot convert {unit} to {}; configure the model unit first",
                        settings.unit_name
                    )
                })?
        };
        if !settings.unit_size.is_finite() || settings.unit_size <= 0. {
            return Err("Model unit size must be positive".into());
        }
        value = value * ratio / settings.unit_size;
    }
    if !value.is_finite() {
        return Err("Numeric input is out of range".into());
    }
    Ok(PointInput::Scalar {
        value,
        explicit_length: !unit.is_empty() && !angle,
        angle,
    })
}
pub fn parse(text: &str, settings: &ModelSettings) -> Result<Entry, String> {
    let text = text.trim();
    if text.is_empty() || text.len() > 256 {
        return Err("Enter at most 256 characters".into());
    }
    let mut point_text = text;
    let mut repetition = None;
    if let Some((index, marker)) = text.char_indices().rev().find(|(i, c)| {
        *c == '/'
            || (matches!(c, 'x' | 'X')
                && (*i == 0
                    || text[..*i]
                        .chars()
                        .next_back()
                        .is_some_and(|p| p.is_ascii_digit() || p.is_whitespace())
                    || text[*i + 1..]
                        .trim()
                        .chars()
                        .next()
                        .is_some_and(|n| n.is_ascii_digit() || matches!(n, '+' | '-'))))
    }) {
        let count_text = text[index + 1..].trim();
        let count = count_text
            .parse::<usize>()
            .map_err(|_| "Copy count must be a positive whole number")?;
        if count == 0 || count > crate::operations::copy::MAX_LINEAR_ARRAY_COPIES {
            return Err("Copy count must be between 1 and 10000".into());
        }
        repetition = Some(Repetition {
            mode: if marker == '/' {
                RepeatMode::Divide
            } else {
                RepeatMode::Multiply
            },
            count,
        });
        point_text = text[..index].trim();
    }
    let point = if point_text.is_empty() {
        None
    } else {
        let components = point_text.split(',').collect::<Vec<_>>();
        Some(if components.len() == 1 {
            scalar(point_text, "", settings)?
        } else {
            if !(2..=3).contains(&components.len()) {
                return Err("Relative coordinates need two or three components".into());
            }
            let suffixes = components
                .iter()
                .map(|s| number_and_unit(s).map(|(_, u)| u))
                .collect::<Result<Vec<_>, _>>()?;
            let shared = if suffixes[..suffixes.len() - 1].iter().all(String::is_empty) {
                suffixes.last().unwrap().as_str()
            } else {
                ""
            };
            let mut values = [0.; 3];
            for (index, component) in components.iter().enumerate() {
                match scalar(component, shared, settings)? {
                    PointInput::Scalar {
                        value,
                        angle: false,
                        ..
                    } => values[index] = value,
                    _ => return Err("Coordinate components need lengths, not angles".into()),
                }
            }
            PointInput::Offset {
                value: Vec3::new(values[0], values[1], values[2]),
                dimensions: components.len(),
            }
        })
    };
    Ok(Entry { point, repetition })
}
impl PointInput {
    pub fn resolve(
        self,
        origin: Vec3,
        pointer: Option<Vec3>,
        axes: [Vec3; 3],
    ) -> Result<Vec3, String> {
        let point = match self {
            Self::Scalar {
                value,
                angle: false,
                ..
            } => {
                origin
                    + (pointer.ok_or("Move the pointer to choose a direction")? - origin)
                        .try_normalized(1e-12)
                        .ok_or("Choose a nonzero direction")?
                        * value
            }
            Self::Scalar { angle: true, .. } => {
                return Err("Angles are accepted at the final rotation or Revolve step".into());
            }
            Self::Offset {
                value,
                dimensions: 2,
            } => origin + axes[0] * value.x + axes[1] * value.y,
            Self::Offset { value, .. } => origin + value,
        };
        point
            .is_finite()
            .then_some(point)
            .ok_or_else(|| "Point is out of range".into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn units_coordinates_counts_and_invalid_inputs_are_explicit() {
        let settings = ModelSettings {
            unit_name: "cm".into(),
            unit_size: 30.,
            grid_size: 0.5,
            ..Default::default()
        };
        let resolve = |s| {
            parse(s, &settings)
                .unwrap()
                .point
                .unwrap()
                .resolve(Vec3::ZERO, Some(Vec3::X), [Vec3::X, Vec3::Z, Vec3::Y])
                .unwrap()
        };
        assert_eq!(Vec3::X * 2., resolve("2"));
        assert_eq!(Vec3::X * 0.5, resolve("15cm"));
        assert!((resolve("1m").x - 100. / 30.).abs() < 1e-12);
        assert_eq!(Vec3::new(1., 0., 2.), resolve("30,60 cm"));
        assert_eq!(Vec3::new(1., 2., 3.), resolve("1,2,3"));
        assert_eq!(
            12,
            parse("2m x12", &settings)
                .unwrap()
                .repetition
                .unwrap()
                .count
        );
        assert_eq!(
            RepeatMode::Divide,
            parse("/12", &settings).unwrap().repetition.unwrap().mode
        );
        for bad in [
            "/0", "x-1", "x1.5", "x10001", "1,2,3,4", "1,,3", "NaN", "1e999", "1unknown", "1deg,2",
        ] {
            assert!(parse(bad, &settings).is_err(), "{bad}");
        }
    }
}
